# Merkle Tree Module

Cryptographic integrity verification for segmented archives. Provides O(log n) corruption detection and partial verification without full file reads.

## Architecture

```
merkle_tree/
    │
    ├── mod.rs       # MerkleTree construction and operations
    ├── node.rs      # Node data structure
    └── manifest.rs  # Manifest file parsing (ManifestFile)
```

## Why Merkle Trees?

When you split a file into hundreds of segments, verifying integrity becomes expensive:

| Approach           | Verification Cost       | Partial Check | Corruption Localization |
| ------------------ | ----------------------- | ------------- | ----------------------- |
| Single file hash   | O(n) - read entire file | ❌ No         | ❌ No                   |
| Per-segment hashes | O(n) - check all        | ✅ Yes        | ✅ Yes                  |
| Merkle tree        | O(log n) - proof path   | ✅ Yes        | ✅ Yes                  |

Merkle trees give us:

1. **Partial verification** — Verify segment 47 without reading segments 0-46
2. **Corruption localization** — Know exactly which segment is corrupt
3. **Distributed verification** — Different machines can verify different branches
4. **Efficient updates** — Changing one leaf only updates O(log n) hashes

## Core Types

### `MerkleTree`

```rust
pub struct MerkleTree {
    pub chunks: Vec<Vec<u8>>,  // Original data (optional, can be empty)
    pub leaves: Vec<Node>,      // Leaf nodes (segment hashes)
    pub root: Node,             // Root node (file identity)
}
```

### `Node`

```rust
pub struct Node {
    pub hash_val: String,           // BLAKE3 hash
    pub left: Option<Box<Node>>,    // Left child
    pub right: Option<Box<Node>>,   // Right child
}
```

## Construction

### From Raw Data

```rust
let tree = MerkleTree::new(vec![
    segment_0.to_vec(),
    segment_1.to_vec(),
    segment_2.to_vec(),
])?;
```

Each chunk is hashed to create leaves, then paired up recursively until root.

### From Pre-computed Hashes

```rust
let hashes = vec![
    sha256(&segment_0)?,
    sha256(&segment_1)?,
    sha256(&segment_2)?,
];
let tree = MerkleTree::from_hashes(hashes)?;
```

Used when hashes are already computed during commit (avoids double-hashing).

## Tree Building Algorithm

```
Leaves:  [H(s0)] [H(s1)] [H(s2)] [H(s3)]
            ↘   ↙         ↘   ↙
Level 1:   H(H0+H1)      H(H2+H3)
                ↘       ↙
Root:         H(L1_0 + L1_1)
```

**Odd leaf handling:** If odd number of leaves, last leaf is duplicated.

```rust
pub fn build_tree(nodes: &[Node]) -> Result<Node, std::io::Error> {
    if nodes.len() == 1 {
        return Ok(nodes[0].clone());  // Base case: single node is root
    }

    // Pair nodes and hash upward
    let mut new_level = Vec::new();
    for i in (0..nodes.len()).step_by(2) {
        let left = nodes[i].clone();
        let right = if i + 1 < nodes.len() {
            nodes[i + 1].clone()
        } else {
            nodes[i].clone()  // Duplicate for odd count
        };

        let combined = sha256(&format!("{}{}", left.hash_val, right.hash_val))?;
        new_level.push(Node::with_children(combined, Some(left), Some(right)));
    }

    Self::build_tree(&new_level)  // Recurse up
}
```

## Proof Generation

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

Parsed representation of `manifest.json`:

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
