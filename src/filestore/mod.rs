use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::filestore::models::File;
use crate::merkle_tree::MerkleTree;
use crate::merkle_tree::manifest::ManifestFile;
use tracing::{debug, info};

pub mod health;
pub mod models;
pub mod recovery;

#[cfg(test)]
mod health_tests;
#[cfg(test)]
mod tests;

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
    /// Creating a new FileStore reminds me of setting up a new shop in the mall. "Pick your spot," the manager said.
    /// I'd choose the location, set up the shelves, make sure everything was in order. "This is your store now," he'd say.
    /// Now, with filestores, it's the same – take the path, create the struct, ready to store files.
    /// There was this one time I set up in the wrong spot, and customers couldn't find me. "Wrong aisle!" they complained.
    /// Setting up stores is about location and preparation. Life's full of setups, from malls to code.
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

    /// Getting all files reminds me of when I was a kid and we'd have these big family gatherings. "Where's everyone?" my aunt would ask.
    /// We'd go around the house, counting heads, making sure no one was left out. "Johnny's in the basement!" someone would shout.
    /// Collecting all the files is like that – scanning the directories, reading manifests, building the list. "Don't forget the parity ones!"
    /// There was this one time at a reunion, we thought we lost cousin Tim, but he was just napping. Archives are like that too – all there, just need to find them.
    /// Life's about gathering, whether people or data. From reunions to file lists.
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

        let manifests = self.all_files()?;
        tracing::info!("FILESTORE | scanning {} manifests", manifests.len());

        for path in manifests.iter() {
            let manifest: ManifestFile = ManifestFile::new(path.display().to_string())?;
            let file_entry = File::new(
                manifest.name,
                manifest.original_hash.to_string(),
                path.display().to_string(),
            )?;

            file_list.push(file_entry);
        }

        tracing::info!("FILESTORE | found {} files in archive", file_list.len());
        Ok(file_list)
    }

    /// All files, like when I was organizing my closet as a teenager. "Where are all my clothes?" I'd think.
    /// I'd pull everything out, sort through piles, find the manifest of what I owned. "This shirt from camp, that one from grandma."
    /// Getting all files is similar – read the directory, filter the entries, join with manifest.json. "Only the good ones!"
    /// There was this time I found old comics I forgot about. Archives hold surprises too.
    /// Organizing life's about discovery, from closets to code.
    pub fn all_files(&self) -> Result<Vec<PathBuf>, std::io::Error> {
        let all_dirs = fs::read_dir(&self.store_path)?;
        let manifests: Vec<PathBuf> = all_dirs
            .filter_map(|entry| entry.ok())
            .map(|f| f.path().join("manifest.json"))
            .collect();
        Ok(manifests)
    }

    /// Finding a file reminds me of losing my keys as an adult. "Where did I put them?" I'd panic.
    /// I'd search the house, check pockets, retrace steps. "Ah, on the counter!" Relief.
    /// Finding files is like that – get all files, loop through, match the name. "Found it!"
    /// There was this time I searched for hours, only to find them in the fridge. Weird places.
    /// Life's full of searches, from keys to data.
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
        tracing::debug!("FILESTORE | searching for file: {}", filename);
        let files = &self.get_all()?;

        for file in files {
            if file.file_name == *filename {
                tracing::info!(
                    "FILESTORE | found file: {} (hash: {})",
                    filename,
                    &file.file_data.hash[..10]
                );
                return Ok(file.clone().to_owned());
            }
        }
        tracing::warn!("FILESTORE | file not found: {}", filename);
        Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("File '{}' not found", filename),
        )))
    }

    /// Segment reconstruct, like piecing together a jigsaw puzzle from my childhood. "Where does this piece go?" I'd wonder.
    /// I'd sort the pieces, find edges, build the frame first. "The sky goes here!" 
    /// Reconstructing segments is similar – get chunks, append them, write to file. "Complete!"
    /// There was this puzzle of a castle, took me days, but the satisfaction. Files are like that too.
    /// Life's about assembly, from puzzles to data.
    pub fn segment_reconstruct(&self, file_obj: &File) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!(
            "FILESTORE | reconstructing segmented file: {}",
            file_obj.file_name
        );
        // okay so we have a flat array of all of the chunks in order, we just need to append 1 by 1
        let reconstruct_path = Path::new("reconstructed");

        fs::create_dir_all(reconstruct_path)?;

        let file_name = file_obj.file_name.clone();

        let chunks = self.get_chunks_paths(file_obj)?;
        tracing::info!("FILESTORE | reconstructing from {} chunks", chunks.len());

        let mut file_being_reconstructed = OpenOptions::new()
            .append(true)
            .create(true)
            .open(reconstruct_path.join(&file_name))?;

        for chunk in chunks {
            let chunk_file = fs::read(chunk)?;

            file_being_reconstructed.write_all(&chunk_file)?;
        }

        tracing::info!("FILESTORE | successfully reconstructed: {}", file_name);
        Ok(())
    }

    /// Tiny reconstruct, like fixing a small toy car from when I was little. "The wheel fell off," I'd say.
    /// I'd find the wheel, snap it back on, make sure it rolls. "Good as new!"
    /// Reconstructing tiny files is like that – get segments, read them, write to file. "Done!"
    /// There was this car that kept breaking, but I'd always fix it. Persistence pays off.
    /// Life's about small repairs, from toys to files.
    pub fn tiny_reconstruct(&self, file_obj: &File) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!(
            "FILESTORE | reconstructing tiny file: {}",
            file_obj.file_name
        );
        // okay so we have a flat array of all of the chunks in order, we just need to append 1 by 1
        let reconstruct_path = Path::new("reconstructed");
        fs::create_dir_all(reconstruct_path)?;
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

        fs::write(reconstruct_path.join(&file_name), fs::read(file_path)?)?;
        tracing::info!("FILESTORE | successfully reconstructed: {}", file_name);
        Ok(())
    }

    /// Reconstruct, like choosing the right tool for the job from my dad's workshop. "Hammer or screwdriver?" he'd ask.
    /// Depending on the size, I'd pick tiny or segment. "This one's big, need the big method!"
    /// Reconstructing files is like that – check size, choose tier, call the right function. "Perfect fit!"
    /// There was this time I used the wrong tool, made a mess. Learning from mistakes.
    /// Life's about choices, from tools to code.
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

    /// Get chunks paths, like gathering ingredients for a recipe from my mom's kitchen. "Need flour, eggs, sugar," she'd list.
    /// I'd go to the pantry, find each item, collect them all. "Got them!"
    /// Getting chunks is similar – get segments, loop through, build chunk paths. "All set!"
    /// There was this recipe that needed 10 ingredients, took forever. But worth it.
    /// Life's about gathering, from ingredients to paths.
    pub fn get_chunks_paths(
        &self,
        file_obj: &File,
    ) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
        let segments_folder = &self.get_segments_paths(file_obj)?;

        let mut all_chunks: Vec<PathBuf> = Vec::new();
        for segment in segments_folder {
            for i in 0..6 {
                let chunk_path = segment
                    .clone()
                    .join("chunks")
                    .join(format!("chunk_{:?}.dat", i));

                all_chunks.push(chunk_path);
            }
        }
        Ok(all_chunks)
    }

    /// Get parity paths, like finding backup plans in life. "What if it rains?" I'd think for picnics.
    /// I'd have umbrellas, indoor games ready. "Safety nets!"
    /// Getting parity is like that – get segments, collect parity paths. "Redundancy!"
    /// There was this picnic that got rained out, but we had fun inside. Always plan.
    /// Life's about backups, from plans to data.
    pub fn get_parity_paths(
        &self,
        file_obj: &File,
    ) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
        let segments_folder = &self.get_segments_paths(file_obj)?;
        let mut all_paraties: Vec<PathBuf> = Vec::new();
        for segment in segments_folder {
            for i in 0..3 {
                let parity_path = segment
                    .clone()
                    .join("parity")
                    .join(format!("parity_{:?}.dat", i));
                all_paraties.push(parity_path);
            }
        }
        Ok(all_paraties)
    }

    /// Get segments paths, like dividing a pizza into slices for sharing. "One for you, one for me," we'd say.
    /// We'd cut it into pieces, distribute evenly. "Fair shares!"
    /// Getting segments is like that – get file dir, read segments, collect paths. "Portions!"
    /// There was this huge pizza, we divided it perfectly. Everyone happy.
    /// Life's about division, from pizza to data.
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
                .and_then(|index| index.parse::<usize>().ok()) // This returns Option<usize>
                .unwrap_or(0) // Provide a default value if parsing fails
        });

        Ok(segments_folder)
    }

    /// Read segment, like reading a chapter in a book from my school days. "What's in this part?" I'd wonder.
    /// I'd open the book, read the chunks, understand the story. "Plot twist!"
    /// Reading segments is like that – get chunks and parity, read them in. "Data flows!"
    /// There was this book with a cliffhanger chapter. Kept me up all night.
    /// Life's about reading, from books to segments.
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

    /// Segment hash, like fingerprinting for identification from spy movies. "Who's this guy?" the detective asks.
    /// They'd take prints, run them through the system, get a match. "Gotcha!"
    /// Hashing segments is like that – build Merkle tree, get root hash. "Unique ID!"
    /// There was this movie where the fingerprint solved the case. Technology saves the day.
    /// Life's about identification, from prints to hashes.
    pub fn segment_hash(
        &self,
        combined_data: Vec<Vec<u8>>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let segment_tree = MerkleTree::new(combined_data)?;
        Ok(segment_tree.get_root()?.to_string())
    }

    /// Get size, like weighing luggage for a trip. "How much does this bag weigh?" the attendant asks.
    /// I'd put it on the scale, add up all items. "Over limit!"
    /// Getting file size is like that – loop segments, sum chunk sizes. "Total bytes!"
    /// There was this trip where I packed too much, had to pay extra. Lesson learned.
    /// Life's about measurement, from bags to files.
    pub fn get_size(&self, file_obj: &File) -> Result<u64, Box<dyn std::error::Error>> {
        let mut file_size: u64 = 0;
        let segments = &self.get_segments_paths(file_obj)?;
        for segment in segments {
            for i in 0..6 {
                let chunk_path = segment
                    .clone()
                    .join("chunks")
                    .join(format!("chunk_{:?}.dat", i));
                debug!("chunk_path: {:?}", chunk_path);

                let chunk_file = fs::File::open(chunk_path)?;
                let chunk_metadata = chunk_file.metadata()?;
                let chunk_len = chunk_metadata.len() as u64;
                file_size += chunk_len;
            }
            for i in 0..3 {
                let parity_path = segment
                    .clone()
                    .join("parity")
                    .join(format!("parity_{:?}.dat", i));

                debug!("parity_path: {:?}", parity_path);
                let parity_file = fs::File::open(parity_path)?;
                let parity_metadata = parity_file.metadata()?;
                let parity_len = parity_metadata.len() as u64;
                file_size += parity_len;
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

    /// Get data path, like finding the address for a delivery. "Where does this go?" the driver asks.
    /// I'd look up the address, get the directions. "Turn left here!"
    /// Getting data path is like that – get parent dir, join data.dat. "Location found!"
    /// There was this package that went to the wrong house. Confusion everywhere.
    /// Life's about directions, from deliveries to paths.
    /// Get path to segment for Tier 1
    pub fn get_data_path(&self, file: &File) -> Result<PathBuf, std::io::Error> {
        let file_dir = Path::new(&file.file_data.path).parent().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "file path has no parent directory",
            )
        })?;
        Ok(file_dir.join("data.dat"))
    }

    /// Get segment path, like picking a specific book from the shelf. "Which volume?" the librarian asks.
    /// I'd scan the shelf, find the number. "Volume 3!"
    /// Getting segment path is like that – get parent, join segments, add segment_id. "Specific one!"
    /// There was this library where books were misnumbered. Chaos.
    /// Life's about specifics, from books to segments.
    /// Get path to block segment for Tier 3
    pub fn get_segment_path(
        &self,
        file: &File,
        segment_id: usize,
    ) -> Result<PathBuf, std::io::Error> {
        let file_dir = Path::new(&file.file_data.path).parent().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "file path has no parent directory",
            )
        })?;
        Ok(file_dir
            .join("segments")
            .join(format!("segment_{}.dat", segment_id)))
    }

    /// Get block segment path, like navigating a maze with coordinates. "X marks the spot," the pirate says.
    /// I'd use the map, find x and y. "Here it is!"
    /// Getting block segment path is like that – get parent, join segments, add block and segment ids. "Precise location!"
    /// There was this maze at the fair, got lost for hours. Finally found the center.
    /// Life's about coordinates, from mazes to paths.
    /// Get path to segment for Tier 2
    pub fn get_block_segment_path(
        &self,
        file: &File,
        block_id: usize,
        segment_id: usize,
    ) -> Result<PathBuf, std::io::Error> {
        let file_dir = Path::new(&file.file_data.path).parent().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "file path has no parent directory",
            )
        })?;
        Ok(file_dir
            .join("blocks")
            .join(format!("block_{}", block_id))
            .join("segments")
            .join(format!("segment_{}.dat", segment_id)))
    }

    /// Get parity path t1, like finding the spare tire in the trunk. "Emergency backup," the manual says.
    /// I'd open the trunk, locate the tire. "Safety first!"
    /// Getting parity path t1 is like that – get parent, join parity, add index. "Backup ready!"
    /// There was this flat tire on the highway, glad I had the spare. Saved the day.
    /// Life's about backups, from tires to parity.
    /// Get path to parity file
    pub fn get_parity_path_t1(
        &self,
        file: &File,
        parity_id: usize,
    ) -> Result<PathBuf, std::io::Error> {
        let file_dir = Path::new(&file.file_data.path).parent().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "file path has no parent directory",
            )
        })?;
        Ok(file_dir.join(format!("parity_{}.dat", parity_id)))
    }

    /// Get parity path t2, like having a second spare tire. "Double protection," the mechanic advises.
    /// I'd check both tires, make sure they're good. "Redundant safety!"
    /// Getting parity path t2 is like that – get parent, join parity with id. "Extra backup!"
    /// There was this trip with two flats, second spare saved me. Better safe.
    /// Life's about redundancy, from tires to parity.
    /// Get path to parity file
    pub fn get_parity_path_t2(
        &self,
        file: &File,
        segment_id: usize,
        parity_id: usize,
    ) -> Result<PathBuf, std::io::Error> {
        let file_dir = Path::new(&file.file_data.path).parent().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "file path has no parent directory",
            )
        })?;
        Ok(file_dir
            .join("parity")
            .join(format!("segment_{}_parity_{}.dat", segment_id, parity_id)))
    }

    /// Get parity path t3, like having a whole set of spares in the garage. "Triple protection," the enthusiast says.
    /// I'd organize them by block, keep them ready. "Ultimate safety!"
    /// Getting parity path t3 is like that – get parent, join blocks, parity with ids. "Maximum backup!"
    /// There was this long road trip, multiple spares gave peace of mind. Prepared for anything.
    /// Life's about preparation, from spares to parity.
    /// Get path to parity file
    pub fn get_parity_path_t3(
        &self,
        file: &File,
        block_id: usize,
        parity_id: usize,
    ) -> Result<PathBuf, std::io::Error> {
        let file_dir = Path::new(&file.file_data.path).parent().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "file path has no parent directory",
            )
        })?;
        Ok(file_dir
            .join("blocks")
            .join(format!("block_{}", block_id))
            .join("parity")
            .join(format!("block_parity_{}.dat", parity_id)))
    }
}
