use super::Chunker;
use chrono::{DateTime, Utc};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::{
    fs::{self},
    path::Path,
};

use serde_json::json;

use crate::merkle_tree::MerkleTree;
impl Chunker {
    /// Reads all stored chunk files from the chunker's working directory while
    /// ignoring the manifest.
    ///
    /// # Examples
    ///
    /// ```
    /// # use blockframe::chunker::Chunker;
    /// # fn main() -> Result<(), std::io::Error> {
    /// let sandbox = std::env::temp_dir().join(format!("blockframe_read_chunks_{}", std::process::id()));
    /// if sandbox.exists() {
    ///     std::fs::remove_dir_all(&sandbox)?;
    /// }
    /// std::fs::create_dir_all(&sandbox)?;
    /// std::fs::write(sandbox.join("chunk_0.dat"), b"data")?;
    /// let mut chunker = Chunker::new().unwrap();
    /// chunker.file_dir = Some(sandbox.clone());
    /// let chunks = chunker.read_chunks()?;
    /// assert_eq!(chunks.len(), 1);
    /// std::fs::remove_dir_all(sandbox)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn read_chunks(&self) -> Result<Vec<Vec<u8>>, std::io::Error> {
        let read_dir = fs::read_dir(&self.file_dir.as_ref().ok_or(std::io::Error::new(
            std::io::ErrorKind::Other,
            "file_dir is None",
        ))?)?;
        let chunks = read_dir
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_file())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map_or(false, |name| name != "manifest.json")
            })
            .filter_map(|path| fs::read(path).ok())
            .collect();
        Ok(chunks)
    }

    /// Ensures the `archive_directory` exists relative to the current working
    /// directory, creating it if necessary.
    ///
    /// # Examples
    ///
    /// ```
    /// # use blockframe::chunker::Chunker;
    /// # fn main() -> Result<(), std::io::Error> {
    /// let sandbox = std::env::temp_dir().join(format!("blockframe_check_archive_{}", std::process::id()));
    /// if sandbox.exists() {
    ///     std::fs::remove_dir_all(&sandbox)?;
    /// }
    /// std::fs::create_dir_all(&sandbox)?;
    /// let original = std::env::current_dir()?;
    /// std::env::set_current_dir(&sandbox)?;
    /// let chunker = Chunker::new().unwrap();
    /// chunker.check_for_archive_dir()?;
    /// assert!(std::path::Path::new("archive_directory").exists());
    /// std::env::set_current_dir(original)?;
    /// std::fs::remove_dir_all(sandbox)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn check_for_archive_dir(&self) -> Result<(), std::io::Error> {
        Ok(if !Path::new("archive_directory").is_dir() {
            self.create_dir(Path::new("archive_directory"))?;
        })
    }

    /// Writes both data and parity shards for a particular segment into the
    /// archive directory structure.
    ///
    /// # Examples
    ///
    /// ```
    /// # use blockframe::chunker::Chunker;
    /// # fn main() -> Result<(), std::io::Error> {
    /// let sandbox = std::env::temp_dir().join(format!("blockframe_write_segment_{}", std::process::id()));
    /// if sandbox.exists() {
    ///     std::fs::remove_dir_all(&sandbox)?;
    /// }
    /// std::fs::create_dir_all(&sandbox)?;
    /// let original = std::env::current_dir()?;
    /// std::env::set_current_dir(&sandbox)?;
    /// let chunker = Chunker::new().unwrap();
    /// let chunks = vec![b"chunk".to_vec(); 6];
    /// let parity = vec![b"parity".to_vec(); 3];
    /// chunker.write_segment_chunks(0, &"file.txt".to_string(), &"hash".to_string(), &chunks, &parity)?;
    /// assert!(std::path::Path::new("archive_directory/file.txt_hash/segments/segment_0/chunks").exists());
    /// std::env::set_current_dir(original)?;
    /// std::fs::remove_dir_all(sandbox)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn write_segment_chunks(
        &self,
        segment_index: usize,
        file_name: &String,
        file_hash: &String,
        chunks: &[Vec<u8>],
        parity: &[Vec<u8>],
    ) -> Result<(), std::io::Error> {
        // so we need to write the segments now.
        // lets get our archive directory
        let archive_dir = &self.get_dir(file_name, file_hash)?.join("segments");
        let segment_dir = archive_dir.join(format!("segment_{}", segment_index));
        self.create_dir(&segment_dir)?;
        // we're already looping through our segments
        // so we need to create a dir with the segment index
        // once we have that, we need to now create a chunks dir and a parity dir
        let chunks_dir = segment_dir.join("chunks");
        let parity_dir = segment_dir.join("parity");
        self.create_dir(&chunks_dir)?;
        self.create_dir(&parity_dir)?;
        // now inside of those dirs, we need to call write chunks and write_parity.
        self.write_chunks(&chunks_dir, chunks)?;
        self.write_parity_chunks(&parity_dir, parity)?;
        Ok(())
    }

    /// Writes the supplied chunk buffers to disk within `chunks_dir`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use blockframe::chunker::Chunker;
    /// # fn main() -> Result<(), std::io::Error> {
    /// let chunker = Chunker::new().unwrap();
    /// let sandbox = std::env::temp_dir().join(format!("blockframe_write_chunks_{}", std::process::id()));
    /// if sandbox.exists() {
    ///     std::fs::remove_dir_all(&sandbox)?;
    /// }
    /// std::fs::create_dir_all(&sandbox)?;
    /// chunker.write_chunks(&sandbox, &[b"chunk".to_vec()])?;
    /// assert!(sandbox.join("chunk_0.dat").exists());
    /// std::fs::remove_dir_all(sandbox)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn write_chunks(
        &self,
        chunks_dir: &Path,
        chunks: &[Vec<u8>],
    ) -> Result<(), std::io::Error> {
        for (index, chunk) in chunks.iter().enumerate() {
            let chunk_filename = format!("chunk_{}.dat", index);
            let chunk_path = chunks_dir.join(chunk_filename);
            let file = File::create(&chunk_path)?;

            let mut writer = BufWriter::new(file);

            writer.write_all(chunk)?;

            println!("Write data chunk {} ({} bytes)", index, chunk.len());
        }
        Ok(())
    }

    /// Writes parity buffers to disk within `parity_dir`.
    ///
    /// The helper is normally called by [`write_segment_chunks`](Self::write_segment_chunks);
    /// the example demonstrates its effect by invoking the public method and
    /// checking that parity files are created.
    ///
    /// # Examples
    ///
    /// ```
    /// # use blockframe::chunker::Chunker;
    /// # fn main() -> Result<(), std::io::Error> {
    /// let sandbox = std::env::temp_dir().join(format!("blockframe_write_parity_{}", std::process::id()));
    /// if sandbox.exists() {
    ///     std::fs::remove_dir_all(&sandbox)?;
    /// }
    /// std::fs::create_dir_all(&sandbox)?;
    /// let original = std::env::current_dir()?;
    /// std::env::set_current_dir(&sandbox)?;
    /// let chunker = Chunker::new().unwrap();
    /// let chunks = vec![b"chunk".to_vec(); 6];
    /// let parity = vec![b"parity".to_vec(); 3];
    /// chunker.write_segment_chunks(0, &"file.txt".to_string(), &"hash".to_string(), &chunks, &parity)?;
    /// assert!(std::path::Path::new("archive_directory/file.txt_hash/segments/segment_0/parity/parity_0.dat").exists());
    /// std::env::set_current_dir(original)?;
    /// std::fs::remove_dir_all(sandbox)?;
    /// # Ok(())
    /// # }
    /// ```
    fn write_parity_chunks(
        &self,
        parity_dir: &Path,
        parity: &[Vec<u8>],
    ) -> Result<(), std::io::Error> {
        for (index, chunk) in parity.iter().enumerate() {
            let parity_filename = format!("parity_{}.dat", index);
            let parity_path = parity_dir.join(parity_filename);

            let file = File::create(&parity_path)?;
            let mut writer = BufWriter::new(file);
            writer.write_all(chunk)?;
            println!("wrote parity chunk {} ({} bytes)", index, chunk.len());
        }
        Ok(())
    }

    /// Calculates the archive directory where a committed file's segments and
    /// manifest will live.
    ///
    /// # Examples
    ///
    /// ```
    /// # use blockframe::chunker::Chunker;
    /// # fn main() -> Result<(), std::io::Error> {
    /// let chunker = Chunker::new().unwrap();
    /// let dir = chunker.get_dir(&"example.txt".to_string(), &"hash".to_string())?;
    /// assert!(dir.ends_with("example.txt_hash"));
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_dir(
        &self,
        file_name: &String,
        file_hash: &String,
    ) -> Result<std::path::PathBuf, std::io::Error> {
        let path = format!("archive_directory/{}_{}", file_name, file_hash);
        let dir = Path::new(&path);
        Ok(dir.to_path_buf())
    }

    /// Creates a directory and returns whether the directory was newly created.
    ///
    /// # Examples
    ///
    /// ```
    /// # use blockframe::chunker::Chunker;
    /// # fn main() -> Result<(), std::io::Error> {
    /// let chunker = Chunker::new().unwrap();
    /// let sandbox = std::env::temp_dir().join(format!("blockframe_create_dir_{}", std::process::id()));
    /// if sandbox.exists() {
    ///     std::fs::remove_dir_all(&sandbox)?;
    /// }
    /// assert!(chunker.create_dir(&sandbox)?);
    /// assert!(!chunker.create_dir(&sandbox)?);
    /// std::fs::remove_dir_all(sandbox)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn create_dir(&self, file_dir: &Path) -> Result<bool, std::io::Error> {
        if !file_dir.is_dir() {
            fs::create_dir_all(file_dir)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Writes the manifest metadata for a committed file, including the Merkle
    /// tree description, to `file_dir`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use blockframe::chunker::Chunker;
    /// # use blockframe::merkle_tree::MerkleTree;
    /// # fn main() -> Result<(), std::io::Error> {
    /// let chunker = Chunker::new().unwrap();
    /// let sandbox = std::env::temp_dir().join(format!("blockframe_write_manifest_{}", std::process::id()));
    /// if sandbox.exists() {
    ///     std::fs::remove_dir_all(&sandbox)?;
    /// }
    /// std::fs::create_dir_all(&sandbox)?;
    /// let tree = MerkleTree::new(vec![b"chunk".to_vec(), b"more".to_vec()])?;
    /// chunker.write_manifest(&tree, &"hash".to_string(), &"file.txt".to_string(), 8, 6, 3, &sandbox)?;
    /// assert!(sandbox.join("manifest.json").exists());
    /// std::fs::remove_dir_all(sandbox)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn write_manifest(
        &self,
        merkle_tree: &MerkleTree,
        file_hash: &String,
        file_name: &String,
        file_size: usize,
        data_shards: usize,
        parity_shards: usize,
        file_dir: &Path,
    ) -> Result<(), std::io::Error> {
        let mk_tree = merkle_tree.get_json()?;
        let now: DateTime<Utc> = Utc::now();
        let manifest = json!({
            "original_hash": file_hash,
            "name": file_name,
            "size": file_size,
            "time_of_creation":  now.to_string(),
            "erasure_coding": {
                "type": "reed-solomon",
                "data_shards": data_shards,
                "parity_shards": parity_shards,
            },
            "merkle_tree": mk_tree
        })
        .to_string()
        .into_bytes();

        let manifest_path = file_dir.join("manifest.json");
        let file = File::create(manifest_path)?;
        let mut writer = BufWriter::new(file);
        writer.write_all(&manifest)?;
        writer.flush()?;
        Ok(())
    }
}
