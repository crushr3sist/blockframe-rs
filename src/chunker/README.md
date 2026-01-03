# Chunker: File Segmentation and Erasure Coding

The chunker transforms files into fault-tolerant archives using Reed-Solomon erasure coding and Merkle tree verification. It handles file segmentation, parity generation, and metadata persistence.

## Architecture

```
chunker/
├── commit.rs      # Entry point and tier-specific commit logic
├── generate.rs    # Reed-Solomon parity generation
├── io.rs          # Segment and parity disk writes
└── tests.rs       # End-to-end commit tests
```

The chunker is stateless. Create a `Chunker`, call `commit()`, receive a `ChunkedFile` result. No session state, no hidden mutation.

## Output: ChunkedFile

```rust
pub struct ChunkedFile {
    pub file_name: String,
    pub file_size: usize,
    pub file_hash: String,        // BLAKE3 hash of original file
    pub merkle_tree: MerkleTree,  // Integrity verification tree
    pub segment_size: usize,      // Chunk size (32MB for tiers 2-3)
    pub num_segments: usize,      // Total segment count
    pub data_shards: usize,       // Reed-Solomon data shard count
    pub parity_shards: usize,     // Reed-Solomon parity shard count
}
```

The hash provides file identity. The Merkle tree enables per-segment verification. The shard counts determine fault tolerance.

## Tier System

File size determines encoding strategy automatically.

| File Size    | Tier | Encoding        | Function           | Overhead |
| ------------ | ---- | --------------- | ------------------ | -------- |
| < 10 MB      | 1    | RS(1,3) whole   | `commit_tiny`      | 300%     |
| 10 MB - 1 GB | 2    | RS(1,3) segment | `commit_segmented` | 300%     |
| 1 GB - 35 GB | 3    | RS(30,3) block  | `commit_blocked`   | 10%      |
| > 35 GB      | 4    | Tier 2 fallback | `commit_segmented` | 300%     |

### Tier 1: commit_tiny

Files under 10 MB are encoded as a single unit with RS(1,3). The entire file is read into memory, encoded to produce 3 parity shards, and written to disk.

**Storage Layout:**

```
filename_hash/
├── manifest.json
├── data.dat         # Original file
├── parity_0.dat     # Recovery shard 0
├── parity_1.dat     # Recovery shard 1
└── parity_2.dat     # Recovery shard 2
```

**Recovery:** Any single shard (data or parity) can reconstruct the original file.

**Overhead:** 300% (4x total storage). A 1 MB file becomes 4 MB on disk.

### Tier 2: commit_segmented

Files between 10 MB and 1 GB are segmented into 32 MB chunks. Each segment receives independent RS(1,3) parity shards.

**Process:**

1. Memory-map the file
2. Iterate in 32 MB segments
3. Generate RS(1,3) parity per segment
4. Write segments and parity in parallel
5. Hash each segment (streaming BLAKE3)
6. Build Merkle tree from segment hashes
7. Write manifest

**Storage Layout:**

```
filename_hash/
├── manifest.json
├── segments/
│   ├── segment_0.dat
│   ├── segment_1.dat
│   └── ...
└── parity/
    ├── segment_0_parity_0.dat
    ├── segment_0_parity_1.dat
    ├── segment_0_parity_2.dat
    └── ...
```

**Recovery:** Per-segment recovery. If segment 5 corrupts, only segment 5 requires reconstruction.

**Overhead:** 300%

**Why 32 MB segments?** Balance between file count (OS overhead) and recovery granularity.

### Tier 3: commit_blocked

Files between 1 GB and 35 GB use block-level parity. Segments are grouped into blocks of 30, with 3 parity shards covering the entire block.

**Encoding:** RS(30,3) - 30 data segments + 3 parity segments per block.

**Recovery Capability:** Any 30 of 33 shards can reconstruct all 30 original segments. Can lose any 3 shards per block.

**Process:**

1. Memory-map entire file
2. Calculate block count (file_size / (32 MB × 30))
3. Pre-create all block directories in parallel
4. For each block:
   - Write 30 segments
   - Generate 3 parity shards from all 30 segments
   - Hash each segment
5. Build Merkle tree from segment hashes
6. Write manifest

**Storage Layout:**

```
filename_hash/
├── manifest.json
└── blocks/
    ├── block_0/
    │   ├── segments/
    │   │   ├── segment_0.dat
    │   │   ├── segment_1.dat
    │   │   └── ... (30 total)
    │   └── parity/
    │       ├── parity_0.dat
    │       ├── parity_1.dat
    │       └── parity_2.dat
    └── block_1/
        └── ...
```

**Overhead:** 10% (3/30 = 10%)

**Parallelism:** Blocks are processed in parallel using Rayon.

**Why 30 segments per block?** Optimal balance between storage efficiency (10% overhead) and recovery time.

## Reed-Solomon Erasure Coding

Reed-Solomon codes provide mathematically guaranteed reconstruction from partial data loss.

**RS(N, K):** N data shards, K parity shards. Total N+K shards. Any N shards can reconstruct all N original shards.

**Example:** RS(30,3) produces 33 total shards. Delete any 3 shards → remaining 30 can reconstruct all 30 original segments.

**Constraint:** All shards must be equal size. Last segment is padded with zeros if needed. Manifest stores original size for truncation after recovery.

**Implementation:** Uses `reed-solomon-simd` for SIMD-accelerated encoding.

## Parity Generation

### generate_parity_segmented(segment_data) → Vec<Vec<u8>>

RS(1,3) encoding for Tier 1 and Tier 2. Takes single segment, returns 3 parity shards.

### generate_parity(segments, data_shards, parity_shards) → Vec<Vec<u8>>

Flexible RS encoding for Tier 3. Handles variable shard counts and automatic padding.

```rust
// Tier 3: 30 data segments → 3 parity shards
let parity = generate_parity(&block_segments, 30, 3)?;
```

## I/O Operations

### Segment Writing

```rust
write_segment(index, dir, data) -> Result<()>
```

Buffered write using `BufWriter` with capacity matching segment size. Reduces syscalls.

### Parity Writing

```rust
write_segment_parities(segment_idx, dir, parity) -> Result<()>  // Tier 2
write_blocked_parities(dir, parity) -> Result<()>               // Tier 3
```

Parallel writes using Rayon. Three parity files written simultaneously.

### Manifest Writing

```rust
write_manifest(merkle_tree, hash, name, size, ...) -> Result<()>
```

JSON serialization of metadata, Merkle tree, and encoding parameters. Written after all segments and parity complete.

## Hashing

### File Hash

- **BLAKE3** used for all hashing (10-20x faster than SHA-256)
- **Tier 1/2:** Streaming hash computed during segment iteration
- **Tier 3:** Direct hash of memory-mapped file (single pass)

### Segment Hash

Each segment's hash is combined with its parity hashes to form Merkle tree leaves. This enables cryptographic verification of segment integrity.

## Memory Management

### Tier 1 (< 10 MB)

Full file loaded into memory with `fs::read()`. Acceptable for small files.

### Tier 2 (10 MB - 1 GB)

- File memory-mapped
- Streaming hash, never loads full file
- Single segment buffer (32 MB) in flight
- Kernel manages page eviction

### Tier 3 (1 GB - 35 GB)

- Entire file memory-mapped (kernel manages paging)
- Block processing with Rayon parallelism
- Constant working set (~1-2 GB) regardless of file size
- Peak RAM: 2-3 blocks worth (~2-3 GB) even for 35 GB file

**Why limit to 35 GB?** Beyond this, memory-mapping risks address space exhaustion or swap thrashing on some systems. Tier 4 falls back to Tier 2 strategy.

## Performance Characteristics

| Tier | Read Strategy  | Write Strategy               | Parallelism               | Bottleneck |
| ---- | -------------- | ---------------------------- | ------------------------- | ---------- |
| 1    | Full load      | Sequential                   | None                      | Negligible |
| 2    | mmap + iterate | Sequential + parallel parity | Per-segment               | Disk I/O   |
| 3    | mmap           | Parallel blocks              | Cross-block + intra-block | Disk I/O   |

**Measured Performance (HDD):**

- 1.6 GB file (Tier 3): 36 seconds, 45 MB/s
- 26.6 GB file (Tier 3): 27 minutes 23 seconds, 16.6 MB/s

**Analysis:** Disk I/O dominates. Reed-Solomon encoding completes in 1-4 seconds per block. 95% of commit time is writing to disk.

**SSD Performance:** On NVMe, expect 3-4 minutes for 26 GB file vs 27 minutes on HDD.

## Error Handling

All functions return `Result<T, Box<dyn std::error::Error>>`. No panics.

**Common Failures:**

- File not found or permission denied
- Insufficient disk space during write
- Reed-Solomon encoding failure (invalid input or memory corruption)
- Directory creation failure
- Hash mismatch during verification

**Partial Commits:**
If commit fails mid-operation, an incomplete directory with segments but no manifest will exist. The archive ignores directories without manifests. Manual cleanup required.

**Not Handled:**

- Disk hardware failures mid-write
- Power loss (manifest written last, so partial commits are detectable)
- Filesystem corruption

## Manifest Format

```json
{
  "name": "movie.mp4",
  "size": 1073741824,
  "hash": "abc123...",
  "tier": 3,
  "encoding": {
    "data_shards": 30,
    "parity_shards": 3
  },
  "merkle_tree": {
    "root": "def456...",
    "blocks": { ... },
    "segments": { ... }
  },
  "created": "2026-01-02T00:00:00Z"
}
```

The manifest is essential for reconstruction. Loss of manifest renders archive unrecoverable.
