use crate::merkle_tree::MerkleTree;

use super::Chunker;

impl Chunker {
    pub fn hash_segment(
        &self,
        chunks: &[Vec<u8>],
        parity: &[Vec<u8>],
    ) -> Result<String, std::io::Error> {
        let combined: Vec<Vec<u8>> = chunks.iter().chain(parity.iter()).cloned().collect();
        let segment_tree = MerkleTree::new(combined)?;

        Ok(segment_tree.get_root()?.to_string())
    }
}
