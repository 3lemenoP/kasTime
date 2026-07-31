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

# Custom calendar server (REQUIRED — see note below)
ktcs stamp --calendar https://my-calendar.example.com document.pdf

# Return immediately with a pending proof (don't wait for confirmation)
ktcs stamp --async document.pdf
```

Calendar mode really submits the digest (`POST /v1/stamp`) to the configured
calendar server and, unless `--async` is given, polls the server for
confirmation and verifies the returned proof before writing it.

!!! warning "Configure your own calendar"
    There is no public calendar service. The built-in per-network default host
    (`calendar.ktcs.kaspa.org` / `testnet-calendar.ktcs.kaspa.org`) is **not
    live**. You must point `ktcs stamp` at a calendar you control via
    `--calendar`, the `KTCS_CALENDAR_URL` environment variable, or the config
    file — or use `--direct` mode, which needs no calendar.

**Options:**

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--calendar` | `-c` | per-network built-in (not live — configure your own) | Calendar server URL |
| `--mode` | `-m` | `standard` | Batch mode: `instant`, `standard`, `economic` |
| `--output` | `-o` | `<input>.kts` | Output file path |
| `--direct` | | `false` | Stamp directly on-chain |
| `--async` | | `false` | Return immediately with a pending proof; complete later with `ktcs complete` |
| `--wallet-file` | | | Path to wallet private key file |
| `--wallet-stdin` | | | Read wallet key from stdin |
| `--rpc-url` | | derived from `--network` | Kaspa RPC URL (direct mode) |
| `--network` | | `testnet` | Network: `mainnet`, `testnet` |

The RPC port is derived from the network when `--rpc-url` is not given:
mainnet uses `ws://localhost:16110`, testnet uses `ws://localhost:16210`.
Precedence for calendar/RPC/network values is: CLI flag > environment variable
> config file > built-in default.

### `ktcs verify` - Verify a proof

Verify a timestamp proof, optionally against the original data.

```bash
# Verify proof structure and attestations (offline)
ktcs verify document.kts

# Verify against original data
ktcs verify --data document.pdf document.kts
ktcs verify -d document.pdf document.kts

# Also verify the attestation against the live blockchain
ktcs verify --chain document.kts
ktcs verify --chain --network testnet document.kts
```

**Options:**

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--data` | `-d` | | Original data file for full verification |
| `--chain` | | `false` | Verify the attestation on-chain (requires network) |
| `--network` | | `mainnet` | Network for chain verification: `mainnet`, `testnet` |
| `--rpc-url` | | derived from `--network` | Kaspa RPC URL for chain verification |

`ktcs verify` **exits non-zero (1) when the proof is invalid**, and also when a
`--chain` verification fails, so `ktcs verify … && next-step` is safe in scripts.

Offline verification (without `--chain`) confirms the proof is
cryptographically and structurally consistent and carries a complete Kaspa
attestation; it does **not** prove the attestation is anchored on-chain. Use
`--chain` to check block/transaction/commitment/timestamp against a node.

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

### `ktcs complete` - Complete a pending proof

Complete a pending proof by fetching the confirmed attestation from the
calendar. The calendar-returned proof is verified before it is written.

> Alias: `ktcs upgrade` is a visible alias for `ktcs complete`.

```bash
# Complete in place
ktcs complete document.kts

# Save to new file
ktcs complete -o completed.kts document.kts
```

**Options:**

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--output` | `-o` | Overwrite input | Output file |

### `ktcs status` - Show proof status

Report whether a proof is pending or complete.

```bash
# Human-readable
ktcs status document.kts

# JSON output
ktcs status --json document.kts
```

**Options:**

| Flag | Description |
|------|-------------|
| `--json` | Output as JSON |

### `ktcs config` - Manage configuration

Read and write the TOML config file (see [Configuration](#configuration)).

```bash
ktcs config init          # Create the config file with defaults
ktcs config init --force  # Overwrite an existing config file
ktcs config show          # Print the effective configuration
ktcs config show --network testnet
ktcs config set network testnet
ktcs config set mainnet.rpc_url ws://127.0.0.1:16110
ktcs config path          # Print the config file path
```

### `ktcs completions` - Generate shell completions

```bash
ktcs completions bash > /etc/bash_completion.d/ktcs
ktcs completions zsh
ktcs completions fish
```

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

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--verbose` | `-v` | `false` | Enable verbose/debug output |
| `--quiet` | `-q` | `false` | Suppress banners and progress; keep errors and essential results |
| `--color` | | `auto` | Color output: `auto`, `always`, `never` (also honors `NO_COLOR`) |
| `--help` | `-h` | | Show help |
| `--version` | `-V` | | Show version |

## Command Aliases

| Command | Alias(es) |
|---------|-----------|
| `stamp` | `s` |
| `verify` | `v` |
| `info` | `i` |
| `complete` | `c`, `upgrade` |
| `wallet` | `w` |
| `wallet generate` | `gen` |

## Configuration

The CLI reads an optional TOML config file (managed with `ktcs config`). Its
location follows the platform config dir, e.g. `~/.config/ktcs/config.toml` on
Linux; `ktcs config path` prints the exact path. The file has per-network
sections wired into the `stamp`, `verify`, and `wallet` commands.

```toml
network = "testnet"          # default network

[mainnet]
rpc_url = "ws://127.0.0.1:16110"
calendar = "https://my-calendar.example.com"

[testnet]
rpc_url = "ws://127.0.0.1:16210"
calendar = "https://my-testnet-calendar.example.com"
```

Values are resolved with the precedence: **CLI flag > environment variable >
config file > built-in default**.

## Security

**Never pass private keys as command-line arguments.** They may be visible in process listings and shell history.

Instead use:
- `--wallet-file` to read from a file
- `--wallet-stdin` to read from stdin (input is not echoed to the terminal)

Key-file handling:
- `wallet generate -o <file>` writes the key with `0600` permissions and
  **refuses to overwrite** an existing file (so you cannot clobber a funded
  wallet by accident — choose a new path or remove the old file first).

**Secure wallet file:**
```bash
ktcs wallet generate -o wallet.key   # written 0600, never overwritten
```

## Examples

### Complete Workflow: Calendar Stamping

```bash
# 1. Create timestamp
ktcs stamp important-document.pdf
# Output: important-document.pdf.kts created (pending)

# 2. If stamped with --async, complete it once confirmed
ktcs complete important-document.pdf.kts
# Output: Proof completed successfully

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
| 1 | Runtime error (I/O, network, invalid or failed verification, etc.) |
| 2 | Usage error (bad arguments, from the argument parser) |

There are only three exit codes. `1` covers every runtime failure — including
an invalid proof or a failed `--chain` check — and `2` is emitted by the
argument parser for usage errors. A successful run always exits `0`.

## Environment Variables

| Variable | Description |
|----------|-------------|
| `KTCS_CALENDAR_URL` | Default calendar URL (overrides the config file, below CLI flags) |
| `KTCS_RPC_URL` | Default Kaspa RPC URL (overrides the config file, below CLI flags) |
| `NO_COLOR` | If set, disables colored output (same as `--color never`) |
| `RUST_LOG` | Logging filter when `--verbose` is set (e.g., `ktcs_cli=debug`) |

## See Also

- [Proof Format](../docs/concepts/proof-format.md)
- [API Reference](../docs/api/reference.md)
- [Security](../docs/concepts/security.md)
