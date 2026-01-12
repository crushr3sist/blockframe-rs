# BlockFrame

![alt text](final.png)
[![Release](https://github.com/crushr3sist/blockframe-rs/actions/workflows/release.yml/badge.svg)](https://github.com/crushr3sist/blockframe-rs/actions/workflows/release.yml)

I started this project because I was sick of the "API Tax." I was working on a project involving Databricks and spent more time wrestling with authentication and network latency than actually processing data. I just wanted to use standard tools like `grep` on my own files, but I needed the durability guarantees of proper object storage.

BlockFrame solves this by bringing Reed-Solomon erasure coding down to the local filesystem level. You act on it like a normal folder using FUSE (Linux) or WinFSP (Windows), but under the hood, it's splitting your files into chunks and protecting them against bit-rot and drive failure - no network required.

It's built specifically for local, write-once archival. It's not trying to replace S3 for the cloud, and it's definitely not for high-frequency dynamic writes. But if you want enterprise-grade durability for your local datasets without the complexity of a distributed cluster, this works.

## Core Engineering Approach

BlockFrame differs from tools like MinIO or S3 by prioritizing **OS-level integration** over API compatibility.

### 1. Storage Layer: Local Erasure Coding

Standard RAID protects against disk failure. BlockFrame protects against _data corruption_ (bit rot) at the file level.

- Reed-Solomon encoding via `reed-solomon-simd` (SIMD-accelerated).
- Small files (<10MB) use RS(1,3) for high redundancy. Large datasets use RS(30,3), splitting files into 32MB segments grouped into blocks for storage efficiency.
- Mathematical reconstruction of corrupted sectors without needing a 4-node cluster or ZFS.

### 2. Access Layer: Virtual Filesystem (FUSE/WinFSP)

Instead of writing a client library, I implemented a virtual filesystem driver.

- The application intercepts syscalls (`open`, `read`, `seek`).
- When a user reads a file, BlockFrame performs a Merkle tree hash check. If the hash mismatches (corruption detected), it transparently pauses the read, reconstructs the data from parity shards in memory, and serves clean bytes to the caller.
- Applications work with the data natively without knowing it's being repaired in real-time.

### Comparative Architecture

| Aspect     | Object Storage (S3/Databricks)     | BlockFrame                             |
| :--------- | :--------------------------------- | :------------------------------------- |
| Interface  | HTTP API (`GET /bucket/key`)       | Syscall (`read()`, `seek()`)           |
| Tooling    | Requires SDKs (`boto3`)            | Standard tools (Explorer, pandas, VLC) |
| Recovery   | Replica-based (network)            | Parity-based (CPU/SIMD)                |
| Complexity | Distributed consensus (Paxos/Raft) | Local state & Merkle proofs            |

### Intended Deployment

The design assumes a central server running BlockFrame with the archive mounted as a local drive. That mount point can then be shared over the network (SMB on Windows, NFS on Linux). This means:

- Only the server needs BlockFrame installed
- Clients connect to a standard network share
- Access control uses existing OS-level permissions (Active Directory, Group Policy, etc.)
- Data never leaves your infrastructure

Remote mounting is also natively supported for direct connection to a BlockFrame server.

## Features

- Reed-Solomon erasure coding at multiple tiers (RS(1,3) for small files, RS(30,3) for large files)
- Automatic tier selection based on file size
- FUSE (Linux) and WinFSP (Windows) filesystem mounting
- On-the-fly segment recovery from parity when corruption is detected
- Hash verification on every read
- Automatic reconstruction and in-place repair of corrupted segments
- Single binary with config file (config.toml)
- No external database or services required

---

## Getting Started

### Prerequisites

Windows:

- [Rust toolchain](https://rustup.rs/) (stable)
- [WinFSP](https://winfsp.dev/) v2.0 or later (required for mounting)

Linux:

- Rust toolchain (stable)
- FUSE development libraries:

  ```bash
  # Debian/Ubuntu
  sudo apt install libfuse-dev pkg-config

  # Fedora/RHEL
  sudo dnf install fuse-devel

  # Arch
  sudo pacman -S fuse2
  ```

### Installation

Blockframe is available for release, so please head over to the releases and download the latest version for your platform. Please do not estrange the winfsp-x64.dll from blockframe.exe.

Clone and build:

```bash
git clone https://github.com/crushr3sist/blockframe-rs.git
cd blockframe-rs
cargo build --release
```

Binary will be available at `target/release/blockframe`.

### Configuration

Before running any commands, create a `config.toml` file in the same directory as the blockframe executable. This file is required and provides default values for all commands.

Example `config.toml`:

```toml
[archive]
# Default directory for storing archived files
# Used by: commit, serve, health, and mount (when default_remote is empty)
directory = "archive_directory"

[mount]
# Default mountpoint for the virtual filesystem
default_mountpoint = "./mnt/blockframe"  # Linux example
# default_mountpoint = "Z:"              # Windows drive letter

# IMPORTANT: Default remote server URL for mounting
# - Leave EMPTY ("") to use local archive directory by default
# - When set, `blockframe mount` will connect to this remote server by default
# - You can still override with --archive flag to mount local archive
# Example: "http://192.168.1.100:8080"
default_remote = ""

[cache]
# Cache settings for filesystem mounting
# 1 segment = 32mb
max_segments = 200

# Maximum cache size (supports KB, MB, GB)
max_size = "3GB"

[server]
# Default port for HTTP server
default_port = 8080

[logging]
# Logging level: "trace", "debug", "info", "warn", "error"
level = "info"
```

Configuration Behavior:

- All CLI flags are optional - they override config defaults when provided
- Mount source priority (first available is used):
  1. `--remote` flag (if provided)
  2. `--archive` flag (if provided)
  3. `config.mount.default_remote` (if not empty)
  4. `config.archive.directory` (fallback)
- Warning: If you set `default_remote`, the mount command will connect to the remote server by default
- To use local archive when `default_remote` is set, use: `blockframe mount --archive archive_directory`
- This eliminates the need to specify `--archive`, `--port`, or `--mountpoint` repeatedly
- Adjust cache settings based on your system resources

### Quick Start

**1. Commit a file to the archive:**

```bash
blockframe commit --file /path/to/your/file.bin
```

Files are automatically stored in the `archive_directory` configured in `config.toml`.

**2. Mount the archive as a filesystem:**

```bash
# Simple: Uses defaults from config.toml
blockframe mount

# Or override specific settings:
# Linux
blockframe mount --mountpoint /mnt/custom --archive archive_directory

# Windows
blockframe mount --mountpoint Z: --archive archive_directory

# Remote mount (connect to another BlockFrame server)
blockframe mount --remote http://192.168.1.100:8080
```

**3. Access your files:**

Once mounted, access files through the mounted filesystem. Original files appear as regular files. Read operations trigger automatic hash verification and recovery if corruption is detected.

## CLI Reference

### `commit`

Archive a file with erasure coding.

```bash
blockframe commit --file <PATH>
```

**Arguments:**

- `--file, -f <PATH>`: Path to file to archive

Behaviour:

- Automatically selects tier based on file size
- Generates Reed-Solomon parity shards
- Builds Merkle tree for verification
- Writes manifest, segments, and parity to `archive_directory/{filename}_{hash}/`

Example:

```bash
blockframe commit --file /data/large-video.mp4
```

### `mount`

Mount archive as virtual filesystem.

```bash
blockframe mount [--mountpoint <PATH>] [--archive <PATH> | --remote <URL>]
```

Arguments (all optional):

- `--mountpoint, -m <PATH>`: Mount location (default: from `config.toml`)
  - Linux: directory path (e.g., `/mnt/blockframe`)
  - Windows: drive letter (e.g., `Z:`)
- `--archive, -a <PATH>`: Local archive directory (default: from `config.toml`, conflicts with `--remote`)
- `--remote, -r <URL>`: Remote BlockFrame server URL (default: from `config.toml`, conflicts with `--archive`)

Behaviour:

- If no flags are provided, uses all defaults from `config.toml`
- If `default_remote` is set in config and no flags are given, connects to remote server
- Otherwise falls back to local archive directory from config
- Reads manifests from archive or remote server
- Presents files as regular filesystem
- Performs hash verification on every read
- Automatically recovers corrupted segments from parity
- Read-only mount (writes not supported)

**Examples:**

```bash
# Use all defaults from config.toml
blockframe mount

# Override mountpoint only
blockframe mount -m /mnt/custom

# Linux local mount with explicit paths
blockframe mount -m /mnt/blockframe -a archive_directory

# Windows remote mount
blockframe mount -m Z: -r http://server.local:8080

# Remote mount using config defaults for mountpoint
blockframe mount -r http://192.168.1.50:8080
```

**Note for Windows:** Requires WinFSP installed. Unmount with Ctrl+C or standard Windows unmount.

### `serve`

Start HTTP API server for remote access.

```bash
blockframe serve [--archive <PATH>] [--port <PORT>]
```

Arguments (all optional):

- `--archive, -a <PATH>`: Archive directory to serve (default: from `config.toml`)
- `--port, -p <PORT>`: HTTP port (default: from `config.toml`)

Behaviour:

- Serves archive over HTTP with CORS enabled for cross-origin access
- Provides file listing, manifest, and segment download endpoints
- Enables remote mounting from other machines on your network
- OpenAPI documentation available at `http://<your-ip>:<port>/docs`
- Read-only access

**Examples:**

```bash
# Use defaults from config.toml
blockframe serve

# Override port only
blockframe serve --port 9000

# Serve custom archive directory
blockframe serve --archive /storage/archive --port 9000
```

**Remote Access:**

Once serving, access the API documentation at `http://<your-ip>:8080/docs` (or your configured port). Other machines can mount your archive using:

```bash
blockframe mount --remote http://<your-ip>:8080
```

### `health`

Scan archive for corruption and attempt repairs.

```bash
blockframe health [--archive <PATH>]
```

Arguments (optional):

- `--archive, -a <PATH>`: Archive directory to check (default: from `config.toml`)

Behaviour:

- Scans all manifests in archive
- Verifies segment hashes against Merkle tree
- Reports corruption statistics
- Attempts reconstruction from parity where possible
- Writes recovered segments back to disk

**Examples:**

```bash
# Use default archive from config.toml
blockframe health

# Check specific archive directory
blockframe health --archive /backup/archive
```

**Output Example:**

```
Checking 15 files...
video.mp4: healthy (120 segments)
dataset.bin: 3 corrupt segments
  Recovered from parity: segments 45, 67, 89
archive.tar: healthy (5 segments)
```

---

## Architecture

Module Structure:

- `chunker/` - File segmentation and Reed-Solomon encoding (commit_tiny, commit_segmented, commit_blocked)
- `filestore/` - Archive operations (get_all, find, repair, reconstruct)
- `merkle_tree/` - Hash tree construction and verification
- `mount/` - FUSE/WinFSP filesystem implementations (LocalSource, RemoteSource)
- `serve/` - HTTP API server (Poem)
- `config.rs` - Configuration management
- `utils.rs` - BLAKE3 hashing and utilities

Core Dependencies:

- Reed-Solomon encoder/decoder (reed-solomon-simd)
- Merkle tree for integrity verification
- Manifest parser and validator
- BLAKE3 hashing

**I/O Layer:**

- BufWriter for buffered disk writes
- memmap2 for zero-copy file reads
- Rayon for parallel processing

**Service Layer:**

- HTTP API (Poem) for remote access
- FUSE (Linux) / WinFSP (Windows) for filesystem mounting
- Health checking and repair CLI

### Tiers

BlockFrame automatically selects encoding tier based on file size, balancing redundancy against storage overhead.

| Tier | File Size    | Encoding            | Overhead | Recovery Capability                 |
| ---- | ------------ | ------------------- | -------- | ----------------------------------- |
| 1    | < 10 MB      | RS(1,3) whole file  | 300%     | Lose 2 of 3 copies, still recover   |
| 2    | 10 MB – 1 GB | RS(1,3) per segment | 300%     | Each segment recovers independently |
| 3    | 1 – 35 GB    | RS(30,3) per block  | 10%      | Lose any 3 of 33 shards per block   |
| 4    | > 35 GB      | Hierarchical        | ~12%     | Planned                             |

Tier 1 (tiny files): Entire file encoded as single unit. Maximum redundancy for critical small files.

Tier 2 (medium files): Each 32MB segment gets independent parity. Corruption in one segment does not affect others.

Tier 3 (large files): Segments grouped into blocks of 30, with block-level parity. Storage efficient for large datasets.

Tier selection is automatic. No manual configuration required.

---

## Storage Layout

```
archive_directory/
└── {filename}_{hash}/
    ├── manifest.json           # Merkle root, hashes, metadata
    ├── segments/               # 32MB data segments
    │   └── segment_N.dat
    ├── parity/                 # Reed-Solomon parity shards
    │   └── parity_N.dat
    └── blocks/                 # Tier 3: block structure
        └── block_N/
            ├── segments/
            └── parity/
```

Manifests are JSON. Segments and parity are raw binary. Everything is inspectable with standard tools.

---

## How It Works

Encoding:

1. File is memory-mapped (zero-copy reads)
2. Split into 32MB segments
3. For Tier 3, segments grouped into blocks of 30
4. Reed-Solomon encoding generates parity shards
5. Merkle tree built from segment hashes
6. Manifest, segments, and parity written to disk

Recovery:

1. Filesystem read triggers hash verification
2. If hash mismatch detected, load parity shards
3. Reed-Solomon decoder reconstructs original segment
4. Verify reconstructed segment against manifest hash
5. Write recovered segment back to disk
6. Return data to caller

**Reed-Solomon guarantees:** RS(30,3) means any 30 of 33 shards can reconstruct original data. RS(1,3) means any 1 of 4 shards (1 data + 3 parity) recovers the file.

---

## Performance

### Test Hardware

- **CPU:** Intel Core i5-12600KF (6P + 4E cores)
- **RAM:** 32 GB
- **Storage:** HDD (~88 MB/s sequential write)
- **OS:** Windows 11 Pro

### Measured Results

Benchmarks measured using `cargo run -- commit --file <file>` on HDD storage.

| File Size | Tier | Commit Time | Throughput |
| --------- | ---- | ----------- | ---------- |
| 171 KB    | 1    | 0.5s        | 0.4 MB/s   |
| 1.6 GB    | 3    | 36s         | 45 MB/s    |
| 26.6 GB   | 3    | 27m 23s     | 16.6 MB/s  |

Tier 3 Performance: Both Tier 3 files use RS(30,3) encoding with 10% storage overhead. The 1.6 GB file achieves 45 MB/s, writing 1.76 GB total (1.6 GB data + 160 MB parity) in 36 seconds. This approaches the HDD's rated sequential write speed of 88 MB/s when accounting for metadata writes and Merkle tree computation.

The larger 26.6 GB file maintains 16.6 MB/s sustained throughput across 830 blocks. The performance difference is due to file system overhead - smaller files benefit from better cache locality and fewer directory operations.

SIMD Acceleration: Reed-Solomon encoding completes in milliseconds per segment. The performance envelope is determined by storage write speeds, not computational throughput.

### Projected Performance

| Storage Type | Sequential Write | Expected Throughput | 10 GB Archive |
| ------------ | ---------------- | ------------------- | ------------- |
| 5400 RPM HDD | 80-100 MB/s      | 15-25 MB/s          | ~7 min        |
| 7200 RPM HDD | 120-150 MB/s     | 30-40 MB/s          | ~4 min        |
| SATA SSD     | 400-500 MB/s     | 100-150 MB/s        | ~80 sec       |
| NVMe SSD     | 2000-3500 MB/s   | 300-500 MB/s        | ~25 sec       |

Performance scales linearly with storage speed. On NVMe, the 26.6 GB file would encode in approximately 3 minutes. The SIMD-accelerated encoding pipeline ensures CPU is not the bottleneck on modern storage.

---

## Module Documentation

BlockFrame is organized into focused modules. Each contains its own README with implementation details, design rationale, and technical decisions.

**`chunker/`** - File segmentation and Reed-Solomon encoding. Handles commit pipeline from raw file to archived segments. See [chunker/README.md](src/chunker/README.md) for tier selection logic and encoding parameters.

**`filestore/`** - Archive operations. Manifest scanning, file location, repair and reconstruction workflows. See [filestore/README.md](src/filestore/README.md) for batch health checking and recovery strategies.

**`mount/`** - Filesystem implementations (FUSE and WinFSP). Transparent access with on-the-fly recovery. See [mount/README.md](src/mount/readme.md) for cache architecture, concurrency patterns, and platform-specific considerations.

**`merkle_tree/`** - Hash tree construction and verification. Provides cryptographic integrity proofs.

**`serve/`** - HTTP API server for remote access.

**`config.rs`** - Configuration management.

**`utils.rs`** - BLAKE3 hashing and segment size calculations.

Browse module READMEs for deeper technical insight into specific subsystems.

---

## Technical Notes

Reed-Solomon: RS(n,k) codes provide mathematically guaranteed reconstruction from partial data loss. BlockFrame uses reed-solomon-simd for SIMD-accelerated encoding/decoding.

Memory-mapped I/O: Files are memory-mapped for zero-copy reads. RAM usage remains constant regardless of file size. Kernel handles paging; application iterates through segments.

BLAKE3: Used for all hashing (the `blake3_hash_bytes` function name is historical). Faster than SHA-256 with better parallelization. Cryptographically secure.

Cache: Mounted filesystems use moka's W-TinyLFU for segment caching. Frequency-based eviction prevents cache pollution from sequential scans. See [mount/README.md](src/mount/readme.md) for detailed cache analysis.

Concurrency: FUSE allows serialized access (`&mut self`). WinFSP requires shared access (`&self`) due to Windows I/O threading model. Both implementations are thread-safe through different mechanisms.

---

## Limitations

Write Operations: Mounting is read-only. Archived files cannot be modified in-place. To update a file, commit a new version.

Tier 4: Files over 35GB currently use Tier 3 encoding. Hierarchical Tier 4 is planned.

Compression: Not implemented. Recommend compressing files before archiving if needed.

Encryption: Not implemented. Use filesystem-level encryption (LUKS, BitLocker) or encrypt files before committing.

Distributed Storage: Single-machine only. Remote mounting is supported but does not provide replication.

---

## Roadmap

- Tier 4 hierarchical encoding for files > 35GB
- Async I/O for improved throughput
- HTTP streaming server with byte-range requests
- Segment-level deduplication
- Optional compression and encryption layers
- Distributed replication protocol

---

## Dependencies

- [reed-solomon-simd](https://github.com/AndersTrier/reed-solomon-simd) - SIMD-accelerated erasure coding
- [blake3](https://github.com/BLAKE3-team/BLAKE3) - Fast cryptographic hashing
- [rayon](https://github.com/rayon-rs/rayon) - Data parallelism
- [memmap2](https://github.com/RazrFalcon/memmap2-rs) - Memory-mapped file I/O
- [serde](https://serde.rs/) - Serialization framework
- [fuser](https://github.com/cberner/fuser) - FUSE bindings (Linux)
- [winfsp](https://winfsp.dev/) - Filesystem driver (Windows)
- [moka](https://github.com/moka-rs/moka) - W-TinyLFU cache
- [clap](https://github.com/clap-rs/clap) - CLI argument parsing
- [tracing](https://github.com/tokio-rs/tracing) - Structured logging

**Note on winfsp-rs:** This project includes a patched version of [winfsp-rs](https://github.com/SnowflakePowered/winfsp-rs) located in `patches/winfsp-rs/`. The patches address specific compatibility and functionality requirements for BlockFrame. The original winfsp-rs is licensed under GPLv3, and the patched version maintains the same license.

---

## License

MIT

---

## Further Reading

For detailed technical explanations, architectural decisions, and implementation rationale, see module-specific READMEs:

- [Cache architecture and W-TinyLFU analysis](src/mount/readme.md)
- [Tier selection and encoding strategies](src/chunker/README.md)
- [Batch health checking and repair workflows](src/filestore/README.md)
- [Merkle tree verification](src/merkle_tree/README.md)

Each module README provides context for design choices, trade-offs considered, and implementation details.
