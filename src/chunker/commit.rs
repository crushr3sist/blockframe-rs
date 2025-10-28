use std::{fs::File, io::Read, path::Path};

use super::Chunker;
use crate::merkle_tree::MerkleTree;
use crate::{
    chunker::ChunkedFile,
    utils::{determine_segment_size, hash_file_streaming},
};

use memmap2::Mmap;

const MMAP_THRESHOLD: u64 = 10 * 1024 * 1024;

impl Chunker {
    pub fn commit(&self, file_path: &Path) -> Result<ChunkedFile, std::io::Error> {
        // 1. Get file metadata (doesnt load file)
        let mut file = File::open(file_path)?;
        let file_size = file.metadata()?.len() as usize;
        let use_mmap = file_size as u64 > MMAP_THRESHOLD;
        let file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::Other, "error getting filename")
            })?
            .to_string();

        let _mmap: Option<Mmap>;
        let file_data: &[u8];

        if use_mmap {
            // we can use mmap is our file is larger than 100mb, which is going to be quite often.
            _mmap = Some(unsafe { Mmap::map(&file)? });
            file_data = _mmap.as_ref().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::Other, "could not copy data into memmap")
            })?;
        } else {
            _mmap = None;
            file_data = &[];
        }

        // 2. determine segment size
        let segment_size = determine_segment_size(file_size as u64)?;
        let num_segments = (file_size + segment_size - 1) / segment_size;

        // 3. hash entire file for directory naming
        let file_hash = hash_file_streaming(file_path)?;
        let file_trun_hash = file_hash[0..10].to_string();
        let file_dir = self.get_dir(&file_name, &file_hash)?;

        // below this is where we need to seperate the instansiation
        // and the commiting of the file.

        // we need to ensure that the archive directory is there, and its created for this file
        self.check_for_archive_dir()?;
        // process segments and build merkle tree
        let mut segment_hashes = Vec::new();
        let mut buffer = vec![0u8; segment_size];

        for segment_index in 0..num_segments {
            let segment_data: &[u8];
            if use_mmap {
                // calculate slice boundaries
                let start = segment_index * segment_size;
                let end = ((segment_index + 1) * segment_size).min(file_data.len());

                // just slice the mmap no copying required
                segment_data = &file_data[start..end];
            } else {
                // read one segment
                let bytes_read = file.read(&mut buffer)?;
                segment_data = &buffer[..bytes_read];
            }

            // process with existing functions
            let chunks = self.get_chunks(segment_data)?;
            let parity = self.generate_parity(&chunks, 6, 3)?;

            // write chunks immediately
            self.write_segment_chunks(segment_index, &file_name, &file_hash, &chunks, &parity)?;
            // collect hash for merkle tree
            let segment_hash = self.hash_segment(&chunks, &parity)?;
            segment_hashes.push(segment_hash);
        }
        let merkle_tree = MerkleTree::from_hashes(segment_hashes)?;
        // then we need to write our manifest

        self.write_manifest(
            &merkle_tree,
            &file_hash,
            &file_name,
            file_size,
            6,
            3,
            &file_dir,
        )?;

        Ok(ChunkedFile {
            file_name: file_name,
            file_size: file_size,
            segment_size: segment_size,
            num_segments: num_segments,
            file_dir: file_dir,
            file_trun_hash: file_trun_hash,
            file_hash: file_hash,
            merkle_tree: merkle_tree,
            data_shards: self.data_shards,
            parity_shards: self.parity_shards,
        })
    }
}
