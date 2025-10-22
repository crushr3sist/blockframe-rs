use std::{fs, path::Path};

use blockframe::chunker::Chunker;
use blockframe::filestore;


fn main() {
    // get our file_name
    let file_path = Path::new("example.txt");
    let store_path = Path::new("archive_directory");
    
    // let chunker = Chunker::new().unwrap();
    // let file_being_chunked = chunker.commit(file_path).expect("msg");

    // if chunker.repair() {
    //     println!("repair successful!");
    // } else {
    //     println!("Repair failed - too many corrupted chunks");
    // }
}

// for now, lets work with a stateless object API
// we're going to expose these functions
// - aggregate all files commited
// - commit files
// - repair files
// - check health
