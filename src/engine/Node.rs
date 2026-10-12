#[derive(Debug)]
pub(crate) struct Node {
    /// neighbors[layer] = connected node IDs.
    pub neighbors: Vec<Vec<usize>>,
    /// Soft-deleted nodes stay in the graph for navigation.
    pub deleted: bool,
}

impl Node {
    pub fn new(level: usize) -> Self {
        Self {
            neighbors: vec![Vec::new(); level + 1],
            deleted: false,
        }
    }
}
