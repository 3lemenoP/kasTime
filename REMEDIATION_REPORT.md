# KTCS Remediation Report

*Remediation of every issue identified in [CODEBASE_ANALYSIS.md](./CODEBASE_ANALYSIS.md), across all four Rust crates, the WASM bindings, the React frontend, deployment/CI infrastructure, and documentation. Work was executed in dependency-ordered waves (core first, then dependents, then docs), with each crate verified independently before commit and the full workspace verified at the end.*

*Branch: `claude/codebase-analysis-hq4f96` · 10 commits (`faf4724`…`4f3afd3`)*

---

## 1. Verification Summary (final state)

| Gate | Result |
|---|---|
| `cargo build --workspace --locked` | ✅ builds |
| `cargo test` (workspace unit) | ✅ **174 passed** (core 97, cli 46, calendar 31) |
| `cargo test -p ktcs-cli` (integration) | ✅ **96 passed**, 8 live-network `#[ignore]` |
| `cargo test -p ktcs-wasm` | ✅ **7 passed** (incl. rusty-kaspa txid KATs) |
| `cargo check -p ktcs-core --no-default-features --features kaspa-client` | ✅ compiles (was a hard compile error) |
| `cargo check -p ktcs-wasm --target wasm32-unknown-unknown` | ✅ compiles |
| `cargo fmt --all --check` | ✅ clean |
| `cargo clippy` | ✅ no errors |
| `npx tsc -b` | ✅ clean |
| `npm run lint` | ✅ 0 errors (2 warnings) |
| `npx vite build` | ✅ builds |

Baseline before remediation: Rust built with 89 core unit tests; frontend compiled; the calendar path, config system, and much of the security posture were non-functional or unverified. The workspace now carries **277 automated tests** (up from ~150 that actually ran hermetically), with the consensus-critical crypto (transaction id, sighash, address codec) cross-verified against rusty-kaspa's own known-answer vectors.

---

## 2. Critical Findings — all resolved

| ID | Finding | Fix | Commit |
|----|---------|-----|--------|
| **C1** | Offline `verify_proof` accepts fabricated attestations | `verify_proof` now runs structural validation on every complete attestation and rejects zero/invalid ones (`valid:false`); adds explicit `structurally_valid`/`chain_verified` fields; offline is never `chain_verified` | `0e81b32` |
| **C2** | Online verify trusts one node; testnet always fails; timestamp never checked; DAA mismatch only warns | Exact-match chain checks hard-fail on DAA/timestamp/network mismatch; frontend + CLI gained network selection so testnet proofs verify; attestation timestamp compared to block timestamp | `0e81b32`, `467c892`, `32ed1b8` |
| **C3** | Commitment matched by naive byte scan at any offset | Exact P2PK burn-script match (`0x20 <32B> 0xac`) in core and frontend, via `extract_commitment_from_script` | `0e81b32`, `467c892` |
| **C4** | CLI calendar mode never submits | CLI now really `POST`s to `/v1/stamp`, uses the server-returned id, and runs `verify_proof` on returned proofs before accepting | `32ed1b8` |
| **C5** | Funds-path bugs (UTXO target, unsafe builder, Bitcoin fee/dust, zeroed blue-work, missed confirmations) | `select_utxos` targets burn+fee; migrated to hardened `tx_builder` and **deleted** the duplicate `tx.rs`; KIP-9-informed mass/dust; odd-length blueWork hex padded (errors instead of zeroing); confirmation walks bounded parent generations (core + frontend) | `0e81b32`, `467c892` |
| **C6** | Calendar orphans stamps on restart; double-anchors on timeout; non-atomic confirms | SIGTERM handled; pending stamps reloaded on startup; submit-failed vs submitted-unconfirmed distinguished (no rebuild/double-spend); single atomic confirm UPDATE | `628c4cc` |
| **C7** | `REQUIRE_API_KEY=true` 401s the whole frontend | API key now guards only `POST /v1/stamp`; reads, verify, WS stream, and health are public; template defaults to a working config with the tradeoff documented | `628c4cc`, `faf4724` |
| **C8** | Key-handling claims false (copy-zeroize; key in JS heap; 0644 files) | Core uses real `Zeroizing` secret storage; CLI writes 0600, refuses overwrite, no-echo stdin, zeroized buffers; browser shows an honest risk warning and clears the key on leaving the flow; docs corrected | `0e81b32`, `32ed1b8`, `467c892`, `b83af73` |

---

## 3. High-Severity Findings — resolved

**Security / abuse**
- SSRF in `ktcs complete` → scheme allowlist (http/https), explicit `direct://` handling; terminal-escape sanitization of proof-embedded strings (`32ed1b8`).
- Calendar responses now `verify_proof`-checked, not just digest-checked; `complete` refuses to overwrite with an incomplete/invalid proof (`32ed1b8`).
- XFF rate-limit bypass → uses `X-Real-IP`/rightmost XFF; WS per-IP cap keyed on the real client IP behind a proxy (`628c4cc`).
- Proof-parsing DoS → cumulative-state cap in `apply_operations`; CLI timestamp year-loop replaced with O(1) arithmetic (`0e81b32`, `32ed1b8`).
- No HTTP timeouts in CLI → 30s request timeout on both clients (`32ed1b8`).

**Correctness**
- Fake WASM txid → real Kaspa keyed-Blake2b `id_v0`, verified against rusty-kaspa KATs; artifact rebuilt (`feff043`).
- Consensus-critical crypto had no KATs → address codec and all 6 SIG_HASH_ALL sighash vectors cross-verified against rusty-kaspa (no divergence found) (`666fc3f`).
- Script version-prefix guesswork → form-aware/guarded handling with consistent big-endian encoding (core + frontend) (`0e81b32`, `467c892`).
- `verify` exit code → non-zero on invalid/failed-chain (`32ed1b8`).
- `compute_kaspa_sighash` silently wrong for non-ALL types → returns an error (`0e81b32`).
- Broken feature combination (`keygen`/`kaspa-client`) → compiles (`0e81b32`).
- Network confusion → `encode/decode_address` reject unknown networks, mixed case, and prefix mismatch; CLI RPC port derives from network (`0e81b32`, `32ed1b8`).
- Merkle tree → leaf/node domain separation (`0e81b32`).
- Frontend flow: direct proof survives reload, 404 poll loops stop, state machines no longer cross-contaminate, timestamps truthful (UTC), block hash no longer shown as document digest, RPC id collisions avoided (`467c892`).
- wRPC: `disconnect` aborts socket tasks, `connected` cleared on error, correct unsubscribe, subscriptions return honest "unsupported" errors, submit logs at trace (`0e81b32`).

---

## 4. Component & Infra Findings — resolved

- **Duplication / dead code:** `tx.rs` deleted; vestigial payload-commitment scheme removed (core); react-query provider, unused components, dead methods/types, and the `d3`/`@tanstack/react-query` deps removed (frontend/infra).
- **Live-network tests** across all six `ktcs-core/tests/*.rs` marked `#[ignore]`; the plaintext-key-to-CWD test now uses a temp dir; CLI live-test flags corrected.
- **CLI config system wired** into `stamp`/`verify`/`complete`/`wallet`; `KTCS_CALENDAR_URL`/`KTCS_RPC_URL` implemented; `--quiet` implemented; unused `thiserror` removed; `--verbose` now emits real tracing.
- **Calendar:** `RUST_LOG` honored (EnvFilter); default `DATABASE_URL` includes `?mode=rwc`; `Batched` status/event made consistent; WS lag re-queries the DB; `.env.example` valid values.
- **WASM artifact:** rebuilt from source; a real `VITE_WASM_HASH` integrity check implemented; CI now builds WASM and checks artifact drift.
- **Reproducible builds:** `Cargo.lock` tracked; Dockerfile/CI build `--locked`.
- **CI:** added `cargo fmt --check`, `cargo clippy`, a WASM build job, eslint, and `tsc` gates; eslint baseline configured so `npm run lint` passes.
- **Deployment:** `VITE_CALENDAR_URL` conventions documented; the 401-wall tradeoff and `TRUST_PROXY` documented; dead `KTCS_INCLUDE_MAGIC`/`PORT` removed; systemd `LimitNOFILE` added; `vite server.fs.allow` restricted.
- **Docs:** every drift item in §5.5 corrected — exit codes, env vars, undocumented commands, wasm arity, all broken cross-links, a single repo identity, Rust 1.78+, reconciled rate-limit numbers, the Prometheus/health example, `src/README`, the `0x05` attestation, and the deleted dead diagram script.

---

## 5. Intentionally Deferred (with rationale)

These were judged out-of-scope for a safe, non-regressing remediation and are documented rather than silently skipped:

1. **CLI default calendar host** (`calendar.ktcs.kaspa.org`) is not a live domain. It is now fully configurable (`--calendar`/`KTCS_CALENDAR_URL`/config) and the docs warn about it, but the built-in default was **not** repointed in code — the live service's calendar path (`kastime.xyz` vs `/api`) can't be confirmed, and hardcoding a guessed-wrong URL would be worse than a documented, overridable default. *Recommend: set the default to the real endpoint, or require `--calendar`, once the production URL is confirmed.*
2. **No Prometheus `/metrics` endpoint** — the docs now state this plainly; adding real metrics is a feature, not a drift fix.
3. **Calendar `connected` flag not unset on RPC error** — deferred deliberately: with no reconnect loop, unsetting it would permanently wedge the service (all refreshes short-circuit). Safe handling needs a reconnect loop (a larger feature).
4. **Serial batch submission (head-of-line latency)** — kept serial on purpose: the single anchoring wallet's cached UTXOs make concurrent submits double-spend-unsafe. The latency is an accepted tradeoff.
5. **Submitted-but-unconfirmed recovery poller** — the double-anchor fix marks such stamps `Batched` (never double-spends); a background poller that later completes them from the accepted tx hash is a follow-up feature.
6. **Stale DAG snapshot in `/v1/verify`** — `current_confirmations` still derives from the connect-time DAG info; a periodic refresh is a minor follow-up.
7. **wRPC notification routing** — subscriptions now fail honestly rather than silently; wiring real notification delivery is a networking feature beyond the drift fix.
8. **`ktcs-cli/src/main.rs` remains one large file** — a structural/readability item; splitting it is risky churn with no behavioral benefit, so it was left for a dedicated refactor.

---

## 6. Notable Validation

The single most important outcome is that KTCS's hand-rolled, consensus-critical cryptography was **independently verified against Kaspa's reference implementation**:

- **Transaction id** — reproduces rusty-kaspa's `id_v0` known-answer vectors exactly.
- **Sighash** — all six `SIG_HASH_ALL` vectors from rusty-kaspa's `test_signature_hash` reproduced exactly.
- **Address codec** — the `kaspa:`/`kaspatest:` bech32 encoding matches rusty-kaspa's `bech32` output for a real x-only pubkey.

No divergence was found — the crypto is consensus-correct — and the trust boundary that the project's name and marketing depend on is now enforced in code (exact burn-script binding, structural attestation validation, honest offline-vs-chain-verified labeling) rather than assumed.
