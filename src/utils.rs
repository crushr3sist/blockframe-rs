use blake3::Hasher;
use std::{fs::File, io, path::Path};
use sysinfo::System;

/// Computes the BLAKE3 digest of the provided bytes and returns it as a
/// hexadecimal string.
///
/// Although the function is named `sha256`, it currently uses the
/// [`blake3`](https://docs.rs/blake3) hasher under the hood to produce a
/// cryptographically secure hash.
///
/// # Examples
///
/// ```
/// use blockframe::utils::sha256;
///
/// # fn main() -> Result<(), std::io::Error> {
/// let digest = sha256(b"blockframe")?;
/// assert_eq!(digest, "c41e3ccb398783c24211ecea54ac84c2029d012165392c9deabbef3a597b8fb7");
/// # Ok(())
/// # }
/// ```
pub fn sha256(data: &[u8]) -> Result<String, std::io::Error> {
    let mut hasher = Hasher::new();
    hasher.update(data);
    let result = hasher.finalize();

    return Ok(result.to_string());
}

/// Determines the optimal segment size, in bytes, to use when reading a file
/// into memory.
///
/// The function inspects the input file size and the host's available memory
/// to choose a practical segment length that avoids overwhelming memory-constrained
/// systems.
///
/// # Examples
///
/// Small files are kept in a single segment:
///
/// ```
/// use blockframe::utils::determine_segment_size;
///
/// # fn main() -> Result<(), std::io::Error> {
/// let single_segment = determine_segment_size(1024)?;
/// assert_eq!(single_segment, 1024);
/// # Ok(())
/// # }
/// ```
pub fn determine_segment_size(file_size: u64) -> Result<usize, std::io::Error> {
    const MIN_SEGMENT: usize = 512 * 1024; // 512KB
    // for small files just read the entire file as one segment
    if file_size < MIN_SEGMENT as u64 {
        return Ok(file_size as usize);
    }

    // adaptive option: for more juice
    let available_ram = detect_available_memory()?;

    if available_ram < 4_000_000 {
        // 1mb
        Ok(1 * 1024 * 1024)
    } else if available_ram < 16_000_000 {
        // 8mb
        Ok(8 * 1024 * 1024)
    } else {
        // 32mb
        Ok(32 * 1024 * 1024)
    }
}

/// Returns the amount of free memory reported by the host operating system in
/// kibibytes.
///
/// # Examples
///
/// ```
/// let available = blockframe::utils::detect_available_memory().unwrap();
/// assert!(available > 0);
/// ```
pub fn detect_available_memory() -> Result<u64, std::io::Error> {
    let sys = System::new_all();
    Ok(sys.available_memory())
}

/// Calculates the BLAKE3 hash of a file by streaming its contents from disk.
///
/// The function avoids loading the entire file into memory at once, making it
/// suitable for hashing large files on systems with limited RAM.
///
/// # Examples
///
/// ```
/// use blockframe::utils::{hash_file_streaming, sha256};
/// use std::fs::File;
/// use std::io::Write;
///
///
/// # fn main() -> Result<(), std::io::Error> {
/// let file_path = std::env::temp_dir()
///     .join(format!("blockframe_hash_{}.txt", std::process::id()));
///
/// let mut file = File::create(&file_path)?;
/// writeln!(file, "hash me")?;
///
/// let digest = hash_file_streaming(&file_path)?;
/// let direct_digest = sha256(&std::fs::read(&file_path)?)?;
/// assert_eq!(digest, direct_digest);
///
/// std::fs::remove_file(file_path)?;
/// # Ok(())
/// # }
/// ```
pub fn hash_file_streaming(file_path: &Path) -> Result<String, std::io::Error> {
    let mut file = File::open(file_path)?;
    let mut hasher = Hasher::new();
    io::copy(&mut file, &mut hasher)?;
    Ok(hasher.finalize().to_string())
}
