use std::fs::File;
use std::io::{BufWriter, Write};

use rand::{RngCore, TryRngCore};

const SIZE: u64 = 11_000_000_000;
const CHUNK_SIZE: usize = 64 * 1024 * 1024; // 64MB chunks

fn main() -> std::io::Result<()> {
    let mut rng = rand::rngs::OsRng;
    let file = File::create("11gb.txt")?;
    let mut writer = BufWriter::with_capacity(CHUNK_SIZE, file);

    let mut buffer = vec![0u8; CHUNK_SIZE];
    let mut remaining = SIZE;

    println!(
        "Generating {}GB file in {}MB chunks...",
        SIZE / 1_000_000_000,
        CHUNK_SIZE / 1_000_000
    );

    while remaining > 0 {
        let chunk_size = remaining.min(CHUNK_SIZE as u64) as usize;
        let _ = rng.try_fill_bytes(&mut buffer[..chunk_size]);
        writer.write_all(&buffer[..chunk_size])?;
        remaining -= chunk_size as u64;

        let progress = ((SIZE - remaining) as f64 / SIZE as f64 * 100.0) as u32;
        if progress % 10 == 0 {
            println!("Progress: {}%", progress);
        }
    }

    writer.flush()?;
    println!("Done! Created 6gb.txt");
    Ok(())
}
