use std::path::Path;

// use blockframe::chunker::Chunker;
use blockframe::{chunker::Chunker, filestore::FileStore};

macro_rules! timeit {
    ($label:expr, $block:block) => {{
        let start = std::time::Instant::now();
        let result = { $block };
        println!("{} took: {:.5?}", $label, start.elapsed());
        result
    }};
}

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
