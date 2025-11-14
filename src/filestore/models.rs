use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::PathBuf};
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

impl ManifestFile {
    pub fn new(file_path: String) -> Result<Self, Box<dyn std::error::Error>> {
        let file_json_string = fs::read_to_string(file_path)?;
        let manifest_file: ManifestFile = serde_json::from_str(&file_json_string)?;

        Ok(manifest_file)
    }
}

pub struct FileData {
    pub hash: String,
    pub path: String,
}

pub struct File {
    pub file_name: String,
    pub file_data: FileData,
    pub manifest: ManifestFile,
}

impl FileData {
    pub fn new(hash: String, path: String) -> Self {
        FileData { hash, path }
    }
}

impl File {
    pub fn new(
        file_name: String,
        hash: String,
        path: String,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let file_data = FileData::new(hash, path.clone());
        let manifest = ManifestFile::new(path.clone())?;
        Ok(File {
            file_name,
            file_data,
            manifest,
        })
    }
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
