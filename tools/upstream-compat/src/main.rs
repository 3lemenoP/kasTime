//! Empirical compatibility check: kasTime (ktcs-core / ktcs-wasm) vs upstream rusty-kaspa v2.1.0.
//!
//! Everything "upstream" here is computed by upstream crates (consensus-core, txscript, rpc-core,
//! mining-errors) built from kaspanet/rusty-kaspa@01b532e8. The only re-implemented upstream logic is
//! the mempool policy *glue* (which check runs when), using the constants and error types read from:
//!   mining/src/mempool/config.rs                 DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE = 100_000 sompi/kg
//!   mining/src/mempool/check_transaction_standard.rs  fee_mass = max(compute, normalized_transient)
//!   mining/src/mempool/check_transaction_limits.rs    per-tx compute/transient/storage <= block limits

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::{
    config::params::MAINNET_PARAMS,
    hashing::{
        sighash::{calc_schnorr_signature_hash, SigHashReusedValuesUnsync},
        sighash_type::SIG_HASH_ALL,
    },
    mass::{ContextualMasses, Mass, MassCalculator},
    network::{NetworkId, NetworkType},
    subnets::SubnetworkId,
    tx::{
        PopulatedTransaction, ScriptPublicKey as UScriptPublicKey, Transaction as UTx,
        TransactionInput as UTxIn, TransactionOutpoint, TransactionOutput as UTxOut, UtxoEntry,
        VerifiableTransaction,
    },
};
use kaspa_hashes::Hash;
use kaspa_math::Uint192;
use kaspa_mining_errors::mempool::{NonStandardError, RuleError};
use kaspa_rpc_core::{
    GetBlockDagInfoResponse, GetBlockResponse, GetMempoolEntryRequest, GetUtxosByAddressesResponse,
    RpcBlock, RpcBlockVerboseData, RpcError, RpcHeader, RpcTransaction, RpcTransactionOutpoint,
    RpcTransactionVerboseData, RpcUtxoEntry, RpcUtxosByAddressesEntry, SubmitTransactionRequest,
    SubmitTransactionResponse,
};
use kaspa_txscript::{
    caches::Cache, pay_to_address_script, EngineCtx, EngineFlags, TxScriptEngine,
};

use ktcs_core::kaspa_types::{
    ScriptPublicKey as KSpk, Transaction as KTx, TransactionInput as KTxIn,
    TransactionOutput as KTxOut, Utxo,
};
use ktcs_core::{
    build_address_script, compute_kaspa_sighash, decode_address, prepare_direct_stamp,
    sign_transaction, verify_attestation_on_chain, DirectStampConfig, KaspaAttestation,
    KaspaClient, KaspaClientConfig, KaspaWallet, SigHashType, SighashInput, SighashOutput,
    SighashTransaction, TransactionBuilder, TransferTransactionBuilder,
};

const SOMPI: u64 = 100_000_000;
/// mining/src/mempool/config.rs: DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE (sompi per kg of mass)
const MIN_RELAY_FEE_PER_KG: u64 = 100_000;
const PRIV: [u8; 32] = [0x11; 32];

// ---------------------------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------------------------

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn bytes32(&mut self) -> [u8; 32] {
        let mut b = [0u8; 32];
        for c in b.chunks_mut(8) {
            c.copy_from_slice(&self.next().to_le_bytes());
        }
        b
    }
    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.next() % (hi - lo + 1)
    }
}

fn p2pk_script(pubkey: &[u8; 32]) -> Vec<u8> {
    let mut s = vec![0x20];
    s.extend_from_slice(pubkey);
    s.push(0xac);
    s
}

fn utxo(txid: [u8; 32], index: u32, amount: u64, script: Vec<u8>) -> Utxo {
    Utxo {
        transaction_id: txid,
        index,
        amount,
        script_public_key: KSpk { version: 0, script },
        block_daa_score: 1_000,
        is_coinbase: false,
    }
}

/// kasTime tx -> upstream consensus tx, mirroring exactly what kasTime puts on the wire
/// (sequence = u64::MAX, sigOpCount = 1 per input; see ktcs-core/src/kaspa.rs submit_transaction)
fn to_upstream(tx: &KTx) -> UTx {
    let inputs = tx
        .inputs
        .iter()
        .map(|i| {
            UTxIn::new(
                TransactionOutpoint::new(
                    Hash::from_bytes(i.previous_outpoint_hash),
                    i.previous_outpoint_index,
                ),
                i.signature_script.clone(),
                u64::MAX,
                1,
            )
        })
        .collect();
    let outputs = tx
        .outputs
        .iter()
        .map(|o| {
            UTxOut::new(
                o.amount,
                UScriptPublicKey::from_vec(
                    o.script_public_key.version,
                    o.script_public_key.script.clone(),
                ),
            )
        })
        .collect();
    UTx::new(
        tx.version,
        inputs,
        outputs,
        tx.lock_time,
        SubnetworkId::from_bytes(tx.subnetwork_id),
        tx.gas,
        tx.payload.clone(),
    )
}

fn entry_of(u: &Utxo) -> UtxoEntry {
    UtxoEntry::new(
        u.amount,
        UScriptPublicKey::from_vec(
            u.script_public_key.version,
            u.script_public_key.script.clone(),
        ),
        u.block_daa_score,
        u.is_coinbase,
        None,
    )
}

/// Exactly mirrors consensus `check_scripts_sequential` (tx_validation_in_utxo_context.rs)
fn validate_scripts(tx: &UTx, entries: Vec<UtxoEntry>) -> Result<(), String> {
    let populated = PopulatedTransaction::new(tx, entries);
    let sig_cache = Cache::new(10_000);
    let reused = SigHashReusedValuesUnsync::new();
    for (i, (input, entry)) in populated.populated_inputs().enumerate() {
        let limit = input.compute_commit.allowed_script_units();
        let mut vm = TxScriptEngine::from_transaction_input_with_script_units_limit(
            &populated,
            input,
            i,
            entry,
            EngineCtx::new(&sig_cache).with_reused(&reused),
            EngineFlags::default(),
            limit,
        );
        vm.execute().map_err(|e| format!("input {i}: {e}"))?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct Masses {
    #[allow(dead_code)] // reported through Debug in the mock node's log
    size: u64,
    compute: u64,
    transient: u64,
    norm_transient: u64,
    storage: Option<u64>,
    min_fee: u64,
    priority_mass: u64,
}

fn min_relay_fee(mass: u64) -> u64 {
    let f = mass.saturating_mul(MIN_RELAY_FEE_PER_KG) / 1000;
    if f == 0 {
        MIN_RELAY_FEE_PER_KG
    } else {
        f
    }
}

fn masses(tx: &UTx, entries: &[UtxoEntry]) -> Masses {
    let mc = MassCalculator::new_with_consensus_params(&MAINNET_PARAMS);
    let nc = mc.calc_non_contextual_masses(tx);
    let populated = PopulatedTransaction::new(tx, entries.to_vec());
    let storage = mc
        .calc_contextual_masses(&populated)
        .map(|c| c.storage_mass);
    let cof = MAINNET_PARAMS.block_mass_limits.cofactors();
    let nt = nc.normalized_transient(&cof);
    let priority_mass =
        Mass::new(nc, ContextualMasses::new(storage.unwrap_or(u64::MAX))).normalized_max(&cof);
    Masses {
        size: kaspa_consensus_core::mass::transaction_estimated_serialized_size(tx),
        compute: nc.compute_mass,
        transient: nc.transient_mass,
        norm_transient: nt,
        storage,
        min_fee: min_relay_fee(nc.compute_mass.max(nt)),
        priority_mass,
    }
}

/// Upstream mempool admission (policy glue re-implemented; constants/errors upstream):
/// script validity -> per-tx limits -> standard fee floor. Returns the exact RPC error text a v2.1 node returns.
fn mempool_verdict(tx: &UTx, entries: &[UtxoEntry], fee: u64) -> Result<Masses, String> {
    let id = tx.id();
    validate_scripts(tx, entries.to_vec()).map_err(|e| format!("script validation failed: {e}"))?;
    let m = masses(tx, entries);
    let lim = MAINNET_PARAMS.block_mass_limits;
    let reject = |r: RuleError| RpcError::RejectedTransaction(id, r.to_string()).to_string();
    if m.compute > lim.compute {
        return Err(reject(RuleError::RejectComputeMass(
            id,
            m.compute,
            lim.compute,
        )));
    }
    if m.transient > lim.transient {
        return Err(reject(RuleError::RejectTransientMass(
            id,
            m.transient,
            lim.transient,
        )));
    }
    let storage = m.storage.unwrap_or(u64::MAX);
    if storage > lim.storage {
        return Err(reject(RuleError::RejectStorageMass(
            id,
            storage,
            lim.storage,
        )));
    }
    if fee < m.min_fee {
        let nse = if m.compute >= m.norm_transient {
            NonStandardError::RejectInsufficientComputeFee(id, fee, m.min_fee, m.compute)
        } else {
            NonStandardError::RejectInsufficientTransientFee(id, fee, m.min_fee, m.norm_transient)
        };
        return Err(reject(RuleError::from(nse)));
    }
    Ok(m)
}

fn ksighash_tx(tx: &KTx, utxos: &[Utxo]) -> SighashTransaction {
    SighashTransaction {
        version: tx.version,
        inputs: tx
            .inputs
            .iter()
            .zip(utxos)
            .map(|(i, u)| SighashInput {
                previous_outpoint_hash: i.previous_outpoint_hash,
                previous_outpoint_index: i.previous_outpoint_index,
                script_public_key_version: u.script_public_key.version,
                script_public_key: u.script_public_key.script.clone(),
                value: u.amount,
                sequence: u64::MAX,
                sig_op_count: 1,
            })
            .collect(),
        outputs: tx
            .outputs
            .iter()
            .map(|o| SighashOutput {
                value: o.amount,
                script_public_key_version: o.script_public_key.version,
                script_public_key: o.script_public_key.script.clone(),
            })
            .collect(),
        lock_time: tx.lock_time,
        subnetwork_id: tx.subnetwork_id,
        gas: tx.gas,
        payload: tx.payload.clone(),
    }
}

/// Verbatim copy of the private `compute_transaction_id` in ktcs-wasm/src/lib.rs (used for the
/// `transaction_id` returned by the WASM `sign_transaction`)
fn ktcs_wasm_compute_transaction_id(tx: &KTx) -> [u8; 32] {
    let mut data = Vec::new();
    data.extend_from_slice(&tx.version.to_le_bytes());
    data.extend_from_slice(&(tx.inputs.len() as u64).to_le_bytes());
    for input in &tx.inputs {
        data.extend_from_slice(&input.previous_outpoint_hash);
        data.extend_from_slice(&input.previous_outpoint_index.to_le_bytes());
        data.extend_from_slice(&(input.signature_script.len() as u64).to_le_bytes());
        data.extend_from_slice(&input.signature_script);
    }
    data.extend_from_slice(&(tx.outputs.len() as u64).to_le_bytes());
    for output in &tx.outputs {
        data.extend_from_slice(&output.amount.to_le_bytes());
        data.extend_from_slice(&output.script_public_key.version.to_le_bytes());
        data.extend_from_slice(&(output.script_public_key.script.len() as u64).to_le_bytes());
        data.extend_from_slice(&output.script_public_key.script);
    }
    data.extend_from_slice(&tx.lock_time.to_le_bytes());
    data.extend_from_slice(&tx.subnetwork_id);
    data.extend_from_slice(&tx.gas.to_le_bytes());
    data.extend_from_slice(&(tx.payload.len() as u64).to_le_bytes());
    data.extend_from_slice(&tx.payload);
    ktcs_core::merkle::sha256(&data)
}

fn kas(s: u64) -> String {
    format!("{:.8} KAS", s as f64 / SOMPI as f64)
}

// ---------------------------------------------------------------------------------------------
// Mock rusty-kaspa wRPC JSON node: responses serialized by upstream kaspa-rpc-core types,
// requests decoded by upstream types, admission by upstream consensus/mempool rules.
// ---------------------------------------------------------------------------------------------

#[derive(Default)]
struct Node {
    utxos: HashMap<String, Vec<(TransactionOutpoint, UtxoEntry)>>,
    blocks: HashMap<Hash, RpcBlock>,
    tips: Vec<Hash>,
    daa: u64,
    /// log of (method, outcome) for the report
    log: Vec<(String, String)>,
}

fn header(hash: Hash, parents: Vec<Vec<Hash>>, daa: u64) -> RpcHeader {
    RpcHeader {
        hash,
        version: 2,
        parents_by_level: parents,
        hash_merkle_root: Hash::from_bytes([1; 32]),
        accepted_id_merkle_root: Hash::from_bytes([2; 32]),
        utxo_commitment: Hash::from_bytes([3; 32]),
        timestamp: 1_790_000_000_000,
        bits: 0x1d00ffff,
        nonce: 42,
        daa_score: daa,
        // a realistic mainnet-scale blue work value
        blue_work: Uint192::from_u128(0x0000_0000_0034_5f3b_9c1d_2e7a_88b1_04ff_u128),
        blue_score: daa - 1_000_000,
        pruning_point: Hash::from_bytes([4; 32]),
    }
}

impl Node {
    fn handle_ro_dag(&self) -> Value {
        let tip = self.tips[0];
        serde_json::to_value(GetBlockDagInfoResponse::new(
            NetworkId::new(NetworkType::Mainnet),
            100,
            100,
            self.tips.clone(),
            1.0e15,
            1_790_000_000_000,
            self.tips.clone(),
            Hash::from_bytes([4; 32]),
            self.daa,
            tip,
        ))
        .unwrap()
    }

    fn add_block(&mut self, txs: Vec<RpcTransaction>) -> Hash {
        self.daa += 1;
        let hash = Hash::from_bytes({
            let mut b = [0u8; 32];
            b[..8].copy_from_slice(&self.daa.to_le_bytes());
            b[31] = 0xbb;
            b
        });
        let p1 = self
            .tips
            .first()
            .copied()
            .unwrap_or(Hash::from_bytes([0xa1; 32]));
        let parents = vec![
            vec![p1, Hash::from_bytes([0xa2; 32])],
            vec![Hash::from_bytes([0xa3; 32])],
        ];
        let ids = txs
            .iter()
            .filter_map(|t| t.verbose_data.as_ref().map(|v| v.transaction_id))
            .collect();
        let block = RpcBlock {
            header: header(hash, parents, self.daa),
            transactions: txs,
            verbose_data: Some(RpcBlockVerboseData {
                hash,
                difficulty: 1.0e15,
                selected_parent_hash: p1,
                transaction_ids: ids,
                is_header_only: false,
                blue_score: self.daa - 1_000_000,
                children_hashes: vec![],
                merge_set_blues_hashes: vec![],
                merge_set_reds_hashes: vec![],
                is_chain_block: true,
            }),
        };
        self.blocks.insert(hash, block);
        self.tips = vec![hash];
        hash
    }

    fn handle(&mut self, method: &str, params: Value) -> Result<Value, String> {
        match method {
            "getBlockDagInfo" => {
                let tip = self.tips[0];
                let r = GetBlockDagInfoResponse::new(
                    NetworkId::new(NetworkType::Mainnet),
                    100,
                    100,
                    self.tips.clone(),
                    1.0e15,
                    1_790_000_000_000,
                    self.tips.clone(),
                    Hash::from_bytes([4; 32]),
                    self.daa,
                    tip,
                );
                Ok(serde_json::to_value(r).unwrap())
            }
            "getBlock" => {
                let hash: Hash =
                    serde_json::from_value(params["hash"].clone()).map_err(|e| e.to_string())?;
                let include_txs = params["includeTransactions"].as_bool().unwrap_or(false);
                let mut block = self
                    .blocks
                    .get(&hash)
                    .cloned()
                    .ok_or_else(|| format!("Block {hash} not found"))?;
                if !include_txs {
                    block.transactions.clear();
                }
                Ok(serde_json::to_value(GetBlockResponse { block }).unwrap())
            }
            "getUtxosByAddresses" => {
                let addrs: Vec<String> = serde_json::from_value(params["addresses"].clone())
                    .map_err(|e| e.to_string())?;
                let mut entries = vec![];
                for a in addrs {
                    let address = Address::try_from(a.as_str()).map_err(|e| e.to_string())?;
                    for (op, e) in self.utxos.get(&a).cloned().unwrap_or_default() {
                        entries.push(RpcUtxosByAddressesEntry {
                            address: Some(address.clone()),
                            outpoint: RpcTransactionOutpoint::from(op),
                            utxo_entry: RpcUtxoEntry::new(
                                e.amount,
                                e.script_public_key.clone(),
                                e.block_daa_score,
                                e.is_coinbase,
                                None,
                            ),
                        });
                    }
                }
                Ok(serde_json::to_value(GetUtxosByAddressesResponse::new(entries)).unwrap())
            }
            "submitTransaction" => {
                // 1) wire decode with upstream types
                let req: SubmitTransactionRequest = serde_json::from_value(params)
                    .map_err(|e| format!("submitTransaction JSON decode failed: {e}"))?;
                let tx = UTx::try_from(req.transaction).map_err(|e| e.to_string())?;
                // 2) populate inputs from our UTXO set
                let mut entries = vec![];
                for i in &tx.inputs {
                    let e = self
                        .utxos
                        .values()
                        .flatten()
                        .find(|(op, _)| *op == i.previous_outpoint)
                        .map(|(_, e)| e.clone())
                        .ok_or_else(|| format!("missing outpoint {}", i.previous_outpoint))?;
                    entries.push(e);
                }
                let fee = entries.iter().map(|e| e.amount).sum::<u64>()
                    - tx.outputs.iter().map(|o| o.value).sum::<u64>();
                // 3) upstream admission
                let m = mempool_verdict(&tx, &entries, fee)?;
                // 4) "mine" it into a new tip block
                let id = tx.id();
                let mut rtx = RpcTransaction::from(&tx);
                rtx.verbose_data = Some(RpcTransactionVerboseData {
                    transaction_id: id,
                    hash: kaspa_consensus_core::hashing::tx::hash(&tx),
                    compute_mass: m.compute,
                    block_hash: Hash::default(),
                    block_time: 0,
                });
                let bh = self.add_block(vec![rtx]);
                if let Some(b) = self.blocks.get_mut(&bh) {
                    for t in b.transactions.iter_mut() {
                        if let Some(v) = t.verbose_data.as_mut() {
                            v.block_hash = bh;
                        }
                    }
                }
                self.log.push((
                    "submitTransaction".into(),
                    format!("ACCEPTED fee={fee} masses={m:?}"),
                ));
                Ok(serde_json::to_value(SubmitTransactionResponse { transaction_id: id }).unwrap())
            }
            "getMempoolEntry" => {
                let _req: GetMempoolEntryRequest = serde_json::from_value(params)
                    .map_err(|e| format!("getMempoolEntry JSON decode failed: {e}"))?;
                Err("Transaction not found in mempool".into())
            }
            other => Err(format!(
                "method `{other}` is not served by rusty-kaspa wRPC v2.1.0"
            )),
        }
    }
}

async fn spawn_node(node: Arc<Mutex<Node>>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => break,
            };
            let node = node.clone();
            tokio::spawn(async move {
                let ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                let (mut tx, mut rx) = ws.split();
                while let Some(Ok(msg)) = rx.next().await {
                    let Message::Text(text) = msg else { continue };
                    let req: Value = serde_json::from_str(&text).unwrap();
                    let id = req["id"].clone();
                    let method = req["method"].as_str().unwrap_or("").to_string();
                    let result = node.lock().unwrap().handle(&method, req["params"].clone());
                    // workflow-rpc 0.18 JSON envelope (JSONServerMessage / JsonServerError)
                    let resp = match &result {
                        Ok(p) => json!({"id": id, "method": method, "params": p}),
                        Err(e) => {
                            json!({"id": id, "method": method, "error": {"code": 0, "message": e, "data": null}})
                        }
                    };
                    node.lock().unwrap().log.push((
                        method.clone(),
                        match &result {
                            Ok(_) => "ok".into(),
                            Err(e) => format!("ERROR: {e}"),
                        },
                    ));
                    if tx.send(Message::Text(resp.to_string())).await.is_err() {
                        break;
                    }
                }
            });
        }
    });
    format!("ws://{addr}")
}

// ---------------------------------------------------------------------------------------------
// [J] 10-BPS tip-lifetime simulation for KaspaClient::wait_for_confirmation
// ---------------------------------------------------------------------------------------------

struct SimNode {
    blocks: HashMap<Hash, (RpcBlock, std::time::Instant)>,
    pending: Vec<(RpcTransaction, std::time::Instant)>,
    daa: u64,
    tip_life: std::time::Duration,
    inclusion_delay: std::time::Duration,
}

impl SimNode {
    fn tips(&self) -> Vec<Hash> {
        let now = std::time::Instant::now();
        self.blocks
            .iter()
            .filter(|(_, (_, t))| now.duration_since(*t) < self.tip_life)
            .map(|(h, _)| *h)
            .collect()
    }
    fn produce(&mut self) {
        self.daa += 1;
        let mut b = [0u8; 32];
        b[..8].copy_from_slice(&self.daa.to_le_bytes());
        b[31] = 0x5e;
        let hash = Hash::from_bytes(b);
        let parents = vec![self.tips()];
        let now = std::time::Instant::now();
        let delay = self.inclusion_delay;
        let (ready, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.pending)
            .into_iter()
            .partition(|(_, t)| now.duration_since(*t) >= delay);
        self.pending = waiting;
        let txs: Vec<RpcTransaction> = ready.into_iter().map(|(t, _)| t).collect();
        let ids = txs
            .iter()
            .filter_map(|t| t.verbose_data.as_ref().map(|v| v.transaction_id))
            .collect();
        let block = RpcBlock {
            header: header(hash, parents, self.daa),
            transactions: txs,
            verbose_data: Some(RpcBlockVerboseData {
                hash,
                difficulty: 1.0,
                selected_parent_hash: hash,
                transaction_ids: ids,
                is_header_only: false,
                blue_score: self.daa - 1_000_000,
                children_hashes: vec![],
                merge_set_blues_hashes: vec![],
                merge_set_reds_hashes: vec![],
                is_chain_block: true,
            }),
        };
        self.blocks.insert(hash, (block, std::time::Instant::now()));
    }
    fn handle(&mut self, method: &str, params: Value) -> Result<Value, String> {
        match method {
            "getBlockDagInfo" => {
                let tips = self.tips();
                Ok(serde_json::to_value(GetBlockDagInfoResponse::new(
                    NetworkId::new(NetworkType::Mainnet),
                    0,
                    0,
                    tips.clone(),
                    1.0,
                    0,
                    tips.clone(),
                    Hash::from_bytes([4; 32]),
                    self.daa,
                    tips[0],
                ))
                .unwrap())
            }
            "getBlock" => {
                let hash: Hash =
                    serde_json::from_value(params["hash"].clone()).map_err(|e| e.to_string())?;
                let (block, _) = self.blocks.get(&hash).cloned().ok_or("not found")?;
                Ok(serde_json::to_value(GetBlockResponse { block }).unwrap())
            }
            "submitTransaction" => {
                let req: SubmitTransactionRequest =
                    serde_json::from_value(params).map_err(|e| e.to_string())?;
                let tx = UTx::try_from(req.transaction).map_err(|e| e.to_string())?;
                let mut r = RpcTransaction::from(&tx);
                r.verbose_data = Some(RpcTransactionVerboseData {
                    transaction_id: tx.id(),
                    hash: tx.id(),
                    compute_mass: 0,
                    block_hash: Hash::default(),
                    block_time: 0,
                });
                self.pending.push((r, std::time::Instant::now()));
                Ok(serde_json::to_value(SubmitTransactionResponse {
                    transaction_id: tx.id(),
                })
                .unwrap())
            }
            _ => Err("unsupported".into()),
        }
    }
}

async fn simulate_confirmation(
    rtt_ms: u64,
    tip_life_ms: u64,
    inclusion_ms: u64,
    trials: usize,
) -> (usize, usize) {
    let node = Arc::new(Mutex::new(SimNode {
        blocks: HashMap::new(),
        pending: vec![],
        daa: 474_300_000,
        tip_life: std::time::Duration::from_millis(tip_life_ms),
        inclusion_delay: std::time::Duration::from_millis(inclusion_ms),
    }));
    node.lock().unwrap().produce();
    // 10 BPS block producer
    let producer = {
        let node = node.clone();
        tokio::spawn(async move {
            let mut iv = tokio::time::interval(std::time::Duration::from_millis(100));
            loop {
                iv.tick().await;
                node.lock().unwrap().produce();
            }
        })
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let server = {
        let node = node.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let node = node.clone();
                tokio::spawn(async move {
                    let ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                    let (mut tx, mut rx) = ws.split();
                    while let Some(Ok(Message::Text(text))) = rx.next().await {
                        let req: Value = serde_json::from_str(&text).unwrap();
                        let method = req["method"].as_str().unwrap_or("").to_string();
                        // jittered RTT in [0.5, 1.5] x rtt_ms; half before the node evaluates, half after
                        let j = {
                            use std::sync::atomic::{AtomicU64, Ordering};
                            static SEED: AtomicU64 = AtomicU64::new(0x2545F4914F6CDD1D);
                            let mut x = SEED.load(Ordering::Relaxed);
                            x ^= x << 13;
                            x ^= x >> 7;
                            x ^= x << 17;
                            SEED.store(x, Ordering::Relaxed);
                            (x % 1001) as f64 / 1000.0
                        };
                        let rtt = (rtt_ms as f64 * (0.5 + j)) as u64;
                        tokio::time::sleep(std::time::Duration::from_millis(rtt / 2)).await;
                        let result = node.lock().unwrap().handle(&method, req["params"].clone());
                        tokio::time::sleep(std::time::Duration::from_millis(rtt - rtt / 2)).await;
                        let resp = match result {
                            Ok(p) => json!({"id": req["id"], "method": method, "params": p}),
                            Err(e) => {
                                json!({"id": req["id"], "method": method, "error": {"code": 0, "message": e, "data": null}})
                            }
                        };
                        if tx.send(Message::Text(resp.to_string())).await.is_err() {
                            break;
                        }
                    }
                });
            }
        })
    };
    let wallet = KaspaWallet::from_private_key(&PRIV, "mainnet").unwrap();
    let pk = wallet.public_key();
    let client = KaspaClient::new(KaspaClientConfig {
        rpc_url: url,
        network: Some("mainnet".into()),
        connect_timeout_ms: 5_000,
        request_timeout_ms: 10_000,
        auto_reconnect: false,
        use_resolver: false,
        tls_verify: true,
    });
    client.connect().await.unwrap();
    let mut ok = 0;
    for t in 0..trials {
        let us = vec![utxo([t as u8; 32], t as u32, 5 * SOMPI, p2pk_script(&pk))];
        let b = TransactionBuilder::new()
            .commitment(&[t as u8; 32])
            .add_inputs(us.clone())
            .unwrap()
            .change_address(wallet.address())
            .fee_per_gram(1_000)
            .build()
            .unwrap();
        let s = sign_transaction(&b.transaction, &wallet, &us).unwrap();
        let id = client.submit_transaction(s).await.unwrap();
        if client.wait_for_confirmation(&id, 6_000).await.is_ok() {
            ok += 1;
        }
    }
    producer.abort();
    server.abort();
    (ok, trials)
}

#[tokio::main]
async fn main() {
    let wallet = KaspaWallet::from_private_key(&PRIV, "mainnet").unwrap();
    let pk = wallet.public_key();
    let addr = wallet.address().to_string();

    println!("=====================================================================");
    println!(" kasTime vs rusty-kaspa v2.1.0 — empirical compatibility checks");
    println!("=====================================================================\n");

    // -----------------------------------------------------------------------------------------
    println!("[A] Address encoding (kasTime KaspaWallet vs upstream kaspa-addresses)");
    let up = Address::new(Prefix::Mainnet, Version::PubKey, &pk).to_string();
    println!("    kasTime : {addr}");
    println!("    upstream: {up}");
    println!("    match   : {}", addr == up);
    let p2sh = Address::new(Prefix::Mainnet, Version::ScriptHash, &[0x5a; 32]);
    let ecdsa = Address::new(Prefix::Mainnet, Version::PubKeyECDSA, &[0x02; 33]);
    match build_address_script(&p2sh.to_string()) {
        Ok(s) => {
            let upstream_spk = pay_to_address_script(&p2sh);
            println!(
                "    P2SH address -> kasTime script {} | upstream script {} | SAME={}",
                hex::encode(&s.script),
                hex::encode(upstream_spk.script()),
                s.script.as_slice() == upstream_spk.script()
            );
        }
        Err(e) => println!("    P2SH address -> kasTime error: {e}"),
    }
    match build_address_script(&ecdsa.to_string()) {
        Ok(s) => println!(
            "    ECDSA address -> kasTime script {}",
            hex::encode(&s.script)
        ),
        Err(e) => println!(
            "    ECDSA address -> kasTime error: {e}  (upstream script: {})",
            hex::encode(pay_to_address_script(&ecdsa).script())
        ),
    }
    println!(
        "    decode_address(P2SH) accepted as valid: {}",
        decode_address(&p2sh.to_string()).is_ok()
    );

    // -----------------------------------------------------------------------------------------
    println!("\n[B] v0 sighash equivalence: kasTime compute_kaspa_sighash vs upstream calc_schnorr_signature_hash");
    let mut rng = Rng(0x9e3779b97f4a7c15);
    let (mut checked, mut matched) = (0, 0);
    for case in 0..300 {
        let n_in = rng.range(1, 5) as usize;
        let n_out = rng.range(1, 4) as usize;
        let utxos: Vec<Utxo> = (0..n_in)
            .map(|i| {
                utxo(
                    rng.bytes32(),
                    rng.range(0, 9) as u32 + i as u32 * 10,
                    rng.range(1, 50) * SOMPI,
                    p2pk_script(&rng.bytes32()),
                )
            })
            .collect();
        let tx = KTx {
            version: 0,
            inputs: utxos
                .iter()
                .map(|u| KTxIn {
                    previous_outpoint_hash: u.transaction_id,
                    previous_outpoint_index: u.index,
                    signature_script: vec![],
                })
                .collect(),
            outputs: (0..n_out)
                .map(|_| KTxOut {
                    amount: rng.range(1, 10) * SOMPI,
                    script_public_key: KSpk {
                        version: 0,
                        script: p2pk_script(&rng.bytes32()),
                    },
                })
                .collect(),
            lock_time: if case % 7 == 0 { rng.next() } else { 0 },
            subnetwork_id: [0; 20],
            gas: 0,
            payload: if case % 3 == 0 {
                (0..rng.range(1, 80)).map(|_| rng.next() as u8).collect()
            } else {
                vec![]
            },
        };
        let ktx = ksighash_tx(&tx, &utxos);
        let utx = to_upstream(&tx);
        let entries: Vec<UtxoEntry> = utxos.iter().map(entry_of).collect();
        let populated = PopulatedTransaction::new(&utx, entries);
        let reused = SigHashReusedValuesUnsync::new();
        for i in 0..n_in {
            let k = compute_kaspa_sighash(&ktx, i, SigHashType::All).unwrap();
            let u = calc_schnorr_signature_hash(&populated, i, SIG_HASH_ALL, &reused);
            checked += 1;
            if k == u.as_bytes() {
                matched += 1;
            }
        }
    }
    println!("    {matched}/{checked} input sighashes identical (incl. ~1/3 with non-empty native payloads)");

    // -----------------------------------------------------------------------------------------
    println!("\n[C] kasTime-signed commitment tx validated by upstream txscript engine (post-Toccata metering)");
    let funding = vec![utxo([0x31; 32], 0, 5 * SOMPI, p2pk_script(&pk))];
    let built = TransactionBuilder::new()
        .commitment(&[0xcc; 32])
        .add_inputs(funding.clone())
        .unwrap()
        .change_address(&addr)
        .fee_per_gram(10)
        .build()
        .unwrap();
    let signed = sign_transaction(&built.transaction, &wallet, &funding).unwrap();
    let usigned = to_upstream(&signed);
    let entries: Vec<UtxoEntry> = funding.iter().map(entry_of).collect();
    println!(
        "    script engine: {:?}",
        validate_scripts(&usigned, entries.clone())
    );
    let wasm_id = ktcs_wasm_compute_transaction_id(&signed);
    println!("    upstream tx.id()          : {}", usigned.id());
    println!(
        "    ktcs-wasm computed tx id  : {}  (match={})",
        hex::encode(wasm_id),
        wasm_id == usigned.id().as_bytes()
    );

    // -----------------------------------------------------------------------------------------
    println!(
        "\n[D] Fee adequacy vs post-Toccata min relay fee (100 sompi/gram * max(compute, 2*bytes))"
    );
    println!(
        "    {:<44} {:>9} {:>8} {:>9} {:>9} {:>12} {:>12} {:>7}",
        "scenario", "kT-mass", "compute", "2*bytes", "storage", "kT fee", "min fee", "ratio"
    );
    let scenarios: Vec<(&str, usize, u64, u64)> = vec![
        ("calendar default FEE_PER_GRAM=1, 1 input", 1, 5 * SOMPI, 1),
        ("calendar prod FEE_PER_GRAM=10, 1 input", 1, 5 * SOMPI, 10),
        ("direct/CLI/browser fee_rate=10, 1 input", 1, 5 * SOMPI, 10),
        ("calendar prod FEE_PER_GRAM=10, 5 inputs", 5, SOMPI, 10),
        (
            "calendar max allowed FEE_PER_GRAM=10000, 1 in",
            1,
            5 * SOMPI,
            10_000,
        ),
    ];
    for (name, n, amt, fpg) in scenarios {
        let us: Vec<Utxo> = (0..n)
            .map(|i| utxo([0x40 + i as u8; 32], i as u32, amt, p2pk_script(&pk)))
            .collect();
        let b = TransactionBuilder::new()
            .commitment(&[0xcc; 32])
            .add_inputs(us.clone())
            .unwrap()
            .change_address(&addr)
            .fee_per_gram(fpg)
            .build()
            .unwrap();
        let s = sign_transaction(&b.transaction, &wallet, &us).unwrap();
        let u = to_upstream(&s);
        let e: Vec<UtxoEntry> = us.iter().map(entry_of).collect();
        let m = masses(&u, &e);
        let kt_mass = b.fee / fpg;
        println!(
            "    {:<44} {:>9} {:>8} {:>9} {:>9} {:>12} {:>12} {:>6.1}x",
            name,
            kt_mass,
            m.compute,
            m.norm_transient,
            m.storage.unwrap_or(0),
            b.fee,
            m.min_fee,
            m.min_fee as f64 / b.fee as f64
        );
    }
    // recycle (TransferTransactionBuilder) path
    let rus: Vec<Utxo> = (0..3)
        .map(|i| utxo([0x50 + i as u8; 32], i as u32, 2 * SOMPI, p2pk_script(&pk)))
        .collect();
    let t = TransferTransactionBuilder::new()
        .add_inputs(rus.clone())
        .destination(&addr)
        .fee_per_gram(10)
        .build()
        .unwrap();
    let ts = sign_transaction(&t.transaction, &wallet, &rus).unwrap();
    let tm = masses(
        &to_upstream(&ts),
        &rus.iter().map(entry_of).collect::<Vec<_>>(),
    );
    println!(
        "    {:<44} {:>9} {:>8} {:>9} {:>9} {:>12} {:>12} {:>6.1}x",
        "recycle 3-in/1-out FEE_PER_GRAM=10",
        t.fee / 10,
        tm.compute,
        tm.norm_transient,
        tm.storage.unwrap_or(0),
        t.fee,
        tm.min_fee,
        tm.min_fee as f64 / t.fee as f64
    );

    // -----------------------------------------------------------------------------------------
    println!("\n[E] Storage mass & effective priority of the 0.2 KAS burn design (1 input -> burn + change)");
    println!(
        "    {:>14} {:>14} {:>10} {:>14} {:>16}",
        "input", "change", "storage", "upstream", "feerate@minfee"
    );
    for input in [
        21_000_000u64,
        25_000_000,
        30_000_000,
        35_000_000,
        50_000_000,
        100_000_000,
        1_000_000_000,
    ] {
        let us = vec![utxo([0x60; 32], 0, input, p2pk_script(&pk))];
        let b = match TransactionBuilder::new()
            .commitment(&[0xcc; 32])
            .add_inputs(us.clone())
            .unwrap()
            .change_address(&addr)
            .fee_per_gram(10)
            .build()
        {
            Ok(b) => b,
            Err(e) => {
                println!("    {:>14} kasTime builder error: {e}", kas(input));
                continue;
            }
        };
        let s = sign_transaction(&b.transaction, &wallet, &us).unwrap();
        let u = to_upstream(&s);
        let e: Vec<UtxoEntry> = us.iter().map(entry_of).collect();
        let m = masses(&u, &e);
        let verdict = mempool_verdict(&u, &e, m.min_fee.max(b.fee));
        println!(
            "    {:>14} {:>14} {:>10} {:>14} {:>13.2} s/g",
            kas(input),
            kas(b.change_amount),
            m.storage.unwrap_or(0),
            if verdict.is_ok() {
                "ok@minfee".to_string()
            } else {
                "REJECTED".to_string()
            },
            m.min_fee as f64 / m.priority_mass as f64
        );
        if let Err(v) = verdict {
            println!("        -> {v}");
        }
    }

    // -----------------------------------------------------------------------------------------
    println!("\n[F] Alternative: commitment in tx payload (native payloads enabled since Crescendo), no burn output");
    let us = vec![utxo([0x70; 32], 0, SOMPI, p2pk_script(&pk))];
    let mut payload = b"KTCS".to_vec();
    payload.extend_from_slice(&[0xcc; 32]);
    let mut ptx = KTx {
        version: 0,
        inputs: vec![KTxIn {
            previous_outpoint_hash: us[0].transaction_id,
            previous_outpoint_index: 0,
            signature_script: vec![],
        }],
        outputs: vec![KTxOut {
            amount: SOMPI - 300_000,
            script_public_key: KSpk {
                version: 0,
                script: p2pk_script(&pk),
            },
        }],
        lock_time: 0,
        subnetwork_id: [0; 20],
        gas: 0,
        payload,
    };
    ptx = sign_transaction(&ptx, &wallet, &us).unwrap();
    let pu = to_upstream(&ptx);
    let pe: Vec<UtxoEntry> = us.iter().map(entry_of).collect();
    let pm = masses(&pu, &pe);
    println!("    script engine: {:?}", validate_scripts(&pu, pe.clone()));
    println!(
        "    masses: compute={} 2*bytes={} storage={} -> min fee {} ({}), nothing burned, no new UTXO-set bloat",
        pm.compute,
        pm.norm_transient,
        pm.storage.unwrap_or(0),
        pm.min_fee,
        kas(pm.min_fee)
    );
    println!(
        "    mempool verdict at fee 300_000: {:?}",
        mempool_verdict(&pu, &pe, 300_000).map(|_| "accepted")
    );

    // -----------------------------------------------------------------------------------------
    println!("\n[G] End-to-end against a mock v2.1.0 wRPC JSON node (upstream serde types + upstream admission rules)");
    let node = Arc::new(Mutex::new(Node {
        daa: 474_200_000,
        ..Default::default()
    }));
    {
        let mut n = node.lock().unwrap();
        n.add_block(vec![]);
        let spk = UScriptPublicKey::from_vec(0, p2pk_script(&pk));
        n.utxos.insert(
            addr.clone(),
            vec![
                (
                    TransactionOutpoint::new(Hash::from_bytes([0x81; 32]), 0),
                    UtxoEntry::new(15_000_000, spk.clone(), 1_000, false, None),
                ),
                (
                    TransactionOutpoint::new(Hash::from_bytes([0x82; 32]), 1),
                    UtxoEntry::new(10_000_000, spk.clone(), 1_000, false, None),
                ),
                (
                    TransactionOutpoint::new(Hash::from_bytes([0x83; 32]), 2),
                    UtxoEntry::new(10_000_000, spk.clone(), 1_000, false, None),
                ),
                (
                    TransactionOutpoint::new(Hash::from_bytes([0x84; 32]), 3),
                    UtxoEntry::new(5 * SOMPI, spk.clone(), 1_000, false, None),
                ),
            ],
        );
    }
    let url = spawn_node(node.clone()).await;
    let client = KaspaClient::new(KaspaClientConfig {
        rpc_url: url.clone(),
        network: Some("mainnet".into()),
        connect_timeout_ms: 5_000,
        request_timeout_ms: 5_000,
        auto_reconnect: false,
        use_resolver: false,
        tls_verify: true,
    });
    client.connect().await.unwrap();

    let dag = client.get_block_dag_info().await;
    match &dag {
        Ok(d) => println!(
            "    get_block_dag_info: network={} daa={} current_blue_score={} current_blue_work={} tips={}",
            d.network,
            d.current_daa_score,
            d.current_blue_score,
            hex::encode(d.current_blue_work),
            d.tip_hashes.len()
        ),
        Err(e) => println!("    get_block_dag_info: ERROR {e}"),
    }

    let utxos = client.get_utxos_by_address(&addr).await.unwrap();
    println!(
        "    get_utxos_by_address: {} utxos, first script version={} script_len={}",
        utxos.len(),
        utxos[0].script_public_key.version,
        utxos[0].script_public_key.script.len()
    );

    // G1: calendar submission path exactly as ktcs-calendar KaspaService::submit_commitment (FEE_PER_GRAM=10)
    let all = utxos.clone();
    let b = TransactionBuilder::new()
        .commitment(&[0xcd; 32])
        .add_inputs(all.clone())
        .unwrap()
        .change_address(&addr)
        .fee_per_gram(10)
        .build()
        .unwrap();
    let s = sign_transaction(&b.transaction, &wallet, &all).unwrap();
    println!(
        "    [calendar path, FEE_PER_GRAM=10] kasTime fee={} -> submit_transaction:",
        b.fee
    );
    match client.submit_transaction(s).await {
        Ok(id) => println!("        accepted: {}", hex::encode(id)),
        Err(e) => println!("        {e}"),
    }

    // G2: CLI `ktcs stamp --direct` path (prepare_direct_stamp w/ DirectStampConfig::default())
    println!("    [CLI direct path] prepare_direct_stamp with utxos 0.15/0.10/0.10/5.0 KAS:");
    match prepare_direct_stamp(b"hello", &wallet, &client, &DirectStampConfig::default()).await {
        Ok(p) => {
            let tx = p.transaction.as_ref().unwrap();
            println!(
                "        built: inputs={} fee={} (selected amounts: {:?})",
                tx.transaction.inputs.len(),
                tx.fee,
                p.utxos
                    .as_ref()
                    .unwrap()
                    .iter()
                    .map(|u| u.amount)
                    .collect::<Vec<_>>()
            );
        }
        Err(e) => println!("        error: {e}"),
    }
    // same, but without the large UTXO (only 0.15 + 0.10 + 0.10 = 0.35 KAS available)
    {
        let mut n = node.lock().unwrap();
        n.utxos
            .get_mut(&addr)
            .unwrap()
            .retain(|(_, e)| e.amount < SOMPI);
    }
    println!("    [CLI direct path] same, wallet holds 0.15+0.10+0.10 = 0.35 KAS:");
    match prepare_direct_stamp(b"hello", &wallet, &client, &DirectStampConfig::default()).await {
        Ok(p) => println!("        built: fee={}", p.estimated_fee),
        Err(e) => println!("        error: {e}"),
    }
    {
        let mut n = node.lock().unwrap();
        let spk = UScriptPublicKey::from_vec(0, p2pk_script(&pk));
        n.utxos.get_mut(&addr).unwrap().push((
            TransactionOutpoint::new(Hash::from_bytes([0x84; 32]), 3),
            UtxoEntry::new(5 * SOMPI, spk, 1_000, false, None),
        ));
    }

    // G3: same calendar path but with a fee that satisfies the new floor -> shows everything else on the wire is still accepted
    let utxos = client.get_utxos_by_address(&addr).await.unwrap();
    let big: Vec<Utxo> = utxos
        .into_iter()
        .filter(|u| u.amount == 5 * SOMPI)
        .collect();
    let b = TransactionBuilder::new()
        .commitment(&[0xce; 32])
        .add_inputs(big.clone())
        .unwrap()
        .change_address(&addr)
        .fee_per_gram(1_000)
        .build()
        .unwrap();
    let s = sign_transaction(&b.transaction, &wallet, &big).unwrap();
    let commitment = [0xce; 32];
    println!(
        "    [calendar path, FEE_PER_GRAM=1000 (fee={})] submit_transaction:",
        b.fee
    );
    let txid = match client.submit_transaction(s).await {
        Ok(id) => {
            println!("        accepted: {}", hex::encode(id));
            Some(id)
        }
        Err(e) => {
            println!("        {e}");
            None
        }
    };

    if let Some(txid) = txid {
        let blk = client.wait_for_confirmation(&txid, 5_000).await;
        match &blk {
            Ok(bi) => {
                let up = node
                    .lock()
                    .unwrap()
                    .blocks
                    .get(&Hash::from_bytes(bi.hash))
                    .unwrap()
                    .header
                    .parents_by_level
                    .clone();
                println!(
                    "    wait_for_confirmation -> block {} daa={} blue_work={} parent_hashes(kasTime)={} | parentsByLevel(node)={:?}",
                    hex::encode(&bi.hash[..6]),
                    bi.daa_score,
                    hex::encode(bi.blue_work),
                    bi.parent_hashes.len(),
                    up.iter().map(|l| l.len()).collect::<Vec<_>>()
                );
                let att = KaspaAttestation::new(
                    bi.daa_score,
                    bi.blue_score,
                    bi.hash,
                    bi.timestamp,
                    txid,
                    0,
                    bi.blue_work,
                    bi.parent_hashes.clone(),
                );
                match verify_attestation_on_chain(&client, &att, &commitment).await {
                    Ok(r) => println!(
                        "    verify_attestation_on_chain -> block_verified={} tx_verified={} commitment_verified={}",
                        r.block_verified, r.tx_verified, r.commitment_verified
                    ),
                    Err(e) => println!("    verify_attestation_on_chain -> ERROR {e}"),
                }
                let (_, txs) = client.get_block_with_transactions(&bi.hash).await.unwrap();
                let o0 = &txs[0].outputs[0];
                println!(
                    "    get_block_with_transactions output[0]: script_len={} (expected 34) -> extract_commitment_from_output={:?}",
                    o0.script_public_key.script.len(),
                    ktcs_core::extract_commitment_from_output(o0).map(hex::encode)
                );
            }
            Err(e) => println!("    wait_for_confirmation -> ERROR {e}"),
        }
    }

    // G4: WASM (browser direct stamping) request builder decoded by upstream
    let us = vec![utxo([0x91; 32], 0, 5 * SOMPI, p2pk_script(&pk))];
    let b = TransactionBuilder::new()
        .commitment(&[0xcf; 32])
        .add_inputs(us.clone())
        .unwrap()
        .change_address(&addr)
        .fee_per_gram(10)
        .build()
        .unwrap();
    let s = sign_transaction(&b.transaction, &wallet, &us).unwrap();
    let req = ktcs_wasm::create_submit_tx_rpc_request(&serde_json::to_string(&s).unwrap())
        .unwrap_or_else(|_| "ERR".into());
    let v: Value = serde_json::from_str(&req).unwrap();
    let dec = serde_json::from_value::<SubmitTransactionRequest>(v["params"].clone());
    println!(
        "\n    ktcs-wasm create_submit_tx_rpc_request: upstream decode={} | tx fields sent: {:?}",
        if dec.is_ok() {
            "ok".to_string()
        } else {
            format!("ERR {:?}", dec.err())
        },
        v["params"]["transaction"]
            .as_object()
            .map(|o| o.keys().cloned().collect::<Vec<_>>())
    );

    // G5: getMempoolEntry params as sent by KaspaClient::is_transaction_in_mempool
    let r = serde_json::from_value::<GetMempoolEntryRequest>(
        json!({"txId": hex::encode([1u8;32]), "includeOrphanPool": false, "filterTransactionPool": true}),
    );
    println!(
        "    getMempoolEntry params {{txId,...}} upstream decode: {}",
        match r {
            Ok(_) => "ok".to_string(),
            Err(e) => format!("ERR {e}"),
        }
    );

    // -----------------------------------------------------------------------------------------
    println!("\n[H] Post-Toccata block contents: a v1 tx (computeBudget + covenant output) next to kasTime's v0 tx");
    {
        use kaspa_consensus_core::tx::CovenantBinding;
        let v1 = UTx::new(
            1,
            vec![UTxIn::new_with_compute_budget(
                TransactionOutpoint::new(Hash::from_bytes([0xd1; 32]), 0),
                vec![0x51],
                0,
                7,
            )],
            vec![UTxOut::with_covenant(
                3 * SOMPI,
                UScriptPublicKey::from_vec(0, p2pk_script(&[0xd2; 32])),
                Some(CovenantBinding::new(0, Hash::from_bytes([0xd3; 32]))),
            )],
            0,
            SubnetworkId::from_bytes([0; 20]),
            0,
            vec![],
        );
        let mut r1 = RpcTransaction::from(&v1);
        r1.verbose_data = Some(RpcTransactionVerboseData {
            transaction_id: v1.id(),
            hash: kaspa_consensus_core::hashing::tx::hash(&v1),
            compute_mass: 0,
            block_hash: Hash::default(),
            block_time: 0,
        });
        let v1_json = serde_json::to_value(&r1).unwrap();
        println!(
            "    v1 tx JSON input keys : {:?}",
            v1_json["inputs"][0]
                .as_object()
                .unwrap()
                .keys()
                .collect::<Vec<_>>()
        );
        println!(
            "    v1 tx JSON output keys: {:?}",
            v1_json["outputs"][0]
                .as_object()
                .unwrap()
                .keys()
                .collect::<Vec<_>>()
        );
        let bh = node.lock().unwrap().add_block(vec![r1]);
        match client.get_block_with_transactions(&bh.as_bytes()).await {
            Ok((b, txs)) => println!(
                "    kasTime get_block_with_transactions on v1-containing block: ok ({} txs parsed, tx ids listed: {})",
                txs.len(),
                b.transaction_ids.len()
            ),
            Err(e) => println!("    kasTime get_block_with_transactions on v1-containing block: ERROR {e}"),
        }
        // covenant-bound UTXO at the calendar's own address: still spendable by kasTime's v0 signature?
        let cov_utxo = utxo([0xd4; 32], 0, 5 * SOMPI, p2pk_script(&pk));
        let b = TransactionBuilder::new()
            .commitment(&[0xc1; 32])
            .add_inputs(vec![cov_utxo.clone()])
            .unwrap()
            .change_address(&addr)
            .fee_per_gram(1_000)
            .build()
            .unwrap();
        let s =
            to_upstream(&sign_transaction(&b.transaction, &wallet, &[cov_utxo.clone()]).unwrap());
        let mut e = entry_of(&cov_utxo);
        e.covenant_id = Some(Hash::from_bytes([0xd5; 32]));
        println!(
            "    v0 spend of covenant-bound P2PK UTXO (scripts): {:?}",
            validate_scripts(&s, vec![e.clone()])
        );
        let populated = PopulatedTransaction::new(&s, vec![e]);
        println!(
            "    covenant context for that spend: {:?}",
            kaspa_txscript::covenants::CovenantsContext::from_tx(&populated).map(|_| "ok")
        );
    }

    println!("\n[I] Exact JSON field names emitted by upstream (what the TS client in src/api/kaspa.ts reads)");
    {
        let n = node.lock().unwrap();
        let dag = n.handle_ro_dag();
        println!(
            "    getBlockDagInfo keys: {:?}",
            dag.as_object().unwrap().keys().collect::<Vec<_>>()
        );
        let any_block = n.blocks.values().next().unwrap().clone();
        let bj = serde_json::to_value(GetBlockResponse { block: any_block }).unwrap();
        println!(
            "    getBlock.block.header keys: {:?}",
            bj["block"]["header"]
                .as_object()
                .unwrap()
                .keys()
                .collect::<Vec<_>>()
        );
        println!(
            "    header.blueWork sample: {:?}",
            bj["block"]["header"]["blueWork"]
        );
        println!(
            "    getBlock.block.verboseData keys: {:?}",
            bj["block"]["verboseData"]
                .as_object()
                .unwrap()
                .keys()
                .collect::<Vec<_>>()
        );
    }

    println!("\n    node log:");
    for (m, o) in node.lock().unwrap().log.iter() {
        println!("      {m:<22} {o}");
    }

    if std::env::var("RUN_SIM").is_ok() {
        println!("\n[J] wait_for_confirmation detection rate at 10 BPS (blocks every 100 ms; a block stays a DAG tip for TIP ms;");
        println!("    tx included INCL ms after submission; RTT jittered x0.5..1.5; each trial times out after 6 s)");
        for incl in [1_000u64] {
            for tip_life in [300u64, 600] {
                for rtt in [20u64, 50, 80, 120, 160] {
                    let (ok, n) = simulate_confirmation(rtt, tip_life, incl, 20).await;
                    println!("    INCL={incl:>5} ms  TIP={tip_life:>4} ms  RTT={rtt:>4} ms  -> confirmed {ok:>2}/{n}");
                }
            }
        }
    }
}
