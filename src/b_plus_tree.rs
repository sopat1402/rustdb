use std::vec::Vec;
use crate::db_errors::DbError;

const MAX_SIZE: usize = 4; // only 4 for dev tests first

pub struct Entry {
    pub record_id: u16,
    pub page_id: u64,
}

pub enum Node {
    Internal {
        keys: Vec<u16>,
        children: Vec<usize>,
    },
    Leaf {
        entries: Vec<Entry>,
        next: Option<usize>,
    },
}

pub struct BPlusTree {
    pub root: usize,
    pub nodes: Vec<Node>,
    pub trash: Vec<usize>,
    pub m: usize,
}

impl BPlusTree {
    pub fn new() -> Self {
        let root = 0;
        let mut nodes: Vec<Node> = Vec::new();
        nodes.push(Node::Leaf { entries: Vec::new(), next: None });
        let trash: Vec<usize> = Vec::new();
        let m = MAX_SIZE;
        Self { root, nodes, trash, m }
    }

    pub fn search(&self, record_id: u16) -> Result<u64, DbError> {
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

    fn min_size(&self) -> usize {
        self.m / 2
    }

    fn push_node(&mut self, node: Node) -> usize {
        if let Some(id) = self.trash.pop() {
            self.nodes[id] = node;
            id
        } else {
            self.nodes.push(node);
            self.nodes.len() - 1
        }
    }

    pub fn insert(&mut self, record_id: u16, page_id: u64) -> Result<(), DbError> {
        if let Some((sep, right_id)) = self.insert_recursive(self.root, record_id, page_id)? {
            let new_root = Node::Internal {
                keys: vec![sep],
                children: vec![self.root, right_id],
            };
            let new_root_id = self.push_node(new_root);
            self.root = new_root_id;
        }
        Ok(())
    }

    fn insert_recursive(
        &mut self,
        node_id: usize,
        record_id: u16,
        page_id: u64,
    ) -> Result<Option<(u16, usize)>, DbError> {
        let is_leaf = matches!(self.nodes[node_id], Node::Leaf { .. });
        if is_leaf {
            return self.insert_into_leaf(node_id, record_id, page_id);
        }

        let idx = {
            let Node::Internal { keys, .. } = &self.nodes[node_id] else { unreachable!() };
            keys.partition_point(|&k| k <= record_id)
        };
        let child_id = {
            let Node::Internal { children, .. } = &self.nodes[node_id] else { unreachable!() };
            children[idx]
        };

        match self.insert_recursive(child_id, record_id, page_id)? {
            Some((sep, new_child_id)) => self.insert_into_internal(node_id, idx, sep, new_child_id),
            None => Ok(None),
        }
    }

    fn insert_into_leaf(
        &mut self,
        leaf_id: usize,
        record_id: u16,
        page_id: u64,
    ) -> Result<Option<(u16, usize)>, DbError> {
        {
            let Node::Leaf { entries, .. } = &mut self.nodes[leaf_id] else { unreachable!() };
            match entries.binary_search_by_key(&record_id, |e| e.record_id) {
                Ok(_) => return Err(DbError::DuplicateKey),
                Err(pos) => entries.insert(pos, Entry { record_id, page_id }),
            }
        }

        let len = match &self.nodes[leaf_id] {
            Node::Leaf { entries, .. } => entries.len(),
            _ => unreachable!(),
        };

        if len < self.m {
            Ok(None)
        } else {
            Ok(Some(self.split_leaf(leaf_id)))
        }
    }

    fn split_leaf(&mut self, leaf_id: usize) -> (u16, usize) {
        let (right_entries, old_next) = {
            let Node::Leaf { entries, next } = &mut self.nodes[leaf_id] else { unreachable!() };
            let mid = entries.len() / 2;
            (entries.split_off(mid), *next)
        };
        let sep_key = right_entries[0].record_id;

        let new_leaf = Node::Leaf { entries: right_entries, next: old_next };
        let new_id = self.push_node(new_leaf);

        let Node::Leaf { next, .. } = &mut self.nodes[leaf_id] else { unreachable!() };
        *next = Some(new_id);

        (sep_key, new_id)
    }

    fn insert_into_internal(
        &mut self,
        node_id: usize,
        idx: usize,
        sep: u16,
        new_child_id: usize,
    ) -> Result<Option<(u16, usize)>, DbError> {
        {
            let Node::Internal { keys, children } = &mut self.nodes[node_id] else { unreachable!() };
            keys.insert(idx, sep);
            children.insert(idx + 1, new_child_id);
        }

        let len = match &self.nodes[node_id] {
            Node::Internal { keys, .. } => keys.len(),
            _ => unreachable!(),
        };

        if len < self.m {
            Ok(None)
        } else {
            Ok(Some(self.split_internal(node_id)))
        }
    }

    fn split_internal(&mut self, node_id: usize) -> (u16, usize) {
        let (mid_key, right_keys, right_children) = {
            let Node::Internal { keys, children } = &mut self.nodes[node_id] else { unreachable!() };
            let mid = keys.len() / 2;
            let mid_key = keys[mid];
            let right_keys = keys.split_off(mid + 1);
            keys.pop(); // mid_key moves up, doesn't stay on either side
            let right_children = children.split_off(mid + 1);
            (mid_key, right_keys, right_children)
        };

        let new_node = Node::Internal { keys: right_keys, children: right_children };
        let new_id = self.push_node(new_node);
        (mid_key, new_id)
    }

    pub fn delete(&mut self, record_id: u16) -> Result<(), DbError> {
        let mut path: Vec<(usize, usize)> = Vec::new(); // (node_id, child_idx taken)
        let mut curr = self.root;

        loop {
            let is_leaf = matches!(self.nodes[curr], Node::Leaf { .. });
            if is_leaf {
                break;
            }
            let idx = {
                let Node::Internal { keys, .. } = &self.nodes[curr] else { unreachable!() };
                keys.partition_point(|&k| k <= record_id)
            };
            path.push((curr, idx));
            let Node::Internal { children, .. } = &self.nodes[curr] else { unreachable!() };
            curr = children[idx];
        }

        let leaf_id = curr;
        {
            let Node::Leaf { entries, .. } = &mut self.nodes[leaf_id] else { unreachable!() };
            match entries.binary_search_by_key(&record_id, |e| e.record_id) {
                Ok(pos) => { entries.remove(pos); }
                Err(_) => return Err(DbError::RecordAbsent),
            }
        }

        self.rebalance(leaf_id, &path);
        Ok(())
    }

    fn rebalance(&mut self, node_id: usize, path: &[(usize, usize)]) {
        let underflow = match &self.nodes[node_id] {
            Node::Leaf { entries, .. } => entries.len() < self.min_size(),
            Node::Internal { keys, .. } => keys.len() < self.min_size(),
        };

        if path.is_empty() {
            if let Node::Internal { keys, children } = &self.nodes[node_id] {
                if keys.is_empty() && children.len() == 1 {
                    self.root = children[0];
                }
            }
            return;
        }

        if !underflow {
            return;
        }

        let (parent_id, child_idx) = *path.last().unwrap();
        let parent_path = &path[..path.len() - 1];

        let (left_sib, right_sib) = {
            let Node::Internal { children, .. } = &self.nodes[parent_id] else { unreachable!() };
            (
                if child_idx > 0 { Some(children[child_idx - 1]) } else { None },
                if child_idx + 1 < children.len() { Some(children[child_idx + 1]) } else { None },
            )
        };

        if let Some(left_id) = left_sib {
            if self.can_lend(left_id) {
                self.borrow_from_left(parent_id, child_idx, left_id, node_id);
                return;
            }
        }
        if let Some(right_id) = right_sib {
            if self.can_lend(right_id) {
                self.borrow_from_right(parent_id, child_idx, node_id, right_id);
                return;
            }
        }

        if let Some(left_id) = left_sib {
            self.merge_nodes(parent_id, child_idx - 1, left_id, node_id);
        } else if let Some(right_id) = right_sib {
            self.merge_nodes(parent_id, child_idx, node_id, right_id);
        }

        self.rebalance(parent_id, parent_path);
    }

    fn can_lend(&self, node_id: usize) -> bool {
        match &self.nodes[node_id] {
            Node::Leaf { entries, .. } => entries.len() > self.min_size(),
            Node::Internal { keys, .. } => keys.len() > self.min_size(),
        }
    }

    fn borrow_from_left(&mut self, parent_id: usize, child_idx: usize, left_id: usize, node_id: usize) {
        let is_leaf = matches!(self.nodes[node_id], Node::Leaf { .. });

        if is_leaf {
            let borrowed = {
                let Node::Leaf { entries, .. } = &mut self.nodes[left_id] else { unreachable!() };
                entries.pop().unwrap()
            };
            let new_first_key = {
                let Node::Leaf { entries, .. } = &mut self.nodes[node_id] else { unreachable!() };
                entries.insert(0, borrowed);
                entries[0].record_id
            };
            let Node::Internal { keys, .. } = &mut self.nodes[parent_id] else { unreachable!() };
            keys[child_idx - 1] = new_first_key;
        } else {
            let (borrowed_key, borrowed_child) = {
                let Node::Internal { keys, children } = &mut self.nodes[left_id] else { unreachable!() };
                (keys.pop().unwrap(), children.pop().unwrap())
            };
            let parent_key = {
                let Node::Internal { keys, .. } = &self.nodes[parent_id] else { unreachable!() };
                keys[child_idx - 1]
            };
            {
                let Node::Internal { keys, children } = &mut self.nodes[node_id] else { unreachable!() };
                keys.insert(0, parent_key);
                children.insert(0, borrowed_child);
            }
            let Node::Internal { keys, .. } = &mut self.nodes[parent_id] else { unreachable!() };
            keys[child_idx - 1] = borrowed_key;
        }
    }

    fn borrow_from_right(&mut self, parent_id: usize, child_idx: usize, node_id: usize, right_id: usize) {
        let is_leaf = matches!(self.nodes[node_id], Node::Leaf { .. });

        if is_leaf {
            let borrowed = {
                let Node::Leaf { entries, .. } = &mut self.nodes[right_id] else { unreachable!() };
                entries.remove(0)
            };
            {
                let Node::Leaf { entries, .. } = &mut self.nodes[node_id] else { unreachable!() };
                entries.push(borrowed);
            }
            let new_right_first_key = {
                let Node::Leaf { entries, .. } = &self.nodes[right_id] else { unreachable!() };
                entries[0].record_id
            };
            let Node::Internal { keys, .. } = &mut self.nodes[parent_id] else { unreachable!() };
            keys[child_idx] = new_right_first_key;
        } else {
            let (borrowed_key, borrowed_child) = {
                let Node::Internal { keys, children } = &mut self.nodes[right_id] else { unreachable!() };
                (keys.remove(0), children.remove(0))
            };
            let parent_key = {
                let Node::Internal { keys, .. } = &self.nodes[parent_id] else { unreachable!() };
                keys[child_idx]
            };
            {
                let Node::Internal { keys, children } = &mut self.nodes[node_id] else { unreachable!() };
                keys.push(parent_key);
                children.push(borrowed_child);
            }
            let Node::Internal { keys, .. } = &mut self.nodes[parent_id] else { unreachable!() };
            keys[child_idx] = borrowed_key;
        }
    }

    fn merge_nodes(&mut self, parent_id: usize, left_idx: usize, left_id: usize, right_id: usize) {
        let is_leaf = matches!(self.nodes[left_id], Node::Leaf { .. });

        if is_leaf {
            let (right_entries, right_next) = {
                let Node::Leaf { entries, next } = &mut self.nodes[right_id] else { unreachable!() };
                (std::mem::take(entries), *next)
            };
            let Node::Leaf { entries, next } = &mut self.nodes[left_id] else { unreachable!() };
            entries.extend(right_entries);
            *next = right_next;
        } else {
            let sep_key = {
                let Node::Internal { keys, .. } = &self.nodes[parent_id] else { unreachable!() };
                keys[left_idx]
            };
            let (right_keys, right_children) = {
                let Node::Internal { keys, children } = &mut self.nodes[right_id] else { unreachable!() };
                (std::mem::take(keys), std::mem::take(children))
            };
            let Node::Internal { keys, children } = &mut self.nodes[left_id] else { unreachable!() };
            keys.push(sep_key);
            keys.extend(right_keys);
            children.extend(right_children);
        }

        let Node::Internal { keys, children } = &mut self.nodes[parent_id] else { unreachable!() };
        keys.remove(left_idx);
        children.remove(left_idx + 1);

        self.trash.push(right_id);
    }
}
