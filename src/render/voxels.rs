use std::cell::{Cell, Ref, RefCell};
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Instant;
use glam::{IVec3, Mat4, UVec3, Vec3};
use noise::{NoiseFn, Perlin};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHasher};
use crate::app::RenderOpts;
use crate::render::deferred::GBuffer;
use crate::render::graph::FrameGraph;
use crate::render::graph::pass::ResLoad;
use crate::render::graph::res::ResId;
use crate::render::mesh::Mesh;
use crate::render::perf::frustum::{Frustum, AABB};
use crate::render::pipeline::desc::PipelineDesc;
use crate::render::pipeline::PipelineId;
use crate::render::renderer::{ObjectData, Renderer};
use crate::render::texture::Texture;
use crate::render::voxels::generate::{ChunkGenerator, GenerationStrategy, MAX_CHUNK_MESHES_PER_FRAME, PADDED, PAD_LO};
use crate::render::voxels::svo::{Voxel, VoxelTree};
use crate::time::Timings;
use crate::wrappers::graphics::batching::UploadBatch;
use crate::wrappers::graphics::GraphicsDevice;

pub mod svo;
pub mod naive;
pub mod generate;
pub mod dual_contour;

pub const NOISE_SCALE: f64 = 12.5;
pub const CHUNK_DEPTH: usize = 5;
pub const CHUNK_FLAT_ARR_DEPTH: usize = 32 * 32 * 32;
pub const VIEW_DISTANCE: f32 = (1 << CHUNK_DEPTH) as f32 * 8.; // 8 chunks around
pub const VIEW_DISTANCE_SQ: f32 = VIEW_DISTANCE * VIEW_DISTANCE;
pub const DROP_DISTANCE: i32 = 12; // if chunk is N chunks away, drop it
pub const DROP_DISTANCE_SQ: i32 = DROP_DISTANCE * DROP_DISTANCE;

#[derive(Debug)]
pub struct VoxelWorld<S: GenerationStrategy> {
    chunks: FxHashMap<IVec3, Chunk>,
    mesher: Arc<dyn VoxelMesher>,
    pso: PipelineId,
    pso_wireframe: PipelineId,
    pso_shadows: PipelineId,
    material_data: [u64; 8],
    materials_va: u64,
    generator: ChunkGenerator<S>,
}

impl<S: GenerationStrategy + 'static> VoxelWorld<S> {
    pub fn new(mesher: Arc<dyn VoxelMesher>, strategy: S, rd: &mut Renderer, mut material_data: Vec<u64>, materials_va: u64) -> anyhow::Result<Self> {
        let (pso, pso_wireframe, pso_shadows) = mesher.make_pipelines(rd)?;
        material_data.resize(8, 0);
        Ok(Self {
            chunks: FxHashMap::with_capacity_and_hasher(16, FxBuildHasher::default()),
            mesher,
            pso,
            pso_wireframe,
            pso_shadows,
            material_data: std::array::from_fn(|i| material_data[i]),
            materials_va,
            generator: ChunkGenerator::new(strategy),
        })
    }

    pub fn material_data(&self) -> [u64; 8] {
        self.material_data
    }

    pub fn generator_mut(&mut self) -> &mut ChunkGenerator<S> {
        &mut self.generator
    }

    pub fn generator(&self) -> &ChunkGenerator<S> {
        &self.generator
    }

    pub fn mesher(&self) -> Arc<dyn VoxelMesher> {
        self.mesher.clone()
    }

    pub fn swap_mesher(&mut self, new_mesher: Arc<dyn VoxelMesher>, rd: &mut Renderer) -> anyhow::Result<()> {
        self.mesher = new_mesher;
        let (pso, pso_wireframe, pso_shadows) = self.mesher.make_pipelines(rd)?;
        self.pso = pso;
        self.pso_wireframe = pso_wireframe;
        self.pso_shadows = pso_shadows;
        for chunk in &mut self.chunks {
            chunk.1.mark_dirty()
        }
        Ok(())
    }

    pub fn update(&mut self, gd: &GraphicsDevice, cam_pos: Vec3) -> anyhow::Result<()> {
        let chunks = self.generator.rx().try_iter().take(MAX_CHUNK_MESHES_PER_FRAME).collect::<Vec<_>>();

        let d = (1 << CHUNK_DEPTH) as f32;
        let cam_pos_chunkwise = (cam_pos / d).floor().as_ivec3();
        let mut update_batch: Option<UploadBatch> = None;
        if !chunks.is_empty() {
            update_batch = Some(gd.begin_upload()?);
            for (chunk_pos, chunk_tree, mesh) in chunks {
                if chunk_pos.distance_squared(cam_pos_chunkwise) > DROP_DISTANCE_SQ {
                    self.generator.forget(chunk_pos);
                    continue;
                }

                self.build_chunk(chunk_pos, chunk_tree, mesh, update_batch.as_mut().unwrap())?;
            }
        }

        self.drop_far_chunks(cam_pos);

        for chunk in &self.chunks {
            if chunk.1.is_dirty.get() && update_batch.is_none() {
                update_batch = Some(gd.begin_upload()?);
            }
            chunk.1.update(self, update_batch.as_mut())?;
        }

        if let Some(batch) = update_batch {
            batch.finish()?;
        }

        Ok(())
    }

    pub fn build_chunk(&mut self, chunk_pos: IVec3, tree: VoxelTree, mesh_data: Option<VoxelMeshData>, batch: &mut UploadBatch) -> anyhow::Result<()> {
        if self.chunks.contains_key(&chunk_pos) {
            return Ok(());
        }

        let chunk = Chunk::new(chunk_pos, tree, mesh_data, self, batch)?;
        self.chunks.insert(chunk_pos, chunk);
        Ok(())
    }

    pub fn drop_far_chunks(&mut self, cam_pos: Vec3) {
        let d = (1 << CHUNK_DEPTH) as f32;
        let cam_pos_chunkwise = (cam_pos / d).floor().as_ivec3();

        let generator = &mut self.generator;
        self.chunks.retain(|key, _| {
            let keep = key.distance_squared(cam_pos_chunkwise) <= DROP_DISTANCE_SQ;
            if !keep { generator.forget(*key); }
            keep
        });
    }

    pub fn select(&self, frustum: &Frustum, camera_pos: Vec3) -> (Vec<(u32, Ref<Mesh>)>, Vec<ObjectData>) {
        let meshes_objects = self.chunks.iter().filter_map(|(chunk_pos, chunk)| {
            let mesh = if let Some(mesh) = transpose_ref_opt(chunk.mesh.borrow()) { mesh } else { return None };
            let chunk_pos_ws = Vec3::new(chunk_pos.x as f32, chunk_pos.y as f32, chunk_pos.z as f32) * chunk.tree.size() as f32;
            let mat = Mat4::from_translation(chunk_pos_ws);

            if chunk_pos_ws.distance_squared(camera_pos) <= VIEW_DISTANCE_SQ && frustum.contains(mesh.aabb(), mat) {
                Some((mesh, ObjectData {
                    model: mat,
                    material_id: chunk_pos.x as u32,
                    _pad: [chunk_pos.y as u32, chunk_pos.z as u32, 0],
                }))
            } else {
                None
            }
        }).enumerate().map(|(idx, (mesh, obj))| { ((idx as u32, mesh), obj) }).collect::<(Vec<(u32, Ref<Mesh>)>, Vec<ObjectData>)>();
        meshes_objects
    }

    pub fn add_pass<'a>(&'a self, rd: &'a Renderer, gbuf: &'a GBuffer, opts: &RenderOpts, graph: &mut FrameGraph<'a>) -> anyhow::Result<()> {
        if self.chunks.is_empty() {
            return Ok(()); // in case we haven't generated any chunks here yet
        }

        let (meshes, objects) = self.select(&rd.cam.frustum(), rd.cam.pos());

        let objects_va = rd.upload_objects(&objects)?;

        let pso = if opts.use_wireframe { self.pso_wireframe } else { self.pso };
        let rs = rd.rs();
        let materials = self.materials_va;
        let pipelines = rd.pipelines();
        graph.add_pass("GPassVoxelWorld")
            .writes_depth(gbuf.scene_depth, ResLoad::Clear)
            .writes(gbuf.albedo, ResLoad::Clear)
            .writes(gbuf.normals, ResLoad::Clear)
            .writes(gbuf.materials, ResLoad::Clear)
            .writes(gbuf.scene_color, ResLoad::Clear)
            .exec(move |ctx| {
                pipelines.get(pso).bind(ctx.cmd);
                rs.set_objects(ctx.cmd, objects_va);
                rs.set_materials(ctx.cmd, materials);
                for (draw_idx, mesh) in meshes {
                    rs.set_draw_index(ctx.cmd, draw_idx);
                    mesh.draw(ctx.cmd);
                }
                Ok(())
            });

        Ok(())
    }

    pub fn add_shadow_passes<'a>(&'a self, rd: &'a Renderer, cascades: impl Iterator<Item = (ResId, Mat4)>, graph: &mut FrameGraph<'a>) -> anyhow::Result<()> {
        let pso = self.pso_shadows;
        let rs = rd.rs();
        let materials = self.materials_va;
        let pipelines = rd.pipelines();

        for (target, vp) in cascades {
            let (meshes, objects) = self.select(&Frustum::from_view_proj(vp), rd.cam.pos());
            let pass_va = rd.upload_cbuffer(&VoxelShadowPass {
                light_vp: vp,
            })?;

            let objects_va = rd.upload_objects(&objects)?;
            graph.add_pass("VoxelWorldShadows")
                .writes_depth(target, ResLoad::Preserve)
                .exec(move |ctx| {
                    pipelines.get(pso).bind(ctx.cmd);
                    rs.set_objects(ctx.cmd, objects_va);
                    rs.set_materials(ctx.cmd, materials);
                    rs.set_pass(ctx.cmd, pass_va);
                    for (draw_idx, mesh) in meshes {
                        rs.set_draw_index(ctx.cmd, draw_idx);
                        mesh.draw(ctx.cmd);
                    }
                    Ok(())
                });
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct Chunk {
    pos: IVec3,
    tree: VoxelTree,
    mesh: RefCell<Option<Mesh>>,
    is_dirty: Cell<bool>,
}

impl Chunk {
    pub fn new<S: GenerationStrategy + 'static>(pos: IVec3, tree: VoxelTree, mesh_data: Option<VoxelMeshData>, world: &VoxelWorld<S>, batch: &mut UploadBatch) -> anyhow::Result<Self> {
        let initial_mesh = mesh_data.map(|it| world.mesher.make_mesh(it, batch)).transpose()?;
        Ok(Self {
            pos,
            tree,
            mesh: RefCell::new(initial_mesh),
            is_dirty: Cell::new(false),
        })
    }

    pub fn update<S: GenerationStrategy + 'static>(&self, world: &VoxelWorld<S>, batch: Option<&mut UploadBatch>) -> anyhow::Result<()> {
        if self.is_dirty.get() {
            let vol = self.padded_volume(world);
            let mesh_data = world.mesher.make_vertices_and_indices(&self.tree, &vol)?;
            *self.mesh.borrow_mut() = mesh_data.map(|it| world.mesher.make_mesh(it, batch.unwrap())).transpose()?;
            self.is_dirty.set(false);
        }
        Ok(())
    }

    pub fn batch_mutate<F: FnOnce(&mut VoxelTree)>(&mut self, op: F) {
        op(&mut self.tree);
        self.is_dirty.set(true);
    }

    pub fn mark_dirty(&self) {
        self.is_dirty.set(true);
    }

    fn padded_volume<S: GenerationStrategy + 'static>(&self, world: &VoxelWorld<S>) -> PaddedVolume {
        let n = self.tree.size() as i32;
        let mut voxels = vec![Voxel::AIR; PADDED * PADDED * PADDED];

        // todo: probably need to fallback here to a generator once its stabilized or smth

        for dz in -1..=1 {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let npos = self.pos + IVec3::new(dx, dy, dz);
                    let Some(neighbor) = world.chunks.get(&npos) else { continue };
                    let min = (self.pos - npos) * n - IVec3::splat(PAD_LO as i32);
                    neighbor.tree.fill_window(min, PADDED, &mut voxels);
                }
            }
        }

        PaddedVolume { voxels }
    }
}

pub trait VoxelMesher: Debug + Send + Sync {
    fn make_vertices_and_indices<'a>(&self, tree: &VoxelTree, volume: &PaddedVolume) -> anyhow::Result<Option<VoxelMeshData>>;
    fn make_mesh(&self, mesh_data: VoxelMeshData, batch: &mut UploadBatch) -> anyhow::Result<Mesh>;
    fn make_pipelines(&self, rd: &mut Renderer) -> anyhow::Result<(PipelineId, PipelineId, PipelineId)>;
}

pub struct PaddedVolume {
    pub voxels: Vec<Voxel>
}

pub struct VoxelMeshData {
    pub vertices: Vec<u8>,
    pub indices: Vec<u32>,
    pub aabb: AABB,
}

#[inline]
pub fn uvec3_to_index(uvec: UVec3, depth: usize) -> usize {
    let d = depth as u32;
    return (uvec.z * (d * d) + uvec.y * (d) + uvec.x) as usize;
}

#[inline]
pub fn index_to_uvec3(idx: usize, depth: usize) -> UVec3 {
    // index = z * (depth ^ 2) + y * (depth) + x
    // so reverse is
    let z = idx / (depth * depth);
    let x_y = idx % (depth * depth);
    let y = x_y / depth;
    let x = x_y % depth;
    UVec3::new(x as u32, y as u32, z as u32)
}

#[inline]
pub fn ivec3_to_index(ivec: IVec3, depth: isize) -> usize {
    let d = depth as i32;
    (ivec.z * (d * d) + ivec.y * (d) + ivec.x) as usize
}

#[inline]
pub fn index_to_ivec3(idx: isize, depth: isize) -> IVec3 {
    // index = z * (depth ^ 2) + y * (depth) + x
    // so reverse is
    let z = idx / (depth * depth);
    let x_y = idx % (depth * depth);
    let y = x_y / depth;
    let x = x_y % depth;
    IVec3::new(x as i32, y as i32, z as i32)
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct VoxelShadowPass {
    light_vp: Mat4
}

fn transpose_ref_opt<T>(x: Ref<'_, Option<T>>) -> Option<Ref<'_, T>> {
    if x.is_none() { None }
    else { Some(Ref::map(x, |x| x.as_ref().unwrap())) }
}