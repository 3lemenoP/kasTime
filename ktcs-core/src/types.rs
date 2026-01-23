//! Core types for KTCS proof format
//!
//! Defines the data structures for timestamps, proofs, operations, and attestations
//! as specified in the KTCS technical specification.

use serde::{Deserialize, Serialize};

/// Magic bytes for .kts file format: 0x00 "KaspaTime" 0x00 0x00 "Proof" 0x00
pub const KTCS_MAGIC: &[u8] = &[
    0x00, 0x4b, 0x61, 0x73, 0x70, 0x61, 0x54, 0x69, 0x6d, 0x65, // 0x00 "KaspaTime"
    0x00, 0x00, 0x50, 0x72, 0x6f, 0x6f, 0x66, 0x00, // 0x00 0x00 "Proof" 0x00
];

/// Current proof format version
pub const PROOF_VERSION: u8 = 0x01;

/// Hash algorithm identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum HashAlgorithm {
    Sha256 = 0x08,
    Ripemd160 = 0x14,
    Keccak256 = 0x67,
}

impl TryFrom<u8> for HashAlgorithm {
    type Error = crate::error::KtcsError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x08 => Ok(HashAlgorithm::Sha256),
            0x14 => Ok(HashAlgorithm::Ripemd160),
            0x67 => Ok(HashAlgorithm::Keccak256),
            _ => Err(crate::error::KtcsError::InvalidHashAlgorithm(value)),
        }
    }
}

/// Operation tags for commitment sequence
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OpTag {
    /// Append bytes to current hash state: H' = current || data
    Append = 0xF0,
    /// Prepend bytes to current hash state: H' = data || current
    Prepend = 0xF1,
    /// Apply SHA256: H' = SHA256(current)
    Sha256 = 0x08,
    /// Apply RIPEMD160: H' = RIPEMD160(current)
    Ripemd160 = 0x14,
    /// Apply Keccak256: H' = Keccak256(current)
    Keccak256 = 0x67,
    /// Fork into multiple paths
    Fork = 0xFF,
}

impl TryFrom<u8> for OpTag {
    type Error = crate::error::KtcsError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0xF0 => Ok(OpTag::Append),
            0xF1 => Ok(OpTag::Prepend),
            0x08 => Ok(OpTag::Sha256),
            0x14 => Ok(OpTag::Ripemd160),
            0x67 => Ok(OpTag::Keccak256),
            0xFF => Ok(OpTag::Fork),
            _ => Err(crate::error::KtcsError::InvalidOperationTag(value)),
        }
    }
}

/// Attestation type tags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AttestationTag {
    /// Pending attestation - proof incomplete, requires calendar upgrade
    Pending = 0x83,
    /// Kaspa block attestation - complete proof anchored to Kaspa
    KaspaBlock = 0x84,
    /// Bitcoin attestation - for dual-anchor mode (OTS compatible)
    Bitcoin = 0x05,
}

impl TryFrom<u8> for AttestationTag {
    type Error = crate::error::KtcsError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x83 => Ok(AttestationTag::Pending),
            0x84 => Ok(AttestationTag::KaspaBlock),
            0x05 => Ok(AttestationTag::Bitcoin),
            _ => Err(crate::error::KtcsError::InvalidAttestationTag(value)),
        }
    }
}

/// An operation that transforms the hash state
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Operation {
    /// Append bytes to current state
    Append(Vec<u8>),
    /// Prepend bytes to current state
    Prepend(Vec<u8>),
    /// Apply SHA256 hash
    Sha256,
    /// Apply RIPEMD160 hash
    Ripemd160,
    /// Apply Keccak256 hash
    Keccak256,
    /// Fork into multiple parallel paths (count of branches)
    Fork(u32),
}

impl Operation {
    /// Get the tag byte for this operation
    pub fn tag(&self) -> u8 {
        match self {
            Operation::Append(_) => OpTag::Append as u8,
            Operation::Prepend(_) => OpTag::Prepend as u8,
            Operation::Sha256 => OpTag::Sha256 as u8,
            Operation::Ripemd160 => OpTag::Ripemd160 as u8,
            Operation::Keccak256 => OpTag::Keccak256 as u8,
            Operation::Fork(_) => OpTag::Fork as u8,
        }
    }
}

/// Pending attestation - proof is incomplete and needs calendar upgrade
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingAttestation {
    /// URL of the calendar server to retrieve the complete proof
    pub calendar_url: String,
}

/// Kaspa block attestation - complete proof anchored to Kaspa blockchain
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KaspaAttestation {
    /// Attestation format version
    pub version: u8,
    /// Difficulty Adjustment Algorithm score (block height equivalent)
    pub daa_score: u64,
    /// Blue score - count of blue blocks in selected chain (GHOSTDAG metric)
    pub blue_score: u64,
    /// Block hash (32 bytes)
    pub block_hash: [u8; 32],
    /// Block timestamp in Unix milliseconds
    pub timestamp: u64,
    /// Transaction hash containing the commitment (32 bytes)
    pub tx_hash: [u8; 32],
    /// Transaction index within the block
    pub tx_index: u32,
    /// Cumulative blue work at this block (32 bytes, big-endian)
    pub blue_work: [u8; 32],
    /// Parent block hashes (DAG context)
    pub parent_hashes: Vec<[u8; 32]>,
}

impl KaspaAttestation {
    /// Create a new Kaspa attestation
    pub fn new(
        daa_score: u64,
        blue_score: u64,
        block_hash: [u8; 32],
        timestamp: u64,
        tx_hash: [u8; 32],
        tx_index: u32,
        blue_work: [u8; 32],
        parent_hashes: Vec<[u8; 32]>,
    ) -> Self {
        Self {
            version: 0x01,
            daa_score,
            blue_score,
            block_hash,
            timestamp,
            tx_hash,
            tx_index,
            blue_work,
            parent_hashes,
        }
    }
}

/// Bitcoin attestation - for cross-chain anchoring (OTS compatible)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BitcoinAttestation {
    /// Bitcoin block height
    pub block_height: u32,
}

/// An attestation proving the timestamp is anchored to a blockchain
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Attestation {
    /// Proof is pending - needs calendar upgrade
    Pending(PendingAttestation),
    /// Proof is anchored to Kaspa blockchain
    Kaspa(KaspaAttestation),
    /// Proof is anchored to Bitcoin blockchain
    Bitcoin(BitcoinAttestation),
}

impl Attestation {
    /// Get the tag byte for this attestation
    pub fn tag(&self) -> u8 {
        match self {
            Attestation::Pending(_) => AttestationTag::Pending as u8,
            Attestation::Kaspa(_) => AttestationTag::KaspaBlock as u8,
            Attestation::Bitcoin(_) => AttestationTag::Bitcoin as u8,
        }
    }

    /// Check if this attestation is complete (not pending)
    pub fn is_complete(&self) -> bool {
        !matches!(self, Attestation::Pending(_))
    }
}

/// A complete KTCS proof
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KtcsProof {
    /// Proof format version
    pub version: u8,
    /// Hash algorithm used for the original digest
    pub hash_algorithm: HashAlgorithm,
    /// Flags (reserved for future use)
    pub flags: u8,
    /// The original data's hash (32 bytes for SHA256)
    pub digest: Vec<u8>,
    /// Sequence of operations from digest to attestation
    pub operations: Vec<Operation>,
    /// One or more attestations
    pub attestations: Vec<Attestation>,
}

impl KtcsProof {
    /// Create a new proof with a SHA256 digest
    pub fn new(digest: Vec<u8>) -> Self {
        Self {
            version: PROOF_VERSION,
            hash_algorithm: HashAlgorithm::Sha256,
            flags: 0x00,
            digest,
            operations: Vec::new(),
            attestations: Vec::new(),
        }
    }

    /// Add an operation to the proof
    pub fn add_operation(&mut self, op: Operation) {
        self.operations.push(op);
    }

    /// Add an attestation to the proof
    pub fn add_attestation(&mut self, attestation: Attestation) {
        self.attestations.push(attestation);
    }

    /// Check if the proof is complete (has at least one non-pending attestation)
    pub fn is_complete(&self) -> bool {
        self.attestations.iter().any(|a| a.is_complete())
    }

    /// Get all Kaspa attestations
    pub fn kaspa_attestations(&self) -> impl Iterator<Item = &KaspaAttestation> {
        self.attestations.iter().filter_map(|a| match a {
            Attestation::Kaspa(ka) => Some(ka),
            _ => None,
        })
    }
}

/// Batch mode for calendar aggregation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BatchMode {
    /// ~100ms batching window, ~10 batches/second
    Instant,
    /// ~1 second batching window, ~1 batch/second
    Standard,
    /// ~10 second batching window, ~0.1 batch/second
    Economic,
}

impl BatchMode {
    /// Get the batching window duration in milliseconds
    pub fn window_ms(&self) -> u64 {
        match self {
            BatchMode::Instant => 100,
            BatchMode::Standard => 1000,
            BatchMode::Economic => 10000,
        }
    }
}

impl Default for BatchMode {
    fn default() -> Self {
        BatchMode::Standard
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_magic_bytes() {
        assert_eq!(KTCS_MAGIC.len(), 18);
        // Check "KaspaTime" is in the magic bytes
        let kaspa_time = &KTCS_MAGIC[1..10];
        assert_eq!(kaspa_time, b"KaspaTime");
        // Check "Proof" is in the magic bytes
        let proof = &KTCS_MAGIC[12..17];
        assert_eq!(proof, b"Proof");
    }

    #[test]
    fn test_hash_algorithm_conversion() {
        assert_eq!(HashAlgorithm::try_from(0x08).unwrap(), HashAlgorithm::Sha256);
        assert_eq!(
            HashAlgorithm::try_from(0x14).unwrap(),
            HashAlgorithm::Ripemd160
        );
        assert!(HashAlgorithm::try_from(0x99).is_err());
    }

    #[test]
    fn test_operation_tags() {
        assert_eq!(Operation::Append(vec![]).tag(), 0xF0);
        assert_eq!(Operation::Prepend(vec![]).tag(), 0xF1);
        assert_eq!(Operation::Sha256.tag(), 0x08);
    }

    #[test]
    fn test_proof_creation() {
        let digest = vec![0u8; 32];
        let mut proof = KtcsProof::new(digest);
        assert!(!proof.is_complete());

        proof.add_attestation(Attestation::Pending(PendingAttestation {
            calendar_url: "https://calendar.ktcs.example.com".to_string(),
        }));
        assert!(!proof.is_complete());

        proof.add_attestation(Attestation::Kaspa(KaspaAttestation::new(
            42000000,
            41500000,
            [0u8; 32],
            1706000000000,
            [0u8; 32],
            0,
            [0u8; 32],
            vec![[0u8; 32]],
        )));
        assert!(proof.is_complete());
    }

    #[test]
    fn test_batch_mode_windows() {
        assert_eq!(BatchMode::Instant.window_ms(), 100);
        assert_eq!(BatchMode::Standard.window_ms(), 1000);
        assert_eq!(BatchMode::Economic.window_ms(), 10000);
    }
}
