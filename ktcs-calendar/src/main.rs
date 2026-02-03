//! KTCS Calendar Server
//!
//! Aggregation service that batches timestamp requests and commits them to Kaspa.

use axum::{
    extract::{Path, Request, State},
    http::{header, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, RwLock};
use tower::ServiceBuilder;
use tower_governor::{governor::GovernorConfigBuilder, key_extractor::{KeyExtractor, PeerIpKeyExtractor}, GovernorError, GovernorLayer};
use tower_http::{
    cors::CorsLayer,
    limit::RequestBodyLimitLayer,
};
use tracing::{info, warn, error, Level};
use tracing_subscriber::FmtSubscriber;
use subtle::ConstantTimeEq;

mod routes;
mod services;

use ktcs_core::{
    serialize_proof, Attestation, BatchMode, KaspaAttestation, KtcsProof, MerkleTree,
    PendingAttestation,
};
use routes::websocket::{ws_handler, BatchedEvent, ConfirmationEvent, HasAllowedOrigins, WsAppState, WsState};
use services::batch_manager::{BatchManager, PendingStamp};
use services::database::{Database, DbStampRecord, DbStampStatus};
use services::kaspa_service::{KaspaService, KaspaServiceConfig};
use services::recycle_service::RecycleService;

/// Application state shared across handlers
#[derive(Clone)]
struct AppState {
    batch_manager: Arc<RwLock<BatchManager>>,
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

/// Proxy-aware IP key extractor for rate limiting
/// When trust_proxy is true, checks X-Forwarded-For header first
#[derive(Clone)]
struct ProxyAwareIpExtractor {
    trust_proxy: bool,
}

impl KeyExtractor for ProxyAwareIpExtractor {
    type Key = IpAddr;

    fn extract<T>(&self, req: &axum::http::Request<T>) -> Result<Self::Key, GovernorError> {
        if self.trust_proxy {
            // Check X-Forwarded-For header (first IP is the original client)
            if let Some(xff) = req.headers().get("x-forwarded-for") {
                if let Ok(xff_str) = xff.to_str() {
                    if let Some(first_ip) = xff_str.split(',').next() {
                        if let Ok(ip) = first_ip.trim().parse::<IpAddr>() {
                            return Ok(ip);
                        }
                    }
                }
            }
            // Also check X-Real-IP header (used by nginx)
            if let Some(real_ip) = req.headers().get("x-real-ip") {
                if let Ok(ip_str) = real_ip.to_str() {
                    if let Ok(ip) = ip_str.trim().parse::<IpAddr>() {
                        return Ok(ip);
                    }
                }
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

    // Check X-API-Key header
    match request.headers().get("X-API-Key") {
        Some(provided_key) => {
            let provided = provided_key.to_str().unwrap_or("");
            // Use constant-time comparison to prevent timing attacks
            let provided_bytes = provided.as_bytes();
            let expected_bytes = expected_key.as_bytes();

            // Constant-time comparison that doesn't leak length
            let expected_len = expected_bytes.len();
            let provided_len = provided_bytes.len();

            // Pad provided bytes to expected length for constant-time comparison
            let mut provided_padded = vec![0u8; expected_len];
            let copy_len = provided_len.min(expected_len);
            provided_padded[..copy_len].copy_from_slice(&provided_bytes[..copy_len]);

            // Both length mismatch and content mismatch return false in constant time
            let lengths_equal = provided_len.ct_eq(&expected_len);
            let bytes_equal = provided_padded.ct_eq(expected_bytes);
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
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

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

    // Initialize database
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:ktcs-calendar.db".to_string());
    let database = match Database::new(&db_url).await {
        Ok(db) => Arc::new(db),
        Err(e) => {
            error!("Failed to initialize database: {}", e);
            std::process::exit(1);
        }
    };
    info!("Database initialized: {}", db_url);

    // Store API key in state for auth middleware (only if required)
    let api_key = if server_config.require_api_key {
        server_config.api_key.clone()
    } else {
        None
    };

    // Initialize application state
    let state = AppState {
        batch_manager: Arc::new(RwLock::new(BatchManager::new())),
        database,
        kaspa_service,
        ws_state,
        public_url: server_config.public_url.clone(),
        api_key: api_key.clone(),
        cors_origins: server_config.cors_origins.clone(),
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

    // Build protected routes (require API key if configured)
    let protected_routes = Router::new()
        .route("/stamp", post(submit_stamp))
        .route("/stamp/:id", get(get_stamp))
        .route("/verify", post(verify_proof))
        .route("/stream", get(ws_handler::<AppState>))
        .route_layer(middleware::from_fn_with_state(state.clone(), api_key_middleware));

    // Build router with security layers
    // CORS must be applied LAST (outermost) so it adds headers to ALL responses including rate-limited 429s
    let app = Router::new()
        .route("/health", get(health_check))  // Health check without auth
        .nest("/v1", protected_routes)
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
            // Wait for Ctrl+C signal
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to install Ctrl+C handler");
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

    // Add to batch manager
    {
        let mut batch_manager = state.batch_manager.write().await;
        batch_manager.add_digest(id.clone(), digest, req.batch_mode);
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
                let ready_batches: Vec<(BatchMode, Vec<PendingStamp>)> = {
                    let mut batch_manager = state.batch_manager.write().await;
                    batch_manager.get_ready_batches()
                };
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
        let ready_batches: Vec<(BatchMode, Vec<PendingStamp>)> = {
            let mut batch_manager = state.batch_manager.write().await;
            batch_manager.get_ready_batches()
        };

        for (mode, stamps) in ready_batches {
            if stamps.is_empty() {
                continue;
            }

            info!(
                "Processing batch: {} stamps (mode: {:?})",
                stamps.len(),
                mode
            );

            // Broadcast batched status for each stamp
            for pending in &stamps {
                state.ws_state.broadcast_batched(BatchedEvent {
                    proof_id: pending.id.clone(),
                });
            }

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
                Err(e) => {
                    tracing::error!("Failed to submit commitment: {}", e);

                    // Requeue stamps for retry instead of discarding
                    {
                        let mut batch_manager = state.batch_manager.write().await;
                        for stamp in &stamps {
                            batch_manager.add_digest(stamp.id.clone(), stamp.digest, mode);
                        }
                        tracing::info!("Requeued {} stamps for retry", stamps.len());
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

                // Update database
                let confirmed_at_secs = (submission.timestamp / 1000) as i64;
                if let Err(e) = state.database.update_stamp_status(
                    &pending.id,
                    DbStampStatus::Confirmed,
                    Some(confirmed_at_secs),
                ).await {
                    tracing::error!("Failed to update stamp status in database: {}", e);
                }
                if let Err(e) = state.database.update_stamp_proof(&pending.id, &proof_bytes).await {
                    tracing::error!("Failed to update stamp proof in database: {}", e);
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
            batch_manager: Arc::new(RwLock::new(BatchManager::new())),
            database,
            kaspa_service,
            ws_state: Arc::new(WsState::new()),
            public_url: "http://localhost:3001".to_string(),
            api_key: None, // No API key for tests
            cors_origins: vec!["*".to_string()], // Allow all origins in tests
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

                let ready_batches: Vec<(BatchMode, Vec<PendingStamp>)> = {
                    let mut batch_manager = batch_state.batch_manager.write().await;
                    batch_manager.get_ready_batches()
                };

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

                        let _ = batch_state.database.update_stamp_status(
                            &pending.id,
                            DbStampStatus::Confirmed,
                            Some(confirmed_at_secs),
                        ).await;
                        let _ = batch_state.database.update_stamp_proof(&pending.id, &proof_bytes).await;
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
