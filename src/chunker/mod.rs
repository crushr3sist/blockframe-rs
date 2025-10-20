//!
//! This module provides functionality to split files into chunks, generate parity data
//! for error correction, and reconstruct missing chunks using Reed-Solomon erasure coding.
//! The implementation ensures data integrity through Merkle tree verification and supports
//! self-healing repair without requiring the original file.
//! File chunking and Reed-Solomon erasure coding for self-healing archival storage.

use std::path::PathBuf;

use crate::merkle_tree::MerkleTree;

CONST 
CONST 

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
    pub data_shards: Option<usize>,
    pub parity_shards: Option<usize>,
}

impl Chunker {
    pub fn file_name(&self) -> {}
    pub fn file_size(&self) -> {}
    pub fn segment_size(&self) -> {}
    pub fn num_segments(&self) -> {}
    pub fn file_dir(&self) -> {}
    pub fn file_trun_hash(&self) -> {}
    pub fn file_hash(&self) -> {}
    pub fn merkle_tree(&self) -> {}
    pub fn new() -> Result<Self, String> {
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
            data_shards: Some(6),
            parity_shards: Some(3),
        })
    }

    
}

mod commit;
mod generate;
mod health;
mod helpers;
mod io;
