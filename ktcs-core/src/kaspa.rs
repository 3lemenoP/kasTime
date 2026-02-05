//! Kaspa RPC client for blockchain integration
//!
//! This module provides a client for interacting with Kaspa nodes via the wRPC/JSON-RPC API.
//! It enables:
//! - Fetching block and transaction information
//! - Submitting OP_RETURN transactions for timestamping
//! - Subscribing to block notifications for real-time confirmations
//! - Querying current chain state (DAA score, blue work, etc.)

use crate::error::{KtcsError, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(feature = "kaspa-client")]
use {
    futures_util::{SinkExt, StreamExt},
    tokio::sync::{mpsc, RwLock, oneshot},
    tokio_tungstenite::{connect_async, tungstenite::Message},
    std::collections::HashMap,
};

#[cfg(not(feature = "kaspa-client"))]
use tokio::sync::{mpsc, RwLock};

// Re-export types from kaspa_types (the canonical WASM-compatible source)
// This allows other modules to import from crate::kaspa for backwards compatibility
pub use crate::kaspa_types::{
    build_commitment_output, BlockEvent, BlockInfo, ConnectionState, DagInfo, ScriptPublicKey,
    Transaction, TransactionInfo, TransactionInput, TransactionOutput, Utxo,
    COMMITMENT_BURN_AMOUNT,
};

/// Default Kaspa mainnet RPC port
pub const DEFAULT_RPC_PORT: u16 = 16110;

/// Default Kaspa testnet RPC port
pub const TESTNET_RPC_PORT: u16 = 16210;

/// Configuration for the Kaspa client
#[derive(Debug, Clone)]
pub struct KaspaClientConfig {
    /// RPC endpoint URL (e.g., "ws://localhost:16110")
    /// If empty and use_resolver is true, will be discovered via resolver
    pub rpc_url: String,
    /// Network name for validation and resolver
    pub network: Option<String>,
    /// Connection timeout in milliseconds
    pub connect_timeout_ms: u64,
    /// Request timeout in milliseconds
    pub request_timeout_ms: u64,
    /// Enable automatic reconnection
    pub auto_reconnect: bool,
    /// Use the PNN resolver to discover public nodes
    pub use_resolver: bool,
    /// Whether to verify TLS certificates (default: true)
    /// Note: Certificate verification is handled by the system TLS implementation.
    /// Set to false only for development with self-signed certificates.
    pub tls_verify: bool,
}

impl Default for KaspaClientConfig {
    fn default() -> Self {
        Self {
            rpc_url: format!("ws://localhost:{}", DEFAULT_RPC_PORT),
            network: None,
            connect_timeout_ms: 10000,
            request_timeout_ms: 30000,
            auto_reconnect: true,
            use_resolver: false,
            tls_verify: true,
        }
    }
}

impl KaspaClientConfig {
    /// Create a config that uses the resolver for testnet-10
    pub fn testnet10_public() -> Self {
        Self {
            rpc_url: String::new(), // Will be resolved
            network: Some("testnet-10".to_string()),
            connect_timeout_ms: 15000,
            request_timeout_ms: 30000,
            auto_reconnect: true,
            use_resolver: true,
            tls_verify: true,
        }
    }

    /// Create a config that uses the resolver for mainnet
    pub fn mainnet_public() -> Self {
        Self {
            rpc_url: String::new(), // Will be resolved
            network: Some("mainnet".to_string()),
            connect_timeout_ms: 15000,
            request_timeout_ms: 30000,
            auto_reconnect: true,
            use_resolver: true,
            tls_verify: true,
        }
    }
}

/// JSON-RPC request structure
#[derive(Debug, Clone, Serialize)]
struct RpcRequest<T: Serialize> {
    jsonrpc: &'static str,
    method: String,
    params: T,
    id: u64,
}

/// JSON-RPC response structure
/// Note: Kaspa wRPC returns results in `params` field, not `result`
#[derive(Debug, Clone, Deserialize)]
struct RpcResponse<T> {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    /// Standard JSON-RPC result field
    result: Option<T>,
    /// Kaspa wRPC returns results in params field
    params: Option<T>,
    error: Option<RpcError>,
    id: u64,
}

/// JSON-RPC error
#[derive(Debug, Clone, Deserialize)]
struct RpcError {
    code: i32,
    message: String,
}

/// Kaspa RPC client for blockchain interaction
///
/// This client provides methods for:
/// - Querying blocks and transactions
/// - Submitting transactions
/// - Subscribing to block events
/// - Getting current chain state
pub struct KaspaClient {
    config: KaspaClientConfig,
    state: Arc<RwLock<ConnectionState>>,
    #[cfg(feature = "kaspa-client")]
    request_id: AtomicU64,
    #[cfg(feature = "kaspa-client")]
    ws_sender: Arc<RwLock<Option<mpsc::Sender<String>>>>,
    #[cfg(feature = "kaspa-client")]
    pending_requests: Arc<RwLock<HashMap<u64, oneshot::Sender<String>>>>,
    #[allow(dead_code)]
    event_sender: Option<mpsc::Sender<BlockEvent>>,
}

impl KaspaClient {
    /// Create a new Kaspa client with the given configuration
    pub fn new(config: KaspaClientConfig) -> Self {
        Self {
            config,
            state: Arc::new(RwLock::new(ConnectionState::Disconnected)),
            #[cfg(feature = "kaspa-client")]
            request_id: AtomicU64::new(1),
            #[cfg(feature = "kaspa-client")]
            ws_sender: Arc::new(RwLock::new(None)),
            #[cfg(feature = "kaspa-client")]
            pending_requests: Arc::new(RwLock::new(HashMap::new())),
            event_sender: None,
        }
    }

    /// Create a client with default configuration for mainnet
    pub fn mainnet() -> Self {
        Self::new(KaspaClientConfig::default())
    }

    /// Create a client with default configuration for testnet
    pub fn testnet() -> Self {
        Self::new(KaspaClientConfig {
            rpc_url: format!("ws://localhost:{}", TESTNET_RPC_PORT),
            network: Some("testnet-11".to_string()),
            ..Default::default()
        })
    }

    /// Connect to the Kaspa node
    ///
    /// If `use_resolver` is enabled in config and no rpc_url is set,
    /// this will use the PNN resolver to discover a public endpoint.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection fails or times out.
    #[cfg(feature = "kaspa-client")]
    pub async fn connect(&self) -> Result<()> {
        {
            let mut state = self.state.write().await;
            *state = ConnectionState::Connecting;
        }

        // Determine the URL to connect to
        let rpc_url = if self.config.use_resolver && self.config.rpc_url.is_empty() {
            // Use resolver to find a public endpoint
            let network = self.config.network.as_deref().unwrap_or("mainnet");
            tracing::info!("Using resolver to find {} endpoint...", network);

            let resolver = crate::resolver::Resolver::default();
            resolver.get_node_url(network).await?
        } else {
            self.config.rpc_url.clone()
        };

        tracing::info!("Connecting to {}", rpc_url);

        // Parse and validate the URL
        let url = url::Url::parse(&rpc_url)
            .map_err(|e| KtcsError::ConnectionError(format!("Invalid RPC URL: {}", e)))?;

        // Connect to the WebSocket
        let connect_timeout = tokio::time::Duration::from_millis(self.config.connect_timeout_ms);

        let ws_stream = tokio::time::timeout(connect_timeout, connect_async(&url))
            .await
            .map_err(|_| KtcsError::ConnectionError("Connection timeout".to_string()))?
            .map_err(|e| KtcsError::ConnectionError(format!("WebSocket connection failed: {}", e)))?
            .0;

        let (mut ws_write, mut ws_read) = ws_stream.split();

        // Create channel for sending messages
        let (tx, mut rx) = mpsc::channel::<String>(32);
        *self.ws_sender.write().await = Some(tx);

        // Clone necessary state for the read/write tasks
        let pending_requests = self.pending_requests.clone();
        let state = self.state.clone();
        let state_clone = self.state.clone();

        // Spawn write task
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if ws_write.send(Message::Text(msg)).await.is_err() {
                    break;
                }
            }
        });

        // Spawn read task
        tokio::spawn(async move {
            while let Some(msg) = ws_read.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        // Try to parse as JSON-RPC response
                        if let Ok(response) = serde_json::from_str::<RpcResponse<serde_json::Value>>(&text) {
                            let mut pending = pending_requests.write().await;
                            if let Some(sender) = pending.remove(&response.id) {
                                let _ = sender.send(text);
                            }
                        }
                    }
                    Ok(Message::Close(_)) => {
                        let mut s = state_clone.write().await;
                        *s = ConnectionState::Disconnected;
                        break;
                    }
                    Err(_) => {
                        let mut s = state_clone.write().await;
                        *s = ConnectionState::Disconnected;
                        break;
                    }
                    _ => {}
                }
            }
        });

        {
            let mut state = state.write().await;
            *state = ConnectionState::Connected;
        }

        Ok(())
    }

    /// Connect to the Kaspa node (stub when kaspa-client feature is disabled)
    #[cfg(not(feature = "kaspa-client"))]
    pub async fn connect(&self) -> Result<()> {
        Err(KtcsError::ConnectionError(
            "Kaspa client feature is not enabled. Rebuild with --features kaspa-client".to_string(),
        ))
    }

    /// Send an RPC request and wait for response
    #[cfg(feature = "kaspa-client")]
    async fn send_request<P: Serialize, R: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        params: P,
    ) -> Result<R> {
        let sender = self.ws_sender.read().await;
        let sender = sender.as_ref().ok_or_else(|| {
            KtcsError::ConnectionError("Not connected to Kaspa node".to_string())
        })?;

        let id = self.request_id.fetch_add(1, Ordering::SeqCst);
        let request = RpcRequest {
            jsonrpc: "2.0",
            method: method.to_string(),
            params,
            id,
        };

        let request_json = serde_json::to_string(&request)
            .map_err(|e| KtcsError::InvalidData(format!("Failed to serialize request: {}", e)))?;

        // Debug: print full request for submitTransaction
        #[cfg(debug_assertions)]
        if method.contains("SubmitTransaction") {
            eprintln!("Full RPC request:\n{}", serde_json::to_string_pretty(&request).unwrap_or_default());
        }

        // Create response channel
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending_requests.write().await;
            pending.insert(id, tx);
        }

        // Send request
        sender.send(request_json).await.map_err(|_| {
            KtcsError::ConnectionError("Failed to send request".to_string())
        })?;

        // Wait for response with timeout
        let timeout = tokio::time::Duration::from_millis(self.config.request_timeout_ms);
        let response_text = tokio::time::timeout(timeout, rx)
            .await
            .map_err(|_| KtcsError::ConnectionError("Request timeout".to_string()))?
            .map_err(|_| KtcsError::ConnectionError("Request cancelled".to_string()))?;

        // Parse response
        let response: RpcResponse<R> = serde_json::from_str(&response_text)
            .map_err(|e| KtcsError::InvalidData(format!("Failed to parse response: {}", e)))?;

        if let Some(error) = response.error {
            return Err(KtcsError::ConnectionError(format!(
                "RPC error {}: {}",
                error.code, error.message
            )));
        }

        // Kaspa wRPC returns results in `params` field, standard JSON-RPC uses `result`
        response.result.or(response.params).ok_or_else(|| {
            KtcsError::InvalidData("Empty response from Kaspa node".to_string())
        })
    }

    /// Disconnect from the Kaspa node
    pub async fn disconnect(&self) -> Result<()> {
        let mut state = self.state.write().await;
        *state = ConnectionState::Disconnected;
        Ok(())
    }

    /// Get the current connection state
    pub async fn connection_state(&self) -> ConnectionState {
        *self.state.read().await
    }

    /// Check if connected
    pub async fn is_connected(&self) -> bool {
        *self.state.read().await == ConnectionState::Connected
    }

    // ========================================
    // Block Operations
    // ========================================

    /// Get a block by its hash
    ///
    /// # Arguments
    ///
    /// * `hash` - The 32-byte block hash
    ///
    /// # Errors
    ///
    /// Returns an error if the block is not found or the request fails.
    #[cfg(feature = "kaspa-client")]
    pub async fn get_block_by_hash(&self, hash: &[u8; 32]) -> Result<BlockInfo> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct BlockHeader {
            hash: String,
            daa_score: u64,
            blue_score: u64,
            blue_work: String,
            timestamp: u64,
            /// Parents field - Kaspa wRPC returns array of arrays of hash strings
            /// Format: [["hash1", "hash2"], ["hash3"]] where each inner array is a level
            #[serde(default)]
            parents: Vec<Vec<String>>,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct BlockResponse {
            block: BlockData,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct BlockData {
            header: BlockHeader,
            verbose_data: Option<VerboseData>,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct VerboseData {
            is_chain_block: bool,
            transaction_ids: Vec<String>,
        }

        let hash_hex = hex::encode(hash);
        let params = serde_json::json!({
            "hash": hash_hex,
            "includeTransactions": false
        });

        let response: BlockResponse = self.send_request("getBlock", params).await?;

        let block = response.block;
        let header = block.header;

        // Parse block hash
        let block_hash_bytes = hex::decode(&header.hash)
            .map_err(|e| KtcsError::InvalidData(format!("Invalid block hash: {}", e)))?;
        let mut block_hash = [0u8; 32];
        if block_hash_bytes.len() == 32 {
            block_hash.copy_from_slice(&block_hash_bytes);
        }

        // Parse blue work
        let blue_work_bytes = hex::decode(&header.blue_work).unwrap_or_else(|_| vec![0u8; 32]);
        let mut blue_work = [0u8; 32];
        let start = 32usize.saturating_sub(blue_work_bytes.len());
        blue_work[start..].copy_from_slice(&blue_work_bytes[..32.min(blue_work_bytes.len())]);

        // Parse parent hashes (first level only)
        // Format is Vec<Vec<String>> where each inner vec is a level
        let parent_hashes: Vec<[u8; 32]> = header
            .parents
            .first()
            .map(|level| {
                level
                    .iter()
                    .filter_map(|h| {
                        let bytes = hex::decode(h).ok()?;
                        if bytes.len() == 32 {
                            let mut arr = [0u8; 32];
                            arr.copy_from_slice(&bytes);
                            Some(arr)
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Parse transaction IDs
        let transaction_ids: Vec<[u8; 32]> = block
            .verbose_data
            .as_ref()
            .map(|v| {
                v.transaction_ids
                    .iter()
                    .filter_map(|t| {
                        let bytes = hex::decode(t).ok()?;
                        if bytes.len() == 32 {
                            let mut arr = [0u8; 32];
                            arr.copy_from_slice(&bytes);
                            Some(arr)
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(BlockInfo {
            hash: block_hash,
            daa_score: header.daa_score,
            blue_score: header.blue_score,
            blue_work,
            timestamp: header.timestamp,
            parent_hashes,
            transaction_ids,
            is_chain_block: block.verbose_data.map(|v| v.is_chain_block).unwrap_or(false),
        })
    }

    #[cfg(not(feature = "kaspa-client"))]
    pub async fn get_block_by_hash(&self, hash: &[u8; 32]) -> Result<BlockInfo> {
        Err(KtcsError::BlockNotFound(hex::encode(hash)))
    }

    /// Get a block with full transaction data (including payloads)
    ///
    /// This is useful for verifying commitments in transaction payloads.
    #[cfg(feature = "kaspa-client")]
    pub async fn get_block_with_transactions(&self, hash: &[u8; 32]) -> Result<(BlockInfo, Vec<TransactionInfo>)> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct BlockHeader {
            hash: String,
            daa_score: u64,
            blue_score: u64,
            blue_work: String,
            timestamp: u64,
            #[serde(default)]
            parents: Vec<Vec<String>>,
        }

        // Script public key can be a string or an object depending on RPC version
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum RpcScriptPublicKey {
            String(String),
            Object { script_public_key: String },
        }

        impl RpcScriptPublicKey {
            fn as_hex(&self) -> &str {
                match self {
                    RpcScriptPublicKey::String(s) => s,
                    RpcScriptPublicKey::Object { script_public_key } => script_public_key,
                }
            }
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RpcTransactionOutput {
            #[serde(alias = "amount")]
            value: u64,
            script_public_key: RpcScriptPublicKey,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RpcOutpoint {
            transaction_id: String,
            index: u32,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RpcTransactionInput {
            previous_outpoint: RpcOutpoint,
            #[serde(default)]
            signature_script: String,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RpcTransaction {
            #[serde(default)]
            payload: String,
            #[serde(default)]
            outputs: Vec<RpcTransactionOutput>,
            #[serde(default)]
            inputs: Vec<RpcTransactionInput>,
            verbose_data: Option<RpcTxVerboseData>,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RpcTxVerboseData {
            transaction_id: String,
            #[serde(default)]
            block_hash: String,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct BlockData {
            header: BlockHeader,
            transactions: Option<Vec<RpcTransaction>>,
            verbose_data: Option<VerboseData>,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct VerboseData {
            is_chain_block: bool,
            transaction_ids: Vec<String>,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct BlockResponse {
            block: BlockData,
        }

        let hash_hex = hex::encode(hash);
        let params = serde_json::json!({
            "hash": hash_hex,
            "includeTransactions": true
        });

        let response: BlockResponse = self.send_request("getBlock", params).await?;

        let block = response.block;
        let header = block.header;

        // Parse block hash
        let block_hash_bytes = hex::decode(&header.hash)
            .map_err(|e| KtcsError::InvalidData(format!("Invalid block hash: {}", e)))?;
        let mut block_hash = [0u8; 32];
        if block_hash_bytes.len() == 32 {
            block_hash.copy_from_slice(&block_hash_bytes);
        }

        // Parse blue work
        let blue_work_bytes = hex::decode(&header.blue_work).unwrap_or_else(|_| vec![0u8; 32]);
        let mut blue_work = [0u8; 32];
        let start = 32usize.saturating_sub(blue_work_bytes.len());
        blue_work[start..].copy_from_slice(&blue_work_bytes[..32.min(blue_work_bytes.len())]);

        // Parse parent hashes
        let parent_hashes: Vec<[u8; 32]> = header
            .parents
            .first()
            .map(|level| {
                level.iter().filter_map(|h| {
                    let bytes = hex::decode(h).ok()?;
                    if bytes.len() == 32 {
                        let mut arr = [0u8; 32];
                        arr.copy_from_slice(&bytes);
                        Some(arr)
                    } else {
                        None
                    }
                }).collect()
            })
            .unwrap_or_default();

        // Parse transaction IDs
        let transaction_ids: Vec<[u8; 32]> = block.verbose_data.as_ref()
            .map(|v| {
                v.transaction_ids.iter().filter_map(|t| {
                    let bytes = hex::decode(t).ok()?;
                    if bytes.len() == 32 {
                        let mut arr = [0u8; 32];
                        arr.copy_from_slice(&bytes);
                        Some(arr)
                    } else {
                        None
                    }
                }).collect()
            })
            .unwrap_or_default();

        let block_info = BlockInfo {
            hash: block_hash,
            daa_score: header.daa_score,
            blue_score: header.blue_score,
            blue_work,
            timestamp: header.timestamp,
            parent_hashes,
            transaction_ids,
            is_chain_block: block.verbose_data.map(|v| v.is_chain_block).unwrap_or(false),
        };

        // Parse transactions
        let transactions: Vec<TransactionInfo> = block.transactions
            .unwrap_or_default()
            .into_iter()
            .filter_map(|tx| {
                let tx_hash = tx.verbose_data.as_ref()
                    .and_then(|v| hex::decode(&v.transaction_id).ok())
                    .and_then(|bytes| {
                        if bytes.len() == 32 {
                            let mut arr = [0u8; 32];
                            arr.copy_from_slice(&bytes);
                            Some(arr)
                        } else {
                            None
                        }
                    })?;

                let block_hash = tx.verbose_data.as_ref()
                    .and_then(|v| hex::decode(&v.block_hash).ok())
                    .and_then(|bytes| {
                        if bytes.len() == 32 {
                            let mut arr = [0u8; 32];
                            arr.copy_from_slice(&bytes);
                            Some(arr)
                        } else {
                            None
                        }
                    });

                let outputs: Vec<TransactionOutput> = tx.outputs.into_iter().map(|o| {
                    let script_bytes = hex::decode(o.script_public_key.as_hex()).unwrap_or_default();
                    TransactionOutput {
                        amount: o.value,
                        script_public_key: ScriptPublicKey {
                            version: 0,
                            script: script_bytes,
                        },
                    }
                }).collect();

                let inputs: Vec<TransactionInput> = tx.inputs.into_iter().map(|i| {
                    let prev_hash = hex::decode(&i.previous_outpoint.transaction_id)
                        .ok()
                        .and_then(|v| {
                            if v.len() == 32 {
                                let mut arr = [0u8; 32];
                                arr.copy_from_slice(&v);
                                Some(arr)
                            } else {
                                None
                            }
                        })
                        .unwrap_or([0u8; 32]);

                    let sig_script = hex::decode(&i.signature_script).unwrap_or_default();

                    TransactionInput {
                        previous_outpoint_hash: prev_hash,
                        previous_outpoint_index: i.previous_outpoint.index,
                        signature_script: sig_script,
                    }
                }).collect();

                Some(TransactionInfo {
                    hash: tx_hash,
                    block_hash,
                    outputs,
                    inputs,
                    is_accepted: true,
                })
            })
            .collect();

        Ok((block_info, transactions))
    }

    #[cfg(not(feature = "kaspa-client"))]
    pub async fn get_block_with_transactions(&self, hash: &[u8; 32]) -> Result<(BlockInfo, Vec<TransactionInfo>)> {
        Err(KtcsError::BlockNotFound(hex::encode(hash)))
    }

    /// Get the current DAG information
    ///
    /// Returns information about the current state of the BlockDAG including
    /// DAA score, blue score, blue work, and tip hashes.
    #[cfg(feature = "kaspa-client")]
    pub async fn get_block_dag_info(&self) -> Result<DagInfo> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct DagInfoResponse {
            /// Network name (e.g., "mainnet", "testnet-10")
            network: String,
            /// Virtual DAA score
            virtual_daa_score: u64,
            /// Blue score (may not be present in all responses)
            #[serde(default)]
            blue_score: u64,
            /// Current difficulty
            difficulty: f64,
            /// Past median time in milliseconds
            past_median_time: u64,
            /// Pruning point hash (hex string)
            pruning_point_hash: String,
            /// Virtual parent hashes (tip hashes)
            virtual_parent_hashes: Vec<String>,
        }

        let response: DagInfoResponse = self.send_request("getBlockDagInfo", serde_json::json!({})).await?;

        // Parse tip hashes
        let tip_hashes: Vec<[u8; 32]> = response
            .virtual_parent_hashes
            .iter()
            .filter_map(|h| {
                let bytes = hex::decode(h).ok()?;
                if bytes.len() == 32 {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&bytes);
                    Some(arr)
                } else {
                    None
                }
            })
            .collect();

        // Parse pruning point hash
        let pruning_point_bytes = hex::decode(&response.pruning_point_hash)
            .map_err(|e| KtcsError::InvalidData(format!("Invalid pruning point hash: {}", e)))?;
        let mut pruning_point_hash = [0u8; 32];
        if pruning_point_bytes.len() == 32 {
            pruning_point_hash.copy_from_slice(&pruning_point_bytes);
        }

        Ok(DagInfo {
            network: response.network,
            current_daa_score: response.virtual_daa_score,
            current_blue_score: response.blue_score,
            current_blue_work: [0u8; 32], // Will be fetched separately if needed
            tip_hashes,
            difficulty: response.difficulty,
            past_median_time: response.past_median_time,
            pruning_point_hash,
        })
    }

    #[cfg(not(feature = "kaspa-client"))]
    pub async fn get_block_dag_info(&self) -> Result<DagInfo> {
        Err(KtcsError::ConnectionError(
            "Kaspa client feature is not enabled".to_string(),
        ))
    }

    /// Get the current DAA score (virtual block height)
    pub async fn get_current_daa_score(&self) -> Result<u64> {
        let dag_info = self.get_block_dag_info().await?;
        Ok(dag_info.current_daa_score)
    }

    /// Get the current blue score
    pub async fn get_current_blue_score(&self) -> Result<u64> {
        let dag_info = self.get_block_dag_info().await?;
        Ok(dag_info.current_blue_score)
    }

    /// Get the current cumulative blue work
    pub async fn get_current_blue_work(&self) -> Result<[u8; 32]> {
        let dag_info = self.get_block_dag_info().await?;
        Ok(dag_info.current_blue_work)
    }

    /// Get the virtual selected parent chain blue score
    pub async fn get_virtual_chain_blue_score(&self) -> Result<u64> {
        self.get_current_blue_score().await
    }

    // ========================================
    // Transaction Operations
    // ========================================

    /// Get a transaction by its hash
    ///
    /// # Arguments
    ///
    /// * `hash` - The 32-byte transaction hash
    ///
    /// # Errors
    ///
    /// Returns an error if the transaction is not found or the request fails.
    #[cfg(feature = "kaspa-client")]
    pub async fn get_transaction(&self, hash: &[u8; 32]) -> Result<TransactionInfo> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RpcScriptPublicKey {
            script_public_key: String,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RpcTransactionOutput {
            amount: u64,
            script_public_key: RpcScriptPublicKey,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RpcOutpoint {
            transaction_id: String,
            index: u32,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RpcTransactionInput {
            previous_outpoint: RpcOutpoint,
            signature_script: String,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RpcTransaction {
            #[allow(dead_code)]
            version: u16,
            inputs: Vec<RpcTransactionInput>,
            outputs: Vec<RpcTransactionOutput>,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct GetTransactionResponse {
            transaction: RpcTransaction,
            block_hash: Option<String>,
        }

        let tx_id = hex::encode(hash);
        let params = serde_json::json!({ "transactionId": tx_id });

        match self.send_request::<_, GetTransactionResponse>("getTransaction", params).await {
            Ok(response) => {
                // Parse block_hash if present
                let block_hash = response.block_hash.as_ref().and_then(|h| {
                    hex::decode(h).ok().and_then(|v| {
                        if v.len() == 32 {
                            let mut arr = [0u8; 32];
                            arr.copy_from_slice(&v);
                            Some(arr)
                        } else {
                            None
                        }
                    })
                });

                // Convert outputs
                let outputs: Vec<TransactionOutput> = response.transaction.outputs
                    .into_iter()
                    .map(|o| {
                        let script_bytes = hex::decode(&o.script_public_key.script_public_key)
                            .unwrap_or_default();
                        TransactionOutput {
                            amount: o.amount,
                            script_public_key: ScriptPublicKey {
                                version: 0,
                                script: script_bytes,
                            },
                        }
                    })
                    .collect();

                // Convert inputs
                let inputs: Vec<TransactionInput> = response.transaction.inputs
                    .into_iter()
                    .map(|i| {
                        let prev_hash = hex::decode(&i.previous_outpoint.transaction_id)
                            .ok()
                            .and_then(|v| {
                                if v.len() == 32 {
                                    let mut arr = [0u8; 32];
                                    arr.copy_from_slice(&v);
                                    Some(arr)
                                } else {
                                    None
                                }
                            })
                            .unwrap_or([0u8; 32]);

                        let sig_script = hex::decode(&i.signature_script).unwrap_or_default();

                        TransactionInput {
                            previous_outpoint_hash: prev_hash,
                            previous_outpoint_index: i.previous_outpoint.index,
                            signature_script: sig_script,
                        }
                    })
                    .collect();

                Ok(TransactionInfo {
                    hash: *hash,
                    block_hash,
                    outputs,
                    inputs,
                    is_accepted: block_hash.is_some(),
                })
            }
            Err(KtcsError::ConnectionError(msg)) if msg.contains("not found") => {
                Err(KtcsError::TransactionNotFound(tx_id))
            }
            Err(e) => Err(e),
        }
    }

    #[cfg(not(feature = "kaspa-client"))]
    pub async fn get_transaction(&self, hash: &[u8; 32]) -> Result<TransactionInfo> {
        Err(KtcsError::ConnectionError(format!(
            "Kaspa client feature not enabled, cannot get transaction {}",
            hex::encode(hash)
        )))
    }

    /// Check if a transaction is in the mempool (pending)
    ///
    /// # Arguments
    ///
    /// * `tx_hash` - The transaction hash to check
    ///
    /// # Returns
    ///
    /// `true` if the transaction is in the mempool, `false` otherwise.
    #[cfg(feature = "kaspa-client")]
    pub async fn is_transaction_in_mempool(&self, tx_hash: &[u8; 32]) -> Result<bool> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct MempoolEntryResponse {
            #[allow(dead_code)]
            is_orphan: bool,
        }

        let tx_id_hex = hex::encode(tx_hash);
        let params = serde_json::json!({
            "txId": tx_id_hex,
            "includeOrphanPool": false,
            "filterTransactionPool": true
        });

        match self.send_request::<_, MempoolEntryResponse>("getMempoolEntry", params).await {
            Ok(_) => Ok(true),
            Err(KtcsError::ConnectionError(msg)) if msg.contains("transaction is not in the pool") => {
                Ok(false)
            }
            Err(e) => Err(e),
        }
    }

    #[cfg(not(feature = "kaspa-client"))]
    pub async fn is_transaction_in_mempool(&self, _tx_hash: &[u8; 32]) -> Result<bool> {
        Err(KtcsError::ConnectionError(
            "Kaspa client feature is not enabled".to_string(),
        ))
    }

    /// Submit a transaction to the network
    ///
    /// # Arguments
    ///
    /// * `tx` - The transaction to submit
    ///
    /// # Returns
    ///
    /// The transaction hash on success.
    ///
    /// # Errors
    ///
    /// Returns an error if the transaction is rejected or the request fails.
    #[cfg(feature = "kaspa-client")]
    pub async fn submit_transaction(&self, tx: Transaction) -> Result<[u8; 32]> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct TxInput {
            previous_outpoint: TxOutpoint,
            signature_script: String,
            sequence: u64,
            sig_op_count: u8,
        }

        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct TxOutpoint {
            transaction_id: String,
            index: u32,
        }

        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct TxOutput {
            value: u64,
            /// ScriptPublicKey is a hex string: version (2 bytes BE) + script concatenated
            script_public_key: String,
        }

        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct SubmitTx {
            version: u16,
            inputs: Vec<TxInput>,
            outputs: Vec<TxOutput>,
            lock_time: u64,
            subnetwork_id: String,
            gas: u64,
            payload: String,
            mass: u64,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct SubmitResponse {
            transaction_id: String,
        }

        // Convert transaction to RPC format
        let inputs: Vec<TxInput> = tx
            .inputs
            .iter()
            .map(|i| TxInput {
                previous_outpoint: TxOutpoint {
                    transaction_id: hex::encode(i.previous_outpoint_hash),
                    index: i.previous_outpoint_index,
                },
                signature_script: hex::encode(&i.signature_script),
                sequence: u64::MAX, // Kaspa default sequence
                sig_op_count: 1,
            })
            .collect();

        let outputs: Vec<TxOutput> = tx
            .outputs
            .iter()
            .map(|o| {
                // ScriptPublicKey is encoded as: version (2 bytes BE) + script
                let mut spk_bytes = Vec::with_capacity(2 + o.script_public_key.script.len());
                spk_bytes.extend_from_slice(&o.script_public_key.version.to_be_bytes());
                spk_bytes.extend_from_slice(&o.script_public_key.script);
                TxOutput {
                    value: o.amount,
                    script_public_key: hex::encode(spk_bytes),
                }
            })
            .collect();

        let submit_tx = SubmitTx {
            version: tx.version,
            inputs,
            outputs,
            lock_time: tx.lock_time,
            subnetwork_id: hex::encode(tx.subnetwork_id),
            gas: tx.gas,
            payload: hex::encode(&tx.payload),
            mass: 0, // Node will calculate actual mass
        };

        let params = serde_json::json!({
            "transaction": submit_tx,
            "allowOrphan": false
        });

        // Debug: print the request JSON
        #[cfg(debug_assertions)]
        eprintln!("submitTransaction params:\n{}", serde_json::to_string_pretty(&params).unwrap_or_default());

        let response: SubmitResponse = self.send_request("submitTransaction", params).await?;

        // Parse transaction ID
        let tx_id_bytes = hex::decode(&response.transaction_id)
            .map_err(|e| KtcsError::InvalidData(format!("Invalid transaction ID: {}", e)))?;
        let mut tx_hash = [0u8; 32];
        if tx_id_bytes.len() == 32 {
            tx_hash.copy_from_slice(&tx_id_bytes);
        }

        Ok(tx_hash)
    }

    #[cfg(not(feature = "kaspa-client"))]
    pub async fn submit_transaction(&self, _tx: Transaction) -> Result<[u8; 32]> {
        Err(KtcsError::ConnectionError(
            "Kaspa client feature is not enabled".to_string(),
        ))
    }

    /// Get UTXOs for an address
    ///
    /// # Arguments
    ///
    /// * `address` - The Kaspa address (e.g., "kaspa:qz...")
    ///
    /// # Returns
    ///
    /// A list of unspent transaction outputs for the address.
    #[cfg(feature = "kaspa-client")]
    pub async fn get_utxos_by_address(&self, address: &str) -> Result<Vec<Utxo>> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct UtxoResponse {
            entries: Vec<UtxoEntry>,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct UtxoEntry {
            outpoint: UtxoOutpoint,
            utxo_entry: UtxoData,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct UtxoOutpoint {
            transaction_id: String,
            index: u32,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct UtxoData {
            amount: u64,
            /// Script public key - Kaspa wRPC returns this as a plain hex string
            script_public_key: String,
            block_daa_score: u64,
            is_coinbase: bool,
        }

        let params = serde_json::json!({
            "addresses": [address]
        });

        let response: UtxoResponse = self.send_request("getUtxosByAddresses", params).await?;

        let utxos: Vec<Utxo> = response
            .entries
            .into_iter()
            .filter_map(|entry| {
                let tx_id_bytes = hex::decode(&entry.outpoint.transaction_id).ok()?;
                if tx_id_bytes.len() != 32 {
                    return None;
                }
                let mut transaction_id = [0u8; 32];
                transaction_id.copy_from_slice(&tx_id_bytes);

                let script_bytes = hex::decode(&entry.utxo_entry.script_public_key).ok()?;

                // Extract version from script (first 2 bytes if present, otherwise 0)
                let (version, script) = if script_bytes.len() >= 2 {
                    // Kaspa script format: 2-byte version prefix + script data
                    let ver = u16::from_le_bytes([script_bytes[0], script_bytes[1]]);
                    (ver, script_bytes[2..].to_vec())
                } else {
                    (0, script_bytes)
                };

                Some(Utxo {
                    transaction_id,
                    index: entry.outpoint.index,
                    amount: entry.utxo_entry.amount,
                    script_public_key: ScriptPublicKey {
                        version,
                        script,
                    },
                    block_daa_score: entry.utxo_entry.block_daa_score,
                    is_coinbase: entry.utxo_entry.is_coinbase,
                })
            })
            .collect();

        Ok(utxos)
    }

    #[cfg(not(feature = "kaspa-client"))]
    pub async fn get_utxos_by_address(&self, _address: &str) -> Result<Vec<Utxo>> {
        Err(KtcsError::ConnectionError(
            "Kaspa client feature is not enabled".to_string(),
        ))
    }

    // ========================================
    // Subscription Operations
    // ========================================

    /// Subscribe to block added notifications
    ///
    /// Registers for block added notifications from the Kaspa node.
    ///
    /// # Arguments
    ///
    /// * `sender` - Channel to receive block events
    ///
    /// # Errors
    ///
    /// Returns an error if the subscription fails or if not connected.
    ///
    /// # Note
    ///
    /// This sends the subscription request to the Kaspa node.
    /// The event_sender field stores the channel but notification delivery
    /// requires handling notifications in the WebSocket receive loop.
    #[cfg(feature = "kaspa-client")]
    pub async fn subscribe_to_block_added(
        &self,
        _sender: mpsc::Sender<BlockEvent>,
    ) -> Result<()> {
        let state = self.state.read().await;
        if !matches!(*state, ConnectionState::Connected) {
            return Err(KtcsError::ConnectionError(
                "Not connected to Kaspa node".to_string(),
            ));
        }
        drop(state);

        // Send subscription request
        let params = serde_json::json!({});
        let _response: serde_json::Value = self
            .send_request("notifyBlockAdded", params)
            .await?;

        // Note: To fully implement notifications, we would need to:
        // 1. Store the sender in a subscription registry
        // 2. Modify the WebSocket receive loop to route notifications
        // 3. Parse notification messages and send to subscribers
        tracing::info!("Subscribed to block added notifications");
        Ok(())
    }

    #[cfg(not(feature = "kaspa-client"))]
    pub async fn subscribe_to_block_added(
        &self,
        _sender: mpsc::Sender<BlockEvent>,
    ) -> Result<()> {
        Err(KtcsError::ConnectionError(
            "Kaspa client feature is not enabled".to_string(),
        ))
    }

    /// Subscribe to virtual selected parent chain changes
    ///
    /// This is useful for tracking transaction confirmations.
    ///
    /// # Arguments
    ///
    /// * `sender` - Channel to receive chain change events
    ///
    /// # Errors
    ///
    /// Returns an error if the subscription fails or if not connected.
    #[cfg(feature = "kaspa-client")]
    pub async fn subscribe_to_virtual_chain_changed(
        &self,
        _sender: mpsc::Sender<BlockEvent>,
    ) -> Result<()> {
        let state = self.state.read().await;
        if !matches!(*state, ConnectionState::Connected) {
            return Err(KtcsError::ConnectionError(
                "Not connected to Kaspa node".to_string(),
            ));
        }
        drop(state);

        // Send subscription request
        let params = serde_json::json!({
            "includeAcceptedTransactionIds": true
        });
        let _response: serde_json::Value = self
            .send_request("notifyVirtualSelectedParentChainChanged", params)
            .await?;

        tracing::info!("Subscribed to virtual chain changed notifications");
        Ok(())
    }

    #[cfg(not(feature = "kaspa-client"))]
    pub async fn subscribe_to_virtual_chain_changed(
        &self,
        _sender: mpsc::Sender<BlockEvent>,
    ) -> Result<()> {
        Err(KtcsError::ConnectionError(
            "Kaspa client feature is not enabled".to_string(),
        ))
    }

    /// Unsubscribe from all notifications
    ///
    /// Sends unsubscribe requests to the Kaspa node for all active subscriptions.
    #[cfg(feature = "kaspa-client")]
    pub async fn unsubscribe_all(&self) -> Result<()> {
        let state = self.state.read().await;
        if !matches!(*state, ConnectionState::Connected) {
            return Ok(()); // Nothing to unsubscribe if not connected
        }
        drop(state);

        // Unsubscribe from block added
        let params = serde_json::json!({});
        let _ = self.send_request::<_, serde_json::Value>("notifyBlockAdded", params).await;

        // Unsubscribe from virtual chain changes
        let params = serde_json::json!({});
        let _ = self.send_request::<_, serde_json::Value>("notifyVirtualSelectedParentChainChanged", params).await;

        tracing::info!("Unsubscribed from all notifications");
        Ok(())
    }

    #[cfg(not(feature = "kaspa-client"))]
    pub async fn unsubscribe_all(&self) -> Result<()> {
        Ok(())
    }

    // ========================================
    // Utility Methods
    // ========================================

    /// Wait for a transaction to be confirmed in a block
    ///
    /// # Arguments
    ///
    /// * `tx_hash` - The transaction hash to wait for
    /// * `timeout_ms` - Maximum time to wait in milliseconds
    ///
    /// # Returns
    ///
    /// The block info containing the transaction.
    #[cfg(feature = "kaspa-client")]
    pub async fn wait_for_confirmation(
        &self,
        tx_hash: &[u8; 32],
        timeout_ms: u64,
    ) -> Result<BlockInfo> {
        use std::time::{Duration, Instant};
        use std::collections::HashSet;

        let start = Instant::now();
        let timeout = Duration::from_millis(timeout_ms);
        let poll_interval = Duration::from_millis(100); // Poll every 100ms

        // Track blocks we've already checked to avoid re-checking
        let mut checked_blocks: HashSet<[u8; 32]> = HashSet::new();

        tracing::info!("Waiting for transaction {} confirmation (timeout: {}ms)",
            hex::encode(tx_hash), timeout_ms);

        loop {
            if start.elapsed() > timeout {
                tracing::warn!("Transaction confirmation timeout after {}ms", timeout_ms);
                return Err(KtcsError::Other("Transaction confirmation timeout".to_string()));
            }

            // Get current DAG state
            let dag_info = match self.get_block_dag_info().await {
                Ok(info) => info,
                Err(e) => {
                    tracing::debug!("Failed to get DAG info: {}, retrying...", e);
                    tokio::time::sleep(poll_interval).await;
                    continue;
                }
            };

            // Check each tip block for our transaction
            for tip_hash in &dag_info.tip_hashes {
                // Skip if we've already checked this block
                if checked_blocks.contains(tip_hash) {
                    continue;
                }

                // Fetch the block with transaction IDs
                match self.get_block_by_hash(tip_hash).await {
                    Ok(block) => {
                        checked_blocks.insert(*tip_hash);

                        // Check if our transaction is in this block
                        if block.transaction_ids.iter().any(|id| id == tx_hash) {
                            // Security: Basic sanity check - block timestamp should be reasonable
                            let now_ms = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_millis() as u64;

                            // Block timestamp should not be more than 1 hour in the future
                            if block.timestamp > now_ms + 3_600_000 {
                                tracing::warn!(
                                    "Block {} has timestamp far in future ({} vs now {}), skipping",
                                    hex::encode(block.hash),
                                    block.timestamp,
                                    now_ms
                                );
                                continue;
                            }

                            tracing::info!(
                                "Transaction {} confirmed in block {} at DAA score {}",
                                hex::encode(tx_hash),
                                hex::encode(block.hash),
                                block.daa_score
                            );
                            return Ok(block);
                        }

                        // Also check parent blocks (transaction might be in a recent parent)
                        for parent_hash in &block.parent_hashes {
                            if checked_blocks.contains(parent_hash) {
                                continue;
                            }

                            if let Ok(parent_block) = self.get_block_by_hash(parent_hash).await {
                                checked_blocks.insert(*parent_hash);

                                if parent_block.transaction_ids.iter().any(|id| id == tx_hash) {
                                    tracing::info!(
                                        "Transaction {} confirmed in parent block {} at DAA score {}",
                                        hex::encode(tx_hash),
                                        hex::encode(parent_block.hash),
                                        parent_block.daa_score
                                    );
                                    return Ok(parent_block);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::debug!("Failed to get block {}: {}", hex::encode(tip_hash), e);
                    }
                }
            }

            // Wait before next poll
            tokio::time::sleep(poll_interval).await;
        }
    }

    #[cfg(not(feature = "kaspa-client"))]
    pub async fn wait_for_confirmation(
        &self,
        tx_hash: &[u8; 32],
        _timeout_ms: u64,
    ) -> Result<BlockInfo> {
        Err(KtcsError::ConnectionError(
            format!("Kaspa client feature is not enabled. Cannot wait for transaction {}", hex::encode(tx_hash)),
        ))
    }

    /// Get the configuration
    pub fn config(&self) -> &KaspaClientConfig {
        &self.config
    }
}

// ========================================
// Transaction Building Utilities
// ========================================

// Note: build_commitment_output is imported from kaspa_types

/// Build an OP_RETURN output for a 32-byte commitment (DEPRECATED)
///
/// NOTE: Kaspa does NOT support OP_RETURN as a standard script type.
/// This function is kept for compatibility but will be rejected by the network.
/// Use `build_commitment_output` instead.
#[deprecated(since = "0.1.0", note = "Kaspa does not support OP_RETURN. Use build_commitment_output instead.")]
pub fn build_op_return_output(commitment: &[u8; 32]) -> TransactionOutput {
    // OP_RETURN script: 0x6a (OP_RETURN) 0x20 (push 32 bytes) <32 bytes>
    let mut script = Vec::with_capacity(34);
    script.push(0x6a); // OP_RETURN
    script.push(0x20); // Push 32 bytes
    script.extend_from_slice(commitment);

    TransactionOutput {
        amount: 0, // OP_RETURN outputs have 0 value
        script_public_key: ScriptPublicKey {
            version: 0,
            script,
        },
    }
}

/// Extract commitment from a transaction if it contains an OP_RETURN
pub fn extract_commitment_from_tx(tx: &TransactionInfo) -> Option<[u8; 32]> {
    for output in &tx.outputs {
        if let Some(data) = output.script_public_key.get_op_return_data() {
            if data.len() == 32 {
                let mut commitment = [0u8; 32];
                commitment.copy_from_slice(&data);
                return Some(commitment);
            }
        }
    }
    None
}

/// Calculate transaction fee given inputs and outputs
pub fn calculate_fee(inputs: &[Utxo], outputs: &[TransactionOutput]) -> i64 {
    let input_sum: u64 = inputs.iter().map(|u| u.amount).sum();
    let output_sum: u64 = outputs.iter().map(|o| o.amount).sum();
    input_sum as i64 - output_sum as i64
}

/// Estimate transaction size for fee calculation
///
/// Approximate sizes:
/// - Base: 10 bytes
/// - Input: ~148 bytes (P2PKH)
/// - Output: ~34 bytes (P2PKH) or ~43 bytes (OP_RETURN with 32 bytes)
pub fn estimate_tx_size(num_inputs: usize, num_outputs: usize, has_op_return: bool) -> usize {
    let base = 10;
    let inputs = num_inputs * 148;
    let outputs = num_outputs * 34;
    let op_return = if has_op_return { 43 } else { 0 };
    base + inputs + outputs + op_return
}

// ========================================
// Blue Work Arithmetic
// ========================================

/// Subtract two 256-bit big-endian values: result = a - b
///
/// Returns the difference as a 256-bit big-endian byte array.
/// If b > a, returns zero (no negative values).
pub fn subtract_blue_work(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let mut result = [0u8; 32];
    let mut borrow: i16 = 0;

    // Subtract byte by byte from least significant (index 31) to most significant (index 0)
    for i in (0..32).rev() {
        let diff = a[i] as i16 - b[i] as i16 - borrow;
        if diff < 0 {
            result[i] = (diff + 256) as u8;
            borrow = 1;
        } else {
            result[i] = diff as u8;
            borrow = 0;
        }
    }

    // If there's still a borrow, b > a, return zero
    if borrow != 0 {
        return [0u8; 32];
    }

    result
}

/// Add two 256-bit big-endian values: result = a + b
///
/// Returns the sum as a 256-bit big-endian byte array.
/// Overflow wraps around (modulo 2^256).
pub fn add_blue_work(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let mut result = [0u8; 32];
    let mut carry: u16 = 0;

    for i in (0..32).rev() {
        let sum = a[i] as u16 + b[i] as u16 + carry;
        result[i] = sum as u8;
        carry = sum >> 8;
    }

    result
}

/// Format blue work as a human-readable scientific notation string
///
/// Example: [0x00, 0x00, ..., 0x11, 0x23, 0x45, 0x67] -> "1.12e9"
pub fn format_blue_work(blue_work: &[u8; 32]) -> String {
    // Convert to a rough decimal approximation
    // Find the most significant non-zero byte
    let mut msb_index = None;
    for (i, &byte) in blue_work.iter().enumerate() {
        if byte != 0 {
            msb_index = Some(i);
            break;
        }
    }

    // If all bytes are zero, return "0"
    let msb_index = match msb_index {
        Some(i) => i,
        None => return "0".to_string(),
    };

    // Calculate approximate value
    // Each byte position represents 256^(31-i) = 2^(8*(31-i))
    let significant_bytes = &blue_work[msb_index..];
    let mut value: f64 = 0.0;
    for (i, &byte) in significant_bytes.iter().take(8).enumerate() {
        value += (byte as f64) * 256_f64.powi(((significant_bytes.len() - 1 - i) as i32).min(7));
    }

    // Calculate the exponent
    let bit_position = (31 - msb_index) * 8;
    let log10_value = value.log10() + (bit_position as f64) * (2_f64.log10());
    let exponent = log10_value.floor() as i32;
    let mantissa = 10_f64.powf(log10_value - exponent as f64);

    format!("{:.2}e{}", mantissa, exponent)
}

/// Compare two 256-bit big-endian values
///
/// Returns:
/// - `Ordering::Less` if a < b
/// - `Ordering::Equal` if a == b
/// - `Ordering::Greater` if a > b
pub fn compare_blue_work(a: &[u8; 32], b: &[u8; 32]) -> std::cmp::Ordering {
    a.cmp(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_op_return_script() {
        let commitment = [0xab; 32];
        let output = build_op_return_output(&commitment);

        assert!(output.script_public_key.is_op_return());
        assert_eq!(output.amount, 0);

        let extracted = output.script_public_key.get_op_return_data().unwrap();
        assert_eq!(extracted, commitment.to_vec());
    }

    #[test]
    fn test_extract_commitment_from_tx() {
        let commitment = [0xcd; 32];
        let tx = TransactionInfo {
            hash: [0x11; 32],
            block_hash: Some([0x22; 32]),
            outputs: vec![
                TransactionOutput {
                    amount: 1000,
                    script_public_key: ScriptPublicKey {
                        version: 0,
                        script: vec![0x76, 0xa9], // Not OP_RETURN
                    },
                },
                build_op_return_output(&commitment),
            ],
            inputs: vec![],
            is_accepted: true,
        };

        let extracted = extract_commitment_from_tx(&tx);
        assert_eq!(extracted, Some(commitment));
    }

    #[test]
    fn test_subtract_blue_work() {
        // Simple case: 100 - 50 = 50
        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        a[31] = 100;
        b[31] = 50;

        let result = subtract_blue_work(&a, &b);
        assert_eq!(result[31], 50);

        // With borrow
        a[31] = 0;
        a[30] = 1; // 256
        b[31] = 100;
        b[30] = 0;

        let result = subtract_blue_work(&a, &b);
        assert_eq!(result[31], 156); // 256 - 100 = 156
        assert_eq!(result[30], 0);
    }

    #[test]
    fn test_subtract_blue_work_underflow() {
        let a = [0u8; 32];
        let mut b = [0u8; 32];
        b[31] = 1;

        let result = subtract_blue_work(&a, &b);
        assert_eq!(result, [0u8; 32]); // Should be zero, not underflow
    }

    #[test]
    fn test_add_blue_work() {
        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        a[31] = 100;
        b[31] = 50;

        let result = add_blue_work(&a, &b);
        assert_eq!(result[31], 150);

        // With carry
        a[31] = 200;
        b[31] = 100;

        let result = add_blue_work(&a, &b);
        assert_eq!(result[31], 44); // (200 + 100) % 256 = 44
        assert_eq!(result[30], 1); // carry
    }

    #[test]
    fn test_format_blue_work() {
        let mut work = [0u8; 32];
        assert_eq!(format_blue_work(&work), "0");

        work[31] = 100;
        // 100 ≈ 1.00e2
        let formatted = format_blue_work(&work);
        assert!(formatted.contains("e"));

        // Larger value
        work = [0u8; 32];
        work[20] = 1; // 2^88 ≈ 3.09e26
        let formatted = format_blue_work(&work);
        assert!(formatted.contains("e"));
    }

    #[test]
    fn test_compare_blue_work() {
        let a = [0u8; 32];
        let mut b = [0u8; 32];
        b[31] = 1;

        assert_eq!(compare_blue_work(&a, &b), std::cmp::Ordering::Less);
        assert_eq!(compare_blue_work(&b, &a), std::cmp::Ordering::Greater);
        assert_eq!(compare_blue_work(&a, &a), std::cmp::Ordering::Equal);
    }

    #[test]
    fn test_estimate_tx_size() {
        // 1 input, 2 outputs (change + OP_RETURN)
        let size = estimate_tx_size(1, 1, true);
        assert!(size > 200); // Should be around 235 bytes
    }

    #[tokio::test]
    async fn test_client_creation() {
        let client = KaspaClient::mainnet();
        assert_eq!(
            client.config().rpc_url,
            format!("ws://localhost:{}", DEFAULT_RPC_PORT)
        );

        let client = KaspaClient::testnet();
        assert_eq!(
            client.config().rpc_url,
            format!("ws://localhost:{}", TESTNET_RPC_PORT)
        );
    }

    #[tokio::test]
    async fn test_connection_state() {
        let client = KaspaClient::mainnet();
        assert_eq!(client.connection_state().await, ConnectionState::Disconnected);
        assert!(!client.is_connected().await);
    }
}
