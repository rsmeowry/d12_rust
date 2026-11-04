use std::cell::Cell;
use std::ops::Div;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;
use glam::{FloatExt, IVec2, IVec3, UVec3, Vec3, Vec3Swizzles};
use noise::{NoiseFn, Perlin};
use rustc_hash::{FxBuildHasher, FxHashSet};
use crate::render::voxels::{index_to_uvec3, uvec3_to_index, Chunk, PaddedVolume, VoxelMeshData, VoxelMesher, VoxelWorld, CHUNK_DEPTH, CHUNK_FLAT_ARR_DEPTH, NOISE_SCALE};
use crate::render::voxels::svo::{Voxel, VoxelTree};
use crate::time::Timings;

pub const MAX_CHUNK_MESHES_PER_FRAME: usize = 16;
pub const D_CHUNK_GEN: i32 = 8;
const D_CHUNK_GEN_SQ: i32 = D_CHUNK_GEN * D_CHUNK_GEN;

#[derive(Debug)]
pub struct ChunkGenerator {
    generated: FxHashSet<IVec3>,
    tx: flume::Sender<(IVec3, VoxelTree, Option<VoxelMeshData>)>,
    rx: flume::Receiver<(IVec3, VoxelTree, Option<VoxelMeshData>)>,
    is_occupied: Cell<bool>,
    perlin: Perlin,
    timings: Arc<Timings>,
}

impl ChunkGenerator {
    pub fn new() -> Self {
        let (tx, rx) = flume::unbounded::<(IVec3, VoxelTree, Option<VoxelMeshData>)>();

        Self {
            generated: FxHashSet::with_capacity_and_hasher(32, FxBuildHasher::default()),
            tx,
            rx,
            is_occupied: Cell::new(false),
            perlin: Perlin::new(0xB217E),
            timings: Arc::new(Timings::new()),
        }
    }

    pub fn timings(&self) -> &Arc<Timings> {
        &self.timings
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
        let cam_pos_chunkwise = cam_pos.div(chunk_size).round();
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
                    let timings = self.timings.clone();
                    rayon::spawn(move || {
                        let begin = Instant::now();
                        let (tree, padded) = Self::build_chunk(chunk_abs_pos, perlin, material_data);
                        let mesh = mesher.make_vertices_and_indices(&tree, &PaddedVolume { voxels: padded }).expect("Failed to build vertices and indices for a chunk mesh");
                        timings.report(begin.elapsed().as_secs_f32() * 1000.);
                        tx.send((chunk_abs_pos, tree, mesh)).expect("Failed to send chunk generation result");
                    });
                }
            }
        }
    }

    fn build_chunk(chunk_pos: IVec3, perlin: Perlin, material_data: [u64; 8]) -> (VoxelTree, Vec<Voxel>) {
        let n = 1 << CHUNK_DEPTH;
        let origin = chunk_pos * n as i32 - IVec3::splat(PAD_LO as i32);
        let mut padded = vec![Voxel::AIR; PADDED * PADDED * PADDED];
        generate_ground(origin, PADDED, &mut padded, &perlin);

        let mut inner = vec![Voxel::AIR; n * n * n];
        for z in 0..n {
            for y in 0..n {
                let src = uvec3_to_index(UVec3::new(PAD_LO as u32, (y + PAD_LO) as u32, (z + PAD_LO) as u32), PADDED);
                let dst = uvec3_to_index(UVec3::new(0, y as u32, z as u32), n);
                inner[dst..dst + n].copy_from_slice(&padded[src..src + n]);
            }
        }

        (VoxelTree::build_from_flat_list(CHUNK_DEPTH as u32, material_data, &inner), padded)
    }
}

impl Default for ChunkGenerator {
    fn default() -> Self {
        Self::new()
    }
}

pub const PAD_LO: usize = 1;
pub const PADDED: usize = (1 << CHUNK_DEPTH) + 4;

fn generate_ground(origin: IVec3, size: usize, out: &mut [Voxel], perlin: &Perlin) {
    const MAX_HEIGHT: f32 = 32.;
    for z in 0..size {
        for x in 0..size {
            let p = (origin.xz() + IVec2::new(x as i32, z as i32)).as_dvec2() / NOISE_SCALE;
            let h = (perlin.get(p.into()) * 0.3
                + perlin.get((p / 3.).into()) * 0.6
                + perlin.get((p * 3.).into()) * 0.1).saturate() as f32 * MAX_HEIGHT;
            for y in 0..size {
                let abs_y = origin.y + y as i32;
                let d = abs_y as f32 - h;
                out[uvec3_to_index(UVec3::new(x as u32, y as u32, z as u32), size)] = Voxel {
                    density: (d.clamp(-1., 1.) * i8::MAX as f32) as i8,
                    material: if d >= 0. { 0 } else if abs_y < -6 * 32 { 1 } else { 2 },
                };
            }
        }
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