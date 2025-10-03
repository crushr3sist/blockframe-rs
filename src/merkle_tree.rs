use crate::{node::Node, utils::sha256};
use serde_json;
use std::{
    collections::HashMap,
    fs::{self, File},
    io::BufWriter,
};

pub struct MerkleTree {
    pub chunks: Vec<Vec<u8>>,
    pub leaves: Vec<Node>,
    pub root: Node,
}

impl MerkleTree {
    pub fn new(chunks: Vec<Vec<u8>>) -> Self {
        let mut leaves: Vec<Node> = chunks
            .iter()
            .map(|chunk| Node::new(sha256(chunk)))
            .collect();

        if leaves.len() % 2 == 1 {
            if let Some(last_leaf) = leaves.last().cloned() {
                leaves.push(last_leaf);
            }
        }
        let root = Self::build_tree(&leaves);

        MerkleTree {
            chunks,
            leaves,
            root,
        }
    }

    pub fn build_tree(nodes: &[Node]) -> Node {
        if nodes.len() == 1 {
            return nodes[0].clone();
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
            let combined = sha256(&combined_hashes);
            let parent = Node::with_children(combined, Some(Box::new(left)), Some(Box::new(right)));
            new_level.push(parent);
        }
        return Self::build_tree(&new_level);
    }

    pub fn get_proof(&self, chunk_index: usize) -> Vec<String> {
        let leaves: Vec<Node> = self
            .chunks
            .iter()
            .map(|chunk| Node::new(sha256(chunk)))
            .collect();

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

                let parent_hash = sha256(&combined_hashes);

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
        return proof;
    }

    pub fn verify_proof(
        &self,
        chunk: &[u8],
        chunk_index: usize,
        proof: &[String],
        root_hash: String,
    ) -> bool {
        let mut current_hash = sha256(&chunk.to_vec());
        let mut chunk_index = chunk_index;
        for sibling_hash in proof {
            if chunk_index % 2 == 0 {
                let combined_hashes = format!("{}{}", current_hash, sibling_hash)
                    .as_bytes()
                    .to_vec();
                current_hash = sha256(&combined_hashes);
            } else {
                let combined_hashes_else = format!("{}{}", sibling_hash, current_hash)
                    .as_bytes()
                    .to_vec();
                current_hash = sha256(&combined_hashes_else);
            }
            chunk_index = chunk_index / 2;
        }
        return current_hash == root_hash;
    }

    pub fn get_root(&self) -> &str {
        return &self.root.hash_val;
    }

    pub fn get_leaves(&self) -> &Vec<Node> {
        return &self.leaves;
    }

    pub fn write_to_file(&self) {
        let mut hashmap: HashMap<usize, &str> = HashMap::new();
        for (index, hash) in self.leaves.iter().enumerate() {
            hashmap.insert(index, &hash.hash_val);
        }
        let file = File::create("manifest.json").expect("Failed to create file");
        let writer = BufWriter::new(file);
        serde_json::to_writer(writer, &hashmap).expect("Failed to write to file");
    }
    

}
