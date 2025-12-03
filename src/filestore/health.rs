// use reed_solomon_simd::ReedSolomonEncoder;
use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{filestore::models::File, merkle_tree::MerkleTree, utils::sha256};
use reed_solomon_simd::ReedSolomonDecoder;

use super::FileStore;

impl FileStore {
    pub fn repair(&self, file_obj: &File) -> Result<(), Box<dyn std::error::Error>> {
        match file_obj.manifest.tier {
            1 => self.repair_tiny(file_obj),
            2 => self.repair_segment(file_obj),
            3 => self.repair_blocked(file_obj),
            _ => Err("unknown tier".into()),
        }
    }
    pub fn repair_tiny(&self, file_obj: &File) -> Result<(), Box<dyn std::error::Error>> {
        let file_dir = Path::new(&file_obj.file_data.path)
            .parent()
            .ok_or("No parent directory found")?;
        println!("{}", &file_obj.file_data.path);

        let data = fs::read(file_dir.join("data.dat"))?;

        if sha256(&data)? == file_obj.file_data.hash {
            return Ok(());
        }

        for i in 0..3 {
            let parity = fs::read(file_dir.join(format!("parity_{}.dat", i)))?;
            fs::write(file_dir.join("data.dat"), &parity)?;
            return Ok(());
        }

        Err("no valid parity found".into())
    }

    pub fn repair_segment(&self, file_obj: &File) -> Result<(), Box<dyn std::error::Error>> {
        let mut corrupt_segments: Vec<(usize, PathBuf)> = Vec::new();

        let file_folder_path = Path::new(&file_obj.file_data.path)
            .parent()
            .ok_or("No parent directory found")?;

        let segments_path = file_folder_path.join("segments");
        let parity_path = file_folder_path.join("parity");

        let leafs = &file_obj.manifest.merkle_tree.leaves;
        let parity_shards = file_obj.manifest.erasure_coding.parity_shards.max(0) as usize;

        for idx in 0..leafs.len() {
            let current_segment = segments_path.join(format!("segment_{}.dat", idx));
            let segment_data = match fs::read(&current_segment) {
                Ok(data) => data,
                Err(_) => {
                    corrupt_segments.push((idx, current_segment));
                    continue;
                }
            };

            let mut parity_chunks = Vec::with_capacity(parity_shards);
            let mut parity_missing = false;
            for parity_idx in 0..parity_shards {
                let parity_file =
                    parity_path.join(format!("segment_{}_parity_{}.dat", idx, parity_idx));
                match fs::read(&parity_file) {
                    Ok(chunk) => parity_chunks.push(chunk),
                    Err(_) => {
                        parity_missing = true;
                        break;
                    }
                }
            }

            if parity_missing {
                corrupt_segments.push((idx, current_segment));
                continue;
            }

            let computed_hash = self.hash_segment_with_parity(&segment_data, &parity_chunks)?;
            let leaf_hash = leafs
                .get(&(idx as i32))
                .ok_or("manifest leaf missing for segment index")?;

            if computed_hash != *leaf_hash {
                corrupt_segments.push((idx, current_segment));
            }
        }

        if corrupt_segments.is_empty() {
            return Ok(());
        }

        for (segment_idx, corrupt_path) in corrupt_segments {
            let parity_chunks: Vec<Vec<u8>> = (0..parity_shards)
                .map(|parity_idx| {
                    fs::read(
                        parity_path
                            .join(format!("segment_{}_parity_{}.dat", segment_idx, parity_idx)),
                    )
                })
                .collect::<Result<_, _>>()?;

            let shard_len = parity_chunks
                .first()
                .map(|chunk| chunk.len())
                .unwrap_or(file_obj.manifest.segment_size as usize);

            let mut recovery_decoder = ReedSolomonDecoder::new(1, parity_shards, shard_len)?;

            for (parity_idx, chunk) in parity_chunks.into_iter().enumerate() {
                recovery_decoder.add_recovery_shard(parity_idx, chunk)?;
            }

            let recovered_segment = recovery_decoder
                .decode()?
                .restored_original(0)
                .ok_or("unable to restore original segment")?
                .to_vec();

            fs::write(corrupt_path, &recovered_segment)?;
        }

        Ok(())
    }

    /// Repairs corrupt or missing segments in Tier 3 (blocked) archives.
    ///
    /// Tier 3 uses RS(30,3): 30 data segments + 3 parity shards per block.
    /// Each block lives in `blocks/block_N/` with:
    ///   - `segments/segment_X.dat` (X = 0..segment_count, max 30)
    ///   - `parity/block_parity_Y.dat` (Y = 0..2)
    ///
    /// Recovery strategy:
    /// 1. For each block, identify missing or corrupt segments
    /// 2. If <= 3 segments are missing, use RS decoder to reconstruct
    /// 3. Write recovered segments back to disk
    pub fn repair_blocked(&self, file_obj: &File) -> Result<(), Box<dyn std::error::Error>> {
        let file_folder_path = Path::new(&file_obj.file_data.path)
            .parent()
            .ok_or("No parent directory found")?;

        let blocks_path = file_folder_path.join("blocks");

        // Determine how many blocks exist
        let block_dirs: Vec<_> = fs::read_dir(&blocks_path)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();

        let segment_size = file_obj.manifest.segment_size as usize;
        let parity_shards = file_obj.manifest.erasure_coding.parity_shards.max(0) as usize;
        let data_shards = file_obj.manifest.erasure_coding.data_shards.max(0) as usize;

        for block_entry in block_dirs {
            let block_dir = block_entry.path();
            let segments_dir = block_dir.join("segments");
            let parity_dir = block_dir.join("parity");

            // Count how many segment files actually exist in this block
            let existing_segments: Vec<_> = fs::read_dir(&segments_dir)?
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path()
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(|s| s.starts_with("segment_") && s.ends_with(".dat"))
                        .unwrap_or(false)
                })
                .collect();

            let segment_count = existing_segments.len().min(data_shards);

            // Identify missing or corrupt segments
            let mut missing_indices: Vec<usize> = Vec::new();
            let mut valid_segments: Vec<(usize, Vec<u8>)> = Vec::new();

            for seg_idx in 0..segment_count {
                let seg_path = segments_dir.join(format!("segment_{}.dat", seg_idx));
                match fs::read(&seg_path) {
                    Ok(data) => {
                        // TODO: optionally verify hash against stored merkle leaf
                        valid_segments.push((seg_idx, data));
                    }
                    Err(_) => {
                        missing_indices.push(seg_idx);
                    }
                }
            }

            // Also check for segments that exist but might be corrupt
            // For now we trust that if the file exists it's valid

            if missing_indices.is_empty() {
                // Block is healthy
                continue;
            }

            if missing_indices.len() > parity_shards {
                return Err(format!(
                    "Block {:?} has {} missing segments but only {} parity shards - unrecoverable",
                    block_dir,
                    missing_indices.len(),
                    parity_shards
                )
                .into());
            }

            // Read parity shards
            let mut parity_data: Vec<Vec<u8>> = Vec::with_capacity(parity_shards);
            for parity_idx in 0..parity_shards {
                let parity_path = parity_dir.join(format!("block_parity_{}.dat", parity_idx));
                let data = fs::read(&parity_path).map_err(|e| {
                    format!(
                        "Failed to read parity {} in {:?}: {}",
                        parity_idx, block_dir, e
                    )
                })?;
                parity_data.push(data);
            }

            // Determine shard size (all shards in a block are same size)
            let shard_size = parity_data.first().map(|p| p.len()).unwrap_or(segment_size);

            // Create decoder
            let mut decoder = ReedSolomonDecoder::new(segment_count, parity_shards, shard_size)?;

            // Add all valid original shards
            for (idx, data) in &valid_segments {
                decoder.add_original_shard(*idx, data)?;
            }

            // Add all parity shards
            for (parity_idx, data) in parity_data.iter().enumerate() {
                decoder.add_recovery_shard(parity_idx, data)?;
            }

            // Decode and recover
            let result = decoder.decode()?;

            // Write recovered segments back to disk
            for missing_idx in missing_indices {
                let recovered = result
                    .restored_original(missing_idx)
                    .ok_or_else(|| format!("Failed to restore segment {}", missing_idx))?;

                let seg_path = segments_dir.join(format!("segment_{}.dat", missing_idx));
                fs::write(&seg_path, recovered)?;
                println!(
                    "Recovered segment {} in block {:?}",
                    missing_idx,
                    block_dir.file_name().unwrap_or_default()
                );
            }
        }

        Ok(())
    }
}
