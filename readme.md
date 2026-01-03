# BlockFrame

![alt text](final.png)

A self-hosted erasure-coded storage engine implementing Reed-Solomon fault tolerance with transparent filesystem mounting.

BlockFrame addresses scenarios where S3-compatible object stores and traditional backup systems may not be suitable: single-machine deployments requiring data resilience, offline operation without cluster quorum, and transparent file access without explicit restoration workflows. The system provides mathematical reconstruction from disk failures, bit rot, and corruption through parity-based recovery, combined with virtual filesystem mounting for seamless integration.

## Problem Statement

Cloud storage solves many problems but introduces dependencies on external infrastructure, recurring costs, and data sovereignty concerns. Self-hosted alternatives like MinIO provide S3 compatibility but require multi-node clusters for erasure coding. Traditional backup solutions rely on snapshots and historical copies rather than inline fault tolerance.

BlockFrame observes these gaps and offers a focused solution: single-binary deployment with erasure coding at the storage layer, transparent file access through FUSE and WinFSP mounts, and offline operation without external dependencies.

## Why Not S3 or MinIO?

S3 and MinIO are proven, production-grade systems serving millions of deployments. BlockFrame does not aim to replace them. It addresses different constraints.

| Consideration          | S3 / MinIO                                  | BlockFrame                                      |
| ---------------------- | ------------------------------------------- | ----------------------------------------------- |
| **Primary use case**   | Multi-tenant object storage, API-first      | Single-user fault-tolerant storage, FS-first    |
| **Deployment**         | Cloud service or 4+ node cluster            | Single machine (Raspberry Pi to server)         |
| **Erasure coding**     | Distributed across nodes                    | Local Reed-Solomon with configurable redundancy |
| **Access model**       | HTTP API (GET, PUT, DELETE)                 | Mounted filesystem (read/write/ls)              |
| **Offline operation**  | Requires cluster consensus or cloud network | Fully offline capable                           |
| **Data format**        | Opaque blobs                                | Inspectable segments, manifests, parity shards  |
| **Recovery model**     | Re-replication from healthy nodes           | Mathematical reconstruction from local parity   |
| **Resource footprint** | ~500MB RAM, runtime dependencies            | ~10MB RAM, single static binary                 |
| **Network dependency** | Required for distributed operation          | Optional (supports remote sources)              |

S3 and MinIO solve distributed coordination, multi-tenancy, and high availability. BlockFrame solves local fault tolerance and transparent access for self-hosted scenarios. They address different parts of the storage stack.

## What BlockFrame Provides

**Erasure Coding:** Reed-Solomon encoding at multiple tiers. Small files get RS(1,3) for maximum redundancy. Large files use RS(30,3) block-level encoding for storage efficiency. Automatic tier selection based on file size.

**Transparent Mounting:** FUSE (Linux) and WinFSP (Windows) filesystem implementations. Access archived files through standard filesystem operations without manual restoration. On-the-fly segment recovery from parity when corruption is detected.

**Offline Capability:** All operations work without network access. Recovery, verification, and mounting function purely from local disk state.

**Self-Healing:** Hash verification on every read. Automatic reconstruction from parity when segment corruption is detected. Corrupted segments are replaced in-place.

**Zero Configuration:** Single binary with sensible defaults. No database setup, no cluster coordination, no external services.

---

## Getting Started

### Prerequisites

**Windows:**

- [Rust toolchain](https://rustup.rs/) (stable)
- [WinFSP](https://winfsp.dev/) v2.0 or later (required for mounting)

**Linux:**

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

Clone and build:

```bash
git clone https://github.com/crushr3sist/blockframe-rs.git
cd blockframe-rs
cargo build --release
```

Binary will be available at `target/release/blockframe`.

### Quick Start

Commit a file to the archive:

```bash
./target/release/blockframe commit --file /path/to/your/file.bin
```

Mount the archive as a filesystem:

```bash
# Linux
./target/release/blockframe mount --mountpoint /mnt/blockframe --archive archive_directory

# Windows
.\target\release\blockframe.exe mount --mountpoint Z: --archive archive_directory
```

Access files through the mounted filesystem. Original files appear as regular files. Read operations trigger automatic hash verification and recovery if needed.

## CLI Reference

### `commit`

Archive a file with erasure coding.

```bash
blockframe commit --file <PATH>
```

**Arguments:**

- `--file, -f <PATH>`: Path to file to archive

**Behaviour:**

- Automatically selects tier based on file size
- Generates Reed-Solomon parity shards
- Builds Merkle tree for verification
- Writes manifest, segments, and parity to `archive_directory/{filename}_{hash}/`

**Example:**

```bash
blockframe commit --file /data/large-video.mp4
```

### `mount`

Mount archive as virtual filesystem.

```bash
blockframe mount --mountpoint <PATH> [--archive <PATH> | --remote <URL>]
```

**Arguments:**

- `--mountpoint, -m <PATH>`: Mount location (directory on Linux, drive letter on Windows)
- `--archive, -a <PATH>`: Local archive directory (conflicts with `--remote`)
- `--remote, -r <URL>`: Remote BlockFrame server URL (conflicts with `--archive`)

**Behaviour:**

- Reads manifests from archive
- Presents files as regular filesystem
- Performs hash verification on every read
- Automatically recovers corrupted segments from parity
- Read-only mount (writes not supported)

**Examples:**

```bash
# Linux local mount
blockframe mount -m /mnt/blockframe -a archive_directory

# Windows remote mount
blockframe mount -m Z: -r http://server.local:8080
```

**Note for Windows:** Requires WinFSP installed. Unmount with Ctrl+C or standard Windows unmount.

### `serve`

Start HTTP API server for remote access.

```bash
blockframe serve [--archive <PATH>] [--port <PORT>]
```

**Arguments:**

- `--archive, -a <PATH>`: Archive directory to serve (default: `archive_directory`)
- `--port, -p <PORT>`: HTTP port (default: `8080`)

**Behaviour:**

- Serves archive over HTTP
- Provides file listing and manifest endpoints
- Enables remote mounting from other machines
- Read-only access

**Example:**

```bash
blockframe serve --archive /storage/archive --port 9000
```

### `health`

Scan archive for corruption and attempt repairs.

```bash
blockframe health [--archive <PATH>]
```

**Arguments:**

- `--archive, -a <PATH>`: Archive directory to check (default: `archive_directory`)

**Behaviour:**

- Scans all manifests in archive
- Verifies segment hashes against Merkle tree
- Reports corruption statistics
- Attempts reconstruction from parity where possible
- Writes recovered segments back to disk

**Example:**

```bash
blockframe health --archive archive_directory
```

**Output Example:**

```
Checking 15 files...
✓ video.mp4: healthy (120 segments)
✗ dataset.bin: 3 corrupt segments
  → Recovered from parity: segments 45, 67, 89
✓ archive.tar: healthy (5 segments)
```

---

## Architecture

```
+------------------------------------------------------------------------------+
|                                PUBLIC API                                    |
|                                                                              |
|        commit()           find()           repair()         reconstruct()    |
|                                                                              |
|             CLI (binary): commit, serve, mount, health (clap)                |
|                                                                              |
+------------------------------------------------------------------------------+
                                    |
                                    v
+------------------------------------------------------------------------------+
|                            PROCESSING MODULES                                |
|                                                                              |
|   +---------------------------+        +---------------------------+         |
|   |         CHUNKER           |        |        FILESTORE          |         |
|   |                           |        |                           |         |
|   |   commit_tiny   (Tier 1)  |        |   get_all                 |         |
|   |   commit_segmented (T2)   |        |   find                    |         |
|   |   commit_blocked (Tier 3) |        |   repair                  |         |
|   |   generate_parity         |        |   reconstruct             |         |
|   +---------------------------+        +---------------------------+         |
+------------------------------------------------------------------------------+
                                    |
                                    v
+------------------------------------------------------------------------------+
|                            CORE COMPONENTS                                   |
|                                                                              |
|   +--------------+  +--------------+  +--------------+  +--------------+     |
|   | REED-SOLOMON |  | MERKLE TREE  |  |   MANIFEST   |  |    UTILS     |     |
|   |              |  |              |  |              |  |              |     |
|   | Encoder      |  | build_tree   |  | parse        |  | blake3 hash  |     |
|   | Decoder      |  | get_proof    |  | validate     |  | segment_size |     |
|   | SIMD accel   |  | verify       |  | serialize    |  |              |     |
|   +--------------+  +--------------+  +--------------+  +--------------+     |
|   +--------------+  +--------------+                                         |
|   |   CONFIG     |  |   LOGGING    |   (config.toml, tracing + logs)         |
|   +--------------+  +--------------+                                         |
+------------------------------------------------------------------------------+
                                    |
                                    v
+------------------------------------------------------------------------------+
|                               I/O LAYER                                      |
|                                                                              |
|        +--------------+      +--------------+      +--------------+          |
|        |  BufWriter   |      |   memmap2    |      |    Rayon     |          |
|        |              |      |              |      |              |          |
|        | buffered     |      | zero-copy    |      | parallel     |          |
|        | disk writes  |      | file reads   |      | processing   |          |
|        +--------------+      +--------------+      +--------------+          |
+------------------------------------------------------------------------------+
                                    |
                                    v
+------------------------------------------------------------------------------+
|                               SERVICE LAYER                                  |
|                                                                              |
|        +--------------+      +--------------+      +--------------+          |
|        |  BufWriter   |      |   memmap2    |      |    Rayon     |          |
|        |              |      |              |      |              |          |
|        | buffered     |      | zero-copy    |      | parallel     |          |
|        | disk writes  |      | file reads   |      | processing   |          |
|        +--------------+      +--------------+      +--------------+          |
|     +--------------------+   +----------------------+   +----------------+|  |
|     | HTTP API (Poem)    |   | Mount (FUSE / WinFsp)|   | Health / Repair |  |
|     |  /api/files        |   |  (LocalSource /      |   | CLI (daemon)    |  |
|     |  /files/*/manifest |   |   RemoteSource)      |   |                 |  |
|     +--------------------+   +----------------------+   +-----------------+  |
+------------------------------------------------------------------------------+
                                    |
                                    v
+------------------------------------------------------------------------------+
|                              FILE SYSTEM                                     |
|                                                                              |
|   archive_directory/                                                         |
|   +-- {filename}_{hash}/                                                     |
|       +-- manifest.json      <- merkle root, hashes, metadata                |
|       +-- segments/          <- original data in 32MB chunks                 |
|       +-- parity/            <- reed-solomon parity shards                   |
|       +-- blocks/            <- tier 3: groups of 30 segments                |
|       +-- reconstructed/     <- recovered files after `reconstruct()`        |
|       +-- logs/              <- runtime logs (logs/blockframe.log.*)         |
+------------------------------------------------------------------------------+
```

### Tiers

BlockFrame automatically selects encoding tier based on file size, balancing redundancy against storage overhead.

| Tier | File Size    | Encoding            | Overhead | Recovery Capability                 |
| ---- | ------------ | ------------------- | -------- | ----------------------------------- |
| 1    | < 10 MB      | RS(1,3) whole file  | 300%     | Lose 2 of 3 copies, still recover   |
| 2    | 10 MB – 1 GB | RS(1,3) per segment | 300%     | Each segment recovers independently |
| 3    | 1 – 35 GB    | RS(30,3) per block  | 10%      | Lose any 3 of 33 shards per block   |
| 4    | > 35 GB      | Hierarchical        | ~12%     | Planned                             |

**Tier 1** (tiny files): Entire file encoded as single unit. Maximum redundancy for critical small files.

**Tier 2** (medium files): Each 32MB segment gets independent parity. Corruption in one segment does not affect others.

**Tier 3** (large files): Segments grouped into blocks of 30, with block-level parity. Storage efficient for large datasets.

**Tier selection is automatic.** No manual configuration required.

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

**Encoding:**

1. File is memory-mapped (zero-copy reads)
2. Split into 32MB segments
3. For Tier 3, segments grouped into blocks of 30
4. Reed-Solomon encoding generates parity shards
5. Merkle tree built from segment hashes
6. Manifest, segments, and parity written to disk

**Recovery:**

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

**Tier 3 Performance:** Both Tier 3 files use RS(30,3) encoding with 10% storage overhead. The 1.6 GB file achieves 45 MB/s, writing 1.76 GB total (1.6 GB data + 160 MB parity) in 36 seconds. This approaches the HDD's rated sequential write speed of 88 MB/s when accounting for metadata writes and Merkle tree computation.

The larger 26.6 GB file maintains 16.6 MB/s sustained throughput across 830 blocks. The performance difference is due to file system overhead - smaller files benefit from better cache locality and fewer directory operations.

**SIMD Acceleration:** Reed-Solomon encoding completes in milliseconds per segment. The performance envelope is determined by storage write speeds, not computational throughput.

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

**`mount/`** - Filesystem implementations (FUSE and WinFSP). Transparent access with on-the-fly recovery. See [mount/README.md](src/mount/README.md) for cache architecture, concurrency patterns, and platform-specific considerations.

**`merkle_tree/`** - Hash tree construction and verification. Provides cryptographic integrity proofs.

**`serve/`** - HTTP API server for remote access.

**`config.rs`** - Configuration management.

**`utils.rs`** - BLAKE3 hashing and segment size calculations.

Browse module READMEs for deeper technical insight into specific subsystems.

---

## Technical Notes

**Reed-Solomon:** RS(n,k) codes provide mathematically guaranteed reconstruction from partial data loss. BlockFrame uses reed-solomon-simd for SIMD-accelerated encoding/decoding.

**Memory-mapped I/O:** Files are memory-mapped for zero-copy reads. RAM usage remains constant regardless of file size. Kernel handles paging; application iterates through segments.

**BLAKE3:** Used for all hashing (the `sha256` function name is historical). Faster than SHA-256 with better parallelization. Cryptographically secure.

**Cache:** Mounted filesystems use moka's W-TinyLFU for segment caching. Frequency-based eviction prevents cache pollution from sequential scans. See [mount/README.md](src/mount/README.md) for detailed cache analysis.

**Concurrency:** FUSE allows serialized access (`&mut self`). WinFSP requires shared access (`&self`) due to Windows I/O threading model. Both implementations are thread-safe through different mechanisms.

---

## Limitations

**Write Operations:** Mounting is read-only. Archived files cannot be modified in-place. To update a file, commit a new version.

**Tier 4:** Files over 35GB currently use Tier 3 encoding. Hierarchical Tier 4 is planned.

**Compression:** Not implemented. Recommend compressing files before archiving if needed.

**Encryption:** Not implemented. Use filesystem-level encryption (LUKS, BitLocker) or encrypt files before committing.

**Distributed Storage:** Single-machine only. Remote mounting is supported but does not provide replication.

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

---

## License

MIT

---

## Further Reading

For detailed technical explanations, architectural decisions, and implementation rationale, see module-specific READMEs:

- [Cache architecture and W-TinyLFU analysis](src/mount/README.md)
- [Tier selection and encoding strategies](src/chunker/README.md)
- [Batch health checking and repair workflows](src/filestore/README.md)
- [Merkle tree verification](src/merkle_tree/README.md)

Each module README provides context for design choices, trade-offs considered, and implementation details.
