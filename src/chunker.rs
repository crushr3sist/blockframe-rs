//! File chunking and Reed-Solomon erasure coding for self-healing archival storage.
//!
//! This module provides functionality to split files into chunks, generate parity data
//! for error correction, and reconstruct missing chunks using Reed-Solomon erasure coding.
//! The implementation ensures data integrity through Merkle tree verification and supports
//! self-healing repair without requiring the original file.

use chrono::{DateTime, Utc};
use std::{
    fs::{self, File},
    io::{BufWriter, Read, Write},
    path::{self, Path, PathBuf},
};

use serde_json::json;
use sysinfo::System;

use crate::{
    manifest::ManifestStructure,
    merkle_tree::{self, MerkleTree},
    utils::{determine_segment_size, hash_file_streaming, sha256},
};
use reed_solomon_erasure::galois_8::ReedSolomon;

pub struct Chunker {
    pub file_name: String,
    pub file_size: usize,
    pub file_dir: PathBuf,
    pub file_trun_hash: String,
    pub file_hash: String,
    pub merkle_tree: MerkleTree,
    pub committed: bool,
    pub data_shards: usize,
    pub parity_shards: usize,
    pub segment_size: usize,
    pub num_segments: usize,
}

impl Chunker {
    pub fn new(file_path: &Path) -> Result<Self, String> {
        // 1. Get file metadata (doesnt load file)
        let mut file = File::open(file_path).expect("couldnt read the file");
        let file_size = file.metadata().expect("no metadata available").len() as usize;
        let file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .to_string();

        // 2. determine segment size
        let segment_size = determine_segment_size(file_size as u64);
        let num_segments = (file_size + segment_size - 1) / segment_size;

        // 3. hash entire file for directory naming
        let file_hash = hash_file_streaming(file_path).expect("problem with streaming hashing");
        let file_trun_hash = file_hash[0..10].to_string();
        let file_dir = Self::get_dir(&file_name, &file_hash);

        // we need to ensure that the archive directory is there, and its created for this file
        Self::check_for_archive_dir();
        // process segments and build merkle tree
        let mut segment_hashes = Vec::new();
        let mut buffer = vec![0u8; segment_size];

        for segment_index in 0..num_segments {
            // read one segment
            let bytes_read = file.read(&mut buffer).expect("failed to read segment");
            let segment_data = &buffer[..bytes_read];

            // process with existing functions
            let chunks = Self::get_chunks(segment_data);
            let parity = Self::generate_parity(&chunks, 6, 3).expect("msg");

            // write chunks immediately
            Self::write_segment_chunks(segment_index, &file_name, &file_hash, &chunks, &parity);
            // collect hash for merkle tree
            let segment_hash = Self::hash_segment(&chunks, &parity);
            segment_hashes.push(segment_hash);
        }
        let merkle_tree = MerkleTree::from_hashes(segment_hashes);
        // then we need to write our manifest

        Self::write_manifest(
            &merkle_tree,
            &file_hash,
            &file_name,
            file_size,
            6,
            3,
            &file_dir,
        );

        Ok(Chunker {
            file_name,
            file_size,
            segment_size,
            num_segments,
            file_dir,
            file_trun_hash,
            file_hash,
            merkle_tree,
            committed: false,
            data_shards: 6,
            parity_shards: 3,
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

    pub fn hash_segment(chunks: &[Vec<u8>], parity: &[Vec<u8>]) -> String {
        let combined: Vec<Vec<u8>> = chunks.iter().chain(parity.iter()).cloned().collect();
        let segment_tree = MerkleTree::new(combined);

        segment_tree.get_root().to_string()
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

    pub fn get_chunks(file_data: &[u8]) -> Vec<Vec<u8>> {
        let total_len = file_data.len();
        let chunk_size = (total_len + 5) / 6; // Round up to ensure we don't create more than 6 chunks

        let mut chunks = Vec::new();

        for i in 0..6 {
            let start = i * chunk_size;
            let end = ((i + 1) * chunk_size).min(total_len);

            if start < total_len {
                chunks.push(file_data[start..end].to_vec());
            } else {
                // If we've exhausted the data, push empty chunks
                chunks.push(vec![]);
            }
        }

        chunks
    }

    fn generate_parity(
        data_chunks: &[Vec<u8>],
        data_shards: usize,
        parity_shards: usize,
    ) -> Result<Vec<Vec<u8>>, String> {
        // create Reed-Solomon encoded
        let encoder = ReedSolomon::new(data_shards, parity_shards)
            .map_err(|e| format!("Failed to create RS encoder: {:?}", e))?;

        // Find max chunk size (all chunks must be the same size for RS)
        let max_chunk_size = data_chunks
            .iter()
            .map(|chunk| chunk.len())
            .max()
            .ok_or("No chunks provided")?;

        // Pad all data chunks to max size
        let mut padded_chunks: Vec<Vec<u8>> = data_chunks
            .iter()
            .map(|chunk| {
                let mut padded = chunk.clone();
                padded.resize(max_chunk_size, 0);
                padded
            })
            .collect();

        // create empty parity chunks
        let mut parity_chunks: Vec<Vec<u8>> = vec![vec![0u8; max_chunk_size]; parity_shards];
        // combine data + parity for encoding
        let mut all_shards: Vec<&mut [u8]> = padded_chunks
            .iter_mut()
            .map(|v| v.as_mut_slice())
            .chain(parity_chunks.iter_mut().map(|v| v.as_mut_slice()))
            .collect();

        // magic: generate parity data
        encoder
            .encode(&mut all_shards)
            .map_err(|e| format!("RS encoding failed: {:?}", e))?;

        println!(
            "Generated {} parity chunks from {} data chunks",
            parity_shards, data_shards
        );

        Ok(parity_chunks)
    }

    pub fn should_repair(&self) -> bool {
        // go to dir and check to see if there's a manifest.json present
        let manifest_path = self.file_dir.join("manifest.json");

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

    pub fn read_chunks(&self) -> Option<Vec<Vec<u8>>> {
        fs::read_dir(&self.file_dir).ok().map(|read_dir| {
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
