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
        assert!(depth > 0 && depth <= 32);
        Self {
            depth,
            nodes: vec![VoxelNode::Empty],
            material_data,
            free_nodes: Vec::new(),
        }
    }

    pub fn build_from_flat_list(depth: u32, material_data: [u64; 8], list: &[Voxel]) -> Self {
        assert!(depth > 0 && depth <= 32);

        // now need to split existing flat list into nodes, bottom-up
        let mut tree = Self {
            depth,
            nodes: vec![VoxelNode::Empty],
            material_data,
            free_nodes: Vec::new(),
        };
        let root = tree.build(list, UVec3::ZERO, tree.size());
        tree.nodes[0] = root;
        tree
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
        self.nodes.len() - self.free_nodes.len() * 8
    }

    pub fn get(&self, pos: UVec3) -> Option<Voxel> {
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

    pub fn set(&mut self, pos: UVec3, value: Option<Voxel>) {
        let old = self.get(pos);
        if old != value {
            self.set_rec(0, pos, self.size() as u32, value);
        }
    }

    pub fn flatten_tree(&self) -> Vec<Voxel> {
        let d = self.size();
        let mut out = vec![Voxel { density: 0, material: 0 }; d * d * d];
        self.fill_dense(0, [0; 3], self.size() as i64, [0; 3], &mut out);
        out
    }

    fn build(&mut self, dense: &[Voxel], origin: UVec3, size: usize) -> VoxelNode {
        if size == 1 {
            let n = self.size();
            return match dense[origin.x as usize + (origin.y as usize * n) + (origin.z as usize * n * n)] {
                Voxel { density: 0, .. } => VoxelNode::Empty,
                other => VoxelNode::Leaf(other)
            }
        }

        let half_size = size / 2;
        let children: [VoxelNode; 8] = std::array::from_fn(|i| {
            let o = UVec3::new((origin.x as usize + (i & 1) * half_size) as u32,
                               (origin.y as usize + ((i >> 1) & 1) * half_size) as u32,
                               (origin.z as usize + ((i >> 2) & 1) * half_size) as u32);
            self.build(dense, o, half_size)
        });

        // a solid 2x2 cube, we dont need branching here
        if !matches!(children[0], VoxelNode::Branch(_)) && children.iter().all(|c| *c == children[0]) {
            return children[0];
        }
        let base = self.nodes.len();
        self.nodes.extend(children);
        VoxelNode::Branch(base as u32)
    }

    fn fill_dense(&self, idx: usize, origin: [i64; 3], size: i64, min: [i64; 3], dst: &mut Vec<Voxel>) {
        // overlapping chunk with node's box
        let d = self.size();

        let lo: [i64; 3] = std::array::from_fn(|a| origin[a].max(min[a]));
        let hi: [i64; 3] = std::array::from_fn(|a| (origin[a] + size).min(min[a] + d as i64));

        if (0..3).any(|a| lo[a] >= hi[a]) {
            return; // doesn't touch the chunk
        }


        match self.nodes[idx] {
            VoxelNode::Empty => {},
            VoxelNode::Leaf(v) => {
                // just a cube, fill it
                for z in lo[2]..hi[2] {
                    for y in lo[1]..hi[1] {
                        let row = d * ((y - min[1]) as usize + d * (z - min[2]) as usize);
                        let (x0, x1) = ((lo[0] - min[0]) as usize, (hi[0] - min[0]) as usize);
                        dst[row + x0..row + x1].fill(Voxel { material: v.material, density: v.density });
                    }
                }
            },
            VoxelNode::Branch(first_idx) => {
                let half_size = size >> 1;
                for child in 0..8 {
                    let origin = [
                        origin[0] + if child & 1 != 0 { half_size } else { 0 },
                        origin[1] + if child & 2 != 0 { half_size } else { 0 },
                        origin[2] + if child & 4 != 0 { half_size } else { 0 },
                    ];
                    self.fill_dense(first_idx as usize + child, origin, half_size, min, dst)
                }
            }
        }
    }

    fn set_rec(&mut self, idx: usize, pos: UVec3, size: u32, value: Option<Voxel>) {
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
            || (matches!(first, VoxelNode::Leaf(_))
            && (children_first..children_first + 8).all(|it| self.nodes[it] == first))
            {
                self.free_nodes.push(children_first as u32);
                self.nodes[idx] = first;
        }
    }

    fn require_branch(&mut self, idx: usize) -> usize {
        match self.nodes[idx] {
            // already a branch, return as-is
            VoxelNode::Branch(idx) => idx as usize,
            leaf => {
                // copy leaf 8 times
                let new_idx = self.alloc_block(leaf);
                self.nodes[idx] = VoxelNode::Branch(new_idx as u32);
                new_idx
            }
        }
    }

    fn alloc_block(&mut self, node: VoxelNode) -> usize {
        if let Some(first_empty) = self.free_nodes.pop() {
            self.nodes[first_empty as usize..first_empty as usize + 8].fill(node);
            first_empty as usize
        } else {
            let base = self.nodes.len();
            self.nodes.extend([node; 8]);
            base
        }
    }

    fn free_subtree(&mut self, idx: usize) {
        if let VoxelNode::Branch(first) = self.nodes[idx] {
            for idx in first as usize..first as usize + 8 {
                self.free_subtree(idx);
                self.free_nodes.push(idx as u32);
            }
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
    Leaf(Voxel), // density + material data
    Branch(u32), // index into a branches 8 children
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct Voxel {
    pub material: u8,
    pub density: i8,
}

#[cfg(test)]
mod tests {
    use std::assert_matches;
    use glam::UVec3;
    use crate::render::voxels::svo::{Voxel, VoxelNode, VoxelTree};

    #[test]
    fn test_svo() {
        let mut tree = VoxelTree::new(5, [0; 8]);
        assert_matches!(tree.get(UVec3::new(2, 4, 6)), None);
        tree.set(UVec3::new(2, 4, 6), Some(Voxel { material: 3, density: 1 }));
        assert_matches!(tree.get(UVec3::new(2, 4, 6)), Some(Voxel { material: 3, density: 1 }));
        let flattened = tree.flatten_tree();
        let d = 1 << 5;
        assert_eq!(flattened.len(), d * d * d);
        assert_eq!(flattened[6 * (d * d) + 4 * d + 2], Voxel { material: 3, density: 1 });
    }
}