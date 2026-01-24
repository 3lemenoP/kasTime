//! Wallet support for direct Kaspa timestamping
//!
//! This module provides wallet functionality for signing transactions
//! when using direct stamping mode (bypassing the calendar server).

use crate::error::{KtcsError, Result};
use bech32::{Bech32m, Hrp};
use secp256k1::{Keypair, Message, Secp256k1, SecretKey, XOnlyPublicKey};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// A Kaspa wallet for signing transactions.
///
/// Private key is automatically zeroed when the wallet is dropped.
#[derive(Clone, ZeroizeOnDrop)]
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

    /// Sign a SIGHASH for a transaction input.
    pub fn sign_transaction_input(
        &self,
        prev_output_script: &[u8],
        value: u64,
        input_index: u32,
        transaction_hash: &[u8; 32],
    ) -> Result<[u8; 64]> {
        let sighash = compute_sighash(prev_output_script, value, input_index, transaction_hash)?;
        self.sign(&sighash)
    }
}

/// Encode a public key as a Kaspa bech32m address.
///
/// Kaspa uses schnorr addresses with version 0 (0x00) and address type 1 (schnorr).
/// The payload is: version (1 byte) + pubkey (32 bytes)
fn encode_address(public_key: &XOnlyPublicKey, network: &str) -> Result<String> {
    let hrp_str = match network {
        "mainnet" => "kaspa",
        "testnet-10" | "testnet-11" | "testnet" => "kaspatest",
        _ => network,
    };

    let hrp = Hrp::parse(hrp_str)
        .map_err(|e| KtcsError::InvalidData(format!("Invalid network prefix: {}", e)))?;

    // Kaspa address format: version byte (0x00 for schnorr) + 32-byte x-only pubkey
    let mut payload = Vec::with_capacity(33);
    payload.push(0x00); // Version 0 = schnorr address
    payload.extend_from_slice(&public_key.serialize());

    let address = bech32::encode::<Bech32m>(hrp, &payload)
        .map_err(|e| KtcsError::InvalidData(format!("Bech32m encoding failed: {}", e)))?;

    Ok(address)
}

/// Decode a Kaspa bech32m address to extract the public key.
pub fn decode_address(address: &str) -> Result<(String, Vec<u8>)> {
    let (hrp, payload) = bech32::decode(address)
        .map_err(|e| KtcsError::InvalidData(format!("Invalid bech32m address: {}", e)))?;

    if payload.is_empty() {
        return Err(KtcsError::InvalidData("Empty address payload".to_string()));
    }

    let version = payload[0];
    let pubkey_hash = payload[1..].to_vec();

    // Version 0 = schnorr (32-byte pubkey)
    // Version 8 = ECDSA (33-byte pubkey)
    match version {
        0x00 => {
            if pubkey_hash.len() != 32 {
                return Err(KtcsError::InvalidData(format!(
                    "Invalid schnorr pubkey length: expected 32, got {}",
                    pubkey_hash.len()
                )));
            }
        }
        0x08 => {
            if pubkey_hash.len() != 33 {
                return Err(KtcsError::InvalidData(format!(
                    "Invalid ECDSA pubkey length: expected 33, got {}",
                    pubkey_hash.len()
                )));
            }
        }
        _ => {
            return Err(KtcsError::InvalidData(format!(
                "Unknown address version: {}",
                version
            )));
        }
    }

    Ok((hrp.to_string(), pubkey_hash))
}

/// Compute SIGHASH for a transaction input.
///
/// This follows Kaspa's sighash computation for signing.
fn compute_sighash(
    prev_output_script: &[u8],
    value: u64,
    input_index: u32,
    transaction_hash: &[u8; 32],
) -> Result<[u8; 32]> {
    let mut hasher = Sha256::new();

    // Domain separation tag
    hasher.update(b"TransactionSigningHashECDSA");

    // Transaction hash
    hasher.update(transaction_hash);

    // Input index
    hasher.update(input_index.to_le_bytes());

    // Previous output script length and data
    hasher.update((prev_output_script.len() as u64).to_le_bytes());
    hasher.update(prev_output_script);

    // Value
    hasher.update(value.to_le_bytes());

    // SIGHASH type (ALL = 0x01)
    hasher.update([0x01]);

    let digest = hasher.finalize();
    let mut output = [0u8; 32];
    output.copy_from_slice(&digest);
    Ok(output)
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

        assert!(wallet.address().starts_with("kaspatest1"));
        assert_eq!(wallet.network(), "testnet");
        assert_eq!(wallet.public_key().len(), 32);
    }

    #[test]
    fn test_wallet_from_hex() {
        let hex_key = "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";
        let wallet = KaspaWallet::from_hex(hex_key, "mainnet").unwrap();

        assert!(wallet.address().starts_with("kaspa1"));
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
        assert!(wallet.address().starts_with("kaspatest1"));

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
