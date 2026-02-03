//! Wallet support for direct Kaspa timestamping
//!
//! This module provides wallet functionality for signing transactions
//! when using direct stamping mode (bypassing the calendar server).

use crate::error::{KtcsError, Result};
use secp256k1::{Keypair, Message, Secp256k1, SecretKey, XOnlyPublicKey};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

// Domain separation keys for Kaspa Blake2b hashes
const TRANSACTION_SIGNING_HASH_KEY: &[u8] = b"TransactionSigningHash";

// ========================================
// Sighash Types and Structures
// ========================================

/// Kaspa sighash types (matches rusty-kaspa SigHashType)
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SigHashType {
    /// Sign all inputs and outputs
    All = 0x01,
    /// Sign all inputs, no outputs
    None = 0x02,
    /// Sign all inputs, only the corresponding output
    Single = 0x03,
    /// Modifier: only sign the current input
    AnyOneCanPay = 0x80,
    /// All + AnyOneCanPay
    AllAnyOneCanPay = 0x81,
    /// None + AnyOneCanPay
    NoneAnyOneCanPay = 0x82,
    /// Single + AnyOneCanPay
    SingleAnyOneCanPay = 0x83,
}

/// Input data needed for sighash computation
#[derive(Debug, Clone)]
pub struct SighashInput {
    /// Previous outpoint transaction ID
    pub previous_outpoint_hash: [u8; 32],
    /// Previous outpoint index
    pub previous_outpoint_index: u32,
    /// Script public key version of the UTXO being spent
    pub script_public_key_version: u16,
    /// Script public key of the UTXO being spent
    pub script_public_key: Vec<u8>,
    /// Value of the UTXO being spent (in sompi)
    pub value: u64,
    /// Sequence number (usually u64::MAX)
    pub sequence: u64,
    /// Signature operation count (usually 1)
    pub sig_op_count: u8,
}

/// Output data needed for sighash computation
#[derive(Debug, Clone)]
pub struct SighashOutput {
    /// Output value in sompi
    pub value: u64,
    /// Script public key version
    pub script_public_key_version: u16,
    /// Script public key bytes
    pub script_public_key: Vec<u8>,
}

/// Transaction data needed for sighash computation
#[derive(Debug, Clone)]
pub struct SighashTransaction {
    /// Transaction version
    pub version: u16,
    /// Inputs with UTXO data
    pub inputs: Vec<SighashInput>,
    /// Outputs
    pub outputs: Vec<SighashOutput>,
    /// Lock time
    pub lock_time: u64,
    /// Subnetwork ID (20 bytes)
    pub subnetwork_id: [u8; 20],
    /// Gas (for subnetwork transactions)
    pub gas: u64,
    /// Payload (for subnetwork transactions)
    pub payload: Vec<u8>,
}

/// Kaspa bech32 charset (same as standard but always lowercase)
const CHARSET: &[u8] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";

/// Reverse charset lookup table
const CHARSET_REV: [i8; 128] = [
    -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
    -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
    15, -1, 10, 17, 21, 20, 26, 30, 7, 5, -1, -1, -1, -1, -1, -1, -1, 29, -1, 24, 13, 25, 9, 8, 23,
    -1, 18, 22, 31, 27, 19, -1, 1, 0, 3, 16, 11, 28, 12, 14, 6, 4, 2, -1, -1, -1, -1, -1, -1, 29,
    -1, 24, 13, 25, 9, 8, 23, -1, 18, 22, 31, 27, 19, -1, 1, 0, 3, 16, 11, 28, 12, 14, 6, 4, 2, -1,
    -1, -1, -1, -1,
];

/// A Kaspa wallet for signing transactions.
///
/// Private key is automatically zeroed when the wallet is dropped.
/// Note: Clone is intentionally NOT derived to prevent accidental key duplication.
#[derive(ZeroizeOnDrop)]
pub struct KaspaWallet {
    /// Private key (32 bytes) - zeroized on drop
    #[zeroize(skip)] // Keypair handles its own zeroing
    keypair: Keypair,
    /// Public key (32 bytes - x-only Schnorr)
    #[zeroize(skip)]
    public_key: XOnlyPublicKey,
    /// Bech32m address
    address: String,
    /// Network prefix (kaspa or kaspatest)
    network: String,
}

impl KaspaWallet {
    /// Create a wallet from a private key.
    pub fn from_private_key(private_key: &[u8], network: &str) -> Result<Self> {
        if private_key.len() != 32 {
            return Err(KtcsError::InvalidData(
                "Private key must be 32 bytes".to_string(),
            ));
        }

        let secp = Secp256k1::new();

        let secret_key = SecretKey::from_slice(private_key)
            .map_err(|e| KtcsError::InvalidData(format!("Invalid private key: {}", e)))?;

        let keypair = Keypair::from_secret_key(&secp, &secret_key);
        let (public_key, _parity) = keypair.x_only_public_key();

        let address = encode_address(&public_key, network)?;

        Ok(Self {
            keypair,
            public_key,
            address,
            network: network.to_string(),
        })
    }

    /// Create a wallet from a hex-encoded private key.
    pub fn from_hex(hex_key: &str, network: &str) -> Result<Self> {
        let bytes = hex::decode(hex_key.trim())
            .map_err(|e| KtcsError::InvalidData(format!("Invalid hex key: {}", e)))?;
        Self::from_private_key(&bytes, network)
    }

    /// Get the wallet address.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// Get the public key as bytes.
    pub fn public_key(&self) -> [u8; 32] {
        self.public_key.serialize()
    }

    /// Get the x-only public key.
    pub fn x_only_public_key(&self) -> &XOnlyPublicKey {
        &self.public_key
    }

    /// Get the network.
    pub fn network(&self) -> &str {
        &self.network
    }

    /// Get the private key as bytes (zeroized on drop).
    ///
    /// ⚠️ WARNING: Handle with care! Never log or expose this.
    pub fn private_key(&self) -> Zeroizing<[u8; 32]> {
        Zeroizing::new(self.keypair.secret_key().secret_bytes())
    }

    /// Get the private key as a hex string (zeroized on drop).
    ///
    /// ⚠️ WARNING: Handle with care! Never log or expose this.
    pub fn to_hex(&self) -> Zeroizing<String> {
        Zeroizing::new(hex::encode(*self.private_key()))
    }

    /// Sign a transaction hash using Schnorr signature.
    ///
    /// Returns a 64-byte Schnorr signature.
    pub fn sign(&self, message_hash: &[u8; 32]) -> Result<[u8; 64]> {
        let secp = Secp256k1::new();

        let message = Message::from_digest(*message_hash);

        let signature = secp.sign_schnorr(&message, &self.keypair);

        Ok(*signature.as_ref())
    }

    /// Verify a Schnorr signature.
    pub fn verify(
        public_key: &XOnlyPublicKey,
        message_hash: &[u8; 32],
        signature: &[u8; 64],
    ) -> Result<bool> {
        let secp = Secp256k1::new();

        let message = Message::from_digest(*message_hash);

        let sig = secp256k1::schnorr::Signature::from_slice(signature)
            .map_err(|e| KtcsError::InvalidData(format!("Invalid signature format: {}", e)))?;

        Ok(secp.verify_schnorr(&sig, &message, public_key).is_ok())
    }

}

/// Encode a public key as a Kaspa address.
///
/// Kaspa uses a custom bech32 encoding with colon separator.
/// Format: {prefix}:{version_char}{base32_data}{checksum}
///
/// Version bytes:
/// - 0x00 = schnorr pubkey (type 'q')
/// - 0x01 = ECDSA pubkey (type 'p')
/// - 0x08 = script hash
fn encode_address(public_key: &XOnlyPublicKey, network: &str) -> Result<String> {
    let prefix = match network {
        "mainnet" => "kaspa",
        "testnet-10" | "testnet-11" | "testnet" => "kaspatest",
        "simnet" | "kaspasim" => "kaspasim",
        "devnet" | "kaspadev" => "kaspadev",
        _ => network,
    };

    // Kaspa address format: version byte (0x00 for schnorr) + 32-byte x-only pubkey
    let mut payload = Vec::with_capacity(33);
    payload.push(0x00); // Version 0 = schnorr address
    payload.extend_from_slice(&public_key.serialize());

    // Convert 8-bit bytes to 5-bit words
    let words = convert_bits(&payload, 8, 5, true)?;

    // Calculate checksum
    let checksum = create_checksum(prefix, &words);

    // Build the address string
    let mut address = String::with_capacity(prefix.len() + 1 + words.len() + 8);
    address.push_str(prefix);
    address.push(':');

    for &word in &words {
        address.push(CHARSET[word as usize] as char);
    }
    for &word in &checksum {
        address.push(CHARSET[word as usize] as char);
    }

    Ok(address)
}

/// Decode a Kaspa address to extract the prefix and public key.
pub fn decode_address(address: &str) -> Result<(String, Vec<u8>)> {
    // Split on colon separator
    let parts: Vec<&str> = address.split(':').collect();
    if parts.len() != 2 {
        return Err(KtcsError::InvalidData(
            "Invalid address format: missing colon separator".to_string(),
        ));
    }

    let prefix = parts[0].to_lowercase();
    let data_part = parts[1].to_lowercase();

    if data_part.len() < 8 {
        return Err(KtcsError::InvalidData("Address data too short".to_string()));
    }

    // Decode the data part
    let mut words = Vec::with_capacity(data_part.len());
    for c in data_part.chars() {
        let idx = c as usize;
        if idx >= 128 {
            return Err(KtcsError::InvalidData(format!("Invalid character: {}", c)));
        }
        let val = CHARSET_REV[idx];
        if val < 0 {
            return Err(KtcsError::InvalidData(format!("Invalid character: {}", c)));
        }
        words.push(val as u8);
    }

    // Verify checksum
    if !verify_checksum(&prefix, &words) {
        return Err(KtcsError::InvalidData("Invalid checksum".to_string()));
    }

    // Remove checksum (last 8 characters)
    let data_words = &words[..words.len() - 8];

    // Convert 5-bit words back to 8-bit bytes
    let payload = convert_bits(data_words, 5, 8, false)?;

    if payload.is_empty() {
        return Err(KtcsError::InvalidData("Empty address payload".to_string()));
    }

    let version = payload[0];
    let pubkey_hash = payload[1..].to_vec();

    // Version 0 = schnorr (32-byte pubkey)
    // Version 1 = ECDSA pubkey
    // Version 8 = script hash
    match version {
        0x00 => {
            if pubkey_hash.len() != 32 {
                return Err(KtcsError::InvalidData(format!(
                    "Invalid schnorr pubkey length: expected 32, got {}",
                    pubkey_hash.len()
                )));
            }
        }
        0x01 => {
            if pubkey_hash.len() != 32 {
                return Err(KtcsError::InvalidData(format!(
                    "Invalid ECDSA pubkey length: expected 32, got {}",
                    pubkey_hash.len()
                )));
            }
        }
        0x08 => {
            // Script hash - variable length
        }
        _ => {
            return Err(KtcsError::InvalidData(format!(
                "Unknown address version: {}",
                version
            )));
        }
    }

    Ok((prefix, pubkey_hash))
}

/// Kaspa polymod function for checksum calculation
fn polymod(values: &[u8]) -> u64 {
    const GEN: [u64; 5] = [
        0x98f2bc8e61,
        0x79b76d99e2,
        0xf33e5fb3c4,
        0xae2eabe2a8,
        0x1e4f43e470,
    ];

    let mut chk: u64 = 1;
    for &v in values {
        let top = chk >> 35;
        chk = ((chk & 0x07ffffffff) << 5) ^ (v as u64);
        for (i, &g) in GEN.iter().enumerate() {
            if (top >> i) & 1 == 1 {
                chk ^= g;
            }
        }
    }
    chk
}

/// Expand the prefix for checksum computation
fn prefix_expand(prefix: &str) -> Vec<u8> {
    let mut ret = Vec::with_capacity(prefix.len() + 1);
    for c in prefix.chars() {
        ret.push((c as u8) & 0x1f);
    }
    ret.push(0);
    ret
}

/// Create checksum for Kaspa address
fn create_checksum(prefix: &str, words: &[u8]) -> Vec<u8> {
    let mut values = prefix_expand(prefix);
    values.extend_from_slice(words);
    values.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]);

    let polymod_val = polymod(&values) ^ 1;

    let mut checksum = Vec::with_capacity(8);
    for i in 0..8 {
        checksum.push(((polymod_val >> (5 * (7 - i))) & 0x1f) as u8);
    }
    checksum
}

/// Verify checksum for Kaspa address
fn verify_checksum(prefix: &str, words: &[u8]) -> bool {
    let mut values = prefix_expand(prefix);
    values.extend_from_slice(words);
    polymod(&values) == 1
}

/// Convert between bit sizes (8-bit to 5-bit or vice versa)
fn convert_bits(data: &[u8], from_bits: u32, to_bits: u32, pad: bool) -> Result<Vec<u8>> {
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    let mut ret = Vec::new();
    let maxv: u32 = (1 << to_bits) - 1;
    let max_acc: u32 = (1 << (from_bits + to_bits - 1)) - 1;

    for &value in data {
        let value = value as u32;
        if (value >> from_bits) != 0 {
            return Err(KtcsError::InvalidData(format!(
                "Invalid value {} for {} bits",
                value, from_bits
            )));
        }
        acc = ((acc << from_bits) | value) & max_acc;
        bits += from_bits;
        while bits >= to_bits {
            bits -= to_bits;
            ret.push(((acc >> bits) & maxv) as u8);
        }
    }

    if pad {
        if bits > 0 {
            ret.push(((acc << (to_bits - bits)) & maxv) as u8);
        }
    } else if bits >= from_bits {
        return Err(KtcsError::InvalidData("Invalid padding".to_string()));
    } else if ((acc << (to_bits - bits)) & maxv) != 0 {
        return Err(KtcsError::InvalidData("Non-zero padding".to_string()));
    }

    Ok(ret)
}

// ========================================
// Sighash Intermediate Hash Functions
// ========================================
// All functions use keyed Blake2b with key "TransactionSigningHash"

/// Create a new keyed Blake2b hasher for transaction signing
fn new_signing_hasher() -> blake2b_simd::State {
    blake2b_simd::Params::new()
        .hash_length(32)
        .key(TRANSACTION_SIGNING_HASH_KEY)
        .to_state()
}

/// Finalize a hasher and return 32-byte hash
fn finalize_hasher(hasher: blake2b_simd::State) -> [u8; 32] {
    let mut output = [0u8; 32];
    output.copy_from_slice(hasher.finalize().as_bytes());
    output
}

/// Compute previousOutputsHash - keyed Blake2b of all input outpoints
/// Per Kaspa spec: for each input, write txId (32 bytes) + index (u32 LE)
fn compute_previous_outputs_hash(inputs: &[SighashInput]) -> [u8; 32] {
    let mut hasher = new_signing_hasher();
    for input in inputs {
        hasher.update(&input.previous_outpoint_hash);
        hasher.update(&input.previous_outpoint_index.to_le_bytes());
    }
    finalize_hasher(hasher)
}

/// Compute sequencesHash - keyed Blake2b of all input sequences
/// Per Kaspa spec: for each input, write sequence (u64 LE)
fn compute_sequences_hash(inputs: &[SighashInput]) -> [u8; 32] {
    let mut hasher = new_signing_hasher();
    for input in inputs {
        hasher.update(&input.sequence.to_le_bytes());
    }
    finalize_hasher(hasher)
}

/// Compute sigOpCountsHash - keyed Blake2b of all input sigOpCounts
/// Per Kaspa spec: for each input, write sigOpCount (u8)
fn compute_sig_op_counts_hash(inputs: &[SighashInput]) -> [u8; 32] {
    let mut hasher = new_signing_hasher();
    for input in inputs {
        hasher.update(&[input.sig_op_count]);
    }
    finalize_hasher(hasher)
}

/// Compute outputsHash - keyed Blake2b of all outputs
/// Per Kaspa spec: for each output, write value (u64 LE) + version (u16 LE) + script length (u64 LE) + script
fn compute_outputs_hash(outputs: &[SighashOutput]) -> [u8; 32] {
    let mut hasher = new_signing_hasher();
    for output in outputs {
        hasher.update(&output.value.to_le_bytes());
        hasher.update(&output.script_public_key_version.to_le_bytes());
        hasher.update(&(output.script_public_key.len() as u64).to_le_bytes());
        hasher.update(&output.script_public_key);
    }
    finalize_hasher(hasher)
}

/// Compute payloadHash - keyed Blake2b of transaction payload
/// Per Kaspa spec: write length (u64 LE) + payload bytes
/// For native subnetwork with empty payload, returns ZERO_HASH
fn compute_payload_hash(payload: &[u8], subnetwork_id: &[u8; 20]) -> [u8; 32] {
    // Native subnetwork ID is all zeros
    let is_native = subnetwork_id.iter().all(|&b| b == 0);

    // For native subnetwork with empty payload, return zero hash
    if is_native && payload.is_empty() {
        return [0u8; 32];
    }

    let mut hasher = new_signing_hasher();
    hasher.update(&(payload.len() as u64).to_le_bytes());
    hasher.update(payload);
    finalize_hasher(hasher)
}

// ========================================
// Main Sighash Computation
// ========================================

/// Compute Kaspa sighash for a specific input.
///
/// This implements the full Kaspa sighash algorithm with 18 fields:
/// 1. version (u16 LE)
/// 2. previousOutputsHash (32 bytes)
/// 3. sequencesHash (32 bytes)
/// 4. sigOpCountsHash (32 bytes)
/// 5. input outpoint txId (32 bytes)
/// 6. input outpoint index (u32 LE)
/// 7. input scriptPubKey version (u16 LE)
/// 8. input scriptPubKey length (u64 LE)
/// 9. input scriptPubKey (variable)
/// 10. input value (u64 LE)
/// 11. input sequence (u64 LE)
/// 12. input sigOpCount (u8)
/// 13. outputsHash (32 bytes)
/// 14. lockTime (u64 LE)
/// 15. subnetworkId (20 bytes)
/// 16. gas (u64 LE)
/// 17. payloadHash (32 bytes)
/// 18. sighash type (u8)
///
/// Reference: https://kaspa-mdbook.aspectron.com/transactions/sighashes.html
pub fn compute_kaspa_sighash(
    tx: &SighashTransaction,
    input_index: usize,
    sighash_type: SigHashType,
) -> Result<[u8; 32]> {
    if input_index >= tx.inputs.len() {
        return Err(KtcsError::InvalidData(format!(
            "Input index {} out of bounds (tx has {} inputs)",
            input_index,
            tx.inputs.len()
        )));
    }

    let input = &tx.inputs[input_index];

    // Compute intermediate hashes based on sighash type
    // For SIGHASH_ALL, we hash all inputs and outputs
    let previous_outputs_hash = compute_previous_outputs_hash(&tx.inputs);
    let sequences_hash = compute_sequences_hash(&tx.inputs);
    let sig_op_counts_hash = compute_sig_op_counts_hash(&tx.inputs);
    let outputs_hash = compute_outputs_hash(&tx.outputs);
    let payload_hash = compute_payload_hash(&tx.payload, &tx.subnetwork_id);

    // Build the final sighash using keyed Blake2b-256
    let mut hasher = new_signing_hasher();

    // Field 1: Version (u16 LE)
    hasher.update(&tx.version.to_le_bytes());

    // Field 2: previousOutputsHash (32 bytes)
    hasher.update(&previous_outputs_hash);

    // Field 3: sequencesHash (32 bytes)
    hasher.update(&sequences_hash);

    // Field 4: sigOpCountsHash (32 bytes)
    hasher.update(&sig_op_counts_hash);

    // Field 5: input outpoint txId (32 bytes)
    hasher.update(&input.previous_outpoint_hash);

    // Field 6: input outpoint index (u32 LE)
    hasher.update(&input.previous_outpoint_index.to_le_bytes());

    // Field 7: input scriptPubKey version (u16 LE)
    hasher.update(&input.script_public_key_version.to_le_bytes());

    // Field 8: input scriptPubKey length (u64 LE)
    hasher.update(&(input.script_public_key.len() as u64).to_le_bytes());

    // Field 9: input scriptPubKey (variable)
    hasher.update(&input.script_public_key);

    // Field 10: input value (u64 LE)
    hasher.update(&input.value.to_le_bytes());

    // Field 11: input sequence (u64 LE)
    hasher.update(&input.sequence.to_le_bytes());

    // Field 12: input sigOpCount (u8)
    hasher.update(&[input.sig_op_count]);

    // Field 13: outputsHash (32 bytes)
    hasher.update(&outputs_hash);

    // Field 14: lockTime (u64 LE)
    hasher.update(&tx.lock_time.to_le_bytes());

    // Field 15: subnetworkId (20 bytes)
    hasher.update(&tx.subnetwork_id);

    // Field 16: gas (u64 LE)
    hasher.update(&tx.gas.to_le_bytes());

    // Field 17: payloadHash (32 bytes)
    hasher.update(&payload_hash);

    // Field 18: sighash type (u8)
    hasher.update(&[sighash_type as u8]);

    Ok(finalize_hasher(hasher))
}

/// Generate a cryptographically secure random private key.
///
/// Uses the system's CSPRNG via getrandom.
#[cfg(feature = "keygen")]
pub fn generate_private_key() -> Result<[u8; 32]> {
    use rand::rngs::OsRng;
    use secp256k1::Secp256k1;

    let secp = Secp256k1::new();
    let (secret_key, _public_key) = secp.generate_keypair(&mut OsRng);

    Ok(secret_key.secret_bytes())
}

/// Generate a new wallet with a random private key.
#[cfg(feature = "keygen")]
pub fn generate_wallet(network: &str) -> Result<KaspaWallet> {
    let private_key = generate_private_key()?;
    KaspaWallet::from_private_key(&private_key, network)
}

/// Securely zeroize a byte array.
pub fn zeroize_bytes(bytes: &mut [u8]) {
    bytes.zeroize();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wallet_creation() {
        // Known test vector private key
        let private_key = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
            0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c,
            0x1d, 0x1e, 0x1f, 0x20,
        ];
        let wallet = KaspaWallet::from_private_key(&private_key, "testnet").unwrap();

        // Kaspa uses colon separator, not bech32's '1'
        assert!(wallet.address().starts_with("kaspatest:"));
        assert_eq!(wallet.network(), "testnet");
        assert_eq!(wallet.public_key().len(), 32);
    }

    #[test]
    fn test_wallet_from_hex() {
        let hex_key = "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";
        let wallet = KaspaWallet::from_hex(hex_key, "mainnet").unwrap();

        // Kaspa uses colon separator
        assert!(wallet.address().starts_with("kaspa:"));
    }

    #[test]
    fn test_wallet_signing_and_verification() {
        let private_key = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
            0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c,
            0x1d, 0x1e, 0x1f, 0x20,
        ];
        let wallet = KaspaWallet::from_private_key(&private_key, "testnet").unwrap();

        let message = [0xab; 32];
        let signature = wallet.sign(&message).unwrap();

        assert_eq!(signature.len(), 64);

        // Verify the signature
        let is_valid =
            KaspaWallet::verify(wallet.x_only_public_key(), &message, &signature).unwrap();
        assert!(is_valid, "Signature should be valid");

        // Different message should fail verification
        let wrong_message = [0xcd; 32];
        let is_valid =
            KaspaWallet::verify(wallet.x_only_public_key(), &wrong_message, &signature).unwrap();
        assert!(!is_valid, "Signature should be invalid for wrong message");
    }

    #[test]
    fn test_invalid_private_key() {
        let short_key = [0x42u8; 16];
        let result = KaspaWallet::from_private_key(&short_key, "testnet");
        assert!(result.is_err());
    }

    #[test]
    fn test_address_encode_decode() {
        let private_key = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
            0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c,
            0x1d, 0x1e, 0x1f, 0x20,
        ];
        let wallet = KaspaWallet::from_private_key(&private_key, "mainnet").unwrap();

        let address = wallet.address();
        let (hrp, pubkey) = decode_address(address).unwrap();

        assert_eq!(hrp, "kaspa");
        assert_eq!(pubkey.len(), 32);
        assert_eq!(pubkey, wallet.public_key().to_vec());
    }

    #[cfg(feature = "keygen")]
    #[test]
    fn test_generate_private_key() {
        let key1 = generate_private_key().unwrap();
        let key2 = generate_private_key().unwrap();

        // Keys should be different (random)
        assert_ne!(key1, key2);
        assert_eq!(key1.len(), 32);

        // Should be valid for wallet creation
        let wallet = KaspaWallet::from_private_key(&key1, "testnet");
        assert!(wallet.is_ok());
    }

    #[cfg(feature = "keygen")]
    #[test]
    fn test_generate_wallet() {
        let wallet = generate_wallet("testnet").unwrap();
        // Kaspa uses colon separator
        assert!(wallet.address().starts_with("kaspatest:"));

        // Signing should work
        let message = [0x42; 32];
        let signature = wallet.sign(&message).unwrap();
        let is_valid =
            KaspaWallet::verify(wallet.x_only_public_key(), &message, &signature).unwrap();
        assert!(is_valid);
    }

    #[test]
    fn test_zeroize_bytes() {
        let mut sensitive = [0x42u8; 32];
        zeroize_bytes(&mut sensitive);
        assert_eq!(sensitive, [0u8; 32]);
    }
}
