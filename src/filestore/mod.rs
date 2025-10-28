use std::collections::HashMap;
use std::fs::{self};
use std::path::{Path, PathBuf};

pub struct FileStore {
    pub store_path: PathBuf,
}

impl FileStore {
    /// Create a new FileStore at the given path
    pub fn new(store_path: &Path) -> Result<Self, std::io::Error> {
        Ok(FileStore {
            store_path: store_path.to_path_buf(),
        })
    }

    pub fn as_hashmap(
        &self,
    ) -> Result<Vec<HashMap<String, HashMap<String, String>>>, std::io::Error> {
        // very simple, walking the archive_directory
        // lets just return the manifest data
        // now lets turn the manifests into hash maps
        // file name: {hash: hash, path: path}
        let mut file_hashmap: Vec<HashMap<String, HashMap<String, String>>> = Vec::new();
        let all_dirs = fs::read_dir(&self.store_path);
        let manifests: Vec<PathBuf> = all_dirs
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok())
            .map(|f| f.path().join("manifest.json"))
            .collect();

        // now per path we're going to construct out hashmap structure

        for path in manifests.iter() {
            if let Some(file) = path.parent() {
                let mut hash_map_entry: HashMap<String, HashMap<String, String>> = HashMap::new();
                let components: Vec<_> = file.components().map(|f| f.as_os_str()).collect();
                if let Some(file_name) = components[1].to_str() {
                    let file_name_full: Vec<&str> = file_name.split("_").collect();
                    let (filename, file_hash) = if file_name_full.len() > 1 {
                        let name_parts = &file_name_full[..&file_name_full.len() - 1];
                        let hash = file_name_full.last().ok_or_else(|| {
                            std::io::Error::new(
                                std::io::ErrorKind::Other,
                                "couldnt extract the hash from file name",
                            )
                        })?;
                        (name_parts.join("_"), hash)
                    } else {
                        continue;
                    };
                    let mut hash_data: HashMap<String, String> = HashMap::new();
                    hash_data.insert("hash".to_string(), file_hash.to_string());
                    hash_data.insert("path".to_string(), path.display().to_string());
                    hash_map_entry.insert(filename, hash_data);
                    file_hashmap.push(hash_map_entry);
                }
            }
        }

        return Ok(file_hashmap);
    }

    pub fn get_all(&self) {
        // this is where we fill in our structs and return a vector of our models
    }
}

pub mod models;
