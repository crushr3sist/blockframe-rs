# Merkle Tree: proving nothing changed

Merkle trees are the integrity backbone of Blockframe. When you have a file split into hundreds of segments, how do you know if segment 47 is corrupt without reading the entire file? Merkle trees solve this.

**Why Merkle trees?**
Because checking a single hash isnt good enough for segmented files, and checking every segment hash is too slow.

## The architecture: trees made of hashes

![alt text](MerkleTreeStructureDiagram.png)
(Binary tree diagram showing: leaves at bottom, internal nodes hashing pairs of children, root at top)

```
merkle_tree/
    │
    ├── mod.rs       # MerkleTree construction, proofs, verification
    ├── node.rs      # Node data structure (hash + optional children)
    └── manifest.rs  # ManifestFile parsing and validation
```

## Why Merkle Trees?

When you split a file into hundreds of segments, verifying integrity becomes expensive:

| Approach           | Verification Cost       | Partial Check | Corruption Localization | Why It Matters                           |
| ------------------ | ----------------------- | ------------- | ----------------------- | ---------------------------------------- |
| Single file hash   | O(n) - read entire file | ❌ No         | ❌ No                   | Slow, tells you IF corrupt but not WHERE |
| Per-segment hashes | O(n) - check all        | ✅ Yes        | ✅ Yes                  | Still O(n), no proof of correctness      |
| Merkle tree        | O(log n) - proof path   | ✅ Yes        | ✅ Yes                  | Fast + cryptographic proof               |

Merkle trees give us:

1. **Partial verification** , Verify segment 47 without reading segments 0-46
2. **Corruption localization** , Know exactly which segment corrupted ("segment 47 hash doesnt match leaf 47")
3. **Distributed verification** , Different machines can verify different branches independently
4. **Efficient updates** , Changing one segment only updates O(log n) hashes, not all of them
5. **Cryptographic proof** , You can prove segment 47 belongs to file X without revealing other segments

## What youre working with: the types

### `MerkleTree`

The full tree structure. Usually you only care about the root, but the tree holds everything.

```rust
pub struct MerkleTree {
    pub chunks: Vec<Vec<u8>>,  // Original data (optional, often empty to save memory)
    pub leaves: Vec<Node>,      // Leaf nodes (segment hashes)
    pub root: Node,             // Root node (the file's fingerprint)
}
```

**The root** is the file's identity. If two files have the same root hash, they are identical. If the root changes, something in the file changed.

**The leaves** are the segment hashes. Each leaf represents one segment (and its parity in Blockframe).

**The chunks** are rarely used, we dont store the actual data in the tree, just the hashes.

### `Node`

A single node in the tree. Can be a leaf (no children) or internal node (has children).

```rust
pub struct Node {
    pub hash_val: String,           // BLAKE3 hash (64 hex chars)
    pub left: Option<Box<Node>>,    // Left child (None for leaves)
    pub right: Option<Box<Node>>,   // Right child (None for leaves)
}
```

**Leaf nodes:** `left` and `right` are `None`, `hash_val` is the segment hash.

**Internal nodes:** `hash_val` is `BLAKE3(left.hash_val + right.hash_val)`, the hash of the concatenated child hashes.

## Building the tree: from segments to root

### From raw data

If you have the actual segment bytes, the tree will hash them for you.

```rust
let tree = MerkleTree::new(vec![
    segment_0.to_vec(),
    segment_1.to_vec(),
    segment_2.to_vec(),
])?;
```

Each chunk is hashed with BLAKE3 to create a leaf, then paired up recursively until we have a root.

### From pre-computed hashes

If you already hashed the segments during commit, dont hash them again, just build the tree from hashes.

```rust
let hashes = vec![
    sha256(&segment_0)?,  // Already computed
    sha256(&segment_1)?,
    sha256(&segment_2)?,
];
let tree = MerkleTree::from_hashes(hashes)?;
```

**Why from_hashes?** During commit, we hash segments as we write them (streaming). By the time we build the tree, we already have all the hashes, no need to re-read the files.

### How tree building works (the algorithm)

## Building the tree

```
Leaves:  [H(s0)] [H(s1)] [H(s2)] [H(s3)]
            ↘   ↙         ↘   ↙
Level 1:   H(H0+H1)      H(H2+H3)
                ↘       ↙
Root:         H(L1_0 + L1_1)
```

**The pairing logic:**

1. Start with all leaves (segment hashes)
2. Pair them up: leaf[0] + leaf[1], leaf[2] + leaf[3], ...
3. Hash each pair: `BLAKE3(left.hash + right.hash)`
4. These hashes become the next level up
5. Repeat until only one hash remains, thats the root

**Odd leaf handling:** If theres an odd number of leaves, the last one is duplicated. So for 5 leaves:

- Pair: [0,1], [2,3], [4,4]
- This ensures the tree is always complete (every level has pairs)

### The recursive build_tree function

```rust
pub fn build_tree(nodes: &[Node]) -> Result<Node, std::io::Error> {
    if nodes.len() == 1 {
        return Ok(nodes[0].clone());  // Base case: single node is the root
    }

    // Pair nodes and hash upward
    let mut new_level = Vec::new();
    for i in (0..nodes.len()).step_by(2) {
        let left = nodes[i].clone();
        let right = if i + 1 < nodes.len() {
            nodes[i + 1].clone()  // Normal case: pair with next node
        } else {
            nodes[i].clone()      // Odd count: duplicate last node
        };

        // Hash the concatenated hashes of left and right
        let combined = sha256(&format!("{}{}", left.hash_val, right.hash_val))?;
        new_level.push(Node::with_children(combined, Some(left), Some(right)));
    }

    Self::build_tree(&new_level)  // Recurse up the tree
}
```

**Example with 5 leaves:**

```
Input:  [L0, L1, L2, L3, L4]
Level 0: [L0, L1, L2, L3, L4, L4]  ← duplicate L4
Level 1: [H(L0+L1), H(L2+L3), H(L4+L4)]
Level 2: [H(P0+P1), H(P2+P2)]  ← duplicate P2
Root:    H(L1_0+L1_1)
```

The tree is always balanced, even with odd segment counts.

## Proof generation: proving a segment belongs

![alt text](MerkleProofDiagram.png)
(Visual diagram showing: leaf 47 → collect sibling hashes up the tree → proof = [sibling, uncle, great-uncle, ...] → verifier recomputes root)

### `get_proof(chunk_index) -> Vec<String>`

Returns the sibling hashes needed to verify a single leaf against the root. This is how you prove "segment 47 belongs to this file" without revealing other segments.

```rust
let tree = MerkleTree::new(segments)?;
let proof = tree.get_proof(47)?;  // Proof for segment 47

// proof contains: [sibling_hash, uncle_hash, great_uncle_hash, ...]
// Verifier can recompute path to root using only segment 47 + proof
```

**How it works:**

1. Start at leaf 47
2. To verify, you need to hash up to the root
3. At each level, you need the sibling hash (the other half of the pair)
4. Collect all sibling hashes from leaf to root
5. Thats your proof

**Example for 8 leaves, verifying leaf 5:**

```
Leaves:  [L0] [L1] [L2] [L3] [L4] [L5] [L6] [L7]
                                     ↑ (verifying this)
Level 1:    [H01]    [H23]    [H45]    [H67]
                                ↑
Proof step 1: Need sibling of L5 → L4
Proof step 2: Need sibling of H45 → H67
Proof step 3: Need sibling of subtree → H01+H23

Proof = [hash(L4), hash(H67), hash(H01+H23)]
```

**Size:** For n leaves, proof has log₂(n) hashes. For 1000 segments, proof is ~10 hashes (~640 bytes). Tiny compared to storing all 1000 hashes.

## Verification: checking the proof

### `verify_proof(chunk_hash, proof, root) -> bool`

Confirms a chunk belongs to the tree without accessing other chunks. This is the magic, you can verify segment 47 is part of file X without reading segments 0-46.

```rust
let is_valid = MerkleTree::verify_proof(
    &sha256(&segment_47)?,
    &proof,
    &tree.root.hash_val,
)?;
```

**The algorithm:**

1. Start with the chunk hash (hash of segment 47)
2. For each sibling in the proof:
   - Combine: `hash(current_hash + sibling_hash)` or `hash(sibling_hash + current_hash)` (order matters!)
   - This gives you the parent hash
   - Set `current_hash = parent_hash`, move up one level
3. After processing all proof hashes, `current_hash` should equal the root
4. If match → segment is valid. If mismatch → segment is corrupt or proof is fake

**Example verification:**

```
Segment 47 hash: abc123...
Proof: [sibling_hash_1, uncle_hash_2, ...]

Step 1: parent = hash(abc123 + sibling_hash_1)
Step 2: grandparent = hash(parent + uncle_hash_2)
...
Final: root_calculated = hash(...)

If root_calculated == known_root → VALID
If root_calculated != known_root → INVALID
```

**Why this is cryptographically secure:**

- Cant forge a proof without breaking BLAKE3 (preimage resistance)
- Cant change segment without changing its hash → changes entire proof path → wont match root
- Collision attacks would need to find two different segments with same hash (computationally infeasible)

## Serialization: storing the tree

### `get_json() -> Value`

Exports tree structure for manifest storage. We dont store the entire tree (too big), just the leaves and root.

```rust
let manifest = json!({
    "merkle_tree": tree.get_json()?,
    "original_hash": file_hash,
    // ...
});
```

**Stored format:**

```json
{
  "leaves": {
    "0": "a1b2c3...",
    "1": "d4e5f6...",
    "2": "789abc..."
  },
  "root": "def012..."
}
```

**Why only leaves + root?**
Because we can rebuild the entire tree from the leaves using `build_tree()`. The intermediate nodes are deterministic, they'll always hash to the same values. No need to store them.

### `get_root() -> String`

Returns just the root hash for quick identity comparison.

```rust
let root = tree.get_root();
if root == known_good_root {
    println!("File is intact");
}
```

**Use case:** Fast file identity check without parsing the whole manifest.

## ManifestFile: the complete package

This wraps the Merkle tree with file metadata (filename, size, tier, timestamps, etc.). See [manifest.rs](manifest.rs#L1) for the full structure.

## Proof generation: checking the proof

### `get_proof(chunk_index) -> Vec<String>`

Returns sibling hashes needed to verify a single leaf against root.

```rust
let tree = MerkleTree::new(segments)?;
let proof = tree.get_proof(47)?;  // Proof for segment 47

// proof contains: [sibling_hash, uncle_hash, ...]
// Verifier can recompute path to root using only segment 47 + proof
```

## Verification

### `verify_proof(chunk_hash, proof, root) -> bool`

Confirms a chunk belongs to the tree without accessing other chunks.

```rust
let is_valid = MerkleTree::verify_proof(
    &sha256(&segment_47)?,
    &proof,
    &tree.root.hash_val,
)?;
```

**Algorithm:**

1. Start with chunk hash
2. For each sibling in proof, combine and hash upward
3. Compare final hash to known root
4. Match = valid, mismatch = corrupt

## Serialization

### `get_json() -> Value`

Exports tree structure for manifest storage.

```rust
let manifest = json!({
    "merkle_tree": tree.get_json()?,
    "original_hash": file_hash,
    // ...
});
```

**Stored format:**

```json
{
  "leaves": {
    "0": "a1b2c3...",
    "1": "d4e5f6...",
    "2": "789abc..."
  },
  "root": "def012..."
}
```

### `get_root() -> String`

Returns root hash for quick identity comparison.

## ManifestFile

Parsed representation of `manifest.json`. This is what FileStore works with.

```rust
pub struct ManifestFile {
    pub erasure_coding: ErasureCoding,       // RS parameters (data/parity shards)
    pub merkle_tree: MerkleTreeStructure,    // Leaves + root from serialization
    pub name: String,                        // Original filename
    pub original_hash: String,               // BLAKE3 hash of original file
    pub size: i64,                           // File size in bytes
    pub time_of_creation: String,            // ISO 8601 timestamp
    pub tier: u8,                            // 1/2/3
    pub segment_size: u64,                   // Bytes per segment
}
```

### Validation

The manifest can be validated before use to catch corruption early.

```rust
impl ManifestFile {
    pub fn validate(&self) -> Result<bool, std::io::Error> {
        // Check root hash format (64 hex chars for BLAKE3)
        // Verify leaves exist and are non-empty
        // Confirm sequential indices (0, 1, 2, ... no gaps)
        // Validate each leaf hash format
    }
}
```

**Why validate?**
If the manifest is corrupt (disk error, manual edit gone wrong), we want to know before we try to use it. Better to fail fast than to waste time reconstructing with bad metadata.

## How Blockframe uses Merkle trees

### During commit (creating the tree)

When chunker commits a file, it builds the Merkle tree as it goes.

**Tier 3 example (block roots become leaves):**

```rust
// Each block gets its own Merkle tree
let block_roots: Vec<String> = blocks.par_iter()
    .map(|block| {
        // Build tree from block's 30 segments
        compute_block_merkle(block)
    })
    .collect();

// File-level tree: leaves are block roots
let file_tree = MerkleTree::from_hashes(block_roots)?;
// file_tree.root is the file's identity
```

This creates a two-level Merkle tree:

- Bottom level: segment hashes within each block
- Top level: block roots forming the file tree

### During repair (detecting corruption)

When FileStore checks segments, it compares against Merkle leaves.

```rust
// Check if segment matches expected leaf hash
let segment_data = fs::read("segment_47.dat")?;
let parity_data = vec![
    fs::read("segment_47_parity_0.dat")?,
    fs::read("segment_47_parity_1.dat")?,
    fs::read("segment_47_parity_2.dat")?,
];
let segment_hash = hash_segment_with_parity(&segment_data, &parity_data);
let expected = manifest.merkle_tree.leaves.get(&47)?;

if segment_hash != expected {
    // Segment 47 is corrupt, trigger Reed-Solomon recovery
    recover_segment(47)?;
}
```

**Why this is fast:** We only hash segment 47 + its parity (~100MB), not the entire 10GB file.

### During mount (verifying on read)

When serving segments remotely, we can provide proofs.

```rust
// Client requests segment 47
let segment = remote_source.read_segment("movie.mkv", 47)?;
let proof = remote_source.get_proof("movie.mkv", 47)?;

// Verify segment without trusting the server
let segment_hash = blake3::hash(&segment).to_hex();
let is_valid = MerkleTree::verify_proof(&segment_hash, &proof, &known_root)?;

if !is_valid {
    return Err("Server sent corrupt or fake segment");
}
```

**The security model:** You trust the root hash (you got it from a trusted source). Everything else is verified cryptographically. The server cant sneak in a corrupted segment without you detecting it.

## Performance characteristics

| Operation    | Time     | Space    | Explanation                              |
| ------------ | -------- | -------- | ---------------------------------------- |
| Construction | O(n)     | O(n)     | Hash all leaves, then O(n) hashes upward |
| Get proof    | O(log n) | O(log n) | Walk from leaf to root, collect siblings |
| Verify proof | O(log n) | O(1)     | Hash upward with proof, compare to root  |
| Get root     | O(1)     | O(1)     | Just return root.hash_val                |
| Serialize    | O(n)     | O(n)     | Write all leaves + root to JSON          |

Where n = number of leaves (segments).

**Real-world numbers:**

- 1000 segment file: proof generation < 1ms, verification < 1ms
- 10,000 segment file: proof generation ~5ms, verification ~5ms
- Building tree from 10,000 hashes: ~50ms

The bottleneck is always disk I/O (reading segments), not Merkle operations.

## Security properties: why you can trust this

**Collision resistance:**
Inherited from BLAKE3. Finding two different inputs with the same hash is computationally infeasible (2^128 operations).

**Preimage resistance:**
Cant reverse a hash to find the original data. Given a leaf hash, you cant forge the segment.

**Second preimage resistance:**
Cant find a different segment that hashes to the same value as your segment. If you change one byte, the hash changes.

**Tamper detection:**
Any modification to any segment changes its hash → changes its parent hash → propagates to root. You cant change segment 47 without changing the root, and you cant forge a new root that matches (preimage resistance).

**Proof forgery resistance:**
To fake a proof, youd need to find a segment hash that, when combined with the proof siblings, produces the correct root. This requires breaking BLAKE3's preimage resistance.

## Future: distributed verification

Merkle trees enable verification across multiple machines without trusting any single machine.

**The pattern:**

```
File with 1000 segments split across 2 machines:

Machine A: Verify left subtree (segments 0-499)
  - Build tree from segments 0-499
  - Report subtree root: abc123...

Machine B: Verify right subtree (segments 500-999)
  - Build tree from segments 500-999
  - Report subtree root: def456...

Coordinator:
  - Combine subtree roots
  - Hash: parent = BLAKE3(abc123 + def456)
  - Compare to known root
  - If match → both subtrees valid
  - If mismatch → at least one machine has corrupt data
```

Each machine only needs:

- Its segments (500 out of 1000)
- The sibling subtree root (one hash)

**Use case:** Distributed archive verification across a cluster. Each node verifies its portion, coordinator confirms the whole.

---

_For commit process details, see [chunker/README.md](../chunker/README.md)_  
_For repair and reconstruction, see [filestore/README.md](../filestore/README.md)_  
_For mounting and remote access, see [mount/README.md](../mount/README.md)_

```rust
pub struct ManifestFile {
    pub erasure_coding: ErasureCoding,
    pub merkle_tree: MerkleTreeStructure,
    pub name: String,
    pub original_hash: String,
    pub size: i64,
    pub time_of_creation: String,
    pub tier: u8,
    pub segment_size: u64,
}
```

### Validation

```rust
impl ManifestFile {
    pub fn validate(&self) -> Result<bool, std::io::Error> {
        // Check root hash format (64 hex chars)
        // Verify leaves exist
        // Confirm sequential indices (no gaps)
        // Validate each leaf hash format
    }
}
```

## Usage in BlockFrame

### During Commit

```rust
// Tier 3: Block roots become leaves
let block_roots: Vec<String> = blocks.par_iter()
    .map(|block| compute_block_merkle(block))
    .collect();

let file_tree = MerkleTree::from_hashes(block_roots)?;
// file_tree.root is the file identity
```

### During Repair

```rust
// Check if segment matches expected leaf hash
let segment_hash = hash_segment_with_parity(&segment, &parity);
let expected = manifest.merkle_tree.leaves.get(&idx)?;

if segment_hash != expected {
    // Segment corrupt, needs recovery
}
```

### During Verification

```rust
// Verify single segment without reading entire file
let proof = stored_tree.get_proof(segment_idx)?;
let is_valid = MerkleTree::verify_proof(&segment_hash, &proof, &root)?;
```

## Complexity

| Operation    | Time     | Space    |
| ------------ | -------- | -------- |
| Construction | O(n)     | O(n)     |
| Get proof    | O(log n) | O(log n) |
| Verify proof | O(log n) | O(1)     |
| Get root     | O(1)     | O(1)     |

Where n = number of leaves (segments).

## Security Properties

- **Collision resistance:** Inherited from BLAKE3
- **Preimage resistance:** Cannot forge segment to match hash
- **Second preimage resistance:** Cannot find different segment with same hash
- **Tamper detection:** Any modification changes root hash

## Future: Distributed Verification

Merkle trees enable verification across multiple machines:

```
Machine A: Verify left subtree (segments 0-499)
Machine B: Verify right subtree (segments 500-999)
Coordinator: Combine subtree roots, verify against file root
```

Each machine only needs its segments + sibling subtree root.
