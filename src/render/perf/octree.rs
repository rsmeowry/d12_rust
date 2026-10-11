use glam::Vec3;
use crate::render::mesh::Mesh;
use crate::render::perf::frustum::{Frustum, AABB};
use crate::render::renderer::ObjectData;

const MAX_DEPTH: u32 = 6;
const MAX_OBJECTS: usize = 8;

#[derive(Debug, Clone)]
pub struct Octree {
    nodes: Vec<OctreeNode>,
    boxes: Vec<AABB>
}

#[derive(Debug, Clone)]
pub struct OctreeNode {
    bounds: AABB,
    first_child: Option<u32>,
    objects: Vec<u32>
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum BoxStatus {
    Outside,
    Intersects,
    Inside,
}

impl Octree {
    pub fn build(boxes: Vec<AABB>) -> Self {
        let root = OctreeNode {
            bounds: Self::cube_around(&boxes),
            first_child: None,
            objects: vec![],
        };
        let mut tree = Octree { nodes: vec![root], boxes };
        for i in 0..tree.boxes.len() as u32 {
            tree.insert(0, i, 0);
        }
        tree
    }

    pub fn query(&self, frustum: &Frustum, out: &mut Vec<u32>) {
        let mut stack = vec![0u32];
        while let Some(i) = stack.pop() {
            let node: &OctreeNode = &self.nodes[i as usize];
            match frustum.classify(&node.bounds) {
                // fully invisibly, ignoring it
                BoxStatus::Outside => {}
                // need to check children
                BoxStatus::Intersects => {
                    out.extend(node.objects.iter().copied()
                        .filter(|&o| frustum.classify(&self.boxes[o as usize]) != BoxStatus::Outside));
                    if let Some(first) = node.first_child {
                        stack.extend(first..first + 8);
                    }
                }
                // fully inside, taking it whole
                BoxStatus::Inside => {
                    let mut stack = vec![i];
                    while let Some(i) = stack.pop() {
                        let n = &self.nodes[i as usize];
                        out.extend_from_slice(&n.objects);
                        if let Some(first) = n.first_child {
                            stack.extend(first..first + 8);
                        }
                    }
                }
            }
        }
    }



    fn push_down(&mut self, node: u32, obj: u32, depth: u32) {
        let b = self.boxes[obj as usize];
        let first = self.nodes[node as usize].first_child.unwrap();
        match (first..first + 8).find(|&c| self.nodes[c as usize].bounds.contains_box(&b)) {
            Some(c) => self.insert(c, obj, depth + 1),
            None => self.nodes[node as usize].objects.push(obj)
        }
    }

    fn insert(&mut self, node: u32, obj: u32, depth: u32) {
        let n = node as usize;
        if self.nodes[n].first_child.is_none() {
            // a leaf
            if self.nodes[n].objects.len() < MAX_OBJECTS || depth == MAX_DEPTH {
                self.nodes[n].objects.push(obj);
                return;
            }

            // overfilled, need to split
            self.split_node(node as usize);
            for o in std::mem::take(&mut self.nodes[n].objects) {
                self.push_down(node, o, depth);
            }
        }
        self.push_down(node, obj, depth);
    }

    fn split_node(&mut self, child: usize) {
        let AABB { min, max } = self.nodes[child].bounds;
        let center = (min + max) * 0.5;
        let size = center - min;
        let first = self.nodes.len() as u32;
        for i in 0..8 {
            // bitwise 0bzyx
            let lo = Vec3::new(
                if i & 0b001 == 0 { min.x } else { center.x },
                if i & 0b010 == 0 { min.y } else { center.y },
                if i & 0b100 == 0 { min.z } else { center.z },
            );
            self.nodes.push(OctreeNode { bounds: AABB { min: lo, max: lo + size }, first_child: None, objects: vec![]});
        }
        self.nodes[child].first_child = Some(first);
    }

    fn cube_around(boxes: &[AABB]) -> AABB {
        let min = boxes.iter().fold(Vec3::MAX, |a, b | a.min(b.min));
        let max = boxes.iter().fold(Vec3::MIN, |a, b | a.max(b.max));
        let center = (min + max) * 0.5;
        let extends = (max - min) * 0.5;
        AABB {
            min: center - extends.max_element(), // taking max element cus we need an equal cube
            max: center + extends.max_element(),
        }
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec3;
    use crate::render::perf::frustum::AABB;
    use crate::render::perf::octree::Octree;

    #[test]
    fn every_object_once_and_inside_its_node() {
        let boxes: Vec<AABB> = (0..1000).map(|_| {
            let p = Vec3::new(rand::random(), rand::random(), rand::random()) * 100.;
            AABB { min: p, max: p + Vec3::ONE }
        }).collect();
        let tree = Octree::build(boxes.clone());

        let mut seen = vec![0; boxes.len()];
        for n in &tree.nodes {
            for &o in &n.objects {
                seen[o as usize] += 1;
                assert!(n.bounds.contains_box(&boxes[o as usize]));
            }
        }
        assert!(seen.iter().all(|&c| c == 1));
    }
}