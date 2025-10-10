use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt::format, fs, path::Path};

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
    pub fn from_file(path: &Path) -> Option<ManifestStructure> {

        let content = fs::read_to_string(path).ok()?;
        // this line populates our struct and attached merkle tree to the read data from the manifest.json
        return serde_json::from_str(&content).ok()
    }

    pub fn validate(&self) -> bool {

        // check root hash is 64 hex characters for sha256
        if !Self::is_valid_hash(&self.merkle_tree.root) {
            return false;
        }

        // check we have leaves
        if self.merkle_tree.leaves.is_empty() {
            return false;
        }

        // check each leaf hash
        for (_index, hash) in &self.merkle_tree.leaves {
            if !Self::is_valid_hash(hash) {
                return false;
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
                return false;
            }
        }

        return true;
    }

    fn is_valid_hash(hash: &str) -> bool {
        hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit())
    }

    /// verify manifest against actual chunk data
    /// returns true if everything matches, false if corrupted
    pub fn verify_against_chunks(&self, chunks: &[Vec<u8>]) -> bool {

        // 1. Check we have the right number of chunks
        if chunks.len() != self.merkle_tree.leaves.len() {
            return false;
        }
        // 2. hash each chunk and compare to manifest
        // so we're enumerateing through all fo the chunks fed to the function
        for (i, chunk) in chunks.iter().enumerate() {
            // we're extracting an expected hash from the read manifest.json
            let expected_hash = match self.merkle_tree.leaves.get(&i.to_string()) {
                Some(hash) => hash,
                None => return false,
            };
            // our actual hash is calculated from the fed chunks
            let actual_hash = sha256(chunk);
            // the rest you can figure out
            if &actual_hash != expected_hash {
                return false;
            }
        }
        let tree = MerkleTree::new(chunks.to_vec());
        if tree.get_root() != self.merkle_tree.root {
            return false;
        }
        return true;
    }
}
