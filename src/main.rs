use std::{fs, path::Path};

use blockframe::chunker::Chunker;

fn main() {
    // get our file_name
    let file_path = Path::new("big_file.txt");

    let chunker = Chunker::new();
    let file_being_chunked = chunker.commit(file_path);




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
