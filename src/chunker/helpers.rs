use crate::merkle_tree::MerkleTree;

use super::Chunker;

impl Chunker {
    /// Hashes the data and parity shards belonging to a segment to produce the
    /// Merkle root recorded in the manifest.
    ///
    /// # Examples
    ///
    /// ```
    /// # use blockframe::chunker::Chunker;
    /// # fn main() -> Result<(), std::io::Error> {
    /// let chunker = Chunker::new().unwrap();
    /// let chunks = chunker.get_chunks(b"blockframe segment hashing")?;
    /// let parity = chunker.generate_parity(&chunks, 6, 3)?;
    /// let segment_hash = chunker.hash_segment(&chunks, &parity)?;
    /// assert_eq!(segment_hash.len(), 64);
    /// # Ok(())
    /// # }
    /// ```
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
