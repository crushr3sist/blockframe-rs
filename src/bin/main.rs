use blockframe::{
    chunker::Chunker,
    config::Config,
    filestore::FileStore,
    mount::{
        BlockframeFS,
        source::{LocalSource, RemoteSource, SegmentSource},
    },
    serve::run_server,
};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{info, warn};
use tracing_appender::{
    non_blocking,
    rolling::{RollingFileAppender, Rotation},
};
use tracing_subscriber::{EnvFilter, Registry, fmt, layer::SubscriberExt};

/// CLI for Accessing Blockframe functions
#[derive(Parser)]
#[command(name = "blockframe")]
#[command(about = "erasure-coded storage with transparent file access")]
#[command(arg_required_else_help = true)]
struct Cli {
    /// Commands parsers for subcommands
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
    },

    /// Start an HTTP server to serve the archive.
    ///
    /// Allows users to browse and download files via a web browser.
    Serve {
        /// Directory where chunks are stored.
        #[arg(short, long)]
        archive: Option<PathBuf>,

        /// Port to bind the server to.
        #[arg(short, long)]
        port: Option<u16>,
    },

    /// Mount the archive as a virtual filesystem.
    ///
    /// This mounts the archive as a Read-Only filesystem, allowing transparent access
    /// to files without manually restoring them.
    Mount {
        /// The location to mount the filesystem (e.g., /tmp/blockframe).
        #[arg(short, long)]
        mountpoint: Option<PathBuf>,
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
        #[arg(short, long)]
        archive: Option<PathBuf>,
    },
}

/// Logging initiser for listing to the logger events and rolling logging
pub fn init_logging() {
    // file_appender a RollingFileAppender object
    // file_appender is used to write to the log file, however the log file will roll over to another log file
    // when the given rotation option. Which is configured to be daily.
    let file_appender = RollingFileAppender::new(Rotation::DAILY, "./logs", "blockframe.log");

    // file_writer and _file_guard are the non-blocking feature function
    // essentially the logging function is moved from the main thread to a dedicated background worker thread
    // this is done to ensure logging does not interfere with main performance
    let (file_writer, _file_guard) = non_blocking(file_appender);

    // std_writer and _stdout_guard are also background worker threads
    let (stdout_writer, _stdout_guard) = non_blocking(std::io::stdout());

    // subscriber is responsible for routing where tracing events are processed
    // it is the global sink that collects tracing events, filters them, formats them and writes them to stdout and a log file
    let subscriber = Registry::default() // the core event router. It keeps track of spans, thier relationships and forwards events to layers
        .with(
            // EnvFilter is used to read a log filter string from the environment
            // if RUST_LOG isnt set in the env then those rules will be used
            // otherwise the fallback flags will be the default
            // info log emits such as warn, info and error
            // debug is enabled if its flagged in the crate
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info, blockframe=debug")),
        )
        .with(
            // fmt::layer() is used to format the log
            fmt::layer()
                // sends the formatted log lines to stdout, the non-blocking stdout_writer
                .with_writer(stdout_writer)
                // the event's target is included in the output, e.g. the crate or module path like blockframe::mount::source
                .with_target(true)
                // includes the thread ID that emitted the log
                // useful for the API calls as they are asynchronous
                .with_thread_ids(true),
        )
        .with(
            // the last format layer
            fmt::layer()
                // this time we use file_writter to write the logs to the log file instead of stdout
                .with_writer(file_writer)
                // ansi disabled color escape codes to keep logs simple plain text, no color pollution
                .with_ansi(false),
        );
    // tracing::subscriber installs the subscriber globally.
    // Ever tracing macro anywhere on the program sends events through the subscriber
    // called twice will cause a fail
    tracing::subscriber::set_global_default(subscriber).expect("Failed to set tracing subscriber");

    // TANGENT_EXPLAINATION: This is a very cool trick.
    // Box<t> is a "smart pointer", its stored on the heap, and when it goes out of scope it is 'dropped'
    // wrapping our non-blocking guards in a smart-pointer seems counter-intuitive
    // however we pass our smart-pointer (Box) into a `Box::leak` which returns `&'static mut T`
    // Box::leak consumes our smart-pointer and forgets how to free it, and hands back a reference that lives for 'static
    // The reason we dont assign it to a variable through explicit let is done so that,
    // we hand the _file_guard and _stdout_guard's ownership into the heap
    // then we deliberately abandon the ability to free it. The side effect is the leak itself not the reference.
    // What this achives is it keeps the guards and writters alive and ensures the logging threads arent randomly shut down and the logs stop working silently.
    Box::leak(Box::new(_file_guard));
    Box::leak(Box::new(_stdout_guard));
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    init_logging();

    // Load configuration file
    let config = Config::load().map_err(|e| {
        format!("Failed to load config.toml: {}. Make sure config.toml exists in the current directory.", e)
    })?;

    // Warn if both remote and archive are configured (could be confusing)
    if !config.mount.default_remote.is_empty() {
        warn!(
            "Config has default_remote set to: {}",
            config.mount.default_remote
        );
        warn!("The 'mount' command will connect to the remote server by default.");
        warn!("Use --archive flag to override and mount local archive instead.");
    }

    let chunker = Chunker::new()?;

    match cli.command {
        Commands::Commit { file } => {
            // use existing Chunker
            info!(file = ?file, "starting commit");
            let _ = chunker.commit(&file)?;
            Ok(())
        }

        Commands::Health { archive } => {
            let archive_path = archive.unwrap_or_else(|| config.archive.directory.clone());
            let store = FileStore::new(&archive_path)?;
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

        Commands::Serve { archive, port } => {
            let archive_path = archive.unwrap_or_else(|| config.archive.directory.clone());
            let server_port = port.unwrap_or(config.server.default_port);

            info!(
                archive = archive_path.to_str(),
                "SERVE | archive directory set"
            );
            info!("CWD: {:?}", std::env::current_dir());
            run_server(archive_path, server_port).await?;
            Ok(())
        }

        Commands::Mount {
            mountpoint,
            archive,
            remote,
        } => {
            let mount_path = mountpoint.unwrap_or_else(|| config.mount.default_mountpoint.clone());

            info!("MOUNT | starting mount operation");
            info!("MOUNT | mountpoint: {:?}", mount_path);

            // source is a smart-pointer which points to our source
            // we're using a smart-pointer as it could either be a RemoteSource or LocalSource
            let source: Box<dyn SegmentSource> = if let Some(url) = remote {
                // If mount command is flagged with remote
                // then we'll return a smart-pointer to a RemoteSource object
                // RemoteSource object connects to another blockframe url which is serving
                info!("MOUNT | using remote source: {}", url);
                Box::new(RemoteSource::new(url))
            } else if let Some(path) = archive {
                // If mount command is flagged with archive
                // then we'll return a smart-pointer to a LocalSource object
                // LocalSource is used to interface local files
                info!("MOUNT | using local source: {:?}", path);
                Box::new(LocalSource::new(path)?)
            } else if !config.mount.default_remote.is_empty() {
                // Use default remote from config if specified
                info!(
                    "MOUNT | using default remote source from config: {}",
                    config.mount.default_remote
                );
                Box::new(RemoteSource::new(config.mount.default_remote.clone()))
            } else {
                // Use default archive directory from config
                info!(
                    "MOUNT | using default archive from config: {:?}",
                    config.archive.directory
                );
                Box::new(LocalSource::new(config.archive.directory)?)
            };
            // Initalising the BlockframeFS class with the given source
            info!("MOUNT | creating filesystem");
            let fs = BlockframeFS::new(source)?;

            #[cfg(target_os = "windows")]
            {
                // winfsp_init_or_die is called to check if WinFsp runtime is loaded and ready
                // if WinFsp isnt installed or cant initalise, the process aborts immediately.
                info!("MOUNT | initializing WinFsp");
                winfsp::winfsp_init_or_die();

                // importing windows specific libraries
                use std::io::{self, Read};
                use winfsp::host::VolumeParams;
                // volume_params is a VolumeParams object
                // VolumeParams is an object that configures the medata about the virtual disk being presented to windows
                // its used to describe the shape and rules of the filesystem volume so windows knows how to interact with it
                info!("MOUNT | configuring volume parameters");
                let mut volume_params = VolumeParams::new();

                // The `sector_size` tells windows the logical sector size of the filesystem
                // the `sector_size` is set to 512 bytes which is the traditional disk sector size.
                volume_params.sector_size(512);

                // `sectors_per_allocation_unit` defines the allocation unit (cluster) size.
                // with 1 sector per allocation unit, the cluster size is 512 bytes.
                // small clusters reduce wasted space but increase metadata churn, but this is a conservative and filesystem-friendly
                volume_params.sectors_per_allocation_unit(1);

                // `volume_serial_number` is the filesystems ID. Windows uses it to recognise whether a volume is "the same" across mounts.
                // 0 means auto assignment.
                volume_params.volume_serial_number(0);

                // `file_info_timeout` is a cache hint in miliseconds
                // we're flagging that windows is allowed to cache the file's metadata for 1 second before asking the filesystem again
                // 1 second is used for reducing call spam and a reasonable balance between refreshes.
                volume_params.file_info_timeout(1000);

                // `case_sensitive_search` treats 'File.txt' and 'file.txt' the same when searching
                volume_params.case_sensitive_search(false);

                // `case_preserved_names` ensures filenames sustain thier casing when displayed. Even though seraches are case-insensitive
                // quality of life option, and its enabled to match windows-esk functionality
                volume_params.case_preserved_names(true);
                // `unicode_on_disk` flags if filenames are treated as unicode. This ensures multi-lingual and obscure characters dont panic the volume
                volume_params.unicode_on_disk(true);
                // `persistent_acls` is flagged to false as Blockframe doesnt impliment Access Control Lists (acl)
                // ACL's are not stored persistently by Blockframe. Windows will still ask about permissions
                // If ACL's are persisted, windows would start relying on behaviour which is not supported by Blockframe
                // enabling would create subtle breakages such as access denied errors, explorer weirdness or files being unreadable for no obvious reason.
                volume_params.persistent_acls(false);
                // `post_cleanup_when_modified_only` is an optimisation.
                // cleanup work after file handles close only happens if the file was actually modified.
                // less overhead, fewer pointless calls.
                volume_params.post_cleanup_when_modified_only(true);

                // host is a FileSystemHost object which manages the lifetime of the mounted volume
                // it binds the `BlockframeFS` to WinFsp using the `volume_params` that were defined.
                info!("MOUNT | creating filesystem host");
                let mut host = winfsp::host::FileSystemHost::new(volume_params, fs)?;

                // this mounts our provided mountpoint which is the directory where the filesystem will mount
                info!("MOUNT | mounting to: {:?}", mount_path);
                host.mount(&mount_path)?;

                // we then start our request loop.
                // from this point on, windows explore, dir, file reads will actively be called into the filesystem
                info!("MOUNT | starting filesystem");
                host.start()?;

                // blockframe uses stdin for exitpoint, its a crude lifetime guard
                // it keeps the processes alive until the user presses enter, which unmounts the filesystem.
                info!("Mounted at {:?}. Press Enter to unmount.", mount_path);
                io::stdin()
                    .read_exact(&mut [0u8])
                    .map_err(|e| e.to_string())?;
            }

            #[cfg(not(target_os = "windows"))]
            {
                // import linux specific libraries
                use fuser::MountOption;

                // options is a list of our MountOption' options.
                // essentially its a configure list for the FUSE kernel module.
                // it is an enum of MountOption enums which tell the Linux Kernel how to treat this specific filesystem mount
                // This is essentially passing a rulebook to the OS before the filesystem mounts.
                let options = vec![
                    // MountOption::RO stands for `Read-Only` option meaning we're blocking all write operations at the system call level
                    // This filters out any write requests.
                    MountOption::RO,
                    // This is the filesystems name. Purely cosmetic.
                    MountOption::FSName("blockframe".to_string()),
                    // AutoUnmount is used to prevent stale mount points, with a tiny uncertainty.
                    // its used to automatically unmount the directory if blockframe crashes or exists.
                    MountOption::AutoUnmount,
                    // DefaultPermissions flags standard unix permission checks (rwx). This avoids stupid permission issues.
                    MountOption::DefaultPermissions,
                ];

                // This is the zombie mount.
                // we start by checking if our mount point doesnt exist, as in the directory to "mount" our filesystem
                // TANGENT_EXPLAINATION: the reason this is a zombie mount is due to the difference in how linux's filesystem works.
                // linux doesnt have drives, it has a huge filesystem which is associated with permissions.
                // However, with linux, if our BlockframeFS crashes for some reason, our linux kernel wont be informed properly.
                // If blockframe crashes, it wont have time to inform the kernel, beacuse unmounting is a manual call that needs to be made.
                // The reason we have to unmount, if blockframe crashes, and we couldnt call fusermount -u, linux will still sustain that mountpoint still belongs to a process that's not alive anymore, making that mountpoint stale.
                if !mount_path.exists() {
                    // if our mountpoint doesnt exist
                    // we then attempt to check if creating out mountpoint causes an error
                    match std::fs::create_dir_all(&mount_path) {
                        // if there were no errors, we exit the match predicate no problems.
                        // we created our mountpoint
                        Ok(_) => {}
                        // if creating the dir does cause an error, we check to see if its an AlreadyExists
                        // meaning we still have a successful outcome
                        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                            warn!("Mountpoint appears to be stale. Attempting cleanup...");
                            let _ = std::process::Command::new("fusermount") // fusermount allows non-root users to mount and unmount FUSE filesystems.
                                .arg("-u") // -u is unmount
                                .arg("-q") // -q (Quiet). We are calling unmount speculatively.
                                // If the directory wasn't actually mounted (and the mkdir failed for a different reason),
                                // fusermount would normally error out. -q suppresses that error so we don't spam logs.
                                .arg(&mount_path) // we also pass in our given mountpoint as the directory we're trying to fix
                                .status();
                        }
                        Err(e) => return Err(e.into()),
                    }
                }

                // and finally we mount the blockframe filesystem.
                // with linux this is a bit different.
                // TANGENT_EXPLAINATION: On windows our filesystem is "plugged in"
                // where as on linux, our filesystem is "placed on-top".
                // Linux doesnt have drives, as akin to windows E: or C:. Linux is just one filesystem with a bunch of folders.
                // When we mount or filesystem on linux, what is happening is, blockframe creats a telephone line (a socket) to the linux kernel
                // when the user checks to see the files, instead of seeing the physical files placed in that folder, the linux kernel intercepts `ls` request and understands that there is a process attached to that folder
                // instead of being served the actual files in that folder, blockframe instead serves the files.
                fuser::mount2(fs, &mount_path, &options)?;
            }

            Ok(())
        }
    }
}
