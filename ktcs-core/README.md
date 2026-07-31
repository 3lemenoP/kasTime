# ktcs-core

Core library for the Kaspa Thermodynamic Clock Service (KTCS).

## Overview

`ktcs-core` provides the foundational functionality for KTCS:

- **Proof Format**: Binary `.kts` file serialization/deserialization
- **Merkle Trees**: Batch aggregation with proof generation
- **Verification**: Cryptographic verification of timestamp proofs
- **Operations**: Hash transformations (SHA256, RIPEMD160, Keccak256)
- **Wallet**: Kaspa wallet operations and signing
- **Transaction Building**: UTXO selection and commitment transactions

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
ktcs-core = "0.1"
```

### Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `keygen` | Yes | Private key generation (requires `getrandom`, `rand`) |
| `kaspa-client` | No | Full Kaspa node connectivity (adds `tokio`, WebSocket, HTTP) |
| `wasm` | No | WebAssembly compatibility (disables native crypto) |

```toml
# With Kaspa client for direct stamping
ktcs-core = { version = "0.1", features = ["kaspa-client"] }

# For WASM builds (browser verification)
ktcs-core = { version = "0.1", default-features = false, features = ["wasm"] }
```

## Quick Start

### Creating a Proof

```rust
use ktcs_core::{
    KtcsProof, serialize_proof, Operation, Attestation, KaspaAttestation
};
use ktcs_core::merkle::sha256;

// Hash your document
let document = b"Important document content";
let digest = sha256(document);

// Create a new proof
let mut proof = KtcsProof::new(digest.to_vec());

// Add Merkle path operations (from calendar aggregation)
proof.add_operation(Operation::Append(sibling_hash.to_vec()));
proof.add_operation(Operation::Sha256);

// Add blockchain attestation
proof.add_attestation(Attestation::Kaspa(KaspaAttestation::new(
    daa_score,
    blue_score,
    block_hash,
    timestamp_ms,
    tx_hash,
    tx_index,
    blue_work,
    parent_hashes,
)));

// Serialize to .kts binary format
let kts_bytes = serialize_proof(&proof);
std::fs::write("document.kts", &kts_bytes)?;
```

### Verifying a Proof

```rust
use ktcs_core::{deserialize_proof, verify_proof};

// Load proof from file
let kts_bytes = std::fs::read("document.kts")?;
let proof = deserialize_proof(&kts_bytes)?;

// Verify against original document (optional). `verify_proof` is OFFLINE:
// it proves the proof is cryptographically + structurally consistent and
// carries a complete Kaspa attestation. It does NOT prove chain inclusion.
let result = verify_proof(&proof, Some(original_document))?;

if result.valid {
    println!("digest:              {}", result.digest);
    println!("computed commitment: {}", result.computed_commitment);
    println!("structurally valid:  {}", result.structurally_valid);
    // `chain_verified` is only ever true via the on-chain path
    // (`verify_attestation_on_chain`); offline it stays false.
    println!("chain verified:      {}", result.chain_verified);
} else {
    println!("Verification failed: {:?}", result.error);
}
```

`VerificationResult` fields: `valid`, `digest`, `computed_commitment`,
`structurally_valid`, `chain_verified`, `attestations`, and `error`. Per-block
thermodynamic figures (blue work, etc.) come from
`ThermodynamicMetrics::from_attestation(&kaspa_attestation)`, not from
`verify_proof`.

To prove the attestation is actually anchored on-chain, use the online path,
which performs an **exact P2PK burn-script match** (not a substring scan):

```rust
use ktcs_core::verify::verify_attestation_on_chain;

// `client` is a connected KaspaClient; `commitment` is the 32-byte value
// computed by applying the proof's operations to its digest.
let chain = verify_attestation_on_chain(&client, attestation, &commitment).await?;
println!("chain_verified: {}", chain.chain_verified);
```

### Building Merkle Trees

```rust
use ktcs_core::merkle::{MerkleTree, sha256};

// Hash multiple documents
let hashes: Vec<[u8; 32]> = documents
    .iter()
    .map(|doc| sha256(doc))
    .collect();

// Build tree
let tree = MerkleTree::build(hashes)?;
let root = tree.root();  // Commit this to blockchain

// Generate proof for specific leaf
let proof = tree.get_proof(leaf_index)?;
assert!(proof.verify());

// Convert to KTCS operations
let operations = proof.to_operations();
```

### Wallet Operations

```rust
use ktcs_core::KaspaWallet;

// Create wallet from private key (hex-encoded)
let wallet = KaspaWallet::from_hex(&private_key_hex, "mainnet")?;
println!("Address: {}", wallet.address());

// Sign a sighash
let signature = wallet.sign(&sighash)?;
```

### Building Transactions

```rust
use ktcs_core::{TransactionBuilder, CommitmentTransaction, sign_transaction};

// Build a commitment transaction. `build()` returns a CommitmentTransaction
// whose inner `.transaction` is the unsigned `Transaction`.
let built: CommitmentTransaction = TransactionBuilder::new()
    .commitment(&merkle_root)         // 32-byte commitment
    .add_inputs(selected_utxos)?      // Selected UTXOs (add_inputs is fallible)
    .change_address("kaspa:...")      // Return change here
    .fee_per_gram(1)                  // Fee rate
    .build()?;

// Sign it. Argument order is (tx, wallet, utxos); returns a signed Transaction.
let signed = sign_transaction(&built.transaction, &wallet, &selected_utxos)?;
```

## Module Reference

### Core Types (`types`)

| Type | Description |
|------|-------------|
| `KtcsProof` | Complete timestamp proof |
| `Operation` | Hash transformation (Append, Prepend, Sha256, etc.) |
| `Attestation` | Blockchain anchoring (Pending, Kaspa, Bitcoin) |
| `KaspaAttestation` | Kaspa block attestation with DAG context |
| `BatchMode` | Calendar batching mode (Instant, Standard, Economic) |

### Proof Operations (`proof`)

| Function | Description |
|----------|-------------|
| `serialize_proof(&proof)` | Convert proof to `.kts` bytes |
| `deserialize_proof(&bytes)` | Parse `.kts` bytes to proof |

### Merkle Trees (`merkle`)

| Type/Function | Description |
|---------------|-------------|
| `MerkleTree::build(leaves)` | Build tree from hashes |
| `tree.root()` | Get Merkle root |
| `tree.get_proof(index)` | Generate proof for leaf |
| `MerkleProof::verify()` | Verify a Merkle proof |
| `MerkleProof::to_operations()` | Convert to KTCS operations |
| `sha256(data)` | Compute SHA256 hash |

### Hash Operations (`ops`)

| Function | Description |
|----------|-------------|
| `apply_operation(state, op)` | Apply single operation |
| `apply_operations(digest, ops)` | Apply operation sequence |
| `create_commitment(hash, nonce)` | Create privacy-preserving commitment |

### Verification (`verify`)

| Function | Description |
|----------|-------------|
| `verify_proof(&proof, data)` | Offline verification (structural + cryptographic); does not prove chain inclusion |
| `verify_attestation_on_chain(&client, att, &commitment)` | On-chain verification with exact P2PK burn-script match (feature-gated) |
| `ThermodynamicMetrics::from_attestation()` | Extract security metrics |

### Kaspa Types (`kaspa_types`)

| Type/Function | Description |
|---------------|-------------|
| `BlockInfo` | Block information structure |
| `Transaction` | Raw transaction |
| `Utxo` | Unspent transaction output |
| `build_commitment_output(commitment)` | Create P2PK burn output |
| `extract_commitment_from_output(output)` | Extract commitment from P2PK |
| `COMMITMENT_BURN_AMOUNT` | Burn amount (0.2 KAS) |

### Transaction Building (`tx_builder`)

| Type/Function | Description |
|---------------|-------------|
| `TransactionBuilder` | Build commitment transactions |
| `CommitmentTransaction` | Unsigned commitment TX |
| `select_utxos_for_tx(utxos, amount)` | UTXO selection |
| `address_to_script(address)` | Convert address to script |

### Wallet (`wallet`)

| Type/Function | Description |
|---------------|-------------|
| `KaspaWallet` | Kaspa wallet with signing |
| `KaspaWallet::from_hex(key, network)` | Create from hex key |
| `wallet.address()` | Get bech32m address |
| `wallet.sign(sighash)` | Sign with Schnorr |
| `compute_kaspa_sighash(...)` | Compute transaction sighash |

### Kaspa Client (`kaspa`, feature-gated)

| Type/Function | Description |
|---------------|-------------|
| `KaspaClient` | RPC client for Kaspa node |
| `KaspaClientConfig` | Client configuration |
| `client.get_utxos(address)` | Fetch UTXOs |
| `client.submit_transaction(tx)` | Submit transaction |
| `client.get_block(hash)` | Get block info |

## Constants

| Constant | Value | Description |
|----------|-------|-------------|
| `KTCS_MAGIC` | `0x00 "KaspaTime" ...` | File magic bytes |
| `PROOF_VERSION` | `0x01` | Current proof version |
| `COMMITMENT_BURN_AMOUNT` | `20,000,000` | Sompi burned per commitment (0.2 KAS) |
| `DUST_THRESHOLD` | `1,000,000` | Minimum output amount (sompi) |

## P2PK Commitment Format

Kaspa does NOT support OP_RETURN. KTCS uses P2PK burn outputs:

```
Script: 0x20 <32-byte commitment> 0xac

0x20 = Push 32 bytes
<commitment> = SHA256 hash (Merkle root or direct commitment)
0xac = OP_CHECKSIG
```

The commitment acts as a "public key" with no private key, making the output provably unspendable.

## Error Handling

All functions return `Result<T, KtcsError>`:

```rust
use ktcs_core::{deserialize_proof, KtcsError};

match deserialize_proof(&bytes) {
    Ok(proof) => { /* use proof */ }
    Err(KtcsError::InvalidMagicBytes) => { /* not a .kts file */ }
    Err(KtcsError::UnsupportedVersion(v)) => { /* unknown version */ }
    Err(KtcsError::UnexpectedEof) => { /* truncated file */ }
    Err(e) => { /* other error */ }
}
```

## Security Considerations

- The wallet secret key is stored in a `Zeroizing<[u8; 32]>` buffer, so the real
  key material is wiped on drop (ktcs-calendar uses `secrecy::Secret` instead).
- Constant-time signature verification via `secp256k1`.
- Secure random number generation via `getrandom` (when the `keygen` feature is
  enabled).
- Offline `verify_proof` does not prove chain inclusion; on-chain verification
  matches the commitment with an exact P2PK burn-script check. See the
  `structurally_valid` / `chain_verified` fields above.

## License

MIT

## See Also

- [Proof Format Specification](../docs/concepts/proof-format.md)
- [Architecture](../docs/concepts/architecture.md)
- [API Reference](../docs/api/reference.md)
