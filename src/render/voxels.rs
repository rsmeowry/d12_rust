use std::cell::{Cell, Ref, RefCell};
use std::fmt::Debug;
use glam::{IVec3, Mat4, UVec3, Vec3};
use noise::{NoiseFn, Perlin};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHasher};
use crate::app::RenderOpts;
use crate::render::deferred::GBuffer;
use crate::render::graph::FrameGraph;
use crate::render::graph::pass::ResLoad;
use crate::render::graph::res::ResId;
use crate::render::mesh::Mesh;
use crate::render::perf::frustum::Frustum;
use crate::render::pipeline::desc::PipelineDesc;
use crate::render::pipeline::PipelineId;
use crate::render::renderer::{ObjectData, Renderer};
use crate::render::texture::Texture;
use crate::render::voxels::svo::VoxelTree;
use crate::wrappers::graphics::GraphicsDevice;

pub mod svo;
pub mod naive;

pub const NOISE_SCALE: f64 = 12.5;
pub const CHUNK_DEPTH: usize = 5;
pub const CHUNK_FLAT_ARR_DEPTH: usize = 32 * 32 * 32;
pub const VIEW_DISTANCE: f32 = (1 << CHUNK_DEPTH) as f32 * 8.; // 8 chunks around
pub const VIEW_DISTANCE_SQ: f32 = VIEW_DISTANCE * VIEW_DISTANCE;

#[derive(Debug)]
pub struct VoxelWorld {
    chunks: FxHashMap<IVec3, Chunk>,
    mesher: Box<dyn VoxelMesher>,
    pso: PipelineId,
    pso_wireframe: PipelineId,
    pso_shadows: PipelineId,
    material_data: [u64; 8],
    materials_va: u64,
    perlin: Perlin,
}

impl VoxelWorld {
    pub fn new(mesher: Box<dyn VoxelMesher>, rd: &mut Renderer, mut material_data: Vec<u64>, materials_va: u64) -> anyhow::Result<Self> {
        let (pso, pso_wireframe, pso_shadows) = mesher.make_pipelines(rd)?;
        material_data.resize(8, 0);
        Ok(Self {
            chunks: FxHashMap::with_capacity_and_hasher(16, FxBuildHasher::default()),
            mesher,
            pso,
            pso_wireframe,
            pso_shadows,
            material_data: std::array::from_fn(|i| material_data[i]),
            perlin: Perlin::new(0xB717E),
            materials_va
        })
    }

    pub fn swap_mesher(&mut self, new_mesher: Box<dyn VoxelMesher>, rd: &mut Renderer) -> anyhow::Result<()> {
        self.mesher = new_mesher;
        let (pso, pso_wireframe, pso_shadows) = self.mesher.make_pipelines(rd)?;
        self.pso = pso;
        self.pso_wireframe = pso_wireframe;
        for chunk in &mut self.chunks {
            chunk.1.mark_dirty()
        }
        Ok(())
    }

    pub fn update(&self, gd: &GraphicsDevice) -> anyhow::Result<()> {
        for chunk in &self.chunks {
            chunk.1.update(self, gd)?;
        }
        Ok(())
    }

    pub fn build_chunk(&mut self, chunk_pos: IVec3, gd: &GraphicsDevice) -> anyhow::Result<()> {
        if self.chunks.contains_key(&chunk_pos) {
            return Ok(());
        }

        let offset = chunk_pos * CHUNK_DEPTH as i32;
        let mut raw_list = [0u8; CHUNK_FLAT_ARR_DEPTH];
        for i in 0..raw_list.len() {
            let pos = index_to_uvec3(i, 1 << CHUNK_DEPTH);
            let noise_pos: IVec3 = offset + IVec3::new(pos.x as i32, pos.y as i32, pos.z as i32);
            let value = self.perlin.get([(noise_pos.x as f64) / NOISE_SCALE, (noise_pos.y as f64) / NOISE_SCALE, (noise_pos.z as f64) / NOISE_SCALE]);
            if value > 0.5 {
                raw_list[i] = 2;
            }
        }

        let tree = VoxelTree::build_from_flat_list(CHUNK_DEPTH as u32, self.material_data, &raw_list);
        self.chunks.insert(chunk_pos, Chunk::new(tree, self, gd)?);
        Ok(())
    }

    pub fn select(&self, frustum: &Frustum, camera_pos: Vec3) -> (Vec<(u32, Ref<Mesh>)>, Vec<ObjectData>) {
        let meshes_objects = self.chunks.iter().enumerate().filter_map(|(idx, (chunk_pos, chunk))| {
            let chunk_pos_ws = Vec3::new(chunk_pos.x as f32, chunk_pos.y as f32, chunk_pos.z as f32);
            let mat = Mat4::from_translation(chunk_pos_ws);
            if chunk_pos_ws.distance_squared(camera_pos) <= VIEW_DISTANCE_SQ && frustum.contains(chunk.mesh.borrow().aabb(), mat) {
                Some(((idx as u32, chunk.mesh.borrow()), ObjectData {
                    model: mat,
                    material_id: 0,
                    _pad: [0; 3],
                }))
            } else {
                None
            }
        }).collect::<(Vec<(u32, Ref<Mesh>)>, Vec<ObjectData>)>();
        meshes_objects
    }

    pub fn add_pass<'a>(&'a self, rd: &'a Renderer, gbuf: &'a GBuffer, opts: &RenderOpts, graph: &mut FrameGraph<'a>) -> anyhow::Result<()> {
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
    tree: VoxelTree,
    mesh: RefCell<Mesh>,
    is_dirty: Cell<bool>,
}

impl Chunk {
    pub fn new(tree: VoxelTree, world: &VoxelWorld, gd: &GraphicsDevice) -> anyhow::Result<Self> {
        let initial_mesh = world.mesher.make_mesh(&tree, gd)?;
        Ok(Self {
            tree,
            mesh: RefCell::new(initial_mesh),
            is_dirty: Cell::new(false),
        })
    }

    pub fn update(&self, world: &VoxelWorld, gd: &GraphicsDevice) -> anyhow::Result<()> {
        if self.is_dirty.get() {
            *self.mesh.borrow_mut() = world.mesher.make_mesh(&self.tree, gd)?;
            self.is_dirty.set(false);
        }
        Ok(())
    }

    pub fn batch_mutate<F: FnOnce(&mut VoxelTree)>(&mut self, op: F) {
        op(&mut self.tree);
        self.is_dirty.set(true);
    }

    pub fn mark_dirty(&mut self) {
        self.is_dirty.set(true);
    }
}

pub trait VoxelMesher: Debug + Send + Sync {
    fn make_mesh(&self, tree: &VoxelTree, gd: &GraphicsDevice) -> anyhow::Result<Mesh>;
    fn make_pipelines(&self, rd: &mut Renderer) -> anyhow::Result<(PipelineId, PipelineId, PipelineId)>;
}

pub fn index_to_uvec3(idx: usize, depth: usize) -> UVec3 {
    // index = z * (depth ^ 2) + y * (depth) + x
    // so reverse is
    let z = idx / (depth * depth);
    let x_y = idx % (depth * depth);
    let y = x_y / depth;
    let x = x_y % depth;
    UVec3::new(x as u32, y as u32, z as u32)
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct VoxelShadowPass {
    light_vp: Mat4
}