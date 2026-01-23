//! Error types for KTCS operations

use thiserror::Error;

/// Main error type for KTCS operations
#[derive(Error, Debug)]
pub enum KtcsError {
    // Proof format errors
    #[error("Invalid magic bytes - not a valid .kts file")]
    InvalidMagicBytes,

    #[error("Unsupported proof version: {0}")]
    UnsupportedVersion(u8),

    #[error("Invalid hash algorithm: 0x{0:02x}")]
    InvalidHashAlgorithm(u8),

    #[error("Invalid operation tag: 0x{0:02x}")]
    InvalidOperationTag(u8),

    #[error("Invalid attestation tag: 0x{0:02x}")]
    InvalidAttestationTag(u8),

    #[error("Unexpected end of data while parsing")]
    UnexpectedEof,

    #[error("Invalid varint encoding")]
    InvalidVarint,

    #[error("Data too large: {0} bytes exceeds maximum")]
    DataTooLarge(usize),

    #[error("Invalid UTF-8 string: {0}")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),

    // Verification errors
    #[error("Digest mismatch: expected {expected}, got {actual}")]
    DigestMismatch { expected: String, actual: String },

    #[error("Proof verification failed: {0}")]
    VerificationFailed(String),

    #[error("No attestations in proof")]
    NoAttestations,

    #[error("Proof is pending - requires calendar upgrade")]
    ProofPending,

    // Merkle tree errors
    #[error("Empty leaf set - cannot build Merkle tree")]
    EmptyLeafSet,

    #[error("Invalid Merkle proof")]
    InvalidMerkleProof,

    #[error("Leaf index {index} out of bounds for tree with {size} leaves")]
    LeafIndexOutOfBounds { index: usize, size: usize },

    // Kaspa RPC errors
    #[error("Kaspa RPC error: {0}")]
    KaspaRpc(String),

    #[error("Block not found: {0}")]
    BlockNotFound(String),

    #[error("Transaction not found: {0}")]
    TransactionNotFound(String),

    #[error("Connection error: {0}")]
    ConnectionError(String),

    // I/O errors
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    // Generic errors
    #[error("{0}")]
    Other(String),
}

/// Result type alias for KTCS operations
pub type Result<T> = std::result::Result<T, KtcsError>;
