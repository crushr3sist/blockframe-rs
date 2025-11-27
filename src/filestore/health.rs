use reed_solomon_erasure::galois_8::ReedSolomon;
use std::{fs, path::Path};

use crate::{filestore::models::File, utils::sha256};

use super::FileStore;

impl FileStore {
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

    // pub fn should_repair(&self, file_obj: &File) -> Result<bool, Box<dyn std::error::Error>> {
    //     if !file_obj.manifest.validate()? {
    //         println!("should_repair: failed to verify the manifest");
    //         return Ok(true);
    //     }

    //     let segments_paths = self.get_segments_paths(file_obj)?;

    //     if segments_paths.len() != file_obj.manifest.merkle_tree.leaves.len() {
    //         println!("should_repair: segment count is mismatched");
    //         return Ok(true);
    //     }

    //     for (idx, segment_path) in segments_paths.iter().enumerate() {
    //         match self.read_segment(segment_path.clone()) {
    //             Ok(segment_data) => {
    //                 let computed_hash = self.segment_hash(segment_data)?;
    //                 let expected_hash = &file_obj.manifest.merkle_tree.leaves[idx].hash_val;

    //                 if &computed_hash != expected_hash {
    //                     println!("should_repair: segment {} corrupted", idx);
    //                     return Ok(true);
    //                 }
    //             }
    //             Err(_) => {
    //                 println!("should_repair: couldn't read segment {}", idx);
    //                 return Ok(true);
    //             }
    //         }
    //     }

    //     Ok(false)
    // }

    // pub fn repair(
    //     &self,
    //     file_obj: &File,
    //     chunker_instance: &Chunker,
    // ) -> Result<bool, Box<dyn std::error::Error>> {
    //     // step 1: read all available chunks (data + parity)
    //     let file_size = &self.get_size(file_obj)?;
    //     let mut shards: Vec<Option<Vec<u8>>> = vec![None; *file_size as usize];

    //     let segments_paths = &self.get_segments_paths(file_obj)?;

    //     // before we were iterating the chunks in 6, but now we have a complete list of chunk paths
    //     // so we just iterate over the chunk paths
    //     // !TODO below
    //     // this is us making sure that our segments compiled data's hash
    //     // matches the hash found in the merkle-tree
    //     // so we need a method to make a hashing function
    //     // which takes in a segment path
    //     // and returns a segment's hash
    //     // all of the files inside of a segment get combined and then hashed

    //     // okay so we need to read the segments per iteration
    //     // per iteration, we get the segment hash and compare against the manifest file
    //     let index_counter: &i32 = &0;

    //     for segment in segments_paths {
    //         if let segment_data = &self.read_segment(*segment)? {
    //             // verify with manifest
    //             let computed_hash = &self.segment_hash(*segment_data)?;
    //             let expected_hash = file_obj
    //                 .manifest
    //                 .merkle_tree
    //                 .leaves
    //                 .get(index_counter)
    //                 .map(|node| node);

    //             if Some(computed_hash.clone()) == expected_hash.cloned() {
    //                 let flattened_data: Vec<u8> = segment_data.iter().flatten().copied().collect();
    //                 shards[*index_counter as usize] = Some(flattened_data);
    //                 println!("Data chunk {} valid", index_counter);
    //             } else {
    //                 println!("Data chunk {} corrupted", index_counter);
    //             }
    //         }
    //         *index_counter += 1;
    //     }

    //     let file_stem = Path::new(&file_obj.file_name)
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
