# ktcs-calendar

Calendar aggregation server for KTCS batch timestamping.

## Overview

The calendar server provides cost-efficient timestamping by:

1. Accepting digest submissions from clients via REST API
2. Batching digests by mode (instant/standard/economic)
3. Building Merkle trees from batched digests
4. Committing Merkle roots to Kaspa via P2PK burn outputs
5. Returning complete proofs with Merkle paths
6. Providing real-time confirmation updates via WebSocket

## Quick Start

### Development

```bash
# Copy environment template
cp .env.example .env

# Edit configuration (set wallet key, etc.)
nano .env

# Run server
cargo run
```

### Production

```bash
cargo build --release
./target/release/ktcs-calendar
```

## Configuration

The server is configured via environment variables. Copy `.env.example` to `.env` and customize.

### Database

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | `sqlite:./data/ktcs-calendar.db?mode=rwc` | SQLite connection URL |

### Server

| Variable | Default | Description |
|----------|---------|-------------|
| `BIND_ADDRESS` | `0.0.0.0:3001` | Listen address |
| `KTCS_PUBLIC_URL` | `http://{BIND_ADDRESS}` | Public URL for proof URLs |
| `CORS_ORIGINS` | `*` | Allowed origins (comma-separated) |
| `KTCS_ENVIRONMENT` | `development` | Environment: development, testing, production |

### Kaspa Network

| Variable | Default | Description |
|----------|---------|-------------|
| `KASPA_NETWORK` | `mainnet` | Network: mainnet, testnet-10, testnet-11 |
| `KASPA_RPC_URL` | `ws://localhost:16110` | Kaspa node RPC URL |

### Wallet (STAMP)

| Variable | Required | Description |
|----------|----------|-------------|
| `CALENDAR_WALLET_KEY` | Yes* | 64-char hex private key |
| `CALENDAR_WALLET_ADDRESS` | No | Derived from key if not set |

*Required when `KTCS_MOCK_MODE=false`

### Dual-Wallet Recycling

For sustainable operation, configure a RETURN wallet that receives change and automatically recycles funds back to the STAMP wallet.

| Variable | Default | Description |
|----------|---------|-------------|
| `RETURN_WALLET_KEY` | - | 64-char hex private key |
| `RETURN_WALLET_ADDRESS` | - | Derived from key if not set |
| `RECYCLE_THRESHOLD_SOMPI` | `100000000` | Min balance before recycling (1 KAS) |
| `RECYCLE_POLL_INTERVAL_SECS` | `30` | Polling interval |

### Rate Limiting

| Variable | Default | Description |
|----------|---------|-------------|
| `RATE_LIMIT_PER_SECOND` | `100` | Requests per second per IP |
| `RATE_LIMIT_BURST` | `200` | Burst allowance |
| `MAX_BODY_SIZE` | `10485760` | Max request body (10 MB) |

### Authentication

| Variable | Default | Description |
|----------|---------|-------------|
| `API_KEY` | - | API key (min 16 characters) |
| `REQUIRE_API_KEY` | `false` | Require API key for write ops |

**Note:** `REQUIRE_API_KEY=true` is enforced when `KTCS_ENVIRONMENT=production`.

### Transaction Settings

| Variable | Default | Description |
|----------|---------|-------------|
| `FEE_PER_GRAM` | `1` | Fee rate (sompi per gram) |
| `CONFIRMATION_TIMEOUT_MS` | `60000` | TX confirmation timeout |
| `KTCS_INCLUDE_MAGIC` | `true` | Include KTCS magic prefix |

### Testing

| Variable | Default | Description |
|----------|---------|-------------|
| `KTCS_MOCK_MODE` | `false` | Mock mode (no blockchain) |

**Warning:** Mock mode produces timestamps that are NOT anchored to the blockchain. Blocked in production environment.

## API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/v1/stamp` | POST | Submit digest for timestamping |
| `/v1/stamp/:id` | GET | Get stamp status and proof |
| `/v1/verify` | POST | Verify a proof file |
| `/v1/stream` | WS | Real-time confirmation events |
| `/health` | GET | Health check (no auth) |

See [API Reference](../docs/API.md) for complete documentation.

## Batch Modes

| Mode | Window | Description |
|------|--------|-------------|
| `instant` | 100ms | Time-critical applications |
| `standard` | 1s | General purpose (default) |
| `economic` | 10s | Maximum aggregation |

## Architecture

### Batch Processing Loop

```
┌─────────────────────────────────────────────────────────────┐
│                    Every 100ms                               │
│                                                             │
│  1. Check batch windows for each mode                       │
│  2. Get ready batches (window expired)                      │
│  3. For each ready batch:                                   │
│     a. Build Merkle tree from digests                       │
│     b. Submit commitment TX to Kaspa                        │
│     c. Wait for block confirmation                          │
│     d. Generate Merkle proofs for each stamp                │
│     e. Update database with complete proofs                 │
│     f. Broadcast confirmation via WebSocket                 │
└─────────────────────────────────────────────────────────────┘
```

### Dual-Wallet Flow

```
   User funding
        │
        ▼
┌───────────────┐
│  STAMP Wallet │ ─── Commitment TX ───┐
└───────────────┘                      │
        ▲                              │
        │                              ▼
   Recycle TX                   ┌─────────────────┐
        │                       │  Kaspa Network  │
        │                       └────────┬────────┘
        │                                │
        │                          Change output
        │                                │
        │                                ▼
        │                       ┌────────────────┐
        └────────────────────── │  RETURN Wallet │
                                └────────────────┘
```

## SQLite Schema

```sql
CREATE TABLE stamps (
    id TEXT PRIMARY KEY,              -- ktcs_<16-char-id>
    digest BLOB NOT NULL,             -- 32-byte SHA256
    status TEXT NOT NULL,             -- pending|batched|confirmed
    submitted_at INTEGER NOT NULL,    -- Unix timestamp
    confirmed_at INTEGER,             -- Unix timestamp
    proof BLOB,                       -- Serialized .kts proof
    batch_mode TEXT NOT NULL          -- instant|standard|economic
);
```

## Monitoring

### Health Check

```bash
curl http://localhost:3001/health
```

**Response:**
```json
{
  "status": "ok",
  "version": "0.1.0",
  "pending_stamps": 42
}
```

### Logging

Set log level via `RUST_LOG`:

```bash
RUST_LOG=info,ktcs_calendar=debug ./ktcs-calendar
```

## Security

### Production Requirements

- `REQUIRE_API_KEY=true` must be set
- `API_KEY` must be at least 16 characters
- `KTCS_MOCK_MODE=false` (enforced)
- HTTPS via reverse proxy

### Recommendations

- Use separate STAMP and RETURN wallets
- Store wallet keys securely (encrypted, restricted permissions)
- Configure rate limiting appropriate to your use case
- Restrict CORS to known origins
- Monitor wallet balances

## Deployment

See [Deployment Guide](../docs/DEPLOYMENT.md) for:

- Systemd service configuration
- nginx reverse proxy setup
- Docker deployment
- Production checklist

## Development

### Run Tests

```bash
cargo test
```

### Run with Verbose Logging

```bash
RUST_LOG=debug cargo run
```

### Mock Mode

For development without a Kaspa node:

```bash
KTCS_MOCK_MODE=true cargo run
```

**Warning:** Mock mode timestamps are NOT blockchain-anchored!

## See Also

- [API Reference](../docs/API.md)
- [Architecture](../docs/ARCHITECTURE.md)
- [Deployment Guide](../docs/DEPLOYMENT.md)
- [Security Model](../docs/SECURITY.md)
