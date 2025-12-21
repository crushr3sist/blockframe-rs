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
use tracing::{error, info};
use tracing_appender::{
    non_blocking,
    rolling::{RollingFileAppender, Rotation},
};
use tracing_subscriber::{EnvFilter, Registry, fmt, layer::SubscriberExt};

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
        #[arg(short, long, conflicts_with = "remote")]
        archive: Option<PathBuf>,
        /// URL of a remote blockframe server.
        #[arg(short, long, conflicts_with = "archive")]
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

pub fn init_logging() {
    let file_appender = RollingFileAppender::new(Rotation::DAILY, "./logs", "blockframe.log");

    let (file_writer, _file_guard) = non_blocking(file_appender);
    let (stdout_writer, _stdout_guard) = non_blocking(std::io::stdout());
    let subscriber = Registry::default()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info, my_crate=debug")),
        )
        .with(
            fmt::layer()
                .with_writer(stdout_writer)
                .with_target(true)
                .with_thread_ids(true),
        )
        .with(fmt::layer().with_writer(file_writer).with_ansi(false));
    tracing::subscriber::set_global_default(subscriber).expect("Failed to set tracing subscriber");

    Box::leak(Box::new(_file_guard));
    Box::leak(Box::new(_stdout_guard));
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    init_logging();

    let chunker = Chunker::new()?;

    match cli.command {
        Commands::Commit { file, archive } => {
            // use existing Chunker
            info!(file = ?file, "starting commit");
            let _ = chunker.commit(&file)?;
            Ok(())
        }

        Commands::Health { archive } => {
            let store = FileStore::new(&archive)?;
            let batch_report = store.batch_health_check()?;
            info!(
                total_files = batch_report.total_files,
                healthy = batch_report.healthy,
                degraded = batch_report.degraded,
                recoverable = batch_report.recoverable,
                unrecoverable = batch_report.unrecoverable
            );

            // Attempt repairs on any recoverable files
            if batch_report.recoverable > 0 || batch_report.degraded > 0 {
                info!("REPAIR | attempting repairs");
                for (filename, report) in &batch_report.reports {
                    if report.status != blockframe::filestore::models::HealthStatus::Healthy {
                        info!(filename = filename, "Repairing");
                        let file = store.find(filename)?;
                        match store.repair(&file) {
                            Ok(_) => info!("Repair completed"),
                            Err(e) => info!(e = e, "Repair failed"),
                        }
                    }
                }

                // Re-check health after repairs
                info!("REPAIR | post-repair health check");
                let post_repair = store.batch_health_check()?;
                info!(
                    "Healthy: {}/{}",
                    post_repair.healthy, post_repair.total_files
                );
                info!(recoverable = post_repair.recoverable, "Recoverable");
                info!(unrecoverable = post_repair.unrecoverable, "Unrecoverable");
            } else {
                info!("REPAIR | all files healthy");
            }
            Ok(())
        }

        //SECTION to be implimented
        Commands::Serve { archive, port } => {
            info!(archive = archive.to_str(), "SERVE | archive directory set");
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
            info!("MOUNT | starting mount operation");
            info!("MOUNT | mountpoint: {:?}", mountpoint);

            let source: Box<dyn SegmentSource> = if let Some(url) = remote {
                info!("MOUNT | using remote source: {}", url);
                Box::new(RemoteSource::new(url))
            } else if let Some(path) = archive {
                info!("MOUNT | using local source: {:?}", path);
                Box::new(LocalSource::new(path)?)
            } else {
                error!("MOUNT | no source specified");
                std::process::exit(1);
            };

            info!("MOUNT | creating filesystem");
            let fs = BlockframeFS::new(source)?;

            #[cfg(target_os = "windows")]
            {
                info!("MOUNT | initializing WinFsp");
                winfsp::winfsp_init_or_die();

                use std::io::{self, Read};
                use winfsp::host::VolumeParams;

                info!("MOUNT | configuring volume parameters");
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

                info!("MOUNT | creating filesystem host");
                let mut host = winfsp::host::FileSystemHost::new(volume_params, fs)?;

                info!("MOUNT | mounting to: {:?}", mountpoint);
                host.mount(&mountpoint)?; // ← LIKELY CRASHES HERE

                info!("MOUNT | starting filesystem");
                host.start()?;

                info!("Mounted at {:?}. Press Enter to unmount.", mountpoint);
                io::stdin()
                    .read_exact(&mut [0u8])
                    .map_err(|e| e.to_string())?;
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
                            error!("Mountpoint appears to be stale. Attempting cleanup...");
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
