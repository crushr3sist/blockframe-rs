use std::path::Path;

// use blockframe::chunker::Chunker;
use blockframe::filestore::FileStore;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // get our file_name
    // let file_path = Path::new("example.txt");
    let store_path = Path::new("archive_directory");

    let store = FileStore::new(store_path)?;

    let files = store.getall()?;

    println!("{:?}", files);

    // let chunker = Chunker::new().unwrap();
    // let file_being_chunked = chunker.commit(file_path).expect("msg");

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
