use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs};

use crate::{merkle_tree::MerkleTree, utils::sha256};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BlockInfo {
    pub block_id: usize,
    pub block_root: String,
    pub segment_hashes: Vec<String>,
    pub parity_hashes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ErasureCoding {
    pub data_shards: i8,
    pub parity_shards: i8,
    pub r#type: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MerkleTreeStructure {
    pub leaves: HashMap<i32, String>,
    pub root: String,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ManifestFile {
    pub erasure_coding: ErasureCoding,
    pub merkle_tree: MerkleTreeStructure,
    pub name: String,
    pub original_hash: String,
    pub size: i64,
    pub time_of_creation: String,
    pub tier: u8,
    pub segment_size: u64,
}

impl ManifestFile {
    pub fn new(file_path: String) -> Result<Self, Box<dyn std::error::Error>> {
        let file_json_string = fs::read_to_string(file_path)?;
        let manifest_file: ManifestFile = serde_json::from_str(&file_json_string)?;

        Ok(manifest_file)
    }

    pub fn validate(&self) -> Result<bool, std::io::Error> {
        // check root hash is 64 hex characters for sha256
        if !Self::is_valid_hash(&self.merkle_tree.root)? {
            return Ok(false);
        }

        // check we have leaves
        if self.merkle_tree.leaves.is_empty() {
            return Ok(false);
        }

        // check each leaf hash
        for (_index, hash) in &self.merkle_tree.leaves {
            if !Self::is_valid_hash(hash)? {
                return Ok(false);
            }
        }

        // check if the indices are 0, 1, 2, 3... (no gaps)
        let mut indices: Vec<&i32> = self.merkle_tree.leaves.keys().collect();
        indices.sort();

        for (expected, actual) in indices.iter().enumerate() {
            if expected != **actual as usize {
                return Ok(false);
            }
        }

        return Ok(true);
    }

    /// Checks whether the supplied string is a 64-character hexadecimal hash.
    ///
    /// # Examples
    ///
    /// ```
    /// # use blockframe::merkle_tree::manifest::ManifestFile;
    /// # fn main() -> Result<(), std::io::Error> {
    /// assert!(ManifestFile::is_valid_hash(&"f".repeat(64)).unwrap());
    /// assert!(!ManifestFile::is_valid_hash("xyz").unwrap());
    /// # Ok(())
    /// # }
    /// ```
    pub fn is_valid_hash(hash: &str) -> Result<bool, std::io::Error> {
        Ok(hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()))
    }

    /// Verifies that the hashes in the manifest match a collection of chunk
    /// bytes and that the reconstructed Merkle tree root matches the stored
    /// root.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::collections::HashMap;
    /// # use blockframe::merkle_tree::manifest::{ManifestFile, MerkleTreeStructure, ErasureCoding};
    /// # fn main() -> Result<(), std::io::Error> {
    /// let chunks = vec![b"block".to_vec(), b"frame".to_vec()];
    /// let mut leaves = HashMap::new();
    /// for (index, chunk) in chunks.iter().enumerate() {
    ///     leaves.insert(index as i32, blockframe::utils::sha256(chunk)?);
    /// }
    /// let tree = blockframe::merkle_tree::MerkleTree::new(chunks.clone())?;
    /// let manifest = ManifestFile {
    ///     name: "test".to_string(),
    ///     original_hash: "hash".to_string(),
    ///     size: 10,
    ///     tier: 1,
    ///     segment_size: 0,
    ///     time_of_creation: "2024-01-01T00:00:00Z".to_string(),
    ///     erasure_coding: ErasureCoding { r#type: "reed_solomon".to_string(), data_shards: 1, parity_shards: 3 },
    ///     merkle_tree: MerkleTreeStructure {
    ///         leaves,
    ///         root: tree.get_root()?.to_string(),
    ///     },
    /// };
    /// assert!(manifest.verify_against_chunks(&chunks)?);
    /// # Ok(())
    /// # }
    /// ```
    pub fn verify_against_chunks(&self, chunks: &[Vec<u8>]) -> Result<bool, std::io::Error> {
        // 1. Check we have the right number of chunks
        if chunks.len() != self.merkle_tree.leaves.len() {
            return Ok(false);
        }
        // 2. hash each chunk and compare to manifest
        // so we're enumerateing through all fo the chunks fed to the function
        for (i, chunk) in chunks.iter().enumerate() {
            // we're extracting an expected hash from the read manifest.json
            let expected_hash = match self.merkle_tree.leaves.get(&(i as i32)) {
                Some(hash) => hash,
                None => return Ok(false),
            };
            // our actual hash is calculated from the fed chunks
            let actual_hash = sha256(chunk)?;
            // the rest you can figure out
            if &actual_hash != expected_hash {
                return Ok(false);
            }
        }
        let tree = MerkleTree::new(chunks.to_vec())?;
        if tree.get_root()? != self.merkle_tree.root {
            return Ok(false);
        }
        Ok(true)
    }
}
