use glam::{IVec3, UVec3};
use rustc_hash::FxHashMap;

// sparse voxel octree
#[derive(Debug, Clone)]
pub struct VoxelTree {
    depth: u32,
    nodes: Vec<VoxelNode>,
    free_nodes: Vec<u32>,
    material_data: [u64; 8] // just 8 materials for now
}

impl VoxelTree {
    pub fn new(depth: u32, material_data: [u64; 8]) -> Self {
        assert!(depth > 0 && depth < 32);
        Self {
            depth,
            nodes: vec![VoxelNode::Empty],
            material_data,
            free_nodes: Vec::new(),
        }
    }

    // side length of the whole volume
    pub fn size(&self) -> usize {
        1 << self.depth
    }

    pub fn is_in_bounds(&self, pos: UVec3) -> bool {
        let size = self.size() as u32;
        pos.x < size && pos.y < size && pos.z < size
    }

    pub fn alive_node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn get(&self, pos: UVec3) -> Option<u8> {
        if !self.is_in_bounds(pos) {
            return None;
        }

        let mut id = 0;
        let mut half_size = self.size() >> 1;
        loop {
            match self.nodes[id] {
                VoxelNode::Leaf(v) => return Some(v),
                VoxelNode::Empty => return None,
                VoxelNode::Branch(branch_idx) => {
                    id = branch_idx as usize + Self::child_idx(pos, half_size as u32);
                    half_size >>= 1; // next pow of two
                }
            }
        }
    }

    pub fn set(&mut self, pos: UVec3, value: Option<u8>) {
        let old = self.get(pos);
        if old != value {
            self.set_rec(0, pos, self.size() as u32, value);
        }
    }

    fn set_rec(&mut self, idx: usize, pos: UVec3, size: u32, value: Option<u8>) {
        // already at the lowest level of the tree
        if size == 1 {
            self.nodes[idx] = if let Some(v) = value { VoxelNode::Leaf(v) } else { VoxelNode::Empty };
            return;
        }

        let branch_children = self.require_branch(idx);
        let half_size = size >> 1;
        let target_child = branch_children + Self::child_idx(pos, half_size);
        self.set_rec(target_child, pos, half_size, value);
    }

    fn collapse(&mut self, idx: usize, children_first: usize) {
        let first = self.nodes[children_first];
        if
            (matches!(first, VoxelNode::Empty)
                && (children_first..children_first + 8).all(|it| matches!(self.nodes[it], VoxelNode::Empty)))
            || (matches!(first, VoxelNode::Leaf(l))
            && (children_first..children_first + 8).all(|it| matches!(self.nodes[it], VoxelNode::Leaf(l))))
            {
                self.free_nodes.extend(children_first as u32..children_first as u32 + 8);
                self.nodes[idx] = first;
        }
    }

    fn require_branch(&mut self, idx: usize) -> usize {
        match self.nodes[idx] {
            VoxelNode::Empty => {
                // make an array of empty voxels
                let new_idx = self.nodes.len();
                let arr: [VoxelNode; 8] = std::array::from_fn(|_| VoxelNode::Empty);
                self.nodes.extend(arr);
                self.nodes[idx] = VoxelNode::Branch(new_idx as u32);
                new_idx
            }
            VoxelNode::Leaf(v) => {
                // copy voxel 8 times
                let new_idx = self.nodes.len();
                let arr: [VoxelNode; 8] = std::array::from_fn(|_| VoxelNode::Leaf(v));
                self.nodes.extend(arr);
                self.nodes[idx] = VoxelNode::Branch(new_idx as u32);
                new_idx
            }
            // already a branch, return as-is
            VoxelNode::Branch(idx) => idx as usize
        }
    }

    fn child_idx(pos: UVec3, size: u32) -> usize {
        // bitmasks yay !!!
        // e.g. pos: 0 23 12
        // x: 0     (0b0000000)
        // y: 23    (0b0010111)
        // z: 12    (0b0001100)
        // size: 16 (0b0010000)
        // a: 0b0000000 << 0 = 0b0000000
        // b: 0b0000001 << 1 = 0b0000010
        // c: 0b0000000 << 2 = 0b0000000
        // a | b | c =         0b0000010 = 4
        ((pos.x & size != 0) as usize) | (((pos.y & size != 0) as usize) << 1) | (((pos.z & size != 0) as usize) << 2)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub enum VoxelNode {
    Empty,
    Leaf(u8), // material data
    Branch(u32), // index into a branches 8 children
}

#[cfg(test)]
mod tests {
    use std::assert_matches;
    use glam::UVec3;
    use crate::render::voxels::svo::VoxelTree;

    #[test]
    fn test_svo() {
        let mut tree = VoxelTree::new(8, [0; 8]);
        println!("{:#?}", tree);
        assert_matches!(tree.get(UVec3::new(2, 4, 6)), None);
        tree.set(UVec3::new(2, 4, 6), Some(3));
        println!("{:#?}", tree);
        assert_matches!(tree.get(UVec3::new(2, 4, 6)), Some(3));
    }
}