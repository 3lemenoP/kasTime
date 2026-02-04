# Core Library (ktcs-core)

The core Rust library providing all KTCS functionality.

## Features

- **Proof Creation** - Build timestamps with Merkle aggregation
- **Verification** - Validate proofs against data
- **Serialization** - Binary `.kts` format encoding/decoding
- **Merkle Trees** - Efficient batch aggregation
- **Kaspa Types** - P2PK commitment generation
- **Wallet** - Key management and signing

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
ktcs-core = { path = "./ktcs-core" }
```

### Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `wallet` | Yes | Wallet operations and signing |
| `kaspa-rpc` | No | Kaspa node RPC client |
| `wasm` | No | WASM compatibility |

```toml
[dependencies]
ktcs-core = { path = "./ktcs-core", default-features = false, features = ["wallet"] }
```

## Quick Start

### Hash a Document

```rust
use ktcs_core::merkle::sha256;

let data = b"Important document content";
let digest = sha256(data);
println!("SHA256: {}", hex::encode(digest));
```

### Create a Proof

```rust
use ktcs_core::{KtcsProof, Operation, PendingAttestation, Attestation};

// Start with document digest
let digest = sha256(b"document");

// Create pending proof
let proof = KtcsProof::new(digest)
    .with_pending_attestation("https://calendar.example.com/v1/stamp/ktcs_abc123");
```

### Verify a Proof

```rust
use ktcs_core::{verify_proof, deserialize_proof};

// Load and parse proof
let proof_bytes = std::fs::read("document.kts")?;
let proof = deserialize_proof(&proof_bytes)?;

// Verify against original data
let document = std::fs::read("document.pdf")?;
let result = verify_proof(&proof, Some(&document))?;

if result.valid {
    println!("Proof is valid!");
    println!("Timestamp: {:?}", result.attestations[0].timestamp);
}
```

### Build Merkle Tree

```rust
use ktcs_core::merkle::{MerkleTree, sha256};

// Batch multiple digests
let digests = vec![
    sha256(b"doc1"),
    sha256(b"doc2"),
    sha256(b"doc3"),
];

let tree = MerkleTree::from_leaves(&digests);
let root = tree.root();

// Get proof path for a specific leaf
let path = tree.proof(0);  // Path for first document
```

### Wallet Operations

```rust
use ktcs_core::wallet::{PrivateKey, derive_address};

// Generate new key
let key = PrivateKey::random();

// Get address
let address = derive_address(&key, "mainnet");
println!("Address: {}", address);

// From hex
let key = PrivateKey::from_hex("abc123...")?;
```

## Types Reference

### KtcsProof

Main proof structure:

```rust
pub struct KtcsProof {
    pub version: u8,
    pub hash_algorithm: HashAlgorithm,
    pub digest: [u8; 32],
    pub operations: Vec<Operation>,
    pub attestations: Vec<Attestation>,
}
```

### Operation

Merkle path transformations:

```rust
pub enum Operation {
    Append(Vec<u8>),      // Append data, then hash
    Prepend(Vec<u8>),     // Prepend data, then hash
    Sha256,               // Apply SHA256
    Ripemd160,            // Apply RIPEMD160
    Keccak256,            // Apply Keccak256
}
```

### Attestation

Blockchain anchors:

```rust
pub enum Attestation {
    Pending(PendingAttestation),
    Kaspa(KaspaAttestation),
    Bitcoin(BitcoinAttestation),
}

pub struct KaspaAttestation {
    pub daa_score: u64,
    pub blue_score: u64,
    pub block_hash: [u8; 32],
    pub timestamp: u64,
    pub tx_hash: [u8; 32],
    pub tx_index: u32,
    pub blue_work: Vec<u8>,
    pub parent_hashes: Vec<[u8; 32]>,
}
```

### BatchMode

Aggregation timing:

```rust
pub enum BatchMode {
    Instant,   // 100ms window
    Standard,  // 1 second window
    Economic,  // 10 second window
}
```

## Serialization

### Serialize Proof

```rust
use ktcs_core::serialize_proof;

let bytes = serialize_proof(&proof)?;
std::fs::write("document.kts", bytes)?;
```

### Deserialize Proof

```rust
use ktcs_core::deserialize_proof;

let bytes = std::fs::read("document.kts")?;
let proof = deserialize_proof(&bytes)?;
```

## Kaspa Commitment

Build P2PK commitment outputs:

```rust
use ktcs_core::kaspa::{build_commitment_output, COMMITMENT_BURN_AMOUNT};

let commitment = sha256(b"merkle_root");
let output = build_commitment_output(&commitment);

// Burns 0.2 KAS (20,000,000 sompi)
assert_eq!(output.value, COMMITMENT_BURN_AMOUNT);
```

## Error Handling

```rust
use ktcs_core::{KtcsError, Result};

fn process_proof(bytes: &[u8]) -> Result<()> {
    let proof = deserialize_proof(bytes)?;

    match verify_proof(&proof, None) {
        Ok(result) if result.valid => println!("Valid!"),
        Ok(result) => println!("Invalid: {:?}", result.error),
        Err(KtcsError::InvalidFormat(msg)) => println!("Format error: {}", msg),
        Err(e) => println!("Error: {}", e),
    }

    Ok(())
}
```

## See Also

- [Proof Format Specification](../concepts/proof-format.md)
- [Architecture Overview](../concepts/architecture.md)
- [WASM Bindings](wasm.md) - Browser-compatible API
