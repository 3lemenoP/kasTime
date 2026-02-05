//! Proof verification logic
//!
//! Implements the verification procedure for KTCS proofs as specified
//! in section 6 of the technical specification.

use crate::error::{KtcsError, Result};
use crate::merkle::sha256;
use crate::ops::apply_operations;
use crate::types::{Attestation, KaspaAttestation, KtcsProof};
use serde::{Deserialize, Serialize};

#[cfg(feature = "kaspa-client")]
use crate::kaspa::KaspaClient;

/// Result of proof verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    /// Whether the proof is valid
    pub valid: bool,
    /// The original digest from the proof
    pub digest: String,
    /// The computed commitment (result of applying operations)
    pub computed_commitment: String,
    /// Information about each attestation
    pub attestations: Vec<AttestationInfo>,
    /// Error message if verification failed
    pub error: Option<String>,
}

/// Information about an attestation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationInfo {
    /// Type of attestation
    pub attestation_type: String,
    /// Whether this attestation is complete
    pub complete: bool,
    /// Additional details (varies by attestation type)
    pub details: AttestationDetails,
}

/// Attestation-specific details
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AttestationDetails {
    Pending {
        calendar_url: String,
    },
    Kaspa {
        daa_score: u64,
        blue_score: u64,
        block_hash: String,
        timestamp: u64,
        tx_hash: String,
        blue_work: String,
        parent_count: usize,
    },
    Bitcoin {
        block_height: u32,
    },
}

/// Verify a KTCS proof
///
/// This performs the computational verification:
/// 1. If original data is provided, verify SHA256(data) matches the digest
/// 2. Apply all operations to the digest
/// 3. Return the computed commitment and attestation info
///
/// Note: Full verification against the Kaspa blockchain requires a node connection,
/// which is handled separately by the Kaspa client module.
pub fn verify_proof(proof: &KtcsProof, original_data: Option<&[u8]>) -> Result<VerificationResult> {
    let digest_hex = hex::encode(&proof.digest);

    // Step 1: Verify original data matches digest (if provided)
    if let Some(data) = original_data {
        let computed_hash = sha256(data);
        if computed_hash.to_vec() != proof.digest {
            return Ok(VerificationResult {
                valid: false,
                digest: digest_hex.clone(),
                computed_commitment: String::new(),
                attestations: Vec::new(),
                error: Some(format!(
                    "Data hash mismatch: expected {}, computed {}",
                    digest_hex,
                    hex::encode(computed_hash)
                )),
            });
        }
    }

    // Step 2: Apply operations to compute commitment
    let computed_commitment = match apply_operations(&proof.digest, &proof.operations) {
        Ok(commitment) => commitment,
        Err(e) => {
            return Ok(VerificationResult {
                valid: false,
                digest: digest_hex,
                computed_commitment: String::new(),
                attestations: Vec::new(),
                error: Some(format!("Failed to apply operations: {}", e)),
            });
        }
    };

    let commitment_hex = hex::encode(&computed_commitment);

    // Step 3: Check attestations
    if proof.attestations.is_empty() {
        return Ok(VerificationResult {
            valid: false,
            digest: digest_hex,
            computed_commitment: commitment_hex,
            attestations: Vec::new(),
            error: Some("No attestations in proof".to_string()),
        });
    }

    // Build attestation info
    let attestations: Vec<AttestationInfo> = proof
        .attestations
        .iter()
        .map(attestation_to_info)
        .collect();

    // Proof is valid if at least one attestation is complete
    let has_complete_attestation = proof.attestations.iter().any(|a| a.is_complete());

    Ok(VerificationResult {
        valid: has_complete_attestation,
        digest: digest_hex,
        computed_commitment: commitment_hex,
        attestations,
        error: if !has_complete_attestation {
            Some("Proof is pending - no complete attestations".to_string())
        } else {
            None
        },
    })
}

/// Convert an attestation to info struct
fn attestation_to_info(attestation: &Attestation) -> AttestationInfo {
    match attestation {
        Attestation::Pending(pending) => AttestationInfo {
            attestation_type: "pending".to_string(),
            complete: false,
            details: AttestationDetails::Pending {
                calendar_url: pending.calendar_url.clone(),
            },
        },
        Attestation::Kaspa(ka) => AttestationInfo {
            attestation_type: "kaspa".to_string(),
            complete: true,
            details: AttestationDetails::Kaspa {
                daa_score: ka.daa_score,
                blue_score: ka.blue_score,
                block_hash: hex::encode(ka.block_hash),
                timestamp: ka.timestamp,
                tx_hash: hex::encode(ka.tx_hash),
                blue_work: hex::encode(ka.blue_work),
                parent_count: ka.parent_hashes.len(),
            },
        },
        Attestation::Bitcoin(btc) => AttestationInfo {
            attestation_type: "bitcoin".to_string(),
            complete: true,
            details: AttestationDetails::Bitcoin {
                block_height: btc.block_height,
            },
        },
    }
}

/// Compute thermodynamic security metrics for a Kaspa attestation
///
/// This calculates the accumulated proof-of-work since the attestation,
/// which represents the thermodynamic security of the timestamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThermodynamicMetrics {
    /// Blue work at the time of attestation
    pub blue_work_at_attestation: String,
    /// Current blue work (if available)
    pub current_blue_work: Option<String>,
    /// Accumulated blue work since attestation
    pub accumulated_blue_work: Option<String>,
    /// Number of blocks since attestation
    pub blocks_since: Option<u64>,
    /// Rough equivalent Bitcoin confirmations (for comparison)
    pub btc_equivalent_confirmations: Option<f64>,
}

impl ThermodynamicMetrics {
    /// Create metrics from a Kaspa attestation
    pub fn from_attestation(attestation: &KaspaAttestation) -> Self {
        Self {
            blue_work_at_attestation: hex::encode(attestation.blue_work),
            current_blue_work: None,
            accumulated_blue_work: None,
            blocks_since: None,
            btc_equivalent_confirmations: None,
        }
    }

    /// Update metrics with current chain state
    pub fn with_current_state(
        mut self,
        current_blue_work: [u8; 32],
        current_daa_score: u64,
        attestation_daa_score: u64,
    ) -> Self {
        self.current_blue_work = Some(hex::encode(current_blue_work));

        // Calculate accumulated work (simplified - treat as big integers)
        // In production, would use proper big integer arithmetic
        self.blocks_since = Some(current_daa_score.saturating_sub(attestation_daa_score));

        // Rough BTC equivalence: ~10 Kaspa minutes ≈ 1 BTC confirmation
        // (Based on spec section 9.3)
        if let Some(blocks) = self.blocks_since {
            // At 10 BPS, 6000 blocks = 10 minutes ≈ 0.1 BTC confirmations
            // So 60000 blocks = 100 minutes ≈ 1 BTC confirmation
            self.btc_equivalent_confirmations = Some(blocks as f64 / 60000.0);
        }

        self
    }
}

/// KTCS commitment prefix for identifying commitments in transaction payloads
pub const KTCS_COMMITMENT_PREFIX: &[u8] = b"KTCS";

/// Verify that a commitment matches what's in a transaction payload.
///
/// This parses the transaction payload and verifies the commitment is present.
/// KTCS commitments are expected to be prefixed with "KTCS" followed by the 32-byte commitment.
///
/// Returns true if the commitment is found in the payload.
pub fn verify_commitment_in_tx(computed_commitment: &[u8], tx_payload: &[u8]) -> bool {
    if computed_commitment.len() != 32 {
        return false;
    }

    // Check for KTCS-prefixed commitment: "KTCS" + 32-byte commitment = 36 bytes
    if tx_payload.len() >= 36
        && tx_payload.starts_with(KTCS_COMMITMENT_PREFIX)
        && &tx_payload[4..36] == computed_commitment
    {
        return true;
    }

    // Also check for bare commitment (just 32 bytes)
    if tx_payload.len() >= 32 {
        // Scan for the commitment anywhere in the payload
        for window in tx_payload.windows(32) {
            if window == computed_commitment {
                return true;
            }
        }
    }

    false
}

/// Verify a Kaspa attestation against the computed commitment.
///
/// This performs the following checks:
/// 1. Validates attestation structure
/// 2. Verifies the commitment would be valid in the referenced transaction
/// 3. Checks that attestation fields are within valid ranges
pub fn verify_kaspa_attestation(
    computed_commitment: &[u8; 32],
    attestation: &KaspaAttestation,
    tx_payload: Option<&[u8]>,
) -> Result<bool> {
    // Note: computed_commitment is already [u8; 32], no length check needed

    // Validate attestation version
    if attestation.version != 0x01 {
        return Err(KtcsError::InvalidData(format!(
            "Unknown attestation version: {}",
            attestation.version
        )));
    }

    // Validate block hash is not all zeros (would indicate invalid data)
    if attestation.block_hash == [0u8; 32] {
        return Err(KtcsError::InvalidData(
            "Invalid block hash (all zeros)".to_string(),
        ));
    }

    // Validate tx hash is not all zeros
    if attestation.tx_hash == [0u8; 32] {
        return Err(KtcsError::InvalidData(
            "Invalid transaction hash (all zeros)".to_string(),
        ));
    }

    // Validate DAA score is reasonable (greater than 0)
    if attestation.daa_score == 0 {
        return Err(KtcsError::InvalidData(
            "Invalid DAA score (zero)".to_string(),
        ));
    }

    // Validate timestamp is reasonable (after year 2020, before year 2100)
    const MIN_TIMESTAMP: u64 = 1577836800000; // 2020-01-01 00:00:00 UTC in ms
    const MAX_TIMESTAMP: u64 = 4102444800000; // 2100-01-01 00:00:00 UTC in ms
    if attestation.timestamp < MIN_TIMESTAMP || attestation.timestamp > MAX_TIMESTAMP {
        return Err(KtcsError::InvalidData(format!(
            "Invalid timestamp: {} (out of valid range)",
            attestation.timestamp
        )));
    }

    // If transaction payload is provided, verify commitment is present
    if let Some(payload) = tx_payload {
        if !verify_commitment_in_tx(computed_commitment, payload) {
            return Ok(false);
        }
    }

    Ok(true)
}

/// Compute the expected transaction payload for a given commitment.
///
/// This creates the standard KTCS payload format: "KTCS" + commitment
pub fn create_commitment_payload(commitment: &[u8; 32]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(36);
    payload.extend_from_slice(KTCS_COMMITMENT_PREFIX);
    payload.extend_from_slice(commitment);
    payload
}

/// Result of verifying an attestation against the live blockchain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainVerificationResult {
    /// Whether the block was found and verified on chain
    pub block_verified: bool,
    /// Whether the transaction was found in the block
    pub tx_verified: bool,
    /// Whether the commitment was found in the transaction payload
    pub commitment_verified: bool,
    /// Current DAA score of the network
    pub current_daa_score: u64,
    /// Number of blocks since the attestation
    pub blocks_since: u64,
}

/// Verify a Kaspa attestation against the live blockchain
///
/// This performs the following checks:
/// 1. Fetches the block with full transactions by hash
/// 2. Verifies the block's DAA score matches the attestation
/// 3. Verifies the transaction is in the block
/// 4. Verifies the commitment is in the transaction outputs
/// 5. Returns current chain state for thermodynamic metrics
///
/// # Arguments
///
/// * `client` - Connected KaspaClient
/// * `attestation` - The Kaspa attestation to verify
/// * `computed_commitment` - The commitment computed from the proof operations
///
/// # Returns
///
/// ChainVerificationResult with verification status and chain state
#[cfg(feature = "kaspa-client")]
pub async fn verify_attestation_on_chain(
    client: &KaspaClient,
    attestation: &KaspaAttestation,
    computed_commitment: &[u8; 32],
) -> Result<ChainVerificationResult> {
    // 1. Fetch block with full transactions
    let (block, transactions) = client.get_block_with_transactions(&attestation.block_hash).await?;

    // 2. Verify block metadata matches attestation
    let block_verified = block.daa_score == attestation.daa_score;
    if !block_verified {
        tracing::warn!(
            "DAA score mismatch: attestation has {}, block has {}",
            attestation.daa_score,
            block.daa_score
        );
    }

    // 3. Verify transaction is in block
    let tx_verified = block.transaction_ids.iter().any(|id| *id == attestation.tx_hash);
    if !tx_verified {
        tracing::warn!(
            "Transaction {} not found in block {}",
            hex::encode(attestation.tx_hash),
            hex::encode(attestation.block_hash)
        );
    }

    // 4. Verify commitment in transaction outputs
    let commitment_verified = if tx_verified {
        // Find our transaction in the block's transactions
        let tx_info = transactions.iter().find(|tx| tx.hash == attestation.tx_hash);

        match tx_info {
            Some(tx) => {
                let mut found = false;
                for output in &tx.outputs {
                    // Check if this output contains our commitment
                    // KTCS commitments are stored as: "KTCS" prefix + 32-byte commitment
                    let script = &output.script_public_key.script;
                    if verify_commitment_in_tx(computed_commitment, script) {
                        found = true;
                        break;
                    }
                }

                // Also scan for bare commitment in output scripts
                if !found {
                    for output in &tx.outputs {
                        if output.script_public_key.script.len() >= 32 {
                            for window in output.script_public_key.script.windows(32) {
                                if window == computed_commitment {
                                    found = true;
                                    break;
                                }
                            }
                        }
                        if found {
                            break;
                        }
                    }
                }

                found
            }
            None => {
                tracing::warn!("Transaction data not found in block response");
                false
            }
        }
    } else {
        false
    };

    // 5. Get current chain state for thermodynamic metrics
    let dag_info = client.get_block_dag_info().await?;
    let blocks_since = dag_info.current_daa_score.saturating_sub(attestation.daa_score);

    Ok(ChainVerificationResult {
        block_verified,
        tx_verified,
        commitment_verified,
        current_daa_score: dag_info.current_daa_score,
        blocks_since,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{KaspaAttestation, Operation, PendingAttestation};

    #[test]
    fn test_verify_proof_with_data() {
        let data = b"hello world";
        let digest = sha256(data);

        let mut proof = KtcsProof::new(digest.to_vec());
        proof.add_attestation(Attestation::Kaspa(KaspaAttestation::new(
            42000000,
            41500000,
            [0xde; 32],
            1706000000000,
            [0xab; 32],
            0,
            [0x12; 32],
            vec![[0x11; 32]],
        )));

        let result = verify_proof(&proof, Some(data)).unwrap();
        assert!(result.valid);
        assert!(result.error.is_none());
        assert_eq!(result.attestations.len(), 1);
    }

    #[test]
    fn test_verify_proof_data_mismatch() {
        let data = b"hello world";
        let wrong_data = b"wrong data";
        let digest = sha256(data);

        let mut proof = KtcsProof::new(digest.to_vec());
        proof.add_attestation(Attestation::Kaspa(KaspaAttestation::new(
            42000000,
            41500000,
            [0xde; 32],
            1706000000000,
            [0xab; 32],
            0,
            [0x12; 32],
            vec![],
        )));

        let result = verify_proof(&proof, Some(wrong_data)).unwrap();
        assert!(!result.valid);
        assert!(result.error.is_some());
        assert!(result.error.unwrap().contains("mismatch"));
    }

    #[test]
    fn test_verify_pending_proof() {
        let digest = [0xab; 32];
        let mut proof = KtcsProof::new(digest.to_vec());
        proof.add_attestation(Attestation::Pending(PendingAttestation {
            calendar_url: "https://calendar.ktcs.example.com".to_string(),
        }));

        let result = verify_proof(&proof, None).unwrap();
        assert!(!result.valid);
        assert!(result.error.is_some());
        assert!(result.error.unwrap().contains("pending"));
    }

    #[test]
    fn test_verify_proof_with_operations() {
        let digest = [0xab; 32];
        let mut proof = KtcsProof::new(digest.to_vec());

        // Add operations that would come from a Merkle proof
        proof.add_operation(Operation::Append(vec![0xcd; 32]));
        proof.add_operation(Operation::Sha256);

        proof.add_attestation(Attestation::Kaspa(KaspaAttestation::new(
            42000000,
            41500000,
            [0xde; 32],
            1706000000000,
            [0xab; 32],
            0,
            [0x12; 32],
            vec![],
        )));

        let result = verify_proof(&proof, None).unwrap();
        assert!(result.valid);
        assert!(!result.computed_commitment.is_empty());
    }

    #[test]
    fn test_thermodynamic_metrics() {
        let attestation = KaspaAttestation::new(
            42000000,
            41500000,
            [0xde; 32],
            1706000000000,
            [0xab; 32],
            0,
            [0x12; 32],
            vec![],
        );

        let metrics = ThermodynamicMetrics::from_attestation(&attestation);
        assert_eq!(metrics.blue_work_at_attestation, hex::encode([0x12; 32]));
        assert!(metrics.current_blue_work.is_none());

        // Update with current state
        let updated = metrics.with_current_state([0x13; 32], 42060000, 42000000);
        assert!(updated.current_blue_work.is_some());
        assert_eq!(updated.blocks_since, Some(60000));
        // 60000 blocks ≈ 1 BTC confirmation
        assert!((updated.btc_equivalent_confirmations.unwrap() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_no_attestations() {
        let digest = [0xab; 32];
        let proof = KtcsProof::new(digest.to_vec());

        let result = verify_proof(&proof, None).unwrap();
        assert!(!result.valid);
        assert!(result.error.unwrap().contains("No attestations"));
    }

    #[test]
    fn test_verify_commitment_in_tx_with_prefix() {
        let commitment = [0xab; 32];
        let mut payload = Vec::from(KTCS_COMMITMENT_PREFIX);
        payload.extend_from_slice(&commitment);

        assert!(verify_commitment_in_tx(&commitment, &payload));

        // Wrong commitment should fail
        let wrong_commitment = [0xcd; 32];
        assert!(!verify_commitment_in_tx(&wrong_commitment, &payload));
    }

    #[test]
    fn test_verify_commitment_in_tx_bare() {
        let commitment = [0xab; 32];

        // Bare commitment without prefix
        assert!(verify_commitment_in_tx(&commitment, &commitment));

        // Commitment embedded in larger payload
        let mut payload = vec![0x00; 10];
        payload.extend_from_slice(&commitment);
        payload.extend_from_slice(&[0x00; 10]);
        assert!(verify_commitment_in_tx(&commitment, &payload));
    }

    #[test]
    fn test_verify_commitment_not_found() {
        let commitment = [0xab; 32];
        let payload = [0x00; 64]; // No matching commitment

        assert!(!verify_commitment_in_tx(&commitment, &payload));
    }

    #[test]
    fn test_verify_kaspa_attestation_valid() {
        let commitment = [0xab; 32];
        let attestation = KaspaAttestation::new(
            42000000,
            41500000,
            [0xde; 32],
            1706000000000, // Valid timestamp
            [0xab; 32],
            0,
            [0x12; 32],
            vec![[0x11; 32]],
        );

        let result = verify_kaspa_attestation(&commitment, &attestation, None).unwrap();
        assert!(result);
    }

    #[test]
    fn test_verify_kaspa_attestation_with_payload() {
        let commitment = [0xab; 32];
        let payload = create_commitment_payload(&commitment);
        let attestation = KaspaAttestation::new(
            42000000,
            41500000,
            [0xde; 32],
            1706000000000,
            [0xab; 32],
            0,
            [0x12; 32],
            vec![],
        );

        let result = verify_kaspa_attestation(&commitment, &attestation, Some(&payload)).unwrap();
        assert!(result);

        // Wrong payload should fail
        let wrong_payload = create_commitment_payload(&[0xcd; 32]);
        let result =
            verify_kaspa_attestation(&commitment, &attestation, Some(&wrong_payload)).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_verify_kaspa_attestation_invalid_block_hash() {
        let commitment = [0xab; 32];
        let attestation = KaspaAttestation::new(
            42000000,
            41500000,
            [0x00; 32], // Invalid: all zeros
            1706000000000,
            [0xab; 32],
            0,
            [0x12; 32],
            vec![],
        );

        let result = verify_kaspa_attestation(&commitment, &attestation, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_create_commitment_payload() {
        let commitment = [0xab; 32];
        let payload = create_commitment_payload(&commitment);

        assert_eq!(payload.len(), 36);
        assert!(payload.starts_with(KTCS_COMMITMENT_PREFIX));
        assert_eq!(&payload[4..], &commitment);
    }
}
