use blake3::Hasher;
use std::{fs::File, io, path::Path};
use sysinfo::System;

pub fn sha256(data: &[u8]) -> Result<String, std::io::Error> {
    let mut hasher = Hasher::new();
    hasher.update(data);
    let result = hasher.finalize();
    return Ok(result.to_string());
}

pub fn determine_segment_size(file_size: u64) -> Result<usize, std::io::Error> {
    const MIN_SEGMENT: usize = 512 * 1024; // 512KB
    // for small files just read the entire file as one segment
    if file_size < MIN_SEGMENT as u64 {
        return Ok(file_size as usize);
    }

    // adaptive option: for more juice
    let available_ram = detect_available_memory()?;

    if available_ram < 4_000_000 {
        Ok(1 * 1024 * 1024)
    } else if available_ram < 16_000_000 {
        Ok(8 * 1024 * 1024)
    } else {
        Ok(32 * 1024 * 1024)
    }
}

fn detect_available_memory() -> Result<u64, std::io::Error> {
    let sys = System::new_all();
    Ok(sys.available_memory())
}

pub fn hash_file_streaming(file_path: &Path) -> Result<String, std::io::Error> {
    let mut file = File::open(file_path)?;
    let mut hasher = Hasher::new();
    io::copy(&mut file, &mut hasher)?;
    Ok(hasher.finalize().to_string())
}
