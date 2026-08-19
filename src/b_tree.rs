use std::vec::Vec;
use crate::db_errors::DbError;

const MAX_SIZE: usize = 4; // only 4 for dev tests first

struct Entry {
    record_id: u16,
    page_id: u64,
}

enum Node {
    Internal {
        keys: Vec<u16>,
        children: Vec<usize>,
    },
    Leaf {
        entries: Vec<Entry>,
        next: Option<usize>,
    },
}

struct BTree {
    root: usize,
    nodes: Vec<Node>,
    trash: Vec<usize>,
    m: usize,
}

impl BTree {
    fn new() -> Self {
        let root = 0;
        let mut nodes: Vec<Node> = Vec::new();
        nodes.push(Node::Leaf { entries: Vec::new(), next: None });
        let trash: Vec<usize> = Vec::new();
        let m = MAX_SIZE;
        Self { root, nodes, trash, m }
    }

    fn search(&self, record_id: u16) -> Result<u64, DbError> {
        let mut curr: usize = self.root;

        loop {
            match &self.nodes[curr] {
                Node::Internal { keys, children } => {
                    let idx = keys.partition_point(|&k| k <= record_id);
                    curr = children[idx];
                }
                Node::Leaf { entries, .. } => {
                    return match entries.binary_search_by_key(&record_id, |e| e.record_id) {
                        Ok(pos) => Ok(entries[pos].page_id),
                        Err(_) => Err(DbError::RecordAbsent),
                    };
                }
            }
        }
    }

    fn insert(&mut self, record_id: u16, page_id: u64) -> Result<(), DbError> {
        todo!("descend to leaf, insert in sorted position, split on overflow, propagate up")
    }

    fn delete(&mut self, record_id: u16) -> Result<(), DbError> {
        todo!("descend to leaf, remove entry, borrow/merge on underflow, propagate up")
    }
}
