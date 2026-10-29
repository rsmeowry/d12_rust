use glam::{Vec2, Vec3};
use image::{ImageBuffer, Luma};
use crate::render::perf::frustum::{Frustum, AABB};
use crate::render::perf::octree::BoxStatus;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct NodePatch {
    pub origin: Vec2,
    pub lod: u32
}

#[derive(Debug, Clone)]
pub struct Quadtree {
    leaf_size_texels: u32,
    leaf_count: u32,
    texel_size_ws: f32,
    lods: Vec<Vec<QuadNode>>
}

#[derive(Debug, Copy, Clone)]
struct QuadNode {
    min_y: f32,
    max_y: f32,
}

impl Quadtree {
    pub fn build(height_map: &ImageBuffer<Luma<u16>, Vec<u16>>, leaf_size_texels: u32, texel_size_ws: f32, height_scale: f32) -> Self {
        let leaf_count = (height_map.width() - 1) / leaf_size_texels;
        let get_height = |x: u32, z: u32| (height_map.get_pixel(x, z).0[0] as f32 / 65535.) * height_scale;
        let minmax = |a: (f32, f32), b: (f32, f32)| (a.0.min(b.0), a.1.max(b.1));

        let leaves: Vec<QuadNode> = (0..leaf_count * leaf_count).map(|leaf| {
            let (x0, z0) = ((leaf % leaf_count) * leaf_size_texels, (leaf / leaf_count) * leaf_size_texels);

            // finding the lowest and highest terrain point for this leaf
            let mut mm = (f32::MAX, f32::MIN);
            for z_texel in z0..=z0 + leaf_size_texels {
                for x_texel in x0..=x0 + leaf_size_texels {
                    let height = get_height(x_texel, z_texel);
                    mm = minmax(mm, (height, height))
                }
            }
            QuadNode {
                min_y: mm.0,
                max_y: mm.1,
            }
        }).collect();

        let mut levels = vec![leaves];
        let mut side = leaf_count;
        while side > 1 {
            let lo = levels.last().unwrap();
            side /= 2;
            let mut level = vec![];
            for i in 0..side * side {
                let (x0, z0) = ((i % side) * 2, (i / side) * 2);
                let mut up = (f32::MAX, f32::MIN);
                for (corner_x, corner_z) in [(x0, z0), (x0 + 1, z0), (x0, z0 + 1), (x0 + 1, z0 + 1)] {
                    let node = lo[(corner_z * side * 2 + corner_x) as usize];
                    up = minmax(up, (node.min_y, node.max_y));
                }
                level.push(QuadNode { min_y: up.0, max_y: up.1 });
            }
            levels.push(level);
        }

        Self {
            leaf_size_texels,
            leaf_count,
            texel_size_ws,
            lods: levels,
        }
    }

    pub fn root_lod(&self) -> usize {
        self.lods.len() - 1
    }

    pub fn select(&self, lod: usize, x: u32, z: u32, cam_pos: Vec3, ranges: &[f32], frustum: &Frustum, out: &mut Vec<NodePatch>) -> bool {
        let b = self.lod_aabb(lod, x, z);
        let in_range = |r: f32| cam_pos.clamp(b.min, b.max).distance_squared(cam_pos) <= r * r;

        // need less detailed lod
        if !in_range(ranges[lod]) {
            return false;
        }

        // invisible, skip rendering
        if frustum.classify(&b) == BoxStatus::Outside {
            return true;
        }

        // fully detailed or parent is out of range
        let is_whole = lod == 0 || !in_range(ranges[lod - 1]);
        let half = (self.leaf_size_texels << lod) / 2;
        for i in 0..4 {
            let (center_x, center_z) = (x * 2 + i % 2, z * 2 + i / 2);
            if is_whole || !self.select(lod - 1, center_x, center_z, cam_pos, ranges, frustum, out) {
                out.push(NodePatch {
                    origin: Vec2::new((center_x * half) as f32, (center_z * half) as f32),
                    lod: lod as u32
                })
            }
        }
        true
    }

    fn lod_aabb(&self, lod: usize, x: u32, z: u32) -> AABB {
        let size = ((self.leaf_size_texels << lod) as f32) * self.texel_size_ws;
        let node = self.lods[lod][(z * (self.leaf_count) + x) as usize];
        AABB {
            min: Vec3::new(x as f32 * size, node.min_y, z as f32 * size),
            max: Vec3::new((x + 1) as f32 * size, node.max_y, (z + 1) as f32 * size),
        }
    }
}

#[cfg(test)]
mod tests {
    use image::{DynamicImage, GenericImageView, ImageBuffer, Luma};
    use crate::render::terrain::quadtree::Quadtree;

    #[test]
    fn test_quadtree_build() {
        let tex = image::open("assets/textures/terrain/terrain_height.png").unwrap();
        let qt = Quadtree::build(&tex.to_luma16(), 64, 1., 1.5);
        assert_eq!(qt.lods.len() as u32, ((tex.width() - 1) / 64).ilog2() + 1);
        for (i, lod) in qt.lods.iter().enumerate() {
            assert_eq!(lod.len() as u32, (tex.width() - 1) / 4u32.pow((i + 1) as u32));
        }
    }
}