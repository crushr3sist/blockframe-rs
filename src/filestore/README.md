# FileStore Module

Archive management and recovery operations. Scans committed files, provides discovery, and orchestrates repair and reconstruction.

## Architecture

```
FileStore (archive manager)
    │
    ├── mod.rs       # Core operations: find, reconstruct, path utilities
    ├── health.rs    # Repair functions per tier
    └── models.rs    # File and manifest data structures
```

## Core Types

### `FileStore`

Manager for a single archive directory. Stateful—holds path to archive root.

```rust
let store = FileStore::new(Path::new("archive_directory"))?;
let files = store.get_all()?;
```

### `File`

Represents a committed file with parsed manifest and metadata.

```rust
pub struct File {
    pub file_name: String,
    pub file_data: FileData,     // Hash and path
    pub manifest: ManifestFile,  // Parsed manifest.json
}
```

### `FileData`

```rust
pub struct FileData {
    pub hash: String,   // Original file BLAKE3 hash
    pub path: String,   // Path to manifest.json
}
```

## Discovery Operations

### `get_all() -> Vec<File>`

Scans archive directory, parses all manifests, returns list of committed files.

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

### `find(filename) -> File`

Locate a specific file by name.

```rust
let dataset = store.find(&"my_dataset.bin".to_string())?;
```

### `all_files() -> Vec<PathBuf>`

Returns paths to all `manifest.json` files in archive.

## Reconstruction

### `reconstruct(file) -> Result<()>`

Rebuilds original file from segments. Auto-selects method based on tier.

```rust
store.reconstruct(&file)?;
// Output written to: reconstructed/{filename}
```

### `tiny_reconstruct` — Tier 1

Reads `data.dat` directly. No segment assembly needed.

### `segment_reconstruct` — Tier 2/3

Iterates segments in order, appends to output file.

## Repair Operations

### `repair(file) -> Result<()>`

Entry point for self-healing. Routes to tier-specific repair function.

```rust
store.repair(&file)?;  // Auto-detects tier and repairs
```

### `repair_tiny` — Tier 1

Simple recovery: if `data.dat` hash doesn't match, copy from first valid parity.

**Strategy:**

1. Read `data.dat`, compute hash
2. If hash matches manifest → done
3. Otherwise, try each `parity_N.dat`
4. Write first valid parity as `data.dat`

### `repair_segment` — Tier 2

Per-segment Reed-Solomon recovery.

**Strategy:**

1. For each segment index:
   - Read segment + 3 parity files
   - Compute combined hash
   - Compare against Merkle leaf
2. Collect corrupt segment indices
3. For each corrupt segment:
   - Read remaining parity shards
   - RS decode to recover original
   - Write recovered segment

**Limitations:** Can recover if segment missing but parity intact. Cannot recover if >1 shard lost per segment (would need RS(1,3) → can lose 2).

### `repair_blocked` — Tier 3

Block-level Reed-Solomon recovery.

**Strategy:**

1. Iterate each `blocks/block_N/` directory
2. Identify missing/corrupt segments (up to 30 per block)
3. If ≤3 missing → recoverable via RS(30,3)
4. Read valid segments + 3 block parity shards
5. RS decode to recover missing segments
6. Write recovered segments back

**Recovery capacity:** 3 segments per block can be completely lost and recovered.

```rust
// If block_0/segments/segment_21.dat is deleted:
store.repair(&file)?;
// Recovers segment_21 from remaining 29 segments + 3 parity
```

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
