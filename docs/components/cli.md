# CLI Tool (ktcs-cli)

Command-line interface for creating and verifying KTCS timestamps.

## Installation

```bash
# From source
cargo install --path ktcs-cli

# Or build manually
cargo build --release -p ktcs-cli
```

## Commands

### `ktcs stamp` - Create Timestamp

Create a timestamp for a file via calendar or direct on-chain.

!!! note "Calendar Mode Status"
    Calendar mode currently creates a pending proof locally. For full calendar
    integration, use the web interface or API to submit digests, then use
    `ktcs upgrade` to complete pending proofs. Direct stamping (`--direct`)
    is fully implemented.

```bash
# Via calendar (creates pending proof)
ktcs stamp document.pdf

# Direct on-chain (requires wallet)
ktcs stamp --direct --wallet-file ./wallet.key document.pdf

# Specify batch mode
ktcs stamp --mode instant document.pdf

# Custom output path
ktcs stamp -o timestamp.kts document.pdf

# Custom calendar server
ktcs stamp --calendar https://calendar.example.com document.pdf
```

**Options:**

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--calendar` | `-c` | `https://calendar.ktcs.kaspa.org` | Calendar server URL |
| `--mode` | `-m` | `standard` | Batch mode: `instant`, `standard`, `economic` |
| `--output` | `-o` | `<input>.kts` | Output file path |
| `--direct` | | `false` | Stamp directly on-chain |
| `--wallet-file` | | | Path to wallet private key file |
| `--wallet-stdin` | | | Read wallet key from stdin |
| `--rpc-url` | | `ws://localhost:16110` | Kaspa RPC URL (direct mode) |
| `--network` | | `testnet` | Network: `mainnet`, `testnet` |

---

### `ktcs verify` - Verify Proof

Verify a timestamp proof, optionally against original data.

```bash
# Verify proof structure and attestations
ktcs verify document.kts

# Verify against original data
ktcs verify --data document.pdf document.kts
ktcs verify -d document.pdf document.kts

# Verify attestation exists on blockchain
ktcs verify --chain document.kts
```

**Options:**

| Flag | Short | Description |
|------|-------|-------------|
| `--data` | `-d` | Original data file for full verification |
| `--chain` | | Verify attestation exists on blockchain |
| `--rpc-url` | | Kaspa RPC URL for chain verification (default: mainnet resolver) |

**Output:**

```
Proof is VALID
  Digest: abc123def456...
  Timestamp: 2026-01-23 12:00:00 UTC
  DAA Score: 42,847,291
  Blue Score: 42,501,832
  Block: abc123...
  Thermodynamic Weight: 1.23e18 blue work
```

---

### `ktcs info` - Display Proof Information

Show detailed information about a proof file.

```bash
# Human-readable output
ktcs info document.kts

# JSON output
ktcs info --json document.kts
```

**Options:**

| Flag | Description |
|------|-------------|
| `--json` | Output as JSON |

**Output:**

```
Proof Information

Version: 1
Hash Algorithm: SHA256
Digest: abc123def456...
Status: Complete

Operations: 4
  1. Append (32 bytes)
  2. SHA256
  3. Prepend (32 bytes)
  4. SHA256

Attestations: 1
  1. Kaspa Block
     DAA Score: 42,847,291
     Blue Score: 42,501,832
     Block Hash: abc123...
     TX Hash: def456...
     Timestamp: 2026-01-23 12:00:00 UTC
```

---

### `ktcs upgrade` - Upgrade Pending Proof

Upgrade a pending proof by fetching the attestation from the calendar.

```bash
# Upgrade in place
ktcs upgrade document.kts

# Save to new file
ktcs upgrade -o completed.kts document.kts
```

**Options:**

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--output` | `-o` | Overwrite input | Output file |

---

### `ktcs hash` - Compute File Hash

Compute the SHA256 hash of a file.

```bash
ktcs hash document.pdf
# Output: abc123def456789...
```

---

### `ktcs wallet` - Wallet Management

#### `ktcs wallet generate` - Generate New Wallet

```bash
# Generate for mainnet (default)
ktcs wallet generate

# Generate for testnet
ktcs wallet generate -n testnet

# Save to file
ktcs wallet generate -o wallet.key

# JSON format with address
ktcs wallet generate --format json
```

**Options:**

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--format` | `-f` | `hex` | Output format: `hex`, `json` |
| `--network` | `-n` | `mainnet` | Network: `mainnet`, `testnet` |
| `--output` | `-o` | stdout | Output file |

#### `ktcs wallet address` - Show Address

```bash
# From file
ktcs wallet address --wallet-file ./wallet.key

# From stdin
echo "abc123..." | ktcs wallet address --wallet-stdin

# For specific network
ktcs wallet address --wallet-file ./wallet.key -n testnet
```

#### `ktcs wallet balance` - Check Balance

```bash
# By wallet file
ktcs wallet balance --wallet-file ./wallet.key

# By address
ktcs wallet balance --address kaspa:qr...

# Custom RPC
ktcs wallet balance --address kaspa:qr... --rpc ws://localhost:16110
```

**Output:**

```
Address: kaspa:qr...
Balance: 1.50000000 KAS (150,000,000 sompi)
UTXOs: 3
```

## Global Options

| Flag | Short | Description |
|------|-------|-------------|
| `--verbose` | `-v` | Enable verbose/debug output |
| `--help` | `-h` | Show help |
| `--version` | `-V` | Show version |

## Environment Variables

| Variable | Description |
|----------|-------------|
| `KTCS_CALENDAR_URL` | Default calendar URL |
| `KTCS_RPC_URL` | Default Kaspa RPC URL |
| `RUST_LOG` | Logging level (e.g., `ktcs=debug`) |

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Error (any failure) |

!!! note "Exit Code Details"
    The CLI currently uses a single non-zero exit code (1) for all error conditions.
    Future versions may implement more specific exit codes for scripting purposes.

## Security

!!! danger "Never pass private keys as command-line arguments"
    Keys may be visible in process listings and shell history.

Use secure methods:

```bash
# From file (recommended)
ktcs stamp --direct --wallet-file ./wallet.key document.pdf

# From stdin
cat wallet.key | ktcs stamp --direct --wallet-stdin document.pdf
```

Secure your wallet file:

```bash
ktcs wallet generate -o wallet.key
chmod 600 wallet.key
```

## Examples

### Complete Calendar Workflow

```bash
# 1. Create timestamp
ktcs stamp important-document.pdf
# Output: important-document.pdf.kts created (pending)

# 2. Wait for confirmation, then upgrade
ktcs upgrade important-document.pdf.kts
# Output: Proof upgraded successfully

# 3. Verify
ktcs verify -d important-document.pdf important-document.pdf.kts
# Output: Proof is VALID
```

### Complete Direct Stamping Workflow

```bash
# 1. Generate wallet
ktcs wallet generate -n testnet -o wallet.key

# 2. Check balance (after funding)
ktcs wallet balance --wallet-file wallet.key -n testnet

# 3. Create timestamp
ktcs stamp --direct --wallet-file wallet.key \
  --network testnet document.pdf

# 4. Verify
ktcs verify -d document.pdf document.kts
```

### Batch Processing

```bash
# Timestamp multiple files
for file in documents/*.pdf; do
  ktcs stamp "$file"
done

# Verify all proofs
for proof in documents/*.kts; do
  original="${proof%.kts}"
  ktcs verify -d "$original" "$proof"
done
```

## See Also

- [Quick Start Guide](../getting-started/quickstart.md)
- [Core Library](core.md)
- [Proof Format](../concepts/proof-format.md)
