# BlockFrame

Erasure-coded archival storage in Rust.

BlockFrame splits files into segments, generates Reed-Solomon parity shards, and stores everything with a Merkle tree manifest. If segments get corrupted or lost, the system reconstructs them from parity data without needing the original file.

The design scales from small files to multi-gigabyte datasets using a tiered encoding strategy that balances storage overhead against recovery guarantees.

---

## Architecture

```
┌──────────────────────────────────────────────────────────────────────────────────┐
│                                   PUBLIC API                                     │
│                                                                                  │
│         commit()            find()            repair()          reconstruct()    │
└────────────┬─────────────────────────────────────────────────────────────────────┘
             │
             ▼
┌──────────────────────────────────────────────────────────────────────────────────┐
│                              PROCESSING MODULES                                  │
│                                                                                  │
│  ┌─────────────────────────────────┐      ┌─────────────────────────────────┐    │
│  │           CHUNKER               │      │          FILESTORE              │    │
│  │                                 │      │                                 │    │
│  │  • commit_tiny      (Tier 1)    │      │  • get_all                      │    │
│  │  • commit_segmented (Tier 2)    │      │  • find                         │    │
│  │  • commit_blocked   (Tier 3)    │      │  • repair                       │    │
│  │  • generate_parity              │      │  • reconstruct                  │    │
│  │                                 │      │                                 │    │
│  └─────────────────────────────────┘      └─────────────────────────────────┘    │
└────────────┬─────────────────────────────────────────────────────────────────────┘
             │
             ▼
┌──────────────────────────────────────────────────────────────────────────────────┐
│                              CORE COMPONENTS                                     │
│                                                                                  │
│  ┌───────────────┐  ┌───────────────┐  ┌───────────────┐  ┌────────────────┐     │
│  │ REED-SOLOMON  │  │  MERKLE TREE  │  │   MANIFEST    │  │     UTILS      │     │
│  │               │  │               │  │               │  │                │     │
│  │ • Encoder     │  │ • build_tree  │  │ • parse       │  │ • blake3 hash  │     │
│  │ • Decoder     │  │ • get_proof   │  │ • validate    │  │ • segment_size │     │
│  │ • SIMD accel  │  │ • verify      │  │ • serialize   │  │                │     │
│  │               │  │               │  │               │  │                │     │
│  └───────────────┘  └───────────────┘  └───────────────┘  └────────────────┘     │
└────────────┬─────────────────────────────────────────────────────────────────────┘
             │
             ▼
┌──────────────────────────────────────────────────────────────────────────────────┐
│                                I/O LAYER                                         │
│                                                                                  │
│       ┌───────────────┐       ┌───────────────┐       ┌───────────────┐          │
│       │   BufWriter   │       │    memmap2    │       │     Rayon     │          │
│       │               │       │               │       │               │          │
│       │ Buffered disk │       │  Zero-copy    │       │   Parallel    │          │
│       │    writes     │       │  file reads   │       │  processing   │          │
│       │               │       │               │       │               │          │
│       └───────────────┘       └───────────────┘       └───────────────┘          │
└────────────┬─────────────────────────────────────────────────────────────────────┘
             │
             ▼
┌──────────────────────────────────────────────────────────────────────────────────┐
│                               FILE SYSTEM                                        │
│                                                                                  │
│    archive_directory/                                                            │
│    └── {filename}_{hash}/                                                        │
│        ├── manifest.json         ← Merkle root, segment hashes, metadata         │
│        ├── segments/             ← Original data split into 32MB chunks          │
│        │   └── segment_N.dat                                                     │
│        ├── parity/               ← Reed-Solomon parity shards                    │
│        │   └── parity_N.dat                                                      │
│        └── blocks/               ← Tier 3: groups of 30 segments                 │
│            └── block_N/                                                          │
└──────────────────────────────────────────────────────────────────────────────────┘
```

### Tiers

| Tier | File Size | Encoding | Overhead | Notes |
|------|-----------|----------|----------|-------|
| 1 | < 10 MB | RS(1,3) on whole file | 300% | Simple, high redundancy |
| 2 | 10 MB – 1 GB | RS(1,3) per segment | 300% | Independent segment recovery |
| 3 | 1 – 35 GB | RS(30,3) per block | 10% | 30 segments + 3 parity per block |
| 4 | > 35 GB | Hierarchical | 11-15% | Planned |

The tier is selected automatically based on file size. Smaller files use higher redundancy (simpler recovery), larger files use block-level encoding for storage efficiency.

---

## Benchmarks

Tested on Windows 11 with a mechanical HDD (~88 MB/s sequential write):

| File Size | Tier | Time | Throughput |
|-----------|------|------|------------|
| 1 GB | 2 | 70s | 14 MB/s |
| 2 GB | 3 | 77s | 26 MB/s |
| 6 GB | 3 | 290s | 21 MB/s |

Performance is I/O bound. RS encoding takes 1-4 seconds per block; the rest is disk write time. On NVMe, expect 150+ MB/s.

---

## How It Works

**Encoding:** A file is memory-mapped and split into 32MB segments. For Tier 3, segments are grouped into blocks of 30. Reed-Solomon encoding generates parity shards for each block. A Merkle tree is built from segment hashes, and everything is written to disk with a JSON manifest.

**Recovery:** The system detects corruption by comparing segment hashes against the manifest. If a segment is missing or damaged, it reads the remaining segments plus parity shards and runs RS decoding to reconstruct the original data.

---

## Storage Layout

```
archive_directory/{filename}_{hash}/
├── manifest.json           # Merkle root, hashes, metadata
├── segments/               # 32MB data chunks
│   └── segment_N.dat
├── parity/                 # RS parity shards
│   └── parity_N.dat
└── blocks/                 # Tier 3 only
    └── block_N/
        ├── segments/
        └── parity/
```

---

## Usage

```rust
use blockframe::{chunker::Chunker, filestore::FileStore};
use std::path::Path;

let chunker = Chunker::new()?;
chunker.commit(Path::new("dataset.bin"))?;

let store = FileStore::new(Path::new("archive_directory"))?;
let file = store.find(&"dataset.bin".to_string())?;
store.repair(&file)?;
```

---

## Modules

**chunker/** — File segmentation and RS encoding. Handles the commit pipeline from raw file to archived segments.

**filestore/** — Archive operations. Scans manifests, locates files, runs repair and reconstruction.

**merkle_tree/** — Hash tree construction and verification. Provides O(log n) integrity proofs.

**utils.rs** — BLAKE3 hashing and segment size calculation.

---

## Design Notes

Reed-Solomon codes provide mathematically guaranteed reconstruction. RS(30,3) means 30 data shards plus 3 parity shards—any 30 of the 33 can reconstruct the original data.

Memory-mapped I/O keeps RAM usage constant regardless of file size. The kernel handles paging; we just iterate through segments.

BLAKE3 is used for hashing (the `sha256` function name is historical). It's faster than SHA-256 and parallelizes well.

---

## Roadmap

HTTP streaming server, Tier 4 hierarchical encoding, async I/O, compression, encryption, distributed storage.

---

## Dependencies

- [reed-solomon-simd](https://github.com/AndersTrier/reed-solomon-simd)
- [blake3](https://github.com/BLAKE3-team/BLAKE3)
- [rayon](https://github.com/rayon-rs/rayon)
- [memmap2](https://github.com/RazrFalcon/memmap2-rs)
- [serde](https://serde.rs/)

---

MIT License
