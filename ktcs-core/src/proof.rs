//! Proof serialization and deserialization for .kts format
//!
//! Implements the binary proof format as specified in the KTCS technical specification.

use crate::error::{KtcsError, Result};
use crate::types::*;

/// Maximum number of parent hashes allowed (Kaspa blocks typically have ~2 parents)
const MAX_PARENT_HASHES: u64 = 1024;
/// Maximum operation data size (1MB)
const MAX_OPERATION_DATA_SIZE: u64 = 1024 * 1024;
/// Maximum URL length
const MAX_URL_LENGTH: u64 = 2048;
/// Maximum number of operations in a proof (DoS prevention)
const MAX_OPERATIONS: usize = 10_000;
/// Maximum number of attestations in a proof (DoS prevention)
const MAX_ATTESTATIONS: usize = 100;

/// Serialize a KTCS proof to binary .kts format
pub fn serialize_proof(proof: &KtcsProof) -> Vec<u8> {
    let mut buf = Vec::new();

    // Header (26 bytes)
    buf.extend_from_slice(KTCS_MAGIC); // 18 bytes
    buf.push(proof.version); // 1 byte
    buf.push(proof.hash_algorithm as u8); // 1 byte
    buf.push(proof.flags); // 1 byte
    buf.extend_from_slice(&[0x00; 5]); // 5 bytes reserved

    // Digest
    buf.extend_from_slice(&proof.digest);

    // Operations
    for op in &proof.operations {
        serialize_operation(&mut buf, op);
    }

    // Attestations
    for attestation in &proof.attestations {
        serialize_attestation(&mut buf, attestation);
    }

    buf
}

/// Deserialize a KTCS proof from binary .kts format
pub fn deserialize_proof(data: &[u8]) -> Result<KtcsProof> {
    // Check minimum length for header
    if data.len() < 26 {
        return Err(KtcsError::UnexpectedEof);
    }

    // Validate magic bytes
    if &data[0..18] != KTCS_MAGIC {
        return Err(KtcsError::InvalidMagicBytes);
    }
    let mut cursor = 18;

    // Version
    let version = data[cursor];
    if version != PROOF_VERSION {
        return Err(KtcsError::UnsupportedVersion(version));
    }
    cursor += 1;

    // Hash algorithm
    let hash_algorithm = HashAlgorithm::try_from(data[cursor])?;
    cursor += 1;

    // Flags
    let flags = data[cursor];
    cursor += 1;

    // Reserved (skip 5 bytes)
    cursor += 5;

    // Digest (size depends on hash algorithm)
    let digest_len = match hash_algorithm {
        HashAlgorithm::Sha256 => 32,
        HashAlgorithm::Keccak256 => 32,
        HashAlgorithm::Ripemd160 => 20,
    };

    if data.len() < cursor + digest_len {
        return Err(KtcsError::UnexpectedEof);
    }
    let digest = data[cursor..cursor + digest_len].to_vec();
    cursor += digest_len;

    // Parse operations and attestations
    let mut operations = Vec::new();
    let mut attestations = Vec::new();
    // Operations must all precede attestations. Once an attestation has been
    // seen, a subsequent operation is malformed (the canonical layout is
    // digest -> operations -> attestations).
    let mut seen_attestation = false;

    while cursor < data.len() {
        let tag = data[cursor];

        // Check if this is an attestation tag
        if tag == AttestationTag::Pending as u8
            || tag == AttestationTag::KaspaBlock as u8
            || tag == AttestationTag::Bitcoin as u8
        {
            let (attestation, new_cursor) = deserialize_attestation(data, cursor)?;
            attestations.push(attestation);
            cursor = new_cursor;
            seen_attestation = true;
            // Security: Prevent DoS via excessive attestation count
            if attestations.len() > MAX_ATTESTATIONS {
                return Err(KtcsError::InvalidData(
                    format!("Too many attestations: max {}", MAX_ATTESTATIONS)
                ));
            }
        } else {
            // It's an operation
            if seen_attestation {
                return Err(KtcsError::InvalidData(
                    "Operation found after an attestation (operations must precede attestations)"
                        .to_string(),
                ));
            }
            let (operation, new_cursor) = deserialize_operation(data, cursor)?;
            operations.push(operation);
            cursor = new_cursor;
            // Security: Prevent DoS via excessive operation count
            if operations.len() > MAX_OPERATIONS {
                return Err(KtcsError::InvalidData(
                    format!("Too many operations: max {}", MAX_OPERATIONS)
                ));
            }
        }
    }

    Ok(KtcsProof {
        version,
        hash_algorithm,
        flags,
        digest,
        operations,
        attestations,
    })
}

/// Serialize an operation to the buffer
fn serialize_operation(buf: &mut Vec<u8>, op: &Operation) {
    match op {
        Operation::Append(data) => {
            buf.push(OpTag::Append as u8);
            encode_varint(buf, data.len() as u64);
            buf.extend_from_slice(data);
        }
        Operation::Prepend(data) => {
            buf.push(OpTag::Prepend as u8);
            encode_varint(buf, data.len() as u64);
            buf.extend_from_slice(data);
        }
        Operation::Sha256 => {
            buf.push(OpTag::Sha256 as u8);
        }
        Operation::Ripemd160 => {
            buf.push(OpTag::Ripemd160 as u8);
        }
        Operation::Keccak256 => {
            buf.push(OpTag::Keccak256 as u8);
        }
        Operation::Fork(count) => {
            buf.push(OpTag::Fork as u8);
            encode_varint(buf, *count as u64);
        }
    }
}

/// Deserialize an operation from data at the given cursor position
fn deserialize_operation(data: &[u8], cursor: usize) -> Result<(Operation, usize)> {
    if cursor >= data.len() {
        return Err(KtcsError::UnexpectedEof);
    }

    let tag = OpTag::try_from(data[cursor])?;
    let mut pos = cursor + 1;

    let op = match tag {
        OpTag::Append => {
            let (len, new_pos) = decode_varint(data, pos)?;
            pos = new_pos;
            // Security: Prevent DoS via excessive allocation
            if len > MAX_OPERATION_DATA_SIZE {
                return Err(KtcsError::InvalidData(
                    format!("Append operation data size {} exceeds maximum {}", len, MAX_OPERATION_DATA_SIZE)
                ));
            }
            if data.len() < pos + len as usize {
                return Err(KtcsError::UnexpectedEof);
            }
            let op_data = data[pos..pos + len as usize].to_vec();
            pos += len as usize;
            Operation::Append(op_data)
        }
        OpTag::Prepend => {
            let (len, new_pos) = decode_varint(data, pos)?;
            pos = new_pos;
            // Security: Prevent DoS via excessive allocation
            if len > MAX_OPERATION_DATA_SIZE {
                return Err(KtcsError::InvalidData(
                    format!("Prepend operation data size {} exceeds maximum {}", len, MAX_OPERATION_DATA_SIZE)
                ));
            }
            if data.len() < pos + len as usize {
                return Err(KtcsError::UnexpectedEof);
            }
            let op_data = data[pos..pos + len as usize].to_vec();
            pos += len as usize;
            Operation::Prepend(op_data)
        }
        OpTag::Sha256 => Operation::Sha256,
        OpTag::Ripemd160 => Operation::Ripemd160,
        OpTag::Keccak256 => Operation::Keccak256,
        OpTag::Fork => {
            let (count, new_pos) = decode_varint(data, pos)?;
            pos = new_pos;
            // Reject counts that would truncate on the u64 -> u32 narrowing.
            if count > u32::MAX as u64 {
                return Err(KtcsError::InvalidData(format!(
                    "Fork count {} exceeds u32::MAX",
                    count
                )));
            }
            Operation::Fork(count as u32)
        }
    };

    Ok((op, pos))
}

/// Serialize an attestation to the buffer
fn serialize_attestation(buf: &mut Vec<u8>, attestation: &Attestation) {
    match attestation {
        Attestation::Pending(pending) => {
            buf.push(AttestationTag::Pending as u8);
            let url_bytes = pending.calendar_url.as_bytes();
            encode_varint(buf, url_bytes.len() as u64);
            buf.extend_from_slice(url_bytes);
        }
        Attestation::Kaspa(ka) => {
            buf.push(AttestationTag::KaspaBlock as u8);
            buf.push(ka.version);
            buf.extend_from_slice(&ka.daa_score.to_le_bytes());
            buf.extend_from_slice(&ka.blue_score.to_le_bytes());
            buf.extend_from_slice(&ka.block_hash);
            buf.extend_from_slice(&ka.timestamp.to_le_bytes());
            buf.extend_from_slice(&ka.tx_hash);
            buf.extend_from_slice(&ka.tx_index.to_le_bytes());
            buf.extend_from_slice(&ka.blue_work);
            encode_varint(buf, ka.parent_hashes.len() as u64);
            for parent in &ka.parent_hashes {
                buf.extend_from_slice(parent);
            }
        }
        Attestation::Bitcoin(btc) => {
            buf.push(AttestationTag::Bitcoin as u8);
            buf.extend_from_slice(&btc.block_height.to_le_bytes());
        }
    }
}

/// Deserialize an attestation from data at the given cursor position
fn deserialize_attestation(data: &[u8], cursor: usize) -> Result<(Attestation, usize)> {
    if cursor >= data.len() {
        return Err(KtcsError::UnexpectedEof);
    }

    let tag = AttestationTag::try_from(data[cursor])?;
    let mut pos = cursor + 1;

    let attestation = match tag {
        AttestationTag::Pending => {
            let (url_len, new_pos) = decode_varint(data, pos)?;
            pos = new_pos;
            // Security: Prevent DoS via excessive URL allocation
            if url_len > MAX_URL_LENGTH {
                return Err(KtcsError::InvalidData(
                    format!("URL length {} exceeds maximum {}", url_len, MAX_URL_LENGTH)
                ));
            }
            if data.len() < pos + url_len as usize {
                return Err(KtcsError::UnexpectedEof);
            }
            let url = String::from_utf8(data[pos..pos + url_len as usize].to_vec())?;
            pos += url_len as usize;
            Attestation::Pending(PendingAttestation { calendar_url: url })
        }
        AttestationTag::KaspaBlock => {
            // Need at least: version(1) + daa_score(8) + blue_score(8) + block_hash(32) +
            // timestamp(8) + tx_hash(32) + tx_index(4) + blue_work(32) = 125 bytes
            if data.len() < pos + 125 {
                return Err(KtcsError::UnexpectedEof);
            }

            let version = data[pos];
            pos += 1;

            let daa_score = u64::from_le_bytes(
                data[pos..pos + 8]
                    .try_into()
                    .map_err(|_| KtcsError::InvalidData("Invalid daa_score bytes".to_string()))?
            );
            pos += 8;

            let blue_score = u64::from_le_bytes(
                data[pos..pos + 8]
                    .try_into()
                    .map_err(|_| KtcsError::InvalidData("Invalid blue_score bytes".to_string()))?
            );
            pos += 8;

            let mut block_hash = [0u8; 32];
            block_hash.copy_from_slice(&data[pos..pos + 32]);
            pos += 32;

            let timestamp = u64::from_le_bytes(
                data[pos..pos + 8]
                    .try_into()
                    .map_err(|_| KtcsError::InvalidData("Invalid timestamp bytes".to_string()))?
            );
            pos += 8;

            let mut tx_hash = [0u8; 32];
            tx_hash.copy_from_slice(&data[pos..pos + 32]);
            pos += 32;

            let tx_index = u32::from_le_bytes(
                data[pos..pos + 4]
                    .try_into()
                    .map_err(|_| KtcsError::InvalidData("Invalid tx_index bytes".to_string()))?
            );
            pos += 4;

            let mut blue_work = [0u8; 32];
            blue_work.copy_from_slice(&data[pos..pos + 32]);
            pos += 32;

            let (parent_count, new_pos) = decode_varint(data, pos)?;
            pos = new_pos;

            // Security: Prevent DoS via excessive allocation
            if parent_count > MAX_PARENT_HASHES {
                return Err(KtcsError::InvalidData(
                    format!("Parent count {} exceeds maximum {}", parent_count, MAX_PARENT_HASHES)
                ));
            }

            let mut parent_hashes = Vec::with_capacity(parent_count as usize);
            for _ in 0..parent_count {
                if data.len() < pos + 32 {
                    return Err(KtcsError::UnexpectedEof);
                }
                let mut parent = [0u8; 32];
                parent.copy_from_slice(&data[pos..pos + 32]);
                parent_hashes.push(parent);
                pos += 32;
            }

            Attestation::Kaspa(KaspaAttestation {
                version,
                daa_score,
                blue_score,
                block_hash,
                timestamp,
                tx_hash,
                tx_index,
                blue_work,
                parent_hashes,
            })
        }
        AttestationTag::Bitcoin => {
            if data.len() < pos + 4 {
                return Err(KtcsError::UnexpectedEof);
            }
            let block_height = u32::from_le_bytes(
                data[pos..pos + 4]
                    .try_into()
                    .map_err(|_| KtcsError::InvalidData("Invalid block_height bytes".to_string()))?
            );
            pos += 4;
            Attestation::Bitcoin(BitcoinAttestation { block_height })
        }
    };

    Ok((attestation, pos))
}

/// Encode a value as a Bitcoin-style varint
fn encode_varint(buf: &mut Vec<u8>, value: u64) {
    if value < 0xFD {
        buf.push(value as u8);
    } else if value <= 0xFFFF {
        buf.push(0xFD);
        buf.extend_from_slice(&(value as u16).to_le_bytes());
    } else if value <= 0xFFFF_FFFF {
        buf.push(0xFE);
        buf.extend_from_slice(&(value as u32).to_le_bytes());
    } else {
        buf.push(0xFF);
        buf.extend_from_slice(&value.to_le_bytes());
    }
}

/// Decode a Bitcoin-style varint from data at the given position
fn decode_varint(data: &[u8], pos: usize) -> Result<(u64, usize)> {
    if pos >= data.len() {
        return Err(KtcsError::UnexpectedEof);
    }

    let first = data[pos];

    if first < 0xFD {
        Ok((first as u64, pos + 1))
    } else if first == 0xFD {
        if data.len() < pos + 3 {
            return Err(KtcsError::UnexpectedEof);
        }
        let bytes: [u8; 2] = data[pos + 1..pos + 3]
            .try_into()
            .map_err(|_| KtcsError::InvalidVarint)?;
        let value = u16::from_le_bytes(bytes);
        Ok((value as u64, pos + 3))
    } else if first == 0xFE {
        if data.len() < pos + 5 {
            return Err(KtcsError::UnexpectedEof);
        }
        let bytes: [u8; 4] = data[pos + 1..pos + 5]
            .try_into()
            .map_err(|_| KtcsError::InvalidVarint)?;
        let value = u32::from_le_bytes(bytes);
        Ok((value as u64, pos + 5))
    } else {
        // first == 0xFF
        if data.len() < pos + 9 {
            return Err(KtcsError::UnexpectedEof);
        }
        let bytes: [u8; 8] = data[pos + 1..pos + 9]
            .try_into()
            .map_err(|_| KtcsError::InvalidVarint)?;
        let value = u64::from_le_bytes(bytes);
        Ok((value, pos + 9))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_varint_encoding() {
        let mut buf = Vec::new();

        // Test small values
        encode_varint(&mut buf, 0);
        assert_eq!(buf, vec![0x00]);

        buf.clear();
        encode_varint(&mut buf, 252);
        assert_eq!(buf, vec![252]);

        // Test 2-byte encoding
        buf.clear();
        encode_varint(&mut buf, 253);
        assert_eq!(buf, vec![0xFD, 253, 0]);

        buf.clear();
        encode_varint(&mut buf, 0xFFFF);
        assert_eq!(buf, vec![0xFD, 0xFF, 0xFF]);

        // Test 4-byte encoding
        buf.clear();
        encode_varint(&mut buf, 0x10000);
        assert_eq!(buf, vec![0xFE, 0x00, 0x00, 0x01, 0x00]);
    }

    #[test]
    fn test_varint_roundtrip() {
        let test_values = [0, 1, 252, 253, 0xFFFF, 0x10000, 0xFFFF_FFFF, 0x1_0000_0000];

        for &value in &test_values {
            let mut buf = Vec::new();
            encode_varint(&mut buf, value);
            let (decoded, _) = decode_varint(&buf, 0).unwrap();
            assert_eq!(decoded, value, "Failed for value {}", value);
        }
    }

    #[test]
    fn test_proof_roundtrip() {
        let digest = vec![0xab; 32];
        let mut proof = KtcsProof::new(digest.clone());

        // Add some operations
        proof.add_operation(Operation::Append(vec![0x01, 0x02, 0x03]));
        proof.add_operation(Operation::Sha256);
        proof.add_operation(Operation::Prepend(vec![0x04, 0x05]));
        proof.add_operation(Operation::Sha256);

        // Add a pending attestation
        proof.add_attestation(Attestation::Pending(PendingAttestation {
            calendar_url: "https://calendar.ktcs.example.com".to_string(),
        }));

        // Add a Kaspa attestation
        proof.add_attestation(Attestation::Kaspa(KaspaAttestation::new(
            42000000,
            41500000,
            [0xde; 32],
            1706000000000,
            [0xab; 32],
            0,
            [0x12; 32],
            vec![[0x11; 32], [0x22; 32], [0x33; 32]],
        )));

        // Serialize and deserialize
        let serialized = serialize_proof(&proof);
        let deserialized = deserialize_proof(&serialized).unwrap();

        assert_eq!(deserialized.version, proof.version);
        assert_eq!(deserialized.hash_algorithm, proof.hash_algorithm);
        assert_eq!(deserialized.digest, proof.digest);
        assert_eq!(deserialized.operations.len(), proof.operations.len());
        assert_eq!(deserialized.attestations.len(), proof.attestations.len());

        // Verify operations
        for (orig, deser) in proof.operations.iter().zip(deserialized.operations.iter()) {
            assert_eq!(orig, deser);
        }

        // Verify attestations
        for (orig, deser) in proof
            .attestations
            .iter()
            .zip(deserialized.attestations.iter())
        {
            assert_eq!(orig, deser);
        }
    }

    #[test]
    fn test_invalid_magic() {
        let data = vec![0x00; 50];
        let result = deserialize_proof(&data);
        assert!(matches!(result, Err(KtcsError::InvalidMagicBytes)));
    }

    #[test]
    fn test_proof_file_extension() {
        // Verify magic bytes match spec
        assert_eq!(KTCS_MAGIC[0], 0x00);
        assert_eq!(&KTCS_MAGIC[1..10], b"KaspaTime");
        assert_eq!(KTCS_MAGIC[10], 0x00);
        assert_eq!(KTCS_MAGIC[11], 0x00);
        assert_eq!(&KTCS_MAGIC[12..17], b"Proof");
        assert_eq!(KTCS_MAGIC[17], 0x00);
    }
}
