use std::path::Path;

// use blockframe::chunker::Chunker;
use blockframe::{chunker::Chunker, filestore::FileStore};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    //SECTION - data init
    // let big_file_path = Path::new("big_file.txt");
    let example_file_path = Path::new("example.txt");
    let shakespeare = Path::new("shakes_peare.txt");
    let image = Path::new("unnamed.jpg");
    let store_path = Path::new("archive_directory");

    //SECTION - chunk files
    let chunker = Chunker::new()?;
    let _ = chunker.commit(example_file_path)?;
    // let _ = chunker.commit(big_file_path)?;
    let _ = chunker.commit(shakespeare)?;
    let _ = chunker.commit(image)?;

    let store = FileStore::new(store_path)?;
    //SECTION - find function for some reason
    // can be for repairs or health check
    // make a file instance with hashes and chunk aggregator use merkle trees for repairs
    let example_entry = store.find(&"example.txt".to_string())?;
    store.repair_tiny(&example_entry)?;

    let shakespeare_entry = store.find(&"shakes_peare.txt".to_string())?;
    store.repair_tiny(&shakespeare_entry)?;

    let image_entry = store.find(&"unnamed.jpg".to_string())?;

    store.repair_tiny(&image_entry)?;
    // println!();
    // println!("big_file.txt full size: {:?}", store.get_size(&entry));
    // println!();

    //SECTION - file store
    let files = store.get_all()?;

    for file in files {
        store.reconstruct(&file)?;
    }

    //SECTION - repair functions

    // if store.should_repair(&entry)? {
    //     println!("repair needed!");
    //     store.repair(&entry)?;
    // } else {
    //     println!("repair not needed");
    // }

    Ok(())
}

// for now, lets work with a stateless object API
// we're going to expose these functions
// - aggregate all files commited
// - commit files
// - repair files
// - check health
