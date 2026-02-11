# Kaspa Thermodynamic Clock Service (KTCS)

> Trustless timestamping powered by Kaspa's BlockDAG

**Live demo: [kastime.xyz](https://kastime.xyz)**

KTCS is a proof-of-existence timestamping protocol built on Kaspa. It anchors cryptographic commitments to the blockchain and delivers **sub-second timestamp confirmations** by exploiting Kaspa's ~100ms block times.

**Kaspathon 2026 — Real-Time Data Track**

---

## Try It

**Web app** — visit [kastime.xyz](https://kastime.xyz) or run locally:

```bash
npm install
npm run dev
# Open http://localhost:5173
```

1. Drop any file onto the page — it's hashed locally, never uploaded
2. Click **Create Timestamp** — the digest is submitted to the calendar server
3. Within seconds, receive a `.kts` proof file anchored to a Kaspa block
4. Verify the proof anytime on the **Verify** tab

**CLI** — for scripting and automation:

```bash
cargo install --path ktcs-cli

ktcs stamp document.pdf          # Timestamp via calendar
ktcs verify document.pdf.kts     # Verify a proof
ktcs stamp --direct --wallet-file wallet.key document.pdf  # Direct on-chain
```

---

## Why Kaspa?

Kaspa's BlockDAG has properties that make it uniquely suited for real-time timestamping:

- **10 blocks per second** — sub-second confirmation instead of waiting minutes or hours
- **GHOSTDAG consensus** — blue work accumulates continuously, providing measurable security
- **DAG structure** — parent hashes in proofs capture concurrent block context, not just linear height
- **P2PK burn outputs** — 34-byte commitment scripts anchor data on-chain at 0.2 KAS per stamp

KTCS is the first timestamping protocol designed specifically for a BlockDAG, treating blocks as thermodynamic clocks — the proof-of-work is unforgeable evidence of time.

---

## Features

| Feature | Description |
|---------|-------------|
| **Sub-second timestamps** | ~100ms to first confirmation |
| **Trustless verification** | Anyone with a Kaspa node can verify |
| **Calendar-optional** | Direct stamping or batched aggregation |
| **Browser verification** | Client-side verification via WASM — no server needed |
| **Privacy-preserving** | Nonce-blinded commitments hide document content |
| **Batch aggregation** | Merkle trees aggregate multiple stamps per transaction |

---

## Architecture

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
    |  - Batch digests  |     |  Signs with own wallet |
    |  - Build Merkle   |     |  Submits to node       |
    |  - Submit to Kaspa|     |  Zero trust required   |
    +--------+----------+     +----------+-------------+
             |                           |
             +-------------+-------------+
                           |
                           v
    +--------------------------------------------------+
    |                  KASPA NETWORK                    |
    |                                                  |
    |   - 10 blocks per second (100ms target)          |
    |   - P2PK commitment outputs (34-byte scripts)    |
    |   - DAA score for height, Blue score for ordering|
    |   - Cumulative proof-of-work (blue work)         |
    +--------------------------------------------------+
```

**Two stamping modes:**

- **Calendar mode** — submit a digest to the calendar server, which batches multiple stamps into a Merkle tree and commits the root on-chain. Cost-efficient, sub-second confirmation.
- **Direct mode** — build and sign the commitment transaction yourself with your own wallet. No intermediary, maximum trustlessness.

---

## Components

| Component | Description |
|-----------|-------------|
| [ktcs-core](./ktcs-core/) | Core Rust library — proof format, Merkle trees, wallet, TX builder |
| [ktcs-cli](./ktcs-cli/) | Command-line tool — stamp, verify, wallet management |
| [ktcs-calendar](./ktcs-calendar/) | Axum server — batch aggregation, REST + WebSocket API |
| [ktcs-wasm](./ktcs-wasm/) | WASM bindings — browser-native hashing, verification, signing |
| [src/](./src/) | React frontend — file upload, real-time confirmation, proof viewer |

---

## How It Works

### P2PK Commitment

KTCS anchors commitments as provably unspendable P2PK burn outputs:

```
Script: 0x20 <32-byte commitment> 0xac

  0x20 = Push 32 bytes
  <commitment> = SHA256 hash (Merkle root or direct commitment)
  0xac = OP_CHECKSIG
```

The commitment acts as a "public key" with no private key. No valid signature can ever be produced, so the output is **provably unspendable**. Each timestamp burns **0.2 KAS**.

### Proof Format

KTCS proofs are compact binary `.kts` files containing:

1. **Header** (26 bytes) — magic bytes `\0KaspaTime\0\0Proof\0`, version, algorithm
2. **Digest** (32 bytes) — SHA256 of the original document
3. **Operations** (variable) — Merkle path from digest to on-chain commitment
4. **Attestations** (variable) — Kaspa block reference with DAA score, blue work, parent hashes

Typical proof size: ~200 bytes (direct) to ~600 bytes (batched).

### Batch Modes

| Mode | Window | Use Case |
|------|--------|----------|
| `instant` | 100ms | Time-critical applications |
| `standard` | 1 second | General purpose (default) |
| `economic` | 10 seconds | Cost-optimized bulk stamping |

---

## Development

### Prerequisites

- **Rust** 1.78+ (for Cargo.lock v4)
- **Node.js** 18+
- **Kaspa node** or public RPC access

### Build

```bash
# All Rust components
cargo build --release

# Frontend
npm install
npm run build

# WASM (requires wasm-pack)
cd ktcs-wasm && wasm-pack build --target web
```

### Test

```bash
cargo test
```

### Docker

```bash
docker compose up
```

Calendar server runs on port 3001, frontend on port 80. See [Deployment Guide](./docs/deployment/production.md) for production configuration.

---

## Documentation

Full documentation is available at [kastime.xyz/docs](https://kastime.xyz/docs) or in the [docs/](./docs/) directory:

- [Architecture](./docs/concepts/architecture.md) — system design, data flows, security boundaries
- [Proof Format](./docs/concepts/proof-format.md) — binary `.kts` file specification
- [API Reference](./docs/api/reference.md) — calendar server REST and WebSocket API
- [Security Model](./docs/concepts/security.md) — trust assumptions, attack scenarios, recommendations
- [Deployment](./docs/deployment/production.md) — production setup, systemd, nginx, Docker

---

## Security

KTCS provides **zero-trust verification**:

- Anyone with a Kaspa node can independently verify any proof
- Calendar servers cannot forge or backdate timestamps
- Direct stamping requires no trusted third party
- Private keys use `secrecy::Secret<T>` with zeroization on drop
- API authentication with constant-time key comparison

See [Security Model](./docs/concepts/security.md) for the full threat model.

---

## License

[MIT](./LICENSE)

---

## Links

- [Live Demo — kastime.xyz](https://kastime.xyz)
- [Kaspa](https://kaspa.org)
- [Technical Specification](./kaspa-thermodynamic-clock-spec.md)
