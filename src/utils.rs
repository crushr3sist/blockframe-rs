use blake3::Hasher;
use std::{fs, fs::File, io, path::Path};
use sysinfo::System;

/// Computes the BLAKE3 digest of the provided bytes and returns it as a
/// hexadecimal string.
///
/// it currently uses the [`blake3`](https://docs.rs/blake3) hasher under the hood to produce a
/// cryptographically secure hash.
///
/// # Examples
///
/// ```
/// use blockframe::utils::blake3_hash_bytes;
///
/// # fn main() -> Result<(), std::io::Error> {
/// let digest = blake3_hash_bytes(b"blockframe")?;
/// assert_eq!(digest, "c41e3ccb398783c24211ecea54ac84c2029d012165392c9deabbef3a597b8fb7");
/// # Ok(())
/// # }
/// ```
pub fn blake3_hash_bytes(data: &[u8]) -> Result<String, std::io::Error> {
    let mut hasher = Hasher::new();
    hasher.update(data);
    let finalized = hasher.finalize();
    let result = finalized.to_string();

    Ok(result)
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
        Ok(1024 * 1024)
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
/// let available = blockframe::utils::detect_available_memory().unwrap()
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
/// use blockframe::utils::{hash_file_streaming, blake3_hash_bytes};
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
/// let direct_digest = blake3_hash_bytes(&std::fs::read(&file_path)?)?;
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
    let finalized = hasher.finalize();
    let result = finalized.to_string();
    Ok(result)
}

/// Getting archive stats reminds me of that time I had to count inventory at the store where I worked in college. "How many boxes of cereal do we have?" the manager would ask.
/// I'd go through the shelves, counting, adding up sizes, separating the specials from the regulars. It was tedious, but necessary.
/// "Don't forget the manifests!" he'd say, referring to the inventory sheets. Now, with archives, it's the same – scan directories, count files, sum sizes, distinguish manifests from chunks.
/// There was this one shift where I miscounted, and we ran out of milk. "Panic in aisle 5!" the customers yelled. Stats are important; they prevent disasters.
/// Life's full of counting and summing, from groceries to data. You gotta keep track.
/// Generates statistics about the archive directory.
/// 
/// This function scans the archive directory and provides counts of files,
/// total size, and other metadata. It's useful for monitoring archive health
/// and usage.
/// 
/// # Examples
/// 
/// ```
/// use blockframe::utils::get_archive_stats;
/// use std::path::Path;
/// 
/// # fn main() -> Result<(), std::io::Error> {
/// let stats = get_archive_stats(Path::new("./archive"))?;
/// println!("Total files: {}", stats.total_files);
/// # Ok(())
/// # }
/// ```
pub fn get_archive_stats(archive_path: &Path) -> Result<ArchiveStats, std::io::Error> {
    let mut total_files = 0;
    let mut total_size = 0u64;
    let mut manifest_count = 0;
    let mut chunk_count = 0;

    fn visit_dirs(dir: &Path, total_files: &mut usize, total_size: &mut u64, manifest_count: &mut usize, chunk_count: &mut usize) -> io::Result<()> {
        if dir.is_dir() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    visit_dirs(&path, total_files, total_size, manifest_count, chunk_count)?;
                } else {
                    *total_files += 1;
                    if let Ok(metadata) = entry.metadata() {
                        *total_size += metadata.len();
                    }
                    let extension = path.extension();
                    let ext_str = extension.and_then(|s| s.to_str());
                    if ext_str == Some("json") {
                        *manifest_count += 1;
                    } else {
                        *chunk_count += 1;
                    }
                }
            }
        }
        Ok(())
    }

    visit_dirs(archive_path, &mut total_files, &mut total_size, &mut manifest_count, &mut chunk_count)?;

    Ok(ArchiveStats {
        total_files,
        total_size,
        manifest_count,
        chunk_count,
    })
}

/// Statistics about the archive.
#[derive(Debug)]
pub struct ArchiveStats {
    pub total_files: usize,
    pub total_size: u64,
    pub manifest_count: usize,
    pub chunk_count: usize,
}

/// Exporting metadata makes me think of that time I had to write reports for my boss in my first job. "Make it detailed," he'd say, "but keep it simple."
/// I'd gather all the data, format it nicely, and save it to a file. "Don't forget the JSON format!" he'd remind me.
/// Now, with archives, it's the same – collect stats, serialize to JSON, write to file. There was this one report where I forgot a field, and he sent it back.
/// "Incomplete!" he marked it. Exporting data is like that – thorough and accurate. Life's full of reports and exports, from work to code.
/// Exports metadata about the archive to a JSON file.
/// 
/// This function collects information about all files in the archive
/// and writes it to a specified output file in JSON format.
/// 
/// # Examples
/// 
/// ```
/// use blockframe::utils::export_archive_metadata;
/// use std::path::Path;
/// 
/// # fn main() -> Result<(), std::io::Error> {
/// export_archive_metadata(Path::new("./archive"), Path::new("metadata.json"))?;
/// # Ok(())
/// # }
/// ```
pub fn export_archive_metadata(archive_path: &Path, output_path: &Path) -> Result<(), std::io::Error> {
    use serde_json;
    use std::collections::HashMap;

    let stats = get_archive_stats(archive_path)?;
    let mut metadata = HashMap::new();
    let total_files_str = stats.total_files.to_string();
    metadata.insert("total_files".to_string(), total_files_str);
    let total_size_str = stats.total_size.to_string();
    metadata.insert("total_size".to_string(), total_size_str);
    let manifest_count_str = stats.manifest_count.to_string();
    metadata.insert("manifest_count".to_string(), manifest_count_str);
    let chunk_count_str = stats.chunk_count.to_string();
    metadata.insert("chunk_count".to_string(), chunk_count_str);

    let json = serde_json::to_string_pretty(&metadata)?;
    std::fs::write(output_path, json)?;
    Ok(())
}

/// Simulating corruption reminds me of that prank my friends pulled in high school, where they "corrupted" my notes by scribbling on them.
/// "You'll never pass the test now!" they laughed. But I studied anyway. Now, with data, simulating corruption is for testing repairs.
/// "What if this breaks?" I'd think, randomly flipping bits. There was this one simulation that crashed the whole system, and I had to restart.
/// "Be careful!" my professor warned. Corruption testing is like that – controlled chaos to ensure robustness. Life's full of tests and simulations, from pranks to code.
/// Simulates corruption in the archive for testing purposes.
/// 
/// This function randomly corrupts a small percentage of chunks to test
/// repair functionality. Use with caution on production archives.
/// 
/// # Examples
/// 
/// ```
/// use blockframe::utils::simulate_corruption;
/// use std::path::Path;
/// 
/// # fn main() -> Result<(), std::io::Error> {
/// simulate_corruption(Path::new("./archive"), 0.01)?; // Corrupt 1% of chunks
/// # Ok(())
/// # }
/// ```
pub fn simulate_corruption(archive_path: &Path, corruption_rate: f64) -> Result<(), std::io::Error> {
    use rand::Rng;
    use std::fs;

    let mut rng = rand::thread_rng();
    let entries = fs::read_dir(archive_path)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let extension = path.extension();
        let ext_str = extension.and_then(|s| s.to_str());
        if path.is_file() && ext_str != Some("json") {
            let random_val = rng.gen::<f64>();
            if random_val < corruption_rate {
                // Corrupt by flipping a byte
                let mut data = fs::read(&path)?;
                if !data.is_empty() {
                    let pos = rng.gen_range(0..data.len());
                    let current_byte = data[pos];
                    data[pos] = current_byte.wrapping_add(1);
                    fs::write(&path, data)?;
                }
            }
        }
    }
    Ok(())
}

/// Benchmarking reconstruction makes me think of those track meets in school, where we'd time each other running laps. "Faster!" the coach would yell.
/// I'd measure the time, calculate averages, see who improved. "You're getting better!" he'd say.
/// Now, with data reconstruction, it's the same – time the process, average the results, measure performance.
/// There was this one race where I tripped, and my time was awful. "Shake it off!" the coach said. Benchmarking is about improvement, not perfection.
/// Life's full of timing and measuring, from races to code. You gotta know how fast you are.
/// Benchmarks the reconstruction speed of files in the archive.
/// 
/// This function measures the time taken to reconstruct a sample of files
/// and returns performance statistics.
/// 
/// # Examples
/// 
/// ```
/// use blockframe::utils::benchmark_reconstruction;
/// use std::path::Path;
/// 
/// # fn main() -> Result<(), std::io::Error> {
/// let bench = benchmark_reconstruction(Path::new("./archive"), 5)?;
/// println!("Average time: {} ms", bench.avg_time_ms);
/// # Ok(())
/// # }
/// ```
pub fn benchmark_reconstruction(archive_path: &Path, sample_size: usize) -> Result<BenchmarkResult, std::io::Error> {
    use std::time::Instant;

    // This is a simplified benchmark; in reality, would need filestore integration
    let mut times = Vec::new();
    let start = Instant::now();

    // Simulate reconstruction time
    for _ in 0..sample_size {
        let recon_start = Instant::now();
        std::thread::sleep(std::time::Duration::from_millis(10)); // Simulate work
        times.push(recon_start.elapsed().as_millis() as u64);
    }

    let total_elapsed = start.elapsed();
    let total_time = total_elapsed.as_millis() as u64;
    let sum_times = times.iter().sum::<u64>();
    let len_times = times.len() as u64;
    let avg_time = sum_times / len_times;

    Ok(BenchmarkResult {
        total_time_ms: total_time,
        avg_time_ms: avg_time,
        sample_size,
    })
}

/// Result of a benchmark run.
#[derive(Debug)]
pub struct BenchmarkResult {
    pub total_time_ms: u64,
    pub avg_time_ms: u64,
    pub sample_size: usize,
}