# KTCS — Kaspa Thermodynamic Clock Service
## UI/UX Specification & Design Brief

**Project:** Kaspathon Hackathon Submission  
**Version:** 1.0  
**Date:** January 2026  

---

## Executive Summary

KTCS is a trustless timestamping service built on Kaspa's high-throughput BlockDAG. This design brief establishes a **Palantir-inspired** visual language: dark, data-dense, technically precise, and operationally sophisticated. The interface should feel like mission-critical infrastructure—serious, trustworthy, and powerful.

**Design Thesis:** *"Thermodynamic truth, visualized."*

The UI makes the invisible visible—showing users the actual proof-of-work accumulating behind their timestamps, the DAG structure proving their data's existence, and the thermodynamic weight securing their proofs.

---

## 1. Design Philosophy

### 1.1 Core Principles

| Principle | Implementation |
|-----------|----------------|
| **Data Sovereignty** | Users see exactly what's happening—no abstraction hiding the cryptographic reality |
| **Thermodynamic Transparency** | Visualize PoW accumulation as energy/security, not just "confirmations" |
| **Operational Confidence** | Interface conveys trustworthiness through precision and clarity |
| **Technical Honesty** | Show the DAG, the hashes, the proofs—don't dumb it down |

### 1.2 Aesthetic Direction

**Primary Influence:** Palantir Foundry / Gotham  
**Secondary Influences:** Bloomberg Terminal, NASA Mission Control, Military C2 Systems

**Tone:** Sophisticated, technical, authoritative, precise  
**Atmosphere:** Dark command center monitoring critical infrastructure  
**Emotion:** Confidence, control, clarity

### 1.3 Design Mantras

- "Information density without chaos"
- "Every pixel earns its place"
- "The data is the interface"
- "Precision over decoration"

---

## 2. Visual Language

### 2.1 Color System

#### Primary Palette

```css
:root {
  /* Backgrounds — Deep space blacks */
  --bg-primary: #0A0C10;      /* Main canvas */
  --bg-secondary: #0F1218;    /* Elevated surfaces */
  --bg-tertiary: #151921;     /* Cards, panels */
  --bg-quaternary: #1C212B;   /* Hover states, wells */
  
  /* Borders & Dividers */
  --border-subtle: #1E2530;   /* Subtle separation */
  --border-default: #2A3241;  /* Standard borders */
  --border-strong: #3D4759;   /* Emphasized borders */
  
  /* Text Hierarchy */
  --text-primary: #F0F2F5;    /* Primary content */
  --text-secondary: #9BA3B0;  /* Secondary content */
  --text-tertiary: #5C6370;   /* Disabled, hints */
  --text-inverse: #0A0C10;    /* Text on light backgrounds */
  
  /* Accent — Kaspa Teal (Primary Action) */
  --accent-primary: #49EACB;  /* Primary actions, key data */
  --accent-primary-dim: #49EACB40; /* Backgrounds, glows */
  --accent-primary-bright: #7FFFD4; /* Hover states */
  
  /* Accent — Electric Blue (Secondary) */
  --accent-secondary: #4A9EFF; /* Links, secondary actions */
  --accent-secondary-dim: #4A9EFF30;
  
  /* Semantic Colors */
  --status-success: #34D399;  /* Confirmed, valid */
  --status-warning: #FBBF24;  /* Pending, attention */
  --status-error: #F87171;    /* Error, invalid */
  --status-info: #60A5FA;     /* Informational */
  
  /* Thermodynamic Gradient */
  --thermo-cold: #3B82F6;     /* Low security (recent) */
  --thermo-warm: #8B5CF6;     /* Medium security */
  --thermo-hot: #EC4899;      /* High security (aged) */
  
  /* Special Effects */
  --glow-kaspa: 0 0 20px #49EACB40, 0 0 40px #49EACB20;
  --glow-active: 0 0 10px #49EACB60;
}
```

#### Color Usage Rules

1. **Accent colors are rare** — Used only for primary actions, key metrics, and status indicators
2. **Text is gray-scale** — Never colorize body text except for semantic meaning
3. **Backgrounds are layered** — Depth conveyed through subtle background shifts, not shadows
4. **Glows indicate activity** — Pulsing/glowing elements show live data or active processes

### 2.2 Typography

#### Font Stack

```css
:root {
  /* Display — Technical, precise, authoritative */
  --font-display: 'JetBrains Mono', 'Fira Code', 'SF Mono', monospace;
  
  /* Interface — Clean, readable, professional */
  --font-interface: 'Inter', 'SF Pro Display', -apple-system, sans-serif;
  
  /* Data — Tabular, fixed-width for alignment */
  --font-data: 'JetBrains Mono', 'IBM Plex Mono', monospace;
}
```

#### Type Scale

| Token | Size | Weight | Line Height | Use |
|-------|------|--------|-------------|-----|
| `--type-display-xl` | 48px | 700 | 1.1 | Hero headlines |
| `--type-display-lg` | 36px | 600 | 1.2 | Page titles |
| `--type-display-md` | 28px | 600 | 1.2 | Section headers |
| `--type-heading-lg` | 20px | 600 | 1.3 | Card titles |
| `--type-heading-md` | 16px | 600 | 1.4 | Subsection headers |
| `--type-body-lg` | 16px | 400 | 1.6 | Primary body |
| `--type-body-md` | 14px | 400 | 1.5 | Secondary body |
| `--type-body-sm` | 12px | 400 | 1.5 | Captions, labels |
| `--type-mono-lg` | 16px | 500 | 1.4 | Hashes, code |
| `--type-mono-md` | 14px | 500 | 1.4 | Data values |
| `--type-mono-sm` | 11px | 500 | 1.3 | Timestamps, IDs |

#### Typography Rules

1. **Hashes always monospace** — All cryptographic data uses `--font-data`
2. **Numbers align** — Use tabular figures for all numeric data
3. **Truncate with care** — Hashes show first 8 + last 4 chars: `abc12345...6789`
4. **Case matters** — Labels UPPERCASE, values as-is, hashes lowercase

### 2.3 Spacing System

```css
:root {
  --space-1: 4px;
  --space-2: 8px;
  --space-3: 12px;
  --space-4: 16px;
  --space-5: 20px;
  --space-6: 24px;
  --space-8: 32px;
  --space-10: 40px;
  --space-12: 48px;
  --space-16: 64px;
  --space-20: 80px;
}
```

**Grid System:** 12-column grid, 24px gutters, 80px margins (desktop)

### 2.4 Iconography

**Style:** Outlined, 1.5px stroke, rounded caps  
**Library:** Lucide Icons (primary), custom icons for crypto/DAG concepts

**Custom Icons Required:**
- DAG/BlockDAG structure
- Merkle tree
- Thermodynamic meter/gauge
- Proof certificate
- Hash/fingerprint
- Blue work indicator

### 2.5 Motion Design

```css
:root {
  /* Timing Functions */
  --ease-out-expo: cubic-bezier(0.16, 1, 0.3, 1);
  --ease-in-out-expo: cubic-bezier(0.87, 0, 0.13, 1);
  --ease-spring: cubic-bezier(0.34, 1.56, 0.64, 1);
  
  /* Durations */
  --duration-instant: 100ms;
  --duration-fast: 200ms;
  --duration-normal: 300ms;
  --duration-slow: 500ms;
  --duration-slower: 800ms;
}
```

**Animation Principles:**
1. **Data animations are continuous** — Live metrics animate smoothly, not in jumps
2. **User actions are snappy** — Button presses respond in <100ms
3. **Transitions are purposeful** — Movement indicates relationship or state change
4. **Ambient motion is subtle** — Background activity indicators pulse slowly

---

## 3. Component Library

### 3.1 Core Components

#### 3.1.1 Button

```
┌─────────────────────────────────────────────────────────────┐
│  PRIMARY BUTTON                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  ▷ STAMP DOCUMENT                                    │   │
│  └─────────────────────────────────────────────────────┘   │
│  Background: --accent-primary                               │
│  Text: --text-inverse                                       │
│  Border: none                                               │
│  Padding: 12px 24px                                         │
│  Border-radius: 4px                                         │
│  Font: --font-interface, 14px, 600, uppercase              │
│  Letter-spacing: 0.05em                                     │
│  Hover: brightness(1.1), subtle glow                        │
│  Active: scale(0.98)                                        │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│  SECONDARY BUTTON                                           │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  ◇ VERIFY PROOF                                      │   │
│  └─────────────────────────────────────────────────────┘   │
│  Background: transparent                                    │
│  Text: --text-primary                                       │
│  Border: 1px solid --border-default                         │
│  Hover: border-color --accent-primary, text --accent-primary│
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│  GHOST BUTTON                                               │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  ↗ View on Explorer                                  │   │
│  └─────────────────────────────────────────────────────┘   │
│  Background: transparent                                    │
│  Text: --accent-secondary                                   │
│  Border: none                                               │
│  Hover: underline                                           │
└─────────────────────────────────────────────────────────────┘
```

#### 3.1.2 Input Field

```
┌─────────────────────────────────────────────────────────────┐
│  TEXT INPUT                                                 │
│                                                             │
│  DOCUMENT HASH                                              │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ 0x                                                   │   │
│  │ abc123def456789...                              ⎘   │   │
│  └─────────────────────────────────────────────────────┘   │
│  Enter SHA-256 hash or drop file to compute                │
│                                                             │
│  Label: --text-secondary, 11px, uppercase, letter-spacing  │
│  Input: --bg-quaternary, --text-primary, monospace         │
│  Border: 1px solid --border-default                         │
│  Focus: border --accent-primary, glow                       │
│  Helper: --text-tertiary, 12px                             │
└─────────────────────────────────────────────────────────────┘
```

#### 3.1.3 File Drop Zone

```
┌─────────────────────────────────────────────────────────────┐
│  FILE DROP ZONE                                             │
│                                                             │
│  ┌ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┐   │
│  │                                                     │   │
│  │              ┌───────────────────┐                  │   │
│  │              │    ⬆ UPLOAD      │                  │   │
│  │              │     or drop      │                  │   │
│  │              └───────────────────┘                  │   │
│  │                                                     │   │
│  │         Drop file to compute hash locally           │   │
│  │            File never leaves your device            │   │
│  │                                                     │   │
│  └ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┘   │
│                                                             │
│  Border: 2px dashed --border-default                        │
│  Background: --bg-tertiary                                  │
│  Drag-over: border --accent-primary, bg --accent-primary-dim│
└─────────────────────────────────────────────────────────────┘
```

#### 3.1.4 Status Badge

```
┌─────────────────────────────────────────────────────────────┐
│  STATUS BADGES                                              │
│                                                             │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │ ● CONFIRMED  │  │ ◐ PENDING    │  │ ✕ INVALID    │      │
│  └──────────────┘  └──────────────┘  └──────────────┘      │
│                                                             │
│  Confirmed: --status-success, subtle green glow             │
│  Pending: --status-warning, pulsing animation               │
│  Invalid: --status-error                                    │
│                                                             │
│  Padding: 4px 8px                                           │
│  Border-radius: 2px                                         │
│  Font: 11px, uppercase, 600                                 │
│  Background: color at 15% opacity                           │
└─────────────────────────────────────────────────────────────┘
```

#### 3.1.5 Data Card

```
┌─────────────────────────────────────────────────────────────┐
│  DATA CARD                                                  │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ ▸ BLOCK ATTESTATION                    ● CONFIRMED  │   │
│  ├─────────────────────────────────────────────────────┤   │
│  │                                                     │   │
│  │  BLOCK HASH                                         │   │
│  │  abc12345...6789                            ⎘  ↗   │   │
│  │                                                     │   │
│  │  DAA SCORE              BLUE SCORE                  │   │
│  │  42,847,291             42,501,832                  │   │
│  │                                                     │   │
│  │  TIMESTAMP                                          │   │
│  │  2026-01-23 14:32:01.847 UTC                       │   │
│  │                                                     │   │
│  │  ──────────────────────────────────────────────    │   │
│  │                                                     │   │
│  │  THERMODYNAMIC WEIGHT                               │   │
│  │  ████████████████████░░░░░░░░░░  1.23 × 10¹⁸       │   │
│  │                                                     │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
│  Background: --bg-tertiary                                  │
│  Border: 1px solid --border-subtle                          │
│  Border-radius: 6px                                         │
│  Header: --bg-quaternary, 12px padding                      │
│  Content: 16px padding                                      │
└─────────────────────────────────────────────────────────────┘
```

#### 3.1.6 Metric Display

```
┌─────────────────────────────────────────────────────────────┐
│  METRIC DISPLAY (Large)                                     │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │                                                     │   │
│  │  BLUE WORK ACCUMULATED                              │   │
│  │                                                     │   │
│  │  2.47 × 10¹⁷                                       │   │
│  │  ▲ +1.2% last hour                                  │   │
│  │                                                     │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
│  Label: --text-tertiary, 11px, uppercase                   │
│  Value: --text-primary, 36px, --font-display               │
│  Delta: --status-success (positive) or --status-error      │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│  METRIC DISPLAY (Compact)                                   │
│                                                             │
│  ┌────────────┐ ┌────────────┐ ┌────────────┐              │
│  │ DAA SCORE  │ │ BLOCKS     │ │ TIME       │              │
│  │ 42.8M      │ │ +12,847    │ │ 21m 14s    │              │
│  └────────────┘ └────────────┘ └────────────┘              │
│                                                             │
│  Arranged in horizontal row                                 │
│  Subtle left border accent                                  │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 Specialized Components

#### 3.2.1 DAG Visualizer

```
┌─────────────────────────────────────────────────────────────┐
│  DAG VISUALIZER                                             │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │                                                     │   │
│  │         ┌───┐                                       │   │
│  │         │ ● │ ← Your timestamp (highlighted)        │   │
│  │         └─┬─┘                                       │   │
│  │           │                                         │   │
│  │      ┌────┴────┐                                    │   │
│  │      │         │                                    │   │
│  │    ┌─┴─┐     ┌─┴─┐                                  │   │
│  │    │ ○ │     │ ○ │ ← Parent blocks                  │   │
│  │    └─┬─┘     └─┬─┘                                  │   │
│  │      │    ╲  ╱ │                                    │   │
│  │      │     ╲╱  │                                    │   │
│  │    ┌─┴─┐ ┌─┴─┐ ┌─┴─┐                                │   │
│  │    │ ○ │ │ ○ │ │ ○ │                                │   │
│  │    └───┘ └───┘ └───┘                                │   │
│  │                                                     │   │
│  │    ◀ Earlier            Later ▶                     │   │
│  │                                                     │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
│  Nodes: 12px circles                                        │
│  Your block: --accent-primary, glow effect                  │
│  Blue blocks: --accent-secondary at 60%                     │
│  Red blocks: --status-error at 40%                          │
│  Edges: 1px --border-default                                │
│  Animation: Smooth pan, zoom on scroll                      │
│  Interaction: Hover node → show block details tooltip       │
└─────────────────────────────────────────────────────────────┘
```

#### 3.2.2 Thermodynamic Gauge

```
┌─────────────────────────────────────────────────────────────┐
│  THERMODYNAMIC SECURITY GAUGE                               │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │                                                     │   │
│  │                    SECURITY LEVEL                   │   │
│  │                                                     │   │
│  │               ╭─────────────────╮                   │   │
│  │            ╭──╯                 ╰──╮                │   │
│  │          ╭─╯      ▲               ╰─╮              │   │
│  │         ╭╯       ╱│╲                ╰╮             │   │
│  │        ╭╯       ╱ │ ╲                ╰╮            │   │
│  │       ╭╯───────╱──┼──╲───────────────╰╮           │   │
│  │       ├─LOW────┼MEDIUM┼───HIGH────────┤           │   │
│  │                   │                                 │   │
│  │                                                     │   │
│  │              2.47 × 10¹⁷ blue work                 │   │
│  │              ≈ 3.2 Bitcoin confirmations            │   │
│  │                                                     │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
│  Arc: gradient from --thermo-cold → --thermo-warm → hot    │
│  Needle: --text-primary, animated movement                  │
│  Glow: increases with security level                        │
└─────────────────────────────────────────────────────────────┘
```

#### 3.2.3 Proof Chain Visualizer

```
┌─────────────────────────────────────────────────────────────┐
│  PROOF CHAIN                                                │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │                                                     │   │
│  │  YOUR DATA                                          │   │
│  │  ┌──────────────────────────────────────────────┐  │   │
│  │  │ SHA256: abc123...                            │  │   │
│  │  └──────────────────────┬───────────────────────┘  │   │
│  │                         │                           │   │
│  │                         ▼ append nonce              │   │
│  │  ┌──────────────────────────────────────────────┐  │   │
│  │  │ def456...                                    │  │   │
│  │  └──────────────────────┬───────────────────────┘  │   │
│  │                         │                           │   │
│  │                         ▼ sha256                    │   │
│  │  ┌──────────────────────────────────────────────┐  │   │
│  │  │ 789abc...                                    │  │   │
│  │  └──────────────────────┬───────────────────────┘  │   │
│  │                         │                           │   │
│  │            ┌────────────┴────────────┐              │   │
│  │            ▼                         ▼              │   │
│  │     Merkle sibling            Merkle sibling        │   │
│  │            │                         │              │   │
│  │            └────────────┬────────────┘              │   │
│  │                         │                           │   │
│  │                         ▼ sha256                    │   │
│  │  ┌──────────────────────────────────────────────┐  │   │
│  │  │ MERKLE ROOT: fedcba...                       │  │   │
│  │  └──────────────────────┬───────────────────────┘  │   │
│  │                         │                           │   │
│  │                         ▼ OP_RETURN                 │   │
│  │  ╔══════════════════════════════════════════════╗  │   │
│  │  ║ KASPA TRANSACTION                            ║  │   │
│  │  ║ TX: 123fed...                           ↗   ║  │   │
│  │  ╚══════════════════════════════════════════════╝  │   │
│  │                         │                           │   │
│  │                         ▼                           │   │
│  │  ╔══════════════════════════════════════════════╗  │   │
│  │  ║ █ KASPA BLOCK                                ║  │   │
│  │  ║ DAA: 42,847,291                              ║  │   │
│  │  ╚══════════════════════════════════════════════╝  │   │
│  │                                                     │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
│  Operations: monospace, --text-secondary                    │
│  Hashes: monospace, --accent-primary (truncated)           │
│  Final block: double border, --accent-primary glow          │
└─────────────────────────────────────────────────────────────┘
```

#### 3.2.4 Live Activity Feed

```
┌─────────────────────────────────────────────────────────────┐
│  NETWORK ACTIVITY                                     LIVE ●│
│  ├──────────────────────────────────────────────────────────│
│  │                                                          │
│  │  14:32:01.847  BLOCK  daa:42847291  hash:abc12...  ■■■  │
│  │  14:32:01.749  TX     stamp        hash:def45...  ■■    │
│  │  14:32:01.652  BLOCK  daa:42847290  hash:789ab...  ■■■  │
│  │  14:32:01.553  BLOCK  daa:42847289  hash:cde01...  ■■■  │
│  │  14:32:01.451  TX     stamp        hash:234fe...  ■■    │
│  │  ···                                                     │
│  │                                                          │
│  └──────────────────────────────────────────────────────────┘
│                                                             │
│  Monospace throughout                                       │
│  Timestamps: --text-tertiary                               │
│  Types: color-coded badges                                  │
│  Hashes: truncated, clickable                              │
│  Activity bars: recent blocks visualization                 │
│  Auto-scroll with new entries                              │
│  "LIVE" indicator pulses                                    │
└─────────────────────────────────────────────────────────────┘
```

---

## 4. Screen Specifications

### 4.1 Landing Page

**Purpose:** Introduce KTCS, enable quick stamping, show network status

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │  KTCS                              STAMP  VERIFY  DOCS  NETWORK    ●  │  │
│  │  Kaspa Thermodynamic Clock                                            │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │                                                                       │  │
│  │                                                                       │  │
│  │          T R U S T L E S S   T I M E S T A M P I N G                 │  │
│  │                                                                       │  │
│  │              Prove data existed. Backed by physics.                   │  │
│  │                                                                       │  │
│  │                                                                       │  │
│  │  ┌ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┐  │  │
│  │  │                                                               │  │  │
│  │  │                     ⬆ DROP FILE TO STAMP                      │  │  │
│  │  │                                                               │  │  │
│  │  │                  or paste hash below                          │  │  │
│  │  │                                                               │  │  │
│  │  └ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┘  │  │
│  │                                                                       │  │
│  │  ┌─────────────────────────────────────────────────────┐              │  │
│  │  │ 0x                                               ⎘ │  [ STAMP ]   │  │
│  │  └─────────────────────────────────────────────────────┘              │  │
│  │                                                                       │  │
│  │                                                                       │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│  ┌─────────────────────┐ ┌─────────────────────┐ ┌─────────────────────┐   │
│  │ NETWORK STATUS      │ │ LAST STAMP          │ │ SECURITY           │   │
│  │                     │ │                     │ │                     │   │
│  │ ● OPERATIONAL       │ │ 0.3s ago            │ │ 1.23 × 10¹⁸        │   │
│  │                     │ │                     │ │ blue work          │   │
│  │ 10 BPS              │ │ abc123...           │ │                     │   │
│  │ 42.8M DAA           │ │                     │ │ ≈ 6 BTC conf       │   │
│  └─────────────────────┘ └─────────────────────┘ └─────────────────────┘   │
│                                                                             │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │  RECENT BLOCKS                                                  LIVE ●│  │
│  │  ─────────────────────────────────────────────────────────────────    │  │
│  │  ■ ■ ■ ■ ■ ■ ■ ■ ■ ■ ■ ■ ■ ■ ■ ■ ■ ■ ■ ■ ■ ■ ■ ■ ■ ■ ■ ■ ■ ■ ▶    │  │
│  │  Each block = 100ms of thermodynamic time                             │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Key Interactions:**
- File drop → computes hash client-side → auto-fills input
- Paste hash → validates format → enables stamp button
- Stamp button → shows progress → navigates to proof page
- Block feed → live WebSocket updates → click block to view

### 4.2 Stamp Progress Screen

**Purpose:** Show real-time progress of timestamp creation

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │  KTCS                              STAMP  VERIFY  DOCS  NETWORK    ●  │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │                                                                       │  │
│  │                          STAMPING IN PROGRESS                         │  │
│  │                                                                       │  │
│  │  ┌─────────────────────────────────────────────────────────────────┐  │  │
│  │  │                                                                 │  │  │
│  │  │    ● ─────────────── ● ─────────────── ◐ ─────────────── ○     │  │  │
│  │  │   Hash              Submit            Confirm            Done   │  │  │
│  │  │  computed           to DAG            in block                  │  │  │
│  │  │                                                                 │  │  │
│  │  └─────────────────────────────────────────────────────────────────┘  │  │
│  │                                                                       │  │
│  │  ┌─────────────────────────────────────────────────────────────────┐  │  │
│  │  │                                                                 │  │  │
│  │  │  DOCUMENT HASH                                                  │  │  │
│  │  │  abc123def456789012345678901234567890123456789012345678901234    │  │  │
│  │  │                                                                 │  │  │
│  │  │  ─────────────────────────────────────────────────────────────  │  │  │
│  │  │                                                                 │  │  │
│  │  │  COMMITMENT                                                     │  │  │
│  │  │  def456...7890                                              ⎘  │  │  │
│  │  │                                                                 │  │  │
│  │  │  TRANSACTION                                                    │  │  │
│  │  │  789abc...1234                                         ↗   ⎘  │  │  │
│  │  │                                                                 │  │  │
│  │  │  ─────────────────────────────────────────────────────────────  │  │  │
│  │  │                                                                 │  │  │
│  │  │  WAITING FOR BLOCK CONFIRMATION...                              │  │  │
│  │  │                                                                 │  │  │
│  │  │     ◐  Estimated: < 1 second                                    │  │  │
│  │  │                                                                 │  │  │
│  │  │  ████████████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░     │  │  │
│  │  │                                                                 │  │  │
│  │  └─────────────────────────────────────────────────────────────────┘  │  │
│  │                                                                       │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Animations:**
- Progress dots: pulse when active, solid when complete
- Progress bar: smooth fill animation
- Transaction hash: typing animation as it appears
- Confirmation: burst animation when block confirms

### 4.3 Proof View Screen

**Purpose:** Display complete proof with all verification details

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │  KTCS                              STAMP  VERIFY  DOCS  NETWORK    ●  │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │                                                                       │  │
│  │  TIMESTAMP PROOF                                        ● CONFIRMED   │  │
│  │  ═══════════════════════════════════════════════════════════════════  │  │
│  │                                                                       │  │
│  │  ┌────────────────────────────────┐ ┌────────────────────────────┐   │  │
│  │  │ DOCUMENT                       │ │ THERMODYNAMIC SECURITY     │   │  │
│  │  │                                │ │                            │   │  │
│  │  │ HASH                           │ │      ╭────────────╮        │   │  │
│  │  │ abc123def456...                │ │   ╭──╯    ▲       ╰──╮     │   │  │
│  │  │                            ⎘  │ │  ╭╯      ╱│╲         ╰╮    │   │  │
│  │  │                                │ │ ╭╯      ╱ │ ╲         ╰╮   │   │  │
│  │  │ SIZE                           │ │ ├──LOW──┼MED┼──HIGH────┤   │   │  │
│  │  │ 2,847 bytes                    │ │                            │   │  │
│  │  │                                │ │ 2.47 × 10¹⁷ blue work      │   │  │
│  │  │ ALGORITHM                      │ │ ≈ 3.2 BTC confirmations    │   │  │
│  │  │ SHA-256                        │ │                            │   │  │
│  │  └────────────────────────────────┘ └────────────────────────────┘   │  │
│  │                                                                       │  │
│  │  ┌────────────────────────────────────────────────────────────────┐  │  │
│  │  │ ▸ BLOCK ATTESTATION                                            │  │  │
│  │  │                                                                │  │  │
│  │  │  BLOCK HASH                                                    │  │  │
│  │  │  fedcba9876543210...                                    ⎘  ↗  │  │  │
│  │  │                                                                │  │  │
│  │  │  ┌───────────────────┐ ┌───────────────────┐                   │  │  │
│  │  │  │ DAA SCORE         │ │ BLUE SCORE        │                   │  │  │
│  │  │  │ 42,847,291        │ │ 42,501,832        │                   │  │  │
│  │  │  └───────────────────┘ └───────────────────┘                   │  │  │
│  │  │                                                                │  │  │
│  │  │  TIMESTAMP                                                     │  │  │
│  │  │  2026-01-23 14:32:01.847 UTC  (42 minutes ago)                │  │  │
│  │  │                                                                │  │  │
│  │  │  PARENT BLOCKS (3)                                             │  │  │
│  │  │  ├─ abc123...  daa:42847290                                    │  │  │
│  │  │  ├─ def456...  daa:42847290                                    │  │  │
│  │  │  └─ 789abc...  daa:42847289                                    │  │  │
│  │  │                                                                │  │  │
│  │  └────────────────────────────────────────────────────────────────┘  │  │
│  │                                                                       │  │
│  │  ┌────────────────────────────────────────────────────────────────┐  │  │
│  │  │ ▸ PROOF CHAIN                                         [Expand] │  │  │
│  │  └────────────────────────────────────────────────────────────────┘  │  │
│  │                                                                       │  │
│  │  ┌────────────────────────────────────────────────────────────────┐  │  │
│  │  │ ▸ DAG VISUALIZATION                                   [Expand] │  │  │
│  │  └────────────────────────────────────────────────────────────────┘  │  │
│  │                                                                       │  │
│  │  ┌────────────────────────────────────────────────────────────────┐  │  │
│  │  │                                                                │  │  │
│  │  │  [ DOWNLOAD .kts ]      [ VERIFY AGAIN ]      [ SHARE LINK ]   │  │  │
│  │  │                                                                │  │  │
│  │  └────────────────────────────────────────────────────────────────┘  │  │
│  │                                                                       │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 4.4 Verify Screen

**Purpose:** Upload/paste proof for verification

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │  KTCS                              STAMP  VERIFY  DOCS  NETWORK    ●  │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │                                                                       │  │
│  │                       V E R I F Y   P R O O F                         │  │
│  │                                                                       │  │
│  │              Validate a timestamp proof independently                 │  │
│  │                                                                       │  │
│  │  ┌─────────────────────────────────────────────────────────────────┐  │  │
│  │  │                                                                 │  │  │
│  │  │  PROOF FILE (.kts)                                              │  │  │
│  │  │                                                                 │  │  │
│  │  │  ┌ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┐  │  │  │
│  │  │  │                                                         │  │  │  │
│  │  │  │                    ⬆ DROP .kts FILE                     │  │  │  │
│  │  │  │                                                         │  │  │  │
│  │  │  └ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┘  │  │  │
│  │  │                                                                 │  │  │
│  │  │  ─────────────────────── OR ───────────────────────             │  │  │
│  │  │                                                                 │  │  │
│  │  │  ORIGINAL DOCUMENT (optional, for hash verification)            │  │  │
│  │  │                                                                 │  │  │
│  │  │  ┌ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┐  │  │  │
│  │  │  │                                                         │  │  │  │
│  │  │  │              ⬆ DROP ORIGINAL DOCUMENT                   │  │  │  │
│  │  │  │                                                         │  │  │  │
│  │  │  └ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┘  │  │  │
│  │  │                                                                 │  │  │
│  │  │                           [ VERIFY ]                            │  │  │
│  │  │                                                                 │  │  │
│  │  └─────────────────────────────────────────────────────────────────┘  │  │
│  │                                                                       │  │
│  │  ┌─────────────────────────────────────────────────────────────────┐  │  │
│  │  │  ℹ VERIFICATION MODES                                           │  │  │
│  │  │                                                                 │  │  │
│  │  │  ● FULL (Kaspa node)     Trustless, queries blockchain          │  │  │
│  │  │  ○ LIGHT (API)           Fast, trusts verification service      │  │  │
│  │  │                                                                 │  │  │
│  │  └─────────────────────────────────────────────────────────────────┘  │  │
│  │                                                                       │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 4.5 Network Dashboard

**Purpose:** Show live Kaspa network status and KTCS metrics

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │  KTCS                              STAMP  VERIFY  DOCS  NETWORK    ●  │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│  ┌──────────────────────────┐ ┌──────────────────────────────────────────┐  │
│  │  KASPA NETWORK           │ │  KTCS SERVICE                            │  │
│  │  ● OPERATIONAL           │ │  ● OPERATIONAL                           │  │
│  │                          │ │                                          │  │
│  │  ┌────────┐ ┌────────┐   │ │  ┌────────┐ ┌────────┐ ┌────────┐       │  │
│  │  │10 BPS  │ │42.8M   │   │ │  │ 1,247  │ │ 0.3s   │ │  99.9% │       │  │
│  │  │blocks  │ │DAA     │   │ │  │today   │ │avg     │ │uptime  │       │  │
│  │  └────────┘ └────────┘   │ │  └────────┘ └────────┘ └────────┘       │  │
│  │                          │ │                                          │  │
│  │  BLUE WORK (24h)         │ │  CALENDARS ONLINE                        │  │
│  │  ▁▂▃▄▅▆▇█▇▆▅▄▃▄▅▆▇█▇▆▅▄ │ │  ● alpha.ktcs.kaspa.org                  │  │
│  │  1.23×10¹⁸ current       │ │  ● beta.ktcs.kaspa.org                   │  │
│  │                          │ │  ● gamma.ktcs.kaspa.org                  │  │
│  └──────────────────────────┘ └──────────────────────────────────────────┘  │
│                                                                             │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │  LIVE DAG VIEW                                                  ⛶    │  │
│  │  ─────────────────────────────────────────────────────────────────    │  │
│  │                                                                       │  │
│  │            ┌───┐     ┌───┐                                            │  │
│  │            │   │─────│   │                                            │  │
│  │            └─┬─┘     └─┬─┘                                            │  │
│  │         ┌───┴───┐ ┌───┴───┐                                           │  │
│  │         │       │ │       │                                           │  │
│  │       ┌─┴─┐   ┌─┴─┴─┐   ┌─┴─┐                                         │  │
│  │       │   │───│     │───│   │                                         │  │
│  │       └─┬─┘   └──┬──┘   └─┬─┘                                         │  │
│  │         │    ╲   │   ╱    │                                           │  │
│  │         └─────╲──┼──╱─────┘                                           │  │
│  │               ┌──┴──┐                                                 │  │
│  │               │ TIP │                                                 │  │
│  │               └─────┘                                                 │  │
│  │                                                                       │  │
│  │  ◀ Pan                                           Zoom ▶  [ Reset ]   │  │
│  │                                                                       │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │  RECENT ACTIVITY                                                LIVE ●│  │
│  │  ─────────────────────────────────────────────────────────────────    │  │
│  │  14:32:01.847  STAMP   abc123...  ● confirmed   daa:42847291         │  │
│  │  14:32:01.652  BLOCK   fedcba...  blue score: 42501832               │  │
│  │  14:31:58.234  STAMP   def456...  ● confirmed   daa:42847288         │  │
│  │  14:31:57.891  STAMP   789abc...  ● confirmed   daa:42847287         │  │
│  │  14:31:55.123  BLOCK   012def...  blue score: 42501829               │  │
│  │  ···                                                                  │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 5. Interaction Patterns

### 5.1 Micro-Interactions

| Element | Trigger | Animation |
|---------|---------|-----------|
| Button hover | mouseenter | Background lightens 10%, subtle lift (translateY -1px) |
| Button press | mousedown | Scale 0.98, background darkens |
| Input focus | focus | Border color → accent, subtle glow appears |
| Hash truncation hover | mouseenter | Expand to show full hash in tooltip |
| Copy button | click | Icon changes to checkmark, reverts after 2s |
| External link | click | Brief scale pulse, icon rotates |
| Status badge (pending) | continuous | Gentle pulse animation (opacity 0.7-1.0) |
| Live indicator | continuous | Dot pulses with glow |

### 5.2 Page Transitions

**Navigation:** Fade out (150ms) → Fade in (200ms) with slight slide up

**Modal open:** Backdrop fades in (200ms), modal scales from 0.95 → 1.0 with opacity

**Accordion expand:** Height animates with ease-out-expo, content fades in

### 5.3 Loading States

```
┌─────────────────────────────────────────────────────────────┐
│  SKELETON LOADING                                           │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │                                                     │   │
│  │  ████████████████                                   │   │
│  │                                                     │   │
│  │  ██████████████████████████████████████████         │   │
│  │  ████████████████████████████                       │   │
│  │                                                     │   │
│  │  ████████████  ████████████                         │   │
│  │                                                     │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
│  Skeleton: --bg-quaternary                                  │
│  Animation: shimmer gradient left-to-right, 1.5s loop      │
│  Shimmer: linear-gradient with --bg-tertiary highlight      │
└─────────────────────────────────────────────────────────────┘
```

### 5.4 Error States

```
┌─────────────────────────────────────────────────────────────┐
│  ERROR DISPLAY                                              │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  ✕ VERIFICATION FAILED                               │   │
│  │  ─────────────────────────────────────────────────   │   │
│  │                                                     │   │
│  │  The proof could not be verified.                   │   │
│  │                                                     │   │
│  │  ERROR: Block not found in selected chain           │   │
│  │  Block hash: abc123...                              │   │
│  │  Expected DAA: 42847291                             │   │
│  │                                                     │   │
│  │  This may indicate:                                 │   │
│  │  • A recent chain reorganization                    │   │
│  │  • A corrupted proof file                           │   │
│  │  • An invalid or forged proof                       │   │
│  │                                                     │   │
│  │  [ TRY AGAIN ]   [ CONTACT SUPPORT ]                │   │
│  │                                                     │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
│  Border-left: 4px solid --status-error                      │
│  Header background: --status-error at 10%                   │
└─────────────────────────────────────────────────────────────┘
```

---

## 6. Responsive Design

### 6.1 Breakpoints

```css
:root {
  --bp-mobile: 480px;
  --bp-tablet: 768px;
  --bp-desktop: 1024px;
  --bp-wide: 1440px;
}
```

### 6.2 Layout Adaptations

| Viewport | Grid | Navigation | Cards |
|----------|------|------------|-------|
| Mobile (<480px) | 1 column | Bottom sheet | Stack vertically |
| Tablet (480-768px) | 2 columns | Collapsed header | 2-up grid |
| Desktop (768-1024px) | 12 columns | Full header | 3-up grid |
| Wide (>1440px) | 12 columns, max-width | Full header | 4-up grid |

### 6.3 Mobile Considerations

- DAG visualizer becomes horizontal scroll with pinch-zoom
- Proof chain collapses to steps list (expandable)
- Metrics stack vertically in single column
- Touch targets minimum 44px
- Bottom navigation bar for primary actions

---

## 7. Accessibility

### 7.1 Requirements

- **WCAG 2.1 AA compliance minimum**
- Color contrast ratios: 4.5:1 for normal text, 3:1 for large text
- All interactive elements keyboard accessible
- Focus indicators visible and consistent
- Screen reader compatible (ARIA labels)
- Reduced motion option respects `prefers-reduced-motion`

### 7.2 Color-Blind Considerations

- Status indicators include icons, not just color
- Charts use patterns in addition to color
- Critical information never conveyed by color alone

### 7.3 Focus States

```css
*:focus-visible {
  outline: 2px solid var(--accent-primary);
  outline-offset: 2px;
  border-radius: 2px;
}
```

---

## 8. Technical Implementation Notes

### 8.1 Recommended Stack

| Layer | Technology | Rationale |
|-------|------------|-----------|
| Framework | React 18+ or Svelte | Component architecture, good tooling |
| Styling | Tailwind CSS + CSS Variables | Utility-first with design tokens |
| Animation | Framer Motion | Declarative, performant |
| Charts | D3.js | Custom DAG visualization |
| State | Zustand or Jotai | Lightweight, TypeScript-friendly |
| API | TanStack Query | Caching, real-time updates |
| WebSocket | Native + reconnecting-websocket | Live data feeds |

### 8.2 Performance Targets

- First Contentful Paint: < 1.5s
- Time to Interactive: < 3s
- Lighthouse Performance: > 90
- Bundle size (gzipped): < 150KB initial

### 8.3 Key Technical Challenges

1. **DAG Visualization**: Custom D3 force-directed graph with 1000+ nodes, needs WebGL fallback for performance
2. **Real-time Updates**: WebSocket connection with graceful degradation to polling
3. **Client-side Hashing**: Web Crypto API for SHA-256, fallback to WebAssembly for large files
4. **Proof Parsing**: Binary .kts parsing in browser (DataView API)

---

## 9. Hackathon Deliverables

### 9.1 MVP Scope (4 weeks)

**Must Have:**
- [ ] Landing page with file drop stamping
- [ ] Stamp progress screen with live updates
- [ ] Proof view with basic attestation details
- [ ] Verify screen with proof upload
- [ ] Basic network status display

**Should Have:**
- [ ] DAG visualization (simplified)
- [ ] Thermodynamic gauge
- [ ] Proof chain visualizer
- [ ] Download .kts functionality

**Nice to Have:**
- [ ] Full network dashboard
- [ ] Calendar selection
- [ ] Batch stamping
- [ ] Concurrent event proofs

### 9.2 Demo Script

1. **Introduction** (30s): Explain thermodynamic timestamping concept
2. **Stamp Demo** (60s): Drop file, show sub-second confirmation
3. **Proof Exploration** (60s): Walk through proof details, DAG context
4. **Verification** (30s): Verify the proof independently
5. **Network View** (30s): Show live DAG, explain security accumulation
6. **Closing** (30s): Compare to OTS, highlight Kaspa advantages

---

## 10. Asset Requirements

### 10.1 Logo

- Primary: "KTCS" monogram, geometric, works on dark backgrounds
- Icon: Simplified version for favicon, app icon
- Wordmark: "Kaspa Thermodynamic Clock Service"

### 10.2 Custom Icons

- BlockDAG structure
- Merkle tree
- Thermometer/gauge
- Certificate/proof
- Hash symbol
- Clock with physics motif

### 10.3 Illustrations

- Hero illustration: Abstract representation of data → PoW → proof
- Empty states: Minimal line art for each section
- Error states: Appropriate for technical context

---

## Appendix A: Component Specifications

### A.1 Detailed Button Specifications

```css
.btn-primary {
  background: var(--accent-primary);
  color: var(--text-inverse);
  font-family: var(--font-interface);
  font-size: 14px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  padding: 12px 24px;
  border: none;
  border-radius: 4px;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out-expo);
}

.btn-primary:hover {
  filter: brightness(1.1);
  box-shadow: var(--glow-active);
}

.btn-primary:active {
  transform: scale(0.98);
}

.btn-primary:disabled {
  background: var(--bg-quaternary);
  color: var(--text-tertiary);
  cursor: not-allowed;
  box-shadow: none;
}
```

### A.2 Typography Specimens

```
DISPLAY XL (48px/700)
TRUSTLESS TIMESTAMPING

Display Large (36px/600)
Proof of Existence

Heading Large (20px/600)
Block Attestation Details

Body Large (16px/400)
Your document has been timestamped and anchored to the Kaspa 
blockchain. The thermodynamic weight of this proof increases 
with every block added to the DAG.

MONO LARGE (16px/500)
0xabc123def456789012345678901234567890

mono small (11px/500)
2026-01-23T14:32:01.847Z | daa:42847291
```

---

## Appendix B: Color Accessibility Matrix

| Foreground | Background | Contrast | Pass |
|------------|------------|----------|------|
| --text-primary | --bg-primary | 15.2:1 | ✓ AAA |
| --text-secondary | --bg-primary | 7.4:1 | ✓ AAA |
| --text-tertiary | --bg-primary | 4.1:1 | ✓ AA (large) |
| --accent-primary | --bg-primary | 9.8:1 | ✓ AAA |
| --text-inverse | --accent-primary | 8.2:1 | ✓ AAA |
| --status-success | --bg-tertiary | 6.1:1 | ✓ AA |
| --status-error | --bg-tertiary | 5.3:1 | ✓ AA |

---

*End of Design Brief*
