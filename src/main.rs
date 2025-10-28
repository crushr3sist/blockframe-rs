use std::path::Path;

// use blockframe::chunker::Chunker;
use blockframe::{chunker::Chunker, filestore::FileStore};

/// Times the execution of a code block, printing the label and elapsed
/// duration before returning the block's result.
///
/// # Examples
///
/// ```
/// # use super::timeit;
/// let value = timeit!("addition", { 1 + 1 });
/// assert_eq!(value, 2);
/// ```
macro_rules! timeit {
    ($label:expr, $block:block) => {{
        let start = std::time::Instant::now();
        let result = { $block };
        println!("{} took: {:.5?}", $label, start.elapsed());
        result
    }};
}

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

    let example_file_path = Path::new("example.txt");
    let big_file_path = Path::new("big_file.txt");

    let store_path = Path::new("archive_directory");

    let store = FileStore::new(store_path)?;

    let chunker = Chunker::new()?;
    let _ = timeit!("example file", {
        let _ = chunker.commit(example_file_path)?;
    });

    let _ = timeit!("big file", {
        let _ = chunker.commit(big_file_path)?;
    });

    let _ = timeit!("time taken for soft read of files", {
        let files = store.as_hashmap()?;
        for file in files {
            for (file_name, file_data) in file {
                println!("file name: {:?}", file_name);
                println!(
                    "file hash: {:?}",
                    file_data.get("hash").ok_or_else(|| std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "error getting hash from file"
                    ))
                );
                println!(
                    "file path: {:?}",
                    file_data.get("path").ok_or_else(|| std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "error getting path from file"
                    ))
                );
                println!()
            }
        }
    });

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
