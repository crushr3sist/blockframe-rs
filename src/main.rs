use std::{fs, path::Path};

use blockframe::chunker::Chunker;

fn main() {
    // get our file_name
    let file_path = Path::new("big_file.txt");

    let mut chunker = Chunker::new(file_path);

    // if chunker.repair() {
    //     println!("repair successful!");
    // } else {
    //     println!("Repair failed - too many corrupted chunks");
    // }
}
