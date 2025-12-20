use blockframe::{
    chunker::Chunker,
    filestore::FileStore,
    mount::{
        BlockframeFS,
        source::{LocalSource, RemoteSource, SegmentSource},
    },
    serve::run_server,
};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "blockframe")]
#[command(about = "erasure-coded storage with transparent file access")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    // commit a file to the archive
    Commit {
        #[arg(short, long)]
        file: PathBuf,
        #[arg(short, long, default_value = "archive_directory")]
        archive: PathBuf,
    },

    // start HTTP server to serve archive
    Serve {
        #[arg(short, long, default_value = "archive_directory")]
        archive: PathBuf,

        #[arg(short, long, default_value = "8080")]
        port: u16,
    },

    // Mount archive as filesystem
    Mount {
        #[arg(short, long)]
        mountpoint: PathBuf,
        #[arg(short, long)]
        archive: Option<PathBuf>,
        #[arg(short, long)]
        remote: Option<String>,
    },

    // check health of all files
    Health {
        #[arg(short, long, default_value = "archive_directory")]
        archive: PathBuf,
    },
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let chunker = Chunker::new()?;
    winfsp::winfsp_init_or_die();

    match cli.command {
        //SECTION already implimented
        Commands::Commit { file, archive } => {
            // use existing Chunker
            let _ = chunker.commit(&file)?;
            Ok(())
        }

        Commands::Health { archive } => {
            let store = FileStore::new(&archive)?;
            let batch_report = store.batch_health_check()?;
            println!("=== BATCH HEALTH CHECK RESULTS ===");
            println!("Total files: {}", batch_report.total_files);
            println!("  Healthy: {}", batch_report.healthy);
            println!("  Degraded: {}", batch_report.degraded);
            println!("  Recoverable: {}", batch_report.recoverable);
            println!("  Unrecoverable: {}", batch_report.unrecoverable);
            println!();

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

        //SECTION to be implimented
        Commands::Serve { archive, port } => {
            println!("archive directory set for: {:?}", Path::new(&archive));
            println!("CWD: {:?}", std::env::current_dir());
            run_server(archive, port).await?;
            Ok(())
        }

        Commands::Mount {
            mountpoint,
            archive,
            remote,
        } => {
            let source: Box<dyn SegmentSource> = if let Some(url) = remote {
                Box::new(RemoteSource::new(url))
            } else if let Some(path) = archive {
                Box::new(LocalSource::new(path)?)
            } else {
                eprintln!("Must specify --archive or --remote");
                std::process::exit(1);
            };

            let fs = BlockframeFS::new(source)?;

            #[cfg(target_os = "windows")]
            {
                use std::io::{self, Read};
                use winfsp::host::VolumeParams;

                let mut volume_params = VolumeParams::new();
                volume_params.sector_size(512);
                volume_params.sectors_per_allocation_unit(1);
                volume_params.volume_serial_number(0);
                volume_params.file_info_timeout(1000);
                volume_params.case_sensitive_search(false);
                volume_params.case_preserved_names(true);
                volume_params.unicode_on_disk(true);
                volume_params.persistent_acls(false);
                volume_params.post_cleanup_when_modified_only(true);

                let mut host = winfsp::host::FileSystemHost::new(volume_params, fs)?;
                host.mount(&mountpoint)?;
                host.start()?;

                println!("Mounted at {:?}. Press Enter to unmount.", mountpoint);
                io::stdin().read_exact(&mut [0u8]).unwrap();
                // unmount is handled by Drop
            }

            #[cfg(not(target_os = "windows"))]
            {
                use fuser::MountOption;

                let options = vec![
                    MountOption::RO,
                    MountOption::FSName("blockframe".to_string()),
                ];

                if !mountpoint.exists() {
                    match std::fs::create_dir_all(&mountpoint) {
                        Ok(_) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                            // If exists() returned false but mkdir failed with AlreadyExists, it's likely a broken mount.
                            eprintln!("Mountpoint appears to be stale. Attempting cleanup...");
                            let _ = std::process::Command::new("fusermount")
                                .arg("-u")
                                .arg("-q")
                                .arg(&mountpoint)
                                .status();
                        }
                        Err(e) => return Err(e.into()),
                    }
                }
                fuser::mount2(fs, &mountpoint, &options)?;
            }

            Ok(())
        }
    }
}
