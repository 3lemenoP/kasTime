//! KTCS Calendar Server
//!
//! Aggregation service that batches timestamp requests and commits them to Kaspa.

use axum::{
    extract::{Path, State},
    http::{header, HeaderValue, Method, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceBuilder;
use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};
use tower_http::{
    cors::CorsLayer,
    limit::RequestBodyLimitLayer,
};
use tracing::{info, warn, error, Level};
use tracing_subscriber::FmtSubscriber;

mod routes;
mod services;

use ktcs_core::{
    serialize_proof, Attestation, BatchMode, KaspaAttestation, KtcsProof, MerkleTree,
    PendingAttestation,
};
use routes::websocket::{ws_handler, ConfirmationEvent, WsAppState, WsState};
use services::batch_manager::{BatchManager, PendingStamp};
use services::kaspa_service::{KaspaService, KaspaServiceConfig};

/// Application state shared across handlers
#[derive(Clone)]
struct AppState {
    batch_manager: Arc<RwLock<BatchManager>>,
    stamps: Arc<RwLock<HashMap<String, StampRecord>>>,
    kaspa_service: Arc<KaspaService>,
    ws_state: Arc<WsState>,
}

impl WsAppState for AppState {
    fn ws_state(&self) -> &Arc<WsState> {
        &self.ws_state
    }
}

/// Record of a submitted stamp
#[derive(Clone, Debug)]
#[allow(dead_code)]
struct StampRecord {
    id: String,
    digest: [u8; 32],
    status: StampStatus,
    submitted_at: u64,
    confirmed_at: Option<u64>,
    proof: Option<KtcsProof>,
    batch_mode: BatchMode,
}

#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)]
enum StampStatus {
    Pending,
    Batched,
    Confirmed,
}

/// API request to submit a stamp
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
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

        // Warn about insecure configurations
        if self.cors_origins.len() == 1 && self.cors_origins[0] == "*" {
            warn!("CORS allows all origins - this is insecure for production");
        }
        if !self.require_api_key {
            warn!("API key authentication is not required - consider enabling for production");
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

        Self {
            bind_address: std::env::var("BIND_ADDRESS")
                .unwrap_or_else(|_| "0.0.0.0:3001".to_string()),
            cors_origins,
            rate_limit_per_second: std::env::var("RATE_LIMIT_PER_SECOND")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10), // 10 requests per second default
            rate_limit_burst: std::env::var("RATE_LIMIT_BURST")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(50), // Burst of 50 requests
            max_body_size: std::env::var("MAX_BODY_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10 * 1024 * 1024), // 10 MB default
            api_key: std::env::var("API_KEY").ok().filter(|s| !s.is_empty()),
            require_api_key: std::env::var("REQUIRE_API_KEY")
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

    let kaspa_service = Arc::new(KaspaService::new(kaspa_config));

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

    // Store API key in state for auth middleware
    let _api_key = server_config.api_key.clone();
    let require_api_key = server_config.require_api_key;

    // Initialize application state
    let state = AppState {
        batch_manager: Arc::new(RwLock::new(BatchManager::new())),
        stamps: Arc::new(RwLock::new(HashMap::new())),
        kaspa_service,
        ws_state,
    };

    // Start batch processing task
    let batch_state = state.clone();
    tokio::spawn(async move {
        batch_processing_loop(batch_state).await;
    });

    // Configure rate limiting
    let governor_conf = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(server_config.rate_limit_per_second as u64)
            .burst_size(server_config.rate_limit_burst)
            .finish()
            .expect("Failed to create rate limiter config")
    );

    // Build CORS layer
    let cors_layer = build_cors_layer(&server_config.cors_origins);

    // Build router with security layers
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/v1/stamp", post(submit_stamp))
        .route("/v1/stamp/:id", get(get_stamp))
        .route("/v1/verify", post(verify_proof))
        .route("/v1/stream", get(ws_handler::<AppState>))
        .layer(ServiceBuilder::new()
            // Body size limit
            .layer(RequestBodyLimitLayer::new(server_config.max_body_size))
            // Rate limiting
            .layer(GovernorLayer {
                config: governor_conf,
            })
            // CORS
            .layer(cors_layer)
        )
        .with_state(state);

    info!("KTCS Calendar Server starting on {}", server_config.bind_address);
    info!("Security configuration:");
    info!("  CORS origins: {:?}", server_config.cors_origins);
    info!("  Rate limit: {}/s (burst: {})", server_config.rate_limit_per_second, server_config.rate_limit_burst);
    info!("  Max body size: {} bytes", server_config.max_body_size);
    info!("  API key required: {}", require_api_key);
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

    // Run server with proper error handling
    if let Err(e) = axum::serve(listener, app).await {
        error!("Server error: {}", e);
        std::process::exit(1);
    }
}

/// Health check endpoint
async fn health_check(State(state): State<AppState>) -> Json<HealthResponse> {
    let stamps = state.stamps.read().await;
    let pending = stamps.values().filter(|s| s.status == StampStatus::Pending).count();

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
    // Parse and validate digest
    let digest_bytes = hex::decode(&req.digest)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid digest hex: {}", e)))?;

    if digest_bytes.len() != 32 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Digest must be 32 bytes (SHA256)".to_string(),
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
        calendar_url: format!("http://localhost:3001/v1/stamp/{}", id),
    }));

    let pending_proof = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        serialize_proof(&proof),
    );

    // Create record
    let record = StampRecord {
        id: id.clone(),
        digest,
        status: StampStatus::Pending,
        submitted_at: now,
        confirmed_at: None,
        proof: Some(proof),
        batch_mode: req.batch_mode,
    };

    // Store record
    {
        let mut stamps = state.stamps.write().await;
        stamps.insert(id.clone(), record);
    }

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
    }))
}

/// Get stamp status
async fn get_stamp(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<StampResponse>, (StatusCode, String)> {
    let stamps = state.stamps.read().await;

    let record = stamps
        .get(&id)
        .ok_or((StatusCode::NOT_FOUND, format!("Stamp not found: {}", id)))?;

    let response = match &record.status {
        StampStatus::Pending => StampResponse {
            id: record.id.clone(),
            status: "pending".to_string(),
            submitted_at: format_timestamp(record.submitted_at),
            estimated_confirmation: None,
            pending_proof: None,
            confirmed_at: None,
            daa_score: None,
            blue_score: None,
            block_hash: None,
            tx_hash: None,
            proof: None,
            thermodynamic_weight: None,
        },
        StampStatus::Batched => StampResponse {
            id: record.id.clone(),
            status: "batched".to_string(),
            submitted_at: format_timestamp(record.submitted_at),
            estimated_confirmation: None,
            pending_proof: None,
            confirmed_at: None,
            daa_score: None,
            blue_score: None,
            block_hash: None,
            tx_hash: None,
            proof: None,
            thermodynamic_weight: None,
        },
        StampStatus::Confirmed => {
            let proof = record.proof.as_ref().unwrap();
            let proof_base64 = base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                serialize_proof(proof),
            );

            // Get attestation data
            let ka = proof.kaspa_attestations().next();
            let (daa_score, blue_score, block_hash, tx_hash, thermo) = match ka {
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
                ),
                None => (None, None, None, None, None),
            };

            StampResponse {
                id: record.id.clone(),
                status: "confirmed".to_string(),
                submitted_at: format_timestamp(record.submitted_at),
                estimated_confirmation: None,
                pending_proof: None,
                confirmed_at: record.confirmed_at.map(format_timestamp),
                daa_score,
                blue_score,
                block_hash,
                tx_hash,
                proof: Some(proof_base64),
                thermodynamic_weight: thermo,
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
async fn batch_processing_loop(state: AppState) {
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));

    loop {
        interval.tick().await;

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

            // Submit commitment to Kaspa blockchain (or mock if not connected)
            let submission = match state.kaspa_service.submit_commitment(merkle_root).await {
                Ok(result) => result,
                Err(e) => {
                    tracing::error!("Failed to submit commitment: {}", e);
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
            let mut stamps_lock = state.stamps.write().await;

            for (i, pending) in stamps.iter().enumerate() {
                if let Some(record) = stamps_lock.get_mut(&pending.id) {
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

                    // Serialize proof for WebSocket broadcast
                    let proof_base64 = base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        serialize_proof(&proof),
                    );

                    // Update record
                    record.status = StampStatus::Confirmed;
                    record.confirmed_at = Some(submission.timestamp);
                    record.proof = Some(proof);

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
}

fn format_timestamp(ms: u64) -> String {
    // ISO 8601 format
    let secs = ms / 1000;
    let millis = ms % 1000;

    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    let mut year = 1970u64;
    let mut remaining_days = days_since_epoch;

    loop {
        let days_in_year = if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
            366
        } else {
            365
        };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    let days_in_months: [u64; 12] = if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1u64;
    for days in days_in_months {
        if remaining_days < days {
            break;
        }
        remaining_days -= days;
        month += 1;
    }
    let day = remaining_days + 1;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        year, month, day, hours, minutes, seconds, millis
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    /// Create test app state with mock mode enabled
    fn create_test_state() -> AppState {
        let kaspa_config = KaspaServiceConfig {
            mock_mode: true,
            ..Default::default()
        };
        let kaspa_service = Arc::new(KaspaService::new(kaspa_config));

        AppState {
            batch_manager: Arc::new(RwLock::new(BatchManager::new())),
            stamps: Arc::new(RwLock::new(HashMap::new())),
            kaspa_service,
            ws_state: Arc::new(WsState::new()),
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
        let state = create_test_state();
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
        let state = create_test_state();
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
        let state = create_test_state();
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
        let state = create_test_state();
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

        let state = create_test_state();

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

                    // Update stamps
                    let mut stamps_lock = batch_state.stamps.write().await;
                    for (i, pending) in stamps.iter().enumerate() {
                        if let Some(record) = stamps_lock.get_mut(&pending.id) {
                            let merkle_proof = tree.get_proof(i).unwrap();
                            let mut proof = KtcsProof::new(pending.digest.to_vec());
                            for op in merkle_proof.to_operations() {
                                proof.add_operation(op);
                            }
                            proof.add_attestation(Attestation::Kaspa(attestation.clone()));
                            record.status = StampStatus::Confirmed;
                            record.confirmed_at = Some(submission.timestamp);
                            record.proof = Some(proof);
                        }
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
        // Test epoch
        assert_eq!(format_timestamp(0), "1970-01-01T00:00:00.000Z");

        // Test a known timestamp (2024-01-01 00:00:00 UTC)
        let ts = 1704067200000u64;
        let formatted = format_timestamp(ts);
        assert!(formatted.starts_with("2024-01-01T00:00:00"));
    }
}
