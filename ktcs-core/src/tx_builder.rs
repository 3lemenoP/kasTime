//! Transaction building utilities for KTCS (WASM-compatible)
//!
//! This module provides utilities for building Kaspa transactions
//! that can be used in both native and WASM environments.
//!
//! # Transaction Structure
//!
//! A typical KTCS commitment transaction has:
//! - One or more inputs (UTXOs from the wallet)
//! - Output 0: P2PK burn output with 32-byte commitment (burns 0.2 KAS)
//! - Output 1: Change back to the wallet address
//!
//! Note: Kaspa does NOT support OP_RETURN. We use P2PK burn outputs instead.

use std::collections::HashSet;

use crate::error::{KtcsError, Result};
use crate::kaspa_types::{
    build_commitment_output, ScriptPublicKey, Transaction, TransactionInput, TransactionOutput,
    Utxo,
};
use crate::merkle::sha256;
use crate::wallet::decode_address;

// Re-export DUST_THRESHOLD from kaspa_types
pub use crate::kaspa_types::DUST_THRESHOLD;

/// Minimum fee rate in sompi per gram (mass unit)
pub const MIN_FEE_PER_GRAM: u64 = 1;

/// Default fee rate in sompi per gram
pub const DEFAULT_FEE_PER_GRAM: u64 = 1;

/// Maximum OP_RETURN data size in bytes
pub const MAX_OP_RETURN_SIZE: usize = 80;

/// KTCS magic prefix for identifying our outputs (optional)
pub const KTCS_MAGIC_PREFIX: &[u8; 4] = b"KTCS";

/// A commitment transaction ready for signing
#[derive(Debug, Clone)]
pub struct CommitmentTransaction {
    /// The unsigned transaction
    pub transaction: Transaction,
    /// The commitment being anchored
    pub commitment: [u8; 32],
    /// Total input amount in sompi
    pub total_input: u64,
    /// Total output amount in sompi
    pub total_output: u64,
    /// Fee in sompi
    pub fee: u64,
    /// Change amount in sompi
    pub change_amount: u64,
}

impl CommitmentTransaction {
    /// Get the transaction hash (for unsigned transaction)
    ///
    /// Note: The actual transaction ID will differ after signing.
    pub fn unsigned_hash(&self) -> [u8; 32] {
        // Simplified: hash the commitment and inputs
        let mut data = Vec::new();
        data.extend_from_slice(&self.commitment);
        for input in &self.transaction.inputs {
            data.extend_from_slice(&input.previous_outpoint_hash);
            data.extend_from_slice(&input.previous_outpoint_index.to_le_bytes());
        }
        sha256(&data)
    }
}

/// Builder for creating commitment transactions
#[derive(Debug, Clone)]
pub struct TransactionBuilder {
    commitment: Option<[u8; 32]>,
    inputs: Vec<(Utxo, Vec<u8>)>, // UTXO and signature script (empty until signed)
    change_address: Option<String>,
    change_script: Option<ScriptPublicKey>,
    fee_per_gram: u64,
    #[allow(dead_code)]
    include_ktcs_magic: bool,
    /// Tracks seen outpoints to prevent duplicate UTXOs
    seen_outpoints: HashSet<([u8; 32], u32)>,
}

impl TransactionBuilder {
    /// Create a new transaction builder
    pub fn new() -> Self {
        Self {
            commitment: None,
            inputs: Vec::new(),
            change_address: None,
            change_script: None,
            fee_per_gram: DEFAULT_FEE_PER_GRAM,
            include_ktcs_magic: false,
            seen_outpoints: HashSet::new(),
        }
    }

    /// Set the 32-byte commitment (Merkle root or hash)
    pub fn commitment(mut self, commitment: &[u8; 32]) -> Self {
        self.commitment = Some(*commitment);
        self
    }

    /// Add a UTXO input
    ///
    /// # Errors
    ///
    /// Returns an error if the same UTXO (transaction_id + index) is added twice.
    pub fn add_input(mut self, utxo: Utxo) -> Result<Self> {
        let outpoint = (utxo.transaction_id, utxo.index);
        if !self.seen_outpoints.insert(outpoint) {
            return Err(KtcsError::Other(format!(
                "Duplicate UTXO: {}:{}",
                hex::encode(utxo.transaction_id),
                utxo.index
            )));
        }
        self.inputs.push((utxo, Vec::new()));
        Ok(self)
    }

    /// Add multiple UTXO inputs
    ///
    /// # Errors
    ///
    /// Returns an error if any UTXO is a duplicate.
    pub fn add_inputs(mut self, utxos: impl IntoIterator<Item = Utxo>) -> Result<Self> {
        for utxo in utxos {
            let outpoint = (utxo.transaction_id, utxo.index);
            if !self.seen_outpoints.insert(outpoint) {
                return Err(KtcsError::Other(format!(
                    "Duplicate UTXO: {}:{}",
                    hex::encode(utxo.transaction_id),
                    utxo.index
                )));
            }
            self.inputs.push((utxo, Vec::new()));
        }
        Ok(self)
    }

    /// Set the change address (Kaspa address format)
    pub fn change_address(mut self, address: &str) -> Self {
        self.change_address = Some(address.to_string());
        self
    }

    /// Set the change script directly
    pub fn change_script(mut self, script: ScriptPublicKey) -> Self {
        self.change_script = Some(script);
        self
    }

    /// Set the fee rate in sompi per gram (mass unit)
    pub fn fee_per_gram(mut self, fee: u64) -> Self {
        self.fee_per_gram = fee.max(MIN_FEE_PER_GRAM);
        self
    }

    /// Include the KTCS magic prefix in the output (reserved for future use)
    ///
    /// Note: Currently a no-op as Kaspa uses P2PK burn outputs which don't
    /// support additional data. This method exists for API compatibility.
    pub fn include_magic(mut self, include: bool) -> Self {
        self.include_ktcs_magic = include;
        self
    }

    /// Get the UTXOs that will be used as inputs
    pub fn get_utxos(&self) -> Vec<&Utxo> {
        self.inputs.iter().map(|(utxo, _)| utxo).collect()
    }

    /// Build the unsigned commitment transaction
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No commitment is set
    /// - No inputs are provided
    /// - No change address/script is provided
    /// - Insufficient funds for fee
    pub fn build(self) -> Result<CommitmentTransaction> {
        // Validate inputs
        let commitment = self
            .commitment
            .ok_or_else(|| KtcsError::Other("Commitment not set".to_string()))?;

        if self.inputs.is_empty() {
            return Err(KtcsError::Other("No inputs provided".to_string()));
        }

        let change_script = if let Some(script) = self.change_script {
            script
        } else if let Some(address) = &self.change_address {
            address_to_script(address)?
        } else {
            return Err(KtcsError::Other(
                "No change address or script provided".to_string(),
            ));
        };

        // Calculate total input
        let total_input: u64 = self.inputs
            .iter()
            .map(|(u, _)| u.amount)
            .try_fold(0u64, |acc, amount| acc.checked_add(amount))
            .ok_or_else(|| KtcsError::Other("Total input amount overflow".to_string()))?;

        // Build transaction
        let mut tx = Transaction::new();

        // Add inputs
        for (utxo, sig_script) in &self.inputs {
            tx.add_input(TransactionInput {
                previous_outpoint_hash: utxo.transaction_id,
                previous_outpoint_index: utxo.index,
                signature_script: sig_script.clone(),
            });
        }

        // Build commitment output (P2PK with commitment as "public key")
        let commitment_output = build_commitment_output(&commitment);
        let commitment_amount = commitment_output.amount;

        // First, estimate fee with 2 outputs to see if change is needed
        let estimated_mass_2_outputs = estimate_transaction_mass(self.inputs.len(), 2)?;
        let fee_2_outputs = estimated_mass_2_outputs
            .checked_mul(self.fee_per_gram)
            .ok_or_else(|| KtcsError::Other("Fee calculation overflow".to_string()))?;
        let total_required_2_outputs = fee_2_outputs
            .checked_add(commitment_amount)
            .ok_or_else(|| KtcsError::Other("Total required overflow".to_string()))?;

        // Determine if we'll have a change output
        let preliminary_change = total_input.saturating_sub(total_required_2_outputs);
        let has_change_output = preliminary_change >= DUST_THRESHOLD;

        // Calculate actual fee based on whether we have a change output
        let num_outputs = if has_change_output { 2 } else { 1 };
        let estimated_mass = estimate_transaction_mass(self.inputs.len(), num_outputs)?;

        let fee = estimated_mass
            .checked_mul(self.fee_per_gram)
            .ok_or_else(|| KtcsError::Other("Fee calculation overflow".to_string()))?;
        let total_required = fee
            .checked_add(commitment_amount)
            .ok_or_else(|| KtcsError::Other("Total required overflow".to_string()))?;

        // Calculate change
        if total_input < total_required {
            return Err(KtcsError::Other(format!(
                "Insufficient funds: have {} sompi, need {} sompi (fee: {}, commitment: {})",
                total_input, total_required, fee, commitment_amount
            )));
        }

        let change_amount = total_input - total_required;

        // Check dust threshold - if change is below dust, absorb it into the fee
        let (final_change_amount, final_fee) = if change_amount > 0 && change_amount < DUST_THRESHOLD {
            // Absorb dust change into the fee
            (0, fee + change_amount)
        } else {
            (change_amount, fee)
        };

        // Add outputs
        tx.add_output(commitment_output);

        if final_change_amount > 0 {
            tx.add_output(TransactionOutput {
                amount: final_change_amount,
                script_public_key: change_script,
            });
        }

        let total_output = commitment_amount
            .checked_add(final_change_amount)
            .ok_or_else(|| KtcsError::Other("Total output overflow".to_string()))?;

        Ok(CommitmentTransaction {
            transaction: tx,
            commitment,
            total_input,
            total_output,
            fee: final_fee,
            change_amount: final_change_amount,
        })
    }
}

impl Default for TransactionBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Estimate transaction mass for fee calculation
///
/// Kaspa uses "mass" as a measure of transaction resource consumption.
fn estimate_transaction_mass(num_inputs: usize, num_outputs: usize) -> Result<u64> {
    // Approximate mass values (should match Kaspa consensus rules)
    const BASE_MASS: u64 = 10;
    const INPUT_MASS: u64 = 148; // Typical P2PKH input
    const OUTPUT_MASS: u64 = 34; // Typical P2PKH output

    let input_total = INPUT_MASS
        .checked_mul(num_inputs as u64)
        .ok_or_else(|| KtcsError::Other("Input mass overflow".to_string()))?;
    let output_total = OUTPUT_MASS
        .checked_mul(num_outputs as u64)
        .ok_or_else(|| KtcsError::Other("Output mass overflow".to_string()))?;

    BASE_MASS
        .checked_add(input_total)
        .and_then(|v| v.checked_add(output_total))
        .ok_or_else(|| KtcsError::Other("Total mass overflow".to_string()))
}

/// Convert a Kaspa address to a script public key
///
/// # Kaspa Address Format
///
/// Kaspa addresses use Bech32m encoding with:
/// - Version byte: 0x00 for Schnorr (32-byte pubkey), 0x08 for ECDSA (33-byte pubkey)
/// - Mainnet prefix: `kaspa`
/// - Testnet prefix: `kaspatest`
pub fn address_to_script(address: &str) -> Result<ScriptPublicKey> {
    // Decode the bech32m address
    let (_hrp, pubkey_data) = decode_address(address)?;

    // Build script based on address type
    if pubkey_data.len() == 32 {
        // Schnorr address - 32-byte x-only public key
        let mut script = Vec::with_capacity(35);
        script.push(0x20); // Push 32 bytes
        script.extend_from_slice(&pubkey_data);
        script.push(0xac); // OP_CHECKSIG

        Ok(ScriptPublicKey { version: 0, script })
    } else if pubkey_data.len() == 33 {
        // ECDSA address - 33-byte compressed public key
        let mut script = Vec::with_capacity(36);
        script.push(0x21); // Push 33 bytes
        script.extend_from_slice(&pubkey_data);
        script.push(0xac); // OP_CHECKSIG

        Ok(ScriptPublicKey { version: 0, script })
    } else {
        Err(KtcsError::InvalidData(format!(
            "Unexpected pubkey length: {} bytes",
            pubkey_data.len()
        )))
    }
}

/// Select UTXOs to cover the required amount plus fee
///
/// Uses a simple "largest first" selection algorithm.
pub fn select_utxos(
    available: &[Utxo],
    target_amount: u64,
    fee_per_gram: u64,
) -> Result<(Vec<Utxo>, u64)> {
    if available.is_empty() {
        return Err(KtcsError::Other("No UTXOs available".to_string()));
    }

    // Sort by amount descending
    let mut sorted: Vec<_> = available.iter().collect();
    sorted.sort_by(|a, b| b.amount.cmp(&a.amount));

    let mut selected = Vec::new();
    let mut total = 0u64;

    for utxo in sorted {
        selected.push(utxo.clone());
        total = total
            .checked_add(utxo.amount)
            .ok_or_else(|| KtcsError::Other("UTXO total overflow".to_string()))?;

        // Estimate fee with current selection
        let estimated_mass = estimate_transaction_mass(selected.len(), 2)?;
        let estimated_fee = estimated_mass
            .checked_mul(fee_per_gram)
            .ok_or_else(|| KtcsError::Other("Estimated fee overflow".to_string()))?;

        let required = target_amount
            .checked_add(estimated_fee)
            .ok_or_else(|| KtcsError::Other("Required amount overflow".to_string()))?;

        if total >= required {
            return Ok((selected, total));
        }
    }

    // Calculate what we needed
    let final_mass = estimate_transaction_mass(selected.len(), 2)?;
    let final_fee = final_mass
        .checked_mul(fee_per_gram)
        .ok_or_else(|| KtcsError::Other("Final fee overflow".to_string()))?;
    let needed = target_amount
        .checked_add(final_fee)
        .ok_or_else(|| KtcsError::Other("Needed amount overflow".to_string()))?;

    Err(KtcsError::Other(format!(
        "Insufficient funds: have {} sompi, need {} sompi",
        total, needed
    )))
}

/// Create a commitment from a nonce and data hash
///
/// commitment = SHA256(nonce || data_hash)
pub fn create_commitment(nonce: &[u8], data_hash: &[u8; 32]) -> [u8; 32] {
    let mut data = Vec::with_capacity(nonce.len() + 32);
    data.extend_from_slice(nonce);
    data.extend_from_slice(data_hash);
    sha256(&data)
}

/// A simple transfer transaction (no commitment)
#[derive(Debug, Clone)]
pub struct TransferTransaction {
    /// The unsigned transaction
    pub transaction: Transaction,
    /// Total input amount in sompi
    pub total_input: u64,
    /// Amount being sent (to destination)
    pub send_amount: u64,
    /// Fee in sompi
    pub fee: u64,
}

/// Builder for creating simple transfer transactions
#[derive(Debug, Clone)]
pub struct TransferTransactionBuilder {
    inputs: Vec<Utxo>,
    destination_address: Option<String>,
    fee_per_gram: u64,
}

impl TransferTransactionBuilder {
    /// Create a new transfer transaction builder
    pub fn new() -> Self {
        Self {
            inputs: Vec::new(),
            destination_address: None,
            fee_per_gram: DEFAULT_FEE_PER_GRAM,
        }
    }

    /// Add a UTXO input
    pub fn add_input(mut self, utxo: Utxo) -> Self {
        self.inputs.push(utxo);
        self
    }

    /// Add multiple UTXO inputs
    pub fn add_inputs(mut self, utxos: impl IntoIterator<Item = Utxo>) -> Self {
        for utxo in utxos {
            self.inputs.push(utxo);
        }
        self
    }

    /// Set destination address
    pub fn destination(mut self, address: &str) -> Self {
        self.destination_address = Some(address.to_string());
        self
    }

    /// Set fee rate in sompi per gram
    pub fn fee_per_gram(mut self, fee: u64) -> Self {
        self.fee_per_gram = fee.max(MIN_FEE_PER_GRAM);
        self
    }

    /// Get the UTXOs that will be used as inputs
    pub fn get_utxos(&self) -> &[Utxo] {
        &self.inputs
    }

    /// Build the transfer transaction
    pub fn build(self) -> Result<TransferTransaction> {
        if self.inputs.is_empty() {
            return Err(KtcsError::Other("No inputs provided".to_string()));
        }

        let destination = self
            .destination_address
            .ok_or_else(|| KtcsError::Other("Destination address not set".to_string()))?;

        let dest_script = address_to_script(&destination)?;

        let total_input: u64 = self.inputs
            .iter()
            .map(|u| u.amount)
            .try_fold(0u64, |acc, amount| acc.checked_add(amount))
            .ok_or_else(|| KtcsError::Other("Total input amount overflow".to_string()))?;

        // Calculate fee
        let estimated_mass = estimate_transfer_mass(self.inputs.len())?;
        let fee = estimated_mass
            .checked_mul(self.fee_per_gram)
            .ok_or_else(|| KtcsError::Other("Fee calculation overflow".to_string()))?;

        if total_input <= fee {
            return Err(KtcsError::Other(format!(
                "Insufficient funds for transfer: have {} sompi, fee is {} sompi",
                total_input, fee
            )));
        }

        let send_amount = total_input - fee;

        if send_amount < DUST_THRESHOLD {
            return Err(KtcsError::Other(format!(
                "Send amount {} is below dust threshold {}",
                send_amount, DUST_THRESHOLD
            )));
        }

        let mut tx = Transaction::new();

        for utxo in &self.inputs {
            tx.add_input(TransactionInput {
                previous_outpoint_hash: utxo.transaction_id,
                previous_outpoint_index: utxo.index,
                signature_script: vec![],
            });
        }

        tx.add_output(TransactionOutput {
            amount: send_amount,
            script_public_key: dest_script,
        });

        Ok(TransferTransaction {
            transaction: tx,
            total_input,
            send_amount,
            fee,
        })
    }
}

impl Default for TransferTransactionBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Estimate transaction mass for a simple transfer
fn estimate_transfer_mass(num_inputs: usize) -> Result<u64> {
    const BASE_MASS: u64 = 10;
    const INPUT_MASS: u64 = 148;
    const OUTPUT_MASS: u64 = 34;

    let input_total = INPUT_MASS
        .checked_mul(num_inputs as u64)
        .ok_or_else(|| KtcsError::Other("Input mass overflow".to_string()))?;

    BASE_MASS
        .checked_add(input_total)
        .and_then(|v| v.checked_add(OUTPUT_MASS))
        .ok_or_else(|| KtcsError::Other("Total mass overflow".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wallet::KaspaWallet;

    const TEST_PRIVATE_KEY: [u8; 32] = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e,
        0x1f, 0x20,
    ];

    fn create_test_utxo(amount: u64, index: u32) -> Utxo {
        Utxo {
            transaction_id: [index as u8; 32],
            index,
            amount,
            script_public_key: ScriptPublicKey {
                version: 0,
                script: vec![
                    0x76, 0xa9, 0x14, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0x88, 0xac,
                ],
            },
            block_daa_score: 1000,
            is_coinbase: false,
        }
    }

    fn test_wallet() -> KaspaWallet {
        KaspaWallet::from_private_key(&TEST_PRIVATE_KEY, "testnet").unwrap()
    }

    #[test]
    fn test_transaction_builder() {
        let wallet = test_wallet();
        let commitment = [0xab; 32];
        let utxo = create_test_utxo(100_000_000, 0); // 1 KAS

        let result = TransactionBuilder::new()
            .commitment(&commitment)
            .add_input(utxo)
            .unwrap()
            .change_address(wallet.address())
            .fee_per_gram(1)
            .build();

        assert!(result.is_ok());
        let tx = result.unwrap();
        assert_eq!(tx.commitment, commitment);
        assert!(tx.change_amount > 0);
        assert!(tx.fee > 0);
    }

    #[test]
    fn test_select_utxos() {
        let utxos = vec![
            create_test_utxo(1_000_000, 0),
            create_test_utxo(5_000_000, 1),
            create_test_utxo(2_000_000, 2),
        ];

        let result = select_utxos(&utxos, 4_000_000, 1);
        assert!(result.is_ok());
        let (selected, total) = result.unwrap();
        assert!(total >= 4_000_000);
        assert_eq!(selected[0].amount, 5_000_000);
    }

    #[test]
    fn test_create_commitment() {
        let nonce = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        let data_hash = [0xab; 32];

        let commitment = create_commitment(&nonce, &data_hash);
        let commitment2 = create_commitment(&nonce, &data_hash);
        assert_eq!(commitment, commitment2);

        let different_nonce = [0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10];
        let commitment3 = create_commitment(&different_nonce, &data_hash);
        assert_ne!(commitment, commitment3);
    }

    #[test]
    fn test_address_to_script() {
        let wallet = test_wallet();
        let address = wallet.address();

        let result = address_to_script(address);
        assert!(result.is_ok());
        let script = result.unwrap();

        assert_eq!(script.script[0], 0x20);
        assert_eq!(script.script.len(), 34);
        assert_eq!(script.script[33], 0xac);
        assert_eq!(&script.script[1..33], &wallet.public_key());
    }

    #[test]
    fn test_duplicate_utxo_detection() {
        let utxo1 = create_test_utxo(100_000_000, 0);
        let utxo2 = create_test_utxo(100_000_000, 0); // Same outpoint as utxo1

        // Adding the same UTXO twice should fail
        let result = TransactionBuilder::new()
            .add_input(utxo1)
            .unwrap()
            .add_input(utxo2);

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Duplicate UTXO"));
    }

    #[test]
    fn test_multiple_different_utxos() {
        let wallet = test_wallet();
        let commitment = [0xab; 32];
        let utxo1 = create_test_utxo(50_000_000, 0);
        let utxo2 = create_test_utxo(50_000_000, 1); // Different index

        // Adding different UTXOs should succeed
        let result = TransactionBuilder::new()
            .commitment(&commitment)
            .add_input(utxo1)
            .unwrap()
            .add_input(utxo2)
            .unwrap()
            .change_address(wallet.address())
            .build();

        assert!(result.is_ok());
    }
}
