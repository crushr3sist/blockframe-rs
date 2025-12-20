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
use crate::merkle_tree::manifest::MerkleTreeStructure;
impl Chunker {
    pub fn check_for_archive_dir(&self) -> Result<(), Box<dyn std::error::Error>> {
        if !Path::new("archive_directory").is_dir() {
            self.create_dir(Path::new("archive_directory"))?;
        }
        Ok(())
    }

    pub fn write_segment(
        &self,
        segment_index: usize,
        segment_dir: &PathBuf,
        segment: &[u8],
    ) -> Result<(), std::io::Error> {
        // buffering this so windows doesn't throw a tantrum mid write
        let segment_file = segment_dir.join(format!("segment_{}.dat", segment_index));
        let file = File::create(&segment_file)?;
        let capacity = segment.len().max(8 * 1024);
        let mut writer = BufWriter::with_capacity(capacity, file);
        writer.write_all(segment)?;
        writer.flush()
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

        // no point serialising this, let rayon fan it out
        parity.par_iter().enumerate().try_for_each(
            |(index, chunk)| -> Result<(), std::io::Error> {
                let parity_filename = format!("segment_{}_parity_{}.dat", segment_idx, index);
                let parity_path = parity_dir.join(parity_filename);
                let file = File::create(&parity_path)?;
                let capacity = chunk.len().max(8 * 1024);
                let mut writer = BufWriter::with_capacity(capacity, file);
                writer.write_all(chunk)?;
                writer.flush()?;
                println!("wrote parity chunk {} ({} bytes)", index, chunk.len());
                Ok(())
            },
        )?;
        Ok(())
    }

    pub fn write_blocked_parities(
        &self,
        parity_dir: &Path,
        parity: &[Vec<u8>],
    ) -> Result<(), std::io::Error> {
        // TIER 2

        // these parity files are independent so just spray them in parallel
        parity.par_iter().enumerate().try_for_each(
            |(index, chunk)| -> Result<(), std::io::Error> {
                let parity_filename = format!("block_parity_{}.dat", index);
                let parity_path = parity_dir.join(parity_filename);
                let file = File::create(&parity_path)?;
                let capacity = chunk.len().max(8 * 1024);
                let mut writer = BufWriter::with_capacity(capacity, file);
                writer.write_all(chunk)?;
                writer.flush()?;
                println!("wrote parity chunk {} ({} bytes)", index, chunk.len());
                Ok(())
            },
        )?;
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

    pub fn create_dir(&self, file_dir: &Path) -> Result<bool, Box<dyn std::error::Error>> {
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
        segment_size: u64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let now: DateTime<Utc> = Utc::now();
        let mk_tree = merkle_tree.get_json()?;
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
            "segment_size":segment_size,
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

    pub fn write_manifest_struct(
        &self,
        merkle_tree_struct: MerkleTreeStructure,
        file_hash: &String,
        file_name: &String,
        file_size: usize,
        data_shards: usize,
        parity_shards: usize,
        file_dir: &Path,
        tier: u8,
        segment_size: u64,
    ) -> Result<(), Box<dyn std::error::Error>> {
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
            "merkle_tree": merkle_tree_struct,
            "tier": tier,
            "segment_size":segment_size,
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
