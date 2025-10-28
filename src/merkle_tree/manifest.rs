use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::Path};

use crate::{merkle_tree::MerkleTree, utils::sha256};

#[derive(Debug, Serialize, Deserialize)]
pub struct MerkleTreeManifest {
    pub leaves: HashMap<String, String>,
    pub root: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ManifestStructure {
    pub merkle_tree: MerkleTreeManifest,
}

impl ManifestStructure {
    /// Loads a manifest from disk, deserialising the JSON payload into a
    /// [`ManifestStructure`].
    ///
    /// # Examples
    ///
    /// ```
    /// # use blockframe::merkle_tree::manifest::ManifestStructure;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let sandbox = std::env::temp_dir().join(format!("blockframe_manifest_read_{}", std::process::id()));
    /// if sandbox.exists() {
    ///     std::fs::remove_dir_all(&sandbox)?;
    /// }
    /// std::fs::create_dir_all(&sandbox)?;
    /// let path = sandbox.join("manifest.json");
    /// std::fs::write(&path, r#"{"merkle_tree":{"leaves":{"0":"00"},"root":"00"}}"#)?;
    /// let manifest = ManifestStructure::from_file(&path).expect("manifest should parse");
    /// assert_eq!(manifest.merkle_tree.root, "00");
    /// std::fs::remove_dir_all(&sandbox)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_file(path: &Path) -> Option<ManifestStructure> {
        let content = fs::read_to_string(path).ok()?;
        // this line populates our struct and attached merkle tree to the read data from the manifest.json
        return serde_json::from_str(&content).ok();
    }

    /// Validates the manifest by ensuring that the root hash and all leaf hashes
    /// are 64-character hexadecimal strings and that leaves are indexed without
    /// gaps.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::collections::HashMap;
    /// # use blockframe::merkle_tree::manifest::{ManifestStructure, MerkleTreeManifest};
    /// # fn main() -> Result<(), std::io::Error> {
    /// let mut leaves = HashMap::new();
    /// leaves.insert("0".into(), "a".repeat(64));
    /// let manifest = ManifestStructure {
    ///     merkle_tree: MerkleTreeManifest {
    ///         leaves,
    ///         root: "b".repeat(64),
    ///     },
    /// };
    /// assert!(manifest.validate().unwrap());
    /// # Ok(())
    /// # }
    /// ```
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
        let mut indices: Vec<usize> = self
            .merkle_tree
            .leaves
            .keys()
            .filter_map(|k| k.parse().ok())
            .collect();
        indices.sort();

        for (expected, actual) in indices.iter().enumerate() {
            if expected != *actual {
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
    /// # use blockframe::merkle_tree::manifest::ManifestStructure;
    /// # fn main() -> Result<(), std::io::Error> {
    /// assert!(ManifestStructure::is_valid_hash(&"f".repeat(64)).unwrap());
    /// assert!(!ManifestStructure::is_valid_hash("xyz").unwrap());
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
    /// ```
    /// # use std::collections::HashMap;
    /// # use blockframe::merkle_tree::manifest::{ManifestStructure, MerkleTreeManifest};
    /// # fn main() -> Result<(), std::io::Error> {
    /// let chunks = vec![b"block".to_vec(), b"frame".to_vec()];
    /// let mut leaves = HashMap::new();
    /// for (index, chunk) in chunks.iter().enumerate() {
    ///     leaves.insert(index.to_string(), blockframe::utils::sha256(chunk)?);
    /// }
    /// let tree = blockframe::merkle_tree::MerkleTree::new(chunks.clone())?;
    /// let manifest = ManifestStructure {
    ///     merkle_tree: MerkleTreeManifest {
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
            let expected_hash = match self.merkle_tree.leaves.get(&i.to_string()) {
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
