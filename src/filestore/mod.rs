use std::fs::{self, ReadDir};
use std::path::{Path, PathBuf};

pub struct FileStore {
    pub store_path: PathBuf,
}

impl FileStore {
    /// Create a new FileStore at the given path
    pub fn new(store_path: &Path) -> Result<Self, String> {
        Ok(FileStore {
            store_path: store_path.to_path_buf(),
        })
    }

    pub fn getall(&self) -> Result<ReadDir, std::io::Error> {
        // very simple, walking the archive_directory
        // lets just return the manifest data
    }
}

pub mod models;
