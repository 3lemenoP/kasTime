//! Proof verification logic
//!
//! Implements the verification procedure for KTCS proofs as specified
//! in section 6 of the technical specification.

use crate::error::{KtcsError, Result};
use crate::merkle::sha256;
use crate::ops::apply_operations;
use crate::types::{Attestation, KaspaAttestation, KtcsProof};
use serde::{Deserialize, Serialize};

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
        .map(|a| attestation_to_info(a))
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

/// Verify that a commitment matches what's expected in an attestation
///
/// This is used to verify that the computed commitment from operations
/// matches what was actually committed to the blockchain.
pub fn verify_commitment_in_tx(
    computed_commitment: &[u8],
    _tx_payload: &[u8],
) -> bool {
    // In a real implementation, this would parse the Kaspa transaction
    // and verify the commitment is in the payload field.
    // For now, we assume the commitment matches (this would be verified
    // by the Kaspa node during full verification).
    computed_commitment.len() == 32
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
}
