//! KTCS Calendar Server
//!
//! Aggregation service that batches timestamp requests and commits them to Kaspa.

use axum::{
    extract::{Path, Request, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tower::ServiceBuilder;
use tower_governor::{governor::GovernorConfigBuilder, key_extractor::{KeyExtractor, PeerIpKeyExtractor}, GovernorError, GovernorLayer};
use tower_http::{
    cors::CorsLayer,
    limit::RequestBodyLimitLayer,
};
use tracing::{info, warn, error};
use tracing_subscriber::EnvFilter;
use subtle::ConstantTimeEq;

mod routes;
mod services;

use ktcs_core::{
    serialize_proof, Attestation, BatchMode, KaspaAttestation, KtcsProof, MerkleTree,
    PendingAttestation,
};
use routes::websocket::{
    ws_handler, BatchedEvent, ConfirmationEvent, HasAllowedOrigins, HasDatabase, HasProxyConfig,
    WsAppState, WsState,
};
use services::batch_manager::{BatchManager, PendingStamp};
use services::database::{Database, DbStampRecord, DbStampStatus};
use services::kaspa_service::{KaspaService, KaspaServiceConfig, KaspaServiceError};
use services::recycle_service::RecycleService;

/// Application state shared across handlers
#[derive(Clone)]
struct AppState {
    /// Thread-safe batch manager (has internal synchronization)
    batch_manager: Arc<BatchManager>,
    /// SQLite database for persistent stamp storage
    database: Arc<Database>,
    kaspa_service: Arc<KaspaService>,
    ws_state: Arc<WsState>,
    /// Public URL for calendar (used in pending attestations)
    public_url: String,
    /// API key for authentication (None if not required)
    api_key: Option<String>,
    /// Allowed CORS origins (for WebSocket validation)
    cors_origins: Vec<String>,
    /// Whether to trust proxy headers (X-Real-IP / rightmost XFF) for the client IP
    trust_proxy: bool,
}

impl WsAppState for AppState {
    fn ws_state(&self) -> &Arc<WsState> {
        &self.ws_state
    }
}

impl HasAllowedOrigins for AppState {
    fn allowed_origins(&self) -> &[String] {
        &self.cors_origins
    }
}

impl HasProxyConfig for AppState {
    fn trust_proxy(&self) -> bool {
        self.trust_proxy
    }
}

impl HasDatabase for AppState {
    fn database(&self) -> Arc<Database> {
        self.database.clone()
    }
}

/// API request to submit a stamp
#[derive(Debug, Deserialize)]
struct StampRequest {
    /// Hex-encoded digest (SHA256 hash)
    digest: String,
    /// Hash algorithm used (currently only sha256)
    #[serde(default = "default_algorithm")]
    algorithm: String,
    /// Batching mode
    #[serde(default)]
    batch_mode: BatchMode,
}

fn default_algorithm() -> String {
    "sha256".to_string()
}

/// API response for stamp submission
/// Spec reference: Section 5.1.1 and 5.1.2
#[derive(Debug, Serialize)]
struct StampResponse {
    id: String,
    status: String,
    submitted_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    estimated_confirmation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pending_proof: Option<String>,

    // Confirmed fields (flat per spec - not nested in "attestation")
    #[serde(skip_serializing_if = "Option::is_none")]
    confirmed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    daa_score: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    blue_score: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    block_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tx_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    proof: Option<String>, // Spec uses "proof" not "confirmed_proof"
    #[serde(skip_serializing_if = "Option::is_none")]
    thermodynamic_weight: Option<ThermodynamicWeightResponse>,
    /// Parent block hashes (for confirmed stamps)
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_hashes: Option<Vec<String>>,
}

/// Thermodynamic weight information per spec Section 5.1.2
#[derive(Debug, Serialize, Clone)]
struct ThermodynamicWeightResponse {
    blue_work_at_confirmation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_blue_work: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    accumulated_since: Option<String>,
}

/// API response for verification
/// Spec reference: Section 5.1.3
#[derive(Debug, Serialize)]
struct VerifyResponse {
    valid: bool,
    digest: String,
    attestations: Vec<AttestationInfoResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_confirmations: Option<CurrentConfirmationsResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Attestation info in verification response
#[derive(Debug, Serialize)]
struct AttestationInfoResponse {
    #[serde(rename = "type")]
    attestation_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    daa_score: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    blue_score: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    block_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thermodynamic_weight: Option<String>,
}

/// Current confirmations info per spec Section 5.1.3
#[derive(Debug, Serialize)]
struct CurrentConfirmationsResponse {
    blocks_since: u64,
    blue_work_accumulated: String,
    time_elapsed_seconds: u64,
}

/// Health check response
#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
    version: String,
    pending_stamps: usize,
}

/// Server security configuration
#[derive(Debug, Clone)]
struct ServerConfig {
    /// Bind address
    bind_address: String,
    /// Public URL for calendar (used in pending attestations)
    public_url: String,
    /// Allowed CORS origins (comma-separated, or "*" for all)
    cors_origins: Vec<String>,
    /// Rate limit: requests per second per IP
    rate_limit_per_second: u32,
    /// Rate limit: burst size
    rate_limit_burst: u32,
    /// Maximum request body size in bytes
    max_body_size: usize,
    /// API key for authentication (optional)
    api_key: Option<String>,
    /// Require API key for write operations
    require_api_key: bool,
    /// Trust X-Forwarded-For header for client IP (use when behind reverse proxy)
    trust_proxy: bool,
}

/// Extract the client IP from proxy-set headers (only trusted when TRUST_PROXY=true).
///
/// SECURITY: nginx sets `X-Real-IP` from `$remote_addr` (the TCP peer it actually
/// observed) and *appends* that peer to `X-Forwarded-For` via
/// `$proxy_add_x_forwarded_for`. A remote client can put anything in the request's
/// initial `X-Forwarded-For`, so the LEFTMOST entry is attacker-controlled while the
/// RIGHTMOST entry is the address the trusted proxy saw. The previous code trusted
/// the leftmost entry, letting an attacker spoof arbitrary IPs to bypass the
/// per-IP rate limiter (and balloon governor state with fake keys). We now prefer
/// `X-Real-IP`, then fall back to the RIGHTMOST `X-Forwarded-For` entry.
pub(crate) fn forwarded_client_ip(headers: &HeaderMap) -> Option<IpAddr> {
    if let Some(real_ip) = headers.get("x-real-ip") {
        if let Ok(s) = real_ip.to_str() {
            if let Ok(ip) = s.trim().parse::<IpAddr>() {
                return Some(ip);
            }
        }
    }
    if let Some(xff) = headers.get("x-forwarded-for") {
        if let Ok(s) = xff.to_str() {
            // Rightmost entry = the address the trusted proxy actually connected from.
            if let Some(last) = s.split(',').next_back() {
                if let Ok(ip) = last.trim().parse::<IpAddr>() {
                    return Some(ip);
                }
            }
        }
    }
    None
}

/// Proxy-aware IP key extractor for rate limiting.
/// When trust_proxy is true, derives the client IP from proxy headers
/// (X-Real-IP / rightmost X-Forwarded-For); otherwise uses the TCP peer.
#[derive(Clone)]
struct ProxyAwareIpExtractor {
    trust_proxy: bool,
}

impl KeyExtractor for ProxyAwareIpExtractor {
    type Key = IpAddr;

    fn extract<T>(&self, req: &axum::http::Request<T>) -> Result<Self::Key, GovernorError> {
        if self.trust_proxy {
            if let Some(ip) = forwarded_client_ip(req.headers()) {
                return Ok(ip);
            }
        }
        // Fall back to peer IP
        PeerIpKeyExtractor.extract(req)
    }
}

/// Configuration validation error
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Invalid configuration: {0}")]
    Invalid(String),
}

impl ServerConfig {
    /// Validate configuration values
    fn validate(&self) -> Result<(), ConfigError> {
        // Validate bind address
        if self.bind_address.parse::<SocketAddr>().is_err() {
            return Err(ConfigError::Invalid(
                format!("Invalid bind address '{}': must be in format IP:PORT", self.bind_address)
            ));
        }

        // Validate CORS origins
        for origin in &self.cors_origins {
            if origin != "*" {
                // Basic URL validation - must be http:// or https://
                if !origin.starts_with("http://") && !origin.starts_with("https://") {
                    return Err(ConfigError::Invalid(
                        format!("Invalid CORS origin '{}': must start with http:// or https://", origin)
                    ));
                }
            }
        }

        // Validate rate limits
        if self.rate_limit_per_second == 0 {
            return Err(ConfigError::Invalid(
                "Rate limit per second must be greater than 0".to_string()
            ));
        }
        if self.rate_limit_per_second > 10_000 {
            return Err(ConfigError::Invalid(
                format!("Rate limit {} per second seems too high (maximum 10000)", self.rate_limit_per_second)
            ));
        }
        if self.rate_limit_burst == 0 {
            return Err(ConfigError::Invalid(
                "Rate limit burst must be greater than 0".to_string()
            ));
        }
        if self.rate_limit_burst > 100_000 {
            return Err(ConfigError::Invalid(
                format!("Rate limit burst {} seems too high (maximum 100000)", self.rate_limit_burst)
            ));
        }

        // Validate max body size (reasonable range: 1KB to 100MB)
        if self.max_body_size < 1024 {
            return Err(ConfigError::Invalid(
                format!("Max body size {} is too small (minimum 1024 bytes)", self.max_body_size)
            ));
        }
        if self.max_body_size > 100 * 1024 * 1024 {
            return Err(ConfigError::Invalid(
                format!("Max body size {} is too large (maximum 100MB)", self.max_body_size)
            ));
        }

        // Validate API key if required
        if self.require_api_key && self.api_key.is_none() {
            return Err(ConfigError::Invalid(
                "REQUIRE_API_KEY is true but no API_KEY is set".to_string()
            ));
        }
        if let Some(ref key) = self.api_key {
            if key.len() < 16 {
                return Err(ConfigError::Invalid(
                    "API key is too short (minimum 16 characters for security)".to_string()
                ));
            }
        }

        // SECURITY: Enforce API key in production environment
        if !self.require_api_key {
            let env = std::env::var("KTCS_ENVIRONMENT")
                .unwrap_or_else(|_| "development".to_string())
                .to_lowercase();

            if env == "production" {
                return Err(ConfigError::Invalid(
                    "SECURITY ERROR: API key authentication must be enabled in production. \
                     Set REQUIRE_API_KEY=true and provide API_KEY.".to_string()
                ));
            } else {
                warn!(
                    "API key authentication is DISABLED. \
                     Set REQUIRE_API_KEY=true for production deployments."
                );
            }
        }

        // Warn about insecure configurations
        if self.cors_origins.len() == 1 && self.cors_origins[0] == "*" {
            warn!("CORS allows all origins - this is insecure for production");
        }

        Ok(())
    }

    fn from_env() -> Self {
        // Parse CORS origins
        let cors_origins_str = std::env::var("CORS_ORIGINS")
            .unwrap_or_else(|_| "*".to_string());
        let cors_origins: Vec<String> = if cors_origins_str == "*" {
            vec!["*".to_string()]
        } else {
            cors_origins_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        };

        let bind_address = std::env::var("BIND_ADDRESS")
            .unwrap_or_else(|_| "0.0.0.0:3001".to_string());

        // Public URL defaults to http://{bind_address}
        let public_url = std::env::var("KTCS_PUBLIC_URL")
            .unwrap_or_else(|_| format!("http://{}", bind_address));

        Self {
            bind_address,
            public_url,
            cors_origins,
            rate_limit_per_second: std::env::var("RATE_LIMIT_PER_SECOND")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(100), // 100 requests per second default (generous for dev)
            rate_limit_burst: std::env::var("RATE_LIMIT_BURST")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(200), // Burst of 200 requests
            max_body_size: std::env::var("MAX_BODY_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10 * 1024 * 1024), // 10 MB default
            api_key: std::env::var("API_KEY").ok().filter(|s| !s.is_empty()),
            require_api_key: std::env::var("REQUIRE_API_KEY")
                .ok()
                .map(|s| s == "true" || s == "1")
                .unwrap_or(false),
            trust_proxy: std::env::var("TRUST_PROXY")
                .ok()
                .map(|s| s == "true" || s == "1")
                .unwrap_or(false),
        }
    }
}

/// Build CORS layer from configuration
fn build_cors_layer(origins: &[String]) -> CorsLayer {
    if origins.len() == 1 && origins[0] == "*" {
        warn!("CORS configured to allow all origins - consider restricting in production");
        CorsLayer::permissive()
    } else {
        info!("CORS configured for origins: {:?}", origins);
        let allowed_origins: Vec<HeaderValue> = origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect();

        CorsLayer::new()
            .allow_origin(allowed_origins)
            .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
            .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION, header::ACCEPT])
            // Cache preflight requests for 1 hour to reduce OPTIONS requests
            .max_age(std::time::Duration::from_secs(3600))
    }
}

/// API key authentication middleware
///
/// Validates the X-API-Key header against the configured API key.
/// If no API key is configured (api_key is None), all requests are allowed.
async fn api_key_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // If no API key is configured, allow all requests
    let Some(ref expected_key) = state.api_key else {
        return Ok(next.run(request).await);
    };

    // Maximum API key length for fixed-size buffer comparison
    const MAX_API_KEY_LEN: usize = 128;

    // Check X-API-Key header
    match request.headers().get("X-API-Key") {
        Some(provided_key) => {
            let provided = provided_key.to_str().unwrap_or("");
            // Use constant-time comparison to prevent timing attacks
            let provided_bytes = provided.as_bytes();
            let expected_bytes = expected_key.as_bytes();

            // Use fixed-size buffers to avoid leaking timing info through dynamic allocation
            let provided_len = provided_bytes.len();
            let expected_len = expected_bytes.len();

            let mut provided_padded = [0u8; MAX_API_KEY_LEN];
            let mut expected_padded = [0u8; MAX_API_KEY_LEN];

            let provided_copy = provided_len.min(MAX_API_KEY_LEN);
            let expected_copy = expected_len.min(MAX_API_KEY_LEN);
            provided_padded[..provided_copy].copy_from_slice(&provided_bytes[..provided_copy]);
            expected_padded[..expected_copy].copy_from_slice(&expected_bytes[..expected_copy]);

            // Both length mismatch and content mismatch return false in constant time
            let lengths_equal = provided_len.ct_eq(&expected_len);
            let bytes_equal = provided_padded.ct_eq(&expected_padded);
            let keys_match = bool::from(lengths_equal & bytes_equal);

            if keys_match {
                Ok(next.run(request).await)
            } else {
                warn!("Invalid API key provided");
                Err(StatusCode::UNAUTHORIZED)
            }
        }
        None => {
            warn!("Missing X-API-Key header for protected endpoint");
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}

#[tokio::main]
async fn main() {
    // Initialize logging.
    // Honor RUST_LOG via EnvFilter (documented in .env.example / README) and fall
    // back to a sane default when it is unset or unparseable.
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,ktcs_calendar=info"));
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .init();

    // Load environment variables
    dotenvy::dotenv().ok();

    // Load and validate server configuration
    let server_config = ServerConfig::from_env();
    if let Err(e) = server_config.validate() {
        error!("Server configuration validation failed: {}", e);
        std::process::exit(1);
    }
    info!("Server configuration validated successfully");

    // Initialize Kaspa service
    let kaspa_config = KaspaServiceConfig::from_env()
        .unwrap_or_else(|e| {
            warn!("Failed to load Kaspa config from env: {}. Using defaults.", e);
            KaspaServiceConfig::default()
        });

    // Validate Kaspa configuration
    if let Err(e) = kaspa_config.validate() {
        error!("Kaspa configuration validation failed: {}", e);
        std::process::exit(1);
    }
    info!("Kaspa configuration validated successfully");

    let kaspa_service = match KaspaService::new(kaspa_config) {
        Ok(service) => Arc::new(service),
        Err(e) => {
            error!("Failed to initialize Kaspa service: {}", e);
            std::process::exit(1);
        }
    };

    // Try to connect to Kaspa node
    let kaspa_clone = kaspa_service.clone();
    tokio::spawn(async move {
        match kaspa_clone.connect().await {
            Ok(_) => info!("Connected to Kaspa node"),
            Err(e) => {
                error!("Could not connect to Kaspa node: {}", e);
                error!("Server will reject submissions unless KTCS_MOCK_MODE=true is set");
            }
        }
    });

    // Initialize WebSocket state
    let ws_state = Arc::new(WsState::new());

    // Initialize database.
    // The default MUST include `?mode=rwc` so sqlx creates the file on a fresh
    // install with no env config; without it, startup fails with "unable to open
    // database file".
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:ktcs-calendar.db?mode=rwc".to_string());
    let database = match Database::new(&db_url).await {
        Ok(db) => Arc::new(db),
        Err(e) => {
            error!("Failed to initialize database: {}", e);
            std::process::exit(1);
        }
    };
    info!("Database initialized: {}", db_url);

    // Crash recovery: reload any `pending` stamps persisted before the last stop
    // (e.g. from a `systemctl restart` -> SIGTERM) back into the batch manager so
    // in-flight stamps are re-batched instead of being orphaned forever.
    let batch_manager = Arc::new(BatchManager::new());
    match recover_pending_stamps(&database, &batch_manager).await {
        Ok(n) if n > 0 => info!("Crash recovery: requeued {} pending stamp(s) into the batch manager", n),
        Ok(_) => info!("Crash recovery: no pending stamps to requeue"),
        Err(e) => warn!("Crash recovery failed to reload pending stamps: {}", e),
    }

    // Store API key in state for auth middleware (only if required)
    let api_key = if server_config.require_api_key {
        server_config.api_key.clone()
    } else {
        None
    };

    // Initialize application state
    let state = AppState {
        batch_manager,
        database,
        kaspa_service,
        ws_state,
        public_url: server_config.public_url.clone(),
        api_key: api_key.clone(),
        cors_origins: server_config.cors_origins.clone(),
        trust_proxy: server_config.trust_proxy,
    };

    // Create shutdown channel for graceful shutdown
    let (shutdown_tx, shutdown_rx) = broadcast::channel::<()>(1);

    // Start batch processing task
    let batch_state = state.clone();
    tokio::spawn(async move {
        batch_processing_loop(batch_state, shutdown_rx).await;
    });

    // Start periodic UTXO refresh task (every 10 seconds)
    let utxo_kaspa = state.kaspa_service.clone();
    let mut utxo_shutdown_rx = shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
        loop {
            tokio::select! {
                _ = utxo_shutdown_rx.recv() => {
                    info!("UTXO refresh task received shutdown signal");
                    break;
                }
                _ = interval.tick() => {
                    if let Err(e) = utxo_kaspa.refresh_utxos().await {
                        tracing::debug!("Periodic UTXO refresh failed: {}", e);
                    }
                }
            }
        }
    });

    // Start database cleanup task (every 24 hours)
    let cleanup_db = state.database.clone();
    let mut cleanup_shutdown_rx = shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(86400)); // 24 hours
        loop {
            tokio::select! {
                _ = cleanup_shutdown_rx.recv() => {
                    info!("Database cleanup task received shutdown signal");
                    break;
                }
                _ = interval.tick() => {
                    // Delete confirmed stamps older than 30 days
                    match cleanup_db.delete_old_stamps(30 * 86400).await {
                        Ok(deleted) if deleted > 0 => {
                            info!("Database cleanup: removed {} old stamps", deleted);
                        }
                        Err(e) => warn!("Database cleanup failed: {}", e),
                        _ => {}
                    }
                }
            }
        }
    });

    // Start recycle service task (if dual-wallet recycling is enabled)
    if state.kaspa_service.is_recycling_enabled() {
        let recycle_service = Arc::new(RecycleService::new(state.kaspa_service.clone()));
        let recycle_shutdown_rx = shutdown_tx.subscribe();
        tokio::spawn(async move {
            recycle_service.run_loop(recycle_shutdown_rx).await;
        });

        // Also refresh return wallet UTXOs periodically
        let return_kaspa = state.kaspa_service.clone();
        let mut return_utxo_shutdown_rx = shutdown_tx.subscribe();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
            loop {
                tokio::select! {
                    _ = return_utxo_shutdown_rx.recv() => {
                        info!("Return UTXO refresh task received shutdown signal");
                        break;
                    }
                    _ = interval.tick() => {
                        if let Err(e) = return_kaspa.refresh_return_utxos().await {
                            tracing::debug!("Periodic return UTXO refresh failed: {}", e);
                        }
                    }
                }
            }
        });
    }

    // Configure rate limiting with proxy-aware IP extraction
    let key_extractor = ProxyAwareIpExtractor { trust_proxy: server_config.trust_proxy };
    if server_config.trust_proxy {
        info!("Rate limiter configured to trust X-Forwarded-For/X-Real-IP headers");
    }
    let governor_conf = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(server_config.rate_limit_per_second as u64)
            .burst_size(server_config.rate_limit_burst)
            .key_extractor(key_extractor)
            .finish()
            .expect("Failed to create rate limiter config")
    );

    // Build CORS layer
    let cors_layer = build_cors_layer(&server_config.cors_origins);

    // Split /v1 into write vs. public routes.
    //
    // Per the documented "protects write ops" intent, the API key guards ONLY the
    // write endpoint (POST /v1/stamp). Reads (GET /v1/stamp/:id), verification
    // (POST /v1/verify) and the WebSocket stream (GET /v1/stream) are PUBLIC, so a
    // public frontend can read/stream/verify even when REQUIRE_API_KEY=true.
    // Browsers cannot set X-API-Key on a WebSocket handshake at all, so gating the
    // WS behind the key previously made a working public frontend impossible.
    let write_routes = Router::new()
        .route("/stamp", post(submit_stamp))
        .route_layer(middleware::from_fn_with_state(state.clone(), api_key_middleware));

    let public_v1_routes = Router::new()
        .route("/stamp/:id", get(get_stamp))
        .route("/verify", post(verify_proof))
        .route("/stream", get(ws_handler::<AppState>));

    let v1_routes = write_routes.merge(public_v1_routes);

    // Build router with security layers
    // CORS must be applied LAST (outermost) so it adds headers to ALL responses including rate-limited 429s
    let app = Router::new()
        .route("/health", get(health_check))  // Health check without auth
        .nest("/v1", v1_routes)
        .layer(ServiceBuilder::new()
            // Body size limit
            .layer(RequestBodyLimitLayer::new(server_config.max_body_size))
            // Rate limiting
            .layer(GovernorLayer {
                config: governor_conf,
            })
        )
        // CORS applied after ServiceBuilder - outermost layer
        .layer(cors_layer)
        .with_state(state.clone());

    info!("KTCS Calendar Server starting on {}", server_config.bind_address);
    info!("Security configuration:");
    info!("  CORS origins: {:?}", server_config.cors_origins);
    info!("  Rate limit: {}/s (burst: {})", server_config.rate_limit_per_second, server_config.rate_limit_burst);
    info!("  Max body size: {} bytes", server_config.max_body_size);
    info!("  API key required: {}", api_key.is_some());

    // Log wallet configuration
    if state.kaspa_service.is_recycling_enabled() {
        info!("Wallet configuration (dual-wallet recycling ENABLED):");
        info!("  STAMP wallet: {}", state.kaspa_service.config().wallet_address);
        if let Some(return_wallet) = state.kaspa_service.return_wallet() {
            info!("  RETURN wallet: {}", return_wallet.address());
        }
        info!("  Recycle threshold: {} sompi ({:.2} KAS)",
            state.kaspa_service.recycle_threshold(),
            state.kaspa_service.recycle_threshold() as f64 / 100_000_000.0
        );
        info!("  Recycle poll interval: {}s", state.kaspa_service.config().recycle_poll_interval_secs);
    } else {
        info!("Wallet configuration (single wallet mode):");
        info!("  Wallet: {}", state.kaspa_service.config().wallet_address);
    }
    info!("Endpoints:");
    info!("  POST /v1/stamp     - Submit a timestamp");
    info!("  GET  /v1/stamp/:id - Get stamp status");
    info!("  POST /v1/verify    - Verify a proof");
    info!("  WS   /v1/stream    - Real-time confirmations");
    info!("  GET  /health       - Health check");

    // Bind with proper error handling
    let listener = match tokio::net::TcpListener::bind(&server_config.bind_address).await {
        Ok(l) => l,
        Err(e) => {
            error!("Failed to bind to {}: {}", server_config.bind_address, e);
            std::process::exit(1);
        }
    };

    // Run server with graceful shutdown
    // Use into_make_service_with_connect_info to enable PeerIpKeyExtractor for rate limiting
    let app = app.into_make_service_with_connect_info::<SocketAddr>();
    let server = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            // Wait for EITHER SIGINT (Ctrl+C) OR SIGTERM. systemd stops services
            // with SIGTERM, so listening only for Ctrl+C (SIGINT) meant every
            // `systemctl stop/restart` skipped this drain path and hard-killed the
            // process, stranding in-flight stamps.
            #[cfg(unix)]
            {
                use tokio::signal::unix::{signal, SignalKind};
                let mut sigterm =
                    signal(SignalKind::terminate()).expect("Failed to install SIGTERM handler");
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => info!("SIGINT received"),
                    _ = sigterm.recv() => info!("SIGTERM received"),
                }
            }
            #[cfg(not(unix))]
            {
                tokio::signal::ctrl_c()
                    .await
                    .expect("Failed to install Ctrl+C handler");
            }
            info!("Shutdown signal received, draining requests...");

            // Signal batch processor to shut down
            let _ = shutdown_tx.send(());

            // Give some time for graceful cleanup
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            info!("Graceful shutdown complete");
        });

    if let Err(e) = server.await {
        error!("Server error: {}", e);
        std::process::exit(1);
    }
}

/// Health check endpoint
async fn health_check(State(state): State<AppState>) -> Json<HealthResponse> {
    let pending = state.database.count_by_status(DbStampStatus::Pending)
        .await
        .unwrap_or(0) as usize;

    Json(HealthResponse {
        status: "ok".to_string(),
        version: ktcs_core::VERSION.to_string(),
        pending_stamps: pending,
    })
}

/// Submit a new timestamp
async fn submit_stamp(
    State(state): State<AppState>,
    Json(req): Json<StampRequest>,
) -> Result<Json<StampResponse>, (StatusCode, String)> {
    // Validate algorithm (currently only sha256 is supported)
    if req.algorithm != "sha256" {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Unsupported algorithm: '{}'. Only 'sha256' is supported.", req.algorithm),
        ));
    }

    // Parse and validate digest
    let digest_bytes = hex::decode(&req.digest)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid digest hex: {}", e)))?;

    if digest_bytes.len() != 32 {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Invalid digest length for {}: expected 32 bytes, got {}", req.algorithm, digest_bytes.len()),
        ));
    }

    let mut digest = [0u8; 32];
    digest.copy_from_slice(&digest_bytes);

    // Generate ID
    let id = format!("ktcs_{}", &uuid::Uuid::new_v4().to_string().replace("-", "")[..16]);

    // Get current timestamp
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    // Create pending proof
    let mut proof = KtcsProof::new(digest.to_vec());
    proof.add_attestation(Attestation::Pending(PendingAttestation {
        calendar_url: format!("{}/v1/stamp/{}", state.public_url, id),
    }));

    let pending_proof = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        serialize_proof(&proof),
    );

    // Save to database
    let db_record = DbStampRecord {
        id: id.clone(),
        digest,
        status: DbStampStatus::Pending,
        submitted_at: (now / 1000) as i64, // ms -> seconds for DB
        confirmed_at: None,
        proof: Some(serialize_proof(&proof)),
        batch_mode: format!("{:?}", req.batch_mode),
    };
    state.database.save_stamp(&db_record).await
        .map_err(|e| {
            error!("Database error saving stamp: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
        })?;

    // Add to batch manager (may fail if pending limit exceeded)
    if !state.batch_manager.add_digest(id.clone(), digest, req.batch_mode).await {
        // Clean up the database record we just saved
        if let Err(e) = state.database.delete_stamp(&id).await {
            error!("Failed to clean up stamp after batch limit exceeded: {}", e);
        }
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Server is at capacity. Please try again later.".to_string(),
        ));
    }

    info!("Stamp submitted: {} (mode: {:?})", id, req.batch_mode);

    // Estimate confirmation time
    let estimated_ms = req.batch_mode.window_ms() + 1000; // batch window + 1s for block confirmation
    let estimated_at = now + estimated_ms;

    Ok(Json(StampResponse {
        id,
        status: "pending".to_string(),
        submitted_at: format_timestamp(now),
        estimated_confirmation: Some(format_timestamp(estimated_at)),
        pending_proof: Some(pending_proof),
        // Confirmed fields are None for pending stamps
        confirmed_at: None,
        daa_score: None,
        blue_score: None,
        block_hash: None,
        tx_hash: None,
        proof: None,
        thermodynamic_weight: None,
        parent_hashes: None,
    }))
}

/// Get stamp status
async fn get_stamp(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<StampResponse>, (StatusCode, String)> {
    let db_record = state.database.get_stamp(&id).await
        .map_err(|e| {
            error!("Database error fetching stamp: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
        })?
        .ok_or((StatusCode::NOT_FOUND, format!("Stamp not found: {}", id)))?;

    // Convert DB timestamp (seconds) to milliseconds for formatting
    let submitted_at_ms = (db_record.submitted_at as u64) * 1000;

    let response = match db_record.status {
        DbStampStatus::Pending => StampResponse {
            id: db_record.id.clone(),
            status: "pending".to_string(),
            submitted_at: format_timestamp(submitted_at_ms),
            estimated_confirmation: None,
            pending_proof: None,
            confirmed_at: None,
            daa_score: None,
            blue_score: None,
            block_hash: None,
            tx_hash: None,
            proof: None,
            thermodynamic_weight: None,
            parent_hashes: None,
        },
        DbStampStatus::Batched => StampResponse {
            id: db_record.id.clone(),
            status: "batched".to_string(),
            submitted_at: format_timestamp(submitted_at_ms),
            estimated_confirmation: None,
            pending_proof: None,
            confirmed_at: None,
            daa_score: None,
            blue_score: None,
            block_hash: None,
            tx_hash: None,
            proof: None,
            thermodynamic_weight: None,
            parent_hashes: None,
        },
        DbStampStatus::Confirmed => {
            // Deserialize the proof from database
            let proof_bytes = db_record.proof.as_ref()
                .ok_or((StatusCode::INTERNAL_SERVER_ERROR, "Confirmed stamp missing proof".to_string()))?;
            let proof = ktcs_core::deserialize_proof(proof_bytes)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Invalid proof data: {}", e)))?;

            let proof_base64 = base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                proof_bytes,
            );

            // Get attestation data
            let ka = proof.kaspa_attestations().next();
            let (daa_score, blue_score, block_hash, tx_hash, thermo, parent_hashes) = match ka {
                Some(att) => (
                    Some(att.daa_score),
                    Some(att.blue_score),
                    Some(hex::encode(att.block_hash)),
                    Some(hex::encode(att.tx_hash)),
                    Some(ThermodynamicWeightResponse {
                        blue_work_at_confirmation: ktcs_core::format_blue_work(&att.blue_work),
                        current_blue_work: None, // Would be populated if connected to node
                        accumulated_since: None,
                    }),
                    Some(
                        att.parent_hashes
                            .iter()
                            .map(|h| hex::encode(h))
                            .collect::<Vec<_>>(),
                    ),
                ),
                None => (None, None, None, None, None, None),
            };

            let confirmed_at_ms = db_record.confirmed_at.map(|t| (t as u64) * 1000);

            StampResponse {
                id: db_record.id.clone(),
                status: "confirmed".to_string(),
                submitted_at: format_timestamp(submitted_at_ms),
                estimated_confirmation: None,
                pending_proof: None,
                confirmed_at: confirmed_at_ms.map(format_timestamp),
                daa_score,
                blue_score,
                block_hash,
                tx_hash,
                proof: Some(proof_base64),
                thermodynamic_weight: thermo,
                parent_hashes,
            }
        }
    };

    Ok(Json(response))
}

/// Verify a proof
async fn verify_proof(
    State(state): State<AppState>,
    body: axum::body::Bytes,
) -> Result<Json<VerifyResponse>, (StatusCode, String)> {
    let proof = ktcs_core::deserialize_proof(&body)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid proof: {}", e)))?;

    let result = ktcs_core::verify_proof(&proof, None)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Verification error: {}", e)))?;

    // Convert attestations to the response format
    let attestations: Vec<AttestationInfoResponse> = result
        .attestations
        .iter()
        .map(|a| {
            // Extract Kaspa-specific fields from details enum
            match &a.details {
                ktcs_core::verify::AttestationDetails::Kaspa {
                    daa_score,
                    blue_score,
                    block_hash,
                    timestamp,
                    blue_work,
                    ..
                } => AttestationInfoResponse {
                    attestation_type: "kaspa_block".to_string(),
                    daa_score: Some(*daa_score),
                    blue_score: Some(*blue_score),
                    block_hash: Some(block_hash.clone()),
                    timestamp: Some(format_timestamp(*timestamp)),
                    thermodynamic_weight: Some(blue_work.clone()),
                },
                ktcs_core::verify::AttestationDetails::Pending { .. } => AttestationInfoResponse {
                    attestation_type: "pending".to_string(),
                    daa_score: None,
                    blue_score: None,
                    block_hash: None,
                    timestamp: None,
                    thermodynamic_weight: None,
                },
                ktcs_core::verify::AttestationDetails::Bitcoin { block_height } => AttestationInfoResponse {
                    attestation_type: "bitcoin".to_string(),
                    daa_score: Some(*block_height as u64),
                    blue_score: None,
                    block_hash: None,
                    timestamp: None,
                    thermodynamic_weight: None,
                },
            }
        })
        .collect();

    // Try to get current confirmations if we have a Kaspa attestation
    let current_confirmations = {
        // Find first Kaspa attestation
        let kaspa_att = result.attestations.iter().find_map(|a| {
            if let ktcs_core::verify::AttestationDetails::Kaspa {
                daa_score,
                blue_work,
                ..
            } = &a.details
            {
                // Convert hex blue_work back to bytes
                let mut bw = [0u8; 32];
                if let Ok(decoded) = hex::decode(blue_work) {
                    if decoded.len() == 32 {
                        bw.copy_from_slice(&decoded);
                    }
                }
                Some((*daa_score, bw))
            } else {
                None
            }
        });

        if let Some((daa_score, blue_work)) = kaspa_att {
            match state
                .kaspa_service
                .calculate_thermodynamic_metrics(&blue_work, daa_score)
                .await
            {
                Ok(metrics) => {
                    if let (Some(blocks), Some(accumulated), Some(elapsed)) = (
                        metrics.blocks_since,
                        metrics.accumulated_blue_work,
                        metrics.time_elapsed_seconds,
                    ) {
                        Some(CurrentConfirmationsResponse {
                            blocks_since: blocks,
                            blue_work_accumulated: accumulated,
                            time_elapsed_seconds: elapsed,
                        })
                    } else {
                        None
                    }
                }
                Err(_) => None,
            }
        } else {
            None
        }
    };

    Ok(Json(VerifyResponse {
        valid: result.valid,
        digest: result.digest,
        attestations,
        current_confirmations,
        error: result.error,
    }))
}

/// Background task that processes batches
async fn batch_processing_loop(state: AppState, mut shutdown: broadcast::Receiver<()>) {
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));

    loop {
        tokio::select! {
            _ = shutdown.recv() => {
                info!("Batch processor received shutdown signal, draining current batch...");
                // Process any remaining ready batches before exiting
                let ready_batches: Vec<(BatchMode, Vec<PendingStamp>)> =
                    state.batch_manager.get_ready_batches().await;
                if !ready_batches.is_empty() {
                    info!("Processing {} remaining batches before shutdown", ready_batches.len());
                }
                // Note: For a complete implementation, we would process remaining batches here
                info!("Batch processor shutdown complete");
                return;
            }
            _ = interval.tick() => {
                // Continue with normal processing below
            }
        }

        // Check for batches ready to commit
        let ready_batches: Vec<(BatchMode, Vec<PendingStamp>)> =
            state.batch_manager.get_ready_batches().await;

        for (mode, stamps) in ready_batches {
            if stamps.is_empty() {
                continue;
            }

            info!(
                "Processing batch: {} stamps (mode: {:?})",
                stamps.len(),
                mode
            );

            // NOTE: the `batched` WS event is intentionally NOT broadcast here.
            // Previously it fired before submission, so clients were told "batched"
            // even for batches that then failed to submit, and the DB `batched`
            // status was never written (making it unreachable). We now emit both the
            // `batched` status and event together, only once a tx has actually been
            // accepted but not yet confirmed (the SubmittedUnconfirmed path below).

            // Build Merkle tree from digests
            let leaves: Vec<[u8; 32]> = stamps.iter().map(|s| s.digest).collect();
            let tree = match MerkleTree::build(leaves) {
                Ok(t) => t,
                Err(e) => {
                    tracing::error!("Failed to build Merkle tree: {}", e);
                    continue;
                }
            };

            let merkle_root = tree.root();
            info!("Merkle root: {}", hex::encode(merkle_root));

            // Refresh UTXOs before each batch submission to avoid stale cache
            if let Err(e) = state.kaspa_service.refresh_utxos().await {
                tracing::warn!("Failed to refresh UTXOs before batch: {}", e);
                // Continue anyway - submit_commitment will fail if truly no UTXOs
            }

            // Submit commitment to Kaspa blockchain (or mock if not connected)
            let submission = match state.kaspa_service.submit_commitment(merkle_root).await {
                Ok(result) => result,
                // The tx was ACCEPTED on-chain but confirmation timed out. Do NOT
                // rebuild/resubmit — that would double-anchor (pay the fee twice and
                // spend stale UTXOs). Instead mark the stamps `batched` (submitted,
                // awaiting confirmation) and broadcast the `batched` event. They are
                // not requeued; a follow-up confirmation would need this tx hash.
                Err(KaspaServiceError::SubmittedUnconfirmed { tx_hash }) => {
                    tracing::warn!(
                        "Batch tx {} accepted but unconfirmed; marking {} stamp(s) as batched \
                         (NOT resubmitting to avoid double-anchoring)",
                        tx_hash,
                        stamps.len()
                    );
                    for pending in &stamps {
                        if let Err(e) = state
                            .database
                            .update_stamp_status(&pending.id, DbStampStatus::Batched, None)
                            .await
                        {
                            tracing::error!("Failed to mark stamp {} as batched: {}", pending.id, e);
                        }
                        state.ws_state.broadcast_batched(BatchedEvent {
                            proof_id: pending.id.clone(),
                        });
                    }
                    continue;
                }
                Err(e) => {
                    // Submit genuinely failed (tx never accepted) → safe to requeue.
                    tracing::error!("Failed to submit commitment: {}", e);

                    // Requeue stamps for retry instead of discarding
                    let mut requeued = 0;
                    for stamp in &stamps {
                        if state.batch_manager.add_digest(stamp.id.clone(), stamp.digest, mode).await {
                            requeued += 1;
                        }
                    }
                    if requeued < stamps.len() {
                        tracing::warn!(
                            "Could not requeue all stamps: {} of {} (pending limit reached)",
                            requeued, stamps.len()
                        );
                    } else {
                        tracing::info!("Requeued {} stamps for retry", requeued);
                    }

                    // Add backoff delay to avoid rapid retry loops
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    continue;
                }
            };

            info!(
                "Commitment submitted: tx={} block={} daa={}",
                hex::encode(submission.tx_hash),
                hex::encode(submission.block_hash),
                submission.daa_score
            );

            // Build attestation from submission result
            let attestation = KaspaAttestation::new(
                submission.daa_score,
                submission.blue_score,
                submission.block_hash,
                submission.timestamp,
                submission.tx_hash,
                0,
                submission.blue_work,
                submission.parent_hashes,
            );

            // Update each stamp with its proof
            for (i, pending) in stamps.iter().enumerate() {
                // Get Merkle proof for this leaf
                let merkle_proof = match tree.get_proof(i) {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::error!("Failed to get Merkle proof: {}", e);
                        continue;
                    }
                };

                // Build complete proof
                let mut proof = KtcsProof::new(pending.digest.to_vec());

                // Add Merkle path operations
                for op in merkle_proof.to_operations() {
                    proof.add_operation(op);
                }

                // Add attestation
                proof.add_attestation(Attestation::Kaspa(attestation.clone()));

                // Serialize proof
                let proof_bytes = serialize_proof(&proof);

                // Serialize proof for WebSocket broadcast
                let proof_base64 = base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    &proof_bytes,
                );

                // Atomically write status + proof + confirmed_at in a SINGLE UPDATE.
                // Previously these were two separate writes, so a crash between them
                // could leave a `confirmed` row carrying a stale/pending proof with no
                // Kaspa attestation. confirm_stamp guarantees a confirmed row always
                // has its complete proof.
                let confirmed_at_secs = (submission.timestamp / 1000) as i64;
                if let Err(e) = state
                    .database
                    .confirm_stamp(&pending.id, &proof_bytes, confirmed_at_secs)
                    .await
                {
                    tracing::error!("Failed to atomically confirm stamp in database: {}", e);
                    // Do not broadcast a confirmation we failed to persist; a later
                    // recovery/poll can reconcile from the still-pending row.
                    continue;
                }

                info!("Stamp confirmed: {}", pending.id);

                // Broadcast confirmation to WebSocket subscribers
                state.ws_state.broadcast_confirmation(ConfirmationEvent {
                    proof_id: pending.id.clone(),
                    block_hash: hex::encode(submission.block_hash),
                    daa_score: submission.daa_score,
                    blue_score: submission.blue_score,
                    timestamp: submission.timestamp,
                    proof_base64,
                });
            }
        }
    }
}

/// Parse a stored batch-mode string back into a [`BatchMode`].
///
/// The DB persists `format!("{:?}", mode)` ("Instant"/"Standard"/"Economic"); we
/// also accept the lowercase serde form for robustness. Unknown values fall back
/// to the default (Standard).
fn parse_batch_mode(s: &str) -> BatchMode {
    match s.trim().to_ascii_lowercase().as_str() {
        "instant" => BatchMode::Instant,
        "economic" => BatchMode::Economic,
        "standard" => BatchMode::Standard,
        other => {
            warn!("Unknown batch_mode '{}' during recovery, defaulting to standard", other);
            BatchMode::Standard
        }
    }
}

/// Reload persisted `pending` stamps into the batch manager on startup.
///
/// Pending rows are stamps that were accepted but whose batch had not yet been
/// committed when the process last stopped. Without this, every restart orphans
/// them as `pending` rows that the confirmed-only cleanup task never removes.
///
/// Note: `batched` rows (a tx was submitted but confirmation timed out — see the
/// SubmittedUnconfirmed path) are deliberately NOT requeued here, as resubmitting
/// them would double-anchor. Returns the number of stamps requeued.
async fn recover_pending_stamps(
    database: &Database,
    batch_manager: &BatchManager,
) -> std::result::Result<usize, services::database::DatabaseError> {
    let pending = database.get_stamps_by_status(DbStampStatus::Pending).await?;
    let mut requeued = 0usize;
    for record in pending {
        let mode = parse_batch_mode(&record.batch_mode);
        if batch_manager
            .add_digest(record.id.clone(), record.digest, mode)
            .await
        {
            requeued += 1;
        } else {
            warn!(
                "Could not requeue pending stamp {} on startup (batch limit reached)",
                record.id
            );
        }
    }
    Ok(requeued)
}

fn format_timestamp(ms: u64) -> String {
    use time::{OffsetDateTime, format_description::well_known::Rfc3339};

    let secs = (ms / 1000) as i64;
    let nanos = ((ms % 1000) * 1_000_000) as u32;

    OffsetDateTime::from_unix_timestamp(secs)
        .ok()
        .and_then(|dt| dt.replace_nanosecond(nanos).ok())
        .and_then(|dt| dt.format(&Rfc3339).ok())
        .unwrap_or_else(|| "Invalid timestamp".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    /// Create test app state with mock mode enabled and in-memory database
    async fn create_test_state() -> AppState {
        let kaspa_config = KaspaServiceConfig {
            mock_mode: true,
            ..Default::default()
        };
        let kaspa_service = Arc::new(KaspaService::new(kaspa_config).expect("Failed to create test Kaspa service"));

        // Use in-memory database for tests
        let database = Arc::new(
            Database::in_memory().await.expect("Failed to create test database")
        );

        AppState {
            batch_manager: Arc::new(BatchManager::new()),
            database,
            kaspa_service,
            ws_state: Arc::new(WsState::new()),
            public_url: "http://localhost:3001".to_string(),
            api_key: None, // No API key for tests
            cors_origins: vec!["*".to_string()], // Allow all origins in tests
            trust_proxy: false,
        }
    }

    /// Create test router without middleware (for unit testing)
    fn create_test_router(state: AppState) -> Router {
        Router::new()
            .route("/health", get(health_check))
            .route("/v1/stamp", post(submit_stamp))
            .route("/v1/stamp/:id", get(get_stamp))
            .route("/v1/verify", post(verify_proof))
            .with_state(state)
    }

    /// Create a router that mirrors production auth routing: the API key guards
    /// ONLY POST /v1/stamp; reads/verify/health stay public.
    fn create_test_router_with_auth(state: AppState) -> Router {
        let write_routes = Router::new()
            .route("/stamp", post(submit_stamp))
            .route_layer(middleware::from_fn_with_state(state.clone(), api_key_middleware));
        let public_v1 = Router::new()
            .route("/stamp/:id", get(get_stamp))
            .route("/verify", post(verify_proof));
        let v1 = write_routes.merge(public_v1);
        Router::new()
            .route("/health", get(health_check))
            .nest("/v1", v1)
            .with_state(state)
    }

    #[tokio::test]
    async fn test_health_check() {
        let state = create_test_state().await;
        let app = create_test_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["status"], "ok");
        assert!(json["version"].as_str().is_some());
    }

    #[tokio::test]
    async fn test_submit_stamp() {
        let state = create_test_state().await;
        let app = create_test_router(state);

        // Submit a stamp
        let request_body = serde_json::json!({
            "digest": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "algorithm": "sha256",
            "batch_mode": "instant"
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/stamp")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&request_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["status"], "pending");
        assert!(json["id"].as_str().unwrap().starts_with("ktcs_"));
        assert!(json["pending_proof"].as_str().is_some());
    }

    #[tokio::test]
    async fn test_submit_stamp_invalid_digest() {
        let state = create_test_state().await;
        let app = create_test_router(state);

        // Submit with invalid (too short) digest
        let request_body = serde_json::json!({
            "digest": "abc123",
            "algorithm": "sha256"
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/stamp")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&request_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_get_stamp_not_found() {
        let state = create_test_state().await;
        let app = create_test_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/stamp/nonexistent_id")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_full_stamp_workflow_mock() {
        // This test verifies the full workflow in mock mode:
        // 1. Submit a stamp
        // 2. Process the batch
        // 3. Check the stamp is confirmed
        // 4. Verify the proof

        let state = create_test_state().await;

        // Start batch processing in background
        let batch_state = state.clone();
        let batch_handle = tokio::spawn(async move {
            // Run just a few iterations
            for _ in 0..50 {
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

                let ready_batches: Vec<(BatchMode, Vec<PendingStamp>)> =
                    batch_state.batch_manager.get_ready_batches().await;

                for (_mode, stamps) in ready_batches {
                    if stamps.is_empty() {
                        continue;
                    }

                    // Build Merkle tree
                    let leaves: Vec<[u8; 32]> = stamps.iter().map(|s| s.digest).collect();
                    let tree = MerkleTree::build(leaves).unwrap();
                    let merkle_root = tree.root();

                    // Submit to mock Kaspa
                    let submission = batch_state.kaspa_service.submit_commitment(merkle_root).await.unwrap();

                    // Build attestation
                    let attestation = KaspaAttestation::new(
                        submission.daa_score,
                        submission.blue_score,
                        submission.block_hash,
                        submission.timestamp,
                        submission.tx_hash,
                        0,
                        submission.blue_work,
                        submission.parent_hashes,
                    );

                    // Update stamps in database
                    for (i, pending) in stamps.iter().enumerate() {
                        let merkle_proof = tree.get_proof(i).unwrap();
                        let mut proof = KtcsProof::new(pending.digest.to_vec());
                        for op in merkle_proof.to_operations() {
                            proof.add_operation(op);
                        }
                        proof.add_attestation(Attestation::Kaspa(attestation.clone()));

                        let proof_bytes = serialize_proof(&proof);
                        let confirmed_at_secs = (submission.timestamp / 1000) as i64;

                        let _ = batch_state
                            .database
                            .confirm_stamp(&pending.id, &proof_bytes, confirmed_at_secs)
                            .await;
                    }
                }
            }
        });

        // Create router for API calls
        let app = create_test_router(state.clone());

        // SHA256 of empty string
        let digest = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

        // Step 1: Submit a stamp with instant batch mode
        let request_body = serde_json::json!({
            "digest": digest,
            "batch_mode": "instant"
        });

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/stamp")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&request_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let stamp_id = json["id"].as_str().unwrap().to_string();

        // Step 2: Wait for batch processing
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // Clean up batch processor
        batch_handle.abort();

        // Step 3: Check status - should be confirmed
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/v1/stamp/{}", stamp_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["status"], "confirmed");
        assert!(json["proof"].as_str().is_some());
        assert!(json["daa_score"].as_u64().is_some());
        assert!(json["block_hash"].as_str().is_some());
        assert!(json["tx_hash"].as_str().is_some());

        // Step 4: Verify the proof
        let proof_base64 = json["proof"].as_str().unwrap();
        let proof_bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            proof_base64,
        ).unwrap();

        // Deserialize and verify
        let proof = ktcs_core::deserialize_proof(&proof_bytes).unwrap();
        let verification = ktcs_core::verify_proof(&proof, None).unwrap();

        assert!(verification.valid, "Proof verification failed: {:?}", verification.error);
        assert_eq!(verification.digest, digest);
        assert!(!verification.attestations.is_empty());

        // Check attestation details
        let attestation = &verification.attestations[0];
        if let ktcs_core::verify::AttestationDetails::Kaspa { daa_score, .. } = &attestation.details {
            assert!(*daa_score > 42_000_000); // Mock mode DAA score base
        } else {
            panic!("Expected Kaspa attestation");
        }
    }

    #[tokio::test]
    async fn test_api_key_only_guards_writes() {
        // C7 fix: with REQUIRE_API_KEY on, the key must protect ONLY POST /v1/stamp.
        let mut state = create_test_state().await;
        state.api_key = Some("test-api-key-1234567890".to_string());
        let app = create_test_router_with_auth(state);

        let digest = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let body = serde_json::json!({ "digest": digest, "batch_mode": "instant" }).to_string();

        // POST /v1/stamp WITHOUT key -> 401
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/stamp")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "write must require key");

        // POST /v1/stamp WITH correct key -> 200
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/stamp")
                    .header("Content-Type", "application/json")
                    .header("X-API-Key", "test-api-key-1234567890")
                    .body(Body::from(body.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "write must succeed with key");

        // GET /v1/stamp/:id WITHOUT key -> public (404 for unknown id, never 401)
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/stamp/does_not_exist")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(resp.status(), StatusCode::UNAUTHORIZED, "reads must be public");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // POST /v1/verify WITHOUT key -> public (400 for a garbage proof, never 401)
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/verify")
                    .body(Body::from(vec![0u8, 1, 2, 3]))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(resp.status(), StatusCode::UNAUTHORIZED, "verify must be public");

        // GET /health -> public
        let resp = app
            .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[test]
    fn test_forwarded_client_ip_rejects_spoofed_leftmost_xff() {
        // X-Real-IP is preferred when present.
        let mut h = HeaderMap::new();
        h.insert("x-real-ip", "203.0.113.9".parse().unwrap());
        h.insert("x-forwarded-for", "1.1.1.1, 203.0.113.9".parse().unwrap());
        assert_eq!(forwarded_client_ip(&h).unwrap().to_string(), "203.0.113.9");

        // Without X-Real-IP, the RIGHTMOST XFF entry (proxy-observed) is used, NOT the
        // attacker-controlled leftmost one.
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", "6.6.6.6, 203.0.113.9".parse().unwrap());
        assert_eq!(forwarded_client_ip(&h).unwrap().to_string(), "203.0.113.9");

        // Single-entry XFF.
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", "198.51.100.7".parse().unwrap());
        assert_eq!(forwarded_client_ip(&h).unwrap().to_string(), "198.51.100.7");

        // No proxy headers -> None (caller falls back to the TCP peer).
        assert!(forwarded_client_ip(&HeaderMap::new()).is_none());
    }

    #[tokio::test]
    async fn test_recover_pending_stamps_requeues() {
        // C6b fix: pending rows persisted before a restart are reloaded into the
        // batch manager instead of being orphaned.
        let db = Database::in_memory().await.unwrap();

        for (i, mode) in ["Instant", "Standard"].iter().enumerate() {
            db.save_stamp(&DbStampRecord {
                id: format!("recover_{}", i),
                digest: [i as u8; 32],
                status: DbStampStatus::Pending,
                submitted_at: 1000 + i as i64,
                confirmed_at: None,
                proof: None,
                batch_mode: mode.to_string(),
            })
            .await
            .unwrap();
        }
        // A confirmed row must NOT be requeued.
        db.save_stamp(&DbStampRecord {
            id: "already_done".to_string(),
            digest: [42u8; 32],
            status: DbStampStatus::Confirmed,
            submitted_at: 2000,
            confirmed_at: Some(2001),
            proof: Some(vec![1, 2, 3]),
            batch_mode: "Instant".to_string(),
        })
        .await
        .unwrap();

        let bm = BatchManager::new();
        let n = recover_pending_stamps(&db, &bm).await.unwrap();
        assert_eq!(n, 2, "only the two pending rows are requeued");
        assert_eq!(bm.pending_count().await, 2);
        assert_eq!(bm.pending_count_by_mode(BatchMode::Instant).await, 1);
        assert_eq!(bm.pending_count_by_mode(BatchMode::Standard).await, 1);
    }

    #[test]
    fn test_parse_batch_mode() {
        assert_eq!(parse_batch_mode("Instant"), BatchMode::Instant);
        assert_eq!(parse_batch_mode("instant"), BatchMode::Instant);
        assert_eq!(parse_batch_mode("Economic"), BatchMode::Economic);
        assert_eq!(parse_batch_mode("Standard"), BatchMode::Standard);
        // Unknown falls back to the default.
        assert_eq!(parse_batch_mode("bogus"), BatchMode::Standard);
    }

    #[test]
    fn test_format_timestamp() {
        // Test epoch - RFC 3339 may omit milliseconds when zero
        let epoch = format_timestamp(0);
        assert!(epoch.starts_with("1970-01-01T00:00:00"));
        assert!(epoch.ends_with("Z"));

        // Test a known timestamp (2024-01-01 00:00:00 UTC)
        let ts = 1704067200000u64;
        let formatted = format_timestamp(ts);
        assert!(formatted.starts_with("2024-01-01T00:00:00"));

        // Test with milliseconds
        let ts_with_ms = 1704067200123u64;
        let formatted_ms = format_timestamp(ts_with_ms);
        assert!(formatted_ms.contains("123") || formatted_ms.contains(".123"));
    }
}
