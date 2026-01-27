//! Wallet recycling service
//!
//! Monitors the RETURN wallet and automatically sends funds back to the STAMP wallet.
//! This creates a self-sustaining loop where KAS circulates between wallets,
//! minimizing the need for manual refunding (only losing transaction fees over time).

use crate::services::kaspa_service::{KaspaService, KaspaServiceError};
use ktcs_core::{sign_transaction, TransferTransactionBuilder};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{info, warn, error, debug};

/// Result type for recycle operations
pub type Result<T> = std::result::Result<T, RecycleError>;

/// Errors from the recycle service
#[derive(Debug, thiserror::Error)]
pub enum RecycleError {
    #[error("Recycle service not enabled (no RETURN wallet configured)")]
    NotEnabled,

    #[error("Kaspa service error: {0}")]
    KaspaService(#[from] KaspaServiceError),

    #[error("Transaction build failed: {0}")]
    BuildFailed(String),

    #[error("Transaction signing failed: {0}")]
    SigningFailed(String),

    #[error("Transaction submission failed: {0}")]
    SubmissionFailed(String),
}

/// Statistics for the recycle service
#[derive(Debug, Clone, Default)]
pub struct RecycleStats {
    /// Number of successful recycle transactions
    pub successful_recycles: u64,
    /// Total amount recycled (in sompi)
    pub total_recycled: u64,
    /// Total fees paid (in sompi)
    pub total_fees: u64,
    /// Number of failed recycle attempts
    pub failed_attempts: u64,
}

/// Result of a successful recycle transaction
#[derive(Debug, Clone)]
pub struct RecycleResult {
    /// Transaction hash
    pub tx_hash: [u8; 32],
    /// Amount sent (in sompi)
    pub amount: u64,
    /// Fee paid (in sompi)
    pub fee: u64,
}

/// Recycle service for automatically sending RETURN wallet funds back to STAMP wallet
pub struct RecycleService {
    kaspa_service: Arc<KaspaService>,
    stats: Arc<tokio::sync::RwLock<RecycleStats>>,
}

impl RecycleService {
    /// Create a new recycle service
    pub fn new(kaspa_service: Arc<KaspaService>) -> Self {
        Self {
            kaspa_service,
            stats: Arc::new(tokio::sync::RwLock::new(RecycleStats::default())),
        }
    }

    /// Check if recycling is enabled
    pub fn is_enabled(&self) -> bool {
        self.kaspa_service.is_recycling_enabled()
    }

    /// Get current statistics
    pub async fn stats(&self) -> RecycleStats {
        self.stats.read().await.clone()
    }

    /// Execute one recycle check/transaction
    ///
    /// Returns `Ok(Some(result))` if a recycle transaction was made,
    /// `Ok(None)` if balance was below threshold or not connected,
    /// or an error if something went wrong.
    pub async fn try_recycle(&self) -> Result<Option<RecycleResult>> {
        if !self.is_enabled() {
            return Err(RecycleError::NotEnabled);
        }

        // Check if connected
        if !self.kaspa_service.is_connected().await {
            debug!("Cannot recycle: not connected to Kaspa node");
            return Ok(None);
        }

        // Refresh RETURN wallet UTXOs
        self.kaspa_service.refresh_return_utxos().await?;

        let balance = self.kaspa_service.return_wallet_balance().await;
        let threshold = self.kaspa_service.recycle_threshold();

        debug!(
            "Return wallet balance: {} sompi ({:.4} KAS), threshold: {} sompi ({:.4} KAS)",
            balance,
            balance as f64 / 100_000_000.0,
            threshold,
            threshold as f64 / 100_000_000.0
        );

        // Check if above threshold
        if balance < threshold {
            debug!("Balance below threshold, skipping recycle");
            return Ok(None);
        }

        info!(
            "Return wallet has {} sompi ({:.4} KAS), initiating recycle to STAMP wallet",
            balance,
            balance as f64 / 100_000_000.0
        );

        // Get UTXOs
        let utxos = self.kaspa_service.return_utxos.read().await.clone();

        // Get wallets
        let return_wallet = self.kaspa_service.return_wallet()
            .ok_or(RecycleError::NotEnabled)?;
        let stamp_address = self.kaspa_service.config().wallet_address.clone();

        // Build transfer transaction
        let transfer_tx = TransferTransactionBuilder::new()
            .add_inputs(utxos.clone())
            .destination(&stamp_address)
            .fee_per_gram(self.kaspa_service.config().fee_per_gram)
            .build()
            .map_err(|e| RecycleError::BuildFailed(e.to_string()))?;

        info!(
            "Built recycle transaction: {} sompi ({:.4} KAS) → STAMP wallet, fee={} sompi",
            transfer_tx.send_amount,
            transfer_tx.send_amount as f64 / 100_000_000.0,
            transfer_tx.fee
        );

        // Sign transaction
        let signed_tx = sign_transaction(&transfer_tx.transaction, return_wallet, &utxos)
            .map_err(|e| RecycleError::SigningFailed(e.to_string()))?;

        // Submit transaction
        let tx_hash = self.kaspa_service.client
            .submit_transaction(signed_tx)
            .await
            .map_err(|e| RecycleError::SubmissionFailed(e.to_string()))?;

        info!(
            "Recycle transaction submitted: {} ({} sompi → STAMP wallet)",
            hex::encode(tx_hash),
            transfer_tx.send_amount
        );

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.successful_recycles += 1;
            stats.total_recycled += transfer_tx.send_amount;
            stats.total_fees += transfer_tx.fee;
        }

        // Refresh STAMP wallet UTXOs (the recycled funds will appear there soon)
        if let Err(e) = self.kaspa_service.refresh_utxos().await {
            warn!("Failed to refresh STAMP UTXOs after recycle: {}", e);
        }

        Ok(Some(RecycleResult {
            tx_hash,
            amount: transfer_tx.send_amount,
            fee: transfer_tx.fee,
        }))
    }

    /// Run the recycle loop
    ///
    /// Periodically checks the RETURN wallet balance and initiates recycling
    /// when above the threshold. Runs until shutdown signal is received.
    pub async fn run_loop(&self, mut shutdown: broadcast::Receiver<()>) {
        let poll_interval = Duration::from_secs(
            self.kaspa_service.config().recycle_poll_interval_secs
        );

        let mut interval = tokio::time::interval(poll_interval);

        info!(
            "Recycle service started (poll interval: {}s, threshold: {} sompi = {:.2} KAS)",
            poll_interval.as_secs(),
            self.kaspa_service.recycle_threshold(),
            self.kaspa_service.recycle_threshold() as f64 / 100_000_000.0
        );

        loop {
            tokio::select! {
                _ = shutdown.recv() => {
                    info!("Recycle service received shutdown signal");
                    break;
                }
                _ = interval.tick() => {
                    match self.try_recycle().await {
                        Ok(Some(result)) => {
                            info!(
                                "Recycle completed: {} sompi ({:.4} KAS) sent, tx={}",
                                result.amount,
                                result.amount as f64 / 100_000_000.0,
                                hex::encode(result.tx_hash)
                            );
                        }
                        Ok(None) => {
                            // Below threshold or not connected - normal operation
                        }
                        Err(RecycleError::NotEnabled) => {
                            // Should not happen in loop, but handle gracefully
                            error!("Recycle service not enabled but loop is running");
                            break;
                        }
                        Err(e) => {
                            error!("Recycle failed: {}", e);
                            let mut stats = self.stats.write().await;
                            stats.failed_attempts += 1;
                        }
                    }
                }
            }
        }

        // Log final stats on shutdown
        let stats = self.stats.read().await;
        info!(
            "Recycle service stopped. Stats: {} successful, {} sompi ({:.4} KAS) recycled, {} sompi fees, {} failures",
            stats.successful_recycles,
            stats.total_recycled,
            stats.total_recycled as f64 / 100_000_000.0,
            stats.total_fees,
            stats.failed_attempts
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recycle_stats_default() {
        let stats = RecycleStats::default();
        assert_eq!(stats.successful_recycles, 0);
        assert_eq!(stats.total_recycled, 0);
        assert_eq!(stats.total_fees, 0);
        assert_eq!(stats.failed_attempts, 0);
    }
}
