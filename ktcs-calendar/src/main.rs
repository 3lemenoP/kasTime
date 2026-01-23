//! KTCS Calendar Server
//!
//! Aggregation service that batches timestamp requests and commits them to Kaspa.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

mod services;

use ktcs_core::{
    serialize_proof, Attestation, BatchMode, KaspaAttestation, KtcsProof, MerkleTree,
    PendingAttestation,
};
use services::batch_manager::{BatchManager, PendingStamp};

/// Application state shared across handlers
#[derive(Clone)]
struct AppState {
    batch_manager: Arc<RwLock<BatchManager>>,
    stamps: Arc<RwLock<HashMap<String, StampRecord>>>,
}

/// Record of a submitted stamp
#[derive(Clone, Debug)]
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
enum StampStatus {
    Pending,
    Batched,
    Confirmed,
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
#[derive(Debug, Serialize)]
struct StampResponse {
    id: String,
    status: String,
    submitted_at: String,
    estimated_confirmation: Option<String>,
    pending_proof: Option<String>,
    confirmed_proof: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    attestation: Option<AttestationResponse>,
}

#[derive(Debug, Serialize)]
struct AttestationResponse {
    daa_score: u64,
    blue_score: u64,
    block_hash: String,
    timestamp: u64,
    tx_hash: String,
}

/// API response for verification
#[derive(Debug, Serialize)]
struct VerifyResponse {
    valid: bool,
    digest: String,
    attestations: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Health check response
#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
    version: String,
    pending_stamps: usize,
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

    // Initialize state
    let state = AppState {
        batch_manager: Arc::new(RwLock::new(BatchManager::new())),
        stamps: Arc::new(RwLock::new(HashMap::new())),
    };

    // Start batch processing task
    let batch_state = state.clone();
    tokio::spawn(async move {
        batch_processing_loop(batch_state).await;
    });

    // Build router
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/v1/stamp", post(submit_stamp))
        .route("/v1/stamp/:id", get(get_stamp))
        .route("/v1/verify", post(verify_proof))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state);

    // Get bind address from environment or use default
    let addr = std::env::var("BIND_ADDRESS").unwrap_or_else(|_| "0.0.0.0:3000".to_string());

    info!("KTCS Calendar Server starting on {}", addr);
    info!("Endpoints:");
    info!("  POST /v1/stamp     - Submit a timestamp");
    info!("  GET  /v1/stamp/:id - Get stamp status");
    info!("  POST /v1/verify    - Verify a proof");
    info!("  GET  /health       - Health check");

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
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
    let id = format!("ktcs_{}", uuid::Uuid::new_v4().to_string().replace("-", "")[..16].to_string());

    // Get current timestamp
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    // Create pending proof
    let mut proof = KtcsProof::new(digest.to_vec());
    proof.add_attestation(Attestation::Pending(PendingAttestation {
        calendar_url: format!("http://localhost:3000/v1/stamp/{}", id),
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
        confirmed_proof: None,
        attestation: None,
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

    let (status, confirmed_proof, attestation) = match &record.status {
        StampStatus::Pending => ("pending".to_string(), None, None),
        StampStatus::Batched => ("batched".to_string(), None, None),
        StampStatus::Confirmed => {
            let proof = record.proof.as_ref().unwrap();
            let proof_base64 = base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                serialize_proof(proof),
            );

            let att = proof.kaspa_attestations().next().map(|ka| AttestationResponse {
                daa_score: ka.daa_score,
                blue_score: ka.blue_score,
                block_hash: hex::encode(ka.block_hash),
                timestamp: ka.timestamp,
                tx_hash: hex::encode(ka.tx_hash),
            });

            ("confirmed".to_string(), Some(proof_base64), att)
        }
    };

    Ok(Json(StampResponse {
        id: record.id.clone(),
        status,
        submitted_at: format_timestamp(record.submitted_at),
        estimated_confirmation: None,
        pending_proof: None,
        confirmed_proof,
        attestation,
    }))
}

/// Verify a proof
async fn verify_proof(
    body: axum::body::Bytes,
) -> Result<Json<VerifyResponse>, (StatusCode, String)> {
    let proof = ktcs_core::deserialize_proof(&body)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid proof: {}", e)))?;

    let result = ktcs_core::verify_proof(&proof, None)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Verification error: {}", e)))?;

    Ok(Json(VerifyResponse {
        valid: result.valid,
        digest: result.digest,
        attestations: result
            .attestations
            .iter()
            .map(|a| serde_json::to_value(a).unwrap())
            .collect(),
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

            // Simulate Kaspa transaction (in production, would actually submit)
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;

            let mock_attestation = KaspaAttestation::new(
                42000000 + (now / 100) as u64, // Simulated DAA score
                41500000 + (now / 100) as u64, // Simulated blue score
                merkle_root,                    // Using merkle root as mock block hash
                now,
                merkle_root, // Using merkle root as mock tx hash
                0,
                [0x12; 32], // Mock blue work
                vec![[0x11; 32], [0x22; 32]], // Mock parents
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
                    proof.add_attestation(Attestation::Kaspa(mock_attestation.clone()));

                    // Update record
                    record.status = StampStatus::Confirmed;
                    record.confirmed_at = Some(now);
                    record.proof = Some(proof);

                    info!("Stamp confirmed: {}", pending.id);
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
