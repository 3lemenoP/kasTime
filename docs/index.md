# Kaspa Thermodynamic Clock Service

<p class="hero-subtitle">Trustless timestamping powered by Kaspa's BlockDAG</p>

KTCS is a proof-of-existence timestamping protocol that leverages Kaspa's high-throughput BlockDAG to provide **sub-second timestamp confirmations** with thermodynamic security.

---

## What is KTCS?

KTCS proves that data existed at a specific point in time by anchoring cryptographic commitments to the Kaspa blockchain. Exploiting Kaspa's ~100ms block times, KTCS delivers near-instant timestamp confirmations.

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
          |   - Cumulative proof-of-work (blue work)    |
          +---------------------------------------------+
```

[Learn more about the architecture :material-arrow-right:](concepts/architecture.md){ .md-button }

## How It Works

### P2PK Commitment

KTCS uses P2PK burn outputs to anchor commitments on-chain.

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

