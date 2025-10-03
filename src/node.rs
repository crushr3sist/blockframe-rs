use std::fmt;

#[derive(Clone)]
pub struct Node {
    pub hash_val: String,
    pub left: Option<Box<Node>>,
    pub right: Option<Box<Node>>,
}
impl fmt::Debug for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Node")
            .field("hash", &self.hash_val)
            .finish()
    }
}

impl Node {
    pub fn new(hash_val: String) -> Self {
        Node {
            hash_val,
            left: None,
            right: None,
        }
    }
    pub fn with_children(
        hash_val: String,
        left: Option<Box<Node>>,
        right: Option<Box<Node>>,
    ) -> Self {
        Node {
            hash_val,
            left: left,
            right: right,
        }
    }
}


