# KTCS Proof Format Specification

## Complete Technical Documentation

This document provides byte-level detail of the KTCS (Kaspa Thermodynamic Clock Service) proof format, including the `.kts` file structure, Kaspa transaction encoding, and verification process.

---

## Table of Contents

1. [Overview](#1-overview)
2. [File Structure](#2-file-structure)
3. [Header Format (26 bytes)](#3-header-format)
4. [Digest Section](#4-digest-section)
5. [Operations](#5-operations)
6. [Attestations](#6-attestations)
7. [Real Proof Examples](#7-real-proof-examples)
8. [Kaspa Transaction Structure](#8-kaspa-transaction-structure)
9. [Verification Process](#9-verification-process)
10. [Merkle Tree Batching](#10-merkle-tree-batching)

---

## 1. Overview

KTCS proofs provide cryptographic proof-of-existence by anchoring document hashes to the Kaspa blockchain. The proof format:

- **File extension**: `.kts`
- **Binary format**: Compact, non-text encoding
- **Supports**: Pending (calendar) and Complete (blockchain) attestations
- **Hash algorithms**: SHA256 (default), RIPEMD160, Keccak256

### Proof Lifecycle

```
Document → SHA256 → Digest → Operations → Commitment → Kaspa TX → Attestation
                              (Merkle)      (hash)      (on-chain)   (proof)
```

---

## 2. File Structure

```
┌─────────────────────────────────────────────┐
│ HEADER (26 bytes, fixed)                    │
│   ├─ Magic (18 bytes)                       │
│   ├─ Version (1 byte)                       │
│   ├─ Hash Algorithm (1 byte)                │
│   ├─ Flags (1 byte)                         │
│   └─ Reserved (5 bytes)                     │
├─────────────────────────────────────────────┤
│ DIGEST (20-32 bytes, variable)              │
│   └─ SHA256: 32 bytes                       │
│   └─ RIPEMD160: 20 bytes                    │
│   └─ Keccak256: 32 bytes                    │
├─────────────────────────────────────────────┤
│ OPERATIONS (0+ items, variable)             │
│   ├─ Prepend: 0xF1 + varint len + data      │
│   ├─ Append:  0xF0 + varint len + data      │
│   ├─ SHA256:  0x08                          │
│   ├─ RIPEMD160: 0x14                        │
│   ├─ Keccak256: 0x67                        │
│   └─ Fork:    0xFF + varint count           │
├─────────────────────────────────────────────┤
│ ATTESTATIONS (1+ items, variable)           │
│   ├─ Pending: 0x83 + varint len + URL       │
│   ├─ Kaspa:   0x84 + attestation data       │
│                                              │
└─────────────────────────────────────────────┘
```

---

## 3. Header Format

### 3.1 Magic Bytes (18 bytes)

```
Offset  Hex                                     Meaning
------  --------------------------------------  -------
0x00    00                                      Null separator
0x01    4B 61 73 70 61 54 69 6D 65              "KaspaTime" (9 bytes)
0x0A    00 00                                   Double null
0x0C    50 72 6F 6F 66                          "Proof" (5 bytes)
0x11    00                                      Null terminator
```

**Constant in code:**
```rust
pub const KTCS_MAGIC: &[u8] = &[
    0x00, 0x4b, 0x61, 0x73, 0x70, 0x61, 0x54, 0x69, 0x6d, 0x65,  // \0KaspaTime
    0x00, 0x00, 0x50, 0x72, 0x6f, 0x6f, 0x66, 0x00,              // \0\0Proof\0
];
```

### 3.2 Version (1 byte)

| Value | Meaning |
|-------|---------|
| 0x01  | Version 1 (current) |

### 3.3 Hash Algorithm (1 byte)

| Value | Algorithm | Digest Size |
|-------|-----------|-------------|
| 0x08  | SHA256    | 32 bytes    |
| 0x14  | RIPEMD160 | 20 bytes    |
| 0x67  | Keccak256 | 32 bytes    |

### 3.4 Flags (1 byte)

Currently reserved (0x00). Future uses:
- Privacy mode indicators
- Compression flags
- Batch aggregation hints

### 3.5 Reserved (5 bytes)

Five null bytes for future expansion.

---

## 4. Digest Section

Immediately follows the header. Size determined by hash algorithm:

```
Offset 0x1A (after 26-byte header):
┌──────────────────────────────────────────────────────────────────┐
│ 32 bytes for SHA256/Keccak256, or 20 bytes for RIPEMD160         │
└──────────────────────────────────────────────────────────────────┘
```

This is the hash of the original document being timestamped.

---

## 5. Operations

Operations transform the digest into a commitment. Each operation has a tag byte.

### 5.1 Operation Tags

| Tag  | Name      | Format                      | Semantic                    |
|------|-----------|-----------------------------|-----------------------------|
| 0xF0 | Append    | `F0 <varint len> <data>`    | state = state \|\| data     |
| 0xF1 | Prepend   | `F1 <varint len> <data>`    | state = data \|\| state     |
| 0x08 | SHA256    | `08`                        | state = SHA256(state)       |
| 0x14 | RIPEMD160 | `14`                        | state = RIPEMD160(state)    |
| 0x67 | Keccak256 | `67`                        | state = Keccak256(state)    |
| 0xFF | Fork      | `FF <varint count>`         | (reserved for multi-path)   |

### 5.2 Varint Encoding

| Value Range       | Encoding                              |
|-------------------|---------------------------------------|
| 0-252             | 1 byte: `<value>`                     |
| 253-65535         | 3 bytes: `FD <u16 LE>`                |
| 65536-4294967295  | 5 bytes: `FE <u32 LE>`                |
| 4294967296+       | 9 bytes: `FF <u64 LE>`                |

### 5.3 Direct Stamp Operation Sequence

For direct stamps, a nonce is prepended then hashed:

```
Initial:    digest = SHA256(document)
Operation:  Prepend(16-byte nonce)
Operation:  SHA256
Result:     commitment = SHA256(nonce || digest)
```

Serialized:
```
F1 10 <16 bytes nonce> 08
│  │  │                │
│  │  │                └─ SHA256 operation
│  │  └─ 16 bytes of nonce data
│  └─ Varint: 16 (0x10)
└─ Prepend tag
```

---

## 6. Attestations

### 6.1 Pending Attestation (Tag: 0x83)

Used when proof awaits blockchain confirmation.

```
Structure:
┌────┬──────────────┬─────────────────────────────┐
│ 83 │ varint(len)  │ UTF-8 calendar URL          │
└────┴──────────────┴─────────────────────────────┘
```

Example:
```
83 3E 68 74 74 70 73 3A 2F 2F 63 61 6C 65 6E 64 61 72 ...
│  │  └─────────────────────────────────────────────────
│  │    URL: "https://calendar.ktcs.kaspa.org/v1/stamp/ktcs_..."
│  └─ Varint: 62 bytes
└─ Pending tag
```

### 6.2 Kaspa Block Attestation (Tag: 0x84)

Complete proof anchored to Kaspa blockchain.

```
Structure:
┌────┬────┬──────────┬──────────┬──────────┬──────────┬──────────┬────────┬──────────┬────────────────┐
│ 84 │ V  │ DAA (8)  │ Blue (8) │ Block(32)│ Time (8) │ TX (32)  │ Idx(4) │ Work(32) │ Parents        │
└────┴────┴──────────┴──────────┴──────────┴──────────┴──────────┴────────┴──────────┴────────────────┘
  1B   1B    8B LE      8B LE      32B        8B LE      32B        4B LE    32B BE    varint + N×32B
```

| Field         | Size    | Encoding     | Description                           |
|---------------|---------|--------------|---------------------------------------|
| Tag           | 1       | -            | 0x84                                  |
| Version       | 1       | -            | 0x01                                  |
| DAA Score     | 8       | Little-end   | Difficulty Adjustment Algorithm score |
| Blue Score    | 8       | Little-end   | GHOSTDAG blue block count             |
| Block Hash    | 32      | Raw bytes    | Block containing the TX               |
| Timestamp     | 8       | Little-end   | Unix milliseconds                     |
| TX Hash       | 32      | Raw bytes    | Transaction hash                      |
| TX Index      | 4       | Little-end   | Output index in TX                    |
| Blue Work     | 32      | Big-endian   | Cumulative proof-of-work              |
| Parent Count  | varint  | -            | Number of parent blocks               |
| Parent Hashes | N × 32  | Raw bytes    | Parent block hashes                   |

---

## 7. Real Proof Examples

### 7.1 Pending Proof (122 bytes)

```
00000000: 00 4b 61 73 70 61 54 69 6d 65 00 00 50 72 6f 6f  .KaspaTime..Proo
00000010: 66 00 01 08 00 00 00 00 00 00 a1 ff f0 ff ef b9  f...............
00000020: ea ce 72 30 c2 4e 50 73 1f 0a 91 c6 2f 9c ef df  ..r0.NPs..../...
00000030: e7 71 21 c2 f6 07 12 5d ff ae 83 3e 68 74 74 70  .q!....]...>http
00000040: 73 3a 2f 2f 63 61 6c 65 6e 64 61 72 2e 6b 74 63  s://calendar.ktc
00000050: 73 2e 6b 61 73 70 61 2e 6f 72 67 2f 76 31 2f 73  s.kaspa.org/v1/s
00000060: 74 61 6d 70 2f 6b 74 63 73 5f 61 31 66 66 66 30  tamp/ktcs_a1fff0
00000070: 66 66 65 66 62 39 65 61 63 65                    ffefb9eace
```

**Breakdown:**
```
Bytes 0x00-0x11:  Magic "\x00KaspaTime\x00\x00Proof\x00"
Byte  0x12:       Version 0x01
Byte  0x13:       Hash Algorithm 0x08 (SHA256)
Byte  0x14:       Flags 0x00
Bytes 0x15-0x19:  Reserved (5 nulls)
Bytes 0x1A-0x39:  Digest a1fff0ffefb9eace7230c24e50731f0a91c62f9cefdfe77121c2f607125dffae
Byte  0x3A:       Attestation tag 0x83 (Pending)
Byte  0x3B:       URL length 0x3E (62)
Bytes 0x3C-0x79:  URL "https://calendar.ktcs.kaspa.org/v1/stamp/ktcs_a1fff0ffefb9eace"
```

### 7.2 Complete Proof with Kaspa Attestation (204 bytes)

```
00000000: 00 4b 61 73 70 61 54 69 6d 65 00 00 50 72 6f 6f  .KaspaTime..Proo
00000010: 66 00 01 08 00 00 00 00 00 00 5b 7b bf 78 7c dc  f.........[{.x|.
00000020: 2d d1 6d 00 9e 4f b3 31 5b 9d bb c5 0c e9 a6 eb  -.m..O.1[.......
00000030: c4 9b 2d 2e 21 eb 35 cb 78 cb f1 10 a1 34 ba c6  ..-.!.5.x....4..
00000040: 66 11 6a e0 3c 85 5a 46 9e 6f a4 42 08 84 01 72  f.j.<.ZF.o.B...r
00000050: f4 d5 14 00 00 00 00 58 22 ba 14 00 00 00 00 d3  .......X".......
00000060: 5c 05 0c c0 b6 f5 26 17 37 af 95 bd 63 e1 74 f7  \.....&.7...c.t.
00000070: 6d 5e 40 89 03 4c 63 0b 6b cb 3c 33 ac 6c a8 5c  m^@..Lc.k.<3.l.\
00000080: c4 a8 32 9c 01 00 00 8c 83 20 06 75 aa 0e c5 f1  ..2...... .u....
00000090: 11 47 6d 37 5a 56 e0 d1 8f e7 32 cd f0 dd f2 29  .Gm7ZV....2....)
000000a0: b9 eb 13 5d 39 cc 3d 00 00 00 00 00 00 00 00 00  ...]9.=.........
000000b0: 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00  ................
000000c0: 00 00 00 00 29 c9 8d 19 63 8b 85 9b 02 ...       ....)...c....
```

**Breakdown:**
```
Bytes 0x00-0x19:  Header (26 bytes)
Bytes 0x1A-0x39:  Digest 5b7bbf787cdc2dd16d009e4fb3315b9dbbc50ce9a6ebc49b2d2e21eb35cb78cb

OPERATIONS:
Byte  0x3A:       0xF1 (Prepend)
Byte  0x3B:       0x10 (length: 16)
Bytes 0x3C-0x4B:  Nonce: a134bac666116ae03c855a469e6fa442
Byte  0x4C:       0x08 (SHA256)

KASPA ATTESTATION:
Byte  0x4D:       0x84 (Kaspa tag)
Byte  0x4E:       0x01 (version)
Bytes 0x4F-0x56:  DAA Score: 349,566,066 (0x14d5f472 LE)
Bytes 0x57-0x5E:  Blue Score: 347,742,808 (0x14ba2258 LE)
Bytes 0x5F-0x7E:  Block Hash: d35c050cc0b6f5261737af95bd63e174f76d5e4089034c630b6bcb3c33ac6ca8
Bytes 0x7F-0x86:  Timestamp: 1738837101660 ms
Bytes 0x87-0xA6:  TX Hash: 8c83200675aa0ec5f111476d375a56e0d18fe732cdf0ddf229b9eb135d39cc3d
Bytes 0xA7-0xAA:  TX Index: 0
Bytes 0xAB-0xCA:  Blue Work: 0x9b858b63198dc929 (partial, big-endian)
Byte  0xCB:       Parent count (varint)
Bytes 0xCC+:      Parent hashes (2 parents × 32 bytes)
```

**Verification result:**
```
Digest:     5b7bbf787cdc2dd16d009e4fb3315b9dbbc50ce9a6ebc49b2d2e21eb35cb78cb
Commitment: 51098bfd244d85dd5e2f597a80302b63a0e6f3a6edfcdf1f9adf16e26d342413
            └─ SHA256(nonce || digest)
```

---

## 8. Kaspa Transaction Structure

### 8.1 Commitment Transaction

A KTCS commitment transaction has this structure:

```
Transaction {
  version: 0,
  inputs: [
    {
      previous_outpoint: { tx_id: ..., index: ... },
      signature_script: <Schnorr signature>,
      sequence: 0xFFFFFFFFFFFFFFFF,
      sig_op_count: 1
    }
  ],
  outputs: [
    {                                    // Output 0: Commitment (burn)
      value: 20,000,000 sompi (0.2 KAS),
      script_public_key: {
        version: 0,
        script: <commitment script>
      }
    },
    {                                    // Output 1: Change
      value: <remaining balance - fee>,
      script_public_key: <change address>
    }
  ],
  lock_time: 0,
  subnetwork_id: "00000000...",
  gas: 0,
  payload: ""
}
```

### 8.2 Commitment Script

The commitment is embedded in the first output's script:

```
Script format:
┌────┬──────────────────────────────────────────┬────┐
│ 00 │ 00 20 <32-byte commitment>               │ ac │
└────┴──────────────────────────────────────────┴────┘
  │      │   │                                    │
  │      │   └─ 32-byte commitment hash           └─ OP_CHECKSIG (ignored)
  │      └─ Push 32 bytes
  └─ Version byte

Real example from TX output:
scriptPublicKey: "00002051098bfd244d85dd5e2f597a80302b63a0e6f3a6edfcdf1f9adf16e26d342413ac"
                  │   ││└──────────────────────────────────────────────────────────────┘│
                  │   │└─ Commitment: 51098bfd244d85dd5e2f597a80302b63a0e6f3a6...      │
                  │   └─ OP_PUSHDATA 32 (0x20)                                          │
                  └─ Version 0x0000                                                     └─ OP_CHECKSIG
```

### 8.3 Fee Calculation

```
transaction_mass = 10 + (num_inputs × 148) + (num_outputs × 34)
fee = transaction_mass × fee_per_gram

Example (1 input, 2 outputs):
  mass = 10 + 148 + 68 = 226
  fee = 226 × 10 = 2,260 sompi
```

### 8.4 Actual Transaction Example

From the direct stamp test:

```json
{
  "inputs": [{
    "previousOutpoint": {
      "transactionId": "0d61aacee9c16e6ce66d7324077be3ae8e73abf4d6476c9ea072ed1ee2fea55b",
      "index": 1
    },
    "signatureScript": "416caf0f8d21c9bf8fe5b384e34b0c756b1db85fea931108e51fc0bf29b9cb1e16a6936bd5b584c086f6afb10bd7875392b8418f74ffc13d59c8083d2a632e01ff01",
    "sequence": 18446744073709551615,
    "sigOpCount": 1
  }],
  "outputs": [
    {
      "value": 20000000,
      "scriptPublicKey": "00002051098bfd244d85dd5e2f597a80302b63a0e6f3a6edfcdf1f9adf16e26d342413ac"
    },
    {
      "value": 8819830680,
      "scriptPublicKey": "000020f45fe47bd8279a987b7b90b863f0db20ed54dbe8b77a9bbf93536b91e076def4ac"
    }
  ]
}
```

**Decoded:**
- Input: Spending from previous TX, 1 signature
- Output 0: 0.2 KAS burned with commitment `51098bfd...`
- Output 1: 88.198 KAS change returned to wallet

---

## 9. Verification Process

### 9.1 Computational Verification

```
1. Parse proof file
2. Extract digest (original document hash)
3. Apply all operations sequentially:

   state = digest
   for op in operations:
     match op:
       Prepend(data): state = data || state
       Append(data):  state = state || data
       SHA256:        state = SHA256(state)
       ...

   commitment = state

4. Check attestation validity
5. Return: valid if complete attestation exists
```

### 9.2 On-Chain Verification

```
1. Complete computational verification
2. Connect to Kaspa RPC
3. Fetch block by attestation.block_hash
4. Verify:
   - Block exists
   - Block.daa_score matches attestation
   - TX exists in block
   - Commitment appears in TX output script
5. Calculate thermodynamic security:
   blocks_since = current_daa - attestation.daa_score
```

### 9.3 Commitment Derivation Example

```
Document: "test for direct 1738837097"
SHA256:   5b7bbf787cdc2dd16d009e4fb3315b9dbbc50ce9a6ebc49b2d2e21eb35cb78cb

Nonce:    a134bac666116ae03c855a469e6fa442

Step 1: Prepend nonce
  state = a134bac666116ae03c855a469e6fa442 || 5b7bbf787cdc2dd16d009e4fb3315b9dbbc50ce9a6ebc49b2d2e21eb35cb78cb
        = a134bac666116ae03c855a469e6fa4425b7bbf787cdc2dd16d009e4fb3315b9dbbc50ce9a6ebc49b2d2e21eb35cb78cb (48 bytes)

Step 2: SHA256
  commitment = SHA256(state)
             = 51098bfd244d85dd5e2f597a80302b63a0e6f3a6edfcdf1f9adf16e26d342413
```

---

## 10. Merkle Tree Batching

### 10.1 Tree Structure

For calendar-based batching, multiple documents are combined:

```
                    Root (committed to blockchain)
                   /                              \
              H(A||B)                          H(C||D)
             /      \                         /      \
         Leaf A   Leaf B                 Leaf C   Leaf D
           │        │                       │        │
        SHA256   SHA256                  SHA256   SHA256
           │        │                       │        │
         Doc A   Doc B                   Doc C   Doc D
```

### 10.2 Merkle Proof Operations

For Leaf A's proof:

```
Operations to reach root from Leaf A:
1. Append(Leaf B hash)    # Sibling on right
2. SHA256                 # Compute parent H(A||B)
3. Append(H(C||D))        # Sibling subtree on right
4. SHA256                 # Compute root
```

Serialized:
```
F0 20 <32-byte Leaf B>    # Append
08                        # SHA256
F0 20 <32-byte H(C||D)>   # Append
08                        # SHA256
```

### 10.3 Proof Size Estimation

For a tree with N leaves:
- Operations: 2 × log2(N) (one append/prepend + one hash per level)
- Each Append: 1 + 1 + 32 = 34 bytes
- Each SHA256: 1 byte
- Total per level: 35 bytes
- Depth 10 (1024 docs): ~350 bytes of operations

---

## Summary

| Component | Size | Purpose |
|-----------|------|---------|
| Magic | 18 bytes | File identification |
| Header | 26 bytes | Version, algorithm, flags |
| Digest | 20-32 bytes | Document hash |
| Operations | Variable | Merkle proof path |
| Pending Attestation | ~70 bytes | Calendar URL |
| Kaspa Attestation | ~145+ bytes | Full blockchain anchor |

**Typical file sizes:**
- Pending proof: ~100-150 bytes
- Complete proof (direct): ~200-250 bytes
- Complete proof (batched, depth 5): ~400-500 bytes
