use std::{
    fs,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

use crate::{manifest::ManifestStructure, merkle_tree::MerkleTree, utils::sha256};
/**
 * get the bytes
 * before we start chunking, we figure out the files information
 * - file hash
 * - file name
 * - file data
 * after thats evaluated, we need to create a new folder called [file_name]_[file_hash]
 * in that folder we need to write a file called manifest
 *
 * after those preliminarys are done, we can then proceed to manipulate our file
 *
 * so do we go with a single function call that does all of the chunking and merkle tree hashing?
 * i think we need to
 */

pub struct Chunker {
    pub file_name: String,
    pub file_data: Vec<u8>,
    pub file_size: usize,
    pub file_chunks: Vec<Vec<u8>>,
    pub file_dir: PathBuf,
    pub file_trun_hash: String,
    pub file_hash: String,
    pub merkle_tree: MerkleTree,
    pub committed: bool,
}

impl Chunker {
    pub fn new(file_name: String, file_data: Vec<u8>) -> Self {
        let file_hash = sha256(&file_data);

        let file_trun_hash = file_hash[0..10].to_string();

        let file_chunks = Self::get_chunks(&file_data);

        let merkle_tree = MerkleTree::new(file_chunks.clone());

        let file_dir = Self::get_dir(&file_name, &file_hash);
        let file_size = file_data.len();
        Chunker {
            file_name,
            file_data,
            file_chunks,
            file_trun_hash,
            file_hash,
            file_dir,
            merkle_tree,
            file_size,
            committed: false,
        }
    }

    pub fn create_and_commit(
        file_name: String,
        file_data: Vec<u8>,
    ) -> Result<Self, std::io::Error> {
        let mut chunker = Self::new(file_name, file_data);
        let _ = chunker.commit_all();
        Ok(chunker)
    }

    pub fn commit_all(&mut self) -> Result<(), String> {
        if self.committed {
            return Ok(());
        }
        if self.create_dir() {
            self.write_chunks();
            self.write_manifest();
            self.committed = true;

            Ok(())
        } else {
            if !self.should_repair() {
                self.committed = true;
                Ok(())
            } else {
                self.repair();
                Ok(())
            }
        }
    }

    pub fn should_repair(&self) -> bool {
        /*
         * we should add a self healing functionality for the process
         * if its interupted some how, we should be able to use an algorithm to continue the process
         * if during the process we get a dir that already contains chunks
         * because its a process thats based on raw non-repeating values
         * we can read those chunks, create a merkle tree,
         * and for the chunks we've read, we continue our new process
         * if all of those read chunks match up with our chunks we're processing again
         * we then continue on to commit the new chunks
         * we can construct a merkle tree with those chunks that we already have
         * we can then create another merkle tree to check against
         * we'll have 2 bits of leaves and 2 merkle tree's
         * from there we just need to check, per chunk when sorted,
         * if those proofs match then we filter out those chunks already present and write the new ones
         *
         * - manifest.json missing?        -> YES = repair needed
         * - any chunk file missing?       -> YES = repair needed
         * - merkle tree can't be built?   -> YES = repair needed
         * - All chunks present and valid? -> NO  = no repair needed
         */

        // go to dir and check to see if there's a manifest.json present
        let manifest_path = self.file_dir.join("manifest.json");

        // Try to load manifest
        let manifest = ManifestStructure::from_file(&manifest_path);

        // Validate structure
        if !manifest.validate() {
            return true; // Bad structure = repair needed
        }

        // Read chunks
        let chunks = match self.read_chunks() {
            Ok(files) => files,
            Err(_) => return true, // Can't read chunks = repair needed
        };

        // Check chunk count
        if chunks.len() != manifest.merkle_tree.leaves.len() {
            return true; // Wrong count = repair needed
        }

        // Verify data matches manifest
        if !manifest.verify_against_chunks(&chunks) {
            return true; // Data doesn't match = repair needed
        }

        false
    }

    pub fn read_chunks(&self) -> Result<Vec<Vec<u8>>, std::io::Error> {
        let chunk_data: Result<Vec<Vec<u8>>, _> = fs::read_dir(&self.file_dir)?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_file())
            .map(|path| fs::read(path))
            .collect();

        return chunk_data;
    }

    pub fn repair(&self) -> bool {
        /*
         * flow chart:
         * 1. read_existing_chunks() -> Vec<(index, Vec<u8>)>
         * 2. build_merkle_from_existing() -> MerkelTree
         * 3. verify_each_chunk() -> valid_indices, corrupt_indices
         *      - for each existing chunk
         *        1. hash matches?  - YES -> mark as Valid
         *        2. hash mis-match - NO  -> mark as Corrupt
         * 4. find_missing_chunks() -> [0..n] filter valid_indices
         * 5. write_missing_and_corrupt()
         *    for index in (missing + corrupt):
         *        write_chunk(index)
         * 6. rebuild_merkle_tree() -> use ALL chunks (valid+repaired)
         * 7. update_manifest()
         *        new merkle root + time_stamp
         * commited = true
         *
         *
         * ------------------------------------
         * Only write:
         *      - Missing chunks (not on disk)
         *      - Corrupt chunks (hash mismatch)
         *
         *  Keep:
         *      - Valid chunks (hash matches)
         *
         * ------------------------------------
         * struct RepairReport {
         *       total_chunks: usize,
         *       valid_chunks: Vec<usize>,      // Indices of good chunks
         *       corrupt_chunks: Vec<usize>,    // Indices of bad chunks
         *       missing_chunks: Vec<usize>,    // Indices not on disk
         *       repaired: bool,
         *   }
         *
         * repair_incremental():
         *   1. existing = read_all_chunk_files()
         *   2. valid = []
         *   3. corrupt = []
         *
         *   4. for (index, existing_chunk) in existing:
         *       if sha256(existing_chunk) == sha256(self.file_chunks[index]):
         *           valid.push(index)
         *       else:
         *           corrupt.push(index)
         *
         *   5. missing = [0..self.file_chunks.len()] - existing.keys()
         *
         *   6. to_write = missing + corrupt
         *
         *   7. for index in to_write:
         *       write_chunk_file(index, self.file_chunks[index])
         *
         *   8. rebuild_merkle_tree(all_chunks)
         *   9. write_manifest()
         *
         */

        true
    }

    pub fn write_manifest(&self) {
        // so we need to now, go to that file dir
        // use our chunk array to create our merkle tree
        // when our merkle tree is created
        // we will use the json to write our manifest
        // along with injecting the file's original hash
        // and the time of its creation
        // and the file name and the original number of bytes it had
    }

    pub fn get_dir(file_name: &String, file_hash: &String) -> std::path::PathBuf {
        let path = format!("archive_directory/{}_{}", file_name, file_hash);
        let dir = Path::new(&path);
        return dir.to_path_buf();
    }

    pub fn create_dir(&self) -> bool {
        if !&self.file_dir.is_dir() {
            fs::create_dir(&self.file_dir).expect("");
            return true;
        } else {
            return false;
        }
    }

    pub fn get_chunks(file_data: &[u8]) -> Vec<Vec<u8>> {
        let chunk_size = file_data.len() / 6;

        (0..file_data.len())
            .step_by(chunk_size)
            .map(|i| file_data[i..(i + chunk_size).min(file_data.len())].to_vec())
            .collect()
    }

    pub fn write_chunks(&self) {
        // get the file name and seperate them based on the .
        let file_parts: Vec<&str> = self.file_name.split(".").collect();
        // get the last element of the split array
        let file_ext = file_parts.last().unwrap_or(&"");
        // get the first element of the split array
        let _file_name = file_parts.first().unwrap_or(&"");
        // enumerate over the chunks
        for (index, chunk) in self.file_chunks.iter().enumerate() {
            // create a chunk path based on the enumerated chunk index
            let path = format!(
                "{}/{}_{}.{}",
                self.file_dir.to_str().unwrap_or(""),
                _file_name,
                index,
                file_ext
            );
            // write that chunk with the formatted path
            fs::write(path, chunk).expect("msg");
        }
    }

    // pub fn verify_all(&self) {}
    pub fn get_hash(&self) -> String {
        sha256(&self.file_data)
    }
}
