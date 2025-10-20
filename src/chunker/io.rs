use super::Chunker;
use chrono::{DateTime, Utc};
use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::Path,
};

use serde_json::json;

use crate::merkle_tree::MerkleTree;
impl Chunker {
    pub fn read_chunks(&self) -> Option<Vec<Vec<u8>>> {
        fs::read_dir(&self.file_dir.as_ref().expect("file_dir not set"))
            .ok()
            .map(|read_dir| {
                read_dir
                    .filter_map(|entry| entry.ok())
                    .map(|entry| entry.path())
                    .filter(|path| path.is_file())
                    .filter(|path| {
                        path.file_name()
                            .and_then(|name| name.to_str())
                            .map(|name| name != "manifest.json")
                            .unwrap_or(false)
                    })
                    .filter_map(|path| fs::read(path).ok())
                    .collect()
            })
    }

    pub fn check_for_archive_dir() {
        if !Path::new("archive_directory").is_dir() {
            Self::create_dir(Path::new("archive_directory"));
        }
    }

    pub fn write_segment_chunks(
        segment_index: usize,
        file_name: &String,
        file_hash: &String,
        chunks: &[Vec<u8>],
        parity: &[Vec<u8>],
    ) {
        // so we need to write the segments now.
        // lets get our archive directory
        let archive_dir = Self::get_dir(file_name, file_hash).join("segments");
        let segment_dir = archive_dir.join(format!("segment_{}", segment_index));
        Self::create_dir(&segment_dir);
        // we're already looping through our segments
        // so we need to create a dir with the segment index
        // once we have that, we need to now create a chunks dir and a parity dir
        let chunks_dir = segment_dir.join("chunks");
        let parity_dir = segment_dir.join("parity");
        Self::create_dir(&chunks_dir);
        Self::create_dir(&parity_dir);
        // now inside of those dirs, we need to call write chunks and write_parity.
        Self::write_chunks(&chunks_dir, chunks).expect("msg");
        Self::write_parity_chunks(&parity_dir, parity).expect("msg");
    }

    pub fn write_chunks(chunks_dir: &Path, chunks: &[Vec<u8>]) -> Result<(), String> {
        for (index, chunk) in chunks.iter().enumerate() {
            let chunk_filename = format!("chunk_{}.dat", index);
            let chunk_path = chunks_dir.join(chunk_filename);
            fs::write(&chunk_path, chunk)
                .map_err(|e| format!("failed to write chunk: {}: {}", index, e))?;
            println!("Write data chunk {} ({} bytes)", index, chunk.len());
        }
        Ok(())
    }

    fn write_parity_chunks(parity_dir: &Path, parity: &[Vec<u8>]) -> Result<(), String> {
        for (index, chunk) in parity.iter().enumerate() {
            // parity files: example_p0.dat, example_p1.dat, example_p2.dat
            let parity_filename = format!("parity_{}.dat", index);
            let parity_path = parity_dir.join(parity_filename);
            fs::write(&parity_path, chunk)
                .map_err(|e| format!("failed to write parity chunk {}: {}", index, e))?;
            println!("wrote parity chunk {} ({} bytes)", index, chunk.len());
        }

        Ok(())
    }

    pub fn get_dir(file_name: &String, file_hash: &String) -> std::path::PathBuf {
        let path = format!("archive_directory/{}_{}", file_name, file_hash);
        let dir = Path::new(&path);
        return dir.to_path_buf();
    }

    pub fn create_dir(file_dir: &Path) -> bool {
        if !file_dir.is_dir() {
            fs::create_dir_all(file_dir).unwrap_or_else(|_| {
                panic!("there was an error creating dir: {:?}", &file_dir.to_str())
            });
            return true;
        } else {
            return false;
        }
    }

    pub fn write_manifest(
        merkle_tree: &MerkleTree,
        file_hash: &String,
        file_name: &String,
        file_size: usize,
        data_shards: usize,
        parity_shards: usize,
        file_dir: &Path,
    ) {
        let mk_tree = merkle_tree.get_json();
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
        let file = File::create(manifest_path).expect("Failed to create manifest file");
        let mut writer = BufWriter::new(file);
        writer.write_all(&manifest).expect("msg");
        writer.flush().expect("msg");
    }
}
