//!
//! This module provides functionality to split files into chunks, generate parity data
//! for error correction, and reconstruct missing chunks using Reed-Solomon erasure coding.
//! The implementation ensures data integrity through Merkle tree verification and supports
//! self-healing repair without requiring the original file.
//! File chunking and Reed-Solomon erasure coding for self-healing archival storage.

use std::path::PathBuf;

use crate::merkle_tree::MerkleTree;

pub struct Chunker {
    pub file_name: Option<String>,
    pub file_size: Option<usize>,
    pub file_dir: Option<PathBuf>,
    pub file_trun_hash: Option<String>,
    pub file_hash: Option<String>,
    pub merkle_tree: Option<MerkleTree>,
    pub committed: Option<bool>,
    pub segment_size: Option<usize>,
    pub num_segments: Option<usize>,
    pub data_shards: usize,
    pub parity_shards: usize,
}
pub struct ChunkedFile {
    pub file_name: String,
    pub file_size: usize,
    pub file_dir: PathBuf,
    pub file_trun_hash: String,
    pub file_hash: String,
    pub merkle_tree: MerkleTree,
    pub segment_size: usize,
    pub num_segments: usize,
    pub data_shards: usize,
    pub parity_shards: usize,
}

impl Chunker {
    /// Creates a new [`Chunker`] instance with default shard counts suitable for
    /// Reed-Solomon encoding.
    ///
    /// # Examples
    ///
    /// ```
    /// # use blockframe::chunker::Chunker;
    /// let chunker = Chunker::new().unwrap();
    /// assert_eq!(chunker.data_shards, 6);
    /// assert_eq!(chunker.parity_shards, 3);
    /// ```
    pub fn new() -> Result<Self, String> {
        const DATA_SHARDS: usize = 6;
        const PARITY_SHARDS: usize = 3;
        Ok(Chunker {
            file_name: None,
            file_size: None,
            segment_size: None,
            num_segments: None,
            file_dir: None,
            file_trun_hash: None,
            file_hash: None,
            merkle_tree: None,
            committed: Some(false),
            data_shards: DATA_SHARDS,
            parity_shards: PARITY_SHARDS,
        })
    }
}

mod commit;
mod generate;
mod helpers;
mod io;
