//! Merkle tree implementation for batch aggregation
//!
//! Provides functions to build Merkle trees from digests, generate proofs,
//! and convert Merkle proofs to KTCS operations for inclusion in .kts files.

use crate::error::{KtcsError, Result};
use crate::types::Operation;
use sha2::{Digest, Sha256};

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

    /// Compute the root from the leaf and siblings
    pub fn compute_root(&self) -> [u8; 32] {
        let mut current = self.leaf;

        for sibling in &self.siblings {
            current = match sibling.position {
                Position::Left => hash_pair(&sibling.hash, &current),
                Position::Right => hash_pair(&current, &sibling.hash),
            };
        }

        current
    }

    /// Convert this Merkle proof to KTCS operations
    ///
    /// The operations will transform the leaf hash into the root hash
    /// through a series of append/prepend and SHA256 operations.
    pub fn to_operations(&self) -> Vec<Operation> {
        let mut ops = Vec::new();

        for sibling in &self.siblings {
            match sibling.position {
                Position::Left => {
                    // Sibling is on the left, so we prepend it
                    ops.push(Operation::Prepend(sibling.hash.to_vec()));
                }
                Position::Right => {
                    // Sibling is on the right, so we append it
                    ops.push(Operation::Append(sibling.hash.to_vec()));
                }
            }
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

        // Pad to even number if necessary (duplicate last element)
        let mut current_layer = leaves.clone();
        if current_layer.len() % 2 != 0 {
            current_layer.push(*current_layer.last().unwrap());
        }
        layers.push(current_layer.clone());

        // Build tree layers from bottom to top
        while current_layer.len() > 1 {
            let mut next_layer = Vec::new();

            for chunk in current_layer.chunks(2) {
                let hash = hash_pair(&chunk[0], &chunk[1]);
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

        // Handle padding: if index is past padded length, use padded index
        if self.layers[0].len() > self.leaves.len() && index == self.leaves.len() - 1 {
            // Last leaf might have been duplicated
        }

        for layer in &self.layers[..self.layers.len() - 1] {
            // Adjust index for padded layers
            let layer_index = current_index.min(layer.len() - 1);
            let sibling_index = if layer_index % 2 == 0 {
                layer_index + 1
            } else {
                layer_index - 1
            };

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

/// Hash two 32-byte values together using SHA256
fn hash_pair(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
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

        // Root should be hash of both leaves
        let expected_root = hash_pair(&leaves[0], &leaves[1]);
        assert_eq!(tree.root(), expected_root);

        // Test proofs
        let proof0 = tree.get_proof(0).unwrap();
        assert!(proof0.verify());
        assert_eq!(proof0.siblings.len(), 1);
        assert_eq!(proof0.siblings[0].hash, leaves[1]);
        assert_eq!(proof0.siblings[0].position, Position::Right);

        let proof1 = tree.get_proof(1).unwrap();
        assert!(proof1.verify());
        assert_eq!(proof1.siblings.len(), 1);
        assert_eq!(proof1.siblings[0].hash, leaves[0]);
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

        // Should have: append sibling, sha256
        assert_eq!(ops.len(), 2);
        assert!(matches!(&ops[0], Operation::Append(data) if data == &leaves[1].to_vec()));
        assert!(matches!(&ops[1], Operation::Sha256));
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
