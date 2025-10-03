use sha2::{Digest, Sha256};
use std::fs;

pub fn read_file_to_bytes(path: &str) -> Vec<u8> {
    fs::read(path).expect("failed to read file")
}

pub fn dummy_data() -> Vec<Vec<u8>> {
    let file = read_file_to_bytes("example.txt");
    let chunk_size = file.len() / 6;

    (0..file.len())
        .step_by(chunk_size)
        .map(|i| file[i..(i + chunk_size).min(file.len())].to_vec())
        .collect()
}

pub fn sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    return format!("{:x}", result);
}
