# KTCS Proof Format Specification

This document specifies the binary format for KTCS (Kaspa Thermodynamic Clock Service) timestamp proofs. Proof files use the `.kts` extension.

## Overview

A `.kts` proof file contains everything needed to verify that a piece of data existed at a specific point in time, anchored to the Kaspa blockchain. The proof consists of:

1. **Header** (26 bytes) - Magic bytes, version, and algorithm info
2. **Digest** (variable) - The original document's hash
3. **Operations** (variable) - Transformations from digest to on-chain commitment
4. **Attestations** (variable) - Blockchain anchoring proofs

## File Structure

```
+---------------------------+
| HEADER (26 bytes)         |
+---------------------------+
| DIGEST (32 bytes SHA256)  |
+---------------------------+
| OPERATIONS (variable)     |
+---------------------------+
| ATTESTATIONS (variable)   |
+---------------------------+
```

## Header Format (26 bytes)

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 18 | Magic | File format identifier |
| 18 | 1 | Version | Proof format version (`0x01`) |
| 19 | 1 | Hash Algorithm | Algorithm for original digest |
| 20 | 1 | Flags | Reserved for future use (`0x00`) |
| 21 | 5 | Reserved | Padding (`0x00 0x00 0x00 0x00 0x00`) |

### Magic Bytes (18 bytes)

```
0x00 "KaspaTime" 0x00 0x00 "Proof" 0x00
```

Hex representation:
```
00 4b 61 73 70 61 54 69 6d 65 00 00 50 72 6f 6f 66 00
```

### Hash Algorithm Identifiers

| Value | Algorithm | Digest Size |
|-------|-----------|-------------|
| `0x08` | SHA256 | 32 bytes |
| `0x14` | RIPEMD160 | 20 bytes |
| `0x67` | Keccak256 | 32 bytes |

## Digest

The digest immediately follows the header. Its size depends on the hash algorithm:
- SHA256: 32 bytes
- RIPEMD160: 20 bytes
- Keccak256: 32 bytes

This is the hash of the original document being timestamped.

## Operations

Operations describe transformations applied to the digest to arrive at the on-chain commitment. This typically represents a Merkle proof path when using calendar aggregation.

### Operation Format

Each operation starts with a 1-byte tag, optionally followed by data:

| Tag | Name | Format | Description |
|-----|------|--------|-------------|
| `0xF0` | Append | `0xF0 <varint length> <data>` | Append bytes: `H' = current \|\| data` |
| `0xF1` | Prepend | `0xF1 <varint length> <data>` | Prepend bytes: `H' = data \|\| current` |
| `0x08` | SHA256 | `0x08` | Apply SHA256: `H' = SHA256(current)` |
| `0x14` | RIPEMD160 | `0x14` | Apply RIPEMD160: `H' = RIPEMD160(current)` |
| `0x67` | Keccak256 | `0x67` | Apply Keccak256: `H' = Keccak256(current)` |
| `0xFF` | Fork | `0xFF <varint count>` | Branch into N parallel paths |

### Varint Encoding (Bitcoin-style)

Length values use compact variable-length integer encoding:

| First Byte | Interpretation |
|------------|----------------|
| `0x00` - `0xFC` | Direct value (1 byte total) |
| `0xFD` | Next 2 bytes as uint16 LE |
| `0xFE` | Next 4 bytes as uint32 LE |
| `0xFF` | Next 8 bytes as uint64 LE |

### Merkle Proof Operations

When a proof uses calendar aggregation, the operations section contains the Merkle path from the document digest to the Merkle root that was committed on-chain:

```
For each sibling in path (bottom to top):
  If sibling is on the LEFT:  Prepend(sibling_hash), SHA256
  If sibling is on the RIGHT: Append(sibling_hash), SHA256
```

## Attestations

Attestations prove the commitment is anchored to a blockchain. A proof may contain multiple attestations.

### Attestation Tags

| Tag | Name | Description |
|-----|------|-------------|
| `0x83` | Pending | Incomplete proof, requires calendar upgrade |
| `0x84` | Kaspa Block | Complete proof anchored to Kaspa |
| `0x05` | Bitcoin | Cross-chain anchor (for long-term archival) |

### Pending Attestation (`0x83`)

Indicates the proof is incomplete and needs to be upgraded via the calendar server.

```
+--------+-------------------+-------------+
| 0x83   | varint URL length | UTF-8 URL   |
| 1 byte | 1-9 bytes         | variable    |
+--------+-------------------+-------------+
```

### Kaspa Block Attestation (`0x84`)

Complete proof anchored to the Kaspa blockchain.

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 1 | Tag | `0x84` |
| 1 | 1 | Version | Attestation version (`0x01`) |
| 2 | 8 | DAA Score | uint64 LE - Difficulty adjustment score |
| 10 | 8 | Blue Score | uint64 LE - GHOSTDAG blue score |
| 18 | 32 | Block Hash | Block containing the commitment TX |
| 50 | 8 | Timestamp | uint64 LE - Unix milliseconds |
| 58 | 32 | TX Hash | Transaction hash |
| 90 | 4 | TX Index | uint32 LE - Index within block |
| 94 | 32 | Blue Work | 256-bit BE - Cumulative PoW |
| 126 | var | Parent Count | varint - Number of parent hashes |
| var | 32×N | Parent Hashes | N × 32 bytes - DAG parent blocks |

**Field Descriptions:**

- **DAA Score**: Difficulty Adjustment Algorithm score, analogous to block height
- **Blue Score**: Count of blue blocks in the GHOSTDAG selected chain
- **Blue Work**: Cumulative proof-of-work (thermodynamic weight) at this block
- **Parent Hashes**: Captures DAG structure for concurrent event analysis

### Bitcoin Attestation (`0x05`)

For cross-chain anchoring to Bitcoin (optional, for long-term archival).

```
+--------+--------------+
| 0x05   | Block Height |
| 1 byte | uint32 LE    |
+--------+--------------+
```

## On-Chain Commitment Format

KTCS uses P2PK burn outputs for on-chain commitments.

### P2PK Commitment Output

The 32-byte commitment is stored as a provably unspendable P2PK output:

```
Script: 0x20 <32-byte commitment> 0xac

Where:
  0x20 = Push 32 bytes
  <commitment> = 32-byte SHA256 hash (Merkle root or direct commitment)
  0xac = OP_CHECKSIG
```

**Total script size**: 34 bytes

### Why P2PK?

The commitment acts as a "public key" with no corresponding private key. Since no one can produce a valid Schnorr signature for an arbitrary 32-byte value, the output is **provably unspendable**.

Each commitment burns **0.2 KAS** (20,000,000 sompi).

### Extracting Commitments

To extract a commitment from a Kaspa transaction output:

```
If script.length == 34 AND script[0] == 0x20 AND script[33] == 0xac:
    commitment = script[1..33]
```

## Example Proof

### Annotated Hex Dump

```
# HEADER (26 bytes)
00 4b 61 73 70 61 54 69 6d 65 00 00 50 72 6f 6f 66 00  # Magic: "KaspaTime" ... "Proof"
01                                                      # Version: 1
08                                                      # Hash Algorithm: SHA256
00                                                      # Flags: 0
00 00 00 00 00                                          # Reserved

# DIGEST (32 bytes)
ab c1 23 44 55 66 77 88 99 aa bb cc dd ee ff 00        # First 16 bytes
11 22 33 44 55 66 77 88 99 aa bb cc dd ee ff 00        # Last 16 bytes

# OPERATIONS (Merkle path)
f0 20 [32 bytes]     # Append sibling hash (0xF0 = Append, 0x20 = 32 bytes)
08                   # SHA256
f1 20 [32 bytes]     # Prepend sibling hash (0xF1 = Prepend, 0x20 = 32 bytes)
08                   # SHA256

# KASPA ATTESTATION
84                   # Tag: Kaspa block attestation
01                   # Version: 1
[8 bytes]            # DAA score (uint64 LE)
[8 bytes]            # Blue score (uint64 LE)
[32 bytes]           # Block hash
[8 bytes]            # Timestamp in ms (uint64 LE)
[32 bytes]           # TX hash
[4 bytes]            # TX index (uint32 LE)
[32 bytes]           # Blue work (256-bit BE)
03                   # Parent count: 3
[32 bytes]           # Parent hash 1
[32 bytes]           # Parent hash 2
[32 bytes]           # Parent hash 3
```

### Size Estimates

| Proof Type | Typical Size |
|------------|--------------|
| Direct (no Merkle path) | ~200 bytes |
| Calendar (depth 10) | ~600 bytes |
| With Bitcoin anchor | ~800 bytes |

## Verification Algorithm

```
VERIFY(proof_bytes, original_data):

  1. Parse proof file
     - Validate magic bytes equal KTCS_MAGIC
     - Check version is supported (0x01)
     - Extract hash algorithm, digest, operations, attestations

  2. Verify original data (if provided)
     - Compute HASH(original_data) using proof's hash algorithm
     - Assert computed hash equals proof digest

  3. Compute on-chain commitment
     - state = digest
     - For each operation:
         - Append: state = state || data
         - Prepend: state = data || state
         - SHA256: state = SHA256(state)
         - etc.
     - commitment = state

  4. Verify attestation(s)
     - For Kaspa attestations:
         - Fetch TX from Kaspa node
         - Verify TX output contains P2PK script with commitment
         - Verify TX is in attested block
         - Verify block is in selected chain (blue block)
         - Verify DAA score and blue score match

  5. Compute security metrics
     - Get current chain tip
     - blue_work_accumulated = current_blue_work - attestation_blue_work
     - blocks_since = current_daa_score - attestation_daa_score

  RETURN {
    valid: true,
    timestamp: attestation.timestamp,
    daa_score: attestation.daa_score,
    thermodynamic_security: blue_work_accumulated
  }
```

## File Extension

KTCS proof files use the `.kts` extension.

**Naming convention**: `{original_filename}.kts` or `{hash_prefix}.kts`

## Reference Implementation

See `ktcs-core/src/proof.rs` in the repository for the reference serialization/deserialization implementation.
