//! Kaspa blockchain service for the calendar server
//!
//! This module provides the integration between the calendar server and the Kaspa
//! blockchain. It handles:
//! - Submitting commitment transactions to Kaspa
//! - Waiting for block confirmations
//! - Retrieving block information for attestations
//! - Managing the calendar wallet

use ktcs_core::{
    BlockInfo, DagInfo, KaspaClient, KaspaClientConfig, TransactionBuilder,
    Utxo, format_blue_work, subtract_blue_work, sign_transaction,
};
use ktcs_core::wallet::KaspaWallet;
use secrecy::{Secret, ExposeSecret};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{info, warn, error, debug};

/// Result type for Kaspa service operations
pub type Result<T> = std::result::Result<T, KaspaServiceError>;

/// Errors from the Kaspa service
#[derive(Debug, thiserror::Error)]
pub enum KaspaServiceError {
    #[error("Not connected to Kaspa node")]
    NotConnected,

    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Transaction submission failed: {0}")]
    SubmissionFailed(String),

    #[error("Transaction not confirmed within timeout")]
    ConfirmationTimeout,

    #[error("Insufficient funds: need {needed} sompi, have {available} sompi")]
    InsufficientFunds { needed: u64, available: u64 },

    #[error("Block not found: {0}")]
    BlockNotFound(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Core error: {0}")]
    Core(#[from] ktcs_core::KtcsError),
}

/// Result of submitting a commitment to Kaspa
#[derive(Debug, Clone)]
pub struct SubmissionResult {
    /// Transaction hash
    pub tx_hash: [u8; 32],
    /// Block hash containing the transaction
    pub block_hash: [u8; 32],
    /// DAA score of the block
    pub daa_score: u64,
    /// Blue score of the block
    pub blue_score: u64,
    /// Block timestamp in milliseconds
    pub timestamp: u64,
    /// Cumulative blue work at this block
    pub blue_work: [u8; 32],
    /// Parent block hashes
    pub parent_hashes: Vec<[u8; 32]>,
}

/// Configuration for the Kaspa service
#[derive(Debug, Clone)]
pub struct KaspaServiceConfig {
    /// RPC endpoint URL
    pub rpc_url: String,
    /// Network name (mainnet, testnet-11, etc.)
    pub network: String,
    /// Calendar wallet address (STAMP wallet)
    pub wallet_address: String,
    /// Confirmation timeout in milliseconds
    pub confirmation_timeout_ms: u64,
    /// Fee rate in sompi per gram
    pub fee_per_gram: u64,
    /// Include KTCS magic prefix in OP_RETURN
    pub include_magic: bool,
    /// Calendar wallet private key (hex-encoded, 64 chars) - STAMP wallet
    /// Required when mock_mode is false
    /// Wrapped in Secret for secure memory handling
    pub wallet_key: Option<Secret<String>>,
    /// Enable mock mode (for testing only - DISABLES BLOCKCHAIN ANCHORING)
    /// WARNING: When enabled, timestamps are NOT anchored to the blockchain!
    pub mock_mode: bool,

    // --- Dual-Wallet Recycling Configuration ---

    /// Return wallet private key (hex-encoded, 64 chars)
    /// When set, change from STAMP transactions goes here and is automatically
    /// recycled back to STAMP wallet.
    /// Wrapped in Secret for secure memory handling
    pub return_wallet_key: Option<Secret<String>>,
    /// Return wallet address (auto-derived from key if not set)
    pub return_wallet_address: Option<String>,
    /// Minimum balance in sompi before recycling (default: 1 KAS = 100,000,000)
    pub recycle_threshold: u64,
    /// Polling interval for the recycle loop in seconds
    pub recycle_poll_interval_secs: u64,
}

impl Default for KaspaServiceConfig {
    fn default() -> Self {
        Self {
            rpc_url: format!("ws://localhost:{}", ktcs_core::DEFAULT_RPC_PORT),
            network: "mainnet".to_string(),
            wallet_address: String::new(),
            confirmation_timeout_ms: 60_000, // 1 minute
            fee_per_gram: 1,
            include_magic: false,
            wallet_key: None,
            mock_mode: false, // Disabled by default - real blockchain required
            // Dual-wallet recycling (disabled by default)
            return_wallet_key: None,
            return_wallet_address: None,
            recycle_threshold: 100_000_000, // 1 KAS
            recycle_poll_interval_secs: 30,
        }
    }
}

impl KaspaServiceConfig {
    /// Validate configuration values
    pub fn validate(&self) -> Result<()> {
        // Validate RPC URL format
        if !self.rpc_url.starts_with("ws://") && !self.rpc_url.starts_with("wss://") {
            return Err(KaspaServiceError::InvalidConfig(
                format!("Invalid RPC URL '{}': must start with ws:// or wss://", self.rpc_url)
            ));
        }

        // Validate network name
        let valid_networks = ["mainnet", "testnet-10", "testnet-11", "simnet", "devnet"];
        if !valid_networks.contains(&self.network.as_str()) {
            return Err(KaspaServiceError::InvalidConfig(
                format!("Invalid network '{}': must be one of {:?}", self.network, valid_networks)
            ));
        }

        // Validate wallet address if provided (must be kaspa: or kaspatest: prefix)
        if !self.wallet_address.is_empty() {
            if !self.wallet_address.starts_with("kaspa:") && !self.wallet_address.starts_with("kaspatest:") {
                return Err(KaspaServiceError::InvalidConfig(
                    format!("Invalid wallet address '{}': must start with 'kaspa:' or 'kaspatest:'", self.wallet_address)
                ));
            }
            // Basic length check (bech32m addresses are typically 61-63 chars)
            if self.wallet_address.len() < 60 || self.wallet_address.len() > 70 {
                return Err(KaspaServiceError::InvalidConfig(
                    format!("Invalid wallet address length: expected 60-70 chars, got {}", self.wallet_address.len())
                ));
            }
        }

        // Validate confirmation timeout (must be reasonable: 10s to 10min)
        if self.confirmation_timeout_ms < 10_000 {
            return Err(KaspaServiceError::InvalidConfig(
                format!("Confirmation timeout {}ms is too short (minimum 10000ms)", self.confirmation_timeout_ms)
            ));
        }
        if self.confirmation_timeout_ms > 600_000 {
            return Err(KaspaServiceError::InvalidConfig(
                format!("Confirmation timeout {}ms is too long (maximum 600000ms)", self.confirmation_timeout_ms)
            ));
        }

        // Validate fee rate (must be positive and reasonable)
        if self.fee_per_gram == 0 {
            return Err(KaspaServiceError::InvalidConfig(
                "Fee per gram must be greater than 0".to_string()
            ));
        }
        if self.fee_per_gram > 10_000 {
            return Err(KaspaServiceError::InvalidConfig(
                format!("Fee per gram {} seems too high (maximum 10000)", self.fee_per_gram)
            ));
        }

        // CRITICAL: Require wallet key when mock mode is disabled
        if !self.mock_mode && self.wallet_key.is_none() {
            return Err(KaspaServiceError::InvalidConfig(
                "CALENDAR_WALLET_KEY is required when KTCS_MOCK_MODE is false. \
                 Set KTCS_MOCK_MODE=true for testing without a wallet, or provide a wallet key.".to_string()
            ));
        }

        // Validate wallet key format if provided
        if let Some(ref key) = self.wallet_key {
            let key_str = key.expose_secret();
            if key_str.len() != 64 {
                return Err(KaspaServiceError::InvalidConfig(
                    format!("CALENDAR_WALLET_KEY must be 64 hex characters (32 bytes), got {} chars", key_str.len())
                ));
            }
            if hex::decode(key_str).is_err() {
                return Err(KaspaServiceError::InvalidConfig(
                    "CALENDAR_WALLET_KEY is not valid hex".to_string()
                ));
            }
        }

        // Validate return wallet key if provided (dual-wallet recycling)
        if let Some(ref key) = self.return_wallet_key {
            let key_str = key.expose_secret();
            if key_str.len() != 64 {
                return Err(KaspaServiceError::InvalidConfig(
                    format!("RETURN_WALLET_KEY must be 64 hex characters (32 bytes), got {} chars", key_str.len())
                ));
            }
            if hex::decode(key_str).is_err() {
                return Err(KaspaServiceError::InvalidConfig(
                    "RETURN_WALLET_KEY is not valid hex".to_string()
                ));
            }
            // Ensure RETURN wallet is different from STAMP wallet
            if let Some(ref stamp_key) = self.wallet_key {
                if key_str == stamp_key.expose_secret() {
                    return Err(KaspaServiceError::InvalidConfig(
                        "RETURN_WALLET_KEY must be different from CALENDAR_WALLET_KEY".to_string()
                    ));
                }
            }
        }

        // Validate recycle threshold (warn if too low)
        if self.return_wallet_key.is_some() && self.recycle_threshold < 50_000_000 {
            warn!(
                "Recycle threshold {} sompi ({:.2} KAS) is low - may result in high fee percentage",
                self.recycle_threshold,
                self.recycle_threshold as f64 / 100_000_000.0
            );
        }

        // CRITICAL: Prevent mock mode in production environments
        if self.mock_mode {
            // Check for production environment indicator
            let is_production = std::env::var("KTCS_ENVIRONMENT")
                .map(|v| v.to_lowercase() == "production")
                .unwrap_or(false);

            if is_production {
                return Err(KaspaServiceError::InvalidConfig(
                    "SECURITY ERROR: Mock mode cannot be enabled in production environment. \
                     Set KTCS_ENVIRONMENT to 'development' or 'testing' to use mock mode.".to_string()
                ));
            }

            // Prevent mock mode on mainnet regardless of environment
            if self.network == "mainnet" {
                return Err(KaspaServiceError::InvalidConfig(
                    "SECURITY ERROR: Mock mode cannot be used with mainnet. \
                     Use a testnet for testing purposes.".to_string()
                ));
            }

            // Log prominent warning for non-production environments
            error!("==================================================");
            error!("MOCK MODE ENABLED - Timestamps will NOT be anchored!");
            error!("This should ONLY be used for testing purposes!");
            error!("==================================================");
        }

        Ok(())
    }

    /// Create configuration from environment variables
    pub fn from_env() -> Result<Self> {
        let rpc_url = std::env::var("KASPA_RPC_URL")
            .unwrap_or_else(|_| format!("ws://localhost:{}", ktcs_core::DEFAULT_RPC_PORT));

        let network = std::env::var("KASPA_NETWORK").unwrap_or_else(|_| "mainnet".to_string());

        let wallet_address = std::env::var("CALENDAR_WALLET_ADDRESS").unwrap_or_default();

        let confirmation_timeout_ms = std::env::var("CONFIRMATION_TIMEOUT_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(60_000);

        let fee_per_gram = std::env::var("FEE_PER_GRAM")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);

        let include_magic = std::env::var("KTCS_INCLUDE_MAGIC")
            .ok()
            .map(|s| s == "true" || s == "1")
            .unwrap_or(false);

        // Load wallet private key from environment
        // SECURITY: Key is wrapped in Secret for secure memory handling
        let wallet_key = std::env::var("CALENDAR_WALLET_KEY")
            .ok()
            .filter(|s| !s.is_empty())
            .map(Secret::new);

        if wallet_key.is_some() {
            info!("Calendar wallet configured");
        }

        // Clear environment variable after reading (defense in depth)
        std::env::remove_var("CALENDAR_WALLET_KEY");

        // SECURITY WARNING: Mock mode should ONLY be enabled for testing
        // When enabled, timestamps are NOT anchored to the blockchain!
        let mock_mode = std::env::var("KTCS_MOCK_MODE")
            .ok()
            .map(|s| s == "true" || s == "1")
            .unwrap_or(false);

        if mock_mode {
            warn!("SECURITY WARNING: Mock mode is enabled! Timestamps will NOT be anchored to the blockchain!");
            warn!("This should ONLY be used for testing purposes.");
        }

        // Load return wallet key (dual-wallet recycling)
        // SECURITY: Key is wrapped in Secret for secure memory handling
        let return_wallet_key = std::env::var("RETURN_WALLET_KEY")
            .ok()
            .filter(|s| !s.is_empty())
            .map(Secret::new);

        if return_wallet_key.is_some() {
            info!("Return wallet configured (dual-wallet recycling enabled)");
        }

        // Clear environment variable after reading (defense in depth)
        std::env::remove_var("RETURN_WALLET_KEY");

        let return_wallet_address = std::env::var("RETURN_WALLET_ADDRESS")
            .ok()
            .filter(|s| !s.is_empty());

        let recycle_threshold = std::env::var("RECYCLE_THRESHOLD_SOMPI")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(100_000_000); // 1 KAS default

        let recycle_poll_interval_secs = std::env::var("RECYCLE_POLL_INTERVAL_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30);

        Ok(Self {
            rpc_url,
            network,
            wallet_address,
            confirmation_timeout_ms,
            fee_per_gram,
            include_magic,
            wallet_key,
            mock_mode,
            return_wallet_key,
            return_wallet_address,
            recycle_threshold,
            recycle_poll_interval_secs,
        })
    }
}

/// Kaspa blockchain service
///
/// Provides methods for interacting with the Kaspa blockchain
/// for timestamp commitment and verification.
pub struct KaspaService {
    config: KaspaServiceConfig,
    /// Kaspa RPC client
    pub client: KaspaClient,
    /// Calendar wallet for signing transactions (STAMP wallet, None in mock mode)
    wallet: Option<KaspaWallet>,
    /// Cached UTXOs for the STAMP wallet
    utxos: Arc<RwLock<Vec<Utxo>>>,
    /// Current chain state
    dag_info: Arc<RwLock<Option<DagInfo>>>,
    /// Whether we're connected
    connected: Arc<RwLock<bool>>,

    // --- Dual-Wallet Recycling ---

    /// Return wallet for receiving change (None if not configured)
    return_wallet: Option<KaspaWallet>,
    /// Cached UTXOs for the RETURN wallet
    pub return_utxos: Arc<RwLock<Vec<Utxo>>>,
}

impl KaspaService {
    /// Create a new Kaspa service with the given configuration
    pub fn new(config: KaspaServiceConfig) -> Result<Self> {
        let client_config = KaspaClientConfig {
            rpc_url: config.rpc_url.clone(),
            network: Some(config.network.clone()),
            connect_timeout_ms: 10_000,
            request_timeout_ms: 30_000,
            auto_reconnect: true,
            use_resolver: false, // We use explicit RPC URL from config
            tls_verify: true,    // Always verify TLS certificates
        };

        // Initialize wallet if key is provided
        let wallet = if let Some(ref key_secret) = config.wallet_key {
            let wallet = KaspaWallet::from_hex(key_secret.expose_secret(), &config.network)
                .map_err(|e| KaspaServiceError::InvalidConfig(
                    format!("Failed to load wallet from CALENDAR_WALLET_KEY: {}", e)
                ))?;

            info!("Calendar wallet initialized: {}", wallet.address());

            // Warn if wallet_address in config differs from derived address
            if !config.wallet_address.is_empty() && config.wallet_address != wallet.address() {
                warn!(
                    "CALENDAR_WALLET_ADDRESS ({}) differs from derived address ({}). Using derived address.",
                    config.wallet_address,
                    wallet.address()
                );
            }

            Some(wallet)
        } else {
            if !config.mock_mode {
                warn!("No wallet configured - real transactions will fail!");
            }
            None
        };

        // Get wallet address (from wallet or config)
        let wallet_address = wallet.as_ref()
            .map(|w| w.address().to_string())
            .unwrap_or_else(|| config.wallet_address.clone());

        // Initialize return wallet if key is provided (dual-wallet recycling)
        let return_wallet = if let Some(ref key_secret) = config.return_wallet_key {
            let rw = KaspaWallet::from_hex(key_secret.expose_secret(), &config.network)
                .map_err(|e| KaspaServiceError::InvalidConfig(
                    format!("Failed to load return wallet from RETURN_WALLET_KEY: {}", e)
                ))?;

            info!("Return wallet initialized: {}", rw.address());

            // Ensure it's different from the stamp wallet
            if let Some(ref stamp) = wallet {
                if stamp.address() == rw.address() {
                    return Err(KaspaServiceError::InvalidConfig(
                        "RETURN_WALLET_KEY must produce a different address than CALENDAR_WALLET_KEY".to_string()
                    ));
                }
            }

            // Warn if return_wallet_address in config differs from derived address
            if let Some(ref configured_addr) = config.return_wallet_address {
                if !configured_addr.is_empty() && configured_addr != rw.address() {
                    warn!(
                        "RETURN_WALLET_ADDRESS ({}) differs from derived address ({}). Using derived address.",
                        configured_addr,
                        rw.address()
                    );
                }
            }

            Some(rw)
        } else {
            debug!("No return wallet configured - change will go back to stamp wallet");
            None
        };

        Ok(Self {
            config: KaspaServiceConfig {
                wallet_address,
                ..config
            },
            client: KaspaClient::new(client_config),
            wallet,
            utxos: Arc::new(RwLock::new(Vec::new())),
            dag_info: Arc::new(RwLock::new(None)),
            connected: Arc::new(RwLock::new(false)),
            return_wallet,
            return_utxos: Arc::new(RwLock::new(Vec::new())),
        })
    }

    /// Create a service from environment configuration
    #[allow(dead_code)]
    pub fn from_env() -> Result<Self> {
        let config = KaspaServiceConfig::from_env()?;
        Self::new(config)
    }

    /// Connect to the Kaspa node with retry logic
    pub async fn connect(&self) -> Result<()> {
        info!("Connecting to Kaspa node at {}", self.config.rpc_url);

        // Retry connection up to 5 times with exponential backoff
        let connect_result = Self::retry_with_backoff(
            "Kaspa node connection",
            5,
            1000, // start with 1 second delay
            || async {
                self.client.connect().await
                    .map_err(|e| KaspaServiceError::ConnectionFailed(e.to_string()))
            }
        ).await;

        match connect_result {
            Ok(()) => {
                *self.connected.write().await = true;
                info!("Connected to Kaspa node");

                // Refresh state (non-fatal if these fail)
                if let Err(e) = self.refresh_dag_info().await {
                    warn!("Failed to refresh DAG info on connect: {}", e);
                }
                if let Err(e) = self.refresh_utxos().await {
                    warn!("Failed to refresh UTXOs on connect: {}", e);
                }

                Ok(())
            }
            Err(e) => {
                warn!("Failed to connect to Kaspa node after retries: {}", e);
                *self.connected.write().await = false;
                Err(e)
            }
        }
    }

    /// Check if connected to Kaspa node
    pub async fn is_connected(&self) -> bool {
        *self.connected.read().await
    }

    /// Refresh DAG information
    pub async fn refresh_dag_info(&self) -> Result<()> {
        if !self.is_connected().await {
            return Err(KaspaServiceError::NotConnected);
        }

        match self.client.get_block_dag_info().await {
            Ok(info) => {
                *self.dag_info.write().await = Some(info);
                Ok(())
            }
            Err(e) => {
                error!("Failed to get DAG info: {}", e);
                Err(KaspaServiceError::Core(e))
            }
        }
    }

    /// Refresh UTXOs for the wallet
    pub async fn refresh_utxos(&self) -> Result<()> {
        if !self.is_connected().await {
            return Err(KaspaServiceError::NotConnected);
        }

        if self.config.wallet_address.is_empty() {
            warn!("No wallet address configured, skipping UTXO refresh");
            return Ok(());
        }

        match self.client.get_utxos_by_address(&self.config.wallet_address).await {
            Ok(utxos) => {
                let total: u64 = utxos.iter().map(|u| u.amount).sum();
                info!(
                    "Wallet {} has {} UTXOs totaling {} sompi",
                    self.config.wallet_address,
                    utxos.len(),
                    total
                );
                *self.utxos.write().await = utxos;
                Ok(())
            }
            Err(e) => {
                error!("Failed to get UTXOs: {}", e);
                Err(KaspaServiceError::Core(e))
            }
        }
    }

    /// Get current DAA score
    #[allow(dead_code)]
    pub async fn get_current_daa_score(&self) -> Result<u64> {
        let dag_info = self.dag_info.read().await;
        match &*dag_info {
            Some(info) => Ok(info.current_daa_score),
            None => Err(KaspaServiceError::NotConnected),
        }
    }

    /// Get current blue work
    #[allow(dead_code)]
    pub async fn get_current_blue_work(&self) -> Result<[u8; 32]> {
        let dag_info = self.dag_info.read().await;
        match &*dag_info {
            Some(info) => Ok(info.current_blue_work),
            None => Err(KaspaServiceError::NotConnected),
        }
    }

    /// Submit a commitment to the blockchain
    ///
    /// # Arguments
    ///
    /// * `commitment` - The 32-byte Merkle root or hash commitment
    ///
    /// # Returns
    ///
    /// Submission result with block and transaction information.
    ///
    /// # Errors
    ///
    /// Returns an error if not connected to a Kaspa node and mock mode is disabled.
    pub async fn submit_commitment(&self, commitment: [u8; 32]) -> Result<SubmissionResult> {
        info!("Submitting commitment: {}", hex::encode(commitment));

        // Check if mock mode is explicitly enabled
        if self.config.mock_mode {
            warn!("MOCK MODE: Submitting to mock backend - timestamp NOT anchored to blockchain!");
            return self.submit_commitment_mock(commitment).await;
        }

        // Require real connection when mock mode is disabled
        if !self.is_connected().await {
            error!("Cannot submit commitment: not connected to Kaspa node and mock mode is disabled");
            return Err(KaspaServiceError::NotConnected);
        }

        // Get available UTXOs
        let utxos = self.utxos.read().await.clone();

        if utxos.is_empty() {
            return Err(KaspaServiceError::InsufficientFunds {
                needed: 1000, // Minimum fee estimate
                available: 0,
            });
        }

        // Build transaction with change going to RETURN wallet (if configured)
        let change_address = self.get_change_address();
        let tx = TransactionBuilder::new()
            .commitment(&commitment)
            .add_inputs(utxos.clone())
            .change_address(change_address)
            .fee_per_gram(self.config.fee_per_gram)
            .include_magic(self.config.include_magic)
            .build()
            .map_err(|e| KaspaServiceError::SubmissionFailed(e.to_string()))?;

        info!(
            "Built transaction: fee={} sompi, change={} sompi → {}",
            tx.fee,
            tx.change_amount,
            if self.is_recycling_enabled() { "RETURN wallet" } else { "self" }
        );

        // Get wallet for signing
        let wallet = self.wallet.as_ref()
            .ok_or_else(|| KaspaServiceError::InvalidConfig(
                "Cannot submit real transaction: no wallet configured. Set CALENDAR_WALLET_KEY.".to_string()
            ))?;

        // Sign the transaction
        let unsigned_tx = tx.transaction;
        let signed_tx = sign_transaction(&unsigned_tx, wallet, &utxos)
            .map_err(|e| KaspaServiceError::SubmissionFailed(
                format!("Transaction signing failed: {}", e)
            ))?;

        debug!(
            "Transaction signed: {} inputs, signature lengths: {:?}",
            signed_tx.inputs.len(),
            signed_tx.inputs.iter().map(|i| i.signature_script.len()).collect::<Vec<_>>()
        );

        // Submit signed transaction with retry logic for transient failures
        let tx_hash = Self::retry_with_backoff(
            "Transaction submission",
            3,  // max 3 retries
            500, // start with 500ms delay
            || async {
                self.client.submit_transaction(signed_tx.clone()).await
                    .map_err(|e| KaspaServiceError::SubmissionFailed(e.to_string()))
            }
        ).await?;

        info!("Transaction submitted: {}", hex::encode(tx_hash));
        debug!("Transaction hash: {}", hex::encode(tx_hash));

        // Wait for confirmation
        let block = self.client
            .wait_for_confirmation(&tx_hash, self.config.confirmation_timeout_ms)
            .await
            .map_err(|_| KaspaServiceError::ConfirmationTimeout)?;

        info!(
            "Transaction confirmed in block {} at DAA score {}",
            hex::encode(block.hash),
            block.daa_score
        );

        // Refresh UTXOs (our change output is now available)
        if let Err(e) = self.refresh_utxos().await {
            warn!("Failed to refresh UTXOs after confirmation: {}", e);
        }

        Ok(SubmissionResult {
            tx_hash,
            block_hash: block.hash,
            daa_score: block.daa_score,
            blue_score: block.blue_score,
            timestamp: block.timestamp,
            blue_work: block.blue_work,
            parent_hashes: block.parent_hashes,
        })
    }

    /// Submit commitment using mock data (TEST MODE ONLY)
    ///
    /// WARNING: This creates a FAKE attestation that is NOT anchored to the blockchain.
    /// Proofs created with mock attestations cannot be independently verified.
    /// This should ONLY be used for testing and development purposes.
    async fn submit_commitment_mock(&self, commitment: [u8; 32]) -> Result<SubmissionResult> {
        error!("==================================================================");
        error!("SECURITY WARNING: Using MOCK Kaspa submission!");
        error!("This timestamp is NOT anchored to the blockchain!");
        error!("Proofs created will NOT be verifiable against the real blockchain!");
        error!("This should ONLY be used for testing purposes!");
        error!("==================================================================");

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Generate deterministic but unique hashes based on commitment and time
        let mut tx_hash = [0u8; 32];
        for (i, byte) in commitment.iter().enumerate() {
            tx_hash[i] = byte.wrapping_add((now & 0xFF) as u8);
        }

        let mut block_hash = [0u8; 32];
        for (i, byte) in tx_hash.iter().enumerate() {
            block_hash[i] = byte.wrapping_add(0x11);
        }

        // Mock DAA and blue scores based on time
        let base_daa = 42_000_000u64;
        let time_offset = now / 100; // ~10 blocks per second
        let daa_score = base_daa + time_offset;
        let blue_score = base_daa - 500_000 + time_offset;

        // Mock blue work (increasing over time)
        let mut blue_work = [0u8; 32];
        blue_work[20] = 0x01; // ~10^26 range
        blue_work[21] = ((time_offset >> 16) & 0xFF) as u8;
        blue_work[22] = ((time_offset >> 8) & 0xFF) as u8;
        blue_work[23] = (time_offset & 0xFF) as u8;

        // Mock parent hashes
        let mut parent1 = block_hash;
        parent1[0] ^= 0xAA;
        let mut parent2 = block_hash;
        parent2[0] ^= 0xBB;

        info!(
            "Mock submission: tx={} block={} daa={}",
            hex::encode(tx_hash),
            hex::encode(block_hash),
            daa_score
        );

        Ok(SubmissionResult {
            tx_hash,
            block_hash,
            daa_score,
            blue_score,
            timestamp: now,
            blue_work,
            parent_hashes: vec![parent1, parent2],
        })
    }

    /// Get a block by hash
    #[allow(dead_code)]
    pub async fn get_block(&self, hash: &[u8; 32]) -> Result<BlockInfo> {
        if !self.is_connected().await {
            return Err(KaspaServiceError::NotConnected);
        }

        self.client.get_block_by_hash(hash).await
            .map_err(|e| KaspaServiceError::BlockNotFound(e.to_string()))
    }

    /// Calculate thermodynamic metrics for an attestation
    ///
    /// # Arguments
    ///
    /// * `attestation_blue_work` - Blue work at the time of attestation
    /// * `attestation_daa_score` - DAA score at the time of attestation
    ///
    /// # Returns
    ///
    /// Tuple of (current_blue_work, accumulated_blue_work, blocks_since)
    pub async fn calculate_thermodynamic_metrics(
        &self,
        attestation_blue_work: &[u8; 32],
        attestation_daa_score: u64,
    ) -> Result<ThermodynamicMetrics> {
        // Try to get current chain state
        let (current_daa, current_blue_work) = if let Some(dag_info) = &*self.dag_info.read().await {
            (dag_info.current_daa_score, dag_info.current_blue_work)
        } else {
            // If not connected, return partial metrics
            return Ok(ThermodynamicMetrics {
                current_blue_work: None,
                accumulated_blue_work: None,
                blocks_since: None,
                time_elapsed_seconds: None,
                btc_equivalent_confirmations: None,
            });
        };

        let accumulated = subtract_blue_work(&current_blue_work, attestation_blue_work);
        let blocks_since = current_daa.saturating_sub(attestation_daa_score);
        let time_elapsed_seconds = blocks_since / 10; // Approximate at 10 BPS

        // Rough BTC equivalence: 1 hour Kaspa ≈ 1 BTC confirmation
        // 36000 blocks (1 hour at 10 BPS) ≈ 1 BTC conf
        let btc_equiv = blocks_since as f64 / 36000.0;

        Ok(ThermodynamicMetrics {
            current_blue_work: Some(format_blue_work(&current_blue_work)),
            accumulated_blue_work: Some(format_blue_work(&accumulated)),
            blocks_since: Some(blocks_since),
            time_elapsed_seconds: Some(time_elapsed_seconds),
            btc_equivalent_confirmations: Some(btc_equiv),
        })
    }

    /// Get configuration
    pub fn config(&self) -> &KaspaServiceConfig {
        &self.config
    }

    // --- Dual-Wallet Recycling Methods ---

    /// Get the change address for commitment transactions.
    /// Returns RETURN wallet address if configured, otherwise STAMP wallet address.
    pub fn get_change_address(&self) -> &str {
        self.return_wallet
            .as_ref()
            .map(|w| w.address())
            .unwrap_or(&self.config.wallet_address)
    }

    /// Check if dual-wallet recycling is enabled
    pub fn is_recycling_enabled(&self) -> bool {
        self.return_wallet.is_some()
    }

    /// Get the return wallet (if configured)
    pub fn return_wallet(&self) -> Option<&KaspaWallet> {
        self.return_wallet.as_ref()
    }

    /// Get the recycle threshold from config
    pub fn recycle_threshold(&self) -> u64 {
        self.config.recycle_threshold
    }

    /// Refresh UTXOs for the return wallet
    pub async fn refresh_return_utxos(&self) -> Result<()> {
        let return_wallet = match &self.return_wallet {
            Some(w) => w,
            None => return Ok(()), // No return wallet configured
        };

        if !self.is_connected().await {
            return Err(KaspaServiceError::NotConnected);
        }

        match self.client.get_utxos_by_address(return_wallet.address()).await {
            Ok(utxos) => {
                let total: u64 = utxos.iter().map(|u| u.amount).sum();
                debug!(
                    "Return wallet {} has {} UTXOs totaling {} sompi ({:.4} KAS)",
                    return_wallet.address(),
                    utxos.len(),
                    total,
                    total as f64 / 100_000_000.0
                );
                *self.return_utxos.write().await = utxos;
                Ok(())
            }
            Err(e) => {
                error!("Failed to get return wallet UTXOs: {}", e);
                Err(KaspaServiceError::Core(e))
            }
        }
    }

    /// Get total balance of return wallet
    pub async fn return_wallet_balance(&self) -> u64 {
        self.return_utxos.read().await.iter().map(|u| u.amount).sum()
    }

    /// Retry an async operation with exponential backoff
    ///
    /// # Arguments
    ///
    /// * `operation_name` - Name for logging
    /// * `max_retries` - Maximum number of retry attempts
    /// * `initial_delay_ms` - Initial delay between retries in milliseconds
    /// * `operation` - The async operation to retry
    ///
    /// # Returns
    ///
    /// The operation result if successful, or the last error if all retries fail.
    async fn retry_with_backoff<T, E, F, Fut>(
        operation_name: &str,
        max_retries: u32,
        initial_delay_ms: u64,
        mut operation: F,
    ) -> std::result::Result<T, E>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = std::result::Result<T, E>>,
        E: std::fmt::Display,
    {
        let mut last_error = None;
        let mut delay = Duration::from_millis(initial_delay_ms);

        for attempt in 0..=max_retries {
            match operation().await {
                Ok(result) => {
                    if attempt > 0 {
                        info!("{} succeeded after {} retries", operation_name, attempt);
                    }
                    return Ok(result);
                }
                Err(e) => {
                    if attempt < max_retries {
                        warn!(
                            "{} failed (attempt {}/{}): {}, retrying in {:?}",
                            operation_name,
                            attempt + 1,
                            max_retries + 1,
                            e,
                            delay
                        );
                        tokio::time::sleep(delay).await;
                        // Exponential backoff with cap at 10 seconds
                        delay = std::cmp::min(delay * 2, Duration::from_secs(10));
                    } else {
                        error!(
                            "{} failed after {} attempts: {}",
                            operation_name,
                            max_retries + 1,
                            e
                        );
                    }
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap())
    }
}

/// Thermodynamic security metrics
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ThermodynamicMetrics {
    /// Current blue work on the chain (formatted string)
    pub current_blue_work: Option<String>,
    /// Blue work accumulated since attestation (formatted string)
    pub accumulated_blue_work: Option<String>,
    /// Number of blocks since attestation
    pub blocks_since: Option<u64>,
    /// Approximate time elapsed in seconds
    pub time_elapsed_seconds: Option<u64>,
    /// Approximate Bitcoin confirmation equivalence
    pub btc_equivalent_confirmations: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = KaspaServiceConfig::default();
        assert!(config.rpc_url.contains("16110"));
        assert_eq!(config.network, "mainnet");
        assert_eq!(config.fee_per_gram, 1);
    }

    #[tokio::test]
    async fn test_mock_submission() {
        let config = KaspaServiceConfig {
            mock_mode: true,
            ..Default::default()
        };
        let service = KaspaService::new(config).expect("Failed to create service");

        let commitment = [0xab; 32];
        let result = service.submit_commitment_mock(commitment).await;

        assert!(result.is_ok());
        let submission = result.unwrap();
        assert!(submission.daa_score > 42_000_000);
        assert!(!submission.parent_hashes.is_empty());
    }
}
