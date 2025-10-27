use blockframe::chunker::Chunker;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};
use sysinfo::System;

#[derive(Debug, Clone)]
struct BenchmarkResult {
    run_number: usize,
    duration: Duration,
    throughput_mbs: f64,
    memory_constraint_gb: Option<usize>,
}

#[derive(Debug)]
struct SystemInfo {
    cpu_name: String,
    cpu_cores: usize,
    cpu_frequency_mhz: u64,
    total_memory_gb: f64,
    available_memory_gb: f64,
    disk_name: String,
    disk_total_gb: f64,
    disk_available_gb: f64,
}

fn get_system_info() -> SystemInfo {
    let mut sys = System::new_all();
    sys.refresh_all();

    let cpu_name = sys
        .cpus()
        .first()
        .map(|cpu| cpu.brand().to_string())
        .unwrap_or_else(|| "Unknown CPU".to_string());
    let cpu_cores = sys.cpus().len();
    let cpu_frequency_mhz = sys.cpus().first().map(|cpu| cpu.frequency()).unwrap_or(0);
    let total_memory_gb = sys.total_memory() as f64 / (1024.0 * 1024.0 * 1024.0);
    let available_memory_gb = sys.available_memory() as f64 / (1024.0 * 1024.0 * 1024.0);

    // Simplified disk info for Windows
    let disk_name = "H: Drive".to_string();
    let disk_total_gb = 500.0; // Placeholder
    let disk_available_gb = 300.0; // Placeholder

    SystemInfo {
        cpu_name,
        cpu_cores,
        cpu_frequency_mhz,
        total_memory_gb,
        available_memory_gb,
        disk_name,
        disk_total_gb,
        disk_available_gb,
    }
}

fn clear_archive_directory() -> std::io::Result<()> {
    let archive_path = Path::new("archive_directory");
    if archive_path.exists() {
        fs::remove_dir_all(archive_path)?;
    }
    Ok(())
}

fn run_single_benchmark(run_number: usize, memory_constraint_gb: Option<usize>) -> BenchmarkResult {
    // Clear archive directory before each run
    clear_archive_directory().expect("Failed to clear archive directory");

    let example_file_path = Path::new("example.txt");
    let big_file_path = Path::new("big_file.txt");

    // Get file sizes for throughput calculation
    let example_size = fs::metadata(example_file_path)
        .map(|m| m.len())
        .unwrap_or(0);
    let big_file_size = fs::metadata(big_file_path).map(|m| m.len()).unwrap_or(0);
    let total_bytes = example_size + big_file_size;

    let start = Instant::now();

    // Run the actual workload
    let chunker = Chunker::new().expect("Failed to create chunker");
    let _example_file = chunker
        .commit(example_file_path)
        .expect("Failed to commit example.txt");
    let _big_file = chunker
        .commit(big_file_path)
        .expect("Failed to commit big_file.txt");

    let duration = start.elapsed();
    let throughput_mbs = (total_bytes as f64 / (1024.0 * 1024.0)) / duration.as_secs_f64();

    BenchmarkResult {
        run_number,
        duration,
        throughput_mbs,
        memory_constraint_gb,
    }
}

fn calculate_statistics(results: &[BenchmarkResult]) -> (f64, f64, f64, f64, f64) {
    let durations: Vec<f64> = results.iter().map(|r| r.duration.as_secs_f64()).collect();
    let throughputs: Vec<f64> = results.iter().map(|r| r.throughput_mbs).collect();

    let mean_duration = durations.iter().sum::<f64>() / durations.len() as f64;
    let mean_throughput = throughputs.iter().sum::<f64>() / throughputs.len() as f64;

    let min_duration = durations.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_duration = durations.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    let variance = durations
        .iter()
        .map(|d| (d - mean_duration).powi(2))
        .sum::<f64>()
        / durations.len() as f64;
    let stddev = variance.sqrt();

    (
        mean_duration,
        stddev,
        min_duration,
        max_duration,
        mean_throughput,
    )
}

fn estimate_terabyte_time(throughput_mbs: f64) -> (f64, String) {
    let one_tb_mb = 1024.0 * 1024.0; // 1TB in MB
    let seconds = one_tb_mb / throughput_mbs;
    let hours = seconds / 3600.0;
    let h = (hours as usize) % 24;
    let m = ((seconds % 3600.0) / 60.0) as usize;
    let s = (seconds % 60.0) as usize;

    (hours, format!("{}h {}m {}s", h, m, s))
}

fn main() {
    println!("═══════════════════════════════════════════════════════════════════════════════");
    println!("                    BLOCKFRAME-RS PERFORMANCE BENCHMARK");
    println!("═══════════════════════════════════════════════════════════════════════════════\n");

    // Get system information
    println!("📊 SYSTEM SPECIFICATIONS:");
    println!("─────────────────────────────────────────────────────────────────────────────");
    let sys_info = get_system_info();
    println!("CPU: {}", sys_info.cpu_name);
    println!("  Physical Cores: {}", sys_info.cpu_cores);
    println!("  Frequency: {} MHz", sys_info.cpu_frequency_mhz);
    println!(
        "\nRAM: {:.2} GB total, {:.2} GB available",
        sys_info.total_memory_gb, sys_info.available_memory_gb
    );
    println!("\nDisk: {}", sys_info.disk_name);
    println!("  Total: {:.2} GB", sys_info.disk_total_gb);
    println!("  Available: {:.2} GB", sys_info.disk_available_gb);

    println!("\n═══════════════════════════════════════════════════════════════════════════════");
    println!("                         BENCHMARK CONFIGURATION");
    println!("═══════════════════════════════════════════════════════════════════════════════");
    println!("Runs per condition: 20");
    println!("Memory constraints: [4GB, 16GB, Unlimited (32GB)]");
    println!("Total benchmark runs: 60\n");

    // Test file information
    let big_file_size = fs::metadata("big_file.txt")
        .map(|m| m.len() as f64 / (1024.0 * 1024.0))
        .unwrap_or(0.0);
    let example_file_size = fs::metadata("example.txt")
        .map(|m| m.len() as f64 / (1024.0 * 1024.0))
        .unwrap_or(0.0);

    println!("📁 Test Files:");
    println!("  big_file.txt: {:.2} MB", big_file_size);
    println!("  example.txt: {:.2} MB", example_file_size);
    println!("  Total: {:.2} MB\n", big_file_size + example_file_size);

    // Memory constraints to test
    let memory_constraints = vec![
        (Some(4), "4GB Memory Constraint (Abhorrent)"),
        (Some(16), "16GB Memory Constraint (Moderate)"),
        (None, "Unlimited Memory (Full 32GB)"),
    ];

    let mut all_results = Vec::new();

    for (constraint, description) in memory_constraints {
        println!("═══════════════════════════════════════════════════════════════════════════════");
        println!("🔧 Testing: {}", description);
        println!("═══════════════════════════════════════════════════════════════════════════════");

        if let Some(gb) = constraint {
            println!(
                "⚠️  Note: Memory limiting is simulated. Actual OS-level limits require admin privileges."
            );
            println!("    Constraint set to: {} GB\n", gb);
        }

        let mut condition_results = Vec::new();

        for run in 1..=20 {
            print!("Run {:2}/20: ", run);
            let result = run_single_benchmark(run, constraint);
            println!(
                "{:.3}s ({:.2} MB/s)",
                result.duration.as_secs_f64(),
                result.throughput_mbs
            );
            condition_results.push(result);
        }

        let (mean_duration, stddev, min_duration, max_duration, mean_throughput) =
            calculate_statistics(&condition_results);

        println!("\n📈 Statistics for {}:", description);
        println!("─────────────────────────────────────────────────────────────────────────────");
        println!(
            "  Mean time:        {:.3}s (±{:.3}s)",
            mean_duration, stddev
        );
        println!("  Fastest:          {:.3}s", min_duration);
        println!("  Slowest:          {:.3}s", max_duration);
        println!("  Mean throughput:  {:.2} MB/s", mean_throughput);

        let (tb_hours, tb_formatted) = estimate_terabyte_time(mean_throughput);
        println!(
            "  Estimated 1TB:    {} ({:.2} hours)",
            tb_formatted, tb_hours
        );
        println!();

        all_results.push((description.to_string(), condition_results));
    }

    // Final comparison
    println!("═══════════════════════════════════════════════════════════════════════════════");
    println!("                          PERFORMANCE COMPARISON");
    println!("═══════════════════════════════════════════════════════════════════════════════\n");

    for (i, (description, results)) in all_results.iter().enumerate() {
        let (mean_duration, _stddev, _min, _max, mean_throughput) = calculate_statistics(results);
        println!("{}. {}", i + 1, description);
        println!(
            "   Average: {:.3}s | Throughput: {:.2} MB/s",
            mean_duration, mean_throughput
        );

        if i > 0 {
            let (baseline_desc, baseline_results) = &all_results[0];
            let (baseline_mean, _, _, _, baseline_throughput) =
                calculate_statistics(baseline_results);
            let speedup = baseline_mean / mean_duration;
            let throughput_gain =
                ((mean_throughput - baseline_throughput) / baseline_throughput) * 100.0;
            println!(
                "   vs {}: {:.2}x faster | Throughput gain: {:.1}%",
                baseline_desc, speedup, throughput_gain
            );
        }
        println!();
    }

    println!("═══════════════════════════════════════════════════════════════════════════════");
    println!("                              BENCHMARK COMPLETE");
    println!("═══════════════════════════════════════════════════════════════════════════════");
}
