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

Calendar mode submits the digest (`POST /v1/stamp`) to the configured calendar
server. In the default synchronous mode it polls for confirmation and verifies
the returned proof before writing it; with `--async` it writes a pending proof
immediately, to be finished later with `ktcs complete`. Direct stamping
(`--direct`) needs no calendar.

!!! warning "Configure your own calendar"
    There is no public calendar service. The built-in per-network default host
    (`calendar.ktcs.kaspa.org` / `testnet-calendar.ktcs.kaspa.org`) is **not
    live**. Point `ktcs stamp` at a calendar you control via `--calendar`, the
    `KTCS_CALENDAR_URL` environment variable, or the config file — or use
    `--direct` mode.

```bash
# Via calendar (submits digest, waits for confirmation)
ktcs stamp document.pdf

# Return immediately with a pending proof
ktcs stamp --async document.pdf

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
| `--calendar` | `-c` | per-network built-in (not live — configure your own) | Calendar server URL |
| `--mode` | `-m` | `standard` | Batch mode: `instant`, `standard`, `economic` |
| `--output` | `-o` | `<input>.kts` | Output file path |
| `--direct` | | `false` | Stamp directly on-chain |
| `--async` | | `false` | Return a pending proof immediately (complete later with `ktcs complete`) |
| `--wallet-file` | | | Path to wallet private key file |
| `--wallet-stdin` | | | Read wallet key from stdin |
| `--rpc-url` | | derived from `--network` | Kaspa RPC URL (direct mode) |
| `--network` | | `testnet` | Network: `mainnet`, `testnet` |

The RPC port is derived from the network when `--rpc-url` is not set: mainnet
`ws://localhost:16110`, testnet `ws://localhost:16210`. Precedence for
calendar/RPC/network values: CLI flag > environment variable > config file >
built-in default.

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

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--data` | `-d` | | Original data file for full verification |
| `--chain` | | `false` | Verify the attestation on-chain (requires network) |
| `--network` | | `mainnet` | Network for chain verification: `mainnet`, `testnet` |
| `--rpc-url` | | derived from `--network` | Kaspa RPC URL for chain verification |

`ktcs verify` exits non-zero (1) when the proof is invalid, and when a `--chain`
check fails. Offline verification (no `--chain`) proves the proof is
cryptographically and structurally consistent with a complete Kaspa
attestation; it does not prove on-chain inclusion. Use `--chain` for that.

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

### `ktcs complete` - Complete Pending Proof

Complete a pending proof by fetching the confirmed attestation from the
calendar. The returned proof is verified before being written.

> `ktcs upgrade` is an alias for `ktcs complete`.

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

---

### `ktcs status` - Show Proof Status

```bash
ktcs status document.kts
ktcs status --json document.kts
```

| Flag | Description |
|------|-------------|
| `--json` | Output as JSON |

---

### `ktcs config` - Manage Configuration

Read and write the TOML config file (per-network sections wired into `stamp`,
`verify`, and `wallet`).

```bash
ktcs config init            # create with defaults
ktcs config init --force    # overwrite existing
ktcs config show            # print effective config
ktcs config show --network testnet
ktcs config set network testnet
ktcs config set mainnet.rpc_url ws://127.0.0.1:16110
ktcs config path            # print config file path
```

The file lives at the platform config dir, e.g.
`~/.config/ktcs/config.toml` on Linux (`ktcs config path` prints the exact
path).

---

### `ktcs completions` - Shell Completions

```bash
ktcs completions bash > /etc/bash_completion.d/ktcs
ktcs completions zsh
ktcs completions fish
```

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

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--verbose` | `-v` | `false` | Enable verbose/debug output |
| `--quiet` | `-q` | `false` | Suppress banners and progress; keep errors and essential results |
| `--color` | | `auto` | Color output: `auto`, `always`, `never` (also honors `NO_COLOR`) |
| `--help` | `-h` | | Show help |
| `--version` | `-V` | | Show version |

## Command Aliases

`s` → `stamp`, `v` → `verify`, `i` → `info`, `c` / `upgrade` → `complete`,
`w` → `wallet`, `gen` → `wallet generate`.

## Environment Variables

| Variable | Description |
|----------|-------------|
| `KTCS_CALENDAR_URL` | Default calendar URL (overrides config file, below CLI flags) |
| `KTCS_RPC_URL` | Default Kaspa RPC URL (overrides config file, below CLI flags) |
| `NO_COLOR` | Disable colored output (same as `--color never`) |
| `RUST_LOG` | Logging filter when `--verbose` is set (e.g., `ktcs_cli=debug`) |

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Runtime error (I/O, network, invalid or failed verification) |
| 2 | Usage error (bad arguments, from the argument parser) |

There are only three exit codes. `1` covers every runtime failure — including
an invalid proof or a failed `--chain` check — and `2` is emitted by the
argument parser for usage errors.

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

# 2. If stamped with --async, complete it once confirmed
ktcs complete important-document.pdf.kts
# Output: Proof completed successfully

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
