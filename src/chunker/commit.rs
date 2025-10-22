use std::{fs::File, io::Read, path::Path};

use super::Chunker;
use crate::{
    chunker::ChunkedFile,
    merkle_tree::MerkleTree,
    utils::{determine_segment_size, hash_file_streaming},
};

impl Chunker {
    pub fn commit(&self, file_path: &Path) -> Result<ChunkedFile, String> {
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
        let file_dir = self.get_dir(&file_name, &file_hash);

        // below this is where we need to seperate the instansiation
        // and the commiting of the file.

        // we need to ensure that the archive directory is there, and its created for this file
        self.check_for_archive_dir();
        // process segments and build merkle tree
        let mut segment_hashes = Vec::new();
        let mut buffer = vec![0u8; segment_size];

        for segment_index in 0..num_segments {
            // read one segment
            let bytes_read = file.read(&mut buffer).expect("failed to read segment");
            let segment_data = &buffer[..bytes_read];

            // process with existing functions
            let chunks = self.get_chunks(segment_data);
            let parity = self.generate_parity(&chunks, 6, 3).expect("msg");

            // write chunks immediately
            self.write_segment_chunks(segment_index, &file_name, &file_hash, &chunks, &parity);
            // collect hash for merkle tree
            let segment_hash = self.hash_segment(&chunks, &parity);
            segment_hashes.push(segment_hash);
        }
        let merkle_tree = MerkleTree::from_hashes(segment_hashes);
        // then we need to write our manifest

        self.write_manifest(
            &merkle_tree,
            &file_hash,
            &file_name,
            file_size,
            6,
            3,
            &file_dir,
        );

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
