use glam::{Mat4, Vec2, Vec3};
use image::GenericImageView;
use windows::Win32::Graphics::Direct3D12::{D3D12_COMPARISON_FUNC_GREATER_EQUAL, D3D12_CULL_MODE_NONE};
use crate::app::RenderOpts;
use crate::render::builtin::{Vertex3, VertexPbr};
use crate::render::deferred::GBuffer;
use crate::render::graph::FrameGraph;
use crate::render::graph::pass::ResLoad;
use crate::render::graph::res::ResId;
use crate::render::mesh::{DrawTopology, Mesh};
use crate::render::perf::frustum::{Frustum, AABB};
use crate::render::pipeline::desc::{Depth, PipelineDesc};
use crate::render::pipeline::material::{GpuMaterial, MaterialParams};
use crate::render::pipeline::PipelineId;
use crate::render::renderer::Renderer;
use crate::render::terrain::quadtree::Quadtree;
use crate::render::texture::{MipCount, Texture};
use crate::render::vertex::Vertex;
use crate::wrappers::graphics::batching::UploadBatch;
use crate::wrappers::graphics::GraphicsDevice;

pub mod quadtree;

#[derive(Debug)]
pub struct Terrain {
    tree: Quadtree,
    gpass: PipelineId,
    gpass_wireframe: PipelineId,
    shadow_pass: PipelineId,
    albedo_map: Texture,
    height_map: Texture,
    normal_map: Texture,
    roughness_map: Texture,
    ao_map: Texture,
    patch_mesh: Mesh,
    ranges: Vec<f32>,
    world_texels: u32,
    texel_size_ws: f32,
    height_scale: f32,
}

impl Terrain {
    pub fn new(rd: &mut Renderer, terrain_path: &str, leaf_size_texels: u32, texel_size_ws: f32, height_scale: f32) -> anyhow::Result<Self> {
        let height_map = image::open(format!("{terrain_path}_height.png"))?;
        let world_texels = height_map.width();
        let tree = Quadtree::build(&height_map.to_luma16(), leaf_size_texels, texel_size_ws, height_scale);

        let leaf_ws = leaf_size_texels as f32 * texel_size_ws;
        let ranges: Vec<f32> = (0..=tree.root_lod()).map(|lod| 3. * leaf_ws * (1 << lod) as f32).collect();

        let mut batch = rd.gd.begin_upload()?;
        let height_map = Texture::load_luma16(&mut batch, height_map.to_luma16(), MipCount::All, &format!("{terrain_path}_HEIGHT"))?;
        let albedo_map = Texture::load(&mut batch, &std::fs::read(format!("{terrain_path}_albedo.png"))?, true, MipCount::All, &format!("{terrain_path}_ALBEDO"))?;
        let roughness_map = Texture::load(&mut batch, &std::fs::read(format!("{terrain_path}_roughness.png"))?, false, MipCount::All, &format!("{terrain_path}_ROUGHNESS"))?;
        let normal_map = Texture::load(&mut batch, &std::fs::read(format!("{terrain_path}_normals.png"))?, false, MipCount::All, &format!("{terrain_path}_NORMALS"))?;
        let ao_map = Texture::load_luma16(&mut batch, image::open(format!("{terrain_path}_ao.png"))?.to_luma16(), MipCount::All, &format!("{terrain_path}_AO"))?;

        let patch_mesh = Self::build_patch_mesh(&mut batch, leaf_size_texels / 2)?;
        batch.finish()?;

        let (gpass, gpass_wireframe) = rd.build_gpass::<Vertex3>("deferred/terrain/GPassTerrain.hlsl", DrawTopology::TriangleList)?;

        let shadow_pass = rd.create_pipeline(PipelineDesc {
            name: "TerrainShadows",
            shader: "deferred/terrain/TerrainShadows.hlsl".into(),
            input: Vertex3::LAYOUT,
            rts: vec![],
            depth: Some(Depth { format: GraphicsDevice::DS_FORMAT, func: D3D12_COMPARISON_FUNC_GREATER_EQUAL, write: true }),
            cull: D3D12_CULL_MODE_NONE,
            depth_bias: -20,
            slope_bias: -1.0,
            depth_clip: false,
            ..Default::default()
        })?;

        Ok(Self {
            tree,
            gpass,
            gpass_wireframe,
            shadow_pass,
            albedo_map,
            height_map,
            normal_map,
            roughness_map,
            ao_map,
            patch_mesh,
            ranges,
            world_texels,
            texel_size_ws,
            height_scale,
        })
    }

    pub fn add_pass<'a>(&'a self, rd: &'a Renderer, gbuf: &GBuffer, opts: &RenderOpts, graph: &mut FrameGraph<'a>) -> anyhow::Result<()> {
        let mut patches = vec![];
        self.tree.select(self.tree.root_lod(), 0, 0, rd.cam.pos(), &self.ranges, &rd.cam.frustum(), &mut patches);

        let mut morphs = [[0.; 4]; MAX_TERRAIN_LODS];
        for (lod, &end) in self.ranges.iter().enumerate() {
            let prev = if lod == 0 { 0. } else { self.ranges[lod - 1] };
            morphs[lod] = [prev + (end - prev) * 0.66, end, 0., 0.]; // morphing the last 3rd
        }

        let pass_va = rd.upload_cbuffer(&TerrainPass {
            height_map: self.height_map.srv_ptr(),
            albedo_map: self.albedo_map.srv_ptr(),
            roughness_map: self.roughness_map.srv_ptr(),
            normal_map: self.normal_map.srv_ptr(),
            ao_map: self.ao_map.srv_ptr(),
            world_texels: self.world_texels,
            texel_size_ws: self.texel_size_ws,
            height_scale: self.height_scale,
            morphs,
            light_view_proj: Mat4::IDENTITY
        })?;
        let patches_va = rd.upload_slice(bytemuck::cast_slice(&patches), 16)?;
        let patch_count = patches.len() as u32;
        let pipeline = if opts.use_wireframe { self.gpass_wireframe } else { self.gpass };
        let pipelines = rd.pipelines();
        let rs = rd.rs();

        graph.add_pass("Terrain")
            .writes_depth(gbuf.scene_depth, ResLoad::Clear)
            .writes(gbuf.albedo, ResLoad::Clear)
            .writes(gbuf.normals, ResLoad::Clear)
            .writes(gbuf.materials, ResLoad::Clear)
            .writes(gbuf.scene_color, ResLoad::Clear)
            .exec(move |ctx| {
                pipelines.get(pipeline).bind(ctx.cmd);
                rs.set_pass(ctx.cmd, pass_va);
                rs.set_objects(ctx.cmd, patches_va);
                self.patch_mesh.draw_instanced(ctx.cmd, patch_count);
                Ok(())
            });

        Ok(())
    }

    pub fn add_shadow_passes<'a>(&'a self, rd: &'a Renderer, cascades: impl Iterator<Item = (ResId, Mat4)>, graph: &mut FrameGraph<'a>) -> anyhow::Result<()> {
        let (pipelines, rs) = (rd.pipelines(), rd.rs());
        let mut morphs = [[0.; 4]; MAX_TERRAIN_LODS];
        for (lod, &end) in self.ranges.iter().enumerate() {
            let prev = if lod == 0 { 0. } else { self.ranges[lod - 1] };
            morphs[lod] = [prev + (end - prev) * 0.66, end, 0., 0.]; // morphing the last 3rd
        }

        for (target, vp) in cascades {
            let mut patches = Vec::new();
            self.tree.select(self.tree.root_lod(), 0, 0, rd.cam.pos(), &self.ranges, &Frustum::from_view_proj(vp), &mut patches);

            let pass_va = rd.upload_cbuffer(&TerrainPass {
                height_map: self.height_map.srv_ptr(),
                albedo_map: self.albedo_map.srv_ptr(),
                roughness_map: self.roughness_map.srv_ptr(),
                normal_map: self.normal_map.srv_ptr(),
                ao_map: self.ao_map.srv_ptr(),
                world_texels: self.world_texels,
                texel_size_ws: self.texel_size_ws,
                height_scale: self.height_scale,
                morphs,
                light_view_proj: vp
            })?;
            let patches_va = rd.gd.frame_context()?.frame_alloc.push_slice(&patches, 16)?;
            let count = patches.len() as u32;
            graph.add_pass("TerrainShadows")
                .writes_depth(target, ResLoad::Preserve)
                .exec(move |ctx| {
                    pipelines.get(self.shadow_pass).bind(ctx.cmd);
                    rs.set_pass(ctx.cmd, pass_va);
                    rs.set_objects(ctx.cmd, patches_va);
                    self.patch_mesh.draw_instanced(ctx.cmd, count);
                    Ok(())
                });
        }
        Ok(())
    }

    fn build_patch_mesh(batch: &mut UploadBatch, size: u32) -> anyhow::Result<Mesh> {
        let mut vertices = vec![];
        let mut indices = vec![];

        for z in 0..=size {
            for x in 0..=size {
                let patch_uv = Vec2::new(x as f32 / size as f32, z as f32 / size as f32);
                vertices.push(Vertex3::new(Vec3::new(x as f32, 0., z as f32), Vec3::Y, patch_uv));
            }
        }
        // * * * * *
        // * * * * *
        // * * * * *

        for z in 0..size {
            for x in 0..size {
                let a = (z * (size + 1) + x);
                let b = (z * (size + 1) + x + 1);
                let c = ((z + 1) * (size + 1) + x);
                let d = ((z + 1) * (size + 1) + (x + 1));

                indices.push(a);
                indices.push(c);
                indices.push(b);

                indices.push(b);
                indices.push(c);
                indices.push(d);
            }
        }

        let aabb = AABB {
            min: Vec3::new(0., 0., 0.),
            max: Vec3::new(1., 0., 1.)
        };
        Mesh::new(batch.gd(), batch, &vertices, &indices, DrawTopology::TriangleList, aabb, true)
    }
}

const MAX_TERRAIN_LODS: usize = 8;
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct TerrainPass {
    height_map: u32,
    albedo_map: u32,
    normal_map: u32,
    roughness_map: u32,
    ao_map: u32,
    world_texels: u32,
    texel_size_ws: f32,
    height_scale: f32,
    morphs: [[f32; 4]; MAX_TERRAIN_LODS],
    light_view_proj: Mat4
}