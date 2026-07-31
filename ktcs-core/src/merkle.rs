//! Merkle tree implementation for batch aggregation
//!
//! Provides functions to build Merkle trees from digests, generate proofs,
//! and convert Merkle proofs to KTCS operations for inclusion in .kts files.

use crate::error::{KtcsError, Result};
use crate::types::Operation;
use sha2::{Digest, Sha256};

/// Domain-separation prefix for leaf hashing: `leaf_node = SHA256(0x00 || leaf)`.
///
/// Distinct leaf/node prefixes prevent the CVE-2012-2459 second-preimage
/// ambiguity where an internal node could be re-presented as a leaf.
pub const LEAF_PREFIX: u8 = 0x00;
/// Domain-separation prefix for internal nodes: `node = SHA256(0x01 || l || r)`.
pub const NODE_PREFIX: u8 = 0x01;

/// A Merkle tree node position indicator
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    Left,
    Right,
}

/// A sibling in a Merkle proof path
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MerkleSibling {
    /// The sibling's hash
    pub hash: [u8; 32],
    /// Position of the sibling relative to the path
    pub position: Position,
}

/// A Merkle proof linking a leaf to the root
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MerkleProof {
    /// The leaf hash being proven
    pub leaf: [u8; 32],
    /// The index of the leaf in the tree
    pub leaf_index: usize,
    /// Path of siblings from leaf to root
    pub siblings: Vec<MerkleSibling>,
    /// The Merkle root
    pub root: [u8; 32],
}

impl MerkleProof {
    /// Verify that this proof is valid
    pub fn verify(&self) -> bool {
        let computed_root = self.compute_root();
        computed_root == self.root
    }

    /// Compute the root from the leaf and siblings.
    ///
    /// A single-leaf tree (no siblings) has the raw leaf as its root. Otherwise
    /// the raw leaf is first domain-separated into a leaf node
    /// (`SHA256(0x00 || leaf)`) and combined upward with node hashing
    /// (`SHA256(0x01 || l || r)`).
    pub fn compute_root(&self) -> [u8; 32] {
        if self.siblings.is_empty() {
            return self.leaf;
        }

        let mut current = hash_leaf(&self.leaf);

        for sibling in &self.siblings {
            current = match sibling.position {
                Position::Left => hash_node(&sibling.hash, &current),
                Position::Right => hash_node(&current, &sibling.hash),
            };
        }

        current
    }

    /// Convert this Merkle proof to KTCS operations
    ///
    /// The operations transform the raw leaf into the root hash through a series
    /// of prepend/append and SHA256 operations, including the leaf and node
    /// domain-separation prefixes so replay matches the tree's hashing exactly.
    pub fn to_operations(&self) -> Vec<Operation> {
        let mut ops = Vec::new();

        // Single-leaf tree: the leaf is the root; identity transform.
        if self.siblings.is_empty() {
            return ops;
        }

        // Leaf domain separation: leaf_node = SHA256(0x00 || leaf)
        ops.push(Operation::Prepend(vec![LEAF_PREFIX]));
        ops.push(Operation::Sha256);

        for sibling in &self.siblings {
            match sibling.position {
                Position::Left => {
                    // Sibling on the left: prepend it -> sibling || current
                    ops.push(Operation::Prepend(sibling.hash.to_vec()));
                }
                Position::Right => {
                    // Sibling on the right: append it -> current || sibling
                    ops.push(Operation::Append(sibling.hash.to_vec()));
                }
            }
            // Node domain separation: node = SHA256(0x01 || ...)
            ops.push(Operation::Prepend(vec![NODE_PREFIX]));
            ops.push(Operation::Sha256);
        }

        ops
    }
}

/// A complete Merkle tree
#[derive(Debug, Clone)]
pub struct MerkleTree {
    /// All leaves (original hashes)
    leaves: Vec<[u8; 32]>,
    /// All layers of the tree, from leaves to root
    /// layers[0] = leaves, layers[n-1] = [root]
    layers: Vec<Vec<[u8; 32]>>,
}

impl MerkleTree {
    /// Build a Merkle tree from a set of leaf hashes
    ///
    /// If the number of leaves is not a power of 2, the last leaf is duplicated
    /// to balance the tree.
    pub fn build(leaves: Vec<[u8; 32]>) -> Result<Self> {
        if leaves.is_empty() {
            return Err(KtcsError::EmptyLeafSet);
        }

        // If only one leaf, the root is the leaf itself
        if leaves.len() == 1 {
            return Ok(Self {
                leaves: leaves.clone(),
                layers: vec![leaves],
            });
        }

        let mut layers = Vec::new();

        // Bottom layer holds DOMAIN-SEPARATED leaf hashes (SHA256(0x00 || leaf)),
        // not the raw leaves, so an internal node can never be re-presented as a
        // leaf. Raw leaves are retained separately in `self.leaves` for indexing.
        let mut current_layer: Vec<[u8; 32]> = leaves.iter().map(hash_leaf).collect();
        // Pad to even number if necessary (duplicate last element)
        if current_layer.len() % 2 != 0 {
            current_layer.push(*current_layer.last().unwrap());
        }
        layers.push(current_layer.clone());

        // Build tree layers from bottom to top
        while current_layer.len() > 1 {
            let mut next_layer = Vec::new();

            for chunk in current_layer.chunks(2) {
                let hash = hash_node(&chunk[0], &chunk[1]);
                next_layer.push(hash);
            }

            // Pad if necessary
            if next_layer.len() > 1 && next_layer.len() % 2 != 0 {
                next_layer.push(*next_layer.last().unwrap());
            }

            layers.push(next_layer.clone());
            current_layer = next_layer;
        }

        Ok(Self { leaves, layers })
    }

    /// Get the Merkle root
    pub fn root(&self) -> [u8; 32] {
        self.layers.last().unwrap()[0]
    }

    /// Get the number of leaves
    pub fn len(&self) -> usize {
        self.leaves.len()
    }

    /// Check if the tree is empty
    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty()
    }

    /// Get a proof for the leaf at the given index
    pub fn get_proof(&self, index: usize) -> Result<MerkleProof> {
        if index >= self.leaves.len() {
            return Err(KtcsError::LeafIndexOutOfBounds {
                index,
                size: self.leaves.len(),
            });
        }

        let leaf = self.leaves[index];
        let mut siblings = Vec::new();
        let mut current_index = index;

        for layer in &self.layers[..self.layers.len() - 1] {
            // Validate index is within bounds - no silent clamping
            let layer_index = current_index;
            debug_assert!(
                layer_index < layer.len(),
                "Merkle proof index {} out of bounds for layer of size {}",
                layer_index, layer.len()
            );
            let sibling_index = if layer_index % 2 == 0 {
                layer_index + 1
            } else {
                layer_index - 1
            };

            // Guard: sibling_index may equal layer.len() in edge cases with odd-sized
            // layers before padding propagates. In such cases, no sibling is added
            // because the element is implicitly paired with itself (duplicated).
            if sibling_index < layer.len() {
                let position = if layer_index % 2 == 0 {
                    Position::Right
                } else {
                    Position::Left
                };

                siblings.push(MerkleSibling {
                    hash: layer[sibling_index],
                    position,
                });
            }

            current_index /= 2;
        }

        Ok(MerkleProof {
            leaf,
            leaf_index: index,
            siblings,
            root: self.root(),
        })
    }

    /// Get all leaves
    pub fn leaves(&self) -> &[[u8; 32]] {
        &self.leaves
    }
}

/// Hash a raw leaf with the leaf domain-separation prefix: `SHA256(0x00 || leaf)`.
fn hash_leaf(leaf: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update([LEAF_PREFIX]);
    hasher.update(leaf);
    let result = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}

/// Hash two child nodes with the node domain-separation prefix:
/// `SHA256(0x01 || left || right)`.
fn hash_node(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update([NODE_PREFIX]);
    hasher.update(left);
    hasher.update(right);
    let result = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}

/// Compute SHA256 hash of data
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}

/// Compute SHA256 hash of data, returning as Vec<u8>
pub fn sha256_vec(data: &[u8]) -> Vec<u8> {
    sha256(data).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_leaf(value: u8) -> [u8; 32] {
        let mut leaf = [0u8; 32];
        leaf[0] = value;
        leaf
    }

    #[test]
    fn test_single_leaf() {
        let leaf = make_leaf(1);
        let tree = MerkleTree::build(vec![leaf]).unwrap();

        assert_eq!(tree.len(), 1);
        assert_eq!(tree.root(), leaf);

        let proof = tree.get_proof(0).unwrap();
        assert!(proof.verify());
        assert_eq!(proof.siblings.len(), 0);
    }

    #[test]
    fn test_two_leaves() {
        let leaves = vec![make_leaf(1), make_leaf(2)];
        let tree = MerkleTree::build(leaves.clone()).unwrap();

        assert_eq!(tree.len(), 2);

        // Root should be node-hash of the two domain-separated leaf hashes.
        let expected_root = hash_node(&hash_leaf(&leaves[0]), &hash_leaf(&leaves[1]));
        assert_eq!(tree.root(), expected_root);

        // Test proofs. Siblings at the leaf level are the HASHED leaf values.
        let proof0 = tree.get_proof(0).unwrap();
        assert!(proof0.verify());
        assert_eq!(proof0.siblings.len(), 1);
        assert_eq!(proof0.siblings[0].hash, hash_leaf(&leaves[1]));
        assert_eq!(proof0.siblings[0].position, Position::Right);

        let proof1 = tree.get_proof(1).unwrap();
        assert!(proof1.verify());
        assert_eq!(proof1.siblings.len(), 1);
        assert_eq!(proof1.siblings[0].hash, hash_leaf(&leaves[0]));
        assert_eq!(proof1.siblings[0].position, Position::Left);
    }

    #[test]
    fn test_four_leaves() {
        let leaves: Vec<[u8; 32]> = (1..=4).map(make_leaf).collect();
        let tree = MerkleTree::build(leaves.clone()).unwrap();

        assert_eq!(tree.len(), 4);

        // All proofs should verify
        for i in 0..4 {
            let proof = tree.get_proof(i).unwrap();
            assert!(proof.verify(), "Proof for leaf {} failed", i);
            assert_eq!(proof.siblings.len(), 2);
        }
    }

    #[test]
    fn test_three_leaves_padding() {
        let leaves: Vec<[u8; 32]> = (1..=3).map(make_leaf).collect();
        let tree = MerkleTree::build(leaves.clone()).unwrap();

        assert_eq!(tree.len(), 3);

        // All proofs should verify
        for i in 0..3 {
            let proof = tree.get_proof(i).unwrap();
            assert!(proof.verify(), "Proof for leaf {} failed", i);
        }
    }

    #[test]
    fn test_proof_to_operations() {
        let leaves = vec![make_leaf(1), make_leaf(2)];
        let tree = MerkleTree::build(leaves.clone()).unwrap();

        let proof = tree.get_proof(0).unwrap();
        let ops = proof.to_operations();

        // With domain separation the ops are:
        //   Prepend(0x00), Sha256      -> leaf hash
        //   Append(hash_leaf(sibling)), Prepend(0x01), Sha256 -> node hash
        assert_eq!(ops.len(), 5);
        assert!(matches!(&ops[0], Operation::Prepend(data) if data == &vec![LEAF_PREFIX]));
        assert!(matches!(&ops[1], Operation::Sha256));
        assert!(
            matches!(&ops[2], Operation::Append(data) if data == &hash_leaf(&leaves[1]).to_vec())
        );
        assert!(matches!(&ops[3], Operation::Prepend(data) if data == &vec![NODE_PREFIX]));
        assert!(matches!(&ops[4], Operation::Sha256));

        // The ops must replay the leaf to the tree root.
        let replayed = crate::ops::apply_operations(&leaves[0], &ops).unwrap();
        assert_eq!(replayed, tree.root().to_vec());
    }

    #[test]
    fn test_empty_tree() {
        let result = MerkleTree::build(vec![]);
        assert!(matches!(result, Err(KtcsError::EmptyLeafSet)));
    }

    #[test]
    fn test_index_out_of_bounds() {
        let leaves = vec![make_leaf(1), make_leaf(2)];
        let tree = MerkleTree::build(leaves).unwrap();

        let result = tree.get_proof(5);
        assert!(matches!(
            result,
            Err(KtcsError::LeafIndexOutOfBounds { index: 5, size: 2 })
        ));
    }

    #[test]
    fn test_sha256() {
        let data = b"hello world";
        let hash = sha256(data);

        // Known SHA256 of "hello world"
        let expected = hex::decode("b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9")
            .unwrap();
        assert_eq!(hash.to_vec(), expected);
    }

    #[test]
    fn test_large_tree() {
        let leaves: Vec<[u8; 32]> = (0..100).map(|i| make_leaf(i as u8)).collect();
        let tree = MerkleTree::build(leaves).unwrap();

        assert_eq!(tree.len(), 100);

        // Verify all proofs
        for i in 0..100 {
            let proof = tree.get_proof(i).unwrap();
            assert!(proof.verify(), "Proof for leaf {} failed", i);
        }
    }
}
