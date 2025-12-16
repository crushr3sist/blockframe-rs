# Merkle Tree & Manifest Architecture Changes

This document details the structural changes made to the BlockFrame manifest format and Merkle tree construction logic to support efficient O(1) reads and robust, granular recovery.

## 1. Manifest Schema Changes

The `manifest.json` structure has been expanded to support hierarchical hashing.

### New Structures

Two new structs were added to `src/merkle_tree/manifest.rs` to explicitly group hashes by their logical unit (Segment or Block).

```rust
pub struct SegmentHashes {
    pub data: String,        // Hash of the data.dat file
    pub parity: Vec<String>, // Hashes of parity_0.dat, parity_1.dat, etc.
}

pub struct BlockHashes {
    pub segments: Vec<String>, // Hashes of all segments in the block
    pub parity: Vec<String>,   // Hashes of block-level parity
}
```

### Updated MerkleTreeStructure

The main structure now holds these maps alongside the root.

```rust
pub struct MerkleTreeStructure {
    pub leaves: HashMap<i32, String>,           // Used for Tier 1 (Flat)
    pub segments: HashMap<usize, SegmentHashes>, // Used for Tier 2 (Hierarchical)
    pub blocks: HashMap<usize, BlockHashes>,     // Used for Tier 3 (Hierarchical)
    pub root: String,
}
```

## 2. Tree Construction Logic (Commit)

We moved from a "Flat Tree" to a "Hierarchical Tree" for Tiers 2 and 3.

### Tier 2 (Segmented)

**Old:** All data and parity hashes were mixed into a single list and hashed up.
**New:**

1. **Segment Level:** For each segment, we create a mini-Merkle tree containing `[DataHash, ParityHash0, ParityHash1, ParityHash2]`.
2. **File Level:** The roots of these mini-trees are collected and hashed to form the File Root.

**Impact:** The File Root cryptographically binds all data and parity. However, the manifest stores the intermediate hashes (`SegmentHashes`), allowing us to verify the Data segment independently without rebuilding the whole tree.

### Tier 3 (Blocked)

**Old:** Block root was just the hash of segments. Parity was not bound.
**New:**

1. **Block Level:** We create a Merkle tree containing all 30 segments AND the 3 parity shards for that block.
2. **File Level:** The roots of these block trees form the File Root.

## 3. Read Path (Filesystem)

### `read_bytes` Optimization

* **Before:** Attempted to verify data by calculating the Merkle root. This required fetching sibling hashes (parity), causing 3x read overhead and failure loops if parity was missing.
* **After:**
  * **Tier 1:** Verifies against `leaves[0]`.
  * **Tier 2:** Verifies against `segments[id].data`. This is an O(1) check.
  * **Tier 3:** **Skipped on read.** Verification requires reading the full block (30 segments), which is too slow for realtime `read()`. Integrity is deferred to `health_check`.

## 4. Recovery Logic

### Parity Verification

* **Before:** `recover_segment` assumed parity files were valid. Corrupt parity would cause the Reed-Solomon decoder to generate corrupt data silently.
* **After:**
    1. Reads parity shard.
    2. Hashes it.
    3. Compares against `manifest.segments[id].parity[n]`.
    4. Only adds to decoder if hash matches.

### Data Verification

* **After:** Once data is recovered, it is hashed and compared against `manifest.segments[id].data` to ensure the recovery was mathematically successful before returning bytes to the user.

## 5. Health Check System

The `health_check` functions in `src/filestore/health.rs` were updated to respect the new schema.

* **Tier 2 Check:** Iterates `manifest.segments`. Checks `segment_N.dat` against `.data` hash, and `parity_N.dat` against `.parity` hash.
* **Tier 3 Check:** Iterates `manifest.blocks`. Checks existence of segments and parity.

## Migration Note

**Breaking Change:** Archives created with previous versions of BlockFrame will fail to load with this version because the manifest parser expects the new `segments` or `blocks` fields which are missing in old manifests. Old archives must be re-committed.

## 6. Migration Visuals (Before vs After)

Below is a direct comparison of how the `manifest.json` structure changes for Tier 2 and Tier 3 files.

### Tier 2 (Segmented)

**Before (Flat):**
The manifest stored a single hash per segment index. This hash was a composite of data+parity, meaning you couldn't verify *just* the data without reading the parity files too.

```json
{
    "tier": 2,
    "merkle_tree": {
        "leaves": {
            "0": "8a0c64bd95259270d1fff93cd11223de60cff3c3eb6bf4958e7b419c15d84ac0",
            "1": "06c9253761d8cd7e015210b2a899677da225c1e413e275b286cbb9ddcc068207",
            "2": "46cd97c31d946430e0aa706492bfdc97c89305a4a3b803715d4c534d7637f8c5",
            ...
        },
        "root": "..."
    }
}
```

**After (Hierarchical):**
The `leaves` object is empty. Instead, we use a `segments` object where every index maps to a struct containing the specific Data hash and the specific Parity hashes.

```json
{
    "tier": 2,
    "merkle_tree": {
        "leaves": {},
        "segments": {
            "0": {
                "data": "hash_of_segment_0_data_only",
                "parity": [
                    "hash_of_segment_0_parity_0",
                    "hash_of_segment_0_parity_1",
                    "hash_of_segment_0_parity_2"
                ]
            },
            "1": {
                "data": "hash_of_segment_1_data_only",
                "parity": [ ... ]
            }
        },
        "blocks": {},
        "root": "..."
    }
}
```

### Tier 3 (Blocked)

**Before (Flat):**
The manifest stored a single hash per block (e.g., "0" for Block 0, "1" for Block 1). This hash was the root of the segments in that block, but parity was often excluded or implicit.

```json
{
    "tier": 3,
    "merkle_tree": {
        "leaves": {
            "0": "86fca16004c52f0e3249fd9e7d16c5ee1a94c4a1684ebd4d03fceb3e65b4749a",
            "1": "1c4d2b66ea31cfe012cfcff3276c1425a49387ed9e8e8adb755d6a3f7aa50328"
        },
        "root": "..."
    }
}
```

**After (Hierarchical):**
The `leaves` object is empty. We use a `blocks` object. Each block explicitly lists the hashes of all segments it contains, plus the hashes of its parity shards.

```json
{
    "tier": 3,
    "merkle_tree": {
        "leaves": {},
        "segments": {},
        "blocks": {
            "0": {
                "segments": [
                    "hash_of_seg_0",
                    "hash_of_seg_1",
                    ...
                    "hash_of_seg_29"
                ],
                "parity": [
                    "hash_of_block_0_parity_0",
                    "hash_of_block_0_parity_1",
                    "hash_of_block_0_parity_2"
                ]
            },
            "1": { ... }
        },
        "root": "..."
    }
}
```
