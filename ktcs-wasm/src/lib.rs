//! KTCS WebAssembly Bindings
//!
//! Provides JavaScript-accessible functions for:
//! - Verifying and parsing KTCS proofs
//! - Wallet creation and address derivation
//! - Transaction building and signing
//! - Direct stamping in the browser

use ktcs_core::{
    build_commitment, compute_kaspa_sighash, decode_address, deserialize_proof,
    kaspa_types::{ScriptPublicKey, Transaction, Utxo},
    merkle::sha256,
    select_utxos_for_tx, serialize_proof,
    tx_builder::TransactionBuilder,
    verify_proof as core_verify, Attestation, KaspaAttestation, KaspaWallet, KtcsProof, Operation,
    SigHashType, SighashInput, SighashOutput, SighashTransaction, COMMITMENT_BURN_AMOUNT,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use zeroize::Zeroize;

/// Initialize panic hook for better error messages in console
#[wasm_bindgen(start)]
pub fn init() {
    #[cfg(feature = "panic_hook")]
    console_error_panic_hook::set_once();
}

/// Result type returned to JavaScript
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WasmVerificationResult {
    pub valid: bool,
    pub digest: String,
    pub computed_commitment: String,
    pub attestations: Vec<WasmAttestationInfo>,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WasmAttestationInfo {
    pub attestation_type: String,
    pub complete: bool,
    pub daa_score: Option<u64>,
    pub blue_score: Option<u64>,
    pub block_hash: Option<String>,
    pub timestamp: Option<u64>,
    pub tx_hash: Option<String>,
    pub calendar_url: Option<String>,
}

/// Proof info returned when parsing
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WasmProofInfo {
    pub version: u8,
    pub hash_algorithm: String,
    pub digest: String,
    pub operations_count: usize,
    pub attestations_count: usize,
    pub is_complete: bool,
    pub attestations: Vec<WasmAttestationInfo>,
}

/// Verify a KTCS proof
///
/// # Arguments
/// * `proof_bytes` - The raw .kts proof file bytes
/// * `data` - Optional original data for full verification
///
/// # Returns
/// A JsValue containing the verification result
#[wasm_bindgen]
pub fn verify_proof(proof_bytes: &[u8], data: Option<Vec<u8>>) -> JsValue {
    let result = verify_proof_internal(proof_bytes, data.as_deref());
    serde_wasm_bindgen::to_value(&result).unwrap_or(JsValue::NULL)
}

fn verify_proof_internal(proof_bytes: &[u8], data: Option<&[u8]>) -> WasmVerificationResult {
    // Deserialize proof
    let proof = match deserialize_proof(proof_bytes) {
        Ok(p) => p,
        Err(e) => {
            return WasmVerificationResult {
                valid: false,
                digest: String::new(),
                computed_commitment: String::new(),
                attestations: Vec::new(),
                error: Some(format!("Failed to parse proof: {}", e)),
            };
        }
    };

    // Verify
    let result = match core_verify(&proof, data) {
        Ok(r) => r,
        Err(e) => {
            return WasmVerificationResult {
                valid: false,
                digest: String::new(),
                computed_commitment: String::new(),
                attestations: Vec::new(),
                error: Some(format!("Verification error: {}", e)),
            };
        }
    };

    // Convert to WASM result
    let attestations: Vec<WasmAttestationInfo> = result
        .attestations
        .iter()
        .map(|a| {
            let mut info = WasmAttestationInfo {
                attestation_type: a.attestation_type.clone(),
                complete: a.complete,
                daa_score: None,
                blue_score: None,
                block_hash: None,
                timestamp: None,
                tx_hash: None,
                calendar_url: None,
            };

            match &a.details {
                ktcs_core::verify::AttestationDetails::Pending { calendar_url } => {
                    info.calendar_url = Some(calendar_url.clone());
                }
                ktcs_core::verify::AttestationDetails::Kaspa {
                    daa_score,
                    blue_score,
                    block_hash,
                    timestamp,
                    tx_hash,
                    ..
                } => {
                    info.daa_score = Some(*daa_score);
                    info.blue_score = Some(*blue_score);
                    info.block_hash = Some(block_hash.clone());
                    info.timestamp = Some(*timestamp);
                    info.tx_hash = Some(tx_hash.clone());
                }
                ktcs_core::verify::AttestationDetails::Bitcoin { .. } => {}
            }

            info
        })
        .collect();

    WasmVerificationResult {
        valid: result.valid,
        digest: result.digest,
        computed_commitment: result.computed_commitment,
        attestations,
        error: result.error,
    }
}

/// Parse a KTCS proof and return its information
///
/// # Arguments
/// * `proof_bytes` - The raw .kts proof file bytes
///
/// # Returns
/// A JsValue containing the proof info
#[wasm_bindgen]
pub fn parse_proof(proof_bytes: &[u8]) -> JsValue {
    let result = parse_proof_internal(proof_bytes);
    serde_wasm_bindgen::to_value(&result).unwrap_or(JsValue::NULL)
}

fn parse_proof_internal(proof_bytes: &[u8]) -> Result<WasmProofInfo, String> {
    let proof = deserialize_proof(proof_bytes).map_err(|e| format!("Parse error: {}", e))?;

    let attestations: Vec<WasmAttestationInfo> = proof
        .attestations
        .iter()
        .map(|a| match a {
            ktcs_core::Attestation::Pending(p) => WasmAttestationInfo {
                attestation_type: "pending".to_string(),
                complete: false,
                daa_score: None,
                blue_score: None,
                block_hash: None,
                timestamp: None,
                tx_hash: None,
                calendar_url: Some(p.calendar_url.clone()),
            },
            ktcs_core::Attestation::Kaspa(ka) => WasmAttestationInfo {
                attestation_type: "kaspa".to_string(),
                complete: true,
                daa_score: Some(ka.daa_score),
                blue_score: Some(ka.blue_score),
                block_hash: Some(hex::encode(ka.block_hash)),
                timestamp: Some(ka.timestamp),
                tx_hash: Some(hex::encode(ka.tx_hash)),
                calendar_url: None,
            },
            ktcs_core::Attestation::Bitcoin(_btc) => WasmAttestationInfo {
                attestation_type: "bitcoin".to_string(),
                complete: true,
                daa_score: None,
                blue_score: None,
                block_hash: None,
                timestamp: None,
                tx_hash: None,
                calendar_url: None,
            },
        })
        .collect();

    Ok(WasmProofInfo {
        version: proof.version,
        hash_algorithm: format!("{:?}", proof.hash_algorithm),
        digest: hex::encode(&proof.digest),
        operations_count: proof.operations.len(),
        attestations_count: proof.attestations.len(),
        is_complete: proof.is_complete(),
        attestations,
    })
}

/// Serialize a proof to .kts format
///
/// This is primarily for testing - proofs are normally created by the calendar server
#[wasm_bindgen]
pub fn serialize_proof_to_bytes(proof_json: JsValue) -> Result<Vec<u8>, JsValue> {
    let proof: KtcsProof = serde_wasm_bindgen::from_value(proof_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid proof JSON: {}", e)))?;

    Ok(serialize_proof(&proof))
}

/// Compute SHA256 hash of data
///
/// # Arguments
/// * `data` - The data to hash
///
/// # Returns
/// 32-byte SHA256 hash as Uint8Array
#[wasm_bindgen]
pub fn compute_sha256(data: &[u8]) -> Vec<u8> {
    sha256(data).to_vec()
}

/// Compute SHA256 hash and return as hex string
#[wasm_bindgen]
pub fn compute_sha256_hex(data: &[u8]) -> String {
    hex::encode(sha256(data))
}

/// Get the KTCS library version
#[wasm_bindgen]
pub fn get_version() -> String {
    ktcs_core::VERSION.to_string()
}

/// Check if proof bytes appear to be a valid KTCS proof (quick check)
#[wasm_bindgen]
pub fn is_valid_proof_format(proof_bytes: &[u8]) -> bool {
    if proof_bytes.len() < 18 {
        return false;
    }
    &proof_bytes[0..18] == ktcs_core::KTCS_MAGIC
}

// =============================================================================
// WALLET FUNCTIONS
// =============================================================================

/// Wallet info returned to JavaScript
#[derive(Serialize, Deserialize)]
pub struct WasmWalletInfo {
    pub address: String,
    pub public_key: String,
    pub network: String,
}

/// Get wallet address from a private key
///
/// # Arguments
/// * `private_key_hex` - 64-character hex string (32 bytes)
/// * `network` - "mainnet" or "testnet"
///
/// # Returns
/// Kaspa address string or error
#[wasm_bindgen]
pub fn get_wallet_address(private_key_hex: &str, network: &str) -> Result<String, JsValue> {
    // Decode key into mutable buffer for zeroization
    let mut key_bytes = hex::decode(private_key_hex)
        .map_err(|e| JsValue::from_str(&format!("Invalid hex: {}", e)))?;

    if key_bytes.len() != 32 {
        key_bytes.zeroize();
        return Err(JsValue::from_str("Private key must be 32 bytes (64 hex chars)"));
    }

    let mut key_array = [0u8; 32];
    key_array.copy_from_slice(&key_bytes);
    key_bytes.zeroize(); // Clear Vec immediately after copying

    // Use closure to ensure cleanup even on error
    let result = (|| {
        let wallet = KaspaWallet::from_private_key(&key_array, network)
            .map_err(|e| JsValue::from_str(&format!("Failed to create wallet: {}", e)))?;
        Ok(wallet.address().to_string())
    })();

    key_array.zeroize(); // Clear array after use
    result
}

/// Get wallet info (address and public key)
///
/// # Arguments
/// * `private_key_hex` - 64-character hex string (32 bytes)
/// * `network` - "mainnet" or "testnet"
///
/// # Returns
/// WasmWalletInfo as JsValue
#[wasm_bindgen]
pub fn get_wallet_info(private_key_hex: &str, network: &str) -> Result<JsValue, JsValue> {
    // Decode key into mutable buffer for zeroization
    let mut key_bytes = hex::decode(private_key_hex)
        .map_err(|e| JsValue::from_str(&format!("Invalid hex: {}", e)))?;

    if key_bytes.len() != 32 {
        key_bytes.zeroize();
        return Err(JsValue::from_str("Private key must be 32 bytes (64 hex chars)"));
    }

    let mut key_array = [0u8; 32];
    key_array.copy_from_slice(&key_bytes);
    key_bytes.zeroize(); // Clear Vec immediately after copying

    // Use closure to ensure cleanup even on error
    let result = (|| {
        let wallet = KaspaWallet::from_private_key(&key_array, network)
            .map_err(|e| JsValue::from_str(&format!("Failed to create wallet: {}", e)))?;

        let info = WasmWalletInfo {
            address: wallet.address().to_string(),
            public_key: hex::encode(wallet.public_key()),
            network: network.to_string(),
        };

        serde_wasm_bindgen::to_value(&info).map_err(|e| JsValue::from_str(&format!("Serialize error: {}", e)))
    })();

    key_array.zeroize(); // Clear array after use
    result
}

/// Validate a private key format
#[wasm_bindgen]
pub fn validate_private_key(private_key_hex: &str) -> bool {
    if let Ok(mut bytes) = hex::decode(private_key_hex) {
        let valid = bytes.len() == 32;
        bytes.zeroize(); // Clear key material from memory
        valid
    } else {
        false
    }
}

/// Validate a Kaspa address
#[wasm_bindgen]
pub fn validate_address(address: &str) -> bool {
    decode_address(address).is_ok()
}

// =============================================================================
// TRANSACTION BUILDING
// =============================================================================

/// UTXO info from JavaScript
#[derive(Serialize, Deserialize)]
pub struct WasmUtxo {
    pub transaction_id: String,  // hex
    pub index: u32,
    pub amount: String,          // sompi as string to preserve precision
    pub script_public_key_hex: String,
    pub block_daa_score: u64,
    pub is_coinbase: bool,
}

/// Built transaction result
#[derive(Serialize, Deserialize)]
pub struct WasmTransactionResult {
    pub transaction_json: String,
    pub commitment: String,       // hex
    pub total_input: u64,
    pub total_output: u64,
    pub fee: u64,
    pub change_amount: u64,
}

/// Create a commitment from nonce and hash
///
/// commitment = SHA256(nonce || hash)
///
/// # Arguments
/// * `nonce_hex` - Hex-encoded nonce (typically 16 bytes)
/// * `hash_hex` - Hex-encoded document hash (32 bytes)
///
/// # Returns
/// 32-byte commitment as hex string
#[wasm_bindgen]
pub fn create_commitment(nonce_hex: &str, hash_hex: &str) -> Result<String, JsValue> {
    let nonce = hex::decode(nonce_hex)
        .map_err(|e| JsValue::from_str(&format!("Invalid nonce hex: {}", e)))?;

    let hash_bytes = hex::decode(hash_hex)
        .map_err(|e| JsValue::from_str(&format!("Invalid hash hex: {}", e)))?;

    if hash_bytes.len() != 32 {
        return Err(JsValue::from_str("Hash must be 32 bytes"));
    }

    let mut hash_array = [0u8; 32];
    hash_array.copy_from_slice(&hash_bytes);

    let commitment = build_commitment(&nonce, &hash_array);
    Ok(hex::encode(commitment))
}

/// Generate a random nonce (16 bytes)
///
/// Uses browser's crypto.getRandomValues via getrandom
#[wasm_bindgen]
pub fn generate_nonce() -> Result<String, JsValue> {
    let mut nonce = [0u8; 16];
    getrandom::getrandom(&mut nonce)
        .map_err(|e| JsValue::from_str(&format!("Failed to generate random: {}", e)))?;
    Ok(hex::encode(nonce))
}

/// Get the burn amount for commitment transactions
#[wasm_bindgen]
pub fn get_commitment_burn_amount() -> u64 {
    COMMITMENT_BURN_AMOUNT
}

/// Build a commitment transaction
///
/// # Arguments
/// * `commitment_hex` - 32-byte commitment as hex
/// * `utxos_json` - JSON array of WasmUtxo
/// * `change_address` - Kaspa address for change
/// * `fee_per_gram` - Fee rate in sompi per gram
///
/// # Returns
/// WasmTransactionResult as JsValue
#[wasm_bindgen]
pub fn build_commitment_transaction(
    commitment_hex: &str,
    utxos_json: &str,
    change_address: &str,
    fee_per_gram: u64,
) -> Result<JsValue, JsValue> {
    // Parse commitment
    let commitment_bytes = hex::decode(commitment_hex)
        .map_err(|e| JsValue::from_str(&format!("Invalid commitment hex: {}", e)))?;

    if commitment_bytes.len() != 32 {
        return Err(JsValue::from_str("Commitment must be 32 bytes"));
    }

    let mut commitment = [0u8; 32];
    commitment.copy_from_slice(&commitment_bytes);

    // Parse UTXOs
    let wasm_utxos: Vec<WasmUtxo> = serde_json::from_str(utxos_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid UTXOs JSON: {}", e)))?;

    let utxos: Result<Vec<Utxo>, JsValue> = wasm_utxos
        .into_iter()
        .map(|u| {
            let tx_id = hex::decode(&u.transaction_id)
                .map_err(|e| JsValue::from_str(&format!("Invalid tx_id hex: {}", e)))?;
            if tx_id.len() != 32 {
                return Err(JsValue::from_str("Transaction ID must be 32 bytes"));
            }
            let mut tx_id_array = [0u8; 32];
            tx_id_array.copy_from_slice(&tx_id);

            let script = hex::decode(&u.script_public_key_hex)
                .map_err(|e| JsValue::from_str(&format!("Invalid script hex: {}", e)))?;

            // Parse amount from string to preserve precision for large values
            let amount: u64 = u.amount.parse()
                .map_err(|e| JsValue::from_str(&format!("Invalid amount: {}", e)))?;

            Ok(Utxo {
                transaction_id: tx_id_array,
                index: u.index,
                amount,
                script_public_key: ScriptPublicKey {
                    version: 0,
                    script,
                },
                block_daa_score: u.block_daa_score,
                is_coinbase: u.is_coinbase,
            })
        })
        .collect();

    let utxos = utxos?;

    // Select UTXOs to cover commitment burn + fee
    let (selected_utxos, _total) = select_utxos_for_tx(&utxos, COMMITMENT_BURN_AMOUNT, fee_per_gram)
        .map_err(|e| JsValue::from_str(&format!("UTXO selection failed: {}", e)))?;

    // Build transaction
    let tx_result = TransactionBuilder::new()
        .commitment(&commitment)
        .add_inputs(selected_utxos)
        .map_err(|e| JsValue::from_str(&format!("Failed to add inputs: {}", e)))?
        .change_address(change_address)
        .fee_per_gram(fee_per_gram)
        .build()
        .map_err(|e| JsValue::from_str(&format!("Transaction build failed: {}", e)))?;

    // Serialize transaction to JSON for later signing
    let tx_json = serde_json::to_string(&tx_result.transaction)
        .map_err(|e| JsValue::from_str(&format!("Serialize error: {}", e)))?;

    let result = WasmTransactionResult {
        transaction_json: tx_json,
        commitment: hex::encode(commitment),
        total_input: tx_result.total_input,
        total_output: tx_result.total_output,
        fee: tx_result.fee,
        change_amount: tx_result.change_amount,
    };

    serde_wasm_bindgen::to_value(&result).map_err(|e| JsValue::from_str(&format!("Serialize error: {}", e)))
}

// =============================================================================
// TRANSACTION SIGNING
// =============================================================================

/// Signed transaction result
#[derive(Serialize, Deserialize)]
pub struct WasmSignedTransaction {
    pub transaction_json: String,
    pub transaction_id: String,  // hex
}

/// Sign a transaction
///
/// # Arguments
/// * `unsigned_tx_json` - JSON serialized Transaction from build_commitment_transaction
/// * `utxos_json` - JSON array of WasmUtxo (same UTXOs used to build the transaction)
/// * `private_key_hex` - Wallet private key
/// * `network` - "mainnet" or "testnet"
///
/// # Returns
/// WasmSignedTransaction with the signed transaction ready for submission
#[wasm_bindgen]
pub fn sign_transaction(
    unsigned_tx_json: &str,
    utxos_json: &str,
    private_key_hex: &str,
    network: &str,
) -> Result<JsValue, JsValue> {
    // Decode key into mutable buffer for zeroization
    let mut key_bytes = hex::decode(private_key_hex)
        .map_err(|e| JsValue::from_str(&format!("Invalid private key hex: {}", e)))?;

    if key_bytes.len() != 32 {
        key_bytes.zeroize();
        return Err(JsValue::from_str("Private key must be 32 bytes"));
    }

    let mut key_array = [0u8; 32];
    key_array.copy_from_slice(&key_bytes);
    key_bytes.zeroize(); // Clear Vec immediately after copying

    // Use closure to ensure cleanup even on error
    let result = (|| {
        let wallet = KaspaWallet::from_private_key(&key_array, network)
            .map_err(|e| JsValue::from_str(&format!("Failed to create wallet: {}", e)))?;

        // Parse transaction
        let mut tx: Transaction = serde_json::from_str(unsigned_tx_json)
            .map_err(|e| JsValue::from_str(&format!("Invalid transaction JSON: {}", e)))?;

        // Parse UTXOs
        let wasm_utxos: Vec<WasmUtxo> = serde_json::from_str(utxos_json)
            .map_err(|e| JsValue::from_str(&format!("Invalid UTXOs JSON: {}", e)))?;

        // Convert to Utxo structs
        let utxos: Result<Vec<Utxo>, JsValue> = wasm_utxos
            .into_iter()
            .map(|u| {
                let tx_id = hex::decode(&u.transaction_id)
                    .map_err(|e| JsValue::from_str(&format!("Invalid tx_id hex: {}", e)))?;
                if tx_id.len() != 32 {
                    return Err(JsValue::from_str("Transaction ID must be 32 bytes"));
                }
                let mut tx_id_array = [0u8; 32];
                tx_id_array.copy_from_slice(&tx_id);

                let script = hex::decode(&u.script_public_key_hex)
                    .map_err(|e| JsValue::from_str(&format!("Invalid script hex: {}", e)))?;

                // Parse amount from string to preserve precision for large values
                let amount: u64 = u.amount.parse()
                    .map_err(|e| JsValue::from_str(&format!("Invalid amount: {}", e)))?;

                Ok(Utxo {
                    transaction_id: tx_id_array,
                    index: u.index,
                    amount,
                    script_public_key: ScriptPublicKey {
                        version: 0,
                        script,
                    },
                    block_daa_score: u.block_daa_score,
                    is_coinbase: u.is_coinbase,
                })
            })
            .collect();

        let utxos = utxos?;

        // First, compute all signatures (borrowing tx immutably)
        let mut signatures: Vec<Vec<u8>> = Vec::with_capacity(tx.inputs.len());

        for i in 0..tx.inputs.len() {
            // Build sighash transaction structure
            let sighash_tx = build_sighash_transaction(&tx, &utxos)?;

            // Compute sighash
            let sighash = compute_kaspa_sighash(&sighash_tx, i, SigHashType::All)
                .map_err(|e| JsValue::from_str(&format!("Sighash computation failed: {}", e)))?;

            // Sign with wallet
            let signature = wallet
                .sign(&sighash)
                .map_err(|e| JsValue::from_str(&format!("Signing failed: {}", e)))?;

            // Build signature script for P2PK: only <signature with sighash type>
            // The pubkey is already in the scriptPubKey, so we don't include it here
            let mut sig_script = Vec::with_capacity(66);
            sig_script.push(65); // Push 65 bytes (64-byte sig + 1-byte sighash type)
            sig_script.extend_from_slice(&signature);
            sig_script.push(SigHashType::All as u8); // Sighash type byte

            signatures.push(sig_script);
        }

        // Now apply signatures to transaction (borrowing tx mutably)
        for (input, sig_script) in tx.inputs.iter_mut().zip(signatures.into_iter()) {
            input.signature_script = sig_script;
        }

        // Compute transaction ID (hash of signed transaction)
        let tx_id = compute_transaction_id(&tx);

        let signed_tx_json = serde_json::to_string(&tx)
            .map_err(|e| JsValue::from_str(&format!("Serialize error: {}", e)))?;

        let result = WasmSignedTransaction {
            transaction_json: signed_tx_json,
            transaction_id: hex::encode(tx_id),
        };

        serde_wasm_bindgen::to_value(&result).map_err(|e| JsValue::from_str(&format!("Serialize error: {}", e)))
    })();

    key_array.zeroize(); // Clear array after use
    result
}

/// Build sighash transaction structure for signing
fn build_sighash_transaction(tx: &Transaction, utxos: &[Utxo]) -> Result<SighashTransaction, JsValue> {
    let inputs: Result<Vec<SighashInput>, JsValue> = tx
        .inputs
        .iter()
        .map(|inp| {
            let utxo = utxos
                .iter()
                .find(|u| {
                    u.transaction_id == inp.previous_outpoint_hash
                        && u.index == inp.previous_outpoint_index
                })
                .ok_or_else(|| {
                    JsValue::from_str(&format!(
                        "UTXO not found for input: {}:{}",
                        hex::encode(inp.previous_outpoint_hash),
                        inp.previous_outpoint_index
                    ))
                })?;

            Ok(SighashInput {
                previous_outpoint_hash: inp.previous_outpoint_hash,
                previous_outpoint_index: inp.previous_outpoint_index,
                script_public_key_version: utxo.script_public_key.version,
                script_public_key: utxo.script_public_key.script.clone(),
                value: utxo.amount,
                // The WASM `Transaction`/`TransactionInput` types carry no
                // per-input sequence or sig_op_count, so these are assumed
                // constant. They MUST match `create_submit_tx_rpc_request`
                // (same u64::MAX / 1) and `compute_transaction_id`
                // (ASSUMED_INPUT_SEQUENCE), otherwise the sighash, the
                // submitted tx, and the computed txid would disagree.
                sequence: u64::MAX,
                sig_op_count: 1, // P2PK single-signature input
            })
        })
        .collect();

    let inputs = inputs?;

    let outputs: Vec<SighashOutput> = tx
        .outputs
        .iter()
        .map(|out| SighashOutput {
            value: out.amount,
            script_public_key_version: out.script_public_key.version,
            script_public_key: out.script_public_key.script.clone(),
        })
        .collect();

    Ok(SighashTransaction {
        version: tx.version,
        inputs,
        outputs,
        lock_time: tx.lock_time,
        subnetwork_id: tx.subnetwork_id,
        gas: tx.gas,
        payload: tx.payload.clone(),
    })
}

/// Domain-separation key for the Kaspa transaction *ID* hash.
///
/// Kaspa hashes are keyed Blake2b-256 where the domain string is the Blake2b
/// key (see rusty-kaspa `kaspa_hashes`: `TransactionID => b"TransactionID"`).
/// This mirrors `ktcs_core::wallet`'s sighash hasher, which uses the same
/// construction with the key `b"TransactionSigningHash"`.
const TRANSACTION_ID_KEY: &[u8] = b"TransactionID";

/// The `sequence` value assumed for every input.
///
/// Kaspa's transaction ID commits to each input's `sequence`, but the WASM
/// [`Transaction`] type carries no per-input sequence field. KTCS builds and
/// submits every input with `sequence = u64::MAX` (see the sighash builder in
/// `build_sighash_transaction` and the RPC encoder in
/// `create_submit_tx_rpc_request`), so the ID must be computed with the same
/// constant to match the txid the node will assign. If the transaction type
/// ever gains a real per-input sequence, thread it through here instead.
const ASSUMED_INPUT_SEQUENCE: u64 = u64::MAX;

/// Compute the real Kaspa transaction ID.
///
/// Kaspa computes the transaction ID as a keyed Blake2b-256 hash (domain key
/// `"TransactionID"`) over the canonical consensus serialization of the
/// transaction, with each input's signature script treated as EMPTY (the ID
/// deliberately excludes signatures — the separate transaction *hash* is what
/// includes them). This follows rusty-kaspa's `hashing::tx::id`/`id_v0` for
/// version-0 transactions (`EXCLUDE_SIGNATURE_SCRIPT | EXCLUDE_MASS_COMMIT`),
/// which is the only version KTCS ever builds.
///
/// The byte layout is verified against rusty-kaspa's own known-answer vectors
/// in the unit tests below.
///
/// NOTE: this is the version-0 encoding. KTCS never constructs version >= 1
/// transactions (`Transaction::new()` fixes `version = 0`); those use a
/// different two-part digest and are intentionally not handled here.
fn compute_transaction_id(tx: &Transaction) -> [u8; 32] {
    compute_transaction_id_with_sequence(tx, ASSUMED_INPUT_SEQUENCE)
}

/// Inner txid computation with an explicit per-input `sequence`.
///
/// Split out from [`compute_transaction_id`] so the known-answer tests can
/// reproduce rusty-kaspa's fixtures (which use non-`u64::MAX` sequences)
/// against the exact same encoding used in production.
fn compute_transaction_id_with_sequence(tx: &Transaction, sequence: u64) -> [u8; 32] {
    let mut hasher = blake2b_simd::Params::new()
        .hash_length(32)
        .key(TRANSACTION_ID_KEY)
        .to_state();

    // version (u16 LE)
    hasher.update(&tx.version.to_le_bytes());

    // number of inputs (u64 LE)
    hasher.update(&(tx.inputs.len() as u64).to_le_bytes());
    for input in &tx.inputs {
        // previous outpoint: transaction_id (32 bytes) + index (u32 LE)
        hasher.update(&input.previous_outpoint_hash);
        hasher.update(&input.previous_outpoint_index.to_le_bytes());
        // signature script EXCLUDED from the ID: write a zero-length var-bytes
        // field (length u64 LE = 0, no bytes). sig_op_count is likewise not part
        // of the ID's exclude-signature-script encoding.
        hasher.update(&0u64.to_le_bytes());
        // sequence (u64 LE)
        hasher.update(&sequence.to_le_bytes());
    }

    // number of outputs (u64 LE)
    hasher.update(&(tx.outputs.len() as u64).to_le_bytes());
    for output in &tx.outputs {
        // value (u64 LE)
        hasher.update(&output.amount.to_le_bytes());
        // script_public_key: version (u16 LE) + script length (u64 LE) + script
        hasher.update(&output.script_public_key.version.to_le_bytes());
        hasher.update(&(output.script_public_key.script.len() as u64).to_le_bytes());
        hasher.update(&output.script_public_key.script);
    }

    // lock_time (u64 LE)
    hasher.update(&tx.lock_time.to_le_bytes());
    // subnetwork_id (20 bytes)
    hasher.update(&tx.subnetwork_id);
    // gas (u64 LE)
    hasher.update(&tx.gas.to_le_bytes());
    // payload as var-bytes: length (u64 LE) + payload. The mass-commitment field
    // is EXCLUDED from the ID.
    hasher.update(&(tx.payload.len() as u64).to_le_bytes());
    hasher.update(&tx.payload);

    let mut out = [0u8; 32];
    out.copy_from_slice(hasher.finalize().as_bytes());
    out
}

// =============================================================================
// PROOF BUILDING
// =============================================================================

/// Pending proof info
#[derive(Serialize, Deserialize)]
pub struct WasmPendingProof {
    pub digest: String,        // hex
    pub nonce: String,         // hex
    pub commitment: String,    // hex
    pub proof_bytes: Vec<u8>,  // Serialized proof without attestation
}

/// Build a pending proof (before blockchain confirmation)
///
/// # Arguments
/// * `digest_hex` - 32-byte document hash as hex
/// * `nonce_hex` - Nonce used for commitment
///
/// # Returns
/// WasmPendingProof with partial proof data
#[wasm_bindgen]
pub fn build_pending_proof(digest_hex: &str, nonce_hex: &str) -> Result<JsValue, JsValue> {
    let digest_bytes = hex::decode(digest_hex)
        .map_err(|e| JsValue::from_str(&format!("Invalid digest hex: {}", e)))?;

    if digest_bytes.len() != 32 {
        return Err(JsValue::from_str("Digest must be 32 bytes"));
    }

    let nonce = hex::decode(nonce_hex)
        .map_err(|e| JsValue::from_str(&format!("Invalid nonce hex: {}", e)))?;

    let mut digest_array = [0u8; 32];
    digest_array.copy_from_slice(&digest_bytes);

    let commitment = build_commitment(&nonce, &digest_array);

    // Create proof with operations
    let mut proof = KtcsProof::new(digest_bytes);
    proof.add_operation(Operation::Prepend(nonce.clone()));
    proof.add_operation(Operation::Sha256);

    // Serialize (without attestation yet)
    let proof_bytes = serialize_proof(&proof);

    let result = WasmPendingProof {
        digest: digest_hex.to_string(),
        nonce: nonce_hex.to_string(),
        commitment: hex::encode(commitment),
        proof_bytes,
    };

    serde_wasm_bindgen::to_value(&result).map_err(|e| JsValue::from_str(&format!("Serialize error: {}", e)))
}

/// Block attestation info from JavaScript
#[derive(Serialize, Deserialize)]
pub struct WasmBlockAttestation {
    pub tx_hash: String,           // hex
    pub block_hash: String,        // hex
    pub daa_score: u64,
    pub blue_score: u64,
    pub timestamp: u64,            // Unix milliseconds
    pub blue_work: String,         // hex
    pub parent_hashes: Vec<String>, // hex array
}

/// Complete a proof with blockchain attestation
///
/// # Arguments
/// * `pending_proof_bytes` - The proof_bytes from build_pending_proof
/// * `attestation_json` - JSON of WasmBlockAttestation
///
/// # Returns
/// Complete .kts proof bytes
#[wasm_bindgen]
pub fn complete_proof(
    pending_proof_bytes: &[u8],
    attestation_json: &str,
) -> Result<Vec<u8>, JsValue> {
    // Parse the pending proof
    let mut proof = deserialize_proof(pending_proof_bytes)
        .map_err(|e| JsValue::from_str(&format!("Invalid proof: {}", e)))?;

    // Parse attestation
    let att: WasmBlockAttestation = serde_json::from_str(attestation_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid attestation JSON: {}", e)))?;

    // Parse hex values
    let tx_hash = hex::decode(&att.tx_hash)
        .map_err(|e| JsValue::from_str(&format!("Invalid tx_hash hex: {}", e)))?;
    let block_hash = hex::decode(&att.block_hash)
        .map_err(|e| JsValue::from_str(&format!("Invalid block_hash hex: {}", e)))?;
    let blue_work_bytes = hex::decode(&att.blue_work)
        .map_err(|e| JsValue::from_str(&format!("Invalid blue_work hex: {}", e)))?;

    if tx_hash.len() != 32 || block_hash.len() != 32 {
        return Err(JsValue::from_str("tx_hash and block_hash must be 32 bytes"));
    }
    if blue_work_bytes.len() > 32 {
        return Err(JsValue::from_str("blue_work cannot exceed 32 bytes"));
    }

    let mut tx_hash_arr = [0u8; 32];
    let mut block_hash_arr = [0u8; 32];
    let mut blue_work_arr = [0u8; 32];
    tx_hash_arr.copy_from_slice(&tx_hash);
    block_hash_arr.copy_from_slice(&block_hash);
    // Left-pad blue_work with zeros (it's a big-endian 256-bit integer)
    let start = 32 - blue_work_bytes.len();
    blue_work_arr[start..].copy_from_slice(&blue_work_bytes);

    let parent_hashes: Result<Vec<[u8; 32]>, JsValue> = att
        .parent_hashes
        .iter()
        .map(|h| {
            let bytes = hex::decode(h)
                .map_err(|e| JsValue::from_str(&format!("Invalid parent hash hex: {}", e)))?;
            if bytes.len() != 32 {
                return Err(JsValue::from_str("Parent hash must be 32 bytes"));
            }
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            Ok(arr)
        })
        .collect();

    let parent_hashes = parent_hashes?;

    // Create Kaspa attestation
    let kaspa_attestation = KaspaAttestation::new(
        att.daa_score,
        att.blue_score,
        block_hash_arr,
        att.timestamp,
        tx_hash_arr,
        0, // tx_index - not strictly needed for verification
        blue_work_arr,
        parent_hashes,
    );

    // Add attestation to proof
    proof.add_attestation(Attestation::Kaspa(kaspa_attestation));

    // Serialize complete proof
    Ok(serialize_proof(&proof))
}

// =============================================================================
// RPC REQUEST BUILDING
// =============================================================================

/// RPC transaction format for Kaspa node submission
/// Note: u64 values are serialized as numbers (not strings) — Rust serde
/// handles precision correctly, and sendRawRequest bypasses JavaScript.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RpcTransaction {
    version: u16,
    inputs: Vec<RpcInput>,
    outputs: Vec<RpcOutput>,
    lock_time: u64,
    subnetwork_id: String,
    gas: u64,
    payload: String,
    mass: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RpcInput {
    previous_outpoint: RpcOutpoint,
    signature_script: String,
    sequence: u64,
    sig_op_count: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RpcOutpoint {
    transaction_id: String,
    index: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RpcOutput {
    value: u64,
    /// Flat hex string: version (2 bytes BE) + script, matching Kaspa wRPC format
    script_public_key: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RpcSubmitRequest {
    transaction: RpcTransaction,
    allow_orphan: bool,
}

#[derive(Serialize)]
struct JsonRpcRequest {
    jsonrpc: &'static str,
    id: u32,
    method: &'static str,
    params: RpcSubmitRequest,
}

/// Create complete JSON-RPC request for transaction submission
///
/// This allows JavaScript to just send the pre-formatted string without
/// parsing or modifying any BigInt values (avoiding precision loss).
///
/// # Arguments
/// * `signed_tx_json` - The transaction JSON from sign_transaction_for_direct_stamp
///
/// # Returns
/// Complete JSON-RPC request string ready to send to Kaspa node
#[wasm_bindgen]
pub fn create_submit_tx_rpc_request(signed_tx_json: &str) -> Result<String, JsValue> {
    let tx: Transaction = serde_json::from_str(signed_tx_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid transaction JSON: {}", e)))?;

    let rpc_tx = RpcTransaction {
        version: tx.version,
        inputs: tx.inputs.iter().map(|inp| RpcInput {
            previous_outpoint: RpcOutpoint {
                transaction_id: hex::encode(inp.previous_outpoint_hash),
                index: inp.previous_outpoint_index,
            },
            signature_script: hex::encode(&inp.signature_script),
            // Assumed constant: the WASM Transaction type has no per-input
            // sequence/sig_op_count. Kept identical to the sighash builder
            // (build_sighash_transaction) and the txid computation
            // (compute_transaction_id / ASSUMED_INPUT_SEQUENCE) so signing,
            // submission, and the txid all agree.
            sequence: u64::MAX,
            sig_op_count: 1, // P2PK single-signature input
        }).collect(),
        outputs: tx.outputs.iter().map(|out| {
            // ScriptPublicKey is encoded as: version (2 bytes BE) + script
            let mut spk_bytes = Vec::with_capacity(2 + out.script_public_key.script.len());
            spk_bytes.extend_from_slice(&out.script_public_key.version.to_be_bytes());
            spk_bytes.extend_from_slice(&out.script_public_key.script);
            RpcOutput {
                value: out.amount,
                script_public_key: hex::encode(spk_bytes),
            }
        }).collect(),
        lock_time: tx.lock_time,
        subnetwork_id: hex::encode(tx.subnetwork_id),
        gas: tx.gas,
        payload: hex::encode(&tx.payload),
        mass: 0,
    };

    let request = JsonRpcRequest {
        jsonrpc: "2.0",
        id: 1,
        method: "submitTransaction",
        params: RpcSubmitRequest {
            transaction: rpc_tx,
            allow_orphan: false,
        },
    };

    serde_json::to_string(&request)
        .map_err(|e| JsValue::from_str(&format!("Serialize error: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ktcs_core::{Attestation, KaspaAttestation, PendingAttestation};

    #[test]
    fn test_verify_proof() {
        let digest = sha256(b"test data");
        let mut proof = KtcsProof::new(digest.to_vec());
        proof.add_attestation(Attestation::Kaspa(KaspaAttestation::new(
            42000000,
            41500000,
            [0xab; 32],
            1706000000000,
            [0xcd; 32],
            0,
            [0x12; 32],
            vec![[0x11; 32]],
        )));

        let proof_bytes = serialize_proof(&proof);
        let result = verify_proof_internal(&proof_bytes, Some(b"test data"));

        assert!(result.valid);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_parse_proof() {
        let digest = sha256(b"test");
        let mut proof = KtcsProof::new(digest.to_vec());
        proof.add_attestation(Attestation::Pending(PendingAttestation {
            calendar_url: "https://example.com".to_string(),
        }));

        let proof_bytes = serialize_proof(&proof);
        let info = parse_proof_internal(&proof_bytes).unwrap();

        assert_eq!(info.version, 1);
        assert!(!info.is_complete);
        assert_eq!(info.attestations.len(), 1);
        assert_eq!(info.attestations[0].attestation_type, "pending");
    }

    #[test]
    fn test_sha256() {
        let hash = compute_sha256(b"hello world");
        let hex = compute_sha256_hex(b"hello world");

        assert_eq!(hash.len(), 32);
        assert_eq!(
            hex,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_is_valid_format() {
        let digest = [0u8; 32];
        let proof = KtcsProof::new(digest.to_vec());
        let bytes = serialize_proof(&proof);

        assert!(is_valid_proof_format(&bytes));
        assert!(!is_valid_proof_format(&[0u8; 10]));
        assert!(!is_valid_proof_format(&[0u8; 50]));
    }

    // -------------------------------------------------------------------------
    // Transaction ID known-answer tests.
    //
    // These vectors come directly from rusty-kaspa's own unit tests
    // (`consensus/core/src/hashing/tx.rs`, `test_transaction_hashing`). They
    // pin our version-0 txid encoding to Kaspa consensus byte-for-byte. If the
    // field order, sizes, endianness, or the Blake2b domain key ever drift,
    // these fail.
    //
    // The rusty-kaspa fixtures use `sequence = 7` per input, so they are
    // exercised through `compute_transaction_id_with_sequence`; production code
    // uses `compute_transaction_id`, which pins `sequence = u64::MAX` (the value
    // KTCS actually submits).
    // -------------------------------------------------------------------------

    fn tx_output(value: u64, spk_version: u16, script: Vec<u8>) -> ktcs_core::kaspa_types::TransactionOutput {
        ktcs_core::kaspa_types::TransactionOutput {
            amount: value,
            script_public_key: ScriptPublicKey { version: spk_version, script },
        }
    }

    #[test]
    fn test_transaction_id_kat_empty_native_tx() {
        // rusty-kaspa test #1: empty version-0 native transaction.
        let tx = Transaction {
            version: 0,
            inputs: vec![],
            outputs: vec![],
            lock_time: 0,
            subnetwork_id: [0u8; 20],
            gas: 0,
            payload: vec![],
        };
        // No inputs => sequence is irrelevant; the public entry point must match.
        let id = compute_transaction_id(&tx);
        assert_eq!(
            hex::encode(id),
            "2c18d5e59ca8fc4c23d9560da3bf738a8f40935c11c162017fbf2c907b7e665c"
        );
    }

    #[test]
    fn test_transaction_id_kat_full_native_tx() {
        // rusty-kaspa test #5: one input (outpoint index 2, sequence 7, a
        // signature script that the ID excludes), one output (value 1564, spk
        // version 7, script [1,2,3,4,5]), lock_time 54, native subnetwork,
        // gas 3, empty payload.
        let mut outpoint = [0u8; 32];
        hex::decode_to_slice(
            "59b3d6dc6cdc660c389c3fdb5704c48c598d279cdf1bab54182db586a4c95dd5",
            &mut outpoint,
        )
        .unwrap();

        let tx = Transaction {
            version: 0,
            inputs: vec![ktcs_core::kaspa_types::TransactionInput {
                previous_outpoint_hash: outpoint,
                previous_outpoint_index: 2,
                // A non-empty signature script must NOT affect the ID.
                signature_script: vec![0x01, 0x02],
            }],
            outputs: vec![tx_output(1564, 7, vec![1, 2, 3, 4, 5])],
            lock_time: 54,
            subnetwork_id: [0u8; 20],
            gas: 3,
            payload: vec![],
        };

        let id = compute_transaction_id_with_sequence(&tx, 7);
        assert_eq!(
            hex::encode(id),
            "c9dd78e818445f617a28348d6db752142e2fab440effa58140ad2773e638b628"
        );
    }

    #[test]
    fn test_transaction_id_excludes_signature_script() {
        // The ID must be independent of signature-script contents: signing an
        // input (or changing its signature) must not change the transaction ID.
        let mut base = Transaction {
            version: 0,
            inputs: vec![ktcs_core::kaspa_types::TransactionInput {
                previous_outpoint_hash: [0x11; 32],
                previous_outpoint_index: 0,
                signature_script: vec![],
            }],
            outputs: vec![tx_output(1000, 0, vec![0x20; 34])],
            lock_time: 0,
            subnetwork_id: [0u8; 20],
            gas: 0,
            payload: vec![],
        };
        let id_unsigned = compute_transaction_id(&base);

        base.inputs[0].signature_script = vec![0xde, 0xad, 0xbe, 0xef, 0x41, 0x42];
        let id_signed = compute_transaction_id(&base);

        assert_eq!(id_unsigned, id_signed, "signature script must not affect txid");
    }
}
