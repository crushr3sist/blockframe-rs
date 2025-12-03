# Chunker Module

The encoding engine of BlockFrame. Responsible for transforming files into fault-tolerant archives with Reed-Solomon erasure coding and Merkle tree integrity verification.

## Architecture

```
Chunker (stateless encoder)
    │
    ├── commit.rs      # Tier-specific commit logic
    ├── generate.rs    # Reed-Solomon parity generation
    ├── io.rs          # File I/O operations
    └── helpers.rs     # Segment hashing utilities
```

## Core Types

### `Chunker`

Stateless encoder instance. Each commit operation is independent—no internal state carries between files.

```rust
let chunker = Chunker::new()?;
let result = chunker.commit(Path::new("dataset.bin"))?;
```

### `ChunkedFile`

Immutable result of a successful commit. Contains all metadata needed for later operations.

```rust
pub struct ChunkedFile {
    pub file_name: String,
    pub file_size: usize,
    pub file_hash: String,        // BLAKE3 hash of original file
    pub merkle_tree: MerkleTree,  // Integrity verification structure
    pub segment_size: usize,      // Bytes per segment
    pub num_segments: usize,
    pub data_shards: usize,       // RS data shard count
    pub parity_shards: usize,     // RS parity shard count
}
```

## Commit Functions

### `commit(path) -> ChunkedFile`

Main entry point. Automatically selects tier based on file size:

| File Size | Tier | Function Called               |
| --------- | ---- | ----------------------------- |
| <10MB     | 1    | `commit_tiny`                 |
| 10MB-1GB  | 2    | `commit_segmented`            |
| 1GB-35GB  | 3    | `commit_blocked`              |
| >35GB     | 4    | `commit_segmented` (fallback) |

### `commit_tiny` — Tier 1

For files under 10MB. No segmentation—treats entire file as single shard.

**Storage:** `data.dat` + 3 parity files  
**Encoding:** RS(1,3) — 1 data shard, 3 parity  
**Overhead:** 300%

### `commit_segmented` — Tier 2

For files 10MB-1GB. Per-segment parity encoding.

**Process:**

1. Memory-map file (if >10MB)
2. Segment into 32MB chunks
3. Generate RS(1,3) parity per segment
4. Hash segments while processing (streaming BLAKE3)
5. Build Merkle tree from segment+parity hashes
6. Write manifest

**Storage:** `segments/segment_N.dat` + `parity/segment_N_parity_M.dat`  
**Overhead:** 300%

### `commit_blocked` — Tier 3

For files 1GB-35GB. Block-level parity with parallel processing.

**Process:**

1. Memory-map entire file
2. Group segments into blocks (30 segments per block)
3. Pre-create all directories
4. **Parallel:** For each block:
   - Write 30 segments in parallel (Rayon)
   - Generate RS(30,3) parity
   - Build block Merkle tree
5. Build file Merkle tree from block roots
6. Write manifest

**Storage:** `blocks/block_N/segments/` + `blocks/block_N/parity/`  
**Overhead:** 10%

## Parity Generation

### `generate_parity_segmented(segment_data) -> Vec<Vec<u8>>`

RS(1,3) encoding for Tier 1/2. Takes single segment, returns 3 parity shards.

### `generate_parity(segments, data_shards, parity_shards) -> Vec<Vec<u8>>`

Flexible RS encoding for Tier 3+. Handles variable shard counts and auto-pads segments to uniform size.

```rust
// Tier 3: 30 data segments → 3 parity shards
let parity = chunker.generate_parity(&block_segments, 30, 3)?;
```

**Implementation details:**

- Uses `reed-solomon-simd` for SIMD-accelerated encoding
- Pads all segments to max segment size (RS requires uniform shard sizes)
- Returns owned `Vec<Vec<u8>>` for immediate write

## I/O Operations

### Segment Writing

```rust
write_segment(index, dir, data) -> Result<()>
```

Buffered write with `BufWriter::with_capacity`. Capacity matches segment size to minimize syscalls.

### Parity Writing

```rust
write_segment_parities(segment_idx, dir, parity) -> Result<()>  // Tier 2
write_blocked_parities(dir, parity) -> Result<()>               // Tier 3
```

Parallel writes with Rayon—3 parity files written simultaneously.

### Manifest Writing

```rust
write_manifest(merkle_tree, hash, name, size, ...) -> Result<()>
```

JSON serialization of file metadata, Merkle tree, and encoding parameters.

## Hashing

### File Hash

- **Tier 1/2:** Streaming BLAKE3 computed during segment iteration
- **Tier 3:** Direct BLAKE3 of memory-mapped file (single pass)

### Segment Hash

```rust
hash_single_segment(segment_data, parity) -> String
```

Merkle tree of segment + parity hashes. Used as leaf in file-level tree.

## Memory Model

### Tier 1 (<10MB)

Full file loaded into memory. Acceptable for tiny files.

### Tier 2 (10MB-1GB)

- Memory-mapped if >10MB
- Streaming hash—never loads full file
- One segment buffer (32MB max)

### Tier 3 (1GB-35GB)

- Full file memory-mapped (kernel manages paging)
- Block references are `&[u8]` slices—no copying
- Parallel block processing with Rayon
- Constant ~1GB working set regardless of file size

## Performance Characteristics

| Tier | Read Strategy  | Write Strategy               | Parallelism               |
| ---- | -------------- | ---------------------------- | ------------------------- |
| 1    | Full load      | Sequential                   | None                      |
| 2    | mmap + iterate | Sequential + parallel parity | Per-segment               |
| 3    | mmap           | Parallel blocks              | Cross-block + intra-block |

**Bottleneck:** Disk I/O, not encoding. RS encoding is 1-4s per block; I/O is 70-80s per GB on HDD.

## Error Handling

All functions return `Result<T, Box<dyn std::error::Error>>`. Common failure modes:

- File not found / permission denied
- Insufficient disk space during write
- RS encoding failure (shouldn't happen with valid input)
- Directory creation failure
