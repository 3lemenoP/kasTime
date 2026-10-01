# upstream-compat

Standalone harness that checks kasTime against **upstream rusty-kaspa** (pinned to `v2.1.0`).
It compiles kasTime's real code (`ktcs-core`, `ktcs-wasm`) together with upstream crates and
reports where they disagree. It backs the findings in
[`reviews/2026-10-01-rusty-kaspa-v2.1-compatibility.md`](../../reviews/2026-10-01-rusty-kaspa-v2.1-compatibility.md).

It is deliberately **not** a member of the kasTime workspace, so it never affects CI or Docker builds.

## Run

```bash
cd tools/upstream-compat
CARGO_NET_GIT_FETCH_WITH_CLI=true cargo run          # sections A–I (~1 min after the first build)
RUN_SIM=1 cargo run                                  # also run the 10-BPS confirmation simulation (slow)
```

The first build fetches rusty-kaspa from GitHub and compiles its txscript dependencies
(risc0/arkworks), which takes several minutes.

## What it checks

| Section | Check | Upstream code used |
|---|---|---|
| A | Address encoding (Schnorr / P2SH / ECDSA) | `kaspa-addresses`, `kaspa_txscript::pay_to_address_script` |
| B | v0 sighash equivalence on 300 random transactions | `calc_schnorr_signature_hash` |
| C | kasTime-signed transaction executes in the script engine; WASM transaction ID | `TxScriptEngine` (Toccata metering), `Transaction::id` |
| D | kasTime fees vs post-Toccata minimum relay fee | `MassCalculator`, mainnet `Params` |
| E | Storage mass and effective priority of the 0.2 KAS burn design | KIP-9 storage mass, block mass limits |
| F | Commitment in a native transaction payload instead of a burn output | same as D/E |
| G | End to end: kasTime's `KaspaClient` against a mock wRPC JSON node | `kaspa-rpc-core` serde types, upstream error types |
| H | Blocks containing v1 / covenant transactions; covenant-bound UTXOs | `CovenantBinding`, `CovenantsContext` |
| I | Exact JSON field names emitted by upstream | `kaspa-rpc-core` |
| J | Confirmation detection rate at 10 BPS (`RUN_SIM=1`) | mock node only |

## Limitations

- The mock node's admission order (scripts, then per-transaction mass limits, then the standard fee floor)
  is re-implemented. The constants, mass calculator, script engine and error types are upstream code.
- No live network calls are made. Section J models block production and latency; its numbers show the
  failure mode, not mainnet rates.
