# KTCS Codebase Analysis

*An in-depth analysis of the entire kasTime repository (KTCS — Kaspa Thermodynamic Clock Service), covering all four Rust crates, the WASM bindings, the React frontend, deployment infrastructure, CI, and documentation. Specification documents in the repo were used as a guide only; every claim below is grounded in the code as it exists (file:line references throughout). Analysis date: 2026-07-30, at commit `d2e71c9`.*

---

## 1. Executive Summary

KTCS is a proof-of-existence timestamping protocol for the Kaspa BlockDAG, built for Kaspathon 2026. It anchors nonce-blinded SHA-256 commitments on-chain as provably unspendable P2PK "burn" outputs (`0x20 <commitment> 0xac`, 0.2 KAS each) and issues compact binary `.kts` proofs modeled on OpenTimestamps: a digest, a chain of Prepend/Append/SHA256 operations, and a Kaspa block attestation carrying DAA score, blue score, blue work, and parent hashes. Two stamping modes exist: a **calendar server** that batches digests into Merkle trees (one on-chain tx per batch), and **direct mode** where the client builds, signs, and submits its own transaction — including entirely in the browser via WASM.

**What is genuinely good:** the binary proof format is well-designed and its implementation matches the spec byte-for-byte; the Cargo workspace is clean; the calendar server has real engineering in it (constant-time API-key comparison, `secrecy`-wrapped keys, documented lock ordering, thorough config validation, production guards on mock mode); the browser really does perform client-side hashing, transaction construction, and Schnorr signing; and ktcs-core's 61 unit tests are fast and genuine.

**What is not:** the project's central claim — *trustless verification* — is not delivered. Offline `verify_proof` accepts any proof containing a fabricated attestation (the computed commitment is never bound to the chain data), and online verification simply trusts whatever an unauthenticated public RPC node says. Beyond that headline, the direct-stamping money path has multiple funds-affecting bugs (UTXO selection that ignores the 0.2 KAS burn, Bitcoin fee/dust constants on a Kaspa chain, blue-work silently zeroed, confirmation polling that can miss real confirmations after funds are burned); the CLI's calendar mode is a facade that never submits anything; the calendar server orphans in-flight stamps on every restart and can double-anchor on timeout; and the production configuration template produces a deployment where every frontend API call returns 401. Documentation drift is systemic — fictional exit codes, unimplemented environment variables, dead config keys, four different repository identities, and a default calendar domain that does not exist.

The codebase reads as a capable hackathon prototype with real cryptographic engineering at its core, wrapped in claims (trustlessness, key hygiene, test coverage) that the implementation does not yet honor.

---

## 2. System Architecture

### 2.1 Components

| Component | Language / stack | Role |
|---|---|---|
| `ktcs-core` | Rust library (~5,300 LOC) | Proof format, operations, Merkle trees, wallet/Schnorr signing, Kaspa sighash, transaction builders, wRPC client, verification |
| `ktcs-cli` | Rust binary (single 2,028-line `main.rs`) | `stamp`, `verify`, `info`, `complete`, `status`, `hash`, `wallet`, `config`, `completions` |
| `ktcs-calendar` | Rust (axum + sqlx/SQLite + tokio) | Aggregation server: batches digests, anchors Merkle roots, REST + WebSocket API |
| `ktcs-wasm` | Rust cdylib (1,101 LOC) | Browser bindings: hashing, wallet derivation, tx build/sign, proof assembly/verification |
| `src/` | React 18 + TypeScript + Vite + zustand | Web app: file drop → stamp (calendar or direct), proof viewer, verifier, bundled docs |
| `docs/`, `deploy/`, CI | mkdocs, nginx, systemd, docker-compose, GitHub Actions | Documentation and two divergent deployment paths |

### 2.2 Data flows

**Calendar stamping (frontend → server):** the browser hashes the file locally via WASM, `POST`s `{digest, algorithm, batch_mode}` to `/v1/stamp`, and receives a pending proof. The server queues the digest in an in-memory `BatchManager` (per-mode windows: instant 100 ms / standard 1 s / economic 10 s, capped at 10,000 pending per mode), and a 100 ms ticker drains elapsed batches: build Merkle tree → submit one commitment tx via the server's STAMP wallet → wait synchronously for block confirmation → write per-stamp Merkle-path proofs with a shared `KaspaAttestation` to SQLite → broadcast `confirmed` events over `/v1/stream` WebSocket. A dual-wallet "recycle" service sweeps change from a RETURN wallet back to the STAMP wallet.

**Direct stamping (CLI or browser):** generate 16-byte CSPRNG nonce → `commitment = SHA256(nonce ‖ SHA256(file))` → fetch UTXOs → build tx (burn output + change) → compute Kaspa sighash (keyed Blake2b, 18-field SIGHASH_ALL layout) → BIP340 Schnorr sign → submit via JSON-wRPC over WebSocket → poll DAG tips for inclusion → append `KaspaAttestation` → write `.kts`.

**Verification:** parse proof → replay operations from digest to commitment → (offline) check that a complete attestation exists; (online) fetch the attested block from an RPC node, check DAA score, tx membership, and that the commitment bytes appear in a tx output script.

### 2.3 The proof format as implemented

The implementation (`ktcs-core/src/types.rs`, `proof.rs`) matches `docs/PROOF_FORMAT_SPECIFICATION.md` closely: 18-byte magic `\0KaspaTime\0\0Proof\0`, 26-byte header (version 0x01, hash-algo 0x08/0x14/0x67, flags, 5 reserved bytes), Bitcoin-style varints, ops `0xF0` Append / `0xF1` Prepend / hash opcodes / `0xFF` Fork, attestations `0x83` Pending (URL) and `0x84` Kaspa (LE u64 DAA/blue/timestamp, 32 B block and tx hashes, LE u32 tx index, 32 B big-endian blue work, varint parent count + hashes). Parse limits are sensible (1 MB per op datum, 10,000 ops, 100 attestations, 1,024 parents).

Divergences from the spec:

- A **Bitcoin attestation tag `0x05`** exists in code (`types.rs:82,92`) but is absent from the spec — a strictly spec-conformant parser would reject valid files. (Commit `8972256` removed Bitcoin references from the docs but not the code.)
- The parser accepts **interleaved ops and attestations** and silently re-partitions them (`proof.rs:94-123`), so a proof does not round-trip byte-identically; combined with **non-canonical varints being accepted**, proofs lack a unique serialization (relevant if proof bytes are ever hashed as identifiers).
- `Fork` is reserved-but-broken: parsing truncates its count u64→u32 (`proof.rs:212`) and `apply_operation` errors on it (`ops.rs:37`), so any proof containing Fork verifies invalid.
- The `tx_index` field means three different things across spec ("output index in TX"), type docs ("transaction index within the block", `types.rs:151`), and code (always output index 0, `direct.rs:198`).

---

## 3. Critical Findings

These are the issues that break the project's core promises or put funds at risk.

### C1. Offline verification is trivially spoofable (ktcs-core)

`verify_proof` computes the commitment from the digest and operations — and then never compares it to anything. Validity is literally `attestations.iter().any(is_complete)` (`ktcs-core/src/verify.rs:126-139`). The attestation contains no data binding commitment→transaction (no tx serialization or Merkle path) or transaction→block (no header). Anyone can attach a fabricated `KaspaAttestation` with arbitrary nonzero hashes and any timestamp to any document and get `valid: true`. A structural sanity-checker `verify_kaspa_attestation` exists (`verify.rs:271`) but is never called. Every consumer surfaces this: the CLI prints "VALID", and the web UI's first verification step shows green, on proofs that are pure fiction.

### C2. Online verification trusts a single unauthenticated RPC node

`verify_attestation_on_chain` (`verify.rs:371`) and the frontend's `blockchainVerify.ts` ask a node whether the block exists and whether the txid is in it — no header proof-of-work check, no transaction Merkle-inclusion proof. The frontend hardcodes one third-party endpoint and always uses mainnet (`VerifyPage.tsx:76`), so **testnet proofs always fail chain verification**; the CLI likewise hardcodes `"mainnet"` for `verify --chain` (`ktcs-cli/src/main.rs:1066`). The resolver silently falls back to a hardcoded third-party node on probe failure (`resolver.rs:112-137`). Additionally, the attested **timestamp is never compared to the on-chain block timestamp** (frontend), and a DAA-score mismatch only emits `console.warn` (`blockchainVerify.ts:121-125`) — so a proof with a real block/tx but a forged timestamp passes every displayed check while the UI displays the forged time. The README's "zero-trust verification" claim is not implemented.

### C3. Commitment matching is a naive byte scan

Both core (`verify.rs:252-260, 419-431`) and frontend (`blockchainVerify.ts:63,76` — bare `String.includes` on script hex) accept the 32-byte commitment appearing at *any* offset of *any* output script — including inside a change output's pubkey or misaligned across fields. A prover can "commit" to a value that happens to equal an existing output pubkey in someone else's transaction. The correct check — exact P2PK burn-script match via `extract_commitment_from_output` (`kaspa_types.rs:232`) — exists and is unused.

### C4. The CLI's calendar mode never submits anything

`ktcs stamp` (calendar mode) prints "Submitting to calendar…" but performs no request — the comment at `ktcs-cli/src/main.rs:605` says "actual calendar submission would go here". It then polls a locally-derived URL (`{calendar}/v1/stamp/ktcs_<first-16-hex>`) for 60 s and gives up, or in `--async` mode writes a pending proof that can never complete against a real calendar. `BatchMode` is parsed and displayed but never transmitted. The CLI's primary advertised function is a facade; only `--direct` mode is real. (The frontend, by contrast, submits correctly — but note the server generates IDs from a UUID while the CLI derives them from the hash prefix, so the two schemes are also incompatible.)

### C5. Direct stamping has funds-affecting bugs (ktcs-core)

- **UTXO selection ignores the burn**: `prepare_direct_stamp` calls `select_utxos(&utxos, 0, fee_rate)` (`direct.rs:103`) — target 0 — so selection stops after one UTXO covering only the fee, while `build` then requires fee + 20,000,000 sompi (`tx.rs:190`). Wallets with ample total balance but no single large UTXO fail with "Insufficient funds".
- **The wrong builder is in production**: `direct.rs:85` uses the old `tx.rs` builder — unchecked arithmetic, a hard error when change falls in the dust range instead of absorbing it into the fee, no duplicate-UTXO detection — while the hardened, near-identical `tx_builder.rs` (which does all three correctly) is exported but unused.
- **Bitcoin constants on a Kaspa chain**: mass is estimated as `10 + 148/input + 34/output` (Bitcoin vbytes; `tx.rs:243`) and `DUST_THRESHOLD = 546` sompi (`kaspa_types.rs:10`) — Kaspa's KIP-9 storage-mass rules make such outputs effectively invalid; the default `fee_rate=10` masks the underestimated mass by overpaying 10×.
- **Blue work silently zeroed**: Kaspa RPC returns `blueWork` as unpadded hex, frequently odd-length; `hex::decode(...).unwrap_or_else(|_| vec![0u8; 32])` (`kaspa.rs:441, 624`) then silently stores all-zero blue work in real proofs — gutting the "thermodynamic security" metric the project is named for. The same silent-default pattern applies to wrong-length block hashes (`kaspa.rs:436-438`).
- **Confirmation polling can miss real confirmations**: `wait_for_confirmation` (`kaspa.rs:1400`) scans only current tips plus one parent level per 100 ms poll; at 10 bps a tx can be buried below that horizon between polls → spurious timeout. In the browser this is worse: the 0.2 KAS is already burned on mainnet, the user sees "error", and no proof file is produced with no recovery path (`src/api/kaspa.ts:663-738`).

### C6. Calendar server: orphaned stamps and double-anchoring

- **No crash recovery**: pending stamps are persisted (`main.rs:810`) but never reloaded on startup — the recovery hook `get_stamps_by_status` is `#[allow(dead_code)]` (`database.rs:253`). And graceful shutdown listens only for `ctrl_c` (`main.rs:719-723`) while systemd stops with SIGTERM, so **every `systemctl restart` hard-kills the process and permanently strands in-flight stamps** as `pending` rows that the cleanup task (confirmed-only, 30 days) never deletes. The drain path itself is a stub that logs and drops ready batches (`main.rs:1076-1085`).
- **Double-anchor on timeout**: if `wait_for_confirmation` times out *after the tx was accepted* (`kaspa_service.rs:686-689`), the stamps are requeued (`main.rs:1140-1153`) and a second commitment tx is built — fee paid twice, and the retry may fail anyway on stale cached UTXOs. The error path doesn't distinguish "submit failed" from "submitted but unconfirmed".
- **Serial head-of-line blocking**: one batch task processes all modes, and `submit_commitment` blocks it for up to 60 s per batch (+5 s sleep on failure) — the "instant" 100 ms mode's latency promise is meaningless under any load.
- **Non-atomic confirms**: status and proof are written in two separate UPDATEs with no transaction (`main.rs:1213-1222`); a crash between them yields a `confirmed` stamp with a pending proof.

### C7. Production config is self-contradictory: the 401 wall

`.env.production.template` sets `REQUIRE_API_KEY=true`, and the API-key middleware guards **all** `/v1` routes including reads and the WebSocket (`ktcs-calendar/src/main.rs:651-663`) — but the frontend never sends `X-API-Key` (no such header anywhere in `src/`), and browsers cannot set headers on WebSocket handshakes at all. Deploying the shipped compose stack per the template yields a public web app where every API call returns 401. The escape hatch is blocked too: `KTCS_ENVIRONMENT=production` *hard-fails startup* unless the key is required (`main.rs:311-327`). As shipped, **production mode and a working public frontend are mutually exclusive** — and the docs' claim that the key protects "write ops" only (`ktcs-calendar/README.md:97`) is wrong.

### C8. Private-key handling does not match its claims

- **Root README** (`README.md:213`): "Private keys use `secrecy::Secret<T>` with zeroization on drop." True only in ktcs-calendar. In **ktcs-core**, `KaspaWallet::Drop` zeroizes a *copy* of the secret bytes while the actual key inside `self.keypair` is never wiped, and also "zeroizes" the public address string (`wallet.rs:114-130`) — hygiene theater.
- **CLI**: keys live in plain `String`s with no zeroization; stdin entry doesn't disable terminal echo (`ktcs-cli/src/main.rs:533-559`); `wallet generate -o` **silently overwrites an existing key file** (`main.rs:1417,1426` — irreversible fund loss if it held a funded wallet's key) and writes with default 0644 permissions.
- **Browser**: docs claim "Keys handled in WASM, not JS heap" (`src/README.md:303`, `ktcs-wasm/README.md:381`) — false. The pasted mainnet key rests as an immutable JS string in React state and the global zustand store (`stores/stamp.ts:89`), readable by any XSS payload via `useStampStore.getState().walletKey`. Rust-side zeroization (`ktcs-wasm/src/lib.rs:569-675`, correctly done) cannot scrub the JS copies. It is at least never persisted (partialize excludes it) and the input avoids logging the key. Raw key entry is the *only* direct-mode path — no wallet-adapter integration.

---

## 4. High-Severity Findings

### Security / abuse

- **SSRF via proof files (CLI)**: `ktcs complete` GETs whatever `calendar_url` is embedded in the `.kts` (`main.rs:1290-1306`) — no scheme/host allowlist, so a malicious proof can point the CLI at internal endpoints. Proof-embedded strings are also printed raw (terminal ANSI/OSC injection, `main.rs:1031,1250,1296`).
- **Calendar responses are trusted**: both CLI confirm paths only check digest equality on the returned proof — `verify_proof` is never run on it (`main.rs:697-712, 1344-1364`); `complete` overwrites the input file by default even when the returned proof is not complete (warn-only, `main.rs:1358-1363`). The WASM `complete_proof` similarly bakes whatever attestation JSON the JS passes into the proof (`lib.rs:846-917`).
- **Rate-limit bypass (calendar)**: with `TRUST_PROXY=true`, the IP extractor trusts the *first* `X-Forwarded-For` entry (`main.rs:209-219`); nginx appends via `$proxy_add_x_forwarded_for`, so the first entry is attacker-controlled → trivial bypass plus unbounded governor-state memory growth from spoofed IPs.
- **WS connection cap inverted behind nginx**: per-IP limiting keys on the TCP peer (`websocket.rs:321-324`); behind the proxy every connection is 127.0.0.1, so `MAX_CONNECTIONS_PER_IP=50` becomes a **global 50-connection ceiling** — and no real per-client limit.
- **Proof-parsing DoS (core)**: limits allow 10,000 ops × 1 MB; `apply_operations` lets state grow to ~10 GB with O(n²) Prepend copying (`ops.rs:27,51`) — a legal `.kts` can pin CPU/memory; the calendar's `/v1/verify` accepts 10 MB of attacker proof bytes. The CLI has its own variant: a crafted `timestamp = u64::MAX` forces ~5.8×10⁸ iterations in the hand-rolled year loop (`main.rs:1593-1626`).
- **No HTTP timeouts in the CLI** (`reqwest::Client::new()`, `main.rs:641,1300`) — a hung calendar stalls `stamp`/`complete` indefinitely.

### Correctness

- **Script version-prefix stripping is guesswork, in two places**: core unconditionally treats the first 2 bytes of a returned UTXO script as an LE version (`kaspa.rs:1221-1227`, while `submit_transaction` encodes it BE at `kaspa.rs:1111` — both "right" only because version is 0); the frontend unconditionally `slice(4)`s the hex whether the RPC returned string or object form (`HomePage.tsx:150-162`). On endpoints returning unprefixed or object-form scripts, real script bytes are corrupted → wrong sighash → rejected signatures.
- **No known-answer vectors for consensus-critical crypto**: the hand-rolled Kaspa sighash (18-field keyed Blake2b, `wallet.rs:559-640`), address codec, and txid computation have zero fixtures against rusty-kaspa; a single field-order or endianness mistake surfaces only as on-chain rejection. Relatedly, `compute_kaspa_sighash` ignores its `sighash_type` argument — non-SIGHASH_ALL types produce silently wrong hashes (`wallet.rs:574-579`).
- **Fake txid in WASM**: `compute_transaction_id` (`ktcs-wasm/src/lib.rs:733-768`) is a single SHA-256 (comment claims double) over an ad-hoc serialization — real Kaspa txids are Blake2b over canonical encoding. The value never matches the network txid; the frontend stores it and later silently overwrites it with the node's answer.
- **`ktcs verify` exits 0 on invalid proofs** (`main.rs:999-1003, 1145`) — chain-check failures also don't affect exit status. `ktcs verify && …` in a script is unsafe. (README documents exit codes 0–5; only 0/1/2 exist.)
- **wRPC subscriptions never worked**: response routing keys on JSON-RPC `id` only, so notifications are dropped (`kaspa.rs:252-256, 1293-1296`); `unsubscribe_all` re-sends *subscribe* methods; `auto_reconnect`/`tls_verify` config fields are dead; `disconnect()` leaks the socket tasks.
- **Broken feature combination**: `cargo check --no-default-features --features kaspa-client` does not compile — `create_pending_stamp` is `keygen`-gated but called from `kaspa-client`-gated code (`direct.rs:88,161`).
- **Network confusion throughout**: nothing cross-checks wallet network vs client network vs `DagInfo`; `encode_address` uses any unknown string verbatim as address prefix (`wallet.rs:241-247`); the CLI's `stamp` defaults to `--network testnet` with a *mainnet-port* default RPC (`ws://localhost:16110`), while `wallet` commands default to mainnet; `decode_address` accepts mixed-case (bech32 forbids).
- **Frontend flow defects**: direct proofs are lost on reload (`confirmedProof` persists but the required `directBlockInfo` doesn't — `/proof/direct` shows "PROOF NOT FOUND" while the proof sits in localStorage, `ProofPage.tsx:31`); ProofPage polls a 404 forever (`ProofPage.tsx:120-128`); `setError`/`setConfirmedProof` cross-contaminate the calendar and direct state machines (`stamp.ts:236-248`); timestamps are rendered in local time with a hardcoded "UTC" suffix (`ConfirmationHero.tsx:36-43`); calendar-mode falls back to displaying the *block* hash labeled as the document's `sha256:` (`ProofPage.tsx:190`); the BTC-confirmations equivalence divides by 60,000 where ~6,000 is right — off ~10× (`blockchainVerify.ts:198-214`, mirrored in the CLI at `main.rs:1129`).
- **Merkle tree lacks domain separation**: `SHA256(l‖r)` with no leaf/node prefixes plus duplicate-last-leaf padding (`merkle.rs:117-119,219`) — the CVE-2012-2459 ambiguity pattern; avoidable in a brand-new format.
- **Stale DAG info in `/v1/verify`**: refreshed only once at connect (`kaspa_service.rs:506`), so "current confirmations" figures drift wrong over server uptime; `connected` flag is never unset on RPC failure.

---

## 5. Component Assessments

### 5.1 ktcs-core — solid format, untrustworthy edges

The format/ops/Merkle layers are clean and genuinely tested (61 fast unit tests: varint and serde round-trips, Merkle proofs, known hash vectors, sign/verify round-trips). The liabilities concentrate in (a) the verification semantics (§3 C1–C3), (b) the networking layer (`kaspa.rs`, 1,734 lines of hand-rolled wRPC with the defects above), and (c) **wholesale duplication**: `tx.rs` (747 lines) vs `tx_builder.rs` (686 lines) are ~90 %-identical transaction builders with *diverging* behavior — and production stamping uses the unsafe one (§3 C5). A vestigial alternative commitment scheme ("KTCS"+payload: `KTCS_COMMITMENT_PREFIX`, `create_commitment_payload`, `verify_kaspa_attestation`) suggests a mid-development pivot from tx-payload to burn-output, with both paths left half-alive. The `wasm` cargo feature is empty; `cfg(not(feature="kaspa-client"))` stubs inside gated modules are unreachable. The five `tests/*.rs` integration files (~2,100 lines) are live-network probe scripts, not tests — they panic on connectivity failure and one `--ignored` flow writes a plaintext mainnet key to the CWD (`mainnet_integration.rs:261-262`).

Hand-rolling everything (address codec, sighash, wRPC client) instead of depending on rusty-kaspa crates was a deliberate WASM-friendliness trade-off — and is the root cause of the blue-work bug, the missing test vectors, and the fee-model errors.

### 5.2 ktcs-cli — real direct path, facade calendar path, unwired config

A single 2,028-line `main.rs` containing config, CLI definitions, twelve handlers, a hand-rolled calendar client, and a hand-rolled date library. The direct-stamping and local-verify paths are genuinely wired to core. Beyond §3 C4 and §4: the **entire config system is decorative** — `Config::load()` is called only by `config show`/`config set`; `stamp`, `verify`, `complete`, and `wallet balance` all use hardcoded clap defaults, so `ktcs config set mainnet.rpc_url …` silently does nothing. Documented env vars (`KTCS_CALENDAR_URL`, `KTCS_RPC_URL`) are not implemented (only `NO_COLOR` is read). `--quiet` is parsed and never used; `thiserror` and `tracing` are dead dependencies. Direct-mode timeout writes a `direct://` pending URL that `complete` cannot process (unknown scheme). Duplication: the confirm logic, the node-connection block, and attestation pretty-printing each appear ~3×.

Tests: 130 exist as TEST_PLAN.md claims, but the plan's "all passing" is false — the 8 live tests pass `--rpc-url`/`--format json`/`--file` flags that don't exist (`cli_live_network.rs:79,167,393`), proving they have never run. The wiremock "calendar" test runs with `--async` so the mock is never contacted; the confirm path, base64 decode, and digest-mismatch check have zero test coverage — as does verify's exit code, which would have caught the exit-0 bug.

### 5.3 ktcs-calendar — the best-engineered crate, with operational blind spots

Clean routes/services separation, per-module `thiserror` enums, thorough startup validation with production guards, constant-time API-key compare via fixed 128-byte padded buffers + `subtle::ConstantTimeEq` (`main.rs:413-463`), `secrecy`-wrapped wallet keys with env-var removal after read, WS handlers with documented lock ordering and cancellation-token shutdown, and a real mock-mode workflow test. The critical gaps are lifecycle ones (§3 C6, C7; §4 proxy issues). Smaller items: the `batched` DB status is unreachable (the WS event fires but the status is never written — and fires *before* submission, so clients hear "batched" for batches that then fail); lagged WS receivers silently drop confirmation events; a stamp can confirm before the client's subscribe lands (100 ms instant window) with no replay; the global `WsState.subscriptions` map is maintained but never consulted for routing; `RUST_LOG` is silently ignored (subscriber built without `EnvFilter`, `main.rs:468-470`); the default `DATABASE_URL` (`sqlite:ktcs-calendar.db`, no `?mode=rwc`) cannot create its own file, so a fresh install without env config fails at startup; `/health` runs a `COUNT(*)` per request and nginx doesn't rate-limit it.

### 5.4 ktcs-wasm + frontend — real browser crypto, thin verification, drift-prone artifact

The WASM layer exposes the full pipeline (hash → nonce/commitment → tx build → **Schnorr sign in the browser** → RPC request string built in Rust to dodge JS u64 precision → proof assembly), and its internal key hygiene (zeroize-on-every-path with closure cleanup) is correct as far as Rust memory goes — undermined by the JS-side reality (§3 C8). The frontend is strictly typed (no `any` in app code), componentized, and the two state machines in `stores/stamp.ts` are legible, though HomePage's 115-line `handleDirectStamp` callback is untestable as written and drives `directStep` out of its nominal order.

The **supply-chain posture is the standout risk**: the 460 KB compiled `ktcs_wasm_bg.wasm` + generated JS are force-committed past their own `.gitignore` (`src/wasm/.gitignore` contains `*`), no npm script, Dockerfile step, or CI job runs `wasm-pack`, and the `VITE_WASM_HASH` integrity check is a stubbed `console.log` (`src/lib/wasm.ts:52-57`). All browser crypto therefore ships from an unverifiable binary that nothing rebuilds — changes to `ktcs-wasm` or `ktcs-core` silently do nothing to the site until someone manually rebuilds. Dead weight: react-query provider with zero queries, `d3` dependency with zero imports, unused `MetricCard`/`ThermodynamicGauge` components, unused store helpers, deprecated stubs, and a `VITE_KASPA_RPC` variable that is documented but never read. Dev-only but notable: `server.fs.allow: ['..']` exposes the whole repo parent through the Vite dev server.

### 5.5 Infrastructure, CI, and docs

**Two divergent deployment paths** — Docker (nginx `frontend.conf`, `/api/` prefix-strip, no TLS anywhere, port 80 only) and bare-metal (nginx `ktcs.conf`, same-origin `/v1/`, TLS + HSTS + CSP) — with **three incompatible `VITE_CALENDAR_URL` conventions** (`/api`, same-origin, and a subdomain in production.md), and the URL is baked at image build time. A frontend image built by the shipped compose/CI does not work behind `ktcs.conf`. The systemd unit is well-hardened (`ProtectSystem=strict`, dedicated user, `MemoryMax`) but stops the server with an unhandled SIGTERM (§3 C6), and production.md embeds a *different* unit and nginx config than the ones shipped in `deploy/`.

**CI** builds the three native crates, runs `cargo test --workspace --exclude ktcs-wasm`, and type-checks/builds the frontend. It has **no clippy, no fmt check, no eslint** (the `lint` script references eslint, which isn't even a dependency — `npm run lint` fails immediately), **never builds WASM**, never publishes images, and its cargo cache key hashes a gitignored `Cargo.lock` — i.e., always the same degenerate key. Because `Cargo.lock` is gitignored for a repo whose primary artifacts are binaries, no build (CI, Docker, or host) is reproducible. Worst: `ktcs-core`'s live-network test files carry zero `#[ignore]` attributes and are enabled by feature unification under `cargo test --workspace`, so **CI hits live public Kaspa nodes** — flaky by construction.

**Documentation drift is systemic.** Confirmed-accurate: the byte-level format docs, routes, batch windows, and the calendar's security mechanisms all match code. Confirmed-wrong: fictional CLI exit codes 0–5; unimplemented env vars; undocumented real commands (`status`, `config`, `completions`, `--async`, aliases); wrong `build_pending_proof` arity in the wasm README; **every crate README's cross-links point to files that don't exist** (`../docs/PROOF-FORMAT.md`, `API.md`, …); the root README links a missing spec file; **four different repository identities** across Cargo.toml (`example/ktcs`), mkdocs.yml (`aspect-build/ktcs`), installation.md (`3lemenoP/kasTime`), and production.md (`your-org/kasTime`); three conflicting Rust-version claims; dead config keys (`KTCS_INCLUDE_MAGIC` appears in zero source files; `PORT` is unused — code reads `BIND_ADDRESS`); three different rate-limit numbers depending on which file you read; a Prometheus example that scrapes JSON `/health` as `metrics_path` (can never work); `scripts/render-diagrams.mjs` imports a package not in package.json and describes an 8-byte header from a stale early design; and `src/README.md` documents pages, directories, and npm test scripts that don't exist. The CLI's default calendar `https://calendar.ktcs.kaspa.org` is a fictional domain — out of the box, `ktcs stamp` points at dead infrastructure (the live service is `kastime.xyz`).

---

## 6. Testing Posture (summary)

| Area | State |
|---|---|
| ktcs-core units | 61 genuine, fast tests — format/ops/Merkle/sign-verify well covered |
| ktcs-core integration | 5 live-network probe scripts masquerading as tests; no `#[ignore]`; CI-hostile |
| Consensus-critical crypto | **Zero known-answer vectors** (sighash, txid, addresses vs rusty-kaspa) |
| ktcs-cli | 130 tests; hermetic ones solid; the confirm path, exit codes, and `--chain` untested; live suite has never run (wrong flags); some config tests mutate the real user config dir on Windows |
| ktcs-calendar | Decent unit coverage incl. a full mock workflow; nothing exercises RPC failure, requeue, auth middleware, rate limiting, WS handshake, or crash recovery |
| Frontend | **Zero tests, no test runner** (docs claim `npm test` exists) |
| ktcs-wasm | Excluded from CI; never built in CI |
| Adversarial/fuzz | None (notable for a binary format parsing untrusted input) |

---

## 7. Prioritized Recommendations

**P0 — protocol integrity and funds:**
1. Make `verify_proof` bind the commitment to the attestation (embed the commitment tx or its script in the attestation, or at minimum require exact burn-script matching via `extract_commitment_from_output`), and label offline results "not chain-verified". (C1, C3)
2. Fix `select_utxos` target (`0` → burn + fee), switch `direct.rs` to the hardened `tx_builder`, delete the other builder, and replace Bitcoin mass/dust constants with KIP-9 values. (C5)
3. Pad odd-length `blueWork` hex before decoding, and make wrong-length hashes an error instead of `[0u8;32]`. (C5)
4. Fix confirmation tracking (walk more than one parent generation, or use virtual-chain/acceptance data), and in the calendar, distinguish "submit failed" from "submitted-but-unconfirmed" before requeueing. (C5, C6)

**P1 — operations and trust:**
5. Handle SIGTERM, reload pending stamps on startup, and wrap confirm writes in a DB transaction. (C6)
6. Resolve the 401 wall: exempt reads/WS from the API key, or add frontend key support (query-param for WS), and align the template/docs. (C7)
7. Either implement calendar submission in the CLI or remove the mode; make `verify` exit non-zero on invalid; run `verify_proof` on calendar responses; allowlist schemes/hosts in `complete`. (C4, §4)
8. Fix key handling to match claims: `secrecy` (or honest docs) in core, 0600 + no-overwrite + no-echo in the CLI, and either a wallet-adapter integration or a prominent risk warning in the browser. (C8)
9. Fix the XFF extractor (rightmost entry or `X-Real-IP`) and key WS limits on the forwarded client IP. (§4)

**P2 — engineering hygiene:**
10. Commit `Cargo.lock`; build WASM in CI and fail on artifact drift (or generate it at Docker build time and stop committing it); implement the `VITE_WASM_HASH` check; add clippy/fmt, install and configure eslint, and `#[ignore]` all live-network tests.
11. Add known-answer vectors for sighash/txid/address against rusty-kaspa fixtures.
12. Wire the CLI config system into the commands (or remove it), implement the documented env vars, and unify network/port defaults.
13. One documentation pass: fix cross-links, pick one repo identity, delete dead config keys from templates/docs, correct exit codes/arity/rate-limit numbers, and remove or implement the `0x05` Bitcoin attestation.

---

## 8. Closing Assessment

KTCS's architecture is sound and its central idea — sub-second proof-of-existence on a BlockDAG with DAA/blue-work-based security metrics — is well matched to Kaspa. The proof format is the strongest artifact in the repository: compact, spec-documented, and faithfully implemented. The calendar server shows the most engineering maturity; ktcs-core's format layer is solid while its networking and verification layers are not; the CLI and frontend both contain one fully-working path (direct stamping, local math verification) alongside facade or trust-dependent paths presented as equivalent.

The consistent failure mode across the codebase is **claims outrunning implementation**: trustlessness that trusts a single RPC node, zeroization that zeroizes copies, a config system that configures nothing, a test plan certifying suites that cannot parse their own arguments, and documentation describing a slightly different program. None of this is unusual for a hackathon deliverable — but the gap between the security narrative and the code is exactly where a timestamping service, whose entire product is trust, needs to close first.
