use chrono::{DateTime, Utc};
use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

use serde_json::json;

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
            println!("file was already commited");
            return Ok(());
        }
        if self.create_dir() {
            println!("file is being commited");
            self.write_chunks();
            self.write_manifest();
            self.committed = true;
            Ok(())
        } else {
            println!("there was an error, repair logic initating");
            if !self.should_repair() {
                println!("there was no need to repair, it was a false positive");
                self.committed = true;
                Ok(())
            } else {
                println!("there was an error, repairing asset");
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

    pub fn repair(&self) -> bool {
        /*

            if chunks are missing, re-commit the missing chunk
            if a chunk is corrupted, re-commit the corrupt chunk
            if the manifest is missing, re-write the manifest
            so we have the original file, this depends on the original file
            clear and simple
            we need 2 merkle trees.
            one merkle tree constructed from the physical archive
        */
        // physical read chunks where we suspect an issue
        let physical_chunks = match self.read_chunks() {
            Some(chunks) => chunks,
            None => {
                println!("Failed to read physical chunks");
                return false;
            }
        };

        // the incoming file chunks, used for the repair
        // we already have a merkle tree for the in memory chunks
        // this is what the session picked up
        // if this file is already commited, and we've ended up here
        // we need to repair the commited file archive.

        // okay so we have a merkle tree for the in memory chunks
        // we need another merkle tree for the read chunks
        let physical_mk = MerkleTree::new(physical_chunks);
        let memory_mk = &self.merkle_tree;
        let mut corrupt: Vec<usize> = Vec::new();

        // we have 2 merkle trees, and 2 sets of chunks
        // now we need to iterate through what physical chunks we have.
        // so before we get into the merkle tree proof, its better to just check in order
        // if all chunks are there or not

        // to do this, we need to read all of the files that arent the manifest.json
        // get the list of thier names
        // filter out the file name and the underscore and file type suffix
        // and we'll have a list of indices.
        // get the file name and seperate them based on the .

        /*"example_0.txt"
            ↓ split("_")
        ["example", "0.txt"]
            ↓ get(1)
        "0.txt"
            ↓ split('.')
        ["0", "txt"]
            ↓ next()
        "0"
            ↓ parse::<usize>()
        0 ✅ */

        let mut indices: Vec<usize> = fs::read_dir(&self.file_dir)
            .ok()
            .map(|read_dir| {
                read_dir
                    .filter_map(|entry| entry.ok())
                    .map(|entry| entry.path())
                    .filter_map(|path| {
                        path.file_name().and_then(|name| name.to_str()).and_then(
                            |file_name: &str| -> Option<usize> {
                                let parts: Vec<&str> = file_name.split("_").collect();
                                parts
                                    .get(1)?
                                    .split(".")
                                    .next()? // gets "0" from "0.txt"
                                    .parse::<usize>()
                                    .ok()
                            },
                        )
                    })
                    .collect::<Vec<usize>>()
            })
            .unwrap_or_default();
        indices.sort();

        dbg!(&indices);

        // we have the indices that are present
        // we need to just make a vector of 0 to 6
        // and filter out those values that arent in both expected and indices
        let missing: Vec<usize> = (0..6).filter(|val| !indices.contains(val)).collect();
        dbg!(&missing);
        // now that we've understood whats missing and whats not
        // its time to recommit those chunks that are missing
        // now we have a list of chunk indexes that are missing
        // we just need to get our in-memory chunk and admit that
        if !missing.is_empty() {
            for index in missing {
                match self.write_chunk(index) {
                    Ok(()) => println!("chunk index {} written successfully", index),
                    Err(e) => println!("writting chunk at index caused error: {}", e),
                };
            }
        }
        // now we call should_repair to check if our problem has been solved, otherwise we keep moving
        if !self.should_repair() {
            return true;
        }

        // if that didnt solve our issue, then we need now fix our corrupted chunks.
        // we have 2 merkle trees now and we have our chunks in order as well
        // we need to go through the range of all of our chunks which should be just 6
        // through that, we index our memory and physical chunks
        // we get thier proofs, and see if the physical and memory chunks are the same
        // if they're not, we remove that chunk from the archive, and rewrite it.

        for idx in 0..6 {
            // loop through out chunk space
            let memory_proof = &self.merkle_tree.get_proof(idx); // get the valid proof
            let physical_proof = physical_mk.get_proof(idx); // get the proof for test
            if physical_mk.verify_proof(
                // verify the test chunks themselfs
                &physical_mk.chunks[idx],
                idx,
                &physical_proof,
                physical_mk.get_root().to_string(),
            ) && *memory_proof == physical_proof
            // and check if the proofs for both valid and test chunks match up
            {
                // true condition to pass
                println!("chunk {} verified as valid", idx);
            }
            else {
                // otherwise, we need to take that index, and rewrite that chunk
                
            }
        }

        true
    }

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
        for (index, chunk) in self.file_chunks.iter().enumerate() {
            // create a chunk path based on the enumerated chunk index
            let path = self
                .chunk_filename_from_index(index)
                .expect("Failed to get chunk filename");
            // write that chunk with the formatted path
            fs::write(path, chunk).expect("msg");
        }
    }

    pub fn write_chunk(&self, index: usize) -> Result<(), std::io::Error> {
        // validate index bounds

        let chunk_path = self.chunk_filename_from_index(index)?;

        fs::write(chunk_path, &self.file_chunks[index])?;
        Ok(())
    }

    pub fn chunk_filename_from_index(&self, index: usize) -> Result<PathBuf, std::io::Error> {
        if index >= self.file_chunks.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Chunk index {} out of bounds", index),
            ));
        }
        // use path for better file name handling
        let file_path = Path::new(&self.file_name);
        let file_stem = file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");
        let file_ext = file_path.extension().and_then(|s| s.to_str()).unwrap_or("");

        // build chunk path using PathBuf

        let chunk_filename = if file_ext.is_empty() {
            format!("{}_{}", file_stem, index)
        } else {
            format!("{}_{}.{}", file_stem, index, file_ext)
        };
        let chunk_path = self.file_dir.join(chunk_filename);

        return Ok(chunk_path);
    }

    // pub fn verify_all(&self) {}
    pub fn get_hash(&self) -> String {
        sha256(&self.file_data)
    }
}
