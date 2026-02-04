# Kaspa Thermodynamic Clock Service (KTCS)

> Trustless timestamping powered by Kaspa's BlockDAG

KTCS is a proof-of-existence timestamping protocol that leverages Kaspa's high-throughput BlockDAG to provide sub-second timestamp confirmations with thermodynamic security.

## What is KTCS?

KTCS proves that data existed at a specific point in time by anchoring cryptographic commitments to the Kaspa blockchain. Unlike Bitcoin-based timestamping (which requires hours for confirmation), KTCS exploits Kaspa's ~100ms block times to deliver near-instant timestamps.

**Key insight:** Kaspa blocks are *thermodynamic clocks*. The proof-of-work proves that computational energy was expended, creating unforgeable evidence of time.

## Features

| Feature | Description |
|---------|-------------|
| **Sub-second timestamps** | ~100ms to first confirmation |
| **Trustless verification** | Anyone with a Kaspa node can verify |
| **Calendar-optional** | Direct stamping or batched aggregation |
| **Browser verification** | Client-side verification via WASM |
| **Thermodynamic security** | Security measured in cumulative PoW |
| **Privacy-preserving** | Commitments hide document content |

## Quick Start

### Option 1: Web Interface

Visit the hosted web app or run locally:

```bash
npm install
npm run dev
# Open http://localhost:5173
```

### Option 2: CLI

```bash
# Install
cargo install --path ktcs-cli

# Stamp a file
ktcs stamp document.pdf

# Verify a proof
ktcs verify document.kts

# Direct on-chain stamping
ktcs stamp --direct --wallet-file wallet.key document.pdf
```

### Option 3: Library Integration

```rust
use ktcs_core::{KtcsProof, verify_proof, deserialize_proof};
use ktcs_core::merkle::sha256;

// Hash document
let digest = sha256(b"Important document");

// Create and verify proofs
let proof = deserialize_proof(&kts_bytes)?;
let result = verify_proof(&proof, Some(document))?;
```

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                              CLIENTS                                     │
│         CLI          Web App           WASM           Libraries         │
└───────────────────────────────┬─────────────────────────────────────────┘
                                │
              ┌─────────────────┼─────────────────┐
              │                 │                 │
              ▼                 │                 ▼
    ┌──────────────────┐        │        ┌──────────────────┐
    │ CALENDAR SERVER  │        │        │  DIRECT STAMPING │
    │                  │        │        │                  │
    │ • Batch digests  │        │        │ • User wallet    │
    │ • Build Merkle   │        │        │ • No intermediary│
    │ • Submit to Kaspa│        │        │ • Higher cost    │
    └────────┬─────────┘        │        └────────┬─────────┘
             │                  │                 │
             └──────────────────┼─────────────────┘
                                │
                                ▼
          ┌─────────────────────────────────────────────────────────────┐
          │                     KASPA NETWORK                            │
          │                                                             │
          │   • 10 blocks per second          • P2PK commitments        │
          │   • GHOSTDAG consensus            • Thermodynamic security  │
          └─────────────────────────────────────────────────────────────┘
```

## Components

| Component | Description | Documentation |
|-----------|-------------|---------------|
| [ktcs-core](./ktcs-core/) | Core Rust library | [README](./ktcs-core/README.md) |
| [ktcs-cli](./ktcs-cli/) | Command-line interface | [README](./ktcs-cli/README.md) |
| [ktcs-calendar](./ktcs-calendar/) | Calendar aggregation server | [README](./ktcs-calendar/README.md) |
| [ktcs-wasm](./ktcs-wasm/) | Browser WASM bindings | [README](./ktcs-wasm/README.md) |
| [src/](./src/) | React web frontend | [README](./src/README.md) |

## Documentation

| Document | Description |
|----------|-------------|
| [Architecture](./docs/concepts/architecture.md) | System design and data flows |
| [Proof Format](./docs/concepts/proof-format.md) | Binary `.kts` file specification |
| [API Reference](./docs/api/reference.md) | Calendar server REST/WebSocket API |
| [Deployment](./docs/deployment/production.md) | Production deployment guide |
| [Security](./docs/concepts/security.md) | Security model and recommendations |

### Documentation Site

Run the full documentation site locally:

```bash
pip install -r docs-requirements.txt
npm run docs:serve
# Open http://localhost:8000
```

## How It Works

### P2PK Commitment (Not OP_RETURN)

**Important:** Kaspa does NOT support OP_RETURN. KTCS uses P2PK burn outputs:

```
Script: 0x20 <32-byte commitment> 0xac

Where:
  0x20 = Push 32 bytes
  <commitment> = SHA256 hash (Merkle root or direct commitment)
  0xac = OP_CHECKSIG
```

The commitment acts as a "public key" with no private key, making the output **provably unspendable**. Each timestamp burns **0.2 KAS**.

### Proof Format

KTCS proofs are binary `.kts` files containing:

1. **Header** - Magic bytes, version, algorithm
2. **Digest** - Original document's SHA256 hash
3. **Operations** - Merkle path transformations
4. **Attestations** - Kaspa block reference with DAG context

### Batch Modes

| Mode | Window | Use Case |
|------|--------|----------|
| `instant` | 100ms | Time-critical applications |
| `standard` | 1 second | General purpose (default) |
| `economic` | 10 seconds | Cost-optimized batching |

### Thermodynamic Security

Security is measured in cumulative proof-of-work (blue work):

| Time Since | Security Level |
|------------|----------------|
| 1 minute | Low-value records |
| 1 hour | Legal documents |
| 1 day | High-value IP |
| + Bitcoin anchor | Long-term archival |

## Development

### Prerequisites

- Rust 1.70+
- Node.js 18+
- Kaspa node (or public RPC access)

### Build

```bash
# Build all Rust components
cargo build --release

# Build frontend
npm install
npm run build

# Build WASM
cd ktcs-wasm && wasm-pack build --target web
```

### Test

```bash
cargo test
npm test
```

## Comparison with OpenTimestamps

| Aspect | OpenTimestamps | KTCS |
|--------|----------------|------|
| Block time | ~10 minutes | ~100ms |
| Confirmation | Hours | Seconds |
| On-chain storage | OP_RETURN | P2PK burn |
| Calendar requirement | Practical necessity | Optional |
| DAG support | No | Yes |
| Thermodynamic metrics | Block depth | Blue work |

## Security

KTCS provides **zero-trust verification**:

- Anyone with a Kaspa node can verify proofs
- Calendars cannot forge or backdate timestamps
- Direct stamping requires no trusted third party

See [Security Model](./docs/concepts/security.md) for details.

## License

MIT

## Contributing

Contributions are welcome! Please read the contributing guidelines before submitting PRs.

## Links

- [Kaspa Website](https://kaspa.org)
- [Kaspa Documentation](https://docs.kaspa.org)
- [KTCS Technical Specification](./kaspa-thermodynamic-clock-spec.md)
