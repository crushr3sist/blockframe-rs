use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::merkle_tree::manifest::ManifestFile;
/// Manifest File Structures

#[derive(Debug, Clone)]

pub struct FileData {
    pub hash: String,
    pub path: String,
}

#[derive(Debug, Clone)]
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

#[derive(Debug, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Recoverable,
    Unrecoverable,
}

#[derive(Debug)]
pub struct HealthReport {
    pub status: HealthStatus,
    pub missing_data: Vec<String>,
    pub missing_parity: Vec<String>,
    pub corrupt_segments: Vec<String>,
    pub recoverable: bool,
    pub details: String,
}

#[derive(Debug)]
pub struct BatchHealthReport {
    pub total_files: usize,
    pub healthy: usize,
    pub degraded: usize,
    pub recoverable: usize,
    pub unrecoverable: usize,
    pub reports: Vec<(String, HealthReport)>,
}
