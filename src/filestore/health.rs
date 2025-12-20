// use reed_solomon_simd::ReedSolomonEncoder;
use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{
    filestore::models::{BatchHealthReport, File, HealthReport, HealthStatus},
    utils::sha256,
};
use reed_solomon_simd::ReedSolomonDecoder;

use super::FileStore;

impl FileStore {
    /// Performs health checks on all files in the archive directory.
    ///
    /// Scans the entire archive, checks each file's health status, and aggregates
    /// the results into a comprehensive batch report.
    ///
    /// # Returns
    ///
    /// A `BatchHealthReport` containing:
    /// - Total file count
    /// - Counts by status (healthy, degraded, recoverable, unrecoverable)
    /// - Individual health reports for each file
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The archive directory cannot be read
    /// - Any manifest file is corrupted or unreadable
    /// - File health checks fail unexpectedly
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::path::Path;
    /// use blockframe::filestore::FileStore;
    ///
    /// let store = FileStore::new(Path::new("archive_directory")).unwrap();
    /// let batch_report = store.batch_health_check().unwrap();
    /// println!("Healthy: {}/{}", batch_report.healthy, batch_report.total_files);
    /// ```
    pub fn batch_health_check(&self) -> Result<BatchHealthReport, Box<dyn std::error::Error>> {
        let files = self.get_all()?;
        let mut reports = Vec::new();
        let mut healthy = 0;
        let mut degraded = 0;
        let mut recoverable = 0;
        let mut unrecoverable = 0;

        for file in &files {
            let report = self.health_check(file)?;

            match report.status {
                HealthStatus::Healthy => healthy += 1,
                HealthStatus::Degraded => degraded += 1,
                HealthStatus::Recoverable => recoverable += 1,
                HealthStatus::Unrecoverable => unrecoverable += 1,
            }

            reports.push((file.file_name.clone(), report));
        }

        Ok(BatchHealthReport {
            total_files: files.len(),
            healthy,
            degraded,
            recoverable,
            unrecoverable,
            reports,
        })
    }

    /// Checks the health of a single file by verifying data integrity and parity availability.
    ///
    /// Routes to the appropriate tier-specific health check based on the file's tier.
    /// Does not modify any files—purely a read-only diagnostic operation.
    ///
    /// # Arguments
    ///
    /// * `file_obj` - The file to check (contains manifest with tier information)
    ///
    /// # Returns
    ///
    /// A `HealthReport` containing:
    /// - Status (Healthy, Degraded, Recoverable, or Unrecoverable)
    /// - Lists of missing data and parity files
    /// - List of corrupt segments
    /// - Whether the file is recoverable
    /// - Human-readable details
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file has an unknown tier
    /// - Required directories don't exist
    /// - File I/O fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use std::path::Path;
    /// # use blockframe::filestore::FileStore;
    /// let store = FileStore::new(Path::new("archive_directory")).unwrap();
    /// let file = store.find(&"example.txt".to_string()).unwrap();
    /// let health = store.health_check(&file).unwrap();
    /// println!("Status: {:?}", health.status);
    /// ```
    pub fn health_check(
        &self,
        file_obj: &File,
    ) -> Result<HealthReport, Box<dyn std::error::Error>> {
        match file_obj.manifest.tier {
            1 => self.health_check_tiny(file_obj),
            2 => self.health_check_segment(file_obj),
            3 => self.health_check_block(file_obj),
            _ => Err("unknown file".into()),
        }
    }

    /// Health check for Tier 1 (tiny) files using RS(1,3) encoding.
    ///
    /// Verifies the integrity of `data.dat` by comparing its hash against the manifest.
    /// Checks availability of all 3 parity files.
    ///
    /// # Status Logic
    /// - **Healthy**: data.dat valid + 3 parity files present
    /// - **Degraded**: data.dat valid + some parity missing  
    /// - **Recoverable**: data.dat corrupt/missing + parity available
    /// - **Unrecoverable**: data.dat corrupt/missing + no parity
    fn health_check_tiny(
        &self,
        file_obj: &File,
    ) -> Result<HealthReport, Box<dyn std::error::Error>> {
        let file_dir = Path::new(&file_obj.file_data.path)
            .parent()
            .ok_or("No parent directory found")?;

        let mut missing_data = Vec::new();
        let mut missing_parity = Vec::new();
        let mut corrupt_segments = Vec::new();

        // Check data.dat
        let data_path = file_dir.join("data.dat");
        let data_exists = data_path.exists();
        let mut data_valid = false;

        if data_exists {
            match fs::read(&data_path) {
                Ok(data) => match sha256(&data) {
                    Ok(hash) => {
                        if hash == file_obj.file_data.hash {
                            data_valid = true;
                        } else {
                            corrupt_segments.push("data.dat".to_string());
                        }
                    }
                    Err(_) => corrupt_segments.push("data.dat".to_string()),
                },
                Err(_) => missing_data.push("data.dat".to_string()),
            }
        } else {
            missing_data.push("data.dat".to_string());
        }

        // Check parity files
        let mut parity_count = 0;
        for i in 0..3 {
            let parity_path = file_dir.join(format!("parity_{}.dat", i));
            if parity_path.exists() {
                parity_count += 1;
            } else {
                missing_parity.push(format!("parity_{}.dat", i));
            }
        }

        // Determine status
        let (status, recoverable) = if data_valid && parity_count == 3 {
            (HealthStatus::Healthy, true)
        } else if data_valid && parity_count > 0 {
            (HealthStatus::Degraded, true)
        } else if !data_valid && parity_count > 0 {
            (HealthStatus::Recoverable, true)
        } else {
            (HealthStatus::Unrecoverable, false)
        };

        let details = format!(
            "Data: {}, Parity: {}/3",
            if data_valid {
                "valid"
            } else {
                "corrupt/missing"
            },
            parity_count
        );

        Ok(HealthReport {
            status,
            missing_data,
            missing_parity,
            corrupt_segments,
            recoverable,
            details,
        })
    }

    /// Health check for Tier 2 (segmented) files using per-segment RS(1,3) encoding.
    ///
    /// Scans all segments and their parity files, verifies hashes against merkle tree.
    /// Counts healthy, missing, and corrupt segments to determine overall status.
    ///
    /// # Status Logic
    /// - **Healthy**: All segments intact and hash-verified
    /// - **Recoverable**: Missing/corrupt segments ≤ parity shards available (≤3)
    /// - **Degraded**: Some parity missing but all data segments healthy
    /// - **Unrecoverable**: Too many segments lost to recover
    fn health_check_segment(
        &self,
        file_obj: &File,
    ) -> Result<HealthReport, Box<dyn std::error::Error>> {
        let file_folder_path = Path::new(&file_obj.file_data.path)
            .parent()
            .ok_or("No parent directory found")?;

        let segments_path = file_folder_path.join("segments");
        let parity_path = file_folder_path.join("parity");

        let segments_map = &file_obj.manifest.merkle_tree.segments;
        let num_segments = segments_map.len();
        let parity_shards = file_obj.manifest.erasure_coding.parity_shards.max(0) as usize;

        let mut missing_data = Vec::new();
        let mut missing_parity = Vec::new();
        let mut corrupt_segments = Vec::new();
        let mut total_segments = 0;
        let mut healthy_segments = 0;

        for (idx, segment_info) in segments_map {
            total_segments += 1;
            let current_segment = segments_path.join(format!("segment_{}.dat", idx));

            // Check segment data
            let segment_data = match fs::read(&current_segment) {
                Ok(data) => data,
                Err(_) => {
                    missing_data.push(format!("segment_{}.dat", idx));
                    continue;
                }
            };

            // Verify Data Hash
            if let Ok(actual) = sha256(&segment_data) {
                if actual != segment_info.data {
                    corrupt_segments.push(format!("segment_{}.dat", idx));
                } else {
                    healthy_segments += 1;
                }
            }

            // Check parity files
            for parity_idx in 0..parity_shards {
                let parity_file =
                    parity_path.join(format!("segment_{}_parity_{}.dat", idx, parity_idx));

                match fs::read(&parity_file) {
                    Ok(chunk) => {
                        // Verify Parity Hash
                        if let Some(expected) = segment_info.parity.get(parity_idx) {
                            if let Ok(actual) = sha256(&chunk) {
                                if actual != *expected {
                                    missing_parity.push(format!(
                                        "segment_{}_parity_{}.dat (CORRUPT)",
                                        idx, parity_idx
                                    ));
                                }
                            }
                        }
                    }
                    Err(_) => {
                        missing_parity.push(format!("segment_{}_parity_{}.dat", idx, parity_idx));
                    }
                }
            }
        }

        // Determine status
        let missing_count = missing_data.len();
        let corrupt_count = corrupt_segments.len();
        let (status, recoverable) = if missing_count == 0 && corrupt_count == 0 {
            (HealthStatus::Healthy, true)
        } else if missing_count + corrupt_count <= parity_shards {
            (HealthStatus::Recoverable, true)
        } else if missing_parity.is_empty() {
            (HealthStatus::Degraded, true)
        } else {
            (HealthStatus::Unrecoverable, false)
        };

        let details = format!(
            "{}/{} segments healthy, {} missing, {} corrupt",
            healthy_segments, total_segments, missing_count, corrupt_count
        );

        Ok(HealthReport {
            status,
            missing_data,
            missing_parity,
            corrupt_segments,
            recoverable,
            details,
        })
    }

    /// Health check for Tier 3 (blocked) files using per-block RS(30,3) encoding.
    ///
    /// Scans all blocks, checks segment availability within each block, and verifies
    /// that block-level parity files exist. Each block can tolerate up to 3 missing segments.
    ///
    /// # Status Logic
    /// - **Healthy**: All blocks have all segments + parity
    /// - **Recoverable**: Some blocks have ≤3 missing segments with parity available
    /// - **Degraded**: No missing segments but some parity missing
    /// - **Unrecoverable**: Any block has >3 missing segments
    fn health_check_block(
        &self,
        file_obj: &File,
    ) -> Result<HealthReport, Box<dyn std::error::Error>> {
        let file_folder_path = Path::new(&file_obj.file_data.path)
            .parent()
            .ok_or("No parent directory found")?;

        let blocks_path = file_folder_path.join("blocks");

        let block_dirs: Vec<_> = fs::read_dir(&blocks_path)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();

        let parity_shards = file_obj.manifest.erasure_coding.parity_shards.max(0) as usize;
        let data_shards = file_obj.manifest.erasure_coding.data_shards.max(0) as usize;

        let mut missing_data = Vec::new();
        let mut missing_parity = Vec::new();
        let corrupt_segments = Vec::new();
        let mut total_blocks = 0;
        let mut healthy_blocks = 0;
        let mut recoverable_blocks = 0;
        let mut unrecoverable_blocks = 0;

        for block_entry in block_dirs {
            total_blocks += 1;
            let block_dir = block_entry.path();
            let block_name = block_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");
            let segments_dir = block_dir.join("segments");
            let parity_dir = block_dir.join("parity");

            // Count existing segments
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

            // Check which segments are missing
            let mut missing_in_block = 0;
            for seg_idx in 0..segment_count {
                let seg_path = segments_dir.join(format!("segment_{}.dat", seg_idx));
                if !seg_path.exists() {
                    missing_data.push(format!("{}/segment_{}.dat", block_name, seg_idx));
                    missing_in_block += 1;
                }
            }

            // Check parity files
            let mut parity_count = 0;
            for parity_idx in 0..parity_shards {
                let parity_path = parity_dir.join(format!("block_parity_{}.dat", parity_idx));
                if parity_path.exists() {
                    parity_count += 1;
                } else {
                    missing_parity.push(format!("{}/block_parity_{}.dat", block_name, parity_idx));
                }
            }

            // Classify block health
            if missing_in_block == 0 && parity_count == parity_shards {
                healthy_blocks += 1;
            } else if missing_in_block <= parity_shards && parity_count == parity_shards {
                recoverable_blocks += 1;
            } else {
                unrecoverable_blocks += 1;
            }
        }

        // Determine overall status
        let (status, recoverable) = if healthy_blocks == total_blocks {
            (HealthStatus::Healthy, true)
        } else if unrecoverable_blocks == 0 && recoverable_blocks > 0 {
            (HealthStatus::Recoverable, true)
        } else if unrecoverable_blocks == 0 && !missing_parity.is_empty() {
            (HealthStatus::Degraded, true)
        } else {
            (HealthStatus::Unrecoverable, false)
        };

        let details = format!(
            "{}/{} blocks healthy, {} recoverable, {} unrecoverable",
            healthy_blocks, total_blocks, recoverable_blocks, unrecoverable_blocks
        );

        Ok(HealthReport {
            status,
            missing_data,
            missing_parity,
            corrupt_segments,
            recoverable,
            details,
        })
    }

    /// Automatically repairs a file by recovering corrupted or missing data.
    ///
    /// First performs a health check to determine if repair is possible.
    /// Skips repair if file is already healthy. Routes to tier-specific repair functions.
    /// Uses Reed-Solomon decoders to reconstruct missing data from parity shards.
    ///
    /// # Arguments
    ///
    /// * `file_obj` - The file to repair (contains manifest with tier information)
    ///
    /// # Returns
    ///
    /// `Ok(())` if repair succeeded or file was already healthy.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - File is unrecoverable (too much data lost)
    /// - Required parity files are missing
    /// - File I/O fails during recovery
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use std::path::Path;
    /// # use blockframe::filestore::FileStore;
    /// let store = FileStore::new(Path::new("archive_directory")).unwrap();
    /// let file = store.find(&"corrupted.txt".to_string()).unwrap();
    /// store.repair(&file).expect("Repair failed");
    /// ```
    pub fn repair(&self, file_obj: &File) -> Result<(), Box<dyn std::error::Error>> {
        let health = self.health_check(file_obj)?;

        if !health.recoverable {
            return Err(format!("File is unrecoverable: {}", health.details).into());
        }

        if health.status == HealthStatus::Healthy {
            return Ok(()); // Nothing to repair
        }

        match file_obj.manifest.tier {
            1 => self.repair_tiny(file_obj),
            2 => self.repair_segment(file_obj),
            3 => self.repair_blocked(file_obj),
            _ => Err("unknown tier".into()),
        }
    }

    /// Repairs Tier 1 (tiny) files by reconstructing data.dat from parity files.
    ///
    /// Uses Reed-Solomon decoder with RS(1,3) configuration to recover the original
    /// data file from any available parity shards. Supports recovery even when data.dat
    /// is completely missing.
    ///
    /// # Note
    /// Recovered data may include padding (rounded up to multiple of 64 bytes).
    pub fn repair_tiny(&self, file_obj: &File) -> Result<(), Box<dyn std::error::Error>> {
        let file_dir = Path::new(&file_obj.file_data.path)
            .parent()
            .ok_or("No parent directory found")?;

        let data_path = file_dir.join("data.dat");

        // Check if data exists and is valid
        if data_path.exists() {
            let data = fs::read(&data_path)?;
            if sha256(&data)? == file_obj.file_data.hash {
                return Ok(());
            }
        }

        // Data is missing or corrupt, use Reed-Solomon decoder
        let shard_size = file_obj.manifest.segment_size as usize;
        let mut decoder = ReedSolomonDecoder::new(1, 3, shard_size)?;

        // Add all available parity shards
        for i in 0..3 {
            let parity_path = file_dir.join(format!("parity_{}.dat", i));
            if let Ok(parity) = fs::read(&parity_path) {
                decoder.add_recovery_shard(i, parity)?;
            }
        }

        // Decode to recover original data
        let result = decoder.decode()?;
        let recovered = result
            .restored_original(0)
            .ok_or("Failed to restore original data")?;

        // Write recovered data (may have padding, but that's okay)
        fs::write(&data_path, recovered)?;
        println!("Recovered data.dat using Reed-Solomon decoder");

        Ok(())
    }

    /// Repairs Tier 2 (segmented) files by reconstructing missing or corrupt segments.
    ///
    /// Scans all segments, identifies those that are missing or fail hash verification,
    /// then uses per-segment RS(1,3) decoding to reconstruct them from parity files.
    /// Each segment is independently recoverable.
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
