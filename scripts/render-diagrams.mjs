#!/usr/bin/env node
/**
 * Render Mermaid diagrams using beautiful-mermaid
 *
 * Usage: node scripts/render-diagrams.mjs
 *
 * Reads diagram definitions from scripts/diagrams.mjs and outputs
 * SVGs to docs/assets/diagrams/
 */

import { renderMermaid } from 'beautiful-mermaid';
import { writeFile, mkdir } from 'fs/promises';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const OUTPUT_DIR = join(__dirname, '../docs/assets/diagrams');

// Kaspa-inspired theme using beautiful-mermaid's theming system
const kaspaTheme = {
  name: 'kaspa',
  background: '#0A0C10',
  foreground: '#E6EDF3',
  // Optional enriched colors
  accent: '#49EACB',
  accentForeground: '#0A0C10',
  border: '#30363D',
  surface: '#1C212B',
  surfaceForeground: '#E6EDF3',
  muted: '#8B949E',
  mutedForeground: '#8B949E',
};

// Light theme variant
const kaspaLightTheme = {
  name: 'kaspa-light',
  background: '#FFFFFF',
  foreground: '#1F2328',
  accent: '#49EACB',
  accentForeground: '#0A0C10',
  border: '#D0D7DE',
  surface: '#F6F8FA',
  surfaceForeground: '#1F2328',
  muted: '#656D76',
  mutedForeground: '#656D76',
};

// Diagram definitions
const diagrams = {
  // System Architecture Overview
  'architecture-overview': `
flowchart TB
    subgraph Clients["CLIENTS"]
        CLI["CLI<br/>(ktcs-cli)"]
        WebApp["Web App<br/>(React/TS)"]
        WASM["WASM<br/>(Browser)"]
        Libraries["Direct Libraries<br/>(Rust/JS SDK)"]
    end

    subgraph Calendar["CALENDAR SERVER"]
        Batch["Batch hashes"]
        Merkle["Build Merkle"]
        Submit["Submit TX"]
        Proofs["Return proofs"]
    end

    subgraph Direct["DIRECT STAMPING"]
        LocalTX["Client builds TX locally"]
        Sign["Signs with own wallet"]
        DirectSubmit["Submits directly to node"]
    end

    subgraph Kaspa["KASPA NETWORK"]
        BlockDAG["BLOCKDAG (GHOSTDAG)<br/>10 blocks/sec • P2PK commitments<br/>DAA score • Blue work"]
    end

    CLI --> Calendar
    WebApp --> Calendar
    WASM --> Calendar
    Libraries --> Calendar
    CLI --> Direct
    WebApp --> Direct
    WASM --> Direct
    Libraries --> Direct

    Calendar --> BlockDAG
    Direct --> BlockDAG

    style Clients fill:#1C212B,stroke:#49EACB,color:#E6EDF3
    style Calendar fill:#1C212B,stroke:#49EACB,color:#E6EDF3
    style Direct fill:#1C212B,stroke:#49EACB,color:#E6EDF3
    style Kaspa fill:#0A0C10,stroke:#49EACB,color:#E6EDF3
    style BlockDAG fill:#49EACB,stroke:#3BC4A8,color:#0A0C10
`,

  // Calendar Stamping Flow
  'calendar-flow': `
sequenceDiagram
    participant C as Client
    participant S as Calendar Server
    participant K as Kaspa Network

    C->>S: POST /v1/stamp<br/>{digest, batch_mode}
    S-->>C: pending_proof

    Note over S: Batch window expires<br/>Build Merkle tree

    S->>K: Submit P2PK TX
    K-->>S: Block confirmation

    S-->>C: WS: confirmed<br/>{proof, block_hash}

    C->>S: GET /v1/stamp/{id}
    S-->>C: Complete proof (.kts)
`,

  // Direct Stamping Flow
  'direct-flow': `
sequenceDiagram
    participant C as Client
    participant K as Kaspa Network

    Note over C: Hash document locally<br/>digest = SHA256(document)

    Note over C: Generate nonce (16 bytes)<br/>commitment = SHA256(nonce || digest)

    Note over C: Build P2PK transaction<br/>Output 0: 0.2 KAS to P2PK(commitment)<br/>Output 1: Change to wallet

    Note over C: Sign transaction with wallet

    C->>K: Submit transaction
    K-->>C: Confirmation

    Note over C: Build proof locally<br/>Save .kts proof file
`,

  // Verification Flow
  'verification-flow': `
flowchart TB
    Parse["1. Parse .kts proof file<br/>Validate magic bytes<br/>Extract digest, ops, attestations"]
    Verify["2. Verify against original data<br/>Compute SHA256(original)<br/>Compare with proof digest"]
    Compute["3. Compute commitment<br/>Apply operations to state"]
    Attest["4. Verify attestation<br/>Fetch TX from Kaspa node<br/>Check P2PK output<br/>Verify block in selected chain"]
    Security["5. Calculate thermodynamic security<br/>Get current blue work<br/>accumulated = current - attestation.blue_work"]
    Result["RESULT: Valid timestamp<br/>at DAA score N with X blue work"]

    Parse --> Verify
    Verify --> Compute
    Compute --> Attest
    Attest --> Security
    Security --> Result

    style Parse fill:#1C212B,stroke:#49EACB,color:#E6EDF3
    style Verify fill:#1C212B,stroke:#49EACB,color:#E6EDF3
    style Compute fill:#1C212B,stroke:#49EACB,color:#E6EDF3
    style Attest fill:#1C212B,stroke:#49EACB,color:#E6EDF3
    style Security fill:#1C212B,stroke:#49EACB,color:#E6EDF3
    style Result fill:#49EACB,stroke:#3BC4A8,color:#0A0C10
`,

  // Dual-Wallet Recycling
  'wallet-recycling': `
flowchart LR
    subgraph Stamp["STAMP Wallet"]
        SReceives["Receives:<br/>- User funding"]
        SSends["Sends:<br/>- Commitment TX"]
    end

    subgraph Return["RETURN Wallet"]
        RReceives["Receives:<br/>- TX change"]
        RSends["Sends:<br/>- Recycle to STAMP"]
    end

    SSends -->|"Change"| RReceives
    RSends -->|"Recycle loop<br/>(balance > threshold)"| SReceives

    style Stamp fill:#1C212B,stroke:#49EACB,color:#E6EDF3
    style Return fill:#1C212B,stroke:#49EACB,color:#E6EDF3
`,

  // Proof Structure
  'proof-structure': `
flowchart TB
    subgraph Header["HEADER (8 bytes)"]
        Magic["Magic: 'KTCS' (4 bytes)"]
        Version["Version: u16"]
        Algorithm["Algorithm: u8"]
        Flags["Flags: u8"]
    end

    subgraph Body["BODY"]
        Digest["Digest (32 bytes)<br/>SHA256 hash"]
        Ops["Operations<br/>Merkle path transforms"]
        Attest["Attestations<br/>Kaspa block reference"]
    end

    Header --> Body

    style Header fill:#49EACB,stroke:#3BC4A8,color:#0A0C10
    style Body fill:#1C212B,stroke:#49EACB,color:#E6EDF3
`,

  // Trust Boundaries
  'trust-boundaries': `
flowchart TB
    subgraph Trustless["TRUSTLESS ZONE"]
        T1["Proof verification<br/>(anyone with Kaspa node)"]
        T2["Direct stamping<br/>(user controls wallet)"]
        T3["Blue work calculation<br/>(deterministic from chain)"]
    end

    subgraph Calendar["CALENDAR ZONE"]
        Can["Calendar CAN:<br/>- Delay batching (DoS)<br/>- Refuse service"]
        Cannot["Calendar CANNOT:<br/>- Forge timestamps<br/>- Backdate proofs<br/>- Tamper with proofs"]
    end

    style Trustless fill:#49EACB,stroke:#3BC4A8,color:#0A0C10
    style Calendar fill:#1C212B,stroke:#49EACB,color:#E6EDF3
`,
};

async function renderDiagrams() {
  // Ensure output directory exists
  await mkdir(OUTPUT_DIR, { recursive: true });

  console.log('Rendering diagrams with beautiful-mermaid...\n');

  for (const [name, mermaid] of Object.entries(diagrams)) {
    try {
      // Render dark theme version
      const svgDark = await renderMermaid(mermaid.trim(), {
        theme: kaspaTheme,
      });
      const darkPath = join(OUTPUT_DIR, `${name}-dark.svg`);
      await writeFile(darkPath, svgDark);
      console.log(`  ✓ ${name}-dark.svg`);

      // Render light theme version
      const svgLight = await renderMermaid(mermaid.trim(), {
        theme: kaspaLightTheme,
      });
      const lightPath = join(OUTPUT_DIR, `${name}-light.svg`);
      await writeFile(lightPath, svgLight);
      console.log(`  ✓ ${name}-light.svg`);
    } catch (error) {
      console.error(`  ✗ ${name}: ${error.message}`);
    }
  }

  console.log('\nDone!');
}

renderDiagrams().catch(console.error);
