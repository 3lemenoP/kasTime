//! KTCS WebAssembly Bindings
//!
//! Provides JavaScript-accessible functions for verifying and parsing KTCS proofs
//! in the browser without requiring a server.

use ktcs_core::{
    deserialize_proof, merkle::sha256, serialize_proof, verify_proof as core_verify, KtcsProof,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

/// Initialize panic hook for better error messages in console
#[wasm_bindgen(start)]
pub fn init() {
    #[cfg(feature = "panic_hook")]
    console_error_panic_hook::set_once();
}

/// Result type returned to JavaScript
#[derive(Serialize, Deserialize)]
pub struct WasmVerificationResult {
    pub valid: bool,
    pub digest: String,
    pub computed_commitment: String,
    pub attestations: Vec<WasmAttestationInfo>,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct WasmAttestationInfo {
    pub attestation_type: String,
    pub complete: bool,
    pub daa_score: Option<u64>,
    pub blue_score: Option<u64>,
    pub block_hash: Option<String>,
    pub timestamp: Option<u64>,
    pub tx_hash: Option<String>,
    pub calendar_url: Option<String>,
}

/// Proof info returned when parsing
#[derive(Serialize, Deserialize)]
pub struct WasmProofInfo {
    pub version: u8,
    pub hash_algorithm: String,
    pub digest: String,
    pub operations_count: usize,
    pub attestations_count: usize,
    pub is_complete: bool,
    pub attestations: Vec<WasmAttestationInfo>,
}

/// Verify a KTCS proof
///
/// # Arguments
/// * `proof_bytes` - The raw .kts proof file bytes
/// * `data` - Optional original data for full verification
///
/// # Returns
/// A JsValue containing the verification result
#[wasm_bindgen]
pub fn verify_proof(proof_bytes: &[u8], data: Option<Vec<u8>>) -> JsValue {
    let result = verify_proof_internal(proof_bytes, data.as_deref());
    serde_wasm_bindgen::to_value(&result).unwrap_or(JsValue::NULL)
}

fn verify_proof_internal(proof_bytes: &[u8], data: Option<&[u8]>) -> WasmVerificationResult {
    // Deserialize proof
    let proof = match deserialize_proof(proof_bytes) {
        Ok(p) => p,
        Err(e) => {
            return WasmVerificationResult {
                valid: false,
                digest: String::new(),
                computed_commitment: String::new(),
                attestations: Vec::new(),
                error: Some(format!("Failed to parse proof: {}", e)),
            };
        }
    };

    // Verify
    let result = match core_verify(&proof, data) {
        Ok(r) => r,
        Err(e) => {
            return WasmVerificationResult {
                valid: false,
                digest: String::new(),
                computed_commitment: String::new(),
                attestations: Vec::new(),
                error: Some(format!("Verification error: {}", e)),
            };
        }
    };

    // Convert to WASM result
    let attestations: Vec<WasmAttestationInfo> = result
        .attestations
        .iter()
        .map(|a| {
            let mut info = WasmAttestationInfo {
                attestation_type: a.attestation_type.clone(),
                complete: a.complete,
                daa_score: None,
                blue_score: None,
                block_hash: None,
                timestamp: None,
                tx_hash: None,
                calendar_url: None,
            };

            match &a.details {
                ktcs_core::verify::AttestationDetails::Pending { calendar_url } => {
                    info.calendar_url = Some(calendar_url.clone());
                }
                ktcs_core::verify::AttestationDetails::Kaspa {
                    daa_score,
                    blue_score,
                    block_hash,
                    timestamp,
                    tx_hash,
                    ..
                } => {
                    info.daa_score = Some(*daa_score);
                    info.blue_score = Some(*blue_score);
                    info.block_hash = Some(block_hash.clone());
                    info.timestamp = Some(*timestamp);
                    info.tx_hash = Some(tx_hash.clone());
                }
                ktcs_core::verify::AttestationDetails::Bitcoin { .. } => {}
            }

            info
        })
        .collect();

    WasmVerificationResult {
        valid: result.valid,
        digest: result.digest,
        computed_commitment: result.computed_commitment,
        attestations,
        error: result.error,
    }
}

/// Parse a KTCS proof and return its information
///
/// # Arguments
/// * `proof_bytes` - The raw .kts proof file bytes
///
/// # Returns
/// A JsValue containing the proof info
#[wasm_bindgen]
pub fn parse_proof(proof_bytes: &[u8]) -> JsValue {
    let result = parse_proof_internal(proof_bytes);
    serde_wasm_bindgen::to_value(&result).unwrap_or(JsValue::NULL)
}

fn parse_proof_internal(proof_bytes: &[u8]) -> Result<WasmProofInfo, String> {
    let proof = deserialize_proof(proof_bytes).map_err(|e| format!("Parse error: {}", e))?;

    let attestations: Vec<WasmAttestationInfo> = proof
        .attestations
        .iter()
        .map(|a| match a {
            ktcs_core::Attestation::Pending(p) => WasmAttestationInfo {
                attestation_type: "pending".to_string(),
                complete: false,
                daa_score: None,
                blue_score: None,
                block_hash: None,
                timestamp: None,
                tx_hash: None,
                calendar_url: Some(p.calendar_url.clone()),
            },
            ktcs_core::Attestation::Kaspa(ka) => WasmAttestationInfo {
                attestation_type: "kaspa".to_string(),
                complete: true,
                daa_score: Some(ka.daa_score),
                blue_score: Some(ka.blue_score),
                block_hash: Some(hex::encode(ka.block_hash)),
                timestamp: Some(ka.timestamp),
                tx_hash: Some(hex::encode(ka.tx_hash)),
                calendar_url: None,
            },
            ktcs_core::Attestation::Bitcoin(btc) => WasmAttestationInfo {
                attestation_type: "bitcoin".to_string(),
                complete: true,
                daa_score: None,
                blue_score: None,
                block_hash: None,
                timestamp: None,
                tx_hash: None,
                calendar_url: None,
            },
        })
        .collect();

    Ok(WasmProofInfo {
        version: proof.version,
        hash_algorithm: format!("{:?}", proof.hash_algorithm),
        digest: hex::encode(&proof.digest),
        operations_count: proof.operations.len(),
        attestations_count: proof.attestations.len(),
        is_complete: proof.is_complete(),
        attestations,
    })
}

/// Serialize a proof to .kts format
///
/// This is primarily for testing - proofs are normally created by the calendar server
#[wasm_bindgen]
pub fn serialize_proof_to_bytes(proof_json: JsValue) -> Result<Vec<u8>, JsValue> {
    let proof: KtcsProof = serde_wasm_bindgen::from_value(proof_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid proof JSON: {}", e)))?;

    Ok(serialize_proof(&proof))
}

/// Compute SHA256 hash of data
///
/// # Arguments
/// * `data` - The data to hash
///
/// # Returns
/// 32-byte SHA256 hash as Uint8Array
#[wasm_bindgen]
pub fn compute_sha256(data: &[u8]) -> Vec<u8> {
    sha256(data).to_vec()
}

/// Compute SHA256 hash and return as hex string
#[wasm_bindgen]
pub fn compute_sha256_hex(data: &[u8]) -> String {
    hex::encode(sha256(data))
}

/// Get the KTCS library version
#[wasm_bindgen]
pub fn get_version() -> String {
    ktcs_core::VERSION.to_string()
}

/// Check if proof bytes appear to be a valid KTCS proof (quick check)
#[wasm_bindgen]
pub fn is_valid_proof_format(proof_bytes: &[u8]) -> bool {
    if proof_bytes.len() < 18 {
        return false;
    }
    &proof_bytes[0..18] == ktcs_core::KTCS_MAGIC
}

#[cfg(test)]
mod tests {
    use super::*;
    use ktcs_core::{Attestation, KaspaAttestation, PendingAttestation};

    #[test]
    fn test_verify_proof() {
        let digest = sha256(b"test data");
        let mut proof = KtcsProof::new(digest.to_vec());
        proof.add_attestation(Attestation::Kaspa(KaspaAttestation::new(
            42000000,
            41500000,
            [0xab; 32],
            1706000000000,
            [0xcd; 32],
            0,
            [0x12; 32],
            vec![[0x11; 32]],
        )));

        let proof_bytes = serialize_proof(&proof);
        let result = verify_proof_internal(&proof_bytes, Some(b"test data"));

        assert!(result.valid);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_parse_proof() {
        let digest = sha256(b"test");
        let mut proof = KtcsProof::new(digest.to_vec());
        proof.add_attestation(Attestation::Pending(PendingAttestation {
            calendar_url: "https://example.com".to_string(),
        }));

        let proof_bytes = serialize_proof(&proof);
        let info = parse_proof_internal(&proof_bytes).unwrap();

        assert_eq!(info.version, 1);
        assert!(!info.is_complete);
        assert_eq!(info.attestations.len(), 1);
        assert_eq!(info.attestations[0].attestation_type, "pending");
    }

    #[test]
    fn test_sha256() {
        let hash = compute_sha256(b"hello world");
        let hex = compute_sha256_hex(b"hello world");

        assert_eq!(hash.len(), 32);
        assert_eq!(
            hex,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_is_valid_format() {
        let digest = [0u8; 32];
        let proof = KtcsProof::new(digest.to_vec());
        let bytes = serialize_proof(&proof);

        assert!(is_valid_proof_format(&bytes));
        assert!(!is_valid_proof_format(&[0u8; 10]));
        assert!(!is_valid_proof_format(&[0u8; 50]));
    }
}
