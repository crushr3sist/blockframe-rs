# BlockFrame: Distributed, Erasure-Coded File Storage

## What is this?

BlockFrame is a distributed file storage system that encrypts, chunks, and distributes files across untrusted nodes with consistency guarantees. It ensures data integrity and availability through Reed-Solomon erasure coding and Merkle tree verification, making it resilient to data loss without relying on traditional backups.

## Why does this exist?

Existing distributed storage solutions like IPFS/BitTorrent, while excellent for content addressing and peer-to-peer distribution, often lack strong, built-in consistency guarantees and fine-grained control over data resilience without complex orchestration. BlockFrame addresses this by focusing on mathematically guaranteed data reconstruction and verifiable integrity, ensuring that data survives hardware failures and bit rot even in single-machine deployments, without requiring active cluster consensus. It provides the underlying storage mathematics for fault-tolerant data, complementing rather than replacing higher-level object stores or distributed file systems.

## How does it work?

### Architecture Diagram

```mermaid
graph LR
    A[Client Application] --> B{Encryption (AES-256)};
    B --> C{Chunking};
    C --> D{Erasure Coding & Merkle Tree Generation};
    D --> E[Distribution to Untrusted Nodes];
    E -- Reconstruction --> F[Client Application];
```

BlockFrame operates by taking a client's file and processing it through several stages:

1.  **Encryption**: Files are first encrypted (e.g., AES-256) to ensure privacy before being chunked and distributed.
2.  **Chunking**: The encrypted file is divided into fixed-size segments (e.g., 32MB).
3.  **Erasure Coding & Merkle Tree Generation**: Each chunk, or groups of chunks, is then fed into a Reed-Solomon encoder to generate parity shards. Simultaneously, a Merkle tree is constructed from the hashes of these segments, providing verifiable integrity proofs.
4.  **Distribution**: The data segments and their corresponding parity shards are then distributed across various storage locations, potentially untrusted nodes.
5.  **Reconstruction**: When a file is retrieved, missing or corrupted segments can be mathematically reconstructed using the remaining data and parity shards. The Merkle tree verifies the integrity of the reconstructed data.

A service layer, integrating with **FUSE** (for Linux) or **WinFSP** (for Windows), allows BlockFrame archives to be mounted as native file systems, providing transparent access to the stored data. Segment ordering is managed to ensure correct reconstruction, and operations are designed to be memory-bounded, meaning RAM usage remains constant regardless of file size by utilizing techniques like memory-mapped I/O.

## How do I run it?

### Platform Requirements

*   **Windows**: [WinFSP](https://winfsp.dev/) must be installed.
*   **Linux/macOS**: [FUSE](https://github.com/libfuse/libfuse) (or macOSFUSE) development libraries must be installed.

### Build Instructions

```bash
cargo build --release
```

### Basic Usage Example

To store a file:

```bash
# Example: Store a file named 'my_document.pdf'
blockframe commit my_document.pdf
```

To retrieve/access a file (assuming a mounted BlockFrame filesystem):

```bash
# Example: Access 'my_document.pdf' from the mounted filesystem
# The mount command will vary based on OS and configuration
# For instance, on Linux:
# sudo blockframe mount /mnt/blockframe
# Then you can access /mnt/blockframe/my_document.pdf
```

### Config File Example (`config.toml`)

```toml
# Example configuration for BlockFrame
archive_directory = "/path/to/your/archive" # Directory where BlockFrame stores its data
log_level = "info" # debug, info, warn, error
# encryption_key_path = "/path/to/your/encryption_key.bin" # Optional: Path to a file containing the encryption key
# segment_size_mb = 32 # Optional: Size of data segments in MB (default 32)
```

## What's the technical depth?

*   **Async Ordering Guarantees**: Ensures that data segments are processed and reconstructed in the correct sequence, even in highly concurrent or distributed environments.
*   **Encryption-Before-Chunking**: Guarantees data privacy by encrypting files prior to their segmentation and distribution, preventing leakage of information through chunk analysis.
*   **Memory-Bounded Operation (Configurable)**: Utilizes memory-mapped I/O and efficient data structures to maintain a low and constant memory footprint, regardless of the size of the files being processed.
*   **FUSE/WinFSP Integration**: Provides native filesystem interfaces for Linux/macOS (FUSE) and Windows (WinFSP), allowing BlockFrame archives to be mounted and interacted with like local drives.
*   **Comprehensive Logging (Tracing)**: Integrates `tracing` for detailed, structured logging across all components, aiding in debugging, performance analysis, and operational monitoring.

## What's not done?

*   **Distributed Consensus/Coordination**: While resilient to node failures, BlockFrame currently focuses on the storage layer on individual nodes. Mechanisms for distributed node consensus and automatic segment rebalancing across a dynamic cluster are future work.
*   **HTTP Streaming Server**: An integrated HTTP server for streaming content with byte-range support is planned.
*   **Native Protocol Handler**: Development of a `blockframe://` protocol handler for seamless integration with native applications.
*   **Advanced Data Deduplication**: While Merkle trees aid in verifying unique data, advanced block-level deduplication is not yet implemented.
*   **Dynamic Configuration Reloading**: Changes to `config.toml` currently require a service restart.
*   **Comprehensive Client Libraries**: More mature client libraries for various programming languages to interact with BlockFrame archives directly.