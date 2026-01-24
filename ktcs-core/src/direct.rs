//! Direct Stamping Mode
//!
//! This module enables direct timestamping without a calendar server.
//! Users submit their own OP_RETURN transactions directly to the Kaspa network.
//!
//! Benefits:
//! - Fully self-sovereign (no trust in calendar)
//! - Immediate confirmation (no batching delay)
//!
//! Tradeoffs:
//! - Higher cost (one transaction per stamp)
//! - Requires wallet with funds

#[cfg(feature = "kaspa-client")]
use crate::kaspa::BlockInfo;
use crate::merkle::sha256;
use crate::tx::{create_commitment, generate_nonce};
use crate::wallet::KaspaWallet;
use crate::{
    error::Result,
    types::{Attestation, KaspaAttestation, KtcsProof, Operation},
};

/// Configuration for direct stamping
#[derive(Clone, Debug)]
pub struct DirectStampConfig {
    /// Fee rate in sompi per gram
    pub fee_rate: u64,
    /// Maximum transaction size in bytes
    pub max_tx_size: u64,
    /// Timeout for confirmation in seconds
    pub confirmation_timeout_secs: u64,
}

impl Default for DirectStampConfig {
    fn default() -> Self {
        Self {
            fee_rate: 1, // 1 sompi per gram (minimum)
            max_tx_size: 10_000,
            confirmation_timeout_secs: 60,
        }
    }
}

/// Result of a direct stamp operation
#[derive(Clone, Debug)]
pub struct DirectStampResult {
    /// The complete proof with attestation
    pub proof: KtcsProof,
    /// Transaction hash
    pub tx_hash: [u8; 32],
    /// Block hash where transaction was confirmed
    pub block_hash: [u8; 32],
    /// DAA score of confirming block
    pub daa_score: u64,
    /// Blue score of confirming block
    pub blue_score: u64,
}

/// Prepare a direct stamp transaction (without submitting)
///
/// This creates the transaction and proof structure but doesn't submit
/// to the network. Useful for previewing fees or manual submission.
///
/// In a full implementation, this would:
/// - Query UTXOs for the wallet
/// - Build the transaction
/// - Estimate fees
/// - Return the unsigned transaction
pub fn prepare_direct_stamp(
    data: &[u8],
    _wallet: &KaspaWallet,
    _config: &DirectStampConfig,
) -> Result<PreparedStamp> {
    let mut prepared = create_pending_stamp(data)?;
    prepared.estimated_fee = 5000; // Placeholder: ~5000 sompi minimum fee
    Ok(prepared)
}

/// A prepared stamp ready for submission
#[derive(Clone, Debug)]
pub struct PreparedStamp {
    /// Proof structure (pending attestation)
    pub proof: KtcsProof,
    /// The commitment that will be put in OP_RETURN
    pub commitment: [u8; 32],
    /// Nonce used in commitment
    pub nonce: [u8; 16],
    /// Original data hash
    pub data_hash: [u8; 32],
    /// Estimated fee in sompi
    pub estimated_fee: u64,
}

/// Create a stamp directly from data (offline mode)
///
/// This creates a pending proof that can be completed later when
/// connected to the network.
#[cfg(feature = "keygen")]
pub fn create_pending_stamp(data: &[u8]) -> Result<PreparedStamp> {
    let nonce = generate_nonce()?;
    let data_hash = sha256(data);
    let commitment = create_commitment(&nonce, &data_hash);

    let mut proof = KtcsProof::new(data_hash.to_vec());
    proof.add_operation(Operation::Prepend(nonce.to_vec()));
    proof.add_operation(Operation::Sha256);

    Ok(PreparedStamp {
        proof,
        commitment,
        nonce,
        data_hash,
        estimated_fee: 0,
    })
}

/// Complete a prepared stamp with block attestation
///
/// This is called after the transaction has been confirmed on-chain.
pub fn complete_stamp(
    mut prepared: PreparedStamp,
    block_info: DirectBlockInfo,
    tx_hash: [u8; 32],
) -> Result<KtcsProof> {
    let attestation = KaspaAttestation::new(
        block_info.daa_score,
        block_info.blue_score,
        block_info.hash,
        block_info.timestamp,
        tx_hash,
        0, // Output index (OP_RETURN is typically output 0)
        block_info.blue_work,
        block_info.parent_hashes,
    );

    prepared.proof.add_attestation(Attestation::Kaspa(attestation));
    Ok(prepared.proof)
}

/// Simplified block info for direct stamping
#[derive(Clone, Debug)]
pub struct DirectBlockInfo {
    pub hash: [u8; 32],
    pub daa_score: u64,
    pub blue_score: u64,
    pub timestamp: u64,
    pub blue_work: [u8; 32],
    pub parent_hashes: Vec<[u8; 32]>,
}

#[cfg(feature = "kaspa-client")]
impl From<BlockInfo> for DirectBlockInfo {
    fn from(block: BlockInfo) -> Self {
        Self {
            hash: block.hash,
            daa_score: block.daa_score,
            blue_score: block.blue_score,
            timestamp: block.timestamp,
            blue_work: block.blue_work,
            parent_hashes: block.parent_hashes,
        }
    }
}

// Note: Full direct stamping via stamp_direct() requires proper type alignment
// between CommitmentTransaction and the Kaspa RPC Transaction type.
// For now, use prepare_direct_stamp() + manual submission + complete_stamp()

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_pending_stamp() {
        let data = b"Hello, Kaspa Timestamping!";
        let prepared = create_pending_stamp(data).unwrap();

        assert_eq!(prepared.proof.digest.len(), 32);
        assert_eq!(prepared.nonce.len(), 16);
        assert_eq!(prepared.commitment.len(), 32);
    }

    #[test]
    fn test_complete_stamp() {
        let data = b"Test data for stamping";
        let prepared = create_pending_stamp(data).unwrap();

        let block_info = DirectBlockInfo {
            hash: [0xab; 32],
            daa_score: 42000000,
            blue_score: 41500000,
            timestamp: 1737627600000,
            blue_work: [0x00; 32],
            parent_hashes: vec![[0xcd; 32]],
        };
        let tx_hash = [0xef; 32];

        let proof = complete_stamp(prepared, block_info, tx_hash).unwrap();

        // Verify the proof has an attestation
        assert!(!proof.attestations.is_empty());
    }

    #[test]
    fn test_direct_stamp_config_default() {
        let config = DirectStampConfig::default();

        assert_eq!(config.fee_rate, 1);
        assert_eq!(config.confirmation_timeout_secs, 60);
    }
}
