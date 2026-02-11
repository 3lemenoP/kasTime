//! Transaction building utilities for KTCS
//!
//! This module provides utilities for building Kaspa transactions
//! that contain P2PK burn commitments for timestamping.
//!
//! # Transaction Structure
//!
//! A typical KTCS commitment transaction has:
//! - One or more inputs (UTXOs from the calendar wallet)
//! - Output 0: P2PK burn output with 32-byte commitment (Merkle root or direct hash)
//! - Output 1: Change back to the calendar address
//!
//! # Example
//!
//! ```rust,ignore
//! use ktcs_core::tx::{TransactionBuilder, CommitmentTransaction};
//!
//! let commitment = [0xab; 32]; // Merkle root
//! let tx = TransactionBuilder::new()
//!     .commitment(&commitment)
//!     .add_input(utxo)
//!     .change_address("kaspa:qz...")
//!     .fee_per_gram(1)
//!     .build()?;
//! ```

use crate::error::{KtcsError, Result};
use crate::kaspa::{
    build_commitment_output, ScriptPublicKey, Transaction, TransactionInput, TransactionOutput,
    Utxo,
};
use crate::kaspa_types::DUST_THRESHOLD;
use crate::merkle::sha256;

/// Minimum fee rate in sompi per gram (mass unit)
pub const MIN_FEE_PER_GRAM: u64 = 1;

/// Default fee rate in sompi per gram
pub const DEFAULT_FEE_PER_GRAM: u64 = 1;

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
        // In practice, this would serialize and hash the transaction
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
        }
    }

    /// Set the 32-byte commitment (Merkle root or hash)
    pub fn commitment(mut self, commitment: &[u8; 32]) -> Self {
        self.commitment = Some(*commitment);
        self
    }

    /// Add a UTXO input
    pub fn add_input(mut self, utxo: Utxo) -> Self {
        self.inputs.push((utxo, Vec::new()));
        self
    }

    /// Add multiple UTXO inputs
    pub fn add_inputs(mut self, utxos: impl IntoIterator<Item = Utxo>) -> Self {
        for utxo in utxos {
            self.inputs.push((utxo, Vec::new()));
        }
        self
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
        let commitment = self.commitment.ok_or_else(|| {
            KtcsError::Other("Commitment not set".to_string())
        })?;

        if self.inputs.is_empty() {
            return Err(KtcsError::Other("No inputs provided".to_string()));
        }

        let change_script = if let Some(script) = self.change_script {
            script
        } else if let Some(address) = &self.change_address {
            address_to_script(address)?
        } else {
            return Err(KtcsError::Other("No change address or script provided".to_string()));
        };

        // Calculate total input
        let total_input: u64 = self.inputs.iter().map(|(u, _)| u.amount).sum();

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
        // Note: Kaspa does NOT support OP_RETURN, so we use a P2PK burn output
        let commitment_output = build_commitment_output(&commitment);
        let commitment_amount = commitment_output.amount; // Dust amount (546 sompi)

        // Calculate transaction mass (for fee calculation)
        // Kaspa uses "mass" instead of "size" for fee calculation
        let estimated_mass = estimate_transaction_mass(
            self.inputs.len(),
            2, // commitment output + change
        );

        let fee = estimated_mass * self.fee_per_gram;
        let total_required = fee + commitment_amount;

        // Calculate change
        if total_input < total_required {
            return Err(KtcsError::Other(format!(
                "Insufficient funds: have {} sompi, need {} sompi (fee: {}, commitment: {})",
                total_input, total_required, fee, commitment_amount
            )));
        }

        let change_amount = total_input - total_required;

        // Check dust threshold
        if change_amount > 0 && change_amount < DUST_THRESHOLD {
            return Err(KtcsError::Other(format!(
                "Change amount {} is below dust threshold {}",
                change_amount, DUST_THRESHOLD
            )));
        }

        // Add outputs
        tx.add_output(commitment_output);

        if change_amount > 0 {
            tx.add_output(TransactionOutput {
                amount: change_amount,
                script_public_key: change_script,
            });
        }

        let total_output = commitment_amount + change_amount;

        Ok(CommitmentTransaction {
            transaction: tx,
            commitment,
            total_input,
            total_output,
            fee,
            change_amount,
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
/// Mass is roughly: base_mass + input_mass * num_inputs + output_mass * num_outputs
fn estimate_transaction_mass(num_inputs: usize, num_outputs: usize) -> u64 {
    // Approximate mass values (these should match Kaspa consensus rules)
    const BASE_MASS: u64 = 10;
    const INPUT_MASS: u64 = 148; // Typical P2PKH input
    const OUTPUT_MASS: u64 = 34; // Typical P2PKH output
    const COMMITMENT_OUTPUT_MASS: u64 = 42; // P2PK commitment output (34 bytes script + overhead)

    let input_total = INPUT_MASS * num_inputs as u64;
    let output_total = OUTPUT_MASS * (num_outputs - 1) as u64; // Exclude commitment output

    BASE_MASS + input_total + output_total + COMMITMENT_OUTPUT_MASS
}

/// Convert a Kaspa address to a script public key
///
/// # Kaspa Address Format
///
/// Kaspa addresses use Bech32m encoding with:
/// - Version byte: 0x00 for Schnorr (32-byte pubkey), 0x08 for ECDSA (33-byte pubkey)
/// - Mainnet prefix: `kaspa`
/// - Testnet prefix: `kaspatest`
///
/// # Errors
///
/// Returns an error if the address format is invalid.
pub fn address_to_script(address: &str) -> Result<ScriptPublicKey> {
    use crate::wallet::decode_address;

    // Decode the bech32m address
    let (_hrp, pubkey_data) = decode_address(address)?;

    // Build script based on address type
    // Kaspa uses a simplified script format for Schnorr addresses
    if pubkey_data.len() == 32 {
        // Schnorr address - 32-byte x-only public key
        // Script: version 0, direct pubkey (Kaspa uses simplified script)
        let mut script = Vec::with_capacity(35);
        script.push(0x20); // Push 32 bytes
        script.extend_from_slice(&pubkey_data);
        script.push(0xac); // OP_CHECKSIG

        Ok(ScriptPublicKey {
            version: 0,
            script,
        })
    } else if pubkey_data.len() == 33 {
        // ECDSA address - 33-byte compressed public key
        let mut script = Vec::with_capacity(36);
        script.push(0x21); // Push 33 bytes
        script.extend_from_slice(&pubkey_data);
        script.push(0xac); // OP_CHECKSIG

        Ok(ScriptPublicKey {
            version: 0,
            script,
        })
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
/// Returns the selected UTXOs and total selected amount.
///
/// # Arguments
///
/// * `available` - Available UTXOs to select from
/// * `target_amount` - Target amount to cover (excluding fee)
/// * `fee_per_gram` - Fee rate in sompi per gram
///
/// # Returns
///
/// Tuple of (selected UTXOs, total amount)
///
/// # Errors
///
/// Returns an error if insufficient funds.
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
        total += utxo.amount;

        // Estimate fee with current selection
        let estimated_mass = estimate_transaction_mass(selected.len(), 2);
        let estimated_fee = estimated_mass * fee_per_gram;

        if total >= target_amount + estimated_fee {
            return Ok((selected, total));
        }
    }

    // Calculate what we needed
    let final_mass = estimate_transaction_mass(selected.len(), 2);
    let final_fee = final_mass * fee_per_gram;
    let needed = target_amount + final_fee;

    Err(KtcsError::Other(format!(
        "Insufficient funds: have {} sompi, need {} sompi",
        total, needed
    )))
}

/// Create a commitment from a nonce and data hash
///
/// commitment = SHA256(nonce || data_hash)
///
/// This provides privacy - the calendar only sees the commitment,
/// not the original data hash.
pub fn create_commitment(nonce: &[u8], data_hash: &[u8; 32]) -> [u8; 32] {
    let mut data = Vec::with_capacity(nonce.len() + 32);
    data.extend_from_slice(nonce);
    data.extend_from_slice(data_hash);
    sha256(&data)
}

/// Generate a cryptographically secure random nonce for commitment creation
///
/// Uses the system's CSPRNG via getrandom.
#[cfg(feature = "keygen")]
pub fn generate_nonce() -> Result<[u8; 16]> {
    let mut nonce = [0u8; 16];
    getrandom::getrandom(&mut nonce)
        .map_err(|e| KtcsError::Other(format!("Failed to generate random nonce: {}", e)))?;
    Ok(nonce)
}

/// Generate a random nonce (fallback for non-keygen builds)
#[cfg(all(feature = "kaspa-client", not(feature = "keygen")))]
pub fn generate_nonce() -> Result<[u8; 16]> {
    Err(KtcsError::Other(
        "Secure nonce generation requires 'keygen' feature".to_string(),
    ))
}

// =============================================================================
// Transfer Transaction Builder (for dual-wallet recycling)
// =============================================================================

/// A simple transfer transaction (no commitment output)
///
/// Used for recycling funds from RETURN wallet back to STAMP wallet.
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

/// Builder for creating simple transfer transactions (no commitment output)
///
/// Used for wallet recycling - sends all funds from one wallet to another.
///
/// # Example
///
/// ```rust,ignore
/// let tx = TransferTransactionBuilder::new()
///     .add_inputs(utxos)
///     .destination("kaspa:qz...")
///     .send_all()
///     .fee_per_gram(1)
///     .build()?;
/// ```
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

    /// Set destination address (where funds will be sent)
    pub fn destination(mut self, address: &str) -> Self {
        self.destination_address = Some(address.to_string());
        self
    }

    /// Set fee rate in sompi per gram (mass unit)
    pub fn fee_per_gram(mut self, fee: u64) -> Self {
        self.fee_per_gram = fee.max(MIN_FEE_PER_GRAM);
        self
    }

    /// Build the transfer transaction
    ///
    /// Sends all input funds (minus fee) to the destination address.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No inputs are provided
    /// - No destination address is set
    /// - Insufficient funds for fee
    /// - Send amount would be below dust threshold
    pub fn build(self) -> Result<TransferTransaction> {
        if self.inputs.is_empty() {
            return Err(KtcsError::Other("No inputs provided".to_string()));
        }

        let destination = self.destination_address.ok_or_else(|| {
            KtcsError::Other("Destination address not set".to_string())
        })?;

        let dest_script = address_to_script(&destination)?;

        let total_input: u64 = self.inputs.iter().map(|u| u.amount).sum();

        // Calculate fee (simpler than commitment tx: only 1 output)
        let estimated_mass = estimate_transfer_mass(self.inputs.len());
        let fee = estimated_mass * self.fee_per_gram;

        if total_input <= fee {
            return Err(KtcsError::Other(format!(
                "Insufficient funds for transfer: have {} sompi, fee is {} sompi",
                total_input, fee
            )));
        }

        let send_amount = total_input - fee;

        // Check dust threshold
        if send_amount < DUST_THRESHOLD {
            return Err(KtcsError::Other(format!(
                "Send amount {} is below dust threshold {}",
                send_amount, DUST_THRESHOLD
            )));
        }

        // Build transaction
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

/// Estimate transaction mass for a simple transfer (no commitment output)
fn estimate_transfer_mass(num_inputs: usize) -> u64 {
    const BASE_MASS: u64 = 10;
    const INPUT_MASS: u64 = 148;
    const OUTPUT_MASS: u64 = 34;

    BASE_MASS + (INPUT_MASS * num_inputs as u64) + OUTPUT_MASS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wallet::KaspaWallet;

    /// Test private key for generating valid addresses
    const TEST_PRIVATE_KEY: [u8; 32] = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
        0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
        0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18,
        0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20,
    ];

    fn create_test_utxo(amount: u64, index: u32) -> Utxo {
        Utxo {
            transaction_id: [index as u8; 32],
            index,
            amount,
            script_public_key: ScriptPublicKey {
                version: 0,
                script: vec![0x76, 0xa9, 0x14, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x88, 0xac],
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
    fn test_transaction_builder_no_commitment() {
        let wallet = test_wallet();
        let utxo = create_test_utxo(100_000_000, 0);

        let result = TransactionBuilder::new()
            .add_input(utxo)
            .change_address(wallet.address())
            .build();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Commitment"));
    }

    #[test]
    fn test_transaction_builder_no_inputs() {
        let wallet = test_wallet();
        let commitment = [0xab; 32];

        let result = TransactionBuilder::new()
            .commitment(&commitment)
            .change_address(wallet.address())
            .build();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("inputs"));
    }

    #[test]
    fn test_transaction_builder_insufficient_funds() {
        let wallet = test_wallet();
        let commitment = [0xab; 32];
        let utxo = create_test_utxo(100, 0); // Very small amount

        let result = TransactionBuilder::new()
            .commitment(&commitment)
            .add_input(utxo)
            .change_address(wallet.address())
            .fee_per_gram(1000) // High fee rate
            .build();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Insufficient"));
    }

    #[test]
    fn test_select_utxos() {
        let utxos = vec![
            create_test_utxo(1_000_000, 0),
            create_test_utxo(5_000_000, 1),
            create_test_utxo(2_000_000, 2),
        ];

        // Should select the largest first
        let result = select_utxos(&utxos, 4_000_000, 1);
        assert!(result.is_ok());
        let (selected, total) = result.unwrap();
        assert!(total >= 4_000_000);
        // Should have selected the 5M UTXO first
        assert_eq!(selected[0].amount, 5_000_000);
    }

    #[test]
    fn test_select_utxos_insufficient() {
        let utxos = vec![
            create_test_utxo(1_000, 0),
        ];

        let result = select_utxos(&utxos, 1_000_000, 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_create_commitment() {
        let nonce = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        let data_hash = [0xab; 32];

        let commitment = create_commitment(&nonce, &data_hash);

        // Should be deterministic
        let commitment2 = create_commitment(&nonce, &data_hash);
        assert_eq!(commitment, commitment2);

        // Different nonce = different commitment
        let different_nonce = [0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10];
        let commitment3 = create_commitment(&different_nonce, &data_hash);
        assert_ne!(commitment, commitment3);
    }

    #[test]
    fn test_address_to_script() {
        // Generate a valid Schnorr address using test wallet
        let wallet = test_wallet();
        let address = wallet.address();

        // Test Schnorr address conversion (32-byte pubkey)
        let result = address_to_script(address);
        assert!(result.is_ok());
        let script = result.unwrap();

        // Verify Kaspa Schnorr script format: <push 32> <32-byte pubkey> OP_CHECKSIG
        assert_eq!(script.script[0], 0x20); // Push 32 bytes
        assert_eq!(script.script.len(), 34); // 1 + 32 + 1
        assert_eq!(script.script[33], 0xac); // OP_CHECKSIG

        // The pubkey in the script should match the wallet's public key
        assert_eq!(&script.script[1..33], &wallet.public_key());

        // Invalid bech32m format should fail
        let result = address_to_script("notanaddress");
        assert!(result.is_err());

        // Invalid checksum should fail
        let result = address_to_script("kaspa:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq");
        assert!(result.is_err());
    }

    #[test]
    fn test_estimate_transaction_mass() {
        // Basic transaction: 1 input, 2 outputs
        let mass = estimate_transaction_mass(1, 2);
        assert!(mass > 0);

        // More inputs = more mass
        let mass2 = estimate_transaction_mass(3, 2);
        assert!(mass2 > mass);
    }

    #[cfg(feature = "keygen")]
    #[test]
    fn test_generate_nonce() {
        let nonce1 = generate_nonce().unwrap();
        let nonce2 = generate_nonce().unwrap();

        // Should be different (with extremely high probability for CSPRNG)
        assert_ne!(nonce1, nonce2);
        assert_eq!(nonce1.len(), 16);
    }
}
