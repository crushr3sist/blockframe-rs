# BlockFrame

**Your files, immortal.**

Hard drives fail. Bits rot. Cloud providers disappear. BlockFrame doesn't care—it encodes your data so that even when pieces go missing, you get everything back. No backups needed, no original file required.

This is erasure-coded storage: split files into segments, generate Reed-Solomon parity, and reconstruct anything that gets lost. The same math that keeps your data safe on enterprise storage arrays, running on your own hardware.

Works on anything from a 5MB config file to a 30GB dataset. Scales storage overhead from 10% to 300% depending on how paranoid you want to be.

---

## Architecture

```
+------------------------------------------------------------------------------+
|                                PUBLIC API                                    |
|                                                                              |
|        commit()           find()           repair()         reconstruct()    |
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
|                              FILE SYSTEM                                     |
|                                                                              |
|   archive_directory/                                                         |
|   +-- {filename}_{hash}/                                                     |
|       +-- manifest.json      <- merkle root, hashes, metadata                |
|       +-- segments/          <- original data in 32MB chunks                 |
|       +-- parity/            <- reed-solomon parity shards                   |
|       +-- blocks/            <- tier 3: groups of 30 segments                |
+------------------------------------------------------------------------------+
```

### Tiers

| Tier | File Size    | Encoding            | Overhead | What it means                       |
| ---- | ------------ | ------------------- | -------- | ----------------------------------- |
| 1    | < 10 MB      | RS(1,3) whole file  | 300%     | Lose 2 of 3 copies, still recover   |
| 2    | 10 MB – 1 GB | RS(1,3) per segment | 300%     | Each segment recovers independently |
| 3    | 1 – 35 GB    | RS(30,3) per block  | 10%      | Lose any 3 of 33 shards per block   |
| 4    | > 35 GB      | Hierarchical        | ~12%     | Coming soon                         |

Tier is picked automatically. Small files get maximum redundancy, large files get efficient block-level encoding.

---

## Performance

### Test Hardware

- **CPU:** Intel Core i5-12600KF (6P + 4E cores)
- **RAM:** 32 GB
- **Storage:** HDD (~88 MB/s sequential write)
- **OS:** Windows 11 Pro

### Measured Results

| File | Tier | Total Time | Throughput |
|------|------|------------|------------|
| 1 GB | 2 | 70 sec | 14 MB/s |
| 2 GB | 3 | 77 sec | 26 MB/s |
| 6 GB | 3 | 290 sec | 21 MB/s |

The bottleneck is disk I/O. Reed-Solomon encoding is SIMD-accelerated and runs in the order of milliseconds per segment—everything else is waiting on the hard drive.

### Projected Performance

| Storage Type | Sequential Write | Expected Throughput | 10 GB Archive |
|--------------|------------------|---------------------|---------------|
| 5400 RPM HDD | 80-100 MB/s | 15-25 MB/s | ~7 min |
| 7200 RPM HDD | 120-150 MB/s | 30-40 MB/s | ~4 min |
| SATA SSD | 400-500 MB/s | 100-150 MB/s | ~80 sec |
| NVMe SSD | 2000-3500 MB/s | 300-500 MB/s | ~25 sec |

Throughput is lower than raw disk speed due to writing multiple files (segments + parity) and Merkle tree computation. On fast storage, CPU becomes the limiter.

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
