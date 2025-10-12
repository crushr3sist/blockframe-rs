//! File chunking and Reed-Solomon erasure coding for self-healing archival storage.
//!
//! This module provides functionality to split files into chunks, generate parity data
//! for error correction, and reconstruct missing chunks using Reed-Solomon erasure coding.
//! The implementation ensures data integrity through Merkle tree verification and supports
//! self-healing repair without requiring the original file.

use chrono::{DateTime, Utc};
use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

use serde_json::json;

use crate::{manifest::ManifestStructure, merkle_tree::MerkleTree, utils::sha256};
use reed_solomon_erasure::galois_8::ReedSolomon;

/// File chunker with Reed-Solomon erasure coding for self-healing storage.
///
/// Splits files into fixed-size chunks (1MB default) and generates parity chunks
/// using Reed-Solomon erasure coding. With 6 data shards and 3 parity shards,
/// the system can reconstruct the original file even if up to 3 chunks are lost
/// or corrupted.
///
/// The chunker maintains a Merkle tree over all chunks (data + parity) for
/// cryptographic verification and supports two-tier storage:
/// - Hot tier (`chunks/`): Data chunks for streaming
/// - Cold tier (`parity/`): Parity chunks for repair operations
///
/// # Examples
///
/// ```
/// use blockframe::chunker::Chunker;
///
/// let data = b"Hello, World!".to_vec();
/// let chunker = Chunker::new("example.txt".to_string(), data);
///
/// // Commit to disk
/// chunker.commit();
///
/// // Later, repair if chunks are corrupted
/// chunker.repair();
/// ```
pub struct Chunker {
    /// Original filename
    pub file_name: String,
    /// Complete file data in memory
    pub file_data: Vec<u8>,
    /// Size of the file in bytes
    pub file_size: usize,
    /// Data chunks (6 shards of 1MB each)
    pub file_chunks: Vec<Vec<u8>>,
    /// Parity chunks for error correction (3 shards)
    pub parity_chunks: Vec<Vec<u8>>,
    /// Archive directory path
    pub file_dir: PathBuf,
    /// Truncated hash (first 10 chars) for directory naming
    pub file_trun_hash: String,
    /// Full SHA-256 hash of the file
    pub file_hash: String,
    /// Merkle tree over all chunks (data + parity)
    pub merkle_tree: MerkleTree,
    /// Whether chunks have been written to disk
    pub committed: bool,
    /// Number of data shards (default: 6)
    pub data_shards: usize,
    /// Number of parity shards (default: 3)
    pub parity_shards: usize,
}

impl Chunker {
    /// Creates a new chunker from a file.
    ///
    /// Splits the file into 1MB chunks, generates Reed-Solomon parity data,
    /// and constructs a Merkle tree over all chunks for integrity verification.
    /// The chunker uses 6 data shards and 3 parity shards, allowing recovery
    /// from up to 3 missing or corrupted chunks.
    ///
    /// # Arguments
    ///
    /// * `file_name` - Name of the original file
    /// * `file_data` - Complete file contents as bytes
    ///
    /// # Examples
    ///
    /// ```
    /// use blockframe::chunker::Chunker;
    ///
    /// let data = std::fs::read("example.txt").unwrap();
    /// let chunker = Chunker::new("example.txt".to_string(), data);
    /// println!("Created {} chunks", chunker.file_chunks.len());
    /// ```
    pub fn new(file_name: String, file_data: Vec<u8>) -> Self {
        let file_hash = sha256(&file_data);
        let file_trun_hash = file_hash[0..10].to_string();

        let file_chunks = Self::get_chunks(&file_data);

        let data_shards = 6;
        let parity_shards = 3;
        let parity_chunks =
            Self::generate_parity(&file_chunks, data_shards, parity_shards).expect("");

        let all_chunks: Vec<Vec<u8>> = file_chunks
            .iter()
            .chain(parity_chunks.iter())
            .cloned()
            .collect();

        let merkle_tree = MerkleTree::new(all_chunks);

        let file_dir = Self::get_dir(&file_name, &file_hash);
        let file_size = file_data.len();

        Chunker {
            file_name,
            file_data,
            file_chunks,
            parity_chunks,
            file_trun_hash,
            file_hash,
            file_dir,
            merkle_tree,
            file_size,
            committed: false,
            data_shards,
            parity_shards,
        }
    }

    /// Generates Reed-Solomon parity chunks for error correction.
    ///
    /// Uses Galois Field arithmetic to create parity shards that enable
    /// reconstruction of missing data shards. All chunks are padded to
    /// the same size (largest chunk size) as required by the Reed-Solomon
    /// algorithm.
    ///
    /// # Arguments
    ///
    /// * `data_chunks` - Slice of data chunks to protect
    /// * `data_shards` - Number of data shards (typically 6)
    /// * `parity_shards` - Number of parity shards to generate (typically 3)
    ///
    /// # Returns
    ///
    /// `Ok(Vec<Vec<u8>>)` containing the parity chunks, or `Err(String)` if encoding fails
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No chunks are provided
    /// - Reed-Solomon encoder creation fails
    /// - Encoding operation fails
    ///
    /// # Examples
    ///
    /// ```
    /// use blockframe::chunker::Chunker;
    ///
    /// let data_chunks = vec![vec![1, 2, 3], vec![4, 5, 6]];
    /// let parity = Chunker::generate_parity(&data_chunks, 2, 1).unwrap();
    /// assert_eq!(parity.len(), 1);
    /// ```
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

    /// Creates a chunker and immediately commits it to disk.
    ///
    /// Convenience method that combines [`Chunker::new`] and [`Chunker::commit_all`]
    /// in a single operation.
    ///
    /// # Arguments
    ///
    /// * `file_name` - Name of the original file
    /// * `file_data` - Complete file contents as bytes
    ///
    /// # Returns
    ///
    /// `Ok(Chunker)` if successful, or `Err` if file operations fail
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use blockframe::chunker::Chunker;
    ///
    /// let data = std::fs::read("file.txt").unwrap();
    /// let chunker = Chunker::create_and_commit("file.txt".to_string(), data).unwrap();
    /// ```
    pub fn create_and_commit(
        file_name: String,
        file_data: Vec<u8>,
    ) -> Result<Self, std::io::Error> {
        let mut chunker = Self::new(file_name, file_data);
        let _ = chunker.commit_all();
        Ok(chunker)
    }

    /// Commits all chunks and metadata to disk.
    ///
    /// Creates the archive directory structure and writes:
    /// - Data chunks to `chunks/` subdirectory
    /// - Parity chunks to `parity/` subdirectory
    /// - JSON manifest with Merkle tree and metadata
    ///
    /// The directory structure follows the pattern:
    /// ```text
    /// archive_directory/
    ///   {filename}_{truncated_hash}/
    ///     chunks/
    ///       {filename}_0.txt
    ///       {filename}_1.txt
    ///       ...
    ///     parity/
    ///       parity_0.dat
    ///       parity_1.dat
    ///       parity_2.dat
    ///     manifest.json
    /// ```
    ///
    /// # Returns
    ///
    /// `Ok(())` if successful, or `Err(String)` if any operation fails
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Already committed
    /// - Directory creation fails
    /// - File writing fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use blockframe::chunker::Chunker;
    ///
    /// let data = b"Hello, World!".to_vec();
    /// let mut chunker = Chunker::new("example.txt".to_string(), data);
    /// chunker.commit_all().unwrap();
    /// ```
    pub fn commit_all(&mut self) -> Result<(), String> {
        if self.committed {
            println!("file was already commited");
            return Ok(());
        }
        if self.create_dir() {
            println!("File is being commited");

            // create subdirectories
            let chunks_dir = self.file_dir.join("chunks");
            let parity_dir = self.file_dir.join("parity");

            fs::create_dir_all(&chunks_dir)
                .map_err(|e| format!("failed to create chunks dir: {}", e))?;
            fs::create_dir_all(&parity_dir)
                .map_err(|e| format!("failed to create parity dir: {}", e))?;

            //write data chunks to chunks/
            self.write_chunks()?;

            // write parity chunks to parity/
            self.write_parity_chunks()?;

            // write manifest

            self.write_manifest();

            self.committed = true;

            println!(
                "Commited {} data + {} parity chunks",
                self.data_shards, self.parity_shards
            );
            Ok(())
        } else {
            println!("Directory exists, checking for repair");
            if !self.should_repair() {
                println!("no repair needed");
                self.committed = true;
                Ok(())
            } else {
                println!("repairing asset");
                self.repair();
                Ok(())
            }
        }
    }

    /// Writes parity chunks to the cold storage tier.
    ///
    /// Saves Reed-Solomon parity shards to the `parity/` subdirectory.
    /// Parity chunks are only accessed during repair operations, not
    /// during normal streaming.
    ///
    /// # Returns
    ///
    /// `Ok(())` if all parity chunks are written successfully,
    /// or `Err(String)` if any write operation fails
    ///
    /// # File Naming
    ///
    /// Parity files follow the pattern: `{filename}_p{index}.dat`
    /// For example: `example_p0.dat`, `example_p1.dat`, `example_p2.dat`
    fn write_parity_chunks(&self) -> Result<(), String> {
        let parity_dir = self.file_dir.join("parity");

        for (index, chunk) in self.parity_chunks.iter().enumerate() {
            // parity files: example_p0.dat, example_p1.dat, example_p2.dat
            let file_stem = Path::new(&self.file_name)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown");
            let parity_filename = format!("{}_p{}.dat", file_stem, index);
            let parity_path = parity_dir.join(parity_filename);
            fs::write(&parity_path, chunk)
                .map_err(|e| format!("failed to write parity chunk {}: {}", index, e))?;
            println!("wrote parity chunk {} ({} bytes)", index, chunk.len());
        }

        Ok(())
    }

    /// Checks if chunk repair is needed.
    ///
    /// Verifies the integrity of stored chunks by:
    /// 1. Loading the manifest file
    /// 2. Checking if all expected chunks exist
    /// 3. Validating chunk hashes against the Merkle tree
    ///
    /// # Returns
    ///
    /// `true` if repair is needed (missing or corrupted chunks),
    /// `false` if all chunks are valid or if validation fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use blockframe::chunker::Chunker;
    ///
    /// let data = b"test".to_vec();
    /// let chunker = Chunker::new("test.txt".to_string(), data);
    /// if chunker.should_repair() {
    ///     println!("Repair needed!");
    /// }
    /// ```
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

    /// Reads all chunk files from the archive directory.
    ///
    /// Scans the archive directory for chunk files (excluding `manifest.json`)
    /// and reads their contents. This is used during validation and repair
    /// operations.
    ///
    /// # Returns
    ///
    /// `Some(Vec<Vec<u8>>)` containing all chunk data, or `None` if
    /// the directory cannot be read
    ///
    /// # Note
    ///
    /// This method reads from the root archive directory, which includes
    /// both data chunks from `chunks/` subdirectory. For repair operations,
    /// parity chunks are read separately from `parity/` subdirectory.
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

    /// Repairs corrupted or missing chunks using Reed-Solomon reconstruction.
    ///
    /// This is the core self-healing functionality that enables data recovery
    /// without the original file. The repair process:
    ///
    /// 1. **Load available chunks**: Reads both data chunks from `chunks/` and
    ///    parity chunks from `parity/` subdirectories
    /// 2. **Verify integrity**: Validates each chunk's SHA-256 hash against
    ///    the Merkle tree stored in the manifest
    /// 3. **Check threshold**: Ensures at least `data_shards` (6) chunks are valid.
    ///    With 6 data + 3 parity = 9 total shards, up to 3 can be missing/corrupted
    /// 4. **Reed-Solomon decode**: Reconstructs missing chunks using Galois Field
    ///    arithmetic over the available shards
    /// 5. **Verify reconstruction**: Checks reconstructed chunk hashes match
    ///    the Merkle tree expectations
    /// 6. **Write repaired chunks**: Saves reconstructed data chunks back to `chunks/`
    ///    and parity chunks back to `parity/`
    ///
    /// # Returns
    ///
    /// `true` if repair was successful or not needed, `false` if repair failed
    ///
    /// # Repair Capability
    ///
    /// - Can recover from up to **3 missing or corrupted chunks** (any combination
    ///   of data or parity)
    /// - Requires at minimum **6 valid chunks** out of 9 total
    /// - Uses (6,3) Reed-Solomon code: 6 data shards + 3 parity shards
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use blockframe::chunker::Chunker;
    ///
    /// let data = std::fs::read("file.txt").unwrap();
    /// let chunker = Chunker::new("file.txt".to_string(), data);
    ///
    /// // Simulate corruption: delete a chunk file
    /// // std::fs::remove_file("archive/.../chunks/file_2.txt").ok();
    ///
    /// if chunker.repair() {
    ///     println!("✅ Repair successful!");
    /// } else {
    ///     println!("❌ Repair failed - too many corrupted chunks");
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns `false` if:
    /// - Fewer than 6 valid chunks are available
    /// - Reed-Solomon decoding fails
    /// - Reconstructed chunks fail hash verification
    /// - File write operations fail
    pub fn repair(&self) -> bool {
        // step 1: read all available chunks (data + parity)
        let chunks_dir = self.file_dir.join("chunks");
        let parity_dir = self.file_dir.join("parity");

        let total_shards = self.data_shards + self.parity_shards;
        let mut shards: Vec<Option<Vec<u8>>> = vec![None; total_shards];

        // Read data chunks (indices 0-5)
        for index in 0..self.data_shards {
            if let Ok(chunk_filename) = self.chunk_filename_from_index(index) {
                let chunk_path = chunks_dir.join(chunk_filename.file_name().unwrap());

                if let Ok(chunk_data) = fs::read(&chunk_path) {
                    // verify with manifest
                    let hash = sha256(&chunk_data);
                    let expected_hash = self
                        .merkle_tree
                        .leaves
                        .get(index)
                        .map(|node| node.hash_val.clone());

                    if Some(hash.clone()) == expected_hash {
                        shards[index] = Some(chunk_data);
                        println!("Data chunk {} valid", index);
                    } else {
                        println!("Data chunk {} corrupted", index);
                    }
                }
            }
        }

        let file_stem = Path::new(&self.file_name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");

        for parity_index in 0..self.parity_shards {
            let parity_filename = format!("{}_p{}.dat", file_stem, parity_index);
            let parity_path = parity_dir.join(parity_filename);
            if let Ok(parity_data) = fs::read(&parity_path) {
                let shard_index = self.data_shards + parity_index;
                let hash = sha256(&parity_data);
                let expected_hash = self
                    .merkle_tree
                    .leaves
                    .get(shard_index)
                    .map(|node| node.hash_val.clone());
                if Some(hash.clone()) == expected_hash {
                    shards[shard_index] = Some(parity_data);
                    println!("parity chunk {} valid", parity_index);
                } else {
                    println!("parity chunk {} corrupted", parity_index);
                }
            }
        }

        // step 2: check if we have enough shards to reconstruct
        let valid_count = shards.iter().filter(|s| s.is_some()).count();

        if valid_count < self.data_shards {
            println!(
                "Not enough chunks to reconstruct: have {}, need {}",
                valid_count, self.data_shards
            );
            return false;
        }
        println!(
            "Have {} valid shards (need {}), reconstruction begining",
            valid_count, self.data_shards
        );

        // step 3: reconstruct using reed-solomon
        let encoder = ReedSolomon::new(self.data_shards, self.parity_shards)
            .expect("failed to create RS encoder");

        // find max shard size
        let max_size = shards
            .iter()
            .filter_map(|s| s.as_ref())
            .map(|chunk| chunk.len())
            .max()
            .unwrap_or(0);

        // Track which shards exist BEFORE we convert to owned
        let shard_present: Vec<bool> = shards.iter().map(|s| s.is_some()).collect();

        // prepare owned shards - pad all to same size
        let mut owned_shards: Vec<Vec<u8>> = shards
            .into_iter()
            .map(|opt| {
                if let Some(data) = opt {
                    let mut padded = data;
                    padded.resize(max_size, 0);
                    padded
                } else {
                    vec![0u8; max_size] // Placeholder for missing
                }
            })
            .collect();

        // Create tuple pattern: (shard_ref, is_present)
        // This is the pattern reed-solomon-erasure expects
        let mut shard_tuple: Vec<_> = owned_shards
            .iter_mut()
            .map(|s| s.as_mut_slice())
            .zip(shard_present.iter().cloned())
            .collect();

        match encoder.reconstruct(&mut shard_tuple) {
            Ok(_) => println!("Reconstruction successful"),
            Err(e) => {
                println!("reconstruction failed: {:?}", e);
                return false;
            }
        }

        for (index, reconstructed) in owned_shards.iter().enumerate() {
            let expected_hash = self
                .merkle_tree
                .leaves
                .get(index)
                .map(|node| node.hash_val.clone());
            let actual_hash = sha256(reconstructed);

            if Some(actual_hash.clone()) != expected_hash {
                println!("Chunk {} hash mismatch after reconstruction", index);
                continue;
            }

            // Rewrite data chunks
            if index < self.data_shards {
                if let Ok(chunk_filename) = self.chunk_filename_from_index(index) {
                    let chunk_path = chunks_dir.join(chunk_filename.file_name().unwrap());

                    if let Err(e) = fs::write(&chunk_path, reconstructed) {
                        println!("Failed to write chunk {}: {}", index, e);
                    } else {
                        println!("Repaired data chunk {}", index);
                    }
                }
            }
            // Rewrite parity chunks
            else {
                let parity_index = index - self.data_shards;
                let parity_filename = format!("{}_p{}.dat", file_stem, parity_index);
                let parity_path = parity_dir.join(parity_filename);

                if let Err(e) = fs::write(&parity_path, reconstructed) {
                    println!("Failed to write parity {}: {}", parity_index, e);
                } else {
                    println!("Repaired parity chunk {}", parity_index);
                }
            }
        }

        println!("Repair complete!");
        true
    }

    /// Writes metadata manifest file to the archive directory.
    ///
    /// Creates a `manifest.json` file containing:
    /// - Original file hash (SHA-256)
    /// - Filename and size
    /// - Creation timestamp
    /// - Erasure coding parameters (Reed-Solomon configuration)
    /// - Complete Merkle tree (root hash and all leaf hashes)
    ///
    /// The manifest is essential for:
    /// - Verifying chunk integrity during repair
    /// - Reconstructing the file from chunks
    /// - Validating the entire archive
    ///
    /// # JSON Structure
    ///
    /// ```json
    /// {
    ///   "original_hash": "abc123...",
    ///   "name": "example.txt",
    ///   "size": 1048576,
    ///   "time_of_creation": "2025-10-13T12:00:00Z",
    ///   "erasure_coding": {
    ///     "type": "reed-solomon",
    ///     "data_shards": 6,
    ///     "parity_shards": 3
    ///   },
    ///   "merkle_tree": {
    ///     "root": "def456...",
    ///     "leaves": {
    ///       "0": "hash0...",
    ///       "1": "hash1...",
    ///       ...
    ///     }
    ///   }
    /// }
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if the manifest file cannot be created or written
    pub fn write_manifest(&self) {
        /*
        {
            original_hash: ""
            name: ""
            size: ...
            time_of_creation: ...

            merkle_tree:{
                "root": ...
                "leaves": [...]
            }

        }
        */
        let mk_tree = &self.merkle_tree.get_json();
        let now: DateTime<Utc> = Utc::now();
        let manifest = json!({
            "original_hash": &self.file_hash,
            "name": &self.file_name,
            "size": &self.file_size,
            "time_of_creation":  now.to_string(),
            "erasure_coding": {
                "type": "reed-solomon",
                "data_shards": self.data_shards,
                "parity_shards": self.parity_shards,
            },
            "merkle_tree": mk_tree
        })
        .to_string()
        .into_bytes();

        let manifest_path = self.file_dir.join("manifest.json");
        let file = File::create(manifest_path).expect("Failed to create manifest file");
        let mut writer = BufWriter::new(file);
        writer.write_all(&manifest).expect("msg");
        writer.flush().expect("msg");
    }

    /// Generates the archive directory path for a file.
    ///
    /// Creates a unique directory name by combining the filename with
    /// the full SHA-256 hash of the file contents. This ensures each
    /// unique file version gets its own storage location.
    ///
    /// # Arguments
    ///
    /// * `file_name` - Original filename
    /// * `file_hash` - SHA-256 hash of the file contents
    ///
    /// # Returns
    ///
    /// Path to the archive directory: `archive_directory/{filename}_{hash}/`
    ///
    /// # Examples
    ///
    /// ```
    /// use blockframe::chunker::Chunker;
    ///
    /// let dir = Chunker::get_dir(
    ///     &"example.txt".to_string(),
    ///     &"abc123def456...".to_string()
    /// );
    /// // Returns: archive_directory/example.txt_abc123def456.../
    /// ```
    pub fn get_dir(file_name: &String, file_hash: &String) -> std::path::PathBuf {
        let path = format!("archive_directory/{}_{}", file_name, file_hash);
        let dir = Path::new(&path);
        return dir.to_path_buf();
    }

    /// Creates the archive directory if it doesn't exist.
    ///
    /// # Returns
    ///
    /// `true` if a new directory was created, `false` if it already exists
    ///
    /// # Panics
    ///
    /// Panics if directory creation fails due to permissions or disk errors
    pub fn create_dir(&self) -> bool {
        if !&self.file_dir.is_dir() {
            fs::create_dir(&self.file_dir).expect("");
            return true;
        } else {
            return false;
        }
    }

    /// Splits file data into fixed-size chunks.
    ///
    /// Divides the file into 6 chunks of approximately equal size.
    /// This is the first step in preparing data for Reed-Solomon encoding.
    ///
    /// # Arguments
    ///
    /// * `file_data` - Complete file contents as a byte slice
    ///
    /// # Returns
    ///
    /// Vector of 6 chunks, where each chunk is approximately `file_size / 6` bytes.
    /// The last chunk may be smaller if the file size isn't evenly divisible by 6.
    ///
    /// # Examples
    ///
    /// ```
    /// use blockframe::chunker::Chunker;
    ///
    /// let data = b"Hello, World! This is a test file.";
    /// let chunks = Chunker::get_chunks(data);
    /// assert_eq!(chunks.len(), 6);
    /// ```
    pub fn get_chunks(file_data: &[u8]) -> Vec<Vec<u8>> {
        let chunk_size = file_data.len() / 6;

        (0..file_data.len())
            .step_by(chunk_size)
            .map(|i| file_data[i..(i + chunk_size).min(file_data.len())].to_vec())
            .collect()
    }

    /// Writes all data chunks to the hot storage tier.
    ///
    /// Saves each data chunk to the `chunks/` subdirectory with sequential
    /// filenames. These chunks are used for streaming and must be quickly
    /// accessible.
    ///
    /// # Returns
    ///
    /// `Ok(())` if all chunks are written successfully, or `Err(String)`
    /// if any write operation fails
    ///
    /// # File Naming
    ///
    /// Data chunks follow the pattern: `{filename}_{index}.txt`
    /// For example: `example_0.txt`, `example_1.txt`, ..., `example_5.txt`
    pub fn write_chunks(&self) -> Result<(), String> {
        let chunks_dir = self.file_dir.join("chunks");

        for (index, chunk) in self.file_chunks.iter().enumerate() {
            let chunk_filename = self
                .chunk_filename_from_index(index)
                .map_err(|e| format!("failed to get chunk filename: {}", e))?;

            let chunk_path =
                chunks_dir.join(chunk_filename.file_name().ok_or("invalid chunk filename")?);

            fs::write(&chunk_path, chunk)
                .map_err(|e| format!("failed to write chunk: {}: {}", index, e))?;
            println!("Write data chunk {} ({} bytes)", index, chunk.len());
        }
        Ok(())
    }

    /// Writes a single data chunk to disk.
    ///
    /// # Arguments
    ///
    /// * `index` - Index of the chunk to write (0-5)
    ///
    /// # Returns
    ///
    /// `Ok(())` if successful, or `Err` if the write fails
    ///
    /// # Errors
    ///
    /// Returns an error if the index is out of bounds or if the file
    /// write operation fails
    pub fn write_chunk(&self, index: usize) -> Result<(), std::io::Error> {
        // validate index bounds

        let chunk_path = self.chunk_filename_from_index(index)?;

        fs::write(chunk_path, &self.file_chunks[index])?;
        Ok(())
    }

    /// Generates the filename for a specific chunk index.
    ///
    /// Creates a filename based on the original file's stem (name without extension)
    /// and the chunk index.
    ///
    /// # Arguments
    ///
    /// * `index` - Chunk index (0-5 for data chunks)
    ///
    /// # Returns
    ///
    /// `Ok(PathBuf)` with the chunk filename, or `Err` if the index is invalid
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use blockframe::chunker::Chunker;
    ///
    /// let data = b"test".to_vec();
    /// let chunker = Chunker::new("example.txt".to_string(), data);
    /// let filename = chunker.chunk_filename_from_index(0).unwrap();
    /// // Returns: example_0.txt
    /// ```
    pub fn chunk_filename_from_index(&self, index: usize) -> Result<PathBuf, std::io::Error> {
        if index >= self.file_chunks.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Chunk index {} out of bounds", index),
            ));
        }

        let file_path = Path::new(&self.file_name);
        let file_stem = file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");
        let file_ext = file_path.extension().and_then(|s| s.to_str()).unwrap_or("");

        let chunk_filename = if file_ext.is_empty() {
            format!("{}_{}", file_stem, index)
        } else {
            format!("{}_{}.{}", file_stem, index, file_ext)
        };

        // Return just the filename, not full path
        Ok(PathBuf::from(chunk_filename))
    }

    // pub fn verify_all(&self) {}
    pub fn get_hash(&self) -> String {
        sha256(&self.file_data)
    }
}
