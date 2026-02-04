# ktcs-cli

Command-line interface for KTCS timestamps.

## Installation

### From Source

```bash
cargo install --path ktcs-cli
```

### Binary

Pre-built binaries are available in the releases.

## Commands

### `ktcs stamp` - Create a timestamp

Create a timestamp for a file, either via calendar aggregation or direct on-chain.

```bash
# Via calendar (default, lower cost)
ktcs stamp document.pdf

# Direct on-chain (requires wallet)
ktcs stamp --direct --wallet-file ./wallet.key document.pdf

# Specify batch mode
ktcs stamp --mode instant document.pdf

# Custom output path
ktcs stamp -o timestamp.kts document.pdf

# Custom calendar server
ktcs stamp --calendar https://my-calendar.example.com document.pdf
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

### `ktcs verify` - Verify a proof

Verify a timestamp proof, optionally against the original data.

```bash
# Verify proof structure and attestations
ktcs verify document.kts

# Verify against original data
ktcs verify --data document.pdf document.kts
ktcs verify -d document.pdf document.kts
```

**Options:**

| Flag | Short | Description |
|------|-------|-------------|
| `--data` | `-d` | Original data file for full verification |

**Output:**

```
✓ Proof is VALID
  Digest: abc123def456...
  Timestamp: 2026-01-23 12:00:00 UTC
  DAA Score: 42,847,291
  Blue Score: 42,501,832
  Block: abc123...
  Thermodynamic Weight: 1.23e18 blue work
```

### `ktcs info` - Display proof information

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
─────────────────
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

### `ktcs upgrade` - Upgrade pending proof

Upgrade a pending proof to a complete proof by fetching the attestation from the calendar.

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

### `ktcs hash` - Compute file hash

Compute the SHA256 hash of a file (useful for manual verification).

```bash
ktcs hash document.pdf
# Output: abc123def456789...
```

### `ktcs wallet` - Wallet management

Manage Kaspa wallets for direct stamping.

#### `ktcs wallet generate` - Generate new wallet

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

**Output (hex):**
```
abc123def456...  (64 hex characters)
```

**Output (json):**
```json
{
  "private_key": "abc123...",
  "address": "kaspa:qr..."
}
```

#### `ktcs wallet address` - Show address from key

```bash
# From file
ktcs wallet address --wallet-file ./wallet.key

# From stdin
echo "abc123..." | ktcs wallet address --wallet-stdin

# For specific network
ktcs wallet address --wallet-file ./wallet.key -n testnet
```

**Options:**

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--wallet-file` | | | Path to wallet key file |
| `--wallet-stdin` | | | Read key from stdin |
| `--network` | `-n` | `mainnet` | Network |

#### `ktcs wallet balance` - Check balance

```bash
# By wallet file
ktcs wallet balance --wallet-file ./wallet.key

# By address
ktcs wallet balance --address kaspa:qr...

# For specific network
ktcs wallet balance --address kaspa:qr... -n mainnet

# Custom RPC
ktcs wallet balance --address kaspa:qr... --rpc ws://localhost:16110
```

**Options:**

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--wallet-file` | | | Path to wallet key file |
| `--wallet-stdin` | | | Read key from stdin |
| `--address` | | | Wallet address directly |
| `--network` | `-n` | `mainnet` | Network |
| `--rpc` | | | Custom RPC URL |
| `--resolver` | | `true` | Use PNN resolver |

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

## Security

**Never pass private keys as command-line arguments.** They may be visible in process listings and shell history.

Instead use:
- `--wallet-file` to read from a file
- `--wallet-stdin` to read from stdin

**Secure wallet file:**
```bash
# Create wallet with restrictive permissions
ktcs wallet generate -o wallet.key
chmod 600 wallet.key
```

## Examples

### Complete Workflow: Calendar Stamping

```bash
# 1. Create timestamp
ktcs stamp important-document.pdf
# Output: important-document.pdf.kts created (pending)

# 2. Wait for confirmation, then upgrade
ktcs upgrade important-document.pdf.kts
# Output: Proof upgraded successfully

# 3. Verify
ktcs verify -d important-document.pdf important-document.pdf.kts
# Output: ✓ Proof is VALID
```

### Complete Workflow: Direct Stamping

```bash
# 1. Generate wallet
ktcs wallet generate -n testnet -o wallet.key

# 2. Fund the wallet (get testnet KAS from faucet)
ktcs wallet balance --wallet-file wallet.key -n testnet
# Output: Balance: 0 KAS

# 3. After funding, create timestamp
ktcs stamp --direct --wallet-file wallet.key \
  --network testnet --rpc-url ws://localhost:16210 \
  document.pdf

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

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Invalid arguments |
| 3 | File not found |
| 4 | Verification failed |
| 5 | Network error |

## Environment Variables

| Variable | Description |
|----------|-------------|
| `KTCS_CALENDAR_URL` | Default calendar URL |
| `KTCS_RPC_URL` | Default Kaspa RPC URL |
| `RUST_LOG` | Logging level (e.g., `ktcs=debug`) |

## See Also

- [Proof Format](../docs/PROOF-FORMAT.md)
- [API Reference](../docs/API.md)
- [Security](../docs/SECURITY.md)
