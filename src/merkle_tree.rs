//! Merkle tree implementation for data integrity verification.
//!
//! This module provides a complete implementation of a binary Merkle tree
//! that can be used to verify the integrity of data chunks using cryptographic hashes.

use crate::{node::Node, utils::sha256};
use serde_json::{self, Value, json};

/// Binary tree for data integrity verification using cryptographic hashes.
///
/// A Merkle tree allows efficient and secure verification of large data structures.
/// Each leaf node represents a data chunk, and each internal node contains the hash
/// of its children, culminating in a single root hash that represents the entire dataset.
///
/// # Examples
///
/// ```
/// use blockframe::merkle_tree::MerkleTree;
///
/// let chunks = vec![
///     b"Hello".to_vec(),
///     b"World".to_vec(),
/// ];
/// let tree = MerkleTree::new(chunks);
/// println!("Root hash: {}", tree.get_root());
/// ```
pub struct MerkleTree {
    pub chunks: Vec<Vec<u8>>,
    pub leaves: Vec<Node>,
    pub root: Node,
}

impl MerkleTree {
    /// Creates a new Merkle tree from data chunks.
    ///
    /// Takes ownership of the chunks and builds a complete binary tree
    /// where each leaf represents a data chunk and each internal node
    /// contains the hash of its children. If there's an odd number of chunks,
    /// the last chunk is duplicated to ensure a complete binary tree.
    ///
    /// # Arguments
    ///
    /// * `chunks` - Vector of data chunks as byte vectors
    ///
    /// # Examples
    ///
    /// ```
    /// use blockframe::merkle_tree::MerkleTree;
    ///
    /// let chunks = vec![
    ///     b"chunk1".to_vec(),
    ///     b"chunk2".to_vec(),
    ///     b"chunk3".to_vec(),
    /// ];
    /// let tree = MerkleTree::new(chunks);
    /// ```
    pub fn new(chunks: Vec<Vec<u8>>) -> Self {
        let mut leaves: Vec<Node> = chunks
            .iter()
            .map(|chunk| Node::new(sha256(chunk)))
            .collect();

        if leaves.len() % 2 == 1 {
            if let Some(last_leaf) = leaves.last().cloned() {
                leaves.push(last_leaf);
            }
        }

        let root = Self::build_tree(&leaves);

        MerkleTree {
            chunks,
            leaves,
            root,
        }
    }
    pub fn from_hashes(hashes: Vec<String>) -> Self {
        let leaves: Vec<Node> = hashes.into_iter().map(|hash| Node::new(hash)).collect();
        let root = Self::build_tree(&leaves);
        MerkleTree {
            chunks: vec![],
            leaves,
            root,
        }
    }

    /// Recursively builds the tree from leaf nodes upward.
    ///
    /// Combines adjacent nodes by hashing their values together
    /// until a single root node remains. This is the core algorithm
    /// that constructs the binary tree structure.
    ///
    /// # Arguments
    ///
    /// * `nodes` - Slice of nodes to build the tree from
    ///
    /// # Returns
    ///
    /// The root [`Node`] of the constructed tree
    pub fn build_tree(nodes: &[Node]) -> Node {
        if nodes.len() == 1 {
            return nodes[0].clone();
        }

        let mut new_level = Vec::new();

        for i in (0..nodes.len()).step_by(2) {
            let left: Node = nodes[i].clone();
            let right: Node = if i + 1 < nodes.len() {
                nodes[i + 1].clone()
            } else {
                nodes[i].clone()
            };

            let combined_hashes = format!("{}{}", left.hash_val, right.hash_val)
                .as_bytes()
                .to_vec();
            let combined = sha256(&combined_hashes);
            let parent = Node::with_children(combined, Some(Box::new(left)), Some(Box::new(right)));
            new_level.push(parent);
        }
        return Self::build_tree(&new_level);
    }

    /// Generates inclusion proof for chunk at given index.
    ///
    /// Returns the sibling hashes needed to reconstruct the path
    /// from the specified chunk to the root of the tree. This proof
    /// can be used to verify that a chunk belongs to the tree without
    /// needing the entire tree structure.
    ///
    /// # Arguments
    ///
    /// * `chunk_index` - Index of the chunk to generate proof for
    ///
    /// # Returns
    ///
    /// Vector of sibling hashes forming the inclusion proof
    ///
    /// # Examples
    ///
    /// ```
    /// use blockframe::merkle_tree::MerkleTree;
    ///
    /// let chunks = vec![b"data1".to_vec(), b"data2".to_vec()];
    /// let tree = MerkleTree::new(chunks);
    /// let proof = tree.get_proof(0);
    /// ```
    pub fn get_proof(&self, chunk_index: usize) -> Vec<String> {
        let leaves: Vec<Node> = self
            .chunks
            .iter()
            .map(|chunk| Node::new(sha256(chunk)))
            .collect();

        let mut index = chunk_index;
        let mut proof = Vec::new();
        let mut level = leaves;

        while level.len() > 1 {
            if level.len() % 2 == 1 {
                if let Some(last_level) = level.last().cloned() {
                    level.push(last_level);
                }
            }
            let mut next_level = Vec::new();

            for i in (0..level.len()).step_by(2) {
                let left = level[i].clone();
                let right = level[i + 1].clone();
                let combined_hashes = format!("{}{}", left.hash_val, right.hash_val)
                    .as_bytes()
                    .to_vec();

                let parent_hash = sha256(&combined_hashes);

                let parent = Node::with_children(
                    parent_hash,
                    Some(Box::new(left.clone())),
                    Some(Box::new(right.clone())),
                );

                next_level.push(parent);

                if i == index || i + 1 == index {
                    let slibling = if i == index { right } else { left };
                    proof.push(slibling.hash_val);

                    index = i / 2;
                }
            }
            level = next_level;
        }
        return proof;
    }

    /// Verifies that a chunk belongs to the tree using an inclusion proof.
    ///
    /// Reconstructs the path from chunk to root using the provided proof
    /// and checks if the computed root matches the expected hash.
    ///
    /// # Arguments
    ///
    /// * `chunk` - The data chunk to verify
    /// * `chunk_index` - Index position of the chunk in the original data
    /// * `proof` - Sibling hashes forming the inclusion proof
    /// * `root_hash` - Expected root hash of the tree
    ///
    /// # Returns
    ///
    /// `true` if the chunk is valid and belongs to the tree, `false` otherwise
    ///
    /// # Examples
    ///
    /// ```
    /// use blockframe::merkle_tree::MerkleTree;
    ///
    /// let chunks = vec![b"data1".to_vec(), b"data2".to_vec()];
    /// let tree = MerkleTree::new(chunks);
    /// let proof = tree.get_proof(0);
    /// let is_valid = tree.verify_proof(
    ///     b"data1",
    ///     0,
    ///     &proof,
    ///     tree.get_root().to_string()
    /// );
    /// assert!(is_valid);
    /// ```
    pub fn verify_proof(
        &self,
        chunk: &[u8],
        chunk_index: usize,
        proof: &[String],
        root_hash: String,
    ) -> bool {
        let mut current_hash = sha256(&chunk.to_vec());
        let mut chunk_index = chunk_index;
        for sibling_hash in proof {
            if chunk_index % 2 == 0 {
                let combined_hashes = format!("{}{}", current_hash, sibling_hash)
                    .as_bytes()
                    .to_vec();
                current_hash = sha256(&combined_hashes);
            } else {
                let combined_hashes_else = format!("{}{}", sibling_hash, current_hash)
                    .as_bytes()
                    .to_vec();
                current_hash = sha256(&combined_hashes_else);
            }
            chunk_index = chunk_index / 2;
        }
        return current_hash == root_hash;
    }

    /// Returns the root hash of the tree.
    ///
    /// The root hash uniquely represents the entire dataset and can be used
    /// to verify the integrity of all chunks in the tree.
    ///
    /// # Examples
    ///
    /// ```
    /// use blockframe::merkle_tree::MerkleTree;
    ///
    /// let chunks = vec![b"data".to_vec()];
    /// let tree = MerkleTree::new(chunks);
    /// println!("Root: {}", tree.get_root());
    /// ```
    pub fn get_root(&self) -> &str {
        return &self.root.hash_val;
    }

    /// Returns reference to all leaf nodes.
    ///
    /// Each leaf node represents a data chunk with its computed hash.
    ///
    /// # Examples
    ///
    /// ```
    /// use blockframe::merkle_tree::MerkleTree;
    ///
    /// let chunks = vec![b"data1".to_vec(), b"data2".to_vec()];
    /// let tree = MerkleTree::new(chunks);
    /// let leaves = tree.get_leaves();
    /// println!("Number of leaves: {}", leaves.len());
    /// ```
    pub fn get_leaves(&self) -> &Vec<Node> {
        return &self.leaves;
    }

    // /// Exports tree structure to manifest.json file.
    // ///
    // /// Serializes the complete tree structure including root hash
    // /// and all leaf hashes to a JSON manifest file in the current directory.
    // ///
    // /// # Panics
    // ///
    // /// Panics if the file cannot be created or written to.
    // ///
    // /// # Examples
    // ///
    // /// ```no_run
    // /// use blockframe::merkle_tree::MerkleTree;
    // ///
    // /// let chunks = vec![b"data".to_vec()];
    // /// let tree = MerkleTree::new(chunks);
    // /// tree.write_to_file(); // Creates manifest.json
    // /// ```
    // pub fn write_to_file(&self) {
    //     let file = File::create("manifest.json").expect("Failed to create file");
    //     let mut writer = BufWriter::new(file);
    //     writer.write_all(&self.get_json()).expect("msg");
    //     writer.flush().expect("");
    // }

    /// Serializes tree to JSON bytes for export.
    ///
    /// Returns the tree structure as JSON bytes containing
    /// the root hash and indexed leaf nodes. The format includes
    /// a `merkle_tree` object with `root` and `leaves` fields.
    ///
    /// # Returns
    ///
    /// JSON representation as UTF-8 encoded bytes
    ///
    /// # Examples
    ///
    /// ```
    /// use blockframe::merkle_tree::MerkleTree;
    ///
    /// let chunks = vec![b"data".to_vec()];
    /// let tree = MerkleTree::new(chunks);
    /// let json_bytes = tree.get_json();
    /// let json_str = String::from_utf8(json_bytes).unwrap();
    /// ```
    pub fn get_json(&self) -> Value {
        let mut leaves_object = serde_json::Map::new();
        for (index, hash) in self.leaves.iter().enumerate() {
            leaves_object.insert(index.to_string(), json!(&hash.hash_val));
        }
        let merkle_tree_object = json!({
                "root": self.get_root(),
                "leaves": leaves_object
        });

        return merkle_tree_object;
    }
}
