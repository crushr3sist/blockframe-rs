use super::Chunker;

use crate::manifest::ManifestStructure;
impl Chunker {
pub fn should_repair(&self) -> bool {
        // go to dir and check to see if there's a manifest.json present
        let manifest_path = self
            .file_dir
            .as_ref()
            .expect("file_dir not set")
            .join("manifest.json");

        // Try to load manifest
        let manifest = match ManifestStructure::from_file(&manifest_path) {
            Some(m) => m,
            None => {
                println!("should_repair: couldn't find the manifest");
                return true;
            }
        };

        // Validate structure
        if !manifest.validate() {
            println!("should_repair: failed to verify the manifest");
            return true; // Bad structure = repair needed
        }

        // Read chunks
        let chunks = match self.read_chunks() {
            Some(chunks) => chunks,
            None => {
                println!("should_repair: couldnt read the chunks");
                return true;
            }
        };

        // Check chunk count
        if chunks.len() != manifest.merkle_tree.leaves.len() {
            println!("should_repair: chunk count is mismatched");
            return true; // Wrong count = repair needed
        }

        // Verify data matches manifest
        if !manifest.verify_against_chunks(&chunks) {
            println!("should_repair: data is mismatching");
            return true; // Data doesn't match = repair needed
        }

        false
    }

    // pub fn repair(&self) -> bool {
    //     // step 1: read all available chunks (data + parity)
    //     let chunks_dir = self.file_dir.join("chunks");
    //     let parity_dir = self.file_dir.join("parity");

    //     let total_shards = self.data_shards + self.parity_shards;
    //     let mut shards: Vec<Option<Vec<u8>>> = vec![None; total_shards];

    //     // Read data chunks (indices 0-5)

    //     for index in 0..self.data_shards {
    //         if let Ok(chunk_filename) = self.chunk_filename_from_index(index) {
    //             let chunk_path = chunks_dir.join(chunk_filename.file_name().unwrap());

    //             if let Ok(chunk_data) = fs::read(&chunk_path) {
    //                 // verify with manifest
    //                 let hash = sha256(&chunk_data);
    //                 let expected_hash = self
    //                     .merkle_tree
    //                     .leaves
    //                     .get(index)
    //                     .map(|node| node.hash_val.clone());

    //                 if Some(hash.clone()) == expected_hash {
    //                     shards[index] = Some(chunk_data);
    //                     println!("Data chunk {} valid", index);
    //                 } else {
    //                     println!("Data chunk {} corrupted", index);
    //                 }
    //             }
    //         }
    //     }

    //     let file_stem = Path::new(&self.file_name)
    //         .file_stem()
    //         .and_then(|s| s.to_str())
    //         .unwrap_or("unknown");

    //     for parity_index in 0..self.parity_shards {
    //         let parity_filename = format!("{}_p{}.dat", file_stem, parity_index);
    //         let parity_path = parity_dir.join(parity_filename);
    //         if let Ok(parity_data) = fs::read(&parity_path) {
    //             let shard_index = self.data_shards + parity_index;
    //             let hash = sha256(&parity_data);
    //             let expected_hash = self
    //                 .merkle_tree
    //                 .leaves
    //                 .get(shard_index)
    //                 .map(|node| node.hash_val.clone());
    //             if Some(hash.clone()) == expected_hash {
    //                 shards[shard_index] = Some(parity_data);
    //                 println!("parity chunk {} valid", parity_index);
    //             } else {
    //                 println!("parity chunk {} corrupted", parity_index);
    //             }
    //         }
    //     }

    //     // step 2: check if we have enough shards to reconstruct
    //     let valid_count = shards.iter().filter(|s| s.is_some()).count();

    //     if valid_count < self.data_shards {
    //         println!(
    //             "Not enough chunks to reconstruct: have {}, need {}",
    //             valid_count, self.data_shards
    //         );
    //         return false;
    //     }
    //     println!(
    //         "Have {} valid shards (need {}), reconstruction begining",
    //         valid_count, self.data_shards
    //     );

    //     // step 3: reconstruct using reed-solomon
    //     let encoder = ReedSolomon::new(self.data_shards, self.parity_shards)
    //         .expect("failed to create RS encoder");

    //     // find max shard size
    //     let max_size = shards
    //         .iter()
    //         .filter_map(|s| s.as_ref())
    //         .map(|chunk| chunk.len())
    //         .max()
    //         .unwrap_or(0);

    //     // Track which shards exist BEFORE we convert to owned
    //     let shard_present: Vec<bool> = shards.iter().map(|s| s.is_some()).collect();

    //     // prepare owned shards - pad all to same size
    //     let mut owned_shards: Vec<Vec<u8>> = shards
    //         .into_iter()
    //         .map(|opt| {
    //             if let Some(data) = opt {
    //                 let mut padded = data;
    //                 padded.resize(max_size, 0);
    //                 padded
    //             } else {
    //                 vec![0u8; max_size] // Placeholder for missing
    //             }
    //         })
    //         .collect();
    //     dbg!(&owned_shards);

    //     // Create tuple pattern: (shard_ref, is_present)
    //     // This is the pattern reed-solomon-erasure expects
    //     let mut shard_tuple: Vec<_> = owned_shards
    //         .iter_mut()
    //         .map(|s| s.as_mut_slice())
    //         .zip(shard_present.iter().cloned())
    //         .collect();

    //     match encoder.reconstruct(&mut shard_tuple) {
    //         Ok(_) => println!("Reconstruction successful"),
    //         Err(e) => {
    //             println!("reconstruction failed: {:?}", e);
    //             return false;
    //         }
    //     }

    //     for (index, reconstructed) in owned_shards.iter().enumerate() {
    //         let expected_hash = self
    //             .merkle_tree
    //             .leaves
    //             .get(index)
    //             .map(|node| node.hash_val.clone());
    //         let actual_hash = sha256(reconstructed);

    //         if Some(actual_hash.clone()) != expected_hash {
    //             println!("Chunk {} hash mismatch after reconstruction", index);
    //             continue;
    //         }

    //         // Rewrite data chunks
    //         if index < self.data_shards {
    //             if let Ok(chunk_filename) = self.chunk_filename_from_index(index) {
    //                 let chunk_path = chunks_dir.join(chunk_filename.file_name().unwrap());

    //                 if let Err(e) = fs::write(&chunk_path, reconstructed) {
    //                     println!("Failed to write chunk {}: {}", index, e);
    //                 } else {
    //                     println!("Repaired data chunk {}", index);
    //                 }
    //             }
    //         }
    //         // Rewrite parity chunks
    //         else {
    //             let parity_index = index - self.data_shards;
    //             let parity_filename = format!("{}_p{}.dat", file_stem, parity_index);
    //             let parity_path = parity_dir.join(parity_filename);

    //             if let Err(e) = fs::write(&parity_path, reconstructed) {
    //                 println!("Failed to write parity {}: {}", parity_index, e);
    //             } else {
    //                 println!("Repaired parity chunk {}", parity_index);
    //             }
    //         }
    //     }

    //     println!("Repair complete!");
    //     true
    // }
}
