use super::Chunker;
use chrono::{DateTime, Utc};
use rayon::prelude::*;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::{
    fs::{self},
    path::Path,
};

use serde_json::json;

use crate::merkle_tree::MerkleTree;
impl Chunker {
    pub fn check_for_archive_dir(&self) -> Result<(), std::io::Error> {
        Ok(if !Path::new("archive_directory").is_dir() {
            self.create_dir(Path::new("archive_directory"))?;
        })
    }

    pub fn write_segment(
        &self,
        segment_index: usize,
        segment_dir: &PathBuf,
        segment: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        // we're just going to go to the segment directory, and write the segment_0.dat etc

        let segment_file = segment_dir.join(format!("segment_{}.dat", segment_index));
        fs::write(segment_file, segment)?;

        Ok(())
    }

    pub fn write_parity_chunks(
        &self,
        parity_dir: &Path,
        parity: &[Vec<u8>],
    ) -> Result<(), std::io::Error> {
        // TIER 1

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

    pub fn write_segment_parities(
        &self,
        segment_idx: usize,
        parity_dir: &Path,
        parity: &[Vec<u8>],
    ) -> Result<(), std::io::Error> {
        // TIER 2

        for (index, chunk) in parity.iter().enumerate() {
            let parity_filename = format!("segment_{}_parity_{}.dat", segment_idx, index);
            let parity_path = parity_dir.join(parity_filename);
            let file = File::create(&parity_path)?;
            let mut writer = BufWriter::new(file);
            writer.write_all(chunk)?;
            println!("wrote parity chunk {} ({} bytes)", index, chunk.len());
        }
        Ok(())
    }

    pub fn write_blocked_parities(
        &self,
        parity_dir: &Path,
        parity: &[Vec<u8>],
    ) -> Result<(), std::io::Error> {
        // TIER 2

        for (index, chunk) in parity.iter().enumerate() {
            let parity_filename = format!("block_parity_{}.dat", index);
            let parity_path = parity_dir.join(parity_filename);
            let file = File::create(&parity_path)?;
            let mut writer = BufWriter::new(file);
            writer.write_all(chunk)?;
            println!("wrote parity chunk {} ({} bytes)", index, chunk.len());
        }
        Ok(())
    }

    pub fn get_dir(
        &self,
        file_name: &String,
        file_hash: &String,
    ) -> Result<std::path::PathBuf, std::io::Error> {
        let path = format!("archive_directory/{}_{}", file_name, file_hash);
        let dir = Path::new(&path);
        Ok(dir.to_path_buf())
    }

    pub fn create_dir(&self, file_dir: &Path) -> Result<bool, std::io::Error> {
        if !file_dir.is_dir() {
            fs::create_dir_all(file_dir)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn write_manifest(
        &self,
        merkle_tree: &MerkleTree,
        file_hash: &String,
        file_name: &String,
        file_size: usize,
        data_shards: usize,
        parity_shards: usize,
        file_dir: &Path,
        tier: u8,
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
            "merkle_tree": mk_tree,
            "tier": tier,
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
