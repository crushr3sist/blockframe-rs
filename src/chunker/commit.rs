use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::path::Path;

use super::Chunker;
use crate::chunker::ChunkedFile;
use crate::merkle_tree::{
    MerkleTree,
    manifest::{BlockHashes, MerkleTreeStructure, SegmentHashes},
use tracing::info;
};
use crate::utils::blake3_hash_bytes;
use rayon::prelude::*;
use tracing::info;

use crate::utils::determine_segment_size;

use memmap2::Mmap;

impl Chunker {
    /// Commit tiny, like storing a small treasure in a safe deposit box. "Keep it secure," the banker says.
    /// I'd read the file, pad to 64, generate parity, write files. "Protected!"
    /// Committing tiny is like that – RS(1,3), create data and parity. "Safe deposit!"
    /// There was this small item I kept losing, put it in a safe place. Peace of mind.
    /// Life's about security, from treasures to files.
    /// Tier 1 commit for files under 10MB. Uses RS(1,3) encoding where the whole file
    /// is treated as a single data shard with 3 parity shards. File is padded to 64-byte
    /// boundary (Reed-Solomon requirement), then 3 parity shards are generated.
    /// Creates data.dat + parity_0/1/2.dat, builds merkle tree from hashes, writes manifest.
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
        let padded_size = file_data.len().div_ceil(64) * 64;
        let parity = self.generate_parity_segmented(&file_data)?;

        let file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("error getting filename")?
            .to_string();

        info!("COMMIT | (tiny) confirming filename: {:?}", file_name);

        let file_hash = blake3_hash_bytes(&file_data)?;

        info!("COMMIT | (tiny) hash: {:?} for: {:?}", file_hash, file_name);

        let parity0_hash = blake3_hash_bytes(&parity[0])?;
        info!(
            "COMMIT | (tiny) parity hash 1: {:?} for: {:?}",
            parity0_hash, file_name
        );
        let parity1_hash = blake3_hash_bytes(&parity[1])?;
        info!(
            "COMMIT | (tiny) parity hash 2: {:?} for: {:?}",
            parity1_hash, file_name
        );
        let parity2_hash = blake3_hash_bytes(&parity[2])?;
        info!(
            "COMMIT | (tiny) parity hash 3: {:?} for: {:?}",
            parity2_hash, file_name
        );

        let file_trun_hash = file_hash[0..10].to_string();

        let file_dir = self.get_dir(&file_name, &file_hash)?;
        let archive_dir_check = self.check_for_archive_dir()?;
        info!("COMMIT | (tiny) archive_dir check {:?}", archive_dir_check);
        let shard_name = "data.dat";
        let shard_path = &file_dir.join(shard_name);

        self.create_dir(&file_dir)?;
        info!("COMMIT | (tiny) writing shards to {:?}", shard_path);
        fs::write(shard_path, file_data)?;
        self.write_parity_chunks(&file_dir, &parity)?;

        let merkle_tree = MerkleTree::from_hashes(vec![
            file_hash.clone(),
            parity0_hash,
            parity1_hash,
            parity2_hash,
        ])?;

        info!("COMMIT | (tiny) writing manifest to {:?}", &file_dir);

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
        info!(
            "COMMIT | (tiny) {:?} commited successfully to {:?} ",
            &file_hash, &file_dir
        );

        Ok(ChunkedFile {
            file_name,
            file_size,
            segment_size: 0,
            num_segments: 0,
            file_dir,
            file_trun_hash,
            file_hash,
            merkle_tree,
            data_shards: self.data_shards,
            parity_shards: self.parity_shards,
        })
    }

    /// Commit segmented, like organizing a library into sections. "Fiction here, non-fiction there," the librarian says.
    /// I'd divide the file into segments, encode each with RS(1,3). "Organized!"
    /// Committing segmented is like that – mmap for large files, process segments. "Structured!"
    /// There was this messy bookshelf, organized it by genre. Much better.
    /// Life's about organization, from libraries to files.
    /// Tier 2 commit for 10MB-1GB files. Divides file into segments (1/8/32MB depending on size),
    /// each segment gets RS(1,3) encoding for independent recovery. Uses mmap for files >10MB.
    /// Builds merkle tree from segment hashes, computes BLAKE3 hash during processing.
    /// Each segment can lose up to 3 shards and still recover.
    pub fn commit_segmented(
        &self,
        file_path: &Path,
        tier: u8,
    ) -> Result<ChunkedFile, Box<dyn std::error::Error>> {
        // we open a file, so we're not reading the file into memory
        let file = File::open(file_path)?;

        // extracting the file size 10mb - 1gb
        let file_size = file.metadata()?.len() as usize;
        info!(
            "COMMIT | (segmented) reading file from {:?} as tier {:?}",
            file_path, tier
        );
        info!("COMMIT | (segmented) file size: {} bytes", file_size);

        // extract the filename from the path given
        let file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("error getting filename")?
            .to_string();

        info!("COMMIT | (segmented) confirming filename: {:?}", file_name);

        // we using default mmap otherwise we can end up with short reads in normal manual io reads at this size.
        let mmap = unsafe { Mmap::map(&file)? };

        // our file data array which will be used as our mmap file reference
        // our file data array is filled through the memory mapped file as a reference to the memory mapped file
        let file_data: &[u8] = mmap.as_ref();

        // get an optimised segment size 1mb/8mb/32mb
        let segment_size = determine_segment_size(file_size as u64)? as usize;
        info!("COMMIT | (segmented) segment size: {} bytes", segment_size);

        // this is the amount of segments we're going to generate
        // its our file size
        // file_size    = 10mb - 1000mb
        // segment_size = 1mb/8mb/32mb
        // max = 1_000_000_000 + 33_554_432 - 1 / 33_554_432 = 30 segments
        // 30 segments x 3 parity shards = 90 files generated in total
        let num_segments = file_size.div_ceil(segment_size);
        info!(
            "COMMIT | (segmented) total segments to create: {}",
            num_segments
        );
        info!("COMMIT | (segmented) rs encoder will use 1:3 ratio per segment");

        info!("Computing file hash while processing segments...");
        let mut file_hasher = blake3::Hasher::new();

        let file_hash_placeholder = "computing";
        let file_dir = self.get_dir(&file_name, &file_hash_placeholder.to_string())?;
        let parity_dir_joined = file_dir.join("parity");
        let parity_dir = &parity_dir_joined;
        let segments_dir_joined = file_dir.join("segments");
        let segments_dir = &segments_dir_joined;

        self.create_dir(&file_dir)?;
        let parity_created = file_dir.join("parity");
        self.create_dir(&parity_created)?;
        let segments_created = file_dir.join("segments");
        self.create_dir(&segments_created)?;

        // a check and create function for our archive directory
        let archive_dir_check = self.check_for_archive_dir()?;
        info!(
            "COMMIT | (segmented) archive_dir check {:?}",
            archive_dir_check
        );

        info!(
            "COMMIT | (segmented) writing segments to {:?}",
            segments_dir
        );

        // empty vector for our segment hashes that are going to be generated
        // through the numerical index loop
        let mut segment_hashes = Vec::new();
        let mut segments_map = HashMap::new();

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
            let segment_data: &[u8] = {
                // segment_index 0..num_segments:MAX(30)
                // segment_size = 1mb/8mb/32mb
                // start = 0..30 x 1_000_000
                let start = segment_index * segment_size;
                // end = (0..30 + 1) x 1_000_000
                // we're moving megabytes at a time,
                // moving forward by a sort of pagenation of our file
                let end = ((segment_index + 1) * segment_size).min(file_data.len());
                // segment_data is our chunk of data or more, our segment
                // which will be broken up into chunks, except //NOTE we wont be doing that
                // NOTE we're writting the segment data as soon as we get, the parity data is also being written when our segment is provided
                &file_data[start..end]
            };

            // Hash file data as we process segments
            file_hasher.update(segment_data);

            let parity = self.generate_parity_segmented(segment_data)?;

            self.write_segment(segment_index, segments_dir, segment_data)?;
            self.write_segment_parities(segment_index, parity_dir, &parity)?;

            let data_hash = blake3_hash_bytes(segment_data)?;
            let mut parity_hashes = Vec::new();
            for p in &parity {
                parity_hashes.push(blake3_hash_bytes(p)?);
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
        info!(
            "COMMIT | (segmented) file hash: {:?} for: {:?}",
            file_hash, file_name
        );

        // Rename directory to include actual hash
        let final_file_dir = self.get_dir(&file_name, &file_hash)?;
        std::fs::rename(&file_dir, &final_file_dir)?;
        info!("COMMIT | (segmented) renamed directory to include hash");

        let root_tree = MerkleTree::from_hashes(segment_hashes)?;
        let merkle_tree_struct = MerkleTreeStructure {
            leaves: HashMap::new(),
            segments: segments_map,
            blocks: HashMap::new(),
            root: root_tree.root.hash_val.clone(),
        };

        info!(
            "COMMIT | (segmented) writing manifest to {:?}",
            &final_file_dir
        );
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
        info!(
            "COMMIT | (segmented) {:?} commited successfully to {:?}",
            &file_hash, &final_file_dir
        );

        Ok(ChunkedFile {
            file_name,
            file_size,
            segment_size,
            num_segments,
            file_dir: final_file_dir,
            file_trun_hash: file_trun_hash.clone(),
            file_hash,
            merkle_tree: root_tree,
            data_shards: self.data_shards,
            parity_shards: self.parity_shards,
        })
    }

    /// Commit blocked, like building a fortress with multiple walls. "Layered defense," the general says.
    /// I'd divide into blocks, apply RS(30,3) per block. "Fortified!"
    /// Committing blocked is like that – parallel processing, two-level merkle. "Impenetrable!"
    /// There was this fort I built as a kid, multiple layers. Imagination.
    /// Life's about defense, from forts to files.
    /// Tier 3 commit for 1GB-35GB files. Divides into blocks of 30 segments each, applies RS(30,3)
    /// per block. Can lose up to 3 segments per block and still recover. Uses parallel block
    /// processing (Rayon), always mmaps, builds two-level merkle tree (file → blocks → segments).
    pub fn commit_blocked(
        &self,
        file_path: &Path,
        tier: u8,
    ) -> Result<ChunkedFile, Box<dyn std::error::Error>> {
        info!(
            "COMMIT | (blocked) reading file from {:?} as tier {:?}",
            file_path, tier
        );
        // open the file as a buffer object
        let file = File::open(file_path)?;

        // get the file size
        let file_size = file.metadata()?.len() as usize;
        info!("COMMIT | (blocked) file size: {} bytes", file_size);

        // extract the file name
        let file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("error getting filename")?
            .to_string();

        info!("COMMIT | (blocked) confirming filename: {:?}", file_name);
        info!("COMMIT | (blocked) using memory mapping for file access");

        // our file data buffer

        // we're just gonna use mmap because we'd want to for files this size
        let mmap = Some(unsafe { Mmap::map(&file)? });

        // assigning our file data buffer to our mmap file buffer reference
        let file_data: &[u8] = mmap
            .as_ref()
            .ok_or_else(|| std::io::Error::other("could not copy data into memmap"))?;
        // using system available memory, getting the sizes of our segments
        let segment_size = determine_segment_size(file_size as u64)? as usize;
        info!("COMMIT | (blocked) segment size: {} bytes", segment_size);

        // how many in total segments will be made from our file
        let num_segments: usize = file_size.div_ceil(segment_size);
        info!("COMMIT | (blocked) total segments: {}", num_segments);

        // how many blocks will be built with our segments
        // each block needs to have max 30 segments
        let blocks = (num_segments as f64 / 30.0).ceil() as usize;
        info!("COMMIT | (blocked) total blocks: {}", blocks);
        info!("COMMIT | (blocked) rs encoder will use 30:3 ratio per block");

        let file_hash_placeholder = "computing";
        let file_dir = self.get_dir(&file_name, &file_hash_placeholder.to_string())?;
        let archive_dir_check = self.check_for_archive_dir()?;
        info!(
            "COMMIT | (blocked) archive_dir check {:?}",
            archive_dir_check
        );

        let blocks_dir = &file_dir.join("blocks");
        info!(
            "COMMIT | (blocked) creating block directories at {:?}",
            blocks_dir
        );

        self.create_dir(&file_dir)?;
        self.create_dir(blocks_dir)?;

        // pre-create all of the directories needed
        // done to reduce operations per iteration
        let _: Result<(), Box<dyn std::error::Error>> = (0..blocks).try_for_each(|block_index| {
            let current_block_dir = blocks_dir.join(format!("block_{}", block_index));

            self.create_dir(&current_block_dir)?;
            self.create_dir(&current_block_dir.join("segments"))?;
            self.create_dir(&current_block_dir.join("parity"))?;
            Ok(())
        });

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
                                let hash = blake3_hash_bytes(segment_data)?;
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
                        parity_hashes.push(blake3_hash_bytes(p)?);
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

        info!(
            "COMMIT | (blocked) all {} blocks processed successfully",
            blocks
        );

        // mmap already handed us the full file, so just hash the slice directly
        let file_hash = blake3_hash_bytes(file_data)?;
        let file_trun_hash = &file_hash[0..10].to_string();
        println!("File hash computed: {}", file_trun_hash);
        info!(
            "COMMIT | (blocked) file hash: {:?} for: {:?}",
            file_hash, file_name
        );

        let final_file_dir = self.get_dir(&file_name, &file_hash)?;
        std::fs::rename(&file_dir, &final_file_dir)?;
        info!("COMMIT | (blocked) renamed directory to include hash");

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

        info!(
            "COMMIT | (blocked) writing manifest to {:?}",
            &final_file_dir
        );
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
        info!(
            "COMMIT | (blocked) {:?} commited successfully to {:?}",
            &file_hash, &final_file_dir
        );

        Ok(ChunkedFile {
            file_name,
            file_size,
            segment_size,
            num_segments,
            file_dir: final_file_dir,
            file_trun_hash: file_trun_hash.clone(),
            file_hash,
            merkle_tree: root_tree,
            data_shards: self.data_shards,
            parity_shards: self.parity_shards,
        })
    }

    /// Commit, like mailing a letter with proper postage. "Choose the right stamp," the postmaster says.
    /// I'd check the size, select the tier, route to the right method. "Delivered!"
    /// Committing is like that – auto-select tier, commit appropriately. "Archived!"
    /// There was this letter I under-stamped, it came back. Learned the rules.
    /// Life's about routing, from mail to files.
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
    /// | 0 - 25 MB                | 1    | `commit_tiny`       | RS(1,3) whole file |
    /// | 25 MB - 1 GB             | 2    | `commit_segmented`  | RS(1,3) per segment|
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
    /// - **Tier 1** (<= 25MB): Fast, entire file in memory
    /// - **Tier 2** (25MB-1GB): Memory-mapped I/O, segment-by-segment processing
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

        const TIER_1_LIMIT: usize = 25_000_000; // 25MB
        const TIER_2_LIMIT: usize = 1_000_000_000; // 1GB

        let tier: u8 = if file_size == 0 {
            return Err("empty file".into());
        } else if file_size <= TIER_1_LIMIT {
            1
        } else if file_size <= TIER_2_LIMIT {
            2
        } else {
            3
        };

        let which = match tier {
            1 => self.commit_tiny(file_path, file_size, tier)?,
            2 => self.commit_segmented(file_path, tier)?,
            3 => self.commit_blocked(file_path, tier)?,
            _ => self.commit_blocked(file_path, tier)?,
        };

        Ok(which)
    }
}
