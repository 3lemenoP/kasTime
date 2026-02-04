# Kaspa Thermodynamic Clock Service

<p class="hero-subtitle">Trustless timestamping powered by Kaspa's BlockDAG</p>

KTCS is a proof-of-existence timestamping protocol that leverages Kaspa's high-throughput BlockDAG to provide **sub-second timestamp confirmations** with thermodynamic security.

---

## What is KTCS?

KTCS proves that data existed at a specific point in time by anchoring cryptographic commitments to the Kaspa blockchain. Unlike Bitcoin-based timestamping (which requires hours for confirmation), KTCS exploits Kaspa's ~100ms block times to deliver near-instant timestamps.

!!! info "Key Insight"
    Kaspa blocks are *thermodynamic clocks*. The proof-of-work proves that computational energy was expended, creating unforgeable evidence of time.

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

=== "Web Interface"

    Visit the hosted web app or run locally:

    ```bash
    npm install
    npm run dev
    # Open http://localhost:5173
    ```

=== "CLI"

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

=== "Library"

    ```rust
    use ktcs_core::{KtcsProof, verify_proof, deserialize_proof};
    use ktcs_core::merkle::sha256;

    // Hash document
    let digest = sha256(b"Important document");

    // Create and verify proofs
    let proof = deserialize_proof(&kts_bytes)?;
    let result = verify_proof(&proof, Some(document))?;
    ```

## Architecture Overview

```
                              CLIENTS
         CLI          Web App           WASM           Libraries
                                |
              +-----------------+-----------------+
              |                 |                 |
              v                 |                 v
    +------------------+        |        +------------------+
    | CALENDAR SERVER  |        |        |  DIRECT STAMPING |
    |                  |        |        |                  |
    | - Batch digests  |        |        | - User wallet    |
    | - Build Merkle   |        |        | - No intermediary|
    | - Submit to Kaspa|        |        | - Higher cost    |
    +--------+---------+        |        +--------+---------+
             |                  |                 |
             +------------------+-----------------+
                                |
                                v
          +---------------------------------------------+
          |               KASPA NETWORK                  |
          |                                             |
          |   - 10 blocks per second                    |
          |   - GHOSTDAG consensus                      |
          |   - P2PK commitments                        |
          |   - Thermodynamic security                  |
          +---------------------------------------------+
```

[Learn more about the architecture :material-arrow-right:](concepts/architecture.md){ .md-button }

## How It Works

### P2PK Commitment

!!! warning "Not OP_RETURN"
    Kaspa does NOT support OP_RETURN. KTCS uses P2PK burn outputs.

```
Script: 0x20 <32-byte commitment> 0xac

Where:
  0x20 = Push 32 bytes
  <commitment> = SHA256 hash (Merkle root or direct commitment)
  0xac = OP_CHECKSIG
```

The commitment acts as a "public key" with no private key, making the output **provably unspendable**. Each timestamp burns **0.2 KAS**.

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

## Comparison with OpenTimestamps

| Aspect | OpenTimestamps | KTCS |
|--------|----------------|------|
| Block time | ~10 minutes | ~100ms |
| Confirmation | Hours | Seconds |
| On-chain storage | OP_RETURN | P2PK burn |
| Calendar requirement | Practical necessity | Optional |
| DAG support | No | Yes |
| Thermodynamic metrics | Block depth | Blue work |

## Next Steps

<div class="grid cards" markdown>

-   :material-clock-fast:{ .lg .middle } __Getting Started__

    ---

    Install KTCS and create your first timestamp in under 5 minutes.

    [:octicons-arrow-right-24: Installation](getting-started/installation.md)

-   :material-file-document:{ .lg .middle } __Proof Format__

    ---

    Understand the binary `.kts` file format and how proofs work.

    [:octicons-arrow-right-24: Proof Format](concepts/proof-format.md)

-   :material-api:{ .lg .middle } __API Reference__

    ---

    Integrate with the calendar server REST and WebSocket APIs.

    [:octicons-arrow-right-24: API Reference](api/reference.md)

-   :material-security:{ .lg .middle } __Security Model__

    ---

    Learn about the zero-trust verification and threat model.

    [:octicons-arrow-right-24: Security](concepts/security.md)

</div>
