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
use tracing::{debug, error, info, warn};
use tracing_subscriber;
#[derive(Parser)]
#[command(name = "blockframe")]
#[command(about = "erasure-coded storage with transparent file access")]
#[command(arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Commit a specific file to the archive.
    ///
    /// This will break the file into chunks, apply erasure coding, and
    /// save it to the archive directory.
    Commit {
        /// The source file to upload.
        #[arg(short, long)]
        file: PathBuf,
        /// Directory where chunks are stored.
        #[arg(short, long, default_value = "archive_directory")]
        archive: PathBuf,
    },

    /// Start an HTTP server to serve the archive.
    ///
    /// Allows users to browse and download files via a web browser.
    Serve {
        /// Directory where chunks are stored.
        #[arg(short, long, default_value = "archive_directory")]
        archive: PathBuf,

        /// Port to bind the server to.
        #[arg(short, long, default_value_t = 8080)]
        port: u16,
    },

    /// Mount the archive as a virtual filesystem.
    ///
    /// This mounts the archive as a Read-Only filesystem, allowing transparent access
    /// to files without manually restoring them.
    Mount {
        /// The location to mount the filesystem (e.g., /tmp/blockframe).
        #[arg(short, long)]
        mountpoint: PathBuf,
        /// Path to a local archive directory.
        #[arg(
            short,
            long,
            conflicts_with = "remote",
            required_unless_present = "archive"
        )]
        archive: Option<PathBuf>,
        /// URL of a remote blockframe server.
        #[arg(
            short,
            long,
            conflicts_with = "archive",
            required_unless_present = "archive"
        )]
        remote: Option<String>,
    },

    /// Check the health of all files and attempt repairs.
    ///
    /// Scans all file manifests and chunks. If chunks are missing but enough
    /// parity chunks exist, the file will be repaired.
    Health {
        /// Directory where chunks are stored.
        #[arg(short, long, default_value = "archive_directory")]
        archive: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    tracing_subscriber::fmt::init();

    let chunker = Chunker::new()?;

    match cli.command {
        Commands::Commit { file, archive } => {
            // use existing Chunker
            let _ = chunker.commit(&file)?;
            Ok(())
        }

        Commands::Health { archive } => {
            let store = FileStore::new(&archive)?;
            let batch_report = store.batch_health_check()?;
            info!("=== BATCH HEALTH CHECK RESULTS ===");
            info!("Total files: {}", batch_report.total_files);
            info!("  Healthy: {}", batch_report.healthy);
            info!("  Degraded: {}", batch_report.degraded);
            info!("  Recoverable: {}", batch_report.recoverable);
            info!("  Unrecoverable: {}", batch_report.unrecoverable);

            // Attempt repairs on any recoverable files
            if batch_report.recoverable > 0 || batch_report.degraded > 0 {
                info!("=== ATTEMPTING REPAIRS ===");
                for (filename, report) in &batch_report.reports {
                    if report.status != blockframe::filestore::models::HealthStatus::Healthy {
                        info!("Repairing {}...", filename);
                        let file = store.find(filename)?;
                        match store.repair(&file) {
                            Ok(_) => info!("  ✓ Repair completed"),
                            Err(e) => info!("  ✗ Repair failed: {}", e),
                        }
                    }
                }

                // Re-check health after repairs
                info!("=== POST-REPAIR HEALTH CHECK ===");
                let post_repair = store.batch_health_check()?;
                info!(
                    "Healthy: {}/{}",
                    post_repair.healthy, post_repair.total_files
                );
                info!("Recoverable: {}", post_repair.recoverable);
                info!("Unrecoverable: {}", post_repair.unrecoverable);
            } else {
                info!("=== ALL FILES HEALTHY ===");
                info!("No repairs needed!");
            }
            Ok(())
        }

        //SECTION to be implimented
        Commands::Serve { archive, port } => {
            info!("archive directory set for: {:?}", Path::new(&archive));
            info!("CWD: {:?}", std::env::current_dir());
            let config_port = option_env!("port").and_then(|s| s.parse().ok());
            if let Some(p) = config_port {
                run_server(archive, p).await?;
                Ok(())
            } else {
                run_server(archive, port).await?;
                Ok(())
            }
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
                winfsp::winfsp_init_or_die();

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

                info!("Mounted at {:?}. Press Enter to unmount.", mountpoint);
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
