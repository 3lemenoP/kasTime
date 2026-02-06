# Web Frontend

React/TypeScript web interface for KTCS timestamping.

## Technology Stack

| Technology | Version | Purpose |
|------------|---------|---------|
| React | 18.2 | UI framework |
| TypeScript | 5.3 | Type safety |
| Vite | 5.0 | Build tool |
| Tailwind CSS | 4.0 | Styling |
| Zustand | 4.4 | State management |
| TanStack Query | 5.0 | Data fetching |
| D3.js | 7.8 | DAG visualization |
| Framer Motion | 10.16 | Animations |

## Development

### Setup

```bash
# Install dependencies
npm install

# Start development server
npm run dev
# Opens at http://localhost:5173

# Build for production
npm run build
# Output in dist/
```

### Environment Variables

Create `.env` for development:

```bash
VITE_CALENDAR_URL=http://localhost:3001
VITE_KASPA_RPC=ws://localhost:16110
```

## Project Structure

```
src/
├── api/                    # API clients
│   ├── calendar.ts         # Calendar server API
│   └── kaspa.ts           # Kaspa RPC client
├── components/             # React components
│   ├── ui/                # Reusable UI components
│   │   ├── Button.tsx
│   │   ├── FileDropZone.tsx
│   │   ├── MetricCard.tsx
│   │   ├── ThermodynamicGauge.tsx
│   │   └── WalletInput.tsx
│   ├── proof/             # Proof-related components
│   │   ├── BlockAttestation.tsx
│   │   └── ConfirmationHero.tsx
│   └── Layout.tsx         # Main layout wrapper
├── lib/                    # Utilities
│   ├── wasm.ts            # WASM module loader
│   └── blockchainVerify.ts # Blockchain verification
├── pages/                  # Page components
│   ├── HomePage.tsx       # Stamp creation
│   ├── ProofPage.tsx      # Proof display
│   └── VerifyPage.tsx     # Verification
├── stores/                 # Zustand stores
│   └── stamp.ts           # Stamp state
├── types/                  # TypeScript types
│   ├── api.ts             # API response types
│   ├── proof.ts           # Proof types
│   └── index.ts           # Exported types
├── wasm/                   # Pre-compiled WASM
│   ├── ktcs_wasm.js
│   ├── ktcs_wasm_bg.wasm
│   └── *.d.ts
├── App.tsx                # Root component
├── main.tsx               # Entry point
└── index.css              # Global styles
```

## Key Features

### File Upload

- Drag-and-drop interface
- Client-side SHA256 hashing (files never uploaded)
- Progress indication
- File type validation

### Batch Mode Selection

- Instant (100ms)
- Standard (1s) - default
- Economic (10s)

### Calendar Stamping Flow

1. User drops file
2. Frontend hashes file locally
3. Submits digest to calendar
4. Receives pending proof
5. WebSocket subscription for confirmation
6. Downloads complete proof

### Direct Stamping Flow

1. User drops file
2. User enters wallet key
3. Frontend hashes file locally
4. Builds transaction via WASM
5. Signs transaction via WASM
6. Submits directly to Kaspa
7. Builds proof locally

### Proof Verification

- Upload `.kts` proof file
- Optional: Upload original file for full verification
- Client-side verification via WASM
- Server-side verification fallback

## Design System

Based on Palantir-inspired dark UI:

### Colors

| Color | Hex | Usage |
|-------|-----|-------|
| Background | `#0A0C10` | Main background |
| Surface | `#1C212B` | Cards, panels |
| Accent | `#49EACB` | Kaspa teal |
| Text | `#FFFFFF` | Primary text |
| Muted | `#8B949E` | Secondary text |

### Typography

| Element | Font | Weight |
|---------|------|--------|
| UI Text | Inter | 400, 500, 600 |
| Code/Hash | JetBrains Mono | 400 |

### Components

- `Button` - Primary and secondary variants
- `FileDropZone` - File upload with drag states
- `MetricCard` - Display stats and metrics
- `WalletInput` - Secure key input (type=password)

## WASM Integration

```typescript
// src/lib/wasm.ts
import init, * as wasm from '../wasm/ktcs_wasm';

let initialized = false;

export async function initWasm() {
  if (!initialized) {
    await init();
    initialized = true;
  }
}

export async function computeSha256Hex(data: Uint8Array): Promise<string> {
  await initWasm();
  return wasm.compute_sha256_hex(data);
}

export async function verifyProof(proofBytes: Uint8Array, data?: Uint8Array) {
  await initWasm();
  return wasm.verify_proof(proofBytes, data || null);
}
```

## State Management

Using Zustand for simple, performant state:

```typescript
// src/stores/stamp.ts
import { create } from 'zustand';

interface StampState {
  digest: string | null;
  proofId: string | null;
  status: 'idle' | 'hashing' | 'submitting' | 'pending' | 'confirmed';
  setDigest: (digest: string) => void;
  setProofId: (id: string) => void;
  setStatus: (status: StampState['status']) => void;
  reset: () => void;
}

export const useStampStore = create<StampState>((set) => ({
  digest: null,
  proofId: null,
  status: 'idle',
  setDigest: (digest) => set({ digest }),
  setProofId: (id) => set({ proofId: id }),
  setStatus: (status) => set({ status }),
  reset: () => set({ digest: null, proofId: null, status: 'idle' }),
}));
```

## API Client

```typescript
// src/api/calendar.ts
class CalendarClient {
  private baseUrl: string;

  constructor(baseUrl: string) {
    this.baseUrl = baseUrl;
  }

  async stamp(digest: string, batchMode: string = 'standard') {
    const res = await fetch(`${this.baseUrl}/v1/stamp`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ digest, batch_mode: batchMode }),
    });
    return res.json();
  }

  async getStamp(id: string) {
    const res = await fetch(`${this.baseUrl}/v1/stamp/${id}`);
    return res.json();
  }

  subscribeToConfirmation(proofId: string, onConfirmed: (msg: any) => void) {
    const ws = new WebSocket(`${this.baseUrl.replace('http', 'ws')}/v1/stream`);
    ws.onopen = () => {
      ws.send(JSON.stringify({ type: 'subscribe', proof_id: proofId }));
    };
    ws.onmessage = (event) => {
      const msg = JSON.parse(event.data);
      if (msg.type === 'confirmed') onConfirmed(msg);
    };
    return () => ws.close();
  }
}
```

## Building for Production

```bash
npm run build
```

Output in `dist/`. Deploy to any static hosting:

- Vercel
- Netlify
- AWS S3 + CloudFront
- nginx

## Security

- **Files never uploaded** - All hashing happens client-side
- **Private keys** - Only enter when using direct stamping
- **Keys stay in browser** - Never sent to calendar server
- **WASM memory** - Keys handled in WASM, not JS heap

## See Also

- [WASM Bindings](wasm.md)
- [API Reference](../api/reference.md)
- [Architecture](../concepts/architecture.md)
