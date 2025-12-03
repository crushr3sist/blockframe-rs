use std::fs::File;
use std::path::Path;
use std::{fs, os::windows::fs::MetadataExt};

use super::Chunker;
use crate::chunker::ChunkedFile;
use crate::merkle_tree::MerkleTree;
use crate::utils::sha256;
use rayon::prelude::*;
use reed_solomon_simd::ReedSolomonEncoder;

use std::io::Read;

use crate::utils::determine_segment_size;

use memmap2::Mmap;

const MMAP_THRESHOLD: u64 = 10 * 1024 * 1024;
impl Chunker {
    pub fn commit_tiny(
        &self,
        file_path: &Path,
        file_size: usize,
        tier: u8,
    ) -> Result<ChunkedFile, Box<dyn std::error::Error>> {
        let file_data = fs::read(file_path)?;
        // our tiny file needs to be round up to a multiple of 64

        let padded_size = ((file_data.len() + 63) / 64) * 64;

        let mut padded_data = file_data.to_vec();
        padded_data.resize(padded_size, 0);

        let mut rs_encoder = ReedSolomonEncoder::new(1, 3, padded_size)?;
        // Add all data shards
        rs_encoder.add_original_shard(&padded_data)?;
        let result = rs_encoder.encode()?;

        // Extract parity shards
        let parity: Vec<Vec<u8>> = result.recovery_iter().map(|shard| shard.to_vec()).collect();

        let file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("error getting filename")?
            .to_string();

        let file_hash = sha256(&file_data)?;
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
            let segment_hash = self.hash_single_segment(&segment_data, &parity)?;
            segment_hashes.push(segment_hash);
        }

        // Finalize hash after processing all segments
        let file_hash = file_hasher.finalize().to_string();
        let file_trun_hash = &file_hash[0..10].to_string();
        println!("File hash computed: {}", file_trun_hash);

        // Rename directory to include actual hash
        let final_file_dir = self.get_dir(&file_name, &file_hash)?;
        std::fs::rename(&file_dir, &final_file_dir)?;

        let merkle_tree = MerkleTree::from_hashes(segment_hashes)?;

        self.write_manifest(
            &merkle_tree,
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
            merkle_tree: merkle_tree,
            data_shards: self.data_shards,
            parity_shards: self.parity_shards,
        })
    }

    pub fn commit_blocked(
        &self,
        file_path: &Path,
        tier: u8,
    ) -> Result<ChunkedFile, Box<dyn std::error::Error>> {
        // open the file as a buffer object
        let file = File::open(file_path)?;

        // get the file size
        let file_size = file.metadata()?.file_size() as usize;

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

        let block_root_hashes: Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> = (0
            ..blocks)
            .into_par_iter()
            .map(
                |block_index| -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
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

                    let mut block_leaf_hashes = segment_hashes;
                    block_leaf_hashes.extend(parity_hashes);

                    let block_merkle = MerkleTree::from_hashes(block_leaf_hashes)?;

                    let block_root = block_merkle.root.hash_val.to_string();

                    Ok(block_root)
                },
            )
            .collect();

        let block_root_hashes =
            block_root_hashes.map_err(|e| -> Box<dyn std::error::Error> { e })?;

        // mmap already handed us the full file, so just hash the slice directly
        let file_hash = sha256(file_data)?;
        let file_trun_hash = &file_hash[0..10].to_string();
        println!("File hash computed: {}", file_trun_hash);

        let final_file_dir = self.get_dir(&file_name, &file_hash)?;
        std::fs::rename(&file_dir, &final_file_dir)?;

        let file_merkle_tree = MerkleTree::from_hashes(block_root_hashes)?;

        self.write_manifest(
            &file_merkle_tree,
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
            merkle_tree: file_merkle_tree,
            data_shards: self.data_shards,
            parity_shards: self.parity_shards,
        })
    }

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
            4 => self.commit_segmented(file_path, tier)?,
            _ => self.commit_segmented(file_path, tier)?,
        };

        Ok(which)
    }
}
