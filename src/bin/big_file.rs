use std::fs;

use rand::{Rng, TryRngCore};

const SIZE: usize = 2 * 1024 * 1024 * 1024;

/// Fills a 2&nbsp;GiB buffer with cryptographically secure random bytes and writes it to
/// `big_file.txt` in the current working directory.
///
/// The function is intentionally minimal: it streams random data directly into a
/// preallocated buffer before persisting it, providing a quick way to manufacture a
/// large file for benchmarking the rest of the project.
///
/// # Examples
///
/// ```
/// use rand::rngs::OsRng;
/// use rand::RngCore;
///
/// // The real binary writes 2 GiB, but the technique scales to any size.
/// let mut sample = vec![0u8; 32];
/// OsRng.fill_bytes(&mut sample);
/// assert!(sample.iter().any(|byte| *byte != 0));
/// ```
fn main() {
    let mut rng = rand::rngs::OsRng;

    let mut random_bytes: Vec<u8> = vec![0; SIZE];
    rng.try_fill_bytes(&mut random_bytes).expect("msg");

    fs::write("big_file.txt", random_bytes).expect("msg");
}
