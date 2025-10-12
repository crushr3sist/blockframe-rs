use std::fs;

use blockframe::chunker::Chunker;

fn main() {
    // get our file_name
    let file_name = "example.txt".to_string();
    // get our bytes
    let file_bytes = fs::read(&file_name).expect("msg");

    let mut chunker = Chunker::new(file_name, file_bytes);
    let _ = match chunker.commit_all() {
        Ok(k) => println!("Successfully commited: {:?}", k),
        Err(e) => println!("error occured while commiting: {:?}", e),
    };
}
