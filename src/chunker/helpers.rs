use crate::merkle_tree::MerkleTree;

use super::Chunker;

impl Chunker {
    pub fn hash_segment(chunks: &[Vec<u8>], parity: &[Vec<u8>]) -> String {
        let combined: Vec<Vec<u8>> = chunks.iter().chain(parity.iter()).cloned().collect();
        let segment_tree = MerkleTree::new(combined);

        segment_tree.get_root().to_string()
    }
}
