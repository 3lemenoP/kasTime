# ktcs-wasm

WebAssembly bindings for KTCS, enabling browser-based timestamp operations.

## Features

- **Proof Verification**: Verify `.kts` proofs without a server
- **Proof Parsing**: Extract information from proof files
- **Hashing**: SHA256 computation in browser
- **Wallet Operations**: Address derivation and validation
- **Transaction Building**: Build and sign commitment transactions
- **Direct Stamping**: Complete direct stamping workflow in browser

## Installation

### npm

```bash
npm install ktcs-wasm
```

### Build from Source

```bash
cd ktcs-wasm
wasm-pack build --target web
# Output in pkg/
```

## Usage

### Initialize

```javascript
import init, {
  verify_proof,
  parse_proof,
  compute_sha256_hex,
  get_wallet_address,
  build_commitment_transaction,
  sign_transaction,
} from 'ktcs-wasm';

// Initialize WASM module
await init();
```

### Verify a Proof

```javascript
// Load proof file
const proofBytes = new Uint8Array(await file.arrayBuffer());

// Verify proof structure
const result = verify_proof(proofBytes, null);
console.log(result.valid);        // true/false
console.log(result.digest);       // hex-encoded digest
console.log(result.attestations); // attestation details

// Verify against original data
const originalData = new Uint8Array(await originalFile.arrayBuffer());
const fullResult = verify_proof(proofBytes, originalData);
```

**Verification Result:**

```typescript
interface VerificationResult {
  valid: boolean;
  digest: string;                    // hex
  computed_commitment: string;       // hex
  attestations: AttestationInfo[];
  error: string | null;
}

interface AttestationInfo {
  attestation_type: 'pending' | 'kaspa' | 'bitcoin';
  complete: boolean;
  daa_score?: number;
  blue_score?: number;
  block_hash?: string;
  timestamp?: number;
  tx_hash?: string;
  calendar_url?: string;
}
```

### Parse Proof Information

```javascript
const proofBytes = new Uint8Array(/* .kts file */);
const info = parse_proof(proofBytes);

console.log(info.version);           // 1
console.log(info.hash_algorithm);    // "Sha256"
console.log(info.digest);            // hex
console.log(info.is_complete);       // true/false
console.log(info.operations_count);  // number of operations
console.log(info.attestations);      // attestation details
```

### Hash a File

```javascript
const fileData = new Uint8Array(await file.arrayBuffer());

// Get hash as hex string
const hashHex = compute_sha256_hex(fileData);
console.log(hashHex); // 64-character hex string

// Get hash as bytes
const hashBytes = compute_sha256(fileData);
console.log(hashBytes); // Uint8Array(32)
```

### Quick Format Check

```javascript
const proofBytes = new Uint8Array(/* potential .kts file */);

if (is_valid_proof_format(proofBytes)) {
  // File has valid KTCS magic bytes
  const info = parse_proof(proofBytes);
}
```

## Wallet Functions

### Get Address from Private Key

```javascript
const privateKeyHex = 'abc123...'; // 64-character hex (32 bytes)
const address = get_wallet_address(privateKeyHex, 'mainnet');
console.log(address); // kaspa:qr...
```

### Get Wallet Info

```javascript
const info = get_wallet_info(privateKeyHex, 'mainnet');
console.log(info.address);     // kaspa:qr...
console.log(info.public_key);  // hex-encoded public key
console.log(info.network);     // mainnet
```

### Validate Private Key

```javascript
if (validate_private_key(keyHex)) {
  // Valid 32-byte hex key
}
```

### Validate Address

```javascript
if (validate_address('kaspa:qr...')) {
  // Valid Kaspa address
}
```

## Transaction Building

### Create Commitment

```javascript
// Generate random nonce
const nonceHex = generate_nonce(); // 16 bytes as hex

// Hash document
const hashHex = compute_sha256_hex(documentBytes);

// Create privacy-preserving commitment
const commitmentHex = create_commitment(nonceHex, hashHex);
// commitment = SHA256(nonce || hash)
```

### Build Transaction

```javascript
// UTXOs from Kaspa node
const utxos = [
  {
    transaction_id: 'abc123...', // 32 bytes hex
    index: 0,
    amount: 100000000,           // sompi
    script_public_key_hex: '...',
    block_daa_score: 42000000,
    is_coinbase: false,
  },
];

// Build unsigned transaction
const txResult = build_commitment_transaction(
  commitmentHex,          // 32-byte commitment
  JSON.stringify(utxos),  // UTXOs as JSON
  'kaspa:qr...',          // Change address
  1                       // Fee per gram (sompi)
);

console.log(txResult.transaction_json); // Unsigned TX
console.log(txResult.commitment);       // Commitment hex
console.log(txResult.total_input);      // Total input sompi
console.log(txResult.total_output);     // Total output sompi
console.log(txResult.fee);              // Fee in sompi
console.log(txResult.change_amount);    // Change in sompi
```

### Sign Transaction

```javascript
const signedResult = sign_transaction(
  txResult.transaction_json,
  JSON.stringify(utxos),
  privateKeyHex,
  'mainnet'
);

console.log(signedResult.transaction_json); // Signed TX
```

### Submit Transaction

```javascript
// Create RPC request for submission
const rpcRequest = create_submit_tx_rpc_request(signedResult.transaction_json);

// Send to Kaspa node via WebSocket
const ws = new WebSocket('wss://kaspa-node/wrpc');
ws.send(JSON.stringify(rpcRequest));
```

## Proof Building

### Build Pending Proof

```javascript
const digestHex = compute_sha256_hex(documentBytes);
const nonceHex = generate_nonce();

const pendingProofBytes = build_pending_proof(
  digestHex,
  nonceHex,
  'https://calendar.example.com/v1/stamp/ktcs_abc123'
);

// Save pending proof
downloadFile(pendingProofBytes, 'document.kts');
```

### Complete Proof with Attestation

```javascript
const attestation = {
  daa_score: 42847291,
  blue_score: 42501832,
  block_hash: 'abc123...',
  timestamp: 1706012400987,
  tx_hash: 'def456...',
  tx_index: 0,
  blue_work: '0123456789...',
  parent_hashes: ['abc...', 'def...'],
};

const completeProofBytes = complete_proof(
  pendingProofBytes,
  JSON.stringify(attestation)
);
```

## Constants

```javascript
// Get burn amount for commitments (0.2 KAS)
const burnAmount = get_commitment_burn_amount();
console.log(burnAmount); // 20000000 sompi

// Get library version
const version = get_version();
console.log(version); // "0.1.0"
```

## API Reference

### Verification

| Function | Description |
|----------|-------------|
| `verify_proof(proofBytes, dataBytes?)` | Full proof verification |
| `parse_proof(proofBytes)` | Parse proof metadata |
| `is_valid_proof_format(proofBytes)` | Quick magic bytes check |

### Hashing

| Function | Description |
|----------|-------------|
| `compute_sha256(data)` | Returns Uint8Array (32 bytes) |
| `compute_sha256_hex(data)` | Returns hex string (64 chars) |

### Wallet

| Function | Description |
|----------|-------------|
| `get_wallet_address(keyHex, network)` | Get address from key |
| `get_wallet_info(keyHex, network)` | Get address + public key |
| `validate_private_key(keyHex)` | Check key format |
| `validate_address(address)` | Check address format |

### Commitment

| Function | Description |
|----------|-------------|
| `create_commitment(nonceHex, hashHex)` | SHA256(nonce \|\| hash) |
| `generate_nonce()` | 16 random bytes as hex |
| `get_commitment_burn_amount()` | Burn amount (20M sompi) |

### Transaction

| Function | Description |
|----------|-------------|
| `build_commitment_transaction(...)` | Build unsigned TX |
| `sign_transaction(...)` | Sign TX with private key |
| `create_submit_tx_rpc_request(txJson)` | Format for Kaspa RPC |

### Proof Building

| Function | Description |
|----------|-------------|
| `build_pending_proof(...)` | Create pending proof |
| `complete_proof(...)` | Add attestation to proof |
| `serialize_proof_to_bytes(proofJson)` | Serialize proof object |

## Browser Compatibility

Requires WebAssembly support (all modern browsers):
- Chrome 57+
- Firefox 52+
- Safari 11+
- Edge 16+

Uses Web Crypto API (`crypto.getRandomValues`) for secure random generation.

## Vite Configuration

For Vite projects:

```typescript
// vite.config.ts
export default defineConfig({
  optimizeDeps: {
    exclude: ['ktcs-wasm'],
  },
  build: {
    target: 'esnext',  // For top-level await
  },
});
```

## React Integration

```typescript
// src/lib/wasm.ts
let wasmModule: typeof import('ktcs-wasm') | null = null;

export async function initWasm() {
  if (!wasmModule) {
    wasmModule = await import('ktcs-wasm');
    await wasmModule.default();
  }
  return wasmModule;
}

export async function verifyProof(proofBytes: Uint8Array) {
  const wasm = await initWasm();
  return wasm.verify_proof(proofBytes, null);
}
```

## Security Notes

- Private keys are handled in WASM memory (not exposed to JS heap)
- Use `crypto.getRandomValues` for all randomness
- Never log or store private keys in browser storage

## See Also

- [Proof Format](../docs/PROOF-FORMAT.md)
- [Architecture](../docs/ARCHITECTURE.md)
- [Security](../docs/SECURITY.md)
