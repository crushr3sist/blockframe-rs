use crate::{merkle_tree::node::Node, utils::sha256};
use serde_json::{self, Value, json};

#[derive(Debug)]
pub struct MerkleTree {
    pub chunks: Vec<Vec<u8>>,
    pub leaves: Vec<Node>,
    pub root: Node,
}

impl MerkleTree {
    /// Constructs a [`MerkleTree`] by hashing the provided chunks and pairing
    /// them up until a single root node remains.
    ///
    /// # Examples
    ///
    /// ```
    /// # use blockframe::merkle_tree::MerkleTree;
    /// let tree = MerkleTree::new(vec![b"block".to_vec(), b"frame".to_vec()]).unwrap();
    /// assert_eq!(tree.leaves.len(), 2);
    /// assert!(!tree.get_root().unwrap().is_empty());
    /// ```
    pub fn new(chunks: Vec<Vec<u8>>) -> Result<Self, std::io::Error> {
        let mut leaves: Vec<Node> = chunks
            .iter()
            .map(|chunk| {
                let hash = sha256(chunk)?;
                Ok(Node::new(hash))
            })
            .collect::<Result<Vec<Node>, std::io::Error>>()?;

        if leaves.len() % 2 == 1 {
            if let Some(last_leaf) = leaves.last().cloned() {
                leaves.push(last_leaf);
            }
        }

        let root = Self::build_tree(&leaves)?;

        Ok(MerkleTree {
            chunks,
            leaves,
            root,
        })
    }
    /// Reconstructs a [`MerkleTree`] from precomputed leaf hashes.
    ///
    /// # Examples
    ///
    /// ```
    /// # use blockframe::merkle_tree::MerkleTree;
    /// let hashes = vec!["a".repeat(64), "b".repeat(64)];
    /// let tree = MerkleTree::from_hashes(hashes).unwrap();
    /// assert_eq!(tree.leaves.len(), 2);
    /// ```
    pub fn from_hashes(hashes: Vec<String>) -> Result<Self, std::io::Error> {
        let leaves: Vec<Node> = hashes.into_iter().map(|hash| Node::new(hash)).collect();
        let root = Self::build_tree(&leaves)?;
        Ok(MerkleTree {
            chunks: vec![],
            leaves,
            root,
        })
    }

    /// Recursively combines nodes two at a time until only the root remains.
    ///
    /// # Examples
    ///
    /// ```
    /// # use blockframe::merkle_tree::node::Node;
    /// # use blockframe::merkle_tree::MerkleTree;
    /// let nodes = vec![Node::new("a".repeat(64)), Node::new("b".repeat(64))];
    /// let root = MerkleTree::build_tree(&nodes).unwrap();
    /// assert!(!root.hash_val.is_empty());
    /// ```
    pub fn build_tree(nodes: &[Node]) -> Result<Node, std::io::Error> {
        if nodes.len() == 1 {
            return Ok(nodes[0].clone());
        }

        let mut new_level = Vec::new();

        for i in (0..nodes.len()).step_by(2) {
            let left: Node = nodes[i].clone();
            let right: Node = if i + 1 < nodes.len() {
                nodes[i + 1].clone()
            } else {
                nodes[i].clone()
            };

            let combined_hashes = format!("{}{}", left.hash_val, right.hash_val)
                .as_bytes()
                .to_vec();
            let combined = sha256(&combined_hashes)?;
            let parent = Node::with_children(combined, Some(Box::new(left)), Some(Box::new(right)));
            new_level.push(parent);
        }
        return Self::build_tree(&new_level);
    }

    /// Produces a Merkle proof for the chunk at the supplied index.
    ///
    /// # Examples
    ///
    /// ```
    /// # use blockframe::merkle_tree::MerkleTree;
    /// let tree = MerkleTree::new(vec![b"left".to_vec(), b"right".to_vec()]).unwrap();
    /// let proof = tree.get_proof(0).unwrap();
    /// assert!(!proof.is_empty());
    /// ```
    pub fn get_proof(&self, chunk_index: usize) -> Result<Vec<String>, std::io::Error> {
        let leaves: Vec<Node> = self
            .chunks
            .iter()
            .map(|chunk| {
                let hash = sha256(chunk)?;
                Ok(Node::new(hash))
            })
            .collect::<Result<Vec<Node>, std::io::Error>>()?;

        let mut index = chunk_index;
        let mut proof = Vec::new();
        let mut level = leaves;

        while level.len() > 1 {
            if level.len() % 2 == 1 {
                if let Some(last_level) = level.last().cloned() {
                    level.push(last_level);
                }
            }
            let mut next_level = Vec::new();

            for i in (0..level.len()).step_by(2) {
                let left = level[i].clone();
                let right = level[i + 1].clone();
                let combined_hashes = format!("{}{}", left.hash_val, right.hash_val)
                    .as_bytes()
                    .to_vec();

                let parent_hash = sha256(&combined_hashes)?;

                let parent = Node::with_children(
                    parent_hash,
                    Some(Box::new(left.clone())),
                    Some(Box::new(right.clone())),
                );

                next_level.push(parent);

                if i == index || i + 1 == index {
                    let slibling = if i == index { right } else { left };
                    proof.push(slibling.hash_val);

                    index = i / 2;
                }
            }
            level = next_level;
        }
        return Ok(proof);
    }

    /// Verifies that a chunk belongs to the Merkle tree given a proof and a
    /// root hash.
    ///
    /// # Examples
    ///
    /// ```
    /// # use blockframe::merkle_tree::MerkleTree;
    /// let data = vec![b"left".to_vec(), b"right".to_vec()];
    /// let tree = MerkleTree::new(data.clone()).unwrap();
    /// let proof = tree.get_proof(1).unwrap();
    /// let root = tree.get_root().unwrap().to_string();
    /// assert!(tree.verify_proof(&data[1], 1, &proof, root).unwrap());
    /// ```
    pub fn verify_proof(
        &self,
        chunk: &[u8],
        chunk_index: usize,
        proof: &[String],
        root_hash: String,
    ) -> Result<bool, std::io::Error> {
        let mut current_hash = sha256(&chunk.to_vec())?;
        let mut chunk_index = chunk_index;
        for sibling_hash in proof {
            if chunk_index % 2 == 0 {
                let combined_hashes = format!("{}{}", current_hash, sibling_hash)
                    .as_bytes()
                    .to_vec();
                current_hash = sha256(&combined_hashes)?;
            } else {
                let combined_hashes_else = format!("{}{}", sibling_hash, current_hash)
                    .as_bytes()
                    .to_vec();
                current_hash = sha256(&combined_hashes_else)?;
            }
            chunk_index = chunk_index / 2;
        }
        return Ok(current_hash == root_hash);
    }

    /// Returns the root hash of the Merkle tree as a string slice.
    ///
    /// # Examples
    ///
    /// ```
    /// # use blockframe::merkle_tree::MerkleTree;
    /// let tree = MerkleTree::new(vec![b"a".to_vec(), b"b".to_vec()]).unwrap();
    /// assert!(!tree.get_root().unwrap().is_empty());
    /// ```
    pub fn get_root(&self) -> Result<&str, std::io::Error> {
        return Ok(&self.root.hash_val);
    }

    /// Returns a reference to the vector of leaf nodes.
    ///
    /// # Examples
    ///
    /// ```
    /// # use blockframe::merkle_tree::MerkleTree;
    /// let tree = MerkleTree::new(vec![b"a".to_vec(), b"b".to_vec()]).unwrap();
    /// assert_eq!(tree.get_leaves().unwrap().len(), 2);
    /// ```
    pub fn get_leaves(&self) -> Result<&Vec<Node>, std::io::Error> {
        return Ok(&self.leaves);
    }

    /// Serialises the Merkle tree into a JSON object containing the root hash and
    /// each leaf's hash keyed by index.
    ///
    /// # Examples
    ///
    /// ```
    /// # use blockframe::merkle_tree::MerkleTree;
    /// let tree = MerkleTree::new(vec![b"a".to_vec(), b"b".to_vec()]).unwrap();
    /// let json = tree.get_json().unwrap();
    /// assert_eq!(json["root"], tree.get_root().unwrap());
    /// ```
    pub fn get_json(&self) -> Result<Value, std::io::Error> {
        let mut leaves_object = serde_json::Map::new();
        for (index, hash) in self.leaves.iter().enumerate() {
            leaves_object.insert(index.to_string(), json!(&hash.hash_val));
        }
        let merkle_tree_object = json!({
                "root": self.get_root()?,
                "leaves": leaves_object
        });

        return Ok(merkle_tree_object);
    }
}
pub mod manifest;
pub mod node;
