# KTCS Architecture

This document describes the system architecture of the Kaspa Thermodynamic Clock Service (KTCS).

## System Overview

```
                            CLIENTS
    +-----------+  +-----------+  +--------+  +------------------+
    |    CLI    |  |  Web App  |  |  WASM  |  | Direct Libraries |
    | (ktcs-cli)|  | (React/TS)|  |(Browser)|  |  (Rust/JS SDK)  |
    +-----+-----+  +-----+-----+  +----+---+  +--------+---------+
          |               |             |               |
          +-------+-------+-------------+---------------+
                  |                     |
                  v                     v
    +-------------------+     +------------------------+
    |  CALENDAR SERVER  |     |    DIRECT STAMPING     |
    |  (ktcs-calendar)  |     |                        |
    |                   |     |  Client builds TX      |
    |  - Batch hashes   |     |  Signs with own wallet |
    |  - Build Merkle   |     |  Submits to node       |
    |  - Submit TX      |     |                        |
    |  - Return proofs  |     |  Higher cost,          |
    +--------+----------+     |  zero trust            |
             |                +----------+-------------+
             |                           |
             +-------------+-------------+
                           |
                           v
    +--------------------------------------------------+
    |                  KASPA NETWORK                    |
    |                                                  |
    |   BLOCKDAG (GHOSTDAG)                            |
    |                                                  |
    |   - 10 blocks per second (100ms target)          |
    |   - P2PK commitment outputs (34-byte scripts)    |
    |   - DAA score for height, Blue score for ordering|
    |   - Cumulative proof-of-work (blue work)         |
    |   - Pruning-safe header chain                    |
    +--------------------------------------------------+
```

## Components

### ktcs-core (Core Library)

The foundational Rust library providing:

- **Proof Format**: Binary `.kts` file serialization/deserialization
- **Merkle Trees**: Batch aggregation with proof generation
- **Operations**: Hash transformations (SHA256, RIPEMD160, Keccak256)
- **Verification**: Cryptographic proof verification
- **Wallet**: Kaspa wallet operations and signing
- **Transaction Building**: UTXO selection and commitment TX construction
- **Kaspa Client**: RPC communication (feature-gated)

**Key modules:**
| Module | Purpose |
|--------|---------|
| `proof.rs` | Proof serialization/deserialization |
| `types.rs` | Core data structures |
| `merkle.rs` | Merkle tree implementation |
| `ops.rs` | Hash operations |
| `verify.rs` | Verification logic |
| `wallet.rs` | Wallet management |
| `tx_builder.rs` | Transaction construction |
| `kaspa.rs` | RPC client (with `kaspa-client` feature) |

### ktcs-calendar (Calendar Server)

Axum-based aggregation server that:

1. Accepts digest submissions via REST API
2. Batches digests by mode (instant/standard/economic)
3. Builds Merkle trees from batched digests
4. Commits Merkle roots to Kaspa via P2PK outputs
5. Returns complete proofs with Merkle paths
6. Provides real-time updates via WebSocket

**Services:**
| Service | Purpose |
|---------|---------|
| `batch_manager.rs` | Batch collection and timing |
| `kaspa_service.rs` | Kaspa node interaction |
| `database.rs` | SQLite persistence |
| `recycle_service.rs` | Dual-wallet KAS recycling |

### ktcs-wasm (WebAssembly Bindings)

Browser-compatible WASM module enabling:

- Client-side proof verification (no server required)
- Wallet address derivation
- Transaction building and signing
- Direct stamping from browser

### ktcs-cli (Command-Line Interface)

Full-featured CLI tool supporting:

- `stamp` - Create timestamps (calendar or direct)
- `verify` - Verify proofs
- `info` - Display proof information
- `upgrade` - Upgrade pending proofs
- `wallet` - Wallet management

### Web Frontend (src/)

React/TypeScript single-page application with:

- File upload with drag-and-drop
- Client-side SHA256 hashing (files never uploaded)
- Real-time confirmation tracking via WebSocket
- Proof visualization and download
- Direct stamping support (browser wallet)

## Data Flows

### Calendar Stamping Flow

```
  Client                    Calendar                  Kaspa
    |                          |                        |
    | 1. POST /v1/stamp        |                        |
    |    {digest, batch_mode}  |                        |
    |------------------------->|                        |
    |                          |                        |
    | 2. pending_proof         |                        |
    |<-------------------------|                        |
    |                          |                        |
    |                          | 3. Batch window expires|
    |                          |    Build Merkle tree   |
    |                          |                        |
    |                          | 4. Submit P2PK TX      |
    |                          |----------------------->|
    |                          |                        |
    |                          | 5. Block confirmation  |
    |                          |<-----------------------|
    |                          |                        |
    | 6. WS: confirmed         |                        |
    |    {proof, block_hash}   |                        |
    |<-------------------------|                        |
    |                          |                        |
    | 7. GET /v1/stamp/{id}    |                        |
    |------------------------->|                        |
    |                          |                        |
    | 8. Complete proof (.kts) |                        |
    |<-------------------------|                        |
    |                          |                        |
```

### Direct Stamping Flow

```
  Client                                               Kaspa
    |                                                    |
    | 1. Hash document locally                           |
    |    digest = SHA256(document)                       |
    |                                                    |
    | 2. Generate nonce (16 bytes)                       |
    |    commitment = SHA256(nonce || digest)             |
    |                                                    |
    | 3. Build P2PK transaction                          |
    |    Output 0: 0.2 KAS to P2PK(commitment)           |
    |    Output 1: Change to wallet address              |
    |                                                    |
    | 4. Sign transaction with wallet                    |
    |                                                    |
    | 5. Submit transaction                              |
    |--------------------------------------------------->|
    |                                                    |
    | 6. Wait for confirmation                           |
    |<---------------------------------------------------|
    |                                                    |
    | 7. Build proof locally                             |
    |    - digest                                        |
    |    - Prepend(nonce), SHA256 operations              |
    |    - KaspaAttestation with block info              |
    |                                                    |
    | 8. Save .kts proof file                            |
    |                                                    |
```

### Proof Verification Flow

```
  Verifier
    |
    | 1. Parse .kts proof file
    |    - Validate magic bytes
    |    - Extract digest, operations, attestations
    |
    | 2. (Optional) Verify against original data
    |    - Compute SHA256(original_data)
    |    - Compare with proof digest
    |
    | 3. Compute commitment
    |    state = digest
    |    for each operation:
    |      apply transformation to state
    |    commitment = state
    |
    | 4. Verify attestation
    |    - Fetch TX from Kaspa node
    |    - Check P2PK output contains commitment
    |    - Verify block is in selected chain
    |
    | 5. Calculate security
    |    - Get current blue work
    |    - accumulated = current - attestation.blue_work
    |
    | RESULT: Valid timestamp at DAA score N
    |         with X blue work accumulated
    |
```

## Commitment Storage

### P2PK Burn Output

KTCS uses P2PK burn outputs:

```
Script: 0x20 <32-byte commitment> 0xac

Components:
  0x20 = Push 32 bytes opcode
  <commitment> = Merkle root (calendar) or SHA256(nonce||digest) (direct)
  0xac = OP_CHECKSIG
```

The commitment acts as a "public key" with no private key. The output is provably unspendable, burning **0.2 KAS** per timestamp.

### Extracting Commitments

```rust
// From kaspa_types.rs
if script.len() == 34 && script[0] == 0x20 && script[33] == 0xac {
    commitment = script[1..33]
}
```

## Batch Modes

| Mode | Window | Cost | Use Case |
|------|--------|------|----------|
| `instant` | 100ms | Higher (fewer batches) | Time-critical applications |
| `standard` | 1 second | Medium | General purpose (default) |
| `economic` | 10 seconds | Lowest (most aggregation) | Cost-optimized bulk stamping |

## Dual-Wallet Recycling

The calendar server uses two wallets for sustainable operation:

```
  +------------------+           +------------------+
  |  STAMP Wallet    |           |  RETURN Wallet   |
  |                  |           |                  |
  |  Receives:       |           |  Receives:       |
  |  - User funding  |           |  - TX change     |
  |                  |           |                  |
  |  Sends:          |           |  Sends:          |
  |  - Commitment TX |---------->|  - Recycle to    |
  |  - Change -------|---------->|    STAMP wallet   |
  +------------------+           +------------------+
                                         |
                                   Recycle loop
                                 (balance > threshold)
                                         |
                                         v
                                +------------------+
                                |  STAMP Wallet    |
                                +------------------+
```

This prevents UTXO fragmentation and maintains operational efficiency.

## Persistence Layer

### SQLite Schema

```sql
CREATE TABLE stamps (
    id TEXT PRIMARY KEY,              -- ktcs_<16-char-id>
    digest BLOB NOT NULL,             -- 32-byte SHA256
    status TEXT NOT NULL,             -- pending|batched|confirmed
    submitted_at INTEGER NOT NULL,    -- Unix timestamp
    confirmed_at INTEGER,             -- Unix timestamp (when confirmed)
    proof BLOB,                       -- Serialized .kts proof
    batch_mode TEXT NOT NULL          -- instant|standard|economic
);

CREATE INDEX idx_stamps_status ON stamps(status);
CREATE INDEX idx_stamps_submitted_at ON stamps(submitted_at);
```

## Technology Stack

### Backend (Rust)
- **Runtime**: Tokio (async)
- **Web Framework**: Axum 0.7
- **Database**: SQLx with SQLite
- **Crypto**: secp256k1, sha2, ripemd, blake2b
- **Kaspa RPC**: kaspa-rpc (via wrpc)

### Frontend (TypeScript)
- **Framework**: React 18
- **Build**: Vite 5
- **Styling**: Tailwind CSS 4
- **State**: Zustand
- **Data Fetching**: TanStack Query

### WASM
- **Bindings**: wasm-bindgen
- **Target**: web (ES modules)
- **Crypto**: Pure Rust (no native dependencies)

## Security Boundaries

```
  +-----------------------------------------------------------+
  |                     TRUST BOUNDARY                        |
  |                                                           |
  |  +-----------------------------------------------------+ |
  |  |                 TRUSTLESS ZONE                       | |
  |  |                                                     | |
  |  |  - Proof verification (anyone with a Kaspa node)    | |
  |  |  - Direct stamping (user controls wallet)           | |
  |  |  - Blue work calculation (deterministic from chain) | |
  |  +-----------------------------------------------------+ |
  |                                                           |
  |  +-----------------------------------------------------+ |
  |  |                 CALENDAR ZONE                        | |
  |  |                                                     | |
  |  |  Calendar servers CAN:                              | |
  |  |  - Delay batching (DoS)                             | |
  |  |  - Refuse service                                   | |
  |  |                                                     | |
  |  |  Calendar servers CANNOT:                           | |
  |  |  - Forge timestamps (requires PoW)                  | |
  |  |  - Backdate proofs (DAG is immutable)               | |
  |  |  - Tamper with proofs (cryptographic binding)       | |
  |  +-----------------------------------------------------+ |
  |                                                           |
  +-----------------------------------------------------------+
```

## See Also

- [Proof Format Specification](proof-format.md)
- [API Reference](../api/reference.md)
- [Security Model](security.md)
- [Deployment Guide](../deployment/production.md)
