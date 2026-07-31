//! Direct Stamping Mode
//!
//! This module enables direct timestamping without a calendar server.
//! Users submit their own commitment transactions directly to the Kaspa network.
//!
//! Benefits:
//! - Fully self-sovereign (no trust in calendar)
//! - Immediate confirmation (no batching delay)
//!
//! Tradeoffs:
//! - Higher cost (one transaction per stamp)
//! - Requires wallet with funds

#[cfg(feature = "kaspa-client")]
use crate::kaspa::{BlockInfo, KaspaClient, Transaction, Utxo};
use crate::merkle::sha256;
#[cfg(feature = "kaspa-client")]
use crate::tx_builder::CommitmentTransaction;
// The hardened, WASM-compatible builder in `tx_builder` is now the single
// transaction-building path (the old unsafe `tx` module was removed).
use crate::tx_builder::{create_commitment, generate_nonce};
use crate::wallet::KaspaWallet;
#[cfg(feature = "kaspa-client")]
use crate::wallet::{
    compute_kaspa_sighash, SigHashType, SighashInput, SighashOutput, SighashTransaction,
};
use crate::{
    error::{KtcsError, Result},
    types::{Attestation, KaspaAttestation, KtcsProof, Operation},
};

/// Configuration for direct stamping
#[derive(Clone, Debug)]
pub struct DirectStampConfig {
    /// Fee rate in sompi per gram
    pub fee_rate: u64,
    /// Maximum transaction size in bytes
    pub max_tx_size: u64,
    /// Timeout for confirmation in seconds
    pub confirmation_timeout_secs: u64,
}

impl Default for DirectStampConfig {
    fn default() -> Self {
        Self {
            fee_rate: 10, // 10 sompi per gram (reasonable default)
            max_tx_size: 10_000,
            confirmation_timeout_secs: 60,
        }
    }
}

/// Result of a direct stamp operation
#[derive(Clone, Debug)]
pub struct DirectStampResult {
    /// The complete proof with attestation
    pub proof: KtcsProof,
    /// Transaction hash
    pub tx_hash: [u8; 32],
    /// Block hash where transaction was confirmed
    pub block_hash: [u8; 32],
    /// DAA score of confirming block
    pub daa_score: u64,
    /// Blue score of confirming block
    pub blue_score: u64,
}

/// Prepare a direct stamp transaction by querying the network for UTXOs.
///
/// This creates the transaction and proof structure ready for signing and submission.
/// It queries the network for UTXOs, selects appropriate inputs, builds the transaction,
/// and calculates the actual fee.
///
/// # Arguments
/// * `data` - The data to timestamp (will be hashed)
/// * `wallet` - The wallet to use for the transaction
/// * `client` - Connected Kaspa RPC client
/// * `config` - Configuration for the stamp (fee rate, etc.)
///
/// # Returns
/// A PreparedStamp containing the unsigned transaction and UTXOs needed for signing.
#[cfg(feature = "kaspa-client")]
pub async fn prepare_direct_stamp(
    data: &[u8],
    wallet: &KaspaWallet,
    client: &KaspaClient,
    config: &DirectStampConfig,
) -> Result<PreparedStamp> {
    use crate::kaspa_types::COMMITMENT_BURN_AMOUNT;
    use crate::tx_builder::{select_utxos, TransactionBuilder};

    // 1. Create pending stamp (generates nonce and commitment)
    let mut prepared = create_pending_stamp(data)?;

    // 2. Get UTXOs for the wallet
    let utxos = client
        .get_utxos_by_address(wallet.address())
        .await
        .map_err(|e| KtcsError::ConnectionError(e.to_string()))?;

    if utxos.is_empty() {
        return Err(KtcsError::InsufficientFunds(
            "No UTXOs available for wallet".to_string(),
        ));
    }

    // 3. Select UTXOs covering the 0.2 KAS burn PLUS the estimated fee.
    // `select_utxos` adds the estimated fee on top of `target_amount`, so the
    // target is the burn amount itself (the old code passed 0 here and
    // under-selected, failing wallets that had funds spread across UTXOs).
    let (selected_utxos, _total) = select_utxos(&utxos, COMMITMENT_BURN_AMOUNT, config.fee_rate)?;

    // 4. Build unsigned transaction using the hardened builder (checked
    // arithmetic, dust-into-fee absorption, duplicate-UTXO detection).
    let tx_result = TransactionBuilder::new()
        .commitment(&prepared.commitment)
        .add_inputs(selected_utxos.clone())?
        .change_address(wallet.address())
        .fee_per_gram(config.fee_rate)
        .build()?;

    // 5. Store transaction and fee info
    prepared.estimated_fee = tx_result.fee;
    prepared.transaction = Some(tx_result);
    prepared.utxos = Some(selected_utxos);

    Ok(prepared)
}

/// Prepare a direct stamp offline (without network access).
///
/// This creates the commitment and proof structure but doesn't build a transaction.
/// Useful for previewing the commitment hash before connecting to the network.
///
/// Note: this lives in the `kaspa-client`-gated `direct` module. Secure nonce
/// generation requires the `keygen` feature; without it, `generate_nonce`
/// returns an error at runtime rather than fabricating a weak nonce.
pub fn prepare_direct_stamp_offline(
    data: &[u8],
    _wallet: &KaspaWallet,
    _config: &DirectStampConfig,
) -> Result<PreparedStamp> {
    let mut prepared = create_pending_stamp(data)?;
    prepared.estimated_fee = 5000; // Estimated minimum fee
    Ok(prepared)
}

/// A prepared stamp ready for submission
#[derive(Clone, Debug)]
pub struct PreparedStamp {
    /// Proof structure (pending attestation)
    pub proof: KtcsProof,
    /// The 32-byte commitment anchored on-chain via P2PK burn
    pub commitment: [u8; 32],
    /// Nonce used in commitment
    pub nonce: [u8; 16],
    /// Original data hash
    pub data_hash: [u8; 32],
    /// Estimated fee in sompi
    pub estimated_fee: u64,
    /// Built transaction ready for signing (only present when prepared via network)
    #[cfg(feature = "kaspa-client")]
    pub transaction: Option<CommitmentTransaction>,
    /// UTXOs used as inputs (needed for signing)
    #[cfg(feature = "kaspa-client")]
    pub utxos: Option<Vec<Utxo>>,
}

/// Create a stamp directly from data (offline mode)
///
/// This creates a pending proof that can be completed later when
/// connected to the network. Requires a secure nonce; with `kaspa-client` but
/// not `keygen`, `generate_nonce` returns an error at runtime.
pub fn create_pending_stamp(data: &[u8]) -> Result<PreparedStamp> {
    let nonce = generate_nonce()?;
    let data_hash = sha256(data);
    let commitment = create_commitment(&nonce, &data_hash);

    let mut proof = KtcsProof::new(data_hash.to_vec());
    proof.add_operation(Operation::Prepend(nonce.to_vec()));
    proof.add_operation(Operation::Sha256);

    Ok(PreparedStamp {
        proof,
        commitment,
        nonce,
        data_hash,
        estimated_fee: 0,
        #[cfg(feature = "kaspa-client")]
        transaction: None,
        #[cfg(feature = "kaspa-client")]
        utxos: None,
    })
}

/// Complete a prepared stamp with block attestation
///
/// This is called after the transaction has been confirmed on-chain.
pub fn complete_stamp(
    mut prepared: PreparedStamp,
    block_info: DirectBlockInfo,
    tx_hash: [u8; 32],
) -> Result<KtcsProof> {
    let attestation = KaspaAttestation::new(
        block_info.daa_score,
        block_info.blue_score,
        block_info.hash,
        block_info.timestamp,
        tx_hash,
        0, // Output index (commitment is output 0)
        block_info.blue_work,
        block_info.parent_hashes,
    );

    prepared
        .proof
        .add_attestation(Attestation::Kaspa(attestation));
    Ok(prepared.proof)
}

/// Simplified block info for direct stamping
#[derive(Clone, Debug)]
pub struct DirectBlockInfo {
    pub hash: [u8; 32],
    pub daa_score: u64,
    pub blue_score: u64,
    pub timestamp: u64,
    pub blue_work: [u8; 32],
    pub parent_hashes: Vec<[u8; 32]>,
}

#[cfg(feature = "kaspa-client")]
impl From<BlockInfo> for DirectBlockInfo {
    fn from(block: BlockInfo) -> Self {
        Self {
            hash: block.hash,
            daa_score: block.daa_score,
            blue_score: block.blue_score,
            timestamp: block.timestamp,
            blue_work: block.blue_work,
            parent_hashes: block.parent_hashes,
        }
    }
}

/// Sign all inputs of a transaction.
///
/// Uses the wallet's Schnorr key to sign each input using Kaspa's
/// full sighash algorithm (Blake2b with 18 fields).
///
/// # Arguments
/// * `tx` - The unsigned transaction to sign
/// * `wallet` - The wallet containing the signing key
/// * `utxos` - The UTXOs corresponding to each input (in same order)
///
/// # Returns
/// A new Transaction with signature scripts filled in for each input.
#[cfg(feature = "kaspa-client")]
pub fn sign_transaction(
    tx: &Transaction,
    wallet: &KaspaWallet,
    utxos: &[Utxo],
) -> Result<Transaction> {
    if tx.inputs.len() != utxos.len() {
        return Err(KtcsError::InvalidData(format!(
            "Input count {} doesn't match UTXO count {}",
            tx.inputs.len(),
            utxos.len()
        )));
    }

    // Build SighashTransaction from tx and utxos
    let sighash_inputs: Vec<SighashInput> = tx
        .inputs
        .iter()
        .zip(utxos.iter())
        .map(|(input, utxo)| SighashInput {
            previous_outpoint_hash: input.previous_outpoint_hash,
            previous_outpoint_index: input.previous_outpoint_index,
            script_public_key_version: utxo.script_public_key.version,
            script_public_key: utxo.script_public_key.script.clone(),
            value: utxo.amount,
            sequence: u64::MAX, // Kaspa standard sequence
            sig_op_count: 1,    // Standard P2PK has 1 sig op
        })
        .collect();

    let sighash_outputs: Vec<SighashOutput> = tx
        .outputs
        .iter()
        .map(|output| SighashOutput {
            value: output.amount,
            script_public_key_version: output.script_public_key.version,
            script_public_key: output.script_public_key.script.clone(),
        })
        .collect();

    let sighash_tx = SighashTransaction {
        version: tx.version,
        inputs: sighash_inputs,
        outputs: sighash_outputs,
        lock_time: tx.lock_time,
        subnetwork_id: tx.subnetwork_id,
        gas: tx.gas,
        payload: tx.payload.clone(),
    };

    let mut signed_tx = tx.clone();

    for (i, input) in signed_tx.inputs.iter_mut().enumerate() {
        // Compute the full Kaspa sighash for this input
        let sighash = compute_kaspa_sighash(&sighash_tx, i, SigHashType::All)?;

        // Sign the sighash with the wallet's Schnorr key
        let signature = wallet.sign(&sighash)?;

        // Build signature script for P2PK: <push 65> <64-byte sig> <sighash_type=01>
        // Note: Public key is NOT included - it's already in the scriptPubKey
        let mut sig_script = Vec::with_capacity(66);
        sig_script.push(0x41); // Push 65 bytes (64 sig + 1 sighash type)
        sig_script.extend_from_slice(&signature);
        sig_script.push(SigHashType::All as u8); // 0x01

        input.signature_script = sig_script;
    }

    Ok(signed_tx)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "kaspa-client")]
    use crate::kaspa::ScriptPublicKey;

    // Test private key for generating valid addresses
    const TEST_PRIVATE_KEY: [u8; 32] = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e,
        0x1f, 0x20,
    ];

    fn test_wallet() -> KaspaWallet {
        KaspaWallet::from_private_key(&TEST_PRIVATE_KEY, "testnet").unwrap()
    }

    #[cfg(feature = "kaspa-client")]
    fn create_test_utxo(amount: u64, index: u32) -> Utxo {
        let wallet = test_wallet();
        Utxo {
            transaction_id: [index as u8; 32],
            index,
            amount,
            script_public_key: ScriptPublicKey {
                version: 0,
                // Schnorr script: <push 32> <pubkey> OP_CHECKSIG
                script: {
                    let mut s = vec![0x20];
                    s.extend_from_slice(&wallet.public_key());
                    s.push(0xac);
                    s
                },
            },
            block_daa_score: 1000,
            is_coinbase: false,
        }
    }

    #[cfg(feature = "kaspa-client")]
    fn create_test_transaction(num_inputs: usize) -> Transaction {
        use crate::kaspa::{TransactionInput, TransactionOutput};

        Transaction {
            version: 0,
            inputs: (0..num_inputs)
                .map(|i| TransactionInput {
                    previous_outpoint_hash: [i as u8; 32],
                    previous_outpoint_index: i as u32,
                    signature_script: vec![],
                })
                .collect(),
            outputs: vec![
                // P2PK burn output (commitment as fake public key)
                TransactionOutput {
                    amount: 20_000_000, // 0.2 KAS
                    script_public_key: ScriptPublicKey {
                        version: 0,
                        script: {
                            let mut s = vec![0x20]; // Push 32 bytes
                            s.extend_from_slice(&[0xab; 32]); // Fake pubkey (commitment)
                            s.push(0xac); // OP_CHECKSIG
                            s
                        },
                    },
                },
                // Change output
                TransactionOutput {
                    amount: 99_000_000,
                    script_public_key: ScriptPublicKey {
                        version: 0,
                        script: vec![0x20; 34], // Placeholder
                    },
                },
            ],
            lock_time: 0,
            subnetwork_id: [0u8; 20],
            gas: 0,
            payload: vec![],
        }
    }

    #[test]
    fn test_create_pending_stamp() {
        let data = b"Hello, Kaspa Timestamping!";
        let prepared = create_pending_stamp(data).unwrap();

        assert_eq!(prepared.proof.digest.len(), 32);
        assert_eq!(prepared.nonce.len(), 16);
        assert_eq!(prepared.commitment.len(), 32);
    }

    #[test]
    fn test_complete_stamp() {
        let data = b"Test data for stamping";
        let prepared = create_pending_stamp(data).unwrap();

        let block_info = DirectBlockInfo {
            hash: [0xab; 32],
            daa_score: 42000000,
            blue_score: 41500000,
            timestamp: 1737627600000,
            blue_work: [0x00; 32],
            parent_hashes: vec![[0xcd; 32]],
        };
        let tx_hash = [0xef; 32];

        let proof = complete_stamp(prepared, block_info, tx_hash).unwrap();

        // Verify the proof has an attestation
        assert!(!proof.attestations.is_empty());
    }

    #[test]
    fn test_direct_stamp_config_default() {
        let config = DirectStampConfig::default();

        assert_eq!(config.fee_rate, 10);
        assert_eq!(config.confirmation_timeout_secs, 60);
    }

    // === sign_transaction() tests ===

    #[test]
    #[cfg(feature = "kaspa-client")]
    fn test_sign_transaction_single_input() {
        let wallet = test_wallet();
        let utxo = create_test_utxo(100_000_000, 0);
        let tx = create_test_transaction(1);

        let signed = sign_transaction(&tx, &wallet, &[utxo]).unwrap();

        // Verify signature was added
        // Format: <push 65:1> <sig:64> <sighash:1> = 66 bytes
        // Note: Public key is NOT included in signature script for P2PK
        assert!(!signed.inputs[0].signature_script.is_empty());
        assert_eq!(signed.inputs[0].signature_script.len(), 66);
    }

    #[test]
    #[cfg(feature = "kaspa-client")]
    fn test_sign_transaction_multiple_inputs() {
        let wallet = test_wallet();
        let utxos: Vec<Utxo> = (0..3).map(|i| create_test_utxo(50_000_000, i)).collect();
        let tx = create_test_transaction(3);

        let signed = sign_transaction(&tx, &wallet, &utxos).unwrap();

        // All inputs should be signed with 66-byte signature scripts
        for (i, input) in signed.inputs.iter().enumerate() {
            assert_eq!(
                input.signature_script.len(),
                66,
                "Input {} not properly signed",
                i
            );
        }
    }

    #[test]
    #[cfg(feature = "kaspa-client")]
    fn test_sign_transaction_signature_script_format() {
        let wallet = test_wallet();
        let utxo = create_test_utxo(100_000_000, 0);
        let tx = create_test_transaction(1);

        let signed = sign_transaction(&tx, &wallet, &[utxo]).unwrap();
        let sig_script = &signed.inputs[0].signature_script;

        // Format: <push 65:1> <64-byte sig> <sighash:1> = 66 bytes
        // Note: Public key is NOT included - it's in the scriptPubKey of the UTXO
        assert_eq!(sig_script.len(), 66, "Signature script should be 66 bytes");
        assert_eq!(sig_script[0], 0x41, "Should push 65 bytes");
        // Bytes 1-64 are the 64-byte Schnorr signature
        assert_eq!(sig_script[65], 0x01, "SIGHASH_ALL type at end");
    }

    #[test]
    #[cfg(feature = "kaspa-client")]
    fn test_sign_transaction_input_count_mismatch() {
        let wallet = test_wallet();
        let utxo = create_test_utxo(100_000_000, 0);
        let tx = create_test_transaction(2); // 2 inputs, 1 UTXO

        let result = sign_transaction(&tx, &wallet, &[utxo]);

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Input count") && err.contains("doesn't match"),
            "Error should mention input/UTXO count mismatch: {}",
            err
        );
    }

    #[test]
    #[cfg(feature = "kaspa-client")]
    fn test_sign_transaction_preserves_structure() {
        let wallet = test_wallet();
        let utxo = create_test_utxo(100_000_000, 0);
        let tx = create_test_transaction(1);

        let signed = sign_transaction(&tx, &wallet, &[utxo]).unwrap();

        // Version unchanged
        assert_eq!(signed.version, tx.version);
        // Lock time unchanged
        assert_eq!(signed.lock_time, tx.lock_time);
        // Outputs unchanged
        assert_eq!(signed.outputs.len(), tx.outputs.len());
        assert_eq!(signed.outputs[0].amount, tx.outputs[0].amount);
        assert_eq!(signed.outputs[1].amount, tx.outputs[1].amount);
        // Subnetwork unchanged
        assert_eq!(signed.subnetwork_id, tx.subnetwork_id);
        // Gas and payload unchanged
        assert_eq!(signed.gas, tx.gas);
        assert_eq!(signed.payload, tx.payload);
    }

    #[test]
    #[cfg(feature = "kaspa-client")]
    fn test_sign_transaction_different_utxos_different_signatures() {
        let wallet = test_wallet();
        let utxo1 = create_test_utxo(100_000_000, 0);
        let utxo2 = create_test_utxo(200_000_000, 1); // Different UTXO
        let tx = create_test_transaction(1);

        let signed1 = sign_transaction(&tx, &wallet, &[utxo1]).unwrap();
        let signed2 = sign_transaction(&tx, &wallet, &[utxo2]).unwrap();

        // Different UTXOs (different amounts/scripts) should produce different signatures
        // because SIGHASH includes the UTXO data
        assert_ne!(
            signed1.inputs[0].signature_script, signed2.inputs[0].signature_script,
            "Different UTXOs should produce different signatures"
        );

        // But both should have valid format (66 bytes)
        assert_eq!(signed1.inputs[0].signature_script.len(), 66);
        assert_eq!(signed2.inputs[0].signature_script.len(), 66);
    }

    #[test]
    #[cfg(feature = "kaspa-client")]
    fn test_sign_transaction_empty_inputs() {
        let wallet = test_wallet();
        let tx = create_test_transaction(0); // No inputs

        let result = sign_transaction(&tx, &wallet, &[]);

        // Should succeed with empty inputs
        assert!(result.is_ok());
        assert!(result.unwrap().inputs.is_empty());
    }
}
