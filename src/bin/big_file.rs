use std::fs;

use rand::{Rng, TryRngCore};

const SIZE: usize = 2 * 1024 * 1024 * 1024;

fn main() {
    let mut rng = rand::rngs::OsRng;

    let mut random_bytes: Vec<u8> = vec![0; SIZE];
    rng.try_fill_bytes(&mut random_bytes).expect("msg");

    fs::write("big_file.txt", random_bytes).expect("msg");
}
