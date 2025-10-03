use blockframe::{merkle_tree::MerkleTree, utils::dummy_data};

fn main() {
    let chunks = dummy_data();

    let tree = MerkleTree::new(chunks);

    println!("Merkle Tree Root of example.txt: {:?}", tree.get_root());

    // explore the tree
    println!("Root Node: {:?}", tree.root);
    println!("Left child Node: {:?}", tree.root.left);
    println!("Right child Node: {:?}", tree.root.right);
    println!();
    println!("Tree's Leaves: {:?}", tree.leaves);

    // Get proof for chunk 0
    let proof = tree.get_proof(0);
    println!("Proof: {:?}", proof);
    // let is_valid = tree.verify_proof(chunks[0].clone(), 0, proof, tree.get_root());
    tree.write_to_file();
}
