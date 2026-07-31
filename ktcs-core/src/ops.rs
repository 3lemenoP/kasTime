//! Commitment operations - apply transformations to hash states
//!
//! This module implements the operations that transform a digest through
//! a sequence of steps to arrive at the final commitment.

use crate::error::{KtcsError, Result};
use crate::types::Operation;
use sha2::{Digest, Sha256};
use sha3::Keccak256;

/// Compute a hash using the given hasher type
fn compute_hash<D: Digest>(data: &[u8]) -> Vec<u8> {
    let mut hasher = D::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

/// Apply a single operation to the current hash state
pub fn apply_operation(current: &[u8], op: &Operation) -> Result<Vec<u8>> {
    match op {
        Operation::Append(data) => {
            let mut result = current.to_vec();
            result.extend_from_slice(data);
            Ok(result)
        }
        Operation::Prepend(data) => {
            let mut result = data.clone();
            result.extend_from_slice(current);
            Ok(result)
        }
        Operation::Sha256 => Ok(compute_hash::<Sha256>(current)),
        Operation::Ripemd160 => {
            use ripemd::Ripemd160;
            Ok(compute_hash::<Ripemd160>(current))
        }
        Operation::Keccak256 => Ok(compute_hash::<Keccak256>(current)),
        Operation::Fork(_) => Err(KtcsError::Other(
            "Fork operation cannot be applied directly".to_string(),
        )),
    }
}

/// Maximum number of operations allowed to prevent CPU DoS attacks
pub const MAX_OPERATIONS_TO_APPLY: usize = 10_000;

/// Maximum cumulative intermediate-state size (in bytes) allowed while applying
/// operations. Each op datum may be up to 1 MB and up to 10,000 ops are
/// permitted; Append/Prepend can otherwise grow the running state to ~10 GB with
/// O(n^2) copying from a legal-but-huge `.kts`. This bound caps the working set
/// so proof application cannot be turned into a memory/CPU DoS.
pub const MAX_STATE_SIZE: usize = 10 * 1024 * 1024; // 10 MB

/// Apply a sequence of operations to a starting digest
///
/// # Errors
///
/// Returns an error if the number of operations exceeds `MAX_OPERATIONS_TO_APPLY`
/// or if the cumulative intermediate state grows beyond `MAX_STATE_SIZE`.
pub fn apply_operations(digest: &[u8], operations: &[Operation]) -> Result<Vec<u8>> {
    if operations.len() > MAX_OPERATIONS_TO_APPLY {
        return Err(KtcsError::InvalidData(format!(
            "Operation count {} exceeds limit {}",
            operations.len(),
            MAX_OPERATIONS_TO_APPLY
        )));
    }

    let mut current = digest.to_vec();

    for op in operations {
        // Bound the state BEFORE growing it further: reject if the next
        // Append/Prepend would push the running state past the cap.
        if let Operation::Append(data) | Operation::Prepend(data) = op {
            if current.len().saturating_add(data.len()) > MAX_STATE_SIZE {
                return Err(KtcsError::InvalidData(format!(
                    "Intermediate state size would exceed limit {} bytes",
                    MAX_STATE_SIZE
                )));
            }
        }

        current = apply_operation(&current, op)?;

        // Defensive: also cap after hashing ops (they shrink, but keep the
        // invariant explicit).
        if current.len() > MAX_STATE_SIZE {
            return Err(KtcsError::InvalidData(format!(
                "Intermediate state size {} exceeds limit {} bytes",
                current.len(),
                MAX_STATE_SIZE
            )));
        }
    }

    Ok(current)
}

/// Create a commitment from original data hash with a privacy nonce
///
/// The commitment is: SHA256(nonce || SHA256(data))
/// This provides privacy - the original data hash is not revealed in the commitment.
pub fn create_commitment(data_hash: &[u8; 32], nonce: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(nonce);
    hasher.update(data_hash);
    let result = hasher.finalize();
    let mut commitment = [0u8; 32];
    commitment.copy_from_slice(&result);
    commitment
}

/// Create a commitment with a cryptographically secure random nonce
#[cfg(all(not(target_arch = "wasm32"), feature = "keygen"))]
pub fn create_commitment_with_random_nonce(data_hash: &[u8; 32]) -> ([u8; 32], [u8; 16]) {
    use rand::RngCore;

    let mut nonce = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut nonce);

    let commitment = create_commitment(data_hash, &nonce);
    (commitment, nonce)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_append_operation() {
        let current = vec![0x01, 0x02];
        let op = Operation::Append(vec![0x03, 0x04]);
        let result = apply_operation(&current, &op).unwrap();
        assert_eq!(result, vec![0x01, 0x02, 0x03, 0x04]);
    }

    #[test]
    fn test_prepend_operation() {
        let current = vec![0x01, 0x02];
        let op = Operation::Prepend(vec![0x03, 0x04]);
        let result = apply_operation(&current, &op).unwrap();
        assert_eq!(result, vec![0x03, 0x04, 0x01, 0x02]);
    }

    #[test]
    fn test_sha256_operation() {
        let data = b"hello";
        let op = Operation::Sha256;
        let result = apply_operation(data, &op).unwrap();

        // Known SHA256 of "hello"
        let expected =
            hex::decode("2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824")
                .unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_ripemd160_operation() {
        let data = b"hello";
        let op = Operation::Ripemd160;
        let result = apply_operation(data, &op).unwrap();

        // Known RIPEMD160 of "hello"
        let expected = hex::decode("108f07b8382412612c048d07d13f814118445acd").unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_apply_operations_sequence() {
        let digest = vec![0x01, 0x02];
        let operations = vec![Operation::Append(vec![0x03, 0x04]), Operation::Sha256];

        let result = apply_operations(&digest, &operations).unwrap();

        // Should be SHA256 of [0x01, 0x02, 0x03, 0x04]
        let expected_input = vec![0x01, 0x02, 0x03, 0x04];
        let mut hasher = Sha256::new();
        hasher.update(&expected_input);
        let expected = hasher.finalize().to_vec();

        assert_eq!(result, expected);
    }

    #[test]
    fn test_create_commitment() {
        let data_hash = [0xab; 32];
        let nonce = [0x01, 0x02, 0x03, 0x04];

        let commitment = create_commitment(&data_hash, &nonce);

        // Verify it's a valid SHA256 hash (32 bytes)
        assert_eq!(commitment.len(), 32);

        // Verify it's deterministic
        let commitment2 = create_commitment(&data_hash, &nonce);
        assert_eq!(commitment, commitment2);

        // Verify different nonce gives different result
        let nonce2 = [0x05, 0x06, 0x07, 0x08];
        let commitment3 = create_commitment(&data_hash, &nonce2);
        assert_ne!(commitment, commitment3);
    }

    #[test]
    fn test_merkle_proof_operations() {
        // Simulate a Merkle proof with one sibling on the right
        let sibling = [0xcd; 32];
        let operations = vec![Operation::Append(sibling.to_vec()), Operation::Sha256];

        let leaf = [0xab; 32];
        let result = apply_operations(&leaf, &operations).unwrap();

        // Manually compute expected result
        let mut hasher = Sha256::new();
        hasher.update(&leaf);
        hasher.update(&sibling);
        let expected = hasher.finalize().to_vec();

        assert_eq!(result, expected);
    }

    #[test]
    fn test_apply_operations_state_size_cap() {
        // A sequence of large Append ops that cumulatively exceeds MAX_STATE_SIZE
        // must be rejected rather than allowed to balloon memory.
        let digest = vec![0u8; 32];
        let chunk = vec![0xAAu8; 1024 * 1024]; // 1 MB (== MAX_OPERATION_DATA_SIZE)
                                               // 11 x 1MB appends -> 11 MB > 10 MB cap.
        let operations: Vec<Operation> =
            (0..11).map(|_| Operation::Append(chunk.clone())).collect();

        let result = apply_operations(&digest, &operations);
        assert!(
            result.is_err(),
            "oversized cumulative state must be rejected"
        );
        assert!(result.unwrap_err().to_string().contains("exceed"));

        // A modest sequence well under the cap still succeeds.
        let small_ops = vec![Operation::Append(vec![0x01; 1024]), Operation::Sha256];
        assert!(apply_operations(&digest, &small_ops).is_ok());
    }

    #[test]
    fn test_keccak256_operation() {
        let data = b"hello";
        let op = Operation::Keccak256;
        let result = apply_operation(data, &op).unwrap();

        // Known Keccak256 of "hello"
        let expected =
            hex::decode("1c8aff950685c2ed4bc3174f3472287b56d9517b9c948127319a09a7a36deac8")
                .unwrap();
        assert_eq!(result, expected);
    }
}
