use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::filestore::models::{File, ManifestFile};

pub struct FileStore {
    pub store_path: PathBuf,
}

impl FileStore {
    pub fn new(store_path: &Path) -> Result<Self, std::io::Error> {
        Ok(FileStore {
            store_path: store_path.to_path_buf(),
        })
    }

    pub fn get_all(&self) -> Result<Vec<File>, Box<dyn std::error::Error>> {
        let mut file_list: Vec<File> = Vec::new();
        let all_dirs = fs::read_dir(&self.store_path);
        let manifests: Vec<PathBuf> = all_dirs
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok())
            .map(|f| f.path().join("manifest.json"))
            .collect();

        println!("manifest files {:?}", manifests);

        for path in manifests.iter() {
            let manifest: ManifestFile =
                ManifestFile::new(path.to_str().unwrap_or("").to_string())?;

            let file_entry = File::new(
                manifest.name,
                manifest.original_hash.to_string(),
                path.display().to_string(),
            )?;

            file_list.push(file_entry);
        }

        return Ok(file_list);
    }

    pub fn reconstruct_with_iter(&self, file_obj: File) -> Result<(), Box<dyn std::error::Error>> {
        // okay so we have a flat array of all of the chunks in order, we just need to append 1 by 1
        let reconstruct_path = Path::new("reconstructed");
        let file_name = file_obj.file_name.clone();
        let chunks = self.get_chunks(file_obj)?;
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

    pub fn get_chunks(&self, file_obj: File) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
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
    pub fn find(&self) {}
}

pub mod models;
