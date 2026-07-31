//! Proof verification logic
//!
//! Implements the verification procedure for KTCS proofs as specified
//! in section 6 of the technical specification.

use crate::error::{KtcsError, Result};
use crate::kaspa_types::extract_commitment_from_script;
use crate::merkle::sha256;
use crate::ops::apply_operations;
use crate::types::{Attestation, KaspaAttestation, KtcsProof};
use serde::{Deserialize, Serialize};

#[cfg(feature = "kaspa-client")]
use crate::kaspa::KaspaClient;

/// Result of proof verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    /// Whether the proof is valid.
    ///
    /// For OFFLINE verification (`verify_proof`) this means: the operations
    /// applied cleanly, and the proof carries a structurally-valid complete
    /// (Kaspa) attestation. It does **NOT** mean the attestation was checked
    /// against the live chain — see `chain_verified`.
    pub valid: bool,
    /// The original digest from the proof
    pub digest: String,
    /// The computed commitment (result of applying operations)
    pub computed_commitment: String,
    /// Whether the complete attestation is structurally valid (non-zero
    /// block/tx hashes, sane DAA score and timestamp). Offline-checkable.
    pub structurally_valid: bool,
    /// Whether the attestation was verified against the live blockchain.
    ///
    /// This is ONLY ever `true` via the online path
    /// (`verify_attestation_on_chain`). Offline verification always leaves this
    /// `false`, because offline verification cannot prove chain inclusion.
    pub chain_verified: bool,
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

/// Verify a KTCS proof OFFLINE (no network access).
///
/// This performs the computational + structural verification:
/// 1. If original data is provided, verify SHA256(data) matches the digest
/// 2. Apply all operations to the digest to compute the commitment
/// 3. Require at least one COMPLETE (Kaspa) attestation, and run the structural
///    sanity checker [`verify_kaspa_attestation`] on every Kaspa attestation
///    (rejects zero block/tx hashes, zero DAA score, out-of-range timestamps).
///
/// # Trust boundary
///
/// Offline verification does **NOT** prove that the proof is anchored on the
/// Kaspa blockchain. It proves only that the proof is computationally
/// consistent and carries a *structurally* well-formed complete attestation.
/// A caller could still fabricate an attestation with plausible-but-fake
/// hashes and pass offline verification. To prove chain inclusion you MUST use
/// the online path ([`verify_attestation_on_chain`]), which sets
/// `chain_verified = true`. The `chain_verified` field returned here is always
/// `false`.
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
                structurally_valid: false,
                chain_verified: false,
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
                structurally_valid: false,
                chain_verified: false,
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
            structurally_valid: false,
            chain_verified: false,
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

    // Proof must carry at least one complete attestation...
    let has_complete_attestation = proof.attestations.iter().any(|a| a.is_complete());
    if !has_complete_attestation {
        return Ok(VerificationResult {
            valid: false,
            digest: digest_hex,
            computed_commitment: commitment_hex,
            structurally_valid: false,
            chain_verified: false,
            attestations,
            error: Some("Proof is pending - no complete attestations".to_string()),
        });
    }

    // ...and every Kaspa attestation must be STRUCTURALLY valid. We do not have
    // the on-chain transaction payload here, so we pass `None` (no tx-payload
    // binding); this is a structural sanity check only, NOT a chain check.
    let commitment32: Option<[u8; 32]> = computed_commitment.as_slice().try_into().ok();
    let mut structural_error: Option<String> = None;
    if let Some(commitment32) = commitment32 {
        for ka in proof.kaspa_attestations() {
            match verify_kaspa_attestation(&commitment32, ka, None) {
                Ok(true) => {}
                Ok(false) => {
                    structural_error =
                        Some("Attestation commitment binding check failed".to_string());
                    break;
                }
                Err(e) => {
                    structural_error =
                        Some(format!("Structurally invalid attestation: {}", e));
                    break;
                }
            }
        }
    } else {
        structural_error = Some(format!(
            "Computed commitment is not 32 bytes (got {})",
            computed_commitment.len()
        ));
    }

    let structurally_valid = structural_error.is_none();

    Ok(VerificationResult {
        valid: structurally_valid,
        digest: digest_hex,
        computed_commitment: commitment_hex,
        structurally_valid,
        // Offline verification never proves chain inclusion.
        chain_verified: false,
        attestations,
        error: structural_error,
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

/// Verify that a computed commitment is anchored by a given OUTPUT SCRIPT via an
/// EXACT P2PK burn-script match: `0x20 <32-byte commitment> 0xac`.
///
/// This is NOT a byte scan. The commitment must be the sole pushed key of a
/// canonical 34-byte burn script; a commitment appearing at any other offset,
/// inside a larger script, or in a change output's pubkey is rejected.
pub fn verify_commitment_in_tx(computed_commitment: &[u8], output_script: &[u8]) -> bool {
    match extract_commitment_from_script(output_script) {
        Some(c) => c.as_slice() == computed_commitment,
        None => false,
    }
}

/// Verify a Kaspa attestation against the computed commitment.
///
/// This performs the following STRUCTURAL checks (offline-checkable):
/// 1. Validates attestation version and that block/tx hashes are non-zero
/// 2. Checks that DAA score is non-zero and the timestamp is in a sane range
/// 3. If an output script is supplied, requires an EXACT P2PK burn-script match
///    binding the commitment to that output (no naive byte scan)
///
/// Passing `None` for `output_script` performs only the structural checks; it
/// does NOT prove the commitment is on chain.
pub fn verify_kaspa_attestation(
    computed_commitment: &[u8; 32],
    attestation: &KaspaAttestation,
    output_script: Option<&[u8]>,
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

    // If an output script is provided, require an exact burn-script match.
    if let Some(script) = output_script {
        if !verify_commitment_in_tx(computed_commitment, script) {
            return Ok(false);
        }
    }

    Ok(true)
}

/// Result of verifying an attestation against the live blockchain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainVerificationResult {
    /// Whether the block was found on chain AND its DAA score matches the
    /// attestation exactly (a DAA-score mismatch is a hard failure).
    pub block_verified: bool,
    /// Whether the transaction was found in the block
    pub tx_verified: bool,
    /// Whether the commitment was found in the transaction outputs via an EXACT
    /// P2PK burn-script match
    pub commitment_verified: bool,
    /// Whether the attestation timestamp matches the on-chain block timestamp
    /// within tolerance
    pub timestamp_verified: bool,
    /// Overall chain verification result: true only if the block, transaction,
    /// commitment (exact burn script) and timestamp all verified.
    pub chain_verified: bool,
    /// Current DAA score of the network
    pub current_daa_score: u64,
    /// Number of blocks since the attestation
    pub blocks_since: u64,
}

/// Tolerance (in milliseconds) allowed between the attestation timestamp and
/// the on-chain block timestamp. A few seconds accommodates clock skew /
/// rounding between the node's reported time and what was recorded at stamp
/// time, while still rejecting a forged timestamp.
pub const TIMESTAMP_TOLERANCE_MS: u64 = 5_000;

/// Verify a Kaspa attestation against the live blockchain.
///
/// This performs the following checks (all must pass for `chain_verified`):
/// 1. Fetches the block with full transactions by hash
/// 2. Validates the node's network matches the client's configured network
/// 3. Verifies the block's DAA score matches the attestation (hard failure)
/// 4. Verifies the transaction is in the block
/// 5. Verifies the commitment is in a transaction output via EXACT burn-script match
/// 6. Verifies the attestation timestamp matches the block timestamp (tolerance)
/// 7. Returns current chain state for thermodynamic metrics
///
/// # Arguments
///
/// * `client` - Connected KaspaClient (its config network is used for validation)
/// * `attestation` - The Kaspa attestation to verify
/// * `computed_commitment` - The commitment computed from the proof operations
///
/// # Returns
///
/// ChainVerificationResult with per-check flags and an overall `chain_verified`.
#[cfg(feature = "kaspa-client")]
pub async fn verify_attestation_on_chain(
    client: &KaspaClient,
    attestation: &KaspaAttestation,
    computed_commitment: &[u8; 32],
) -> Result<ChainVerificationResult> {
    // 1. Fetch block with full transactions
    let (block, transactions) = client.get_block_with_transactions(&attestation.block_hash).await?;

    // 1b. Network validation: the node's reported network must match the
    // network the client was configured for (threaded through the client config
    // rather than hardcoded). A node on the wrong network cannot attest to this
    // proof; treat a mismatch as a hard verification failure.
    let dag_info = client.get_block_dag_info().await?;
    if let Some(expected_network) = client.config().network.as_deref() {
        if !networks_match(expected_network, &dag_info.network) {
            return Err(KtcsError::VerificationFailed(format!(
                "Network mismatch: client expects '{}', node reports '{}'",
                expected_network, dag_info.network
            )));
        }
    }

    // 2. Verify block metadata matches attestation.
    // A DAA-score mismatch is a HARD failure: the attestation names a specific
    // block; if that block's DAA score differs, the attestation is inconsistent
    // with the chain and cannot be trusted.
    let block_verified = block.daa_score == attestation.daa_score;
    if !block_verified {
        tracing::warn!(
            "DAA score mismatch (hard failure): attestation has {}, block has {}",
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

    // 4. Verify commitment in transaction outputs via EXACT P2PK burn-script
    // match (no naive byte scan).
    let commitment_verified = if tx_verified {
        match transactions.iter().find(|tx| tx.hash == attestation.tx_hash) {
            Some(tx) => tx.outputs.iter().any(|output| {
                verify_commitment_in_tx(computed_commitment, &output.script_public_key.script)
            }),
            None => {
                tracing::warn!("Transaction data not found in block response");
                false
            }
        }
    } else {
        false
    };

    // 5. Verify the attestation timestamp against the on-chain block timestamp.
    // A forged timestamp on an otherwise-real block/tx must not pass.
    let timestamp_verified = attestation
        .timestamp
        .abs_diff(block.timestamp)
        <= TIMESTAMP_TOLERANCE_MS;
    if !timestamp_verified {
        tracing::warn!(
            "Timestamp mismatch: attestation {} vs block {} (tolerance {}ms)",
            attestation.timestamp,
            block.timestamp,
            TIMESTAMP_TOLERANCE_MS
        );
    }

    let blocks_since = dag_info.current_daa_score.saturating_sub(attestation.daa_score);

    let chain_verified =
        block_verified && tx_verified && commitment_verified && timestamp_verified;

    Ok(ChainVerificationResult {
        block_verified,
        tx_verified,
        commitment_verified,
        timestamp_verified,
        chain_verified,
        current_daa_score: dag_info.current_daa_score,
        blocks_since,
    })
}

/// Compare an expected network name against the one reported by the node.
///
/// Accepts common aliases: e.g. a client configured for "testnet" matches a
/// node reporting "testnet-10"/"testnet-11", and vice versa.
#[cfg(feature = "kaspa-client")]
fn networks_match(expected: &str, reported: &str) -> bool {
    let norm = |s: &str| -> String {
        let s = s.to_ascii_lowercase();
        // Collapse any testnet-NN variant to "testnet" for comparison.
        if s.starts_with("testnet") {
            "testnet".to_string()
        } else {
            s
        }
    };
    norm(expected) == norm(reported)
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

    /// Build the canonical P2PK burn script for a commitment: 0x20 <32> 0xac.
    fn burn_script(commitment: &[u8; 32]) -> Vec<u8> {
        let mut s = vec![0x20];
        s.extend_from_slice(commitment);
        s.push(0xac);
        s
    }

    #[test]
    fn test_verify_commitment_exact_burn_script_positive() {
        let commitment = [0xab; 32];
        let script = burn_script(&commitment);

        // Exact burn script for the commitment matches.
        assert!(verify_commitment_in_tx(&commitment, &script));

        // A different commitment's script does not match.
        let wrong = [0xcd; 32];
        assert!(!verify_commitment_in_tx(&wrong, &script));
    }

    #[test]
    fn test_verify_commitment_rejects_byte_scan() {
        let commitment = [0xab; 32];

        // Bare 32-byte commitment is NOT a valid burn script (no push/checksig).
        assert!(!verify_commitment_in_tx(&commitment, &commitment));

        // Commitment embedded at a non-canonical offset must be rejected (the
        // old naive byte-scan would have accepted this).
        let mut embedded = vec![0x00; 10];
        embedded.extend_from_slice(&commitment);
        embedded.extend_from_slice(&[0x00; 10]);
        assert!(!verify_commitment_in_tx(&commitment, &embedded));

        // A burn script wrapping a different commitment, with our commitment
        // hidden elsewhere, is rejected because match is on the pushed key only.
        let mut sneaky = burn_script(&[0xcd; 32]);
        sneaky.extend_from_slice(&commitment);
        assert!(!verify_commitment_in_tx(&commitment, &sneaky));
    }

    #[test]
    fn test_verify_commitment_not_found() {
        let commitment = [0xab; 32];
        let payload = [0x00; 64]; // No matching commitment

        assert!(!verify_commitment_in_tx(&commitment, &payload));
    }

    #[test]
    fn test_verify_proof_rejects_fabricated_zero_attestation() {
        // C1 regression: a proof carrying a structurally-INVALID complete
        // attestation (zero block/tx hashes) must NOT verify offline.
        let digest = [0xab; 32];
        let mut proof = KtcsProof::new(digest.to_vec());
        proof.add_attestation(Attestation::Kaspa(KaspaAttestation::new(
            42_000_000,
            41_500_000,
            [0x00; 32], // zero block hash -> structurally invalid
            1_706_000_000_000,
            [0x00; 32], // zero tx hash
            0,
            [0x12; 32],
            vec![],
        )));

        let result = verify_proof(&proof, None).unwrap();
        assert!(!result.valid, "fabricated zero-hash attestation must be invalid");
        assert!(!result.structurally_valid);
        assert!(!result.chain_verified);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_verify_proof_offline_is_never_chain_verified() {
        // Even a structurally-valid complete attestation must report
        // chain_verified=false from the offline path.
        let digest = [0xab; 32];
        let mut proof = KtcsProof::new(digest.to_vec());
        proof.add_attestation(Attestation::Kaspa(KaspaAttestation::new(
            42_000_000,
            41_500_000,
            [0xde; 32],
            1_706_000_000_000,
            [0xab; 32],
            0,
            [0x12; 32],
            vec![[0x11; 32]],
        )));

        let result = verify_proof(&proof, None).unwrap();
        assert!(result.valid);
        assert!(result.structurally_valid);
        assert!(!result.chain_verified, "offline verification never proves chain inclusion");
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
    fn test_verify_kaspa_attestation_with_output_script() {
        let commitment = [0xab; 32];
        let script = burn_script(&commitment);
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

        // Exact burn script for this commitment binds the attestation.
        let result = verify_kaspa_attestation(&commitment, &attestation, Some(&script)).unwrap();
        assert!(result);

        // A burn script for a different commitment must fail the binding.
        let wrong_script = burn_script(&[0xcd; 32]);
        let result =
            verify_kaspa_attestation(&commitment, &attestation, Some(&wrong_script)).unwrap();
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
}
