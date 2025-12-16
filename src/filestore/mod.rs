use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::filestore::models::File;
use crate::merkle_tree::MerkleTree;
use crate::merkle_tree::manifest::ManifestFile;

/// FileStore manages the archive directory and provides access to stored files.
///
/// This is the main interface for interacting with archived files, including:
/// - Listing all files in the archive
/// - Finding specific files by name
/// - Reconstructing original files from erasure-coded shards
/// - Health checking and repair operations
pub struct FileStore {
    pub store_path: PathBuf,
}

impl FileStore {
    /// Creates a new FileStore instance pointing to an archive directory.
    ///
    /// # Parameters
    ///
    /// * `store_path` - Path to the archive directory (e.g., `archive_directory/`)
    ///
    /// # Returns
    ///
    /// * `Ok(FileStore)` - Ready-to-use store instance
    /// * `Err` - If path conversion fails (rare)
    ///
    /// # Example
    ///
    /// ```no_run
    /// use blockframe::filestore::FileStore;
    /// use std::path::Path;
    ///
    /// let store = FileStore::new(Path::new("archive_directory"))?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn new(store_path: &Path) -> Result<Self, std::io::Error> {
        Ok(FileStore {
            store_path: store_path.to_path_buf(),
        })
    }

    /// Retrieves a list of all files in the archive.
    ///
    /// This function scans all subdirectories in the archive, reads each `manifest.json`,
    /// and constructs a `File` object for each archived file.
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<File>)` - List of all files with metadata (name, hash, path)
    /// * `Err` - If directory read fails or manifest parsing fails
    ///
    /// # Performance
    ///
    /// - Reads all manifest.json files in the archive
    /// - For large archives (1000+ files), consider caching this result
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use blockframe::filestore::FileStore;
    /// # use std::path::Path;
    /// # let store = FileStore::new(Path::new("archive_directory"))?;
    /// let all_files = store.get_all()?;
    /// println!("Archive contains {} files", all_files.len());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn get_all(&self) -> Result<Vec<File>, Box<dyn std::error::Error>> {
        let mut file_list: Vec<File> = Vec::new();

        let manifests = &self.all_files();
        for path in manifests.iter() {
            let manifest: ManifestFile = ManifestFile::new(path.display().to_string())?;
            let file_entry = File::new(
                manifest.name,
                manifest.original_hash.to_string(),
                path.display().to_string(),
            )?;

            file_list.push(file_entry);
        }

        return Ok(file_list);
    }

    pub fn all_files(&self) -> Vec<PathBuf> {
        let all_dirs = fs::read_dir(&self.store_path);
        let manifests: Vec<PathBuf> = all_dirs
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok())
            .map(|f| f.path().join("manifest.json"))
            .collect();
        return manifests;
    }

    /// Finds a specific file in the archive by its original filename.
    ///
    /// This function searches through all archived files and returns the first
    /// match with the specified filename.
    ///
    /// # Parameters
    ///
    /// * `filename` - The original filename (e.g., `"example.txt"`)
    ///
    /// # Returns
    ///
    /// * `Ok(File)` - Metadata for the found file
    /// * `Err(NotFound)` - If no file with that name exists in the archive
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use blockframe::filestore::FileStore;
    /// # use std::path::Path;
    /// # let store = FileStore::new(Path::new("archive_directory"))?;
    /// let file = store.find(&"myfile.txt".to_string())?;
    /// println!("Found: {}", file.file_name);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn find(&self, filename: &String) -> Result<File, Box<dyn std::error::Error>> {
        let files = &self.get_all()?;

        for file in files {
            if file.file_name == *filename {
                return Ok(file.clone().to_owned());
            }
        }
        Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("File '{}' not found", filename),
        )))
    }

    pub fn segment_reconstruct(&self, file_obj: &File) -> Result<(), Box<dyn std::error::Error>> {
        // okay so we have a flat array of all of the chunks in order, we just need to append 1 by 1
        let reconstruct_path = Path::new("reconstructed");

        fs::create_dir_all(&reconstruct_path)?;

        let file_name = file_obj.file_name.clone();

        let chunks = self.get_chunks_paths(file_obj)?;

        let mut file_being_reconstructed = OpenOptions::new()
            .append(true)
            .create(true)
            .open(reconstruct_path.join(file_name))?;

        for chunk in chunks {
            let chunk_file = fs::read(chunk)?;

            file_being_reconstructed.write_all(&chunk_file)?;
        }

        Ok(())
    }

    pub fn tiny_reconstruct(&self, file_obj: &File) -> Result<(), Box<dyn std::error::Error>> {
        // okay so we have a flat array of all of the chunks in order, we just need to append 1 by 1
        let reconstruct_path = Path::new("reconstructed");
        fs::create_dir_all(&reconstruct_path)?;
        let file_name = file_obj.file_name.clone();

        let file_path = Path::new(&file_obj.file_data.path)
            .parent()
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Could not get parent directory",
                )
            })?
            .join("data.dat");

        fs::write(reconstruct_path.join(file_name), fs::read(file_path)?)?;
        Ok(())
    }

    pub fn reconstruct(&self, file_obj: &File) -> Result<(), Box<dyn std::error::Error>> {
        let tier: u8 = match file_obj.manifest.size {
            0..=10_000_000 => 1,
            _ => 2,
        };

        match tier {
            1 => self.tiny_reconstruct(file_obj)?,
            _ => self.segment_reconstruct(file_obj)?,
        };

        Ok(())
    }

    pub fn get_chunks_paths(
        &self,
        file_obj: &File,
    ) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
        let segments_folder = &self.get_segments_paths(file_obj)?;

        let mut all_chunks: Vec<PathBuf> = Vec::new();
        for segment in segments_folder {
            for i in 0..6 {
                let chunk_path = PathBuf::from(
                    segment
                        .clone()
                        .join("chunks")
                        .join(format!("chunk_{:?}.dat", i)),
                );
                all_chunks.push(chunk_path);
            }
        }
        Ok(all_chunks)
    }

    pub fn get_parity_paths(
        &self,
        file_obj: &File,
    ) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
        let segments_folder = &self.get_segments_paths(file_obj)?;
        let mut all_paraties: Vec<PathBuf> = Vec::new();
        for segment in segments_folder {
            for i in 0..3 {
                let parity_path = PathBuf::from(
                    segment
                        .clone()
                        .join("parity")
                        .join(format!("parity_{:?}.dat", i)),
                );
                all_paraties.push(parity_path);
            }
        }
        Ok(all_paraties)
    }

    pub fn get_segments_paths(
        &self,
        file_obj: &File,
    ) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
        let file_dir: PathBuf = Path::new(&file_obj.file_data.path)
            .parent()
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Could not get parent directory",
                )
            })?
            .to_path_buf();
        let file_dir = file_dir.join("segments");

        let mut segments_folder: Vec<PathBuf> = fs::read_dir(file_dir)?
            .filter_map(|entry| entry.ok())
            .map(|f| f.path())
            .collect();

        segments_folder.sort_by_key(|path| {
            path.file_stem()
                .and_then(|folder| folder.to_str())
                .and_then(|folder| folder.split("_").last())
                .and_then(|index| index.parse::<usize>().ok())
                .unwrap_or(0)
        });

        Ok(segments_folder)
    }
    pub fn read_segment(&self, path: PathBuf) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
        // gather all the chunks from the path
        // and gather all of the
        let mut chunk_data: Vec<Vec<u8>> = Vec::new();
        let mut parity_data: Vec<Vec<u8>> = Vec::new();
        let chunk_path = path.join("chunks");
        let parity_path = path.join("parity");
        for idx in 0..6 {
            chunk_data.push(fs::read(chunk_path.join(format!("chunk_{idx}.dat")))?);
        }

        for idx in 0..3 {
            parity_data.push(fs::read(parity_path.join(format!("parity_{idx}.dat")))?);
        }

        let combined: Vec<Vec<u8>> = chunk_data
            .iter()
            .chain(parity_data.iter())
            .cloned()
            .collect();
        Ok(combined)
    }

    pub fn segment_hash(
        &self,
        combined_data: Vec<Vec<u8>>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let segment_tree = MerkleTree::new(combined_data)?;
        Ok(segment_tree.get_root()?.to_string())
    }

    pub fn get_size(&self, file_obj: &File) -> Result<u64, Box<dyn std::error::Error>> {
        let mut file_size: u64 = 0;
        let segments = &self.get_segments_paths(file_obj)?;
        for segment in segments {
            for i in 0..6 {
                let chunk_path = PathBuf::from(
                    segment
                        .clone()
                        .join("chunks")
                        .join(format!("chunk_{:?}.dat", i)),
                );
                println!("chunk_path: {:?}", chunk_path);

                file_size = file_size + fs::File::open(chunk_path)?.metadata()?.len() as u64;
            }
            for i in 0..3 {
                let parity_path = PathBuf::from(
                    segment
                        .clone()
                        .join("parity")
                        .join(format!("parity_{:?}.dat", i)),
                );
                println!("parity_path: {:?}", parity_path);
                file_size = file_size + fs::File::open(parity_path)?.metadata()?.len() as u64;
            }
        }

        Ok(file_size)
    }
    fn hash_segment_with_parity(
        &self,
        segment_data: &[u8],
        parity: &[Vec<u8>],
    ) -> Result<String, std::io::Error> {
        let combined: Vec<Vec<u8>> = std::iter::once(segment_data.to_vec())
            .chain(parity.iter().cloned())
            .collect();
        let segment_tree = MerkleTree::new(combined)?;

        Ok(segment_tree.get_root()?.to_string())
    }

    /// Get path to segment for Tier 1
    pub fn get_data_path(&self, file: &File) -> PathBuf {
        let file_dir = Path::new(&file.file_data.path).parent().unwrap();
        file_dir.join("data.dat")
    }

    /// Get path to block segment for Tier 3
    pub fn get_segment_path(&self, file: &File, segment_id: usize) -> PathBuf {
        let file_dir = Path::new(&file.file_data.path).parent().unwrap();
        file_dir
            .join("segments")
            .join(format!("segment_{}.dat", segment_id))
    }

    /// Get path to segment for Tier 2
    pub fn get_block_segment_path(
        &self,
        file: &File,
        block_id: usize,
        segment_id: usize,
    ) -> PathBuf {
        let file_dir = Path::new(&file.file_data.path).parent().unwrap();
        file_dir
            .join("blocks")
            .join(format!("block_{}", block_id))
            .join("segments")
            .join(format!("segment_{}.dat", segment_id))
    }

    /// Get path to parity file
    pub fn get_parity_path_t1(&self, file: &File, parity_id: usize) -> PathBuf {
        let file_dir = Path::new(&file.file_data.path).parent().unwrap();
        file_dir.join(format!("parity_{}.dat", parity_id))
    }

    /// Get path to parity file
    pub fn get_parity_path_t2(&self, file: &File, segment_id: usize, parity_id: usize) -> PathBuf {
        let file_dir = Path::new(&file.file_data.path).parent().unwrap();
        file_dir
            .join("parity")
            .join(format!("segment_{}_parity_{}.dat", segment_id, parity_id))
    }

    /// Get path to parity file
    pub fn get_parity_path_t3(&self, file: &File, block_id: usize, parity_id: usize) -> PathBuf {
        let file_dir = Path::new(&file.file_data.path).parent().unwrap();
        file_dir
            .join("blocks")
            .join(format!("block_{}", block_id))
            .join("parity")
            .join(format!("block_parity_{}.dat", parity_id))
    }
}

pub mod health;
pub mod models;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod health_tests;
