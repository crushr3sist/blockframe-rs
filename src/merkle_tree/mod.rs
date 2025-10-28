use crate::{merkle_tree::node::Node, utils::sha256};
use serde_json::{self, Value, json};

pub struct MerkleTree {
    pub chunks: Vec<Vec<u8>>,
    pub leaves: Vec<Node>,
    pub root: Node,
}

impl MerkleTree {
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
    pub fn from_hashes(hashes: Vec<String>) -> Result<Self, std::io::Error> {
        let leaves: Vec<Node> = hashes.into_iter().map(|hash| Node::new(hash)).collect();
        let root = Self::build_tree(&leaves)?;
        Ok(MerkleTree {
            chunks: vec![],
            leaves,
            root,
        })
    }

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

    pub fn get_root(&self) -> Result<&str, std::io::Error> {
        return Ok(&self.root.hash_val);
    }

    pub fn get_leaves(&self) -> Result<&Vec<Node>, std::io::Error> {
        return Ok(&self.leaves);
    }

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
