use crate::chunker::ChunkedFile;
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
}
