# KTCS Calendar API Reference

This document describes the REST and WebSocket APIs for the KTCS Calendar Server.

## Base URL

There is no public hosted calendar. Operators run their own; point clients at
whatever host they deploy. Examples below use a placeholder:

```
Production: https://your-calendar.example.com   (host you deploy)
Development: http://localhost:3001
```

> The host `your-calendar.example.com` used in older docs is **not** a live
> service.

## Authentication

The API key guards **only** the write endpoint `POST /v1/stamp`, and only when
the server is configured with `REQUIRE_API_KEY=true`. Include it on that
request:

```http
X-API-Key: your-api-key-here
```

`GET /v1/stamp/:id`, `POST /v1/verify`, the `GET /v1/stream` WebSocket, and
`GET /health` are **public** and never require authentication.

## Rate Limiting

The effective production limit (set by `.env.production.template` and mirrored
in nginx) is:
- **Requests per second**: 10 per IP
- **Burst**: 50 requests

When rate limited, you'll receive:
```http
HTTP/1.1 429 Too Many Requests
```

---

## REST API

### POST /v1/stamp

Submit a digest for timestamping.

**Request:**

```http
POST /v1/stamp HTTP/1.1
Content-Type: application/json
X-API-Key: your-key-here

{
  "digest": "abc123...",
  "algorithm": "sha256",
  "batch_mode": "standard"
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `digest` | string | Yes | 64-character hex-encoded SHA256 hash |
| `algorithm` | string | No | Hash algorithm (default: `"sha256"`) |
| `batch_mode` | string | No | `"instant"` \| `"standard"` \| `"economic"` (default: `"standard"`) |

**Response (pending):**

```json
{
  "id": "ktcs_a1b2c3d4e5f6g7h8",
  "status": "pending",
  "submitted_at": "2026-01-23T12:00:00.123Z",
  "estimated_confirmation": "2026-01-23T12:00:01.123Z",
  "pending_proof": "base64-encoded-pending-kts-proof"
}
```

**Response fields:**

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Unique stamp ID (format: `ktcs_<16-char>`) |
| `status` | string | `"pending"` \| `"batched"` \| `"confirmed"` |
| `submitted_at` | string | ISO 8601 timestamp |
| `estimated_confirmation` | string | Estimated confirmation time |
| `pending_proof` | string | Base64-encoded pending .kts proof |

**Errors:**

| Status | Cause |
|--------|-------|
| 400 | Invalid digest format (not 64 hex chars) |
| 400 | Invalid algorithm (only `sha256` supported) |
| 400 | Invalid batch_mode |
| 401 | Missing or invalid API key |
| 429 | Rate limited |

---

### GET /v1/stamp/:id

Get the status of a stamp and retrieve the proof.

**Request:**

```http
GET /v1/stamp/ktcs_a1b2c3d4e5f6g7h8 HTTP/1.1
```

**Response (confirmed):**

```json
{
  "id": "ktcs_a1b2c3d4e5f6g7h8",
  "status": "confirmed",
  "submitted_at": "2026-01-23T12:00:00.123Z",
  "confirmed_at": "2026-01-23T12:00:00.987Z",
  "daa_score": 42847291,
  "blue_score": 42501832,
  "block_hash": "abc123def456...",
  "tx_hash": "789abc012def...",
  "proof": "base64-encoded-complete-kts-proof",
  "thermodynamic_weight": {
    "blue_work_at_confirmation": "1.23e18",
    "current_blue_work": "1.25e18",
    "accumulated_since": "2.00e16"
  },
  "parent_hashes": [
    "parent1hash...",
    "parent2hash...",
    "parent3hash..."
  ]
}
```

**Response fields (confirmed):**

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Stamp ID |
| `status` | string | `"confirmed"` |
| `submitted_at` | string | ISO 8601 submission time |
| `confirmed_at` | string | ISO 8601 confirmation time |
| `daa_score` | integer | Difficulty Adjustment Algorithm score |
| `blue_score` | integer | GHOSTDAG blue score |
| `block_hash` | string | Hex-encoded block hash |
| `tx_hash` | string | Hex-encoded transaction hash |
| `proof` | string | Base64-encoded complete .kts proof |
| `thermodynamic_weight` | object | Security metrics |
| `parent_hashes` | array | Hex-encoded parent block hashes |

**Thermodynamic weight fields:**

| Field | Type | Description |
|-------|------|-------------|
| `blue_work_at_confirmation` | string | Blue work when confirmed (scientific notation) |
| `current_blue_work` | string | Current blue work (if connected to node) |
| `accumulated_since` | string | Blue work accumulated since confirmation |

**Errors:**

| Status | Cause |
|--------|-------|
| 404 | Stamp ID not found |

---

### POST /v1/verify

Verify a proof file.

**Request:**

```http
POST /v1/verify HTTP/1.1
Content-Type: application/octet-stream

[binary .kts proof file bytes]
```

**Response:**

```json
{
  "valid": true,
  "digest": "abc123def456...",
  "attestations": [
    {
      "type": "kaspa_block",
      "daa_score": 42847291,
      "blue_score": 42501832,
      "block_hash": "abc123...",
      "timestamp": "2026-01-23T12:00:00.987Z",
      "thermodynamic_weight": "1.23e18"
    }
  ],
  "current_confirmations": {
    "blocks_since": 50000,
    "blue_work_accumulated": "5.00e17",
    "time_elapsed_seconds": 5000
  }
}
```

**Response fields:**

| Field | Type | Description |
|-------|------|-------------|
| `valid` | boolean | Whether the proof is valid |
| `digest` | string | Hex-encoded document digest |
| `attestations` | array | List of attestations in the proof |
| `current_confirmations` | object | Current security metrics (if connected to node) |
| `error` | string | Error message (if invalid) |

**Attestation types:**

| Type | Description |
|------|-------------|
| `"kaspa_block"` | Kaspa blockchain attestation |
| `"pending"` | Incomplete, needs upgrade |

**Errors:**

| Status | Cause |
|--------|-------|
| 400 | Invalid proof format |
| 400 | Proof verification failed |
| 413 | Request body too large |

---

### GET /health

Health check endpoint (no authentication required).

**Request:**

```http
GET /health HTTP/1.1
```

**Response:**

```json
{
  "status": "ok",
  "version": "0.1.0",
  "pending_stamps": 42
}
```

---

## WebSocket API

Real-time confirmation notifications.

### Endpoint

```
wss://your-calendar.example.com/v1/stream
ws://localhost:3001/v1/stream (development)
```

### Connection

```javascript
const ws = new WebSocket('wss://your-calendar.example.com/v1/stream');

ws.onopen = () => {
  console.log('Connected');
};

ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);
  console.log('Received:', msg);
};

ws.onerror = (error) => {
  console.error('WebSocket error:', error);
};

ws.onclose = () => {
  console.log('Disconnected');
};
```

### Client Messages

#### Subscribe

Subscribe to confirmation events for a proof.

```json
{
  "type": "subscribe",
  "proof_id": "ktcs_a1b2c3d4e5f6g7h8"
}
```

#### Unsubscribe

Unsubscribe from a proof's events.

```json
{
  "type": "unsubscribe",
  "proof_id": "ktcs_a1b2c3d4e5f6g7h8"
}
```

#### Ping

Keep the connection alive.

```json
{
  "type": "ping"
}
```

### Server Messages

#### Subscribed

Acknowledgement of subscription.

```json
{
  "type": "subscribed",
  "proof_id": "ktcs_a1b2c3d4e5f6g7h8"
}
```

#### Unsubscribed

Acknowledgement of unsubscription.

```json
{
  "type": "unsubscribed",
  "proof_id": "ktcs_a1b2c3d4e5f6g7h8"
}
```

#### Batched

Proof has been added to a batch (processing).

```json
{
  "type": "batched",
  "proof_id": "ktcs_a1b2c3d4e5f6g7h8"
}
```

#### Confirmed

Proof has been confirmed on-chain.

```json
{
  "type": "confirmed",
  "proof_id": "ktcs_a1b2c3d4e5f6g7h8",
  "block_hash": "abc123def456...",
  "daa_score": 42847291,
  "blue_score": 42501832,
  "timestamp": 1706012400987,
  "proof": "base64-encoded-kts-proof"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `proof_id` | string | Stamp ID |
| `block_hash` | string | Hex-encoded block hash |
| `daa_score` | integer | DAA score at confirmation |
| `blue_score` | integer | Blue score at confirmation |
| `timestamp` | integer | Unix timestamp in milliseconds |
| `proof` | string | Base64-encoded complete .kts proof |

#### Pong

Response to ping.

```json
{
  "type": "pong"
}
```

#### Error

Error message.

```json
{
  "type": "error",
  "message": "Subscription failed: limit exceeded (100 max)"
}
```

### Limits

- **Max subscriptions per connection**: 100
- **Max proof_id length**: 64 characters

### Example: Complete Flow

```javascript
const ws = new WebSocket('wss://your-calendar.example.com/v1/stream');

ws.onopen = () => {
  // Subscribe to a pending proof
  ws.send(JSON.stringify({
    type: 'subscribe',
    proof_id: 'ktcs_a1b2c3d4e5f6g7h8'
  }));
};

ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);

  switch (msg.type) {
    case 'subscribed':
      console.log('Subscribed to', msg.proof_id);
      break;

    case 'batched':
      console.log('Proof is being processed');
      break;

    case 'confirmed':
      console.log('Proof confirmed!');
      console.log('Block:', msg.block_hash);
      console.log('DAA Score:', msg.daa_score);
      // Decode and save the proof
      const proofBytes = atob(msg.proof);
      break;

    case 'error':
      console.error('Error:', msg.message);
      break;
  }
};
```

---

## Error Responses

All error responses follow this format:

```json
{
  "error": "Error message describing the problem"
}
```

### HTTP Status Codes

| Code | Meaning |
|------|---------|
| 200 | Success |
| 400 | Bad Request (invalid input) |
| 401 | Unauthorized (missing/invalid API key) |
| 403 | Forbidden (e.g., invalid WebSocket origin) |
| 404 | Not Found |
| 413 | Payload Too Large |
| 429 | Too Many Requests (rate limited) |
| 500 | Internal Server Error |

---

## CORS

The server supports Cross-Origin Resource Sharing (CORS).

Configure allowed origins via the `CORS_ORIGINS` environment variable:

```bash
# Single origin
CORS_ORIGINS=https://app.example.com

# Multiple origins
CORS_ORIGINS=https://app.example.com,https://www.example.com

# All origins (development only!)
CORS_ORIGINS=*
```

WebSocket connections are also validated against allowed origins.

---

## SDK Examples

### JavaScript/TypeScript

```typescript
class CalendarClient {
  private baseUrl: string;
  private apiKey?: string;

  constructor(baseUrl: string, apiKey?: string) {
    this.baseUrl = baseUrl;
    this.apiKey = apiKey;
  }

  private headers(): HeadersInit {
    const h: HeadersInit = { 'Content-Type': 'application/json' };
    if (this.apiKey) h['X-API-Key'] = this.apiKey;
    return h;
  }

  async stamp(digest: string, batchMode: string = 'standard'): Promise<StampResponse> {
    const res = await fetch(`${this.baseUrl}/v1/stamp`, {
      method: 'POST',
      headers: this.headers(),
      body: JSON.stringify({ digest, batch_mode: batchMode }),
    });
    if (!res.ok) throw new Error(await res.text());
    return res.json();
  }

  async getStamp(id: string): Promise<StampResponse> {
    const res = await fetch(`${this.baseUrl}/v1/stamp/${id}`, {
      headers: this.headers(),
    });
    if (!res.ok) throw new Error(await res.text());
    return res.json();
  }

  async verify(proofBytes: Uint8Array): Promise<VerifyResponse> {
    const res = await fetch(`${this.baseUrl}/v1/verify`, {
      method: 'POST',
      headers: { ...this.headers(), 'Content-Type': 'application/octet-stream' },
      body: proofBytes,
    });
    if (!res.ok) throw new Error(await res.text());
    return res.json();
  }
}
```

### cURL Examples

**Submit a stamp:**
```bash
curl -X POST https://your-calendar.example.com/v1/stamp \
  -H "Content-Type: application/json" \
  -H "X-API-Key: your-key" \
  -d '{"digest":"abc123...","batch_mode":"standard"}'
```

**Get stamp status:** (public — no API key)
```bash
curl https://your-calendar.example.com/v1/stamp/ktcs_abc123
```

**Verify a proof:** (public — no API key)
```bash
curl -X POST https://your-calendar.example.com/v1/verify \
  -H "Content-Type: application/octet-stream" \
  --data-binary @document.kts
```

**Health check:**
```bash
curl https://your-calendar.example.com/health
```

---

## See Also

- [Architecture](../concepts/architecture.md)
- [Proof Format](../concepts/proof-format.md)
- [Deployment Guide](../deployment/production.md)
