use crate::merkle_tree::MerkleTree;

use super::Chunker;

impl Chunker {
    /// Hashes the data and parity shards belonging to a segment to produce the
    /// Merkle root recorded in the manifest.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use blockframe::chunker::Chunker;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let chunker = Chunker::new()?;
    /// let chunks = chunker.get_chunks(b"blockframe segment hashing")?;
    /// let chunk_refs: Vec<&[u8]> = chunks.iter().map(|c| c.as_slice()).collect();
    /// let parity = chunker.generate_parity(&chunk_refs, 6, 3)?;
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
    pub fn hash_single_segment(
        &self,
        segment: &[u8],
        parity: &[Vec<u8>],
    ) -> Result<String, std::io::Error> {
        let combined: Vec<Vec<u8>> = std::iter::once(segment.to_vec())
            .chain(parity.iter().cloned())
            .collect();
        let segment_tree = MerkleTree::new(combined)?;

        Ok(segment_tree.get_root()?.to_string())
    }
}
