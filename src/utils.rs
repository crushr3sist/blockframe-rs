use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io,
    path::Path,
};
use sysinfo::System;

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
pub fn determine_segment_size(file_size: u64) -> usize {
    const MIN_SEGMENT: usize = 512 * 1024; // 512KB
    // for small files just read the entire file as one segment
    if file_size < MIN_SEGMENT as u64 {
        return file_size as usize;
    }

    // adaptive option: for more juice
    let available_ram = detect_available_memory();

    if available_ram < 4_000_000 {
        1 * 1024 * 1024
    } else if available_ram < 16_000_000 {
        8 * 1024 * 1024
    } else {
        32 * 1024 * 1024
    }
}

fn detect_available_memory() -> u64 {
    let sys = System::new_all();
    sys.available_memory()
}

pub fn hash_file_streaming(file_path: &Path) -> Result<String, std::io::Error> {
    let mut file = File::open(file_path).expect("");
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut hasher).expect("");
    Ok(format!("{:x}", hasher.finalize()))
}
