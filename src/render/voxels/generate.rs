use std::cell::Cell;
use std::ops::Div;
use std::sync::Arc;
use std::thread::JoinHandle;
use glam::{FloatExt, IVec2, IVec3, UVec3, Vec3, Vec3Swizzles};
use noise::{NoiseFn, Perlin};
use rustc_hash::{FxBuildHasher, FxHashSet};
use crate::render::voxels::{index_to_uvec3, Chunk, VoxelMeshData, VoxelMesher, VoxelWorld, CHUNK_DEPTH, CHUNK_FLAT_ARR_DEPTH, NOISE_SCALE};
use crate::render::voxels::svo::VoxelTree;

pub const MAX_CHUNK_MESHES_PER_FRAME: usize = 8;
pub const D_CHUNK_GEN: i32 = 8;
const D_CHUNK_GEN_SQ: i32 = D_CHUNK_GEN * D_CHUNK_GEN;

#[derive(Debug)]
pub struct ChunkGenerator {
    generated: FxHashSet<IVec3>,
    tx: flume::Sender<(IVec3, VoxelTree, Option<VoxelMeshData>)>,
    rx: flume::Receiver<(IVec3, VoxelTree, Option<VoxelMeshData>)>,
    is_occupied: Cell<bool>,
    perlin: Perlin,
}

impl ChunkGenerator {
    pub fn new() -> Self {
        let (tx, rx) = flume::unbounded::<(IVec3, VoxelTree, Option<VoxelMeshData>)>();

        Self {
            generated: FxHashSet::with_capacity_and_hasher(32, FxBuildHasher::default()),
            tx,
            rx,
            is_occupied: Cell::new(false),
            perlin: Perlin::new(0xB217E)
        }
    }

    pub fn rx(&self) -> &flume::Receiver<(IVec3, VoxelTree, Option<VoxelMeshData>)> {
        &self.rx
    }

    pub fn forget(&mut self, pos: IVec3) {
        self.generated.remove(&pos);
    }

    pub fn launch(&mut self, mesher: Arc<dyn VoxelMesher>, cam_pos: Vec3, material_data: [u64; 8]) {
        if self.is_occupied.get() {
            return;
        }

        let chunk_size = (1 << CHUNK_DEPTH) as f32;
        let cam_pos_chunkwise = cam_pos.div(chunk_size).ceil();
        let cam_pos_chunkwise = IVec3::new(cam_pos_chunkwise.x as i32, cam_pos_chunkwise.y as i32, cam_pos_chunkwise.z as i32);

        for dx in -D_CHUNK_GEN..=D_CHUNK_GEN {
            for dy in -D_CHUNK_GEN..=D_CHUNK_GEN {
                for dz in -D_CHUNK_GEN..=D_CHUNK_GEN {
                    let offset = IVec3::new(dx, dy, dz);
                    let chunk_abs_pos = cam_pos_chunkwise + offset;
                    if self.generated.contains(&chunk_abs_pos) {
                        continue;
                    }

                    if offset.length_squared() > D_CHUNK_GEN_SQ {
                        continue;
                    }

                    self.generated.insert(chunk_abs_pos);

                    let tx = self.tx.clone();
                    let perlin = self.perlin.clone();
                    let mesher = mesher.clone();
                    rayon::spawn(move || {
                        let tree = Self::build_chunk(chunk_abs_pos, perlin, material_data);
                        let mesh = mesher.make_vertices_and_indices(&tree).expect("Failed to build vertices and indices for a chunk mesh");
                        tx.send((chunk_abs_pos, tree, mesh)).expect("Failed to send chunk generation result");
                    });
                }
            }
        }
    }

    fn build_chunk(chunk_pos: IVec3, perlin: Perlin, material_data: [u64; 8]) -> VoxelTree {
        let mut raw_list = [0u8; CHUNK_FLAT_ARR_DEPTH];

        generate_ground_chunk(chunk_pos, &mut raw_list, &perlin);

        let tree = VoxelTree::build_from_flat_list(CHUNK_DEPTH as u32, material_data, &raw_list);
        tree
    }
}

impl Default for ChunkGenerator {
    fn default() -> Self {
        Self::new()
    }
}

fn generate_sky_chunk(chunk_pos: IVec3, raw_list: &mut [u8], perlin: &Perlin) {
    let offset = chunk_pos * (1 << CHUNK_DEPTH);

    for i in 0..raw_list.len() {
        let pos = index_to_uvec3(i, 1 << CHUNK_DEPTH);
        let noise_pos: IVec3 = offset + IVec3::new(pos.x as i32, pos.y as i32, pos.z as i32);
        let value_a = perlin.get([(noise_pos.x as f64) / NOISE_SCALE, (noise_pos.y as f64) / NOISE_SCALE, (noise_pos.z as f64) / NOISE_SCALE]);
        let value_b = perlin.get([(noise_pos.x as f64) / NOISE_SCALE * 0.8, (noise_pos.y as f64) / NOISE_SCALE * 0.9, (noise_pos.z as f64) / NOISE_SCALE * 2.1]);
        let value_material = perlin.get([(noise_pos.x as f64) / NOISE_SCALE * 3., (noise_pos.y as f64) / NOISE_SCALE, (noise_pos.z as f64) / NOISE_SCALE * 3.]);
        if value_a * (1. - value_b) > 0.5 {
            if value_material > 0.5 {
                raw_list[i] = 2;
            } else {
                raw_list[i] = 1;
            }
        }
    }
}

fn generate_ground_chunk(chunk_pos: IVec3, raw_list: &mut [u8], perlin: &Perlin) {
    let offset = chunk_pos * (1 << CHUNK_DEPTH);

    if chunk_pos.y > 1 {
        return;
    }

    if chunk_pos.y < -6 {
        raw_list.fill(1);
        return;
    }

    let max_height: f64 = 32.;
    for i in 0..raw_list.len() {
        let pos = index_to_uvec3(i, 1 << CHUNK_DEPTH);

        let abs_pos = offset + IVec3::new(pos.x as i32, pos.y as i32, pos.z as i32);
        let noise_2d_pos = abs_pos.xz();
        let height_a = perlin.get([
            (noise_2d_pos.x as f64) / NOISE_SCALE,
            (noise_2d_pos.y as f64) / NOISE_SCALE,
        ]);
        let height_b = perlin.get([
            (noise_2d_pos.x as f64) / NOISE_SCALE / 3.,
            (noise_2d_pos.y as f64) / NOISE_SCALE / 3.,
        ]);
        let height_c = perlin.get([
            (noise_2d_pos.x as f64) / NOISE_SCALE * 3.,
            (noise_2d_pos.y as f64) / NOISE_SCALE * 3.,
        ]);
        let height = ((height_a * 0.3 + height_b * 0.6 + height_c * 0.1).saturate() * max_height).ceil() as i32;
        if abs_pos.y <= height {
            raw_list[i] = 2;
        }
    }
}