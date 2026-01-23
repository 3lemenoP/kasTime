//! KTCS Core Library
//!
//! Core library for the Kaspa Thermodynamic Clock Service (KTCS).
//! Provides proof format serialization, Merkle tree operations, and verification logic.
//!
//! # Overview
//!
//! KTCS is a trustless timestamping protocol that leverages Kaspa's high-throughput
//! BlockDAG to provide sub-second proof-of-existence attestations.
//!
//! # Features
//!
//! - **Proof Format**: Binary .kts file format for timestamps
//! - **Merkle Trees**: Batch aggregation for calendar services
//! - **Verification**: Cryptographic verification of proofs
//! - **Operations**: Hash transformations (SHA256, RIPEMD160, etc.)
//!
//! # Example
//!
//! ```rust
//! use ktcs_core::{KtcsProof, serialize_proof, deserialize_proof, verify_proof};
//! use ktcs_core::merkle::{MerkleTree, sha256};
//!
//! // Create a proof from a document hash
//! let document = b"Important document content";
//! let digest = sha256(document);
//!
//! let mut proof = KtcsProof::new(digest.to_vec());
//!
//! // Serialize to .kts format
//! let kts_data = serialize_proof(&proof);
//!
//! // Deserialize and verify
//! let loaded = deserialize_proof(&kts_data).unwrap();
//! let result = verify_proof(&loaded, Some(document)).unwrap();
//! ```

pub mod error;
pub mod merkle;
pub mod ops;
pub mod proof;
pub mod types;
pub mod verify;

// Re-export commonly used items
pub use error::{KtcsError, Result};
pub use merkle::{MerkleProof, MerkleTree};
pub use ops::{apply_operation, apply_operations, create_commitment};
pub use proof::{deserialize_proof, serialize_proof};
pub use types::{
    Attestation, BatchMode, BitcoinAttestation, HashAlgorithm, KaspaAttestation, KtcsProof,
    Operation, PendingAttestation, KTCS_MAGIC, PROOF_VERSION,
};
pub use verify::{verify_proof, AttestationInfo, ThermodynamicMetrics, VerificationResult};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::merkle::sha256;

    #[test]
    fn test_full_workflow() {
        // Simulate a complete timestamp workflow

        // 1. User has a document to timestamp
        let document = b"This is my important document that needs timestamping";
        let document_hash = sha256(document);

        // 2. User submits hash to calendar (simulated)
        // Calendar batches multiple hashes into a Merkle tree
        let other_hash1 = sha256(b"Other document 1");
        let other_hash2 = sha256(b"Other document 2");
        let other_hash3 = sha256(b"Other document 3");

        let tree =
            MerkleTree::build(vec![document_hash, other_hash1, other_hash2, other_hash3]).unwrap();

        // 3. Calendar commits Merkle root to Kaspa (simulated)
        let _merkle_root = tree.root();

        // 4. Calendar generates proof for user's document
        let merkle_proof = tree.get_proof(0).unwrap(); // Our document is at index 0
        assert!(merkle_proof.verify());

        // 5. Build the KTCS proof
        let mut proof = KtcsProof::new(document_hash.to_vec());

        // Add Merkle proof operations
        for op in merkle_proof.to_operations() {
            proof.add_operation(op);
        }

        // Add Kaspa attestation (simulated block data)
        proof.add_attestation(Attestation::Kaspa(KaspaAttestation::new(
            42000000,  // DAA score
            41500000,  // Blue score
            [0xab; 32], // Block hash
            1706000000000, // Timestamp
            [0xcd; 32], // TX hash
            0,         // TX index
            [0x12; 32], // Blue work
            vec![[0x11; 32], [0x22; 32]], // Parent hashes
        )));

        // 6. Serialize proof to .kts format
        let kts_data = serialize_proof(&proof);
        assert!(!kts_data.is_empty());

        // 7. Later: Load and verify the proof
        let loaded_proof = deserialize_proof(&kts_data).unwrap();

        // Verify with original document
        let verification = verify_proof(&loaded_proof, Some(document)).unwrap();
        assert!(verification.valid, "Verification failed: {:?}", verification.error);
        assert_eq!(verification.digest, hex::encode(document_hash));
        assert!(!verification.computed_commitment.is_empty());
        assert_eq!(verification.attestations.len(), 1);

        // 8. Check thermodynamic security
        let kaspa_attestation = loaded_proof.kaspa_attestations().next().unwrap();
        let metrics = ThermodynamicMetrics::from_attestation(kaspa_attestation);
        assert!(!metrics.blue_work_at_attestation.is_empty());
    }

    #[test]
    fn test_pending_proof_workflow() {
        // Workflow for a pending proof (not yet confirmed)

        let document = b"Document waiting for confirmation";
        let document_hash = sha256(document);

        // Create pending proof
        let mut proof = KtcsProof::new(document_hash.to_vec());
        proof.add_attestation(Attestation::Pending(PendingAttestation {
            calendar_url: "https://calendar.ktcs.kaspa.org/v1/stamp/ktcs_abc123".to_string(),
        }));

        // Serialize
        let kts_data = serialize_proof(&proof);

        // Load and check
        let loaded = deserialize_proof(&kts_data).unwrap();
        assert!(!loaded.is_complete());

        let verification = verify_proof(&loaded, Some(document)).unwrap();
        assert!(!verification.valid); // Not valid because pending
        assert!(verification.error.unwrap().contains("pending"));
    }

    #[test]
    fn test_proof_with_nonce() {
        // Privacy-preserving workflow with nonce

        let document = b"Private document";
        let document_hash = sha256(document);

        // Create commitment with nonce for privacy
        let nonce = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        let commitment = create_commitment(&document_hash, &nonce);

        // Proof starts from the document hash
        let mut proof = KtcsProof::new(document_hash.to_vec());

        // Add nonce operation (prepend nonce, then hash)
        proof.add_operation(Operation::Prepend(nonce.to_vec()));
        proof.add_operation(Operation::Sha256);

        // The computed commitment after operations should match
        let result = apply_operations(&document_hash, &proof.operations).unwrap();
        assert_eq!(result, commitment.to_vec());
    }
}
