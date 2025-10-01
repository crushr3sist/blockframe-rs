use std::fs;

use std::io;

fn read_file_to_bytes(path: &str) -> io::Result<Vec<u8>> {
    return fs::read(path);
}

fn read_file_to_text(path: &str) -> io::Result<String> {
    return fs::read_to_string(path);
}


fn main() {
    let file_path = "unnamed.jpg";
    
    match read_file_to_bytes(file_path){
        Ok(bytes) => println!("Read {} bytes from file.", bytes.len()),
        Err(e) => eprintln!("Error reading file: {}", e)
    }

    match read_file_to_text("example.txt") {
        Ok(data) => println!("Read '{}' text from file.", data),
        Err(e) => eprintln!("Error reading file: {}", e)
    }

    println!("Hello, world!");
}
