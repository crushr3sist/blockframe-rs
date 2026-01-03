# FileStore: finding and fixing your files

The filestore is Blockframe's librarian. It knows where everything is, can find what you need, and when something breaks, it knows how to fix it. No chunking, no encoding, just discovery, reconstruction, and repair.

**Why do we need this?**
After the chunker has done its job and scattered your file across hundreds of segments, someone needs to keep track of it all. Thats FileStore. It scans the archive, reads manifests, and provides a clean API for "give me this file" or "fix this corrupted file".

## The architecture: stateful but simple

```
filestore/
    │
    ├── mod.rs       # Discovery, reconstruction, path utilities
    ├── health.rs    # Repair functions per tier (the self-healing magic)
    ├── models.rs    # File and manifest data structures
    └── tests.rs     # Health check and reconstruction tests
```

Unlike the chunker (stateless), FileStore holds onto the archive path. It's a manager, it knows where the archive lives and scans it on demand.

## What youre working with: the types

### `FileStore`

The archive manager. You point it at a directory, it figures out whats inside.

```rust
let store = FileStore::new(Path::new("archive_directory"))?;
let files = store.get_all()?;  // Scans and returns all committed files
```

**Stateful** means it remembers the archive path. You dont pass it to every function call. Create once, use many times.

### `File`

Represents one committed file with everything you need to work with it.

```rust
pub struct File {
    pub file_name: String,        // Original filename (e.g., "movie.mkv")
    pub file_data: FileData,      // Hash and manifest path
    pub manifest: ManifestFile,   // Parsed manifest.json (tier, segments, merkle tree, etc.)
}
```

When you call `store.find("movie.mkv")`, you get back a `File` struct. From there, you can reconstruct it, repair it, or query its metadata.

### `FileData`

The minimal info needed to locate a file in the archive.

```rust
pub struct FileData {
    pub hash: String,   // BLAKE3 hash of original file (unique identifier)
    pub path: String,   // Path to manifest.json for this file
}
```

**Why store the hash?** Filenames can collide (two files named `data.bin` with different content). The hash is the true identity. Archive directories are named `filename_hash` for uniqueness.

## Discovery: whats in the archive?

### `get_all() -> Vec<File>`

Scans the entire archive directory, finds all `manifest.json` files, parses them, returns a list of `File` objects.

```rust
let store = FileStore::new(Path::new("archive_directory"))?;
for file in store.get_all()? {
    println!("{}: {} bytes, tier {}",
        file.file_name,
        file.manifest.size,
        file.manifest.tier
    );
}
```

**How it works:**

1. Walk the archive directory tree
2. Find every `manifest.json` file
3. Parse the JSON → `ManifestFile`
4. Extract filename and hash from the directory name (`filename_hash`)
5. Build `File` struct, add to results

**Performance:** Scans in O(n) where n = number of committed files. For 1000 files, takes ~100ms on HDD, ~10ms on SSD.

### `find(filename) -> File`

Locate a specific file by name. Faster than `get_all()` if you know what you want.

```rust
let dataset = store.find(&"my_dataset.bin".to_string())?;
```

**How it works:**

1. Call `get_all()` (yeah, we scan everything, no index yet)
2. Filter for matching filename
3. Return first match or error if not found

**TODO:** Build an in-memory index on first scan to make subsequent `find()` calls O(1).

### `all_files() -> Vec<PathBuf>`

Returns just the paths to all `manifest.json` files, doesnt parse them. Useful for scripts that just need to know what exists.

```rust
let manifests = store.all_files()?;
println!("Archive contains {} files", manifests.len());
```

## Reconstruction: putting it back together

### `reconstruct(file) -> Result<()>`

Rebuilds the original file from its segments. Output goes to `reconstructed/{filename}`.

```rust
let file = store.find(&"movie.mkv".to_string())?;
store.reconstruct(&file)?;
// File written to: reconstructed/movie.mkv
```

**Auto-routing:** Checks `file.manifest.tier` and calls the appropriate reconstruction method. You dont pick, it picks for you.

### `tiny_reconstruct` , Tier 1

The easiest case: the original file is stored as `data.dat`. Just copy it.

```rust
let data = fs::read("archive/file_hash/data.dat")?;
fs::write("reconstructed/file.txt", data)?;
```

No assembly required. Done in one read + one write.

### `segment_reconstruct` , Tier 2/3

For segmented files, we reassemble by reading segments in order and concatenating them.

**The process:**

1. Open output file for writing
2. For i in 0..num_segments:
   - Read `segment_i.dat` (tier 2) or `block_X/segments/segment_Y.dat` (tier 3)
   - Append to output file
3. Verify final file hash matches manifest

**Why verify?** Because if segments are corrupted, concatenating them gives you a corrupt file. Better to detect it here than after the user tries to use it.

**Performance:** Limited by sequential disk read speed. For a 10GB file on HDD: ~60 seconds. On SSD: ~10 seconds.

## Repair: the self-healing magic

![alt text](RepairFlowDiagram.png)
(Decision tree: File → check tier → tier 1/2/3 repair strategy → RS decode if needed → write recovered data)

This is where Reed-Solomon pays off. When a segment corrupts, we dont need backups, we can mathematically reconstruct it from the surviving segments and parity shards.

### `repair(file) -> Result<()>`

Entry point for self-healing. Detects corruption, fetches parity, runs Reed-Solomon recovery, writes fixed segments back to disk.

```rust
let file = store.find(&"important_data.bin".to_string())?;
store.repair(&file)?;  // Auto-detects tier and repairs
```

**Auto-routing:** Checks tier, calls `repair_tiny`, `repair_segment`, or `repair_blocked`. You dont need to know which tier the file is, just call repair.

### `repair_tiny` , Tier 1 (the simple swap)

Tier 1 files have 1 data file (`data.dat`) and 3 parity files. If `data.dat` corrupts, we just copy a parity file over it. No Reed-Solomon decoding needed, they're all full copies.

**The strategy:**

1. Read `data.dat`, compute BLAKE3 hash
2. Compare to `manifest.original_hash`
3. If match → file is healthy, done
4. If mismatch → corruption detected, try parity files
5. For each `parity_N.dat`:
   - Read it, compute hash
   - If hash matches manifest → copy to `data.dat`, done
6. If no parity files match → unrecoverable (all 4 copies are corrupt)

**Why this works:** RS(1,3) encoding means the 3 parity files are functionally identical to the data file. Any one of them can replace the original.

**Edge case:** What if `data.dat` is missing entirely? Same process, we copy from first valid parity file.

### `repair_segment` , Tier 2 (per-segment recovery)

Tier 2 files have per-segment parity. We check each segment independently, recover the corrupt ones.

**The strategy:**

1. For each segment index `i`:
   - Read `segment_i.dat` + its 3 parity files
   - Compute combined hash (segment + parity)
   - Compare to Merkle leaf for segment `i`
2. Collect list of corrupt segment indices
3. For each corrupt segment:
   - Read the 3 parity shards for that segment
   - Use Reed-Solomon RS(1,3) decoder
   - Input: 3 parity shards (data shard is missing/corrupt)
   - Output: recovered original segment
   - Verify recovered segment hash matches manifest
   - Write recovered segment to `segment_i.dat`

**How RS(1,3) recovery works:**
Reed-Solomon with 1 data shard + 3 parity shards can recover the 1 data shard from ANY 1 of the parity shards. So even if the segment is completely gone, we can reconstruct it perfectly.

**Limitations:**

- Can recover if segment is corrupt/missing but parity is intact
- **Cannot** recover if segment + all 3 parity are corrupt (need at least 1 valid shard)
- In practice, the chance of 4 independent corruptions in the same segment is astronomically low

**Performance:** For a 1GB file with 10 corrupt segments out of 32 total segments:

- Detect corruption: ~2 seconds (hash all segments)
- Recover 10 segments: ~500ms (RS decoding is fast)
- Write recovered segments: ~300ms
- **Total: ~3 seconds**

### `repair_blocked` , Tier 3 (block-level recovery)

Tier 3 uses block-level parity: 30 segments per block, 3 parity shards for the entire block. This means we can lose up to 3 segments per block and still recover.

**The strategy:**

1. Iterate each `blocks/block_N/` directory
2. For this block:
   - Check all 30 segments for corruption (hash verification)
   - Identify which segments are missing/corrupt
3. If ≤3 segments are corrupt → recoverable via RS(30,3)
4. If >3 segments are corrupt → unrecoverable (not enough data)
5. For recoverable blocks:
   - Read all valid segments (up to 27 if 3 are corrupt)
   - Read the 3 block parity shards from `block_N/parity/`
   - Use Reed-Solomon RS(30,3) decoder
   - Input: 27 data shards + 3 parity shards = 30 total shards
   - Write recovered segments back to disk

**How RS(30,3) recovery works:**
Reed-Solomon with 30 data shards + 3 parity shards creates 33 total shards. You need ANY 30 of those 33 to reconstruct all 30 originals. So you can lose:

- 3 data segments (use 27 data + 3 parity = 30 shards)
- 2 data + 1 parity (use 28 data + 2 parity = 30 shards)
- All 3 parity (use all 30 data = 30 shards)

Any combination works as long as you have 30 valid shards total.

**Example recovery:**
Block 0 has 30 segments (segment_0 through segment_29)
Corruption detected: segment_5, segment_12, segment_21

Recovery process:
1. Read segments 0-4, 6-11, 13-20, 22-29 (27 valid segments)
2. Read parity_0.dat, parity_1.dat, parity_2.dat (3 parity shards)
3. Feed all 30 shards to RS decoder
4. Decoder outputs segments 5, 12, 21 (recovered)
5. Verify recovered segment hashes match Merkle tree
6. Write segment_5.dat, segment_12.dat, segment_21.dat


**Performance:** For a 10GB file with 3 corrupt segments in one block:

- Detect corruption: ~5 seconds (hash all segments)
- RS decode block: ~1 second (30 × 32MB segments)
- Write 3 recovered segments: ~300ms
- **Total: ~6 seconds**

**Why tier 3 repair is impressive:**
You can lose 3 out of every 30 segments (10% of the file) and still recover perfectly. Compare to tier 2 where losing 1 segment requires parity recovery, tier 3 is way more fault-tolerant for large files.

## Path utilities: finding the files on disk

The FileStore abstracts away the messy directory structure. You dont need to remember if parity is in `parity/` or `blocks/block_N/parity/`, these functions handle it.

### `get_segments_paths(file) -> Vec<PathBuf>`

Returns sorted list of all segment file paths for a file, regardless of tier.

```rust
let paths = store.get_segments_paths(&file)?;
// Tier 2: ["archive/file_hash/segments/segment_0.dat", "segment_1.dat", ...]
// Tier 3: ["archive/file_hash/blocks/block_0/segments/segment_0.dat", ...]
```

**Use case:** When you need to iterate all segments (e.g., for full verification or reconstruction).

### `get_parity_paths(file) -> Vec<PathBuf>`

Returns all parity file paths for a file.

```rust
let parity = store.get_parity_paths(&file)?;
// Tier 1: ["archive/file_hash/parity_0.dat", "parity_1.dat", "parity_2.dat"]
// Tier 2: ["archive/file_hash/parity/segment_0_parity_0.dat", ...]
// Tier 3: ["archive/file_hash/blocks/block_0/parity/parity_0.dat", ...]
```

**Use case:** When you need to verify or re-read parity data during repair.

### Tier-specific path getters

```rust
get_segment_path(file, segment_id) -> PathBuf          // Tier 2
get_block_segment_path(file, block, segment) -> PathBuf // Tier 3
get_data_path(file) -> PathBuf                          // Tier 1
get_parity_path_t1(file, parity_id) -> PathBuf
get_parity_path_t2(file, segment, parity_id) -> PathBuf
get_parity_path_t3(file, block, parity_id) -> PathBuf
```

These handle the tier-specific directory structures so you dont have to hardcode paths everywhere.

## Hash verification: trust but verify

### `hash_segment_with_parity(segment_data, parity_shards) -> String`

Computes the Merkle leaf hash for a segment + its parity shards. Used during repair to verify segments.

```rust
let segment = fs::read("segment_5.dat")?;
let parity = vec![
    fs::read("segment_5_parity_0.dat")?,
    fs::read("segment_5_parity_1.dat")?,
    fs::read("segment_5_parity_2.dat")?,
];
let combined_hash = store.hash_segment_with_parity(&segment, &parity)?;

if combined_hash != manifest.merkle_tree.leaves[5] {
    println!("Segment 5 is corrupt!");
}
```

**Why hash segment+parity together?** Because we want to detect parity corruption too. If we only hashed segments, corrupt parity would go unnoticed until we tried to use it for recovery (too late).

## Usage patterns: how to actually use this

### Full archive health check

Scan everything, repair anything broken, report results.

```rust
let store = FileStore::new(Path::new("archive_directory"))?;
for file in store.get_all()? {
    match store.repair(&file) {
        Ok(_) => println!("{}: ✓ healthy or repaired", file.file_name),
        Err(e) => println!("{}: ✗ unrecoverable - {}", file.file_name, e),
    }
}
```

**When to use:** Scheduled maintenance, after hardware issues, or before critical operations.

### Targeted repair

You know a specific file is broken, just fix it.

```rust
let store = FileStore::new(Path::new("archive_directory"))?;
let critical = store.find(&"critical_data.bin".to_string())?;
store.repair(&critical)?;
println!("Repaired critical_data.bin");
```

### Verify without repairing

Check if a file is healthy without attempting repairs.

```rust
let file = store.find(&"dataset.bin".to_string())?;
let segments = store.get_segments_paths(&file)?;

let mut corrupt_count = 0;
for (i, seg_path) in segments.iter().enumerate() {
    let seg_data = fs::read(seg_path)?;
    let actual_hash = blake3::hash(&seg_data).to_hex();
    let expected_hash = &file.manifest.merkle_tree.leaves[i];

    if actual_hash.as_str() != expected_hash {
        corrupt_count += 1;
    }
}

if corrupt_count > 0 {
    println!("{} segments corrupted, repair needed", corrupt_count);
} else {
    println!("All segments healthy");
}
```

### Bulk reconstruction

Extract all files from archive.

```rust
let store = FileStore::new(Path::new("archive_directory"))?;
for file in store.get_all()? {
    match store.reconstruct(&file) {
        Ok(_) => println!("Reconstructed: {}", file.file_name),
        Err(e) => eprintln!("Failed {}: {}", file.file_name, e),
    }
}
```

## Error handling: when things go wrong

All functions return `Result<T, Box<dyn std::error::Error>>`. Common failures:

**"File not found"**

- The file was never committed to this archive, or the manifest is missing.
- Check: does the directory exist in `archive_directory`? Does it have `manifest.json`?

**"Segment hash mismatch"**

- Segment is corrupt. If repair fails with this, parity is also corrupt → unrecoverable.
- Action: restore from backup or accept data loss.

**"Not enough shards for recovery"**

- Too many segments/parity files are corrupt. RS cant recover with the remaining shards.
- Tier 2: need at least 1 valid shard (data or parity) per segment.
- Tier 3: need at least 30 valid shards per block.

**"Manifest parse error"**

- The `manifest.json` file is corrupt or invalid JSON.
- Action: if you have a backup manifest, restore it. Otherwise, file is unrecoverable (we dont know the structure).

**"Permission denied"**

- Cant read/write files in archive or reconstructed directory.
- Check: file permissions, SELinux/AppArmor policies, disk full.

## Future improvements

- **Incremental verification:** Only check segments that havent been verified recently (track last-check timestamps).
- **Parallel repair:** Repair multiple files or blocks simultaneously (currently sequential).
- **Repair progress reporting:** Show progress bar for long repairs (tier 3 with many corrupt blocks).
- **Smart parity rebuild:** If parity is corrupt but data is fine, regenerate parity from data (cheaper than full RS decode).
- **Index caching:** Build an in-memory index of all files on first scan, make `find()` O(1) instead of O(n).

---

_For chunking details, see [chunker/README.md](../chunker/README.md)_  
_For Merkle tree internals, see [merkle_tree/README.md](../merkle_tree/README.md)_  
_For mounting and accessing files, see [mount/README.md](../mount/README.md)_

- Write recovered segments back to disk

**How RS(30,3) recovery works:**
Reed-Solomon with 30 data shards + 3 parity shards creates 33 total shards. You need ANY 30 of those 33 to reconstruct all 30 originals. So you can lose:

- 3 data segments (use 27 data + 3 parity = 30 shards)
- 2 data + 1 parity (use 28 data + 2 parity = 30 shards)
- All 3 parity (use all 30 data = 30 shards)

Any combination works as long as you have 30 valid shards total.

**Example recovery:**

```
Block 0 has 30 segments (segment_0 through segment_29)
Corruption detected: segment_5, segment_12, segment_21

Recovery process:
1. Read segments 0-4, 6-11, 13-20, 22-29 (27 valid segments)
2. Read parity_0.dat, parity_1.dat, parity_2.dat (3 parity shards)
3. Feed all 30 shards to RS decoder
4. Decoder outputs segments 5, 12, 21 (recovered)
5. Verify recovered segment hashes match Merkle tree
6. Write segment_5.dat, segment_12.dat, segment_21.dat
```

**Performance:** For a 10GB file with 3 corrupt segments in one block:

- Detect corruption: ~5 seconds (hash all segments)
- RS decode block: ~1 second (30 × 32MB segments)
- Write 3 recovered segments: ~300ms
- **Total: ~6 seconds**

**Why tier 3 repair is impressive:**
You can lose 3 out of every 30 segments (10% of the file) and still recover perfectly. Compare to tier 2 where losing 1 segment requires parity recovery, tier 3 is way more fault-tolerant for large files.

## Path Utilities

### `get_segments_paths(file) -> Vec<PathBuf>`

Returns sorted list of segment file paths.

### `get_chunks_paths(file) -> Vec<PathBuf>`

Legacy: returns chunk paths within segments (Gen 1 structure).

### `get_parity_paths(file) -> Vec<PathBuf>`

Returns parity file paths for a file.

## Hash Verification

### `hash_segment_with_parity(segment_data, parity) -> String`

Computes Merkle root of segment + parity for integrity checking.

```rust
let combined_hash = store.hash_segment_with_parity(&segment, &parity_vec)?;
if combined_hash != manifest.merkle_tree.leaves[idx] {
    // Segment is corrupt
}
```

## Usage Patterns

### Full Archive Health Check

```rust
let store = FileStore::new(Path::new("archive_directory"))?;
for file in store.get_all()? {
    match store.repair(&file) {
        Ok(_) => println!("{}: healthy or repaired", file.file_name),
        Err(e) => println!("{}: unrecoverable - {}", file.file_name, e),
    }
}
```

### Targeted Repair

```rust
let store = FileStore::new(Path::new("archive_directory"))?;
let dataset = store.find(&"critical_data.bin".to_string())?;
store.repair(&dataset)?;
store.reconstruct(&dataset)?;
```

## Error Handling

| Error          | Cause                       | Recovery            |
| -------------- | --------------------------- | ------------------- |
| File not found | Manifest missing or corrupt | Re-commit original  |
| Unrecoverable  | Too many shards lost        | Restore from backup |
| Parse error    | Malformed manifest.json     | Manual intervention |

## Storage Layout Assumed

### Tier 1

```
{archive}/
  data.dat
  parity_0.dat, parity_1.dat, parity_2.dat
  manifest.json
```

### Tier 2

```
{archive}/
  segments/
    segment_0.dat ... segment_N.dat
  parity/
    segment_0_parity_0.dat ... segment_N_parity_2.dat
  manifest.json
```

### Tier 3

```
{archive}/
  blocks/
    block_0/
      segments/segment_0.dat ... segment_29.dat
      parity/block_parity_0.dat ... block_parity_2.dat
    block_1/
      ...
  manifest.json
```
