# KTCS Deployment Guide

This guide covers deploying the KTCS Calendar Server and related components.

## Prerequisites

- **Rust**: 1.70 or later
- **Node.js**: 18 or later (for frontend)
- **Kaspa Node**: Local node or public RPC access
- **SQLite**: Embedded (no separate installation)

## Building from Source

### Build All Components

```bash
# Clone the repository
git clone https://github.com/your-org/kasTime.git
cd kasTime

# Build release binaries
cargo build --release

# Binaries will be in target/release/
# - ktcs-cli
# - ktcs-calendar
```

### Build Frontend

```bash
# Install dependencies
npm install

# Development server
npm run dev

# Production build
npm run build
# Output in dist/
```

### Build WASM

```bash
cd ktcs-wasm
wasm-pack build --target web
# Output in pkg/
```

## Calendar Server Configuration

### Environment Variables

Copy `.env.example` to `.env` and configure:

```bash
cp ktcs-calendar/.env.example ktcs-calendar/.env
```

#### Database Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | `sqlite:./data/ktcs-calendar.db?mode=rwc` | SQLite connection URL |

#### Kaspa Network Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `KASPA_NETWORK` | `mainnet` | Network: `mainnet`, `testnet-10`, `testnet-11` |
| `KASPA_RPC_URL` | `ws://localhost:16110` | Kaspa node RPC URL |

**RPC URL Examples:**
- Local mainnet node: `ws://127.0.0.1:16110`
- Local testnet-11 node: `ws://127.0.0.1:16210`
- Public resolver: `wss://resolver.kaspa.stream/wrpc/testnet-11`

#### Wallet Configuration (STAMP Wallet)

| Variable | Required | Description |
|----------|----------|-------------|
| `CALENDAR_WALLET_KEY` | Yes* | 64-character hex private key |
| `CALENDAR_WALLET_ADDRESS` | No | Derived from key if not set |

*Required when `KTCS_MOCK_MODE=false`

**Generate a wallet:**
```bash
./target/release/ktcs-cli wallet generate -n testnet
```

#### Dual-Wallet Recycling (Optional)

| Variable | Default | Description |
|----------|---------|-------------|
| `RETURN_WALLET_KEY` | - | 64-character hex private key for RETURN wallet |
| `RETURN_WALLET_ADDRESS` | - | Derived from key if not set |
| `RECYCLE_THRESHOLD_SOMPI` | `100000000` | Min balance before recycling (1 KAS) |
| `RECYCLE_POLL_INTERVAL_SECS` | `30` | Polling interval |

#### Server Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `BIND_ADDRESS` | `0.0.0.0:3001` | Server bind address |
| `KTCS_PUBLIC_URL` | `http://{BIND_ADDRESS}` | Public URL for proofs |
| `CORS_ORIGINS` | `*` | Allowed origins (comma-separated) |
| `KTCS_ENVIRONMENT` | `development` | `development`, `testing`, `production` |

#### Rate Limiting

| Variable | Default | Description |
|----------|---------|-------------|
| `RATE_LIMIT_PER_SECOND` | `100` | Requests per second per IP |
| `RATE_LIMIT_BURST` | `200` | Burst allowance |
| `MAX_BODY_SIZE` | `10485760` | Max request body (10 MB) |

#### Authentication

| Variable | Default | Description |
|----------|---------|-------------|
| `API_KEY` | - | API key (min 16 characters) |
| `REQUIRE_API_KEY` | `false` | Require API key for write ops |

**Production requirement:** `REQUIRE_API_KEY=true` is enforced when `KTCS_ENVIRONMENT=production`.

#### Transaction Settings

| Variable | Default | Description |
|----------|---------|-------------|
| `FEE_PER_GRAM` | `1` | Fee rate in sompi per gram |
| `CONFIRMATION_TIMEOUT_MS` | `60000` | TX confirmation timeout |
| `KTCS_INCLUDE_MAGIC` | `true` | Include KTCS magic in output |

#### Mode Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `KTCS_MOCK_MODE` | `false` | Mock mode (testing only) |

**Warning:** Mock mode is blocked in production environment.

## Running the Server

### Development

```bash
cd ktcs-calendar
cargo run
```

### Production

```bash
./target/release/ktcs-calendar
```

### With Environment File

```bash
# The server automatically loads .env from the working directory
cd ktcs-calendar
./target/release/ktcs-calendar
```

## Systemd Service

Create `/etc/systemd/system/ktcs-calendar.service`:

```ini
[Unit]
Description=KTCS Calendar Server
After=network.target
Wants=network-online.target

[Service]
Type=simple
User=ktcs
Group=ktcs
WorkingDirectory=/opt/ktcs
ExecStart=/opt/ktcs/ktcs-calendar
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal

# Security hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/opt/ktcs/data
PrivateTmp=true

# Environment
EnvironmentFile=/opt/ktcs/.env

[Install]
WantedBy=multi-user.target
```

**Commands:**
```bash
# Create user
sudo useradd -r -s /bin/false ktcs

# Set up directory
sudo mkdir -p /opt/ktcs/data
sudo cp target/release/ktcs-calendar /opt/ktcs/
sudo cp ktcs-calendar/.env.example /opt/ktcs/.env
sudo chown -R ktcs:ktcs /opt/ktcs
sudo chmod 600 /opt/ktcs/.env

# Enable and start
sudo systemctl daemon-reload
sudo systemctl enable ktcs-calendar
sudo systemctl start ktcs-calendar

# Check status
sudo systemctl status ktcs-calendar
sudo journalctl -u ktcs-calendar -f
```

## Reverse Proxy (nginx)

Create `/etc/nginx/sites-available/ktcs`:

```nginx
# Rate limiting zone
limit_req_zone $binary_remote_addr zone=ktcs_limit:10m rate=10r/s;

server {
    listen 443 ssl http2;
    server_name calendar.example.com;

    # TLS configuration
    ssl_certificate /etc/letsencrypt/live/calendar.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/calendar.example.com/privkey.pem;
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256;
    ssl_prefer_server_ciphers off;

    # Security headers
    add_header X-Frame-Options "SAMEORIGIN" always;
    add_header X-Content-Type-Options "nosniff" always;
    add_header X-XSS-Protection "1; mode=block" always;
    add_header Strict-Transport-Security "max-age=31536000; includeSubDomains" always;

    # API endpoints
    location /v1/ {
        limit_req zone=ktcs_limit burst=20 nodelay;

        proxy_pass http://127.0.0.1:3001;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # WebSocket support
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_read_timeout 86400;
    }

    # Health endpoint (no rate limit)
    location /health {
        proxy_pass http://127.0.0.1:3001;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
    }

    # Block direct access to other paths
    location / {
        return 404;
    }
}

# HTTP redirect
server {
    listen 80;
    server_name calendar.example.com;
    return 301 https://$server_name$request_uri;
}
```

**Enable site:**
```bash
sudo ln -s /etc/nginx/sites-available/ktcs /etc/nginx/sites-enabled/
sudo nginx -t
sudo systemctl reload nginx
```

## Frontend Deployment

### Static File Hosting

The frontend is a static SPA that can be hosted anywhere:

```bash
# Build
npm run build

# Deploy to web server
cp -r dist/* /var/www/ktcs-web/
```

### nginx Configuration for Frontend

```nginx
server {
    listen 443 ssl http2;
    server_name app.example.com;

    root /var/www/ktcs-web;
    index index.html;

    # SPA routing
    location / {
        try_files $uri $uri/ /index.html;
    }

    # Cache static assets
    location ~* \.(js|css|png|jpg|jpeg|gif|ico|svg|woff|woff2)$ {
        expires 1y;
        add_header Cache-Control "public, immutable";
    }

    # Don't cache index.html
    location = /index.html {
        expires -1;
        add_header Cache-Control "no-store, must-revalidate";
    }
}
```

### Environment Variables (Frontend)

Configure at build time via `.env`:

```bash
VITE_CALENDAR_URL=https://calendar.example.com
VITE_KASPA_RPC=wss://resolver.kaspa.stream/wrpc/mainnet
```

## Docker Deployment (Optional)

### Dockerfile

```dockerfile
FROM rust:1.75-slim as builder

WORKDIR /app
COPY . .
RUN cargo build --release -p ktcs-calendar

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/ktcs-calendar .

EXPOSE 3001
CMD ["./ktcs-calendar"]
```

### docker-compose.yml

```yaml
version: '3.8'
services:
  calendar:
    build: .
    ports:
      - "3001:3001"
    environment:
      - BIND_ADDRESS=0.0.0.0:3001
      - KASPA_RPC_URL=wss://resolver.kaspa.stream/wrpc/mainnet
      - KASPA_NETWORK=mainnet
      - REQUIRE_API_KEY=true
      - API_KEY=${API_KEY}
      - CALENDAR_WALLET_KEY=${CALENDAR_WALLET_KEY}
    volumes:
      - ktcs_data:/app/data
    restart: unless-stopped

volumes:
  ktcs_data:
```

## Production Checklist

### Security

- [ ] `REQUIRE_API_KEY=true` is set
- [ ] `API_KEY` is strong (32+ random characters)
- [ ] `KTCS_ENVIRONMENT=production` is set
- [ ] HTTPS is configured with valid certificate
- [ ] CORS is restricted to known origins
- [ ] Firewall blocks direct access to port 3001
- [ ] Wallet keys are stored securely (encrypted, restricted permissions)
- [ ] `.env` file has restricted permissions (chmod 600)

### Reliability

- [ ] Systemd service is configured with auto-restart
- [ ] Reverse proxy is in front of the application
- [ ] Log rotation is configured
- [ ] Monitoring and alerting is set up
- [ ] Backup procedures for database and wallet keys

### Wallet Management

- [ ] STAMP wallet has sufficient funds (track balance)
- [ ] RETURN wallet is configured for recycling
- [ ] Wallet key backup is stored securely
- [ ] Alert on low balance

### Monitoring

**Health check:**
```bash
curl https://calendar.example.com/health
```

**Monitor with Prometheus (example):**
```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'ktcs'
    static_configs:
      - targets: ['calendar.example.com']
    metrics_path: '/health'
```

## Backup and Recovery

### What to Back Up

1. **Database**: `data/ktcs-calendar.db`
2. **Wallet keys**: Store encrypted copies securely
3. **Configuration**: `.env` file

### Backup Script

```bash
#!/bin/bash
DATE=$(date +%Y%m%d_%H%M%S)
BACKUP_DIR=/backup/ktcs

# Backup database
sqlite3 /opt/ktcs/data/ktcs-calendar.db ".backup '$BACKUP_DIR/ktcs-calendar-$DATE.db'"

# Compress
gzip $BACKUP_DIR/ktcs-calendar-$DATE.db

# Retain last 30 days
find $BACKUP_DIR -name "*.db.gz" -mtime +30 -delete
```

### Recovery

```bash
# Stop server
sudo systemctl stop ktcs-calendar

# Restore database
gunzip -c /backup/ktcs/ktcs-calendar-YYYYMMDD_HHMMSS.db.gz > /opt/ktcs/data/ktcs-calendar.db

# Verify permissions
chown ktcs:ktcs /opt/ktcs/data/ktcs-calendar.db

# Start server
sudo systemctl start ktcs-calendar
```

## Troubleshooting

### Server Won't Start

**Check logs:**
```bash
journalctl -u ktcs-calendar -n 100
```

**Common issues:**
- Invalid environment variables (check validation errors)
- Database directory doesn't exist
- Wallet key is missing or invalid
- Port already in use

### Connection to Kaspa Node Failed

- Verify RPC URL is correct
- Check if node is running and synced
- Try public resolver: `wss://resolver.kaspa.stream/wrpc/mainnet`

### Transactions Not Confirming

- Check wallet balance (needs > 0.2 KAS per commitment)
- Verify network configuration matches wallet address prefix
- Check Kaspa network status

### WebSocket Connections Dropping

- Verify nginx WebSocket configuration
- Check timeout settings
- Monitor for rate limiting

## See Also

- [Architecture](../concepts/architecture.md)
- [API Reference](../api/reference.md)
- [Security Model](../concepts/security.md)
