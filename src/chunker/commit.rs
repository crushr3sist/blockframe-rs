use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::path::Path;

use super::Chunker;
use crate::chunker::ChunkedFile;
use crate::merkle_tree::{
    MerkleTree,
    manifest::{BlockHashes, MerkleTreeStructure, SegmentHashes},
};
use crate::utils::sha256;
use rayon::prelude::*;
use reed_solomon_simd::ReedSolomonEncoder;
use tracing::info;

use std::io::Read;

use crate::utils::determine_segment_size;

use memmap2::Mmap;

const MMAP_THRESHOLD: u64 = 10 * 1024 * 1024;
impl Chunker {
    /// Commits a tiny file (< 10MB) using Tier 1 Reed-Solomon encoding.
    ///
    /// This function implements the simplest tier of the erasure coding system,
    /// using RS(1,3) where the entire file is treated as a single data shard
    /// with 3 parity shards generated for redundancy.
    ///
    /// # Algorithm
    ///
    /// 1. Read entire file into memory
    /// 2. Pad data to multiple of 64 bytes (Reed-Solomon requirement)
    /// 3. Generate 3 parity shards using RS(1,3) encoder
    /// 4. Write `data.dat` and `parity_0.dat`, `parity_1.dat`, `parity_2.dat`
    /// 5. Create Merkle tree from all 4 shard hashes
    /// 6. Write manifest with metadata
    ///
    /// # Parameters
    ///
    /// * `file_path` - Path to the source file to commit
    /// * `file_size` - Size of the file in bytes (must be < 10MB)
    /// * `tier` - Tier identifier (should be 1 for tiny files)
    ///
    /// # Returns
    ///
    /// * `Ok(ChunkedFile)` - Metadata about the committed file including:
    ///   - Archive directory path
    ///   - File hash (SHA256 or BLAKE3)
    ///   - Merkle tree root
    ///   - Reed-Solomon parameters (1 data shard, 3 parity shards)
    /// * `Err` - If file read fails, RS encoding fails, or I/O error occurs
    ///
    /// # Example Directory Structure
    ///
    /// ```text
    /// archive_directory/
    ///   example.txt_abc123.../
    ///     data.dat          (original file, padded)
    ///     parity_0.dat      (first parity shard)
    ///     parity_1.dat      (second parity shard)
    ///     parity_2.dat      (third parity shard)
    ///     manifest.json     (metadata + merkle root)
    /// ```
    ///
    /// # Recovery Capability
    ///
    /// With RS(1,3), the original file can be recovered from:
    /// - The data shard alone (if healthy)
    /// - Any 1 of the 3 parity shards (if data is missing/corrupt)
    ///
    /// Up to 3 shards can be lost and the file is still recoverable.
    pub fn commit_tiny(
        &self,
        file_path: &Path,
        file_size: usize,
        tier: u8,
    ) -> Result<ChunkedFile, Box<dyn std::error::Error>> {
        info!(
            "COMMIT | (tiny) reading file from {:?} as tier {:?}",
            file_path, tier
        );
        let file_data = fs::read(file_path)?;
        // our tiny file needs to be round up to a multiple of 64
        let padded_size = ((file_data.len() + 63) / 64) * 64;
        info!("COMMIT | (tiny) padded size {} ", padded_size);

        let mut padded_data = file_data.to_vec();
        padded_data.resize(padded_size, 0);

        let mut rs_encoder = ReedSolomonEncoder::new(1, 3, padded_size)?;
        info!("COMMIT | (tiny) rs encoder initalised 1:3 ratio");
        // Add all data shards
        rs_encoder.add_original_shard(&padded_data)?;
        let result = rs_encoder.encode()?;

        // Extract parity shards
        let parity: Vec<Vec<u8>> = result.recovery_iter().map(|shard| shard.to_vec()).collect();
        info!("COMMIT | (tiny) rs encoder initalised 1:3 ratio");

        let file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("error getting filename")?
            .to_string();

        info!("COMMIT | (tiny) confirming filename: {:?}", file_name);

        let file_hash = sha256(&file_data)?;

        info!("COMMIT | (tiny) hash: {:?} for: {:?}", file_hash, file_name);

        let parirty0_hash = sha256(&parity[0])?;
        let parirty1_hash = sha256(&parity[1])?;
        let parirty2_hash = sha256(&parity[2])?;

        let file_trun_hash = file_hash[0..10].to_string();

        let file_dir = self.get_dir(&file_name, &file_hash)?;
        self.check_for_archive_dir()?;

        let shard_name = "data.dat";
        let shard_path = &file_dir.join(shard_name);
        self.create_dir(&file_dir)?;
        fs::write(shard_path, file_data)?;
        self.write_parity_chunks(&file_dir, &parity)?;

        let merkle_tree = MerkleTree::from_hashes(vec![
            file_hash.clone(),
            parirty0_hash,
            parirty1_hash,
            parirty2_hash,
        ])?;

        self.write_manifest(
            &merkle_tree,
            &file_hash,
            &file_name,
            file_size,
            6,
            3,
            &file_dir,
            tier,
            padded_size as u64,
        )?;

        Ok(ChunkedFile {
            file_name: file_name,
            file_size: file_size,
            segment_size: 0,
            num_segments: 0,
            file_dir: file_dir,
            file_trun_hash: file_trun_hash,
            file_hash: file_hash,
            merkle_tree: merkle_tree,
            data_shards: self.data_shards,
            parity_shards: self.parity_shards,
        })
    }

    /// Commits a medium file (10MB - 1GB) using Tier 2 segmented Reed-Solomon encoding.
    ///
    /// This function implements per-segment erasure coding where the file is divided
    /// into segments (1MB/8MB/32MB each depending on file size), and each segment
    /// gets its own RS(1,3) encoding for independent recovery.
    ///
    /// # Algorithm
    ///
    /// 1. Determine optimal segment size (1MB/8MB/32MB) based on file size
    /// 2. Use memory mapping (mmap) for files > 10MB for efficient I/O
    /// 3. Process file in segments:
    ///    - For each segment, generate 3 parity shards using RS(1,3)
    ///    - Write segment to `segments/segment_N`
    ///    - Write parity to `parity/segment_N_parity_0/1/2`
    ///    - Compute segment hash (data + parity combined)
    /// 4. Build Merkle tree from all segment hashes
    /// 5. Compute full file hash (BLAKE3) during segment processing
    /// 6. Write manifest with tier metadata
    ///
    /// # Parameters
    ///
    /// * `file_path` - Path to the source file to commit
    /// * `tier` - Tier identifier (should be 2 for segmented files)
    ///
    /// # Returns
    ///
    /// * `Ok(ChunkedFile)` - Metadata including:
    ///   - Number of segments created
    ///   - Segment size used
    ///   - Merkle tree of segment hashes
    ///   - File hash
    /// * `Err` - If file read fails, RS encoding fails, or I/O error occurs
    ///
    /// # Example Directory Structure
    ///
    /// ```text
    /// archive_directory/
    ///   largefile.bin_def456.../
    ///     segments/
    ///       segment_0          (first data segment)
    ///       segment_1          (second data segment)
    ///       ...
    ///     parity/
    ///       segment_0_parity_0 (first parity for segment 0)
    ///       segment_0_parity_1
    ///       segment_0_parity_2
    ///       segment_1_parity_0 (parity for segment 1)
    ///       ...
    ///     manifest.json
    /// ```
    ///
    /// # Recovery Capability
    ///
    /// Each segment can be independently recovered:
    /// - If segment N is corrupt, use its 3 parity shards to rebuild it
    /// - Multiple segments can be lost as long as each has <3 shards missing
    /// - Allows partial file recovery (e.g., recover segments 0-5 even if 6+ are lost)
    ///
    /// # Performance
    ///
    /// - Uses memory mapping for efficient I/O on large files
    /// - Parallel processing of segment hashing (via Rayon)
    /// - Segment size optimized for system memory constraints
    pub fn commit_segmented(
        &self,
        file_path: &Path,
        tier: u8,
    ) -> Result<ChunkedFile, Box<dyn std::error::Error>> {
        // we open a file, so we're not reading the file into memory
        let mut file = File::open(file_path)?;

        // extracting the file size 10mb - 1gb
        let file_size = file.metadata()?.len() as usize;

        // the threshold of mmap being enabled: 10mb
        let use_mmap = file_size as u64 > MMAP_THRESHOLD;

        // extract the filename from the path given
        let file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("error getting filename")?
            .to_string();

        // our mmap flag, its prefixed with _ as it might be false and empty
        let _mmap: Option<Mmap>;

        // our file data array which will be used as our mmap file reference
        let file_data: &[u8];

        // if our mmap threshold is triggered - file is bigger than 10mb
        if use_mmap {
            // memory mapping our file data
            _mmap = Some(unsafe { Mmap::map(&file)? });

            // our file data array is filled through the memory mapped file as a reference to the memory mapped file
            file_data = _mmap.as_ref().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::Other, "could not copy data into memmap")
            })?;
        } else {
            // if the file is smaller than 10mb then we're not memory mapping our file
            _mmap = None;

            // our file data array which is used as our memory mapped file reference is empty
            // we're going to just read the file later on through our mut file File::open buffer
            file_data = &[];
        }

        // get an optimised segment size 1mb/8mb/32mb
        let segment_size = determine_segment_size(file_size as u64)?;

        // this is the amount of segments we're going to generate
        // its our file size
        // file_size    = 10mb - 1000mb
        // segment_size = 1mb/8mb/32mb
        // max = 1_000_000_000 + 33_554_432 - 1 / 33_554_432 = 30 segments
        // 30 segments x 3 parity shards = 90 files generated in total
        let num_segments = (file_size + segment_size - 1) / segment_size;

        println!("Computing file hash while processing segments...");
        let mut file_hasher = blake3::Hasher::new();

        let file_hash_placeholder = "computing";
        let file_dir = self.get_dir(&file_name, &file_hash_placeholder.to_string())?;
        let parity_dir = &file_dir.join("parity");
        let segments_dir = &file_dir.join("segments");

        self.create_dir(&file_dir)?;
        self.create_dir(&file_dir.join("parity"))?;
        self.create_dir(&file_dir.join("segments"))?;

        // a check and create function for our archive directory
        self.check_for_archive_dir()?;

        // empty vector for our segment hashes that are going to be generated
        // through the numerical index loop
        let mut segment_hashes = Vec::new();
        let mut segments_map = HashMap::new();

        // our segment read buffer
        // its a statically sized array for segment size consistancy
        let mut buffer = vec![0u8; segment_size];

        // iterating by the amount of segments we need to create
        // TODO: we know the amount of segments we need
        // NOTE - we just need to per iteration, create our segment which we already do
        // NOTE - instead of generating chunks, we just write our segment as a data
        // NOTE - our `buffer` has the segment data, just write it
        // NOTE - then generate our parity chunks per segment
        for segment_index in 0..num_segments {
            // our memory segment buffer
            // our `buffer` buffer is for reading the file with a slice
            // this is the storage buffer so that segment data is used in the code
            let segment_data: &[u8];
            // our memory mapping threshold is triggered aka file is <10mb-1000mb
            if use_mmap {
                // segment_index 0..num_segments:MAX(30)
                // segment_size = 1mb/8mb/32mb
                // start = 0..30 x 1_000_000
                let start = segment_index * segment_size;
                // end = (0..30 + 1) x 1_000_000
                // so it looks like we're moving megabytes at a time,
                // or kind of moving forward by a sort of pagenation of our file
                let end = ((segment_index + 1) * segment_size).min(file_data.len());
                // segment_data is our chunk of data or more, our segment
                // which will be broken up into chunks, except //NOTE we wont be doing that
                // NOTE we're writting the segment data as soon as we get, the parity data is also being written when our segment is provided
                segment_data = &file_data[start..end];
            } else {
                // if our file isnt using mmap, that means its just too small to use an overkill expanded and dicescted segment structure
                let bytes_read = file.read(&mut buffer)?;
                // segment data is taken straight from our file read buffer, and all of the file is filled into it.
                segment_data = &buffer[..bytes_read];
            }

            // again this is where we're dividing our data into chunks
            // here we need to just *write* our segment, instead of distributing it into chunks
            // TODO: make a `self.write_segment`
            // TODO: check what write-segment-chunks does and copy it for a

            // Hash file data as we process segments
            file_hasher.update(segment_data);

            let parity = self.generate_parity_segmented(&segment_data)?;

            self.write_segment(segment_index, segments_dir, &segment_data)?;
            self.write_segment_parities(segment_index, parity_dir, &parity)?;

            let data_hash = sha256(&segment_data)?;
            let mut parity_hashes = Vec::new();
            for p in &parity {
                parity_hashes.push(sha256(p)?);
            }

            segments_map.insert(
                segment_index,
                SegmentHashes {
                    data: data_hash.clone(),
                    parity: parity_hashes.clone(),
                },
            );

            let mut segment_leaves = vec![data_hash];
            segment_leaves.extend(parity_hashes);
            let segment_tree = MerkleTree::from_hashes(segment_leaves)?;
            segment_hashes.push(segment_tree.root.hash_val);
        }

        // Finalize hash after processing all segments
        let file_hash = file_hasher.finalize().to_string();
        let file_trun_hash = &file_hash[0..10].to_string();
        println!("File hash computed: {}", file_trun_hash);

        // Rename directory to include actual hash
        let final_file_dir = self.get_dir(&file_name, &file_hash)?;
        std::fs::rename(&file_dir, &final_file_dir)?;

        let root_tree = MerkleTree::from_hashes(segment_hashes)?;
        let merkle_tree_struct = MerkleTreeStructure {
            leaves: HashMap::new(),
            segments: segments_map,
            blocks: HashMap::new(),
            root: root_tree.root.hash_val.clone(),
        };

        self.write_manifest_struct(
            merkle_tree_struct,
            &file_hash,
            &file_name,
            file_size,
            6,
            3,
            &final_file_dir,
            tier,
            segment_size as u64,
        )?;

        Ok(ChunkedFile {
            file_name: file_name,
            file_size: file_size,
            segment_size: segment_size,
            num_segments: num_segments,
            file_dir: final_file_dir,
            file_trun_hash: file_trun_hash.clone(),
            file_hash: file_hash,
            merkle_tree: root_tree,
            data_shards: self.data_shards,
            parity_shards: self.parity_shards,
        })
    }

    /// Commits a large file (1GB - 35GB) using Tier 3 blocked Reed-Solomon encoding.
    ///
    /// This function implements the most complex tier, dividing files into blocks
    /// where each block contains up to 30 segments. Reed-Solomon RS(30,3) is applied
    /// per-block, allowing recovery of up to 3 missing segments within each block.
    ///
    /// # Algorithm
    ///
    /// 1. Determine segment size (1MB/8MB/32MB) based on available memory
    /// 2. Calculate number of blocks needed (segments / 30, rounded up)
    /// 3. For each block (processed in parallel):
    ///    - Collect up to 30 segments from the file
    ///    - Generate 3 parity shards using RS(30,3) encoder
    ///    - Write segments to `blocks/block_N/segments/segment_0..29`
    ///    - Write parity to `blocks/block_N/parity/parity_0/1/2`
    ///    - Build block-level Merkle tree from segment + parity hashes
    ///    - Return block root hash
    /// 4. Build file-level Merkle tree from all block root hashes
    /// 5. Compute full file hash (SHA256 via mmap)
    /// 6. Write manifest with tier 3 metadata
    ///
    /// # Parameters
    ///
    /// * `file_path` - Path to the source file to commit
    /// * `tier` - Tier identifier (should be 3 for blocked files)
    ///
    /// # Returns
    ///
    /// * `Ok(ChunkedFile)` - Metadata including:
    ///   - Number of blocks and segments
    ///   - Two-level Merkle tree (file → blocks → segments)
    ///   - RS parameters (30 data, 3 parity per block)
    /// * `Err` - If file read fails, RS encoding fails, or I/O error occurs
    ///
    /// # Example Directory Structure
    ///
    /// ```text
    /// archive_directory/
    ///   hugefile.bin_789abc.../
    ///     blocks/
    ///       block_0/
    ///         segments/
    ///           segment_0 ... segment_29
    ///         parity/
    ///           parity_0 (first parity shard)
    ///           parity_1
    ///           parity_2
    ///       block_1/
    ///         segments/ ...
    ///         parity/ ...
    ///     manifest.json
    /// ```
    ///
    /// # Recovery Capability
    ///
    /// Within each block:
    /// - Can lose up to 3 segments (out of 30) and still recover
    /// - Parity shards reconstruct missing segments via Reed-Solomon decoding
    /// - Each block is independent - if block 0 is unrecoverable, block 1+ may still work
    ///
    /// # Performance
    ///
    /// - **Parallel block processing**: Uses Rayon to encode blocks concurrently
    /// - **Memory mapping**: Always uses mmap for multi-GB files
    /// - **Parallel segment hashing**: Within each block, segments are hashed in parallel
    /// - Pre-creates all directories upfront to reduce filesystem operations
    pub fn commit_blocked(
        &self,
        file_path: &Path,
        tier: u8,
    ) -> Result<ChunkedFile, Box<dyn std::error::Error>> {
        // open the file as a buffer object
        let file = File::open(file_path)?;

        // get the file size
        let file_size = file.metadata()?.len() as usize;

        // extract the file name
        let file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("error getting filename")?
            .to_string();

        // our file data buffer
        let file_data: &[u8];

        // we're just gonna use mmap because we'd want to for files this size
        let mmap = Some(unsafe { Mmap::map(&file)? });

        // assigning our file data buffer to our mmap file buffer reference
        file_data = mmap.as_ref().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::Other, "could not copy data into memmap")
        })?;
        // using system available memory, getting the sizes of our segments
        let segment_size = determine_segment_size(file_size as u64)? as usize;

        // how many in total segments will be made from our file
        let num_segments: usize = (file_size + segment_size - 1) / segment_size;

        // how many blocks will be built with our segments
        // each block needs to have max 30 segments
        let blocks = (num_segments as f64 / 30.0).ceil() as usize;

        let file_hash_placeholder = "computing";
        let file_dir = self.get_dir(&file_name, &file_hash_placeholder.to_string())?;
        self.check_for_archive_dir()?;

        let blocks_dir = &file_dir.join("blocks");

        self.create_dir(&file_dir)?;
        self.create_dir(&blocks_dir)?;

        // pre-create all of the directories needed
        // done to reduce operations per iteration
        let _: Result<(), Box<dyn std::error::Error>> = (0..blocks)
            .into_iter()
            .map(|block_index| {
                let current_block_dir = blocks_dir.join(format!("block_{}", block_index));

                self.create_dir(&current_block_dir)?;
                self.create_dir(&current_block_dir.join("segments"))?;
                self.create_dir(&current_block_dir.join("parity"))?;
                Ok(())
            })
            .collect();

        let block_results: Result<Vec<(String, BlockHashes)>, Box<dyn std::error::Error + Send + Sync>> = (0
            ..blocks)
            .into_par_iter()
            .map(
                |block_index| -> Result<(String, BlockHashes), Box<dyn std::error::Error + Send + Sync>> {
                    let current_block_dir = blocks_dir.join(format!("block_{}", block_index));
                    let block_segments_dir = current_block_dir.join("segments");
                    let block_parity_dir = current_block_dir.join("parity");

                    let mut block_segments_refs: Vec<&[u8]> = Vec::with_capacity(30);

                    for segment_index in 0..30 {
                        let global_segment = block_index * 30 + segment_index;

                        let segment_start = global_segment * segment_size;
                        let segment_end =
                            ((global_segment + 1) * segment_size).min(file_data.len());

                        if segment_start >= file_data.len() {
                            break;
                        }

                        let segment_data = &file_data[segment_start..segment_end];

                        block_segments_refs.push(segment_data);
                    }

                    // fan the disk writes out because serialising 30 files in a row is painful
                    let hashed_pairs: Vec<(usize, String)> = block_segments_refs
                        .par_iter()
                        .enumerate()
                        .map(
                            |(segment_index, segment_data)| -> Result<_, std::io::Error> {
                                self.write_segment(
                                    segment_index,
                                    &block_segments_dir,
                                    segment_data,
                                )?;
                                let hash = sha256(segment_data)?;
                                Ok((segment_index, hash))
                            },
                        )
                        .collect::<Result<Vec<_>, _>>()?;

                    let mut segment_hashes = vec![String::new(); hashed_pairs.len()];
                    for (idx, hash) in hashed_pairs {
                        segment_hashes[idx] = hash;
                    }

                    let parity = self
                        .generate_parity(&block_segments_refs, block_segments_refs.len(), 3)
                        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
                            e.to_string().into()
                        })?;

                    self.write_blocked_parities(&block_parity_dir, &parity)?;

                    let mut parity_hashes = Vec::new();

                    for p in &parity {
                        parity_hashes.push(sha256(p)?);
                    }

                    // For Tier 3, the block root is the Merkle root of its segments AND parity
                    let mut block_leaves = segment_hashes.clone();
                    block_leaves.extend(parity_hashes.clone());
                    let block_merkle = MerkleTree::from_hashes(block_leaves)?;
                    let block_root = block_merkle.root.hash_val.to_string();

                    Ok((block_root, BlockHashes {
                        segments: segment_hashes,
                        parity: parity_hashes,
                    }))
                },
            )
            .collect();

        let block_results = block_results.map_err(|e| -> Box<dyn std::error::Error> { e })?;
        let (block_root_hashes, block_structs): (Vec<String>, Vec<BlockHashes>) =
            block_results.into_iter().unzip();

        // mmap already handed us the full file, so just hash the slice directly
        let file_hash = sha256(file_data)?;
        let file_trun_hash = &file_hash[0..10].to_string();
        println!("File hash computed: {}", file_trun_hash);

        let final_file_dir = self.get_dir(&file_name, &file_hash)?;
        std::fs::rename(&file_dir, &final_file_dir)?;

        let root_tree = MerkleTree::from_hashes(block_root_hashes)?;

        let mut blocks_map = HashMap::new();
        for (i, b) in block_structs.into_iter().enumerate() {
            blocks_map.insert(i, b);
        }

        let merkle_tree_struct = MerkleTreeStructure {
            leaves: HashMap::new(),
            segments: HashMap::new(),
            blocks: blocks_map,
            root: root_tree.root.hash_val.clone(),
        };

        self.write_manifest_struct(
            merkle_tree_struct,
            &file_hash,
            &file_name,
            file_size,
            30,
            3,
            &final_file_dir,
            tier,
            segment_size as u64,
        )?;

        Ok(ChunkedFile {
            file_name: file_name,
            file_size: file_size,
            segment_size: segment_size,
            num_segments: num_segments,
            file_dir: final_file_dir,
            file_trun_hash: file_trun_hash.clone(),
            file_hash: file_hash,
            merkle_tree: root_tree,
            data_shards: self.data_shards,
            parity_shards: self.parity_shards,
        })
    }

    /// Main entry point for committing any file to the archive.
    ///
    /// This function automatically selects the appropriate tier based on file size
    /// and routes to the correct commit implementation. It serves as the public
    /// API for archiving files without requiring the caller to know about tiers.
    ///
    /// # Tier Selection Logic
    ///
    /// | File Size Range          | Tier | Method              | RS Encoding        |
    /// |--------------------------|------|---------------------|--------------------|
    /// | 0 - 10 MB                | 1    | `commit_tiny`       | RS(1,3) whole file |
    /// | 10 MB - 1 GB             | 2    | `commit_segmented`  | RS(1,3) per segment|
    /// | 1 GB - 35 GB             | 3    | `commit_blocked`    | RS(30,3) per block |
    /// | > 35 GB (future)         | 4    | `commit_segmented`  | (planned expansion)|
    ///
    /// # Parameters
    ///
    /// * `file_path` - Path to the file to archive. File is not loaded into memory
    ///   until tier-specific processing begins.
    ///
    /// # Returns
    ///
    /// * `Ok(ChunkedFile)` - Metadata structure containing:
    ///   - Archive directory path
    ///   - File hash (for deduplication and verification)
    ///   - Merkle tree (for integrity verification)
    ///   - Tier and RS parameters used
    /// * `Err` - If file cannot be opened, or tier-specific commit fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// use blockframe::chunker::Chunker;
    /// use std::path::Path;
    ///
    /// let chunker = Chunker::new()?;
    /// let result = chunker.commit(Path::new("myfile.dat"))?;
    ///
    /// println!("Committed to: {:?}", result.file_dir);
    /// println!("File hash: {}", result.file_hash);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Performance Characteristics
    ///
    /// - **Tier 1** (< 10MB): Fast, entire file in memory
    /// - **Tier 2** (10MB-1GB): Memory-mapped I/O, segment-by-segment processing
    /// - **Tier 3** (1GB-35GB): Parallel block processing, optimized for large files
    ///
    /// # Notes
    ///
    /// - File size is determined via metadata without reading file content
    /// - The function does not modify the original file
    /// - Archive directory is created automatically if it doesn't exist
    /// - Duplicate files (same hash) will overwrite existing archives
    pub fn commit(&self, file_path: &Path) -> Result<ChunkedFile, Box<dyn std::error::Error>> {
        // 1. Get file metadata (doesnt load file)
        let file = File::open(file_path)?;
        let file_size = file.metadata()?.len() as usize;

        let tier: u8 = match file_size {
            0..=10_000_000 => 1,
            10_000_000..=1_000_000_000 => 2,
            1_000_000_000..=35_000_000_000 => 3,
            _ => 4,
        };

        let which = match tier {
            1 => self.commit_tiny(file_path, file_size, tier)?,
            2 => self.commit_segmented(file_path, tier)?,
            3 => self.commit_blocked(file_path, tier)?,
            4 => self.commit_blocked(file_path, tier)?,
            _ => self.commit_segmented(file_path, tier)?,
        };

        Ok(which)
    }
}
