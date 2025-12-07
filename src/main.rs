//! BlockFrame - Main executable for testing and demonstration
//!
//! This binary demonstrates the core functionality of BlockFrame:
//! - Committing files with adaptive multi-tier erasure coding
//! - Health checking archived files
//! - Repairing corrupted or missing data
//!
//! For library usage, see the module documentation.

use blockframe::filestore::FileStore;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== BLOCKFRAME ARCHIVE SYSTEM ===\n");

    let store_path = Path::new("archive_directory");
    let store = FileStore::new(store_path)?;

    let batch_report = store.batch_health_check()?;

    println!("=== BATCH HEALTH CHECK RESULTS ===");
    println!("Total files: {}", batch_report.total_files);
    println!("  Healthy: {}", batch_report.healthy);
    println!("  Degraded: {}", batch_report.degraded);
    println!("  Recoverable: {}", batch_report.recoverable);
    println!("  Unrecoverable: {}", batch_report.unrecoverable);
    println!();

    // Show details for each file
    for (filename, report) in &batch_report.reports {
        println!("File: {}", filename);
        println!("  Status: {:?}", report.status);
        println!("  Details: {}", report.details);
        if !report.missing_data.is_empty() {
            println!("  Missing data files: {}", report.missing_data.len());
        }
        if !report.corrupt_segments.is_empty() {
            println!("  Corrupt segments: {}", report.corrupt_segments.len());
        }
        println!();
    }

    // Attempt repairs on any recoverable files
    if batch_report.recoverable > 0 || batch_report.degraded > 0 {
        println!("=== ATTEMPTING REPAIRS ===");
        for (filename, report) in &batch_report.reports {
            if report.status != blockframe::filestore::models::HealthStatus::Healthy {
                println!("Repairing {}...", filename);
                let file = store.find(filename)?;
                match store.repair(&file) {
                    Ok(_) => println!("  ✓ Repair completed"),
                    Err(e) => println!("  ✗ Repair failed: {}", e),
                }
            }
        }
        println!();

        // Re-check health after repairs
        println!("=== POST-REPAIR HEALTH CHECK ===");
        let post_repair = store.batch_health_check()?;
        println!(
            "Healthy: {}/{}",
            post_repair.healthy, post_repair.total_files
        );
        println!("Recoverable: {}", post_repair.recoverable);
        println!("Unrecoverable: {}", post_repair.unrecoverable);
    } else {
        println!("=== ALL FILES HEALTHY ===");
        println!("No repairs needed!");
    }

    Ok(())
}

// for now, lets work with a stateless object API
// we're going to expose these functions
// - aggregate all files commited
// - commit files
// - repair files
// - check health
