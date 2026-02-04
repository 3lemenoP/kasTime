# Quick Start

Create your first timestamp in under 5 minutes.

## Choose Your Path

=== "Web Interface (Easiest)"

    ### 1. Open the Web App

    Visit the hosted instance or run locally:

    ```bash
    npm install && npm run dev
    ```

    ### 2. Drop a File

    Drag and drop any file onto the upload area. The file is hashed locally - it never leaves your browser.

    ### 3. Select Batch Mode

    | Mode | Speed | Cost |
    |------|-------|------|
    | Instant | ~100ms | Higher |
    | Standard | ~1s | Medium |
    | Economic | ~10s | Lowest |

    ### 4. Submit

    Click "Create Timestamp". You'll receive a pending proof immediately.

    ### 5. Wait for Confirmation

    The proof updates automatically when the commitment is confirmed on-chain.

    ### 6. Download Proof

    Save the `.kts` file. This is your proof of existence.

=== "CLI (Recommended)"

    ### 1. Install the CLI

    ```bash
    cargo install --path ktcs-cli
    ```

    ### 2. Stamp a File

    ```bash
    ktcs stamp document.pdf
    ```

    Output:
    ```
    Hashing document.pdf...
    Digest: abc123def456...
    Submitting to calendar...
    Proof saved to: document.pdf.kts (pending)
    Waiting for confirmation...
    Confirmed at DAA score 42847291
    Proof updated: document.pdf.kts (complete)
    ```

    ### 3. Verify the Proof

    ```bash
    ktcs verify -d document.pdf document.pdf.kts
    ```

    Output:
    ```
    Proof is VALID
      Digest: abc123def456...
      Timestamp: 2026-01-23 12:00:00 UTC
      DAA Score: 42,847,291
      Block: abc123...
    ```

=== "Direct Stamping (No Calendar)"

    For maximum trustlessness, stamp directly to the blockchain with your own wallet.

    ### 1. Generate a Wallet

    ```bash
    ktcs wallet generate -n testnet -o wallet.key
    chmod 600 wallet.key
    ```

    ### 2. Fund the Wallet

    Get testnet KAS from a faucet, or transfer mainnet KAS.

    Check balance:
    ```bash
    ktcs wallet balance --wallet-file wallet.key -n testnet
    ```

    ### 3. Direct Stamp

    ```bash
    ktcs stamp --direct \
      --wallet-file wallet.key \
      --network testnet \
      document.pdf
    ```

    !!! note "Cost"
        Direct stamping burns 0.2 KAS per timestamp, plus transaction fees.

## Understanding Your Proof

The `.kts` file contains:

1. **Digest** - SHA256 hash of your document
2. **Operations** - Merkle path (if batched via calendar)
3. **Attestation** - Kaspa block reference with:
   - DAA score (block height equivalent)
   - Blue score (chain selection metric)
   - Block hash
   - Timestamp
   - Blue work (thermodynamic security)

## Verify Later

Anyone can verify your proof with:

```bash
ktcs verify -d original-document.pdf proof.kts
```

Or in the browser:

```javascript
import { verify_proof } from 'ktcs-wasm';

const result = verify_proof(proofBytes, documentBytes);
if (result.valid) {
  console.log(`Document existed before ${result.attestations[0].timestamp}`);
}
```

## Common Tasks

### Upgrade a Pending Proof

If you have a pending proof (confirmation in progress):

```bash
ktcs upgrade document.kts
```

### Get Proof Information

```bash
ktcs info document.kts
```

### Hash Without Stamping

```bash
ktcs hash document.pdf
```

## Next Steps

- [Proof Format](../concepts/proof-format.md) - Understand the `.kts` file structure
- [Architecture](../concepts/architecture.md) - How KTCS works
- [Security Model](../concepts/security.md) - Trust assumptions and guarantees
- [CLI Reference](../components/cli.md) - All CLI commands
