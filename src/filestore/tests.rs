//! Unit tests for filestore module
//!
//! Tests cover:
//! - Finding files by name
//! - Listing all files
//! - File reconstruction

#[cfg(test)]
mod tests {
    use super::super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    /// Helper: Create a minimal test archive structure
    fn setup_test_archive(temp_dir: &Path) -> PathBuf {
        let archive_dir = temp_dir.join("archive_directory");
        fs::create_dir_all(&archive_dir).unwrap();

        // Create a test file archive with manifest
        let test_file_dir = archive_dir.join("test.txt_abc123");
        fs::create_dir_all(&test_file_dir).unwrap();

        // Write minimal but complete manifest.json
        let manifest_content = r#"{
            "name": "test.txt",
            "original_hash": "abc123",
            "size": 1000,
            "tier": 1,
            "segment_size": 0,
            "time_of_creation": "2024-01-01T00:00:00Z",
            "erasure_coding": {
                "type": "reed_solomon",
                "data_shards": 1,
                "parity_shards": 3
            },
            "merkle_tree": {
                "root": "0000000000000000000000000000000000000000000000000000000000000000",
                "leaves": {}
            }
        }"#;
        fs::write(test_file_dir.join("manifest.json"), manifest_content).unwrap();

        // Write data.dat
        fs::write(test_file_dir.join("data.dat"), vec![0u8; 1000]).unwrap();

        archive_dir
    }

    #[test]
    fn test_filestore_new() {
        let temp_dir = TempDir::new().unwrap();
        let archive_path = temp_dir.path().join("archive");

        let store = FileStore::new(&archive_path);
        assert!(store.is_ok());
    }

    #[test]
    fn test_get_all_returns_files() {
        let temp_dir = TempDir::new().unwrap();
        let archive_dir = setup_test_archive(temp_dir.path());

        let store = FileStore::new(&archive_dir).unwrap();
        let files = store.get_all();

        match files {
            Ok(file_list) => {
                assert_eq!(file_list.len(), 1);
                assert_eq!(file_list[0].file_name, "test.txt");
            }
            Err(e) => {
                panic!("get_all() failed: {}", e);
            }
        }
    }

    #[test]
    fn test_find_existing_file() {
        let temp_dir = TempDir::new().unwrap();
        let archive_dir = setup_test_archive(temp_dir.path());

        let store = FileStore::new(&archive_dir).unwrap();
        let result = store.find(&"test.txt".to_string());

        assert!(result.is_ok());
        let file = result.unwrap();
        assert_eq!(file.file_name, "test.txt");
    }

    #[test]
    fn test_find_nonexistent_file_fails() {
        let temp_dir = TempDir::new().unwrap();
        let archive_dir = setup_test_archive(temp_dir.path());

        let store = FileStore::new(&archive_dir).unwrap();
        let result = store.find(&"does_not_exist.txt".to_string());

        assert!(result.is_err());
    }

    #[test]
    fn test_get_all_empty_archive() {
        let temp_dir = TempDir::new().unwrap();
        let archive_dir = temp_dir.path().join("empty_archive");
        fs::create_dir_all(&archive_dir).unwrap();

        let store = FileStore::new(&archive_dir).unwrap();
        let files = store.get_all();

        assert!(files.is_ok());
        assert_eq!(files.unwrap().len(), 0);
    }

    #[test]
    fn test_all_files_returns_manifest_paths() {
        let temp_dir = TempDir::new().unwrap();
        let archive_dir = setup_test_archive(temp_dir.path());

        let store = FileStore::new(&archive_dir).unwrap();
        let manifests = store.all_files();

        assert_eq!(manifests.iter().clone().len(), 1);
        assert!(&manifests.unwrap()[0].ends_with("manifest.json"));
    }
}
