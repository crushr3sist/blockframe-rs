use std::path::Path;

// use blockframe::chunker::Chunker;
use blockframe::{chunker::Chunker, filestore::FileStore};

/// Commits two demonstration files, reports the elapsed time for each commit,
/// and prints a lightweight summary of the archived metadata.
///
/// # Examples
///
/// ```
/// # use std::path::Path;
/// # use blockframe::{chunker::Chunker, filestore::FileStore};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let sandbox = std::env::temp_dir().join(format!("blockframe_main_demo_{}", std::process::id()));
/// if sandbox.exists() {
///     std::fs::remove_dir_all(&sandbox)?;
/// }
/// std::fs::create_dir_all(&sandbox)?;
/// let original = std::env::current_dir()?;
/// std::env::set_current_dir(&sandbox)?;
/// std::fs::write("example.txt", b"example data")?;
/// std::fs::write("big_file.txt", b"big data")?;
/// let store_path = Path::new("archive_directory");
/// let store = FileStore::new(store_path)?;
/// let chunker = Chunker::new()?;
/// chunker.commit(Path::new("example.txt"))?;
/// chunker.commit(Path::new("big_file.txt"))?;
/// assert!(!store.as_hashmap()?.is_empty());
/// std::env::set_current_dir(&original)?;
/// std::fs::remove_dir_all(&sandbox)?;
/// # Ok(())
/// # }
/// ```
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // get our file_name

    // let example_file_path = Path::new("example.txt");
    // let big_file_path = Path::new("big_file.txt");

    let store_path = Path::new("archive_directory");

    let store = FileStore::new(store_path)?;

    // let chunker = Chunker::new()?;
    // let _ = chunker.commit(example_file_path)?;

    // let _ = chunker.commit(big_file_path)?;

    let files = store.get_all()?;
    for file in files {
        // println!("filename: {:?}", file.file_name);
        // println!("hash: {:?}", file.file_data.hash);
        // println!("path: {:?}", file.file_data.path);

        // println!("manifest: {:?}", file.manifest);
        // println!(
        //     "data-shards: {:?}",
        //     file.manifest.erasure_coding.data_shards
        // );
        for node in file.manifest.merkle_tree.leaves {
            println!("{:?}:{:?}\n", node.0, node.1);
        }
    }

    // let entry = store.find("big_file.txt");

    // if chunker.repair() {
    //     println!("repair successful!");
    // } else {
    //     println!("Repair failed - too many corrupted chunks");
    // }

    Ok(())
}

// for now, lets work with a stateless object API
// we're going to expose these functions
// - aggregate all files commited
// - commit files
// - repair files
// - check health
