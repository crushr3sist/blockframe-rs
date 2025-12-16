use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Serialize, Deserialize, Clone)]
pub struct Node {
    pub hash_val: String,
    pub left: Option<Box<Node>>,
    pub right: Option<Box<Node>>,
}

impl fmt::Debug for Node {
    /// Formats the node by exposing its hash value, which is often the only
    /// information required when inspecting Merkle trees during debugging.
    ///
    /// # Examples
    ///
    /// ```
    /// # use blockframe::merkle_tree::node::Node;
    /// let node = Node::new("abc123".to_string());
    /// assert!(format!("{:?}", node).contains("abc123"));
    /// ```
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Node")
            .field("hash", &self.hash_val)
            .finish()
    }
}

impl Node {
    /// Creates a leaf node that stores only a hash value.
    ///
    /// # Examples
    ///
    /// ```
    /// # use blockframe::merkle_tree::node::Node;
    /// let node = Node::new("deadbeef".to_string());
    /// assert_eq!(node.hash_val, "deadbeef");
    /// assert!(node.left.is_none());
    /// assert!(node.right.is_none());
    /// ```
    pub fn new(hash_val: String) -> Self {
        Node {
            hash_val,
            left: None,
            right: None,
        }
    }
    /// Creates an internal node with the supplied hash and optional child
    /// pointers, allowing callers to wire custom Merkle tree shapes.
    ///
    /// # Examples
    ///
    /// ```
    /// # use blockframe::merkle_tree::node::Node;
    /// let left = Node::new("left".into());
    /// let right = Node::new("right".into());
    /// let parent = Node::with_children("parent".into(), Some(Box::new(left)), Some(Box::new(right)));
    /// assert_eq!(parent.hash_val, "parent");
    /// assert!(parent.left.is_some());
    /// assert!(parent.right.is_some());
    /// ```
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
