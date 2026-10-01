# kasTime vs rusty-kaspa v2.1.0: compatibility review

**Date:** 2026-10-01 · **kasTime:** `d2e71c9` (written 2026-01-23 → 2026-02-13) · **Upstream:** [kaspanet/rusty-kaspa](https://github.com/kaspanet/rusty-kaspa) `v2.1.0` (`01b532e8`, 2026-09-22)

## Summary

kasTime was built after Crescendo but **before rusty-kaspa v1.1.0 and the Toccata hard fork**. Toccata
(rusty-kaspa v2.0.0) activated on mainnet at DAA score **474,165,565 (about 2026-06-30)**, and v2.1.0 has
since removed the pre-activation code paths.

**Bottom line: as shipped, none of kasTime's on-chain write paths can get a transaction into a v2.x
mempool.** With the default and production-template settings, the calendar, `ktcs stamp --direct`,
browser direct stamping and the wallet recycler all pay fees **80–900× below** the new minimum relay fee.
Everything else on the wire still decodes, and signatures are still valid. So the minimum unblock is a
fee/mass fix, but several other problems sit behind it:

- **Every proof's parent hashes are empty.** kasTime reads a `parents` field that rusty-kaspa does not
  emit; the field has been called `parentsByLevel` since at least v1.0.1.
- **Proofs stop being verifiable after about 30 hours** on standard (pruned) nodes. This contradicts
  "anyone with a Kaspa node can verify any proof".
- **The 0.2 KAS burn output** now dominates the transaction's storage mass. That makes its effective
  feerate the lowest in the mempool, and wallets with small change get rejected outright. Native
  transaction payloads (available since Crescendo) avoid the burn entirely.
- **Confirmation detection only sees blocks that are tips at poll time.** In a 10-BPS simulation it
  missed between 0 and 18 of 20 confirmed transactions, depending erratically on RPC latency.
- **Defaults point at the wrong things:** the gRPC port instead of wRPC, a resolver protocol that doesn't
  match upstream's, and a retired testnet.

### What still works (verified against upstream code)

| Area | Result |
|---|---|
| v0 Schnorr sighash | **927/927** input sighashes identical to `calc_schnorr_signature_hash`, including non-empty payloads |
| Signatures | kasTime-signed inputs execute successfully in upstream `TxScriptEngine` with Toccata script-unit metering (`sigOpCount = 1` is enough) |
| v0 transactions | Still consensus-valid after Toccata (versions 0 and 1 are accepted) |
| `submitTransaction` JSON | Decodes with v2.1.0 types, from both `ktcs-core` and `ktcs-wasm`. The deprecated `mass` field is still accepted, and `computeBudget`/`covenant` are optional |
| Schnorr addresses | Byte-identical to `kaspa-addresses` |
| Block parsing | Blocks containing v1 / covenant transactions parse; blue work (48-hex) parses correctly |
| Covenant-bound UTXOs | A covenant-bound UTXO sent to the calendar's P2PK address is still spendable by kasTime's v0 signature, so this is not a griefing vector |

## Method

1. **Upstream reading.** I read `docs/toccata-guide.md`, mempool policy (`mining/src/mempool/*`), the mass
   calculator, transaction validation, sighash/txid hashing, the RPC model and the wRPC router, the resolver,
   network params, and the commit log from v1.0.0 to v2.1.0.
2. **Empirical harness** ([`tools/upstream-compat`](../tools/upstream-compat)). It compiles kasTime's actual
   code against upstream crates. It runs kasTime's real `KaspaClient` against a mock wRPC JSON node whose
   responses are serialized by `kaspa-rpc-core`. The node decodes requests with upstream types and admits
   transactions using upstream consensus code (script engine, mass calculator) and upstream error types.
   Only the admission ordering is re-implemented. The quoted rejection messages below are what a v2.1 node
   returns.
3. **Limitation.** Public Kaspa endpoints were unreachable from the review environment, so no live mainnet
   calls were made.

---

## Findings

Severity reflects production impact today, with Toccata active on mainnet.

### C1 · Critical: fees are 80–900× below the post-Toccata minimum, so every transaction is rejected

**Upstream:**
- The minimum relay fee is now **100 sompi/gram**: `DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE = 100_000`
  sompi/kg in `mining/src/mempool/config.rs:20` (#1004).
- It is charged on `max(compute_mass, normalized_transient_mass)`, where the normalized transient mass
  is 2 × transaction bytes on mainnet (`check_transaction_standard.rs:67-85`).
- `docs/toccata-guide.md` states the floor as `100 sompi * max(compute grams, 2 * transaction bytes)`.
- Nodes running v2.0.0+ enforced it on RPC submissions immediately. Since activation it also applies to
  P2P relay.
- This is mempool policy, not consensus, and it is not operator-configurable in rusty-kaspa. kaspad builds
  the mempool config with defaults and `relay_non_std_transactions = false` on every network
  (`kaspad/src/daemon.rs:648-656`). In practice a transaction below the floor never reaches a miner, and
  testnet-10 enforces the same floor, so C1 reproduces there.

**kasTime:**
- Mass is a hardcoded `10 + 148·inputs + 34·outputs`, in two copies: `ktcs-core/src/tx_builder.rs:285-302`
  and `ktcs-core/src/tx.rs:243-254`, plus `tx_builder.rs:539-552` / `tx.rs:550-556` for transfers.
- For a 1-in/2-out commitment that gives **226**; the real compute mass is **2,036**.
- The result is multiplied by `fee_per_gram`, which is 1 by default:
  - `DEFAULT_FEE_PER_GRAM` in `tx_builder.rs:30`
  - the `FEE_PER_GRAM` fallback in `ktcs-calendar/src/services/kaspa_service.rs:284-287`
- In other places it is 10:
  - `.env.production.template:70`
  - `DirectStampConfig::default()` in `ktcs-core/src/direct.rs:42`
  - the browser, hardcoded at `src/pages/HomePage.tsx:170`

**Evidence** (harness §D, upstream `MassCalculator` + mainnet params):

| Scenario | kasTime mass | Compute | 2×bytes | Storage | kasTime fee | Required | Short by |
|---|---:|---:|---:|---:|---:|---:|---:|
| Calendar default, `FEE_PER_GRAM=1`, 1 input | 226 | 2,036 | 632 | 50,083 | 226 | 203,600 | 901× |
| Calendar prod, `FEE_PER_GRAM=10`, 1 input | 226 | 2,036 | 632 | 50,083 | 2,260 | 203,600 | 90× |
| CLI / browser direct (fee rate 10), 1 input | 226 | 2,036 | 632 | 50,083 | 2,260 | 203,600 | 90× |
| Calendar prod, 5 inputs | 818 | 6,508 | 1,576 | 2,083 | 8,180 | 650,800 | 80× |
| Recycler 3-in/1-out, `FEE_PER_GRAM=10` | 488 | 3,860 | 1,000 | 0 | 4,880 | 386,000 | 79× |

Here is the node's response to kasTime's calendar submission path (harness §G, 4 UTXOs, `FEE_PER_GRAM=10`):

```
Rejected transaction 6404…ec76: transaction 6404…ec76 is not standard: transaction has 6700 fees
which is under the required amount of 539000 for compute mass 5390
```

**Impact:**
- **Calendar:** `batch_processing_loop` requeues the batch and retries every ~5 s (`ktcs-calendar/src/main.rs:1134-1156`). Stamps stay "batched" forever and no proofs are issued.
- **CLI and browser direct stamping:** both fail at submit.

**Fix:**
- **Fee rate:** fetch it with `getFeeEstimate`.
- **Mass:** compute it the way upstream does (`consensus/core/src/mass/mod.rs:335-370`):
  - compute = serialized size + 10 × Σ(2 + script length) over outputs + 1000 × Σ sigOpCount
  - transient = 4 × size, normalized ×0.5
  - storage per KIP-9
- **Fee:** `max(estimate × max(compute, 2·bytes, storage), 100 × max(compute, 2·bytes))`.
- **Stopgap (calendar only):** with the current estimator, `FEE_PER_GRAM=1000` clears the floor for any
  number of P2PK inputs, overpaying by roughly 10–35%. The harness shows it accepted end to end, and the config
  validator allows up to 10,000. The CLI (`DirectStampConfig::default()`) and the browser (hardcoded `10`)
  have no override, so they need code changes.

### H1 · High: the 0.2 KAS burn output makes kasTime the lowest-priority traffic and rejects small change

**Upstream rules:**
- **KIP-9 storage mass:** an output of value *v* costs about 10¹²/*v* grams.
- **Per-transaction storage limit:** 500,000 (`mining/src/mempool/check_transaction_limits.rs:36-43`,
  limits from `Params::block_mass_limits`).
- **Template priority:** `fee / max(compute, transient×0.5, storage)`
  (`consensus/core/src/tx.rs`, `calculated_feerate`).
- **Dust:** the old dust rule was removed in v1.0.1 (#698). Storage mass is now the only small-output
  control.

**kasTime:**
- The commitment output alone is about **50,000 storage grams**, so it dominates the transaction's
  priority mass.
- Change is only absorbed below **546 sompi** (Bitcoin's dust constant, in `kaspa_types.rs:10` and
  `tx_builder.rs:243-249`).

**Evidence** (harness §E). The fee is set to the new floor in each row:

| Input | Change | Storage mass | Upstream verdict | Feerate at floor fee |
|---:|---:|---:|---|---:|
| 0.21 KAS | 0.00998 KAS | 1,004,646 | **rejected:** storage mass 1004646 > 500000 | 0.20 sompi/g |
| 0.25 KAS | 0.04998 KAS | 210,090 | ok | 0.97 sompi/g |
| 0.30 KAS | 0.09998 KAS | 116,689 | ok | 1.74 sompi/g |
| 1 KAS | 0.79998 KAS | 52,500 | ok | 3.88 sompi/g |
| 10 KAS | 9.79998 KAS | 50,020 | ok | 4.07 sompi/g |

**Consequences:**
- **Priority:** a transaction paying exactly the floor ranks at about 1/25 the feerate of an ordinary
  transfer (≈100 sompi/g), so it is first to starve under congestion.
- **Wallets running low:** change between 546 sompi and about 0.02 KAS produces a transaction that can
  never be mined.

**Fix:**
- Prefer **O1** (a payload carrier), which removes both problems.
- Otherwise, make change handling storage-mass-aware: absorb change below about `10¹²/(450,000 + 10¹²/input)`,
  or add inputs.
- To keep priority, price the fee against the storage mass.

### H2 · High: proof parent hashes are always empty (field-name mismatch)

**Upstream:** the header JSON field is **`parentsByLevel`** (`rpc/core/src/model/header.rs:39`), and it has
had that name since at least v1.0.1.

**kasTime** deserializes `parents` with `#[serde(default)]`:
- `ktcs-core/src/kaspa.rs:397-398` and `:519-520`
- the TypeScript client reads `block.header.parents?.[0]` in `src/api/kaspa.ts:383/402` and `:449/466`

So the field is always silently empty.

**Evidence:** harness §G shows `parent_hashes(kasTime)=0` while the node's header had `parentsByLevel=[2,1]`. §I lists the exact header keys.

**Impact:**
- Every attestation produced by the calendar, CLI and browser has zero parent hashes, despite the docs
  presenting them as DAG context.
- The parent-walk fallback in `wait_for_confirmation` (`kaspa.rs:1474-1493`) never runs (see H4).

**Fix:** rename the field to `parents_by_level` / `parentsByLevel`, and decide how to treat proofs already
issued without parents.

### H3 · High: proof verification does not survive pruning (~30 h)

**Upstream:**
- Pruning depth at 10 BPS is 1,080,000 blocks, about 30 hours (`consensus/core/src/config/bps.rs:96-107`,
  `PRUNING_DURATION = 108_000 s`).
- Normal nodes keep only recent blocks. Older data needs `--archival` or `--retention-period-days`
  (`docs/archival.md`, #592).

**kasTime:** both verifiers fetch the attested block by hash:
- `verify_attestation_on_chain` → `get_block_with_transactions` (`ktcs-core/src/verify.rs:377`)
- `verifyOnBlockchain` (`src/lib/blockchainVerify.ts:111,133`)

On standard or public nodes, `ktcs verify --chain` and the web Verify page will fail for any proof older
than about a day, because the block (or at least its transactions) is no longer served. The web page
reports "Block not found". This contradicts README.md ("Anyone with a Kaspa node can independently verify
any proof") and `docs/concepts/architecture.md:40` ("Pruning-safe header chain").

**Fix:**
- **Now:** document the archival/retention requirement and point verification at an archival node or
  indexer.
- **Medium term:** embed the block header and the transaction's merkle path in the proof, so the proof is
  self-consistent offline.
- **Longer term:** see O4.

### H4 · High: confirmation detection polls DAG tips and misses confirmed transactions

**kasTime:** `wait_for_confirmation` in `ktcs-core/src/kaspa.rs:1400-1504` (TypeScript equivalent:
`src/api/kaspa.ts:663-738`).
- It polls `getBlockDagInfo` every 100 ms and fetches each unseen virtual parent.
- A block is examined only if it is a tip at the moment of a poll. At 10 BPS a block is a tip for a few
  hundred milliseconds.
- The parent walk is dead because of H2.

**Evidence** (harness §J, simplified model):
- blocks every 100 ms
- transaction included about 1 s after submit
- RTT jittered ×0.5–1.5
- 20 trials per cell

| Tip lifetime | RTT 20 ms | 50 ms | 80 ms | 120 ms | 160 ms |
|---|---:|---:|---:|---:|---:|
| 300 ms | 20/20 | 20/20 | **9/20** | **14/20** | **2/20** |
| 600 ms | 20/20 | 20/20 | 20/20 | **11/20** | 20/20 |

The misses are erratic, not monotonic in latency, because detection depends on how the poll cycle lines up
with block production. That unpredictability is itself the problem. On a remote public node (the
production default), these latencies are realistic.

**Related problems:**
- Inclusion is not acceptance. kasTime records the *first block found containing* the transaction. That
  can be a red or non-accepting block, and the transaction can appear in several blocks.
- On timeout, the calendar re-submits the batch as a **new** transaction (`main.rs:1134-1156`). If the
  first one did land, the root is anchored twice and another 0.2 KAS is burned.

**Fix:** use the virtual selected-parent chain. It is gap-free by construction:
- `getVirtualChainFromBlockV2`, available since v1.1.0 (#740), with `minConfirmationCount` (#667)
- or `subscribe` to `VirtualChainChanged` with `includeAcceptedTransactionIds`

Start from a chain block captured before submitting. Record the **accepting chain block** in the proof as
well as the containing block.

### H5 · High: wrong default ports, a resolver protocol that doesn't match upstream, and a retired testnet

**Ports.**
- Upstream wRPC JSON listens on **18110** (mainnet) and **18210** (testnet); 16110/16210 are **gRPC**
  (`consensus/core/src/network.rs:42-66`).
- kasTime defaults to `ws://localhost:16110` in all of these places:
  - `ktcs-core/src/kaspa.rs:35-38,65`
  - the CLI, at `ktcs-cli/src/main.rs:137,243,1506`
  - the calendar default config
  - `docs/deployment/production.md:76` (`ws://127.0.0.1:16210`)
- A WebSocket client cannot talk to the gRPC port. Also note that wRPC must be enabled explicitly
  (`--rpclisten-json`).

**Resolver.**
- Upstream resolves with an HTTP GET to
  `https://{node}.kaspa.{stream|red|green|blue}/v2/kaspa/{network}/{tls|any}/wrpc/{json|borsh}`, which
  returns `{uid, url}` (`rpc/wrpc/client/src/resolver.rs:126-150`, `Resolvers.toml`).
- `ktcs-core/src/resolver.rs:65-137` guesses `wss://pnn.kaspa.stream/...` paths and a `/json` node list,
  then falls back to a single hardcoded third-party endpoint. Each probe waits 5 s.
- The frontend uses `wss://resolver.kaspa.stream/wrpc/{network}` (`src/api/kaspa.ts:745-753`,
  `src/stores/stamp.ts:296-297`), which is not an upstream endpoint shape.
- I could not check from here whether `kaspa.aspectron.com` (the production default) is still up. It is
  not part of the upstream resolver set.

**Testnet.**
- v2.1.0 supports only **testnet-10** (`consensus/core/src/config/params.rs:567-575`). TN11 params were
  removed in v1.0.0 (Mar 2025) and TN12 in June 2026.
- kasTime defaults to testnet-11 in several places:
  - `KaspaClient::testnet()` (`kaspa.rs:179`)
  - `ktcs-calendar/.env.example`
  - the docs
  - the frontend testnet endpoint
- The Toccata guide recommends testing on testnet-10.

### M1 · Medium: P2SH addresses become an unspendable P2PK script, and ECDSA addresses are rejected

**Evidence** (harness §A):

| Address type | kasTime script | Upstream `pay_to_address_script` |
|---|---|---|
| P2SH | `20 <hash> ac`: P2PK with the hash as the key, so funds sent there are burned | `aa 20 <hash> 87` |
| ECDSA | error: "expected 32, got 33" | `21 <pubkey> ab` |

**Cause:**
- `decode_address` drops the version byte and requires 32 bytes for ECDSA (`ktcs-core/src/wallet.rs:321-355`).
- `address_to_script` then guesses the script type from the length (`tx_builder.rs:312-339`, `tx.rs:268-305`).
- Its 33-byte branch emits `OP_CHECKSIG` instead of `OP_CHECKSIGECDSA`.

**Exposure:**
- **Current call sites:** they only pass self-derived Schnorr addresses, so nothing is lost today.
- **Public API:** `build_address_script` is exported.
- **WASM:** `validate_address` returns `true` for P2SH.
- **Trend:** covenants (Toccata) make script-hash addresses more common.

**Fix:** return `(prefix, version, payload)` from the decoder and build scripts per version.

### M2 · Medium: RPC calls and fields that drifted from upstream

| kasTime | Upstream v2.1.0 | Effect |
|---|---|---|
| Sends `"mass": 0` (`kaspa.rs:1128`; `ktcs-wasm/src/lib.rs:936,1018`) | Renamed to `storageMass`. `mass` is accepted as a deprecated alias that is "to be removed" (`rpc/core/src/model/tx.rs:379-418`, #1039/#1043) | Works today; send `storageMass` |
| `getMempoolEntry` with `txId` (`kaspa.rs:1011`) | Field is `transactionId` (`message.rs:449-454`) | Decode error: "missing field `transactionId`" (harness §G) |
| `getTransaction` (`kaspa.rs:863`; `kaspa.ts:328`) | No such RPC: rusty-kaspa has no transaction index (`rpc/core/src/api/ops.rs`) | Always fails |
| `notifyBlockAdded`, `notifyVirtualSelectedParentChainChanged` (`kaspa.rs:1290,1340,1370-1374`); TS sends and listens for `notifyVirtualChainChanged` (`kaspa.ts:608-645`) | wRPC subscriptions go through a single `subscribe` method with a `Scope` (`rpc/wrpc/server/src/router.rs:84-92`). Notifications arrive as `blockAddedNotification`, `virtualChainChangedNotification`, … (`ops.rs:176-189`) | Never works; unused in production paths today |
| Reads `blueScore` from `getBlockDagInfo` (`kaspa.rs:768-770`) | No such field | `current_blue_score` is always 0 |
| `current_blue_work: [0u8; 32]` (`kaspa.rs:811`) | n/a | The calendar's "accumulated blue work" metric is always 0 (`kaspa_service.rs:806-833`) |
| TS reads `networkName` (`kaspa.ts:411-422`) | Field is `network` | `undefined` |
| `get_block_with_transactions` keeps the 2-byte version inside the script bytes (`kaspa.rs:703-711`) | `scriptPublicKey` is a hex string of `version (BE u16) ‖ script` | Script length 36, so `extract_commitment_from_output` returns `None`. Verification only works because it substring-scans |
| UTXO script version parsed little-endian (`kaspa.rs:1223`) | Encoded big-endian (`consensus/core/src/tx/script_public_key.rs:86-90`) | Harmless while version is 0 |

### M3 · Medium: the WASM transaction ID is wrong

- **kasTime:** `ktcs-wasm/src/lib.rs:733-768` hashes the signed transaction, *including signature
  scripts*, with SHA-256.
- **Upstream:** the v0 ID is keyed BLAKE2b (`TransactionID`) and excludes signature scripts
  (`consensus/core/src/hashing/tx.rs:34-47`). v1 transactions use yet another scheme (#849).
- **Evidence:** in harness §C the two IDs differ.
- **Impact:** the browser displays this ID before submission (`HomePage.tsx:181`). It is then replaced by
  the node's ID, so the effect is cosmetic until something relies on it.
- **Fix:** port `id_v0`, or drop the field.

### M4 · Medium: v0 is now a "deprecated compatibility path"

- **Status:** v0 transactions remain consensus-valid (`tx_validation_in_isolation.rs:140-145,192-215`).
  However, `consensus/core/src/mass/units.rs` describes v0 sigop-count inputs as "a deprecated
  compatibility path".
- **What v1 changes:**
  - a `compute_budget` per input instead of `sig_op_count`
  - a different sighash: no sigop hashes, plus covenant flags on outputs (`hashing/sighash.rs:245-279`)
  - a different txid
- **Recommendation:** plan the migration. kasTime's sighash code is v0-only.

### M5 · Medium: UTXO selection and wallet handling

These issues predate the upstream changes, but the new fees make them more expensive.

- **Direct stamping selects UTXOs with a target of 0** (`ktcs-core/src/direct.rs:103`). It picks only
  the largest UTXO and fails even when the balance is sufficient. In harness §G, 0.35 KAS spread over
  three UTXOs gave "Insufficient funds: have 15000000 sompi, need 20002340 sompi". It should target
  `COMMITMENT_BURN_AMOUNT` plus the fee.
- **The calendar spends every cached UTXO in every stamp** (`kaspa_service.rs:625-643`). At 100 sompi/g,
  each extra input costs about 0.0011 KAS. With recycling enabled, the STAMP wallet is drained every
  stamp until the recycler runs (every 30 s, ≥1 KAS).
- **Immature coinbase UTXOs are not filtered.** Coinbase maturity is 1,000 DAA at 10 BPS.
- **There are two diverging builders.** `tx.rs` and `tx_builder.rs` use different mass constants and
  dust behavior.

### Low / informational

- **L1. Sighash types other than ALL.** `SigHashType` exposes NONE, SINGLE and ANYONECANPAY, but
  `compute_kaspa_sighash` always hashes as ALL (`wallet.rs:574-580`). Any other type would produce
  invalid signatures. Either restrict the enum or implement the variants.
- **L2. Docs out of date:**
  - fee formula (`docs/PROOF_FORMAT_SPECIFICATION.md` §8.3)
  - `FEE_PER_GRAM` default of 1 (calendar and deployment docs)
  - "Kaspa addresses use Bech32m"; Kaspa uses a CashAddr-style checksum (`tx_builder.rs:306-311`)
  - wrong version-byte table: ECDSA is `0x01`, script hash is `0x08`
  - "Kaspa does NOT support OP_RETURN" (`ktcs-core/README.md:250`), which skips over native payloads
- **L3. "Provably unspendable" wording.** A 32-byte value used as an x-only key is unspendable only
  because nobody knows its discrete log; that is computational, not provable. This is fine in practice,
  and moot with O1.

---

## Opportunities from recent upstream features

- **O1. Put the commitment in the transaction payload** instead of a burn output.
  - **Availability:** native payloads have been enabled since Crescendo (#591), and the payload is
    committed in the sighash.
  - **Cost** (harness §F): a 1-in/1-out self-send with a 36-byte `"KTCS"‖commitment` payload has compute
    mass 1,660, a minimum fee of **0.00166 KAS** and storage mass of about 30. Compare 0.2 KAS burned plus
    a 0.002 KAS fee today.
  - **Side benefits:** no net UTXO-set growth (one input is replaced by one change output) and normal
    priority.
  - **Verification:** kasTime's verifier already understands `"KTCS"`-prefixed payloads
    (`ktcs-core/src/verify.rs:230-263`).
- **O2. `getFeeEstimate`.** It returns priority, normal and low buckets in sompi/gram, and is the
  supported way to set fees (Toccata guide, "Fee Adaptation").
- **O3. `getVirtualChainFromBlockV2`** (v1.1.0). It returns added chain blocks with accepted
  transactions, including payloads at higher verbosity. That covers both confirmation tracking (H4) and
  verification within the retention window.
- **O4. KIP-21 lane proofs** (`GetSeqCommitLaneProof`, v2.0.1). These are self-contained witnesses
  verifiable against a header's `seq_commit` (= `accepted_id_merkle_root`). Worth evaluating for compact,
  header-anchored attestations that are less dependent on archival nodes (H3). This needs design work and
  was **not** verified here.

## Recommended order of work

1. **Unblock submissions:** C1 (fee and mass), then H1. Moving to payloads (O1) removes H1 and most of
   the cost.
2. **Proof correctness:** H2 (`parentsByLevel`), then H4 (VSPC-based acceptance tracking; record the
   accepting chain block).
3. **Connectivity:** H5 (ports 18110/18210, real resolver protocol, testnet-10).
4. **Hygiene:**
   - M1: address scripts per version
   - M2: `storageMass`, `transactionId`, remove `getTransaction` / `notify*`, `network`
   - M3: WASM transaction ID
   - M5: UTXO selection
5. **Product decisions:** H3 (long-term verifiability story) and M4 (plan v1 transactions).

After each step, re-run `tools/upstream-compat` to confirm against upstream.

## Appendix: upstream changes relevant to kasTime (v1.0.0 → v2.1.0)

| Release | Date | Relevant changes |
|---|---|---|
| v1.0.0 | 2025-03-31 | Crescendo (activated at DAA 110,165,000, 2025-05-05). Introduced: <br>• 10 BPS <br>• KIP-9 storage mass <br>• KIP-10 introspection <br>• native payloads <br>• KIP-13 transient mass <br>• KIP-15 sequencing commitment <br>• `mass` committed in transactions <br>Also removed TN11 params. |
| v1.0.1 | 2025-06-30 | Temporary dust-prevention rule removed (#698) |
| v1.1.0 | 2026-03-04 | <br>• VSPC v2 / enhanced virtual-chain data (#740) <br>• `minConfirmationCount` (#667) <br>• compressed P2P headers (#777) <br>• pruning-proof work |
| v2.0.0 | 2026-06-05 | Toccata: KIP-16 (ZK opcodes), KIP-17 (covenant opcodes), KIP-20 (covenant IDs), KIP-21 (lanes / seq-commit). Changes: <br>• tx v1 with new txid (#849) <br>• `compute_budget` (#884) <br>• multidimensional mass with transient limit 1,000,000 (#868, #995) <br>• **min relay fee 100 sompi/g** (#1004) <br>• `mass` → `storageMass` (#1039, #1043) <br>• header version bump (#998) <br>• `GetBlockRewardInfo` <br>Mainnet activation at DAA 474,165,565. |
| v2.0.1 | 2026-06-15 | `GetSeqCommitLaneProof` (#961); RPC/sync fixes |
| v2.1.0 | 2026-09-22 | Toccata cleanup (activation code removed, #1082–#1101); arithmetic safety (#1138); coinbase txs explicitly rejected from the mempool (#1137) |
