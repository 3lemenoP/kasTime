# Installation

This guide covers installing KTCS components for different use cases.

## Quick Install

=== "Web Interface"

    ```bash
    git clone https://github.com/3lemenoP/kasTime.git
    cd kasTime
    npm install
    npm run dev
    ```

    Open [http://localhost:5173](http://localhost:5173) in your browser.

=== "CLI Tool"

    ```bash
    # From source
    cargo install --path ktcs-cli

    # Or build manually
    cargo build --release -p ktcs-cli
    ./target/release/ktcs --version
    ```

=== "Rust Library"

    Add to your `Cargo.toml`:

    ```toml
    [dependencies]
    ktcs-core = { path = "./ktcs-core" }
    # Or from crates.io (when published):
    # ktcs-core = "0.1"
    ```

=== "WASM/JavaScript"

    ```bash
    npm install ktcs-wasm
    ```

    Or build from source:

    ```bash
    cd ktcs-wasm
    wasm-pack build --target web
    ```

## Prerequisites

### All Platforms

| Requirement | Version | Purpose |
|-------------|---------|---------|
| Git | Any | Clone repository |

### Rust Components (Core, CLI, Calendar, WASM)

| Requirement | Version | Purpose |
|-------------|---------|---------|
| Rust | 1.78+ | Compiler (required for the committed `Cargo.lock` v4) |
| Cargo | Latest | Package manager |

Install Rust:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Web Frontend

| Requirement | Version | Purpose |
|-------------|---------|---------|
| Node.js | 18+ | Runtime |
| npm | 9+ | Package manager |

### WASM Development

| Requirement | Version | Purpose |
|-------------|---------|---------|
| wasm-pack | 0.12+ | WASM build tool |

Install wasm-pack:

```bash
cargo install wasm-pack
```

### Calendar Server (Production)

| Requirement | Purpose |
|-------------|---------|
| Kaspa Node | Blockchain access |
| SQLite 3 | Database |

## Build from Source

### Clone Repository

```bash
git clone https://github.com/3lemenoP/kasTime.git
cd kasTime
```

### Build Everything

```bash
# All Rust components
cargo build --release

# Frontend
npm install
npm run build

# WASM
cd ktcs-wasm
wasm-pack build --target web
```

### Build Individual Components

```bash
# Core library only
cargo build --release -p ktcs-core

# CLI only
cargo build --release -p ktcs-cli

# Calendar server only
cargo build --release -p ktcs-calendar

# WASM only
cd ktcs-wasm && wasm-pack build --target web
```

## Verify Installation

### CLI

```bash
ktcs --version
# ktcs 0.1.0

ktcs hash README.md
# abc123...
```

### Library

```rust
use ktcs_core::merkle::sha256;

fn main() {
    let hash = sha256(b"Hello, KTCS!");
    println!("Hash: {}", hex::encode(hash));
}
```

### WASM

```javascript
import init, { compute_sha256_hex } from 'ktcs-wasm';

await init();
const hash = compute_sha256_hex(new TextEncoder().encode("Hello, KTCS!"));
console.log(hash);
```

## What's Next?

- [Quick Start Guide](quickstart.md) - Create your first timestamp
- [CLI Reference](../components/cli.md) - Full CLI documentation
- [API Reference](../api/reference.md) - Calendar server API
