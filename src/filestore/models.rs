use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};
/// Manifest File Structures

#[derive(Debug, Serialize, Deserialize)]
pub struct ErasureCoding {
    pub data_shards: i8,
    pub parity_shards: i8,
    pub r#type: String,
}
#[derive(Debug, Serialize, Deserialize)]

pub struct MerkleTreeStructure {
    pub leaves: HashMap<i32, String>,
    pub root: String,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct ManifestFile {
    pub erasure_coding: ErasureCoding,
    pub merkle_tree: MerkleTreeStructure,
    pub name: String,
    pub original_hash: String,
    pub size: i32,
    pub time_of_creation: String,
}

/// Segment Directory Structures

#[derive(Debug, Serialize, Deserialize)]
pub struct ChunksAndParity {
    pub chunks: Vec<PathBuf>,
    pub parity: Vec<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IDXDSegments {
    pub idxd_segments: Vec<ChunksAndParity>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Segments {
    pub segments: Vec<IDXDSegments>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CommitedFile {
    pub manifest: ManifestFile,
    pub segments: Segments,
}
