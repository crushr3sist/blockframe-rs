// Build script to copy WinFSP DLL to output directory
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    // Only run on Windows
    if cfg!(target_os = "windows") {
        // This line is only relevant if you actually intend to embed resources.
        // If not, you can remove it.
        embed_resource::compile("blockframe.rc", embed_resource::NONE);

        println!("cargo:rerun-if-changed=patches/winfsp-rs/winfsp-sys/winfsp/bin/winfsp-x64.dll");

        // Get the output directory (target/debug or target/release)
        let profile = env::var("PROFILE").unwrap(); // "debug" or "release"

        // Construct paths
        let dll_source = Path::new("patches/winfsp-rs/winfsp-sys/winfsp/bin/winfsp-x64.dll");
        let target_dir = Path::new("target").join(&profile);
        let dll_dest = target_dir.join("winfsp-x64.dll");

        // Copy DLL to output directory
        if dll_source.exists() {
            if let Err(e) = fs::create_dir_all(&target_dir) {
                eprintln!("Warning: Failed to create target directory: {}", e);
                return;
            }

            match fs::copy(&dll_source, &dll_dest) {
                Ok(_) => println!(
                    "cargo:warning=Copied winfsp-x64.dll to {}",
                    dll_dest.display()
                ),
                Err(e) => eprintln!("Warning: Failed to copy WinFSP DLL: {}", e),
            }
        } else {
            eprintln!("Warning: WinFSP DLL not found at {}", dll_source.display());
        }
    }
}
