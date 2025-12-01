use std::path::Path;

// use blockframe::chunker::Chunker;
use blockframe::{chunker::Chunker, filestore::FileStore};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    //SECTION - data init
    // let onegb = Path::new("files_to_commit/1gb.txt");
    // let two_gb = Path::new("files_to_commit/2gb.txt");
    // let six_gb = Path::new("files_to_commit/6gb.txt");
    // let ten_gb = Path::new("files_to_commit/11gb.txt");
    let shakespeare = Path::new("files_to_commit/shakes_peare.txt");
    let image = Path::new("files_to_commit/unnamed.jpg");
    let store_path = Path::new("archive_directory");

    //SECTION - chunk files
    let chunker = Chunker::new()?;
    // let _ = chunker.commit(onegb)?;
    // let _ = chunker.commit(two_gb)?;
    // let _ = chunker.commit(six_gb)?; // Profile only 6GB file
    // let _ = chunker.commit(ten_gb)?;
    // let _ = chunker.commit(shakespeare)?;
    // let _ = chunker.commit(image)?;

    let store = FileStore::new(store_path)?;
    let seg_file = store.find(&"1gb.txt".to_string())?;
    store.repair_segment(&seg_file)?;

    //SECTION - find function for some reason
    // can be for repairs or health check
    // make a file instance with hashes and chunk aggregator use merkle trees for repairs
    // let example_entry = store.find(&"example.txt".to_string())?;
    // store.repair_tiny(&example_entry)?;
    // let shakespeare_entry = store.find(&"shakes_peare.txt".to_string())?;
    // store.repair_tiny(&shakespeare_entry)?;
    // let image_entry = store.find(&"unnamed.jpg".to_string())?;
    // store.repair_tiny(&image_entry)?;
    // println!();
    // println!("big_file.txt full size: {:?}", store.get_size(&entry));
    // println!();

    //SECTION - file store
    // let files = store.get_all()?;

    // for file in files {
    //     store.reconstruct(&file)?;
    // }

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
