//! Kaspa data types for transaction building
//!
//! This module contains pure data types used for Kaspa transactions.
//! These types are always available (not feature-gated) so they can be used
//! in WASM builds for client-side transaction construction.

use serde::{Deserialize, Serialize};

/// Practical minimum change-output amount, in sompi.
///
/// Kaspa does NOT use Bitcoin's fixed 546-sompi dust rule. Instead, KIP-9
/// "storage mass" penalizes small outputs: a value-`v` output contributes
/// roughly `STORAGE_MASS_PARAMETER / v` to the transaction mass, so tiny
/// outputs become uneconomical (their mass-fee exceeds their value). We use a
/// conservative practical floor here: change below this amount is folded into
/// the fee rather than emitted as a near-worthless output. This is intentionally
/// well above Bitcoin's 546 sompi. The exact economic minimum is governed by
/// KIP-9 storage mass (see `estimate_transaction_mass`), not by this constant.
pub const DUST_THRESHOLD: u64 = 1_000_000;

/// Burn amount for commitment outputs (0.2 KAS = 20,000,000 sompi)
pub const COMMITMENT_BURN_AMOUNT: u64 = 20_000_000;

/// Information about a Kaspa block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockInfo {
    /// Block hash (32 bytes)
    pub hash: [u8; 32],
    /// Difficulty Adjustment Algorithm score
    pub daa_score: u64,
    /// Blue score (GHOSTDAG metric)
    pub blue_score: u64,
    /// Cumulative blue work at this block (big-endian 256-bit)
    pub blue_work: [u8; 32],
    /// Block timestamp in Unix milliseconds
    pub timestamp: u64,
    /// Parent block hashes (DAG structure)
    pub parent_hashes: Vec<[u8; 32]>,
    /// Transaction IDs in this block
    pub transaction_ids: Vec<[u8; 32]>,
    /// Whether this is a blue block (in selected chain)
    pub is_chain_block: bool,
}

/// Information about a Kaspa transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionInfo {
    /// Transaction hash (32 bytes)
    pub hash: [u8; 32],
    /// Block hash containing this transaction (if confirmed)
    pub block_hash: Option<[u8; 32]>,
    /// Transaction outputs
    pub outputs: Vec<TransactionOutput>,
    /// Transaction inputs
    pub inputs: Vec<TransactionInput>,
    /// Whether the transaction is accepted
    pub is_accepted: bool,
}

/// A transaction output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionOutput {
    /// Output amount in sompi (1 KAS = 100,000,000 sompi)
    pub amount: u64,
    /// Script public key (for P2PKH/P2SH/P2PK)
    pub script_public_key: ScriptPublicKey,
}

/// A transaction input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionInput {
    /// Previous transaction hash
    pub previous_outpoint_hash: [u8; 32],
    /// Previous output index
    pub previous_outpoint_index: u32,
    /// Signature script
    pub signature_script: Vec<u8>,
}

/// Script public key with type information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptPublicKey {
    /// Script version
    pub version: u16,
    /// Script bytes
    pub script: Vec<u8>,
}

impl ScriptPublicKey {
    /// Check if this is a P2PK script
    pub fn is_p2pk(&self) -> bool {
        // P2PK format: <push N bytes> <N-byte pubkey> OP_CHECKSIG (0xac)
        if self.script.len() < 34 {
            return false;
        }
        let last = self.script[self.script.len() - 1];
        last == 0xac // OP_CHECKSIG
    }
}

/// An unspent transaction output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Utxo {
    /// Transaction hash
    pub transaction_id: [u8; 32],
    /// Output index
    pub index: u32,
    /// Amount in sompi
    pub amount: u64,
    /// Script public key
    pub script_public_key: ScriptPublicKey,
    /// Block DAA score when this UTXO was created
    pub block_daa_score: u64,
    /// Whether this UTXO is coinbase
    pub is_coinbase: bool,
}

/// Information about the current DAG state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagInfo {
    /// Network name (mainnet, testnet-10, testnet-11, etc.)
    pub network: String,
    /// Current DAA score (block height equivalent)
    pub current_daa_score: u64,
    /// Current blue score
    pub current_blue_score: u64,
    /// Current blue work (cumulative PoW)
    pub current_blue_work: [u8; 32],
    /// Virtual selected parent chain block hashes
    pub tip_hashes: Vec<[u8; 32]>,
    /// Current difficulty
    pub difficulty: f64,
    /// Past median time
    pub past_median_time: u64,
    /// Pruning point hash
    pub pruning_point_hash: [u8; 32],
}

/// A raw Kaspa transaction for submission
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    /// Transaction version
    pub version: u16,
    /// Inputs
    pub inputs: Vec<TransactionInput>,
    /// Outputs
    pub outputs: Vec<TransactionOutput>,
    /// Lock time
    pub lock_time: u64,
    /// Subnetwork ID
    pub subnetwork_id: [u8; 20],
    /// Gas (for subnetwork transactions)
    pub gas: u64,
    /// Payload (for subnetwork transactions)
    pub payload: Vec<u8>,
}

/// Kaspa native subnetwork ID (all zeros for native transactions)
pub const SUBNETWORK_ID_NATIVE: [u8; 20] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

impl Transaction {
    /// Create a new standard transaction
    pub fn new() -> Self {
        Self {
            version: 0,
            inputs: Vec::new(),
            outputs: Vec::new(),
            lock_time: 0,
            subnetwork_id: SUBNETWORK_ID_NATIVE,
            gas: 0,
            payload: Vec::new(),
        }
    }

    /// Add an input
    pub fn add_input(&mut self, input: TransactionInput) {
        self.inputs.push(input);
    }

    /// Add an output
    pub fn add_output(&mut self, output: TransactionOutput) {
        self.outputs.push(output);
    }
}

impl Default for Transaction {
    fn default() -> Self {
        Self::new()
    }
}

/// Event types for block subscriptions
#[derive(Debug, Clone)]
pub enum BlockEvent {
    /// A new block was added to the DAG
    BlockAdded(BlockInfo),
    /// The virtual selected parent chain changed
    VirtualChainChanged {
        /// Blocks removed from the selected chain
        removed_chain_block_hashes: Vec<[u8; 32]>,
        /// Blocks added to the selected chain
        added_chain_block_hashes: Vec<[u8; 32]>,
        /// Transactions that were accepted
        accepted_transaction_ids: Vec<[u8; 32]>,
    },
}

/// Connection state for the Kaspa client
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
}

/// Build a P2PK commitment output (burn output)
///
/// Kaspa does NOT support OP_RETURN. Instead, we create a P2PK output
/// where the "public key" is our 32-byte commitment. This output is
/// provably unspendable since no private key exists for this "public key".
///
/// Script format: `<push 32> <32-byte commitment> OP_CHECKSIG`
///
/// This burns COMMITMENT_BURN_AMOUNT (0.2 KAS) per commitment.
pub fn build_commitment_output(commitment: &[u8; 32]) -> TransactionOutput {
    // Kaspa P2PK format for Schnorr (32-byte x-only pubkey):
    // <push 32 bytes> <32-byte pubkey> OP_CHECKSIG
    let mut script = Vec::with_capacity(34);
    script.push(0x20); // Push 32 bytes
    script.extend_from_slice(commitment);
    script.push(0xac); // OP_CHECKSIG

    TransactionOutput {
        amount: COMMITMENT_BURN_AMOUNT,
        script_public_key: ScriptPublicKey { version: 0, script },
    }
}

/// Extract the 32-byte commitment from a raw output script IFF the script is
/// EXACTLY the KTCS P2PK burn form `0x20 <32-byte commitment> 0xac`.
///
/// This is an exact structural match, not a byte scan: the script must be
/// exactly 34 bytes, begin with the 32-byte push opcode `0x20`, and end with
/// `OP_CHECKSIG` (`0xac`). Any other script (including change outputs, or a
/// commitment appearing at a non-canonical offset) returns `None`.
pub fn extract_commitment_from_script(script: &[u8]) -> Option<[u8; 32]> {
    // P2PK burn format: 0x20 <32 bytes> 0xac  (exactly 34 bytes)
    if script.len() == 34 && script[0] == 0x20 && script[33] == 0xac {
        let mut commitment = [0u8; 32];
        commitment.copy_from_slice(&script[1..33]);
        Some(commitment)
    } else {
        None
    }
}

/// Extract commitment from a P2PK burn output (exact-match; see
/// [`extract_commitment_from_script`]).
pub fn extract_commitment_from_output(output: &TransactionOutput) -> Option<[u8; 32]> {
    extract_commitment_from_script(&output.script_public_key.script)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_commitment_output() {
        let commitment = [0xab; 32];
        let output = build_commitment_output(&commitment);

        assert_eq!(output.amount, COMMITMENT_BURN_AMOUNT);
        assert_eq!(output.script_public_key.script.len(), 34);
        assert_eq!(output.script_public_key.script[0], 0x20);
        assert_eq!(&output.script_public_key.script[1..33], &commitment);
        assert_eq!(output.script_public_key.script[33], 0xac);
    }

    #[test]
    fn test_extract_commitment() {
        let commitment = [0xcd; 32];
        let output = build_commitment_output(&commitment);

        let extracted = extract_commitment_from_output(&output);
        assert_eq!(extracted, Some(commitment));
    }

    #[test]
    fn test_extract_commitment_exact_match_only() {
        let commitment = [0xcd; 32];

        // Exact burn script matches.
        let exact = {
            let mut s = vec![0x20];
            s.extend_from_slice(&commitment);
            s.push(0xac);
            s
        };
        assert_eq!(extract_commitment_from_script(&exact), Some(commitment));

        // Commitment embedded at a non-canonical offset must NOT match (naive
        // byte-scan rejection). Prepend a byte and pad to keep 0xac last.
        let mut embedded = vec![0x00, 0x20];
        embedded.extend_from_slice(&commitment);
        embedded.push(0xac);
        assert_eq!(extract_commitment_from_script(&embedded), None);

        // Wrong push opcode (0x21 = 33-byte push) must not match.
        let mut wrong_push = vec![0x21];
        wrong_push.extend_from_slice(&commitment);
        wrong_push.push(0xac);
        assert_eq!(extract_commitment_from_script(&wrong_push), None);

        // Missing OP_CHECKSIG terminator must not match.
        let mut no_checksig = vec![0x20];
        no_checksig.extend_from_slice(&commitment);
        no_checksig.push(0x00);
        assert_eq!(extract_commitment_from_script(&no_checksig), None);
    }

    #[test]
    fn test_script_is_p2pk() {
        let commitment = [0xab; 32];
        let output = build_commitment_output(&commitment);
        assert!(output.script_public_key.is_p2pk());
    }

    #[test]
    fn test_transaction_builder() {
        let mut tx = Transaction::new();
        tx.add_input(TransactionInput {
            previous_outpoint_hash: [1u8; 32],
            previous_outpoint_index: 0,
            signature_script: vec![],
        });
        tx.add_output(TransactionOutput {
            amount: 1000,
            script_public_key: ScriptPublicKey {
                version: 0,
                script: vec![0x76, 0xa9],
            },
        });

        assert_eq!(tx.inputs.len(), 1);
        assert_eq!(tx.outputs.len(), 1);
    }
}
