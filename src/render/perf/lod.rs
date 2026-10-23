use std::cell::Cell;
use glam::{Mat4, Quat, Vec3, Vec4};
use windows::Win32::Graphics::Direct3D12::{ID3D12GraphicsCommandList, D3D12_CULL_MODE_NONE, D3D12_PRIMITIVE_TOPOLOGY_TYPE_POINT, D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE};
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_POINTLIST;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UNORM_SRGB;
use crate::app::RenderOpts;
use crate::assets::LoadedScene;
use crate::render::builtin::Vertex3;
use crate::render::deferred::GBuffer;
use crate::render::graph::builder::PassBuilder;
use crate::render::graph::FrameGraph;
use crate::render::graph::pass::ResLoad;
use crate::render::graph::res::{ResId, ResSize};
use crate::render::mesh::Mesh;
use crate::render::perf::frustum::AABB;
use crate::render::pipeline::desc::PipelineDesc;
use crate::render::pipeline::material::{GpuMaterial, MaterialParams};
use crate::render::pipeline::PipelineId;
use crate::render::renderer::{ObjectData, Renderer, GB_ALBEDO, GB_MATERIAL, GB_NORMALS, SCENE_FORMAT};
use crate::render::vertex::Vertex;

#[derive(Debug)]
pub struct LodRenderer {
    lod_pipeline: PipelineId,
    lod_wireframe: PipelineId,
    regular_gpass: PipelineId,
    bake_depth: ResId,
    emissive_sink: ResId,
    billboard_textures: Vec<ResId>,
    billboard_normals: Vec<ResId>,
    billboard_materials: Vec<ResId>,
    material_ids: Vec<u32>,
    baked: Cell<bool>,
}

impl LodRenderer {
    pub fn new(scene: &LoadedScene, rd: &mut Renderer, materials: &mut Vec<GpuMaterial>, tex_size: u32) -> anyhow::Result<Self> {
        let gpass = rd.create_pipeline(PipelineDesc {
            rts: vec![GB_ALBEDO, GB_NORMALS, GB_MATERIAL, SCENE_FORMAT],
            name: "GPassBake",
            shader: "deferred/GPassBake.hlsl".into(),
            input: Vertex3::LAYOUT,
            topology: D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
            ..Default::default()
        })?;

        let mut desc = PipelineDesc {
            name: "GPassBillboard",
            shader: "deferred/GPassBillboard.hlsl".into(),
            input: &[],
            cull: D3D12_CULL_MODE_NONE,
            rts: vec![GB_ALBEDO, GB_NORMALS, GB_MATERIAL, SCENE_FORMAT],
            topology: D3D12_PRIMITIVE_TOPOLOGY_TYPE_POINT,
            ..Default::default()
        };
        let lod_pipeline = rd.create_pipeline(desc.clone())?;

        desc.wireframe = true;
        let lod_wireframe = rd.create_pipeline(desc)?;

        let mut material_ids = Vec::new();
        let mut billboard_textures = Vec::new();
        let mut billboard_normals = Vec::new();
        let mut billboard_materials = Vec::new();
        let emissive_sink = rd.gr.color(&rd.gd, "BillboardEmissiveSink", SCENE_FORMAT, Vec4::ZERO, ResSize::Fixed(tex_size, tex_size))?;
        for _ in &scene.meshes {
            let bid = rand::random::<u16>();
            let tex = rd.gr.color(&rd.gd, &format!("BillboardAlbedo_{bid:4x}"), GB_ALBEDO, Vec4::ZERO, ResSize::Fixed(tex_size, tex_size))?;
            let material = rd.gr.color(&rd.gd, &format!("BillboardMaterial_{bid:4x}"), GB_MATERIAL, Vec4::ZERO, ResSize::Fixed(tex_size, tex_size))?;
            let normal = rd.gr.color(&rd.gd, &format!("BillboardNormal_{bid:4x}"), GB_NORMALS, Vec4::new(0.5, 0.5, 0., 0.), ResSize::Fixed(tex_size, tex_size))?;
            material_ids.push(materials.len() as u32);
            materials.push(MaterialParams::Billboard {
                albedo_tex: rd.gr.get(tex)?.srv_ptr(),
                normal_tex: rd.gr.get(normal)?.srv_ptr(),
                material_tex: rd.gr.get(material)?.srv_ptr()
            }.to_gpu_material());
            billboard_textures.push(tex);
            billboard_normals.push(normal);
            billboard_materials.push(material);
        }

        Ok(Self {
            lod_pipeline,
            lod_wireframe,
            billboard_textures,
            billboard_normals,
            billboard_materials,
            material_ids,
            emissive_sink,
            regular_gpass: gpass,
            bake_depth: rd.gr.depth(&rd.gd, "BillboardBakeDepth", ResSize::Fixed(tex_size, tex_size))?,
            baked: Cell::new(false),
        })
    }

    pub fn add_pass<'a>(&'a self, scene: &'a LoadedScene, rd: &'a Renderer, materials_va: u64, bake_model: Mat4, graph: &mut FrameGraph<'a>) -> anyhow::Result<()> {
        if !self.baked.replace(true) {
            // if we haven't baked yet, don't render far objects, prebake the textures
            let (mut objects, draws) = scene.to_objects_and_draws();
            let mut vp_vas = Vec::new();
            let rs = rd.rs();
            let pipelines = rd.pipelines();

            for idx in 0..draws.len() {
                // let bake_model = objects[idx].model;
                objects[idx].model = bake_model;
                let bake_model = objects[idx].model;
                let aabb = draws[idx].aabb();
                let c = bake_model.transform_point3((aabb.min + aabb.max) * 0.5);
                let r = (aabb.max - aabb.min).length() * 0.5;
                let view = glam::camera::lh::view::look_to_mat4(c - Vec3::Z * r, Vec3::Z, Vec3::Y);
                let proj = glam::camera::lh::proj::directx::orthographic(-r, r, -r, r, 2. * r, 0.);
                let vp_va = rd.upload_cbuffer(&(proj * view))?;
                vp_vas.push(vp_va);
            }

            let objects_va = rd.upload_slice(bytemuck::cast_slice(&objects), 16)?;

            for (i, mesh) in draws.into_iter().enumerate() {
                let vp_va = vp_vas[i];
                graph
                    .add_pass("BillboardBake")
                    .writes_depth(self.bake_depth, ResLoad::Clear)
                    .writes(self.billboard_textures[i], ResLoad::Clear)
                    .writes(self.billboard_normals[i], ResLoad::Clear)
                    .writes(self.billboard_materials[i], ResLoad::Preserve)
                    .writes(self.emissive_sink, ResLoad::Preserve)
                    .exec(move |ctx| {
                        rs.set_objects(ctx.cmd, objects_va);
                        rs.set_materials(ctx.cmd, materials_va);
                        pipelines.get(self.regular_gpass).bind(ctx.cmd);
                        rs.set_draw_index(ctx.cmd, i as u32);
                        rs.set_pass(ctx.cmd, vp_va);
                        mesh.draw(ctx.cmd);
                        Ok(())
                    });
            }
            return Ok(())
        }

        Ok(())
    }

    pub fn draw_billboards(&self, cmd: &ID3D12GraphicsCommandList, rd: &Renderer, opts: &RenderOpts, first: u32, count: u32) {
        if count == 0 {
            return;
        }
        rd.pipelines().get(if opts.use_wireframe { self.lod_wireframe } else { self.lod_pipeline }).bind(cmd);
        rd.rs().set_draw_index(cmd, first);
        unsafe {
            cmd.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_POINTLIST);
            cmd.DrawInstanced(count, 1, 0, 0);
        }
    }

    pub fn billboard_for_mesh(&self, obj: &ObjectData, aabb: &AABB, mesh_idx: usize) -> ObjectData {
        let (scale, _, _) = obj.model.to_scale_rotation_translation();
        let center = obj.model.transform_point3((aabb.min + aabb.max) * 0.5);
        let radius = (aabb.max - aabb.min).length() * 0.5 * scale.max_element();
        ObjectData {
            model: Mat4::from_scale_rotation_translation(Vec3::splat(radius), Quat::IDENTITY, center),
            material_id: self.material_ids[mesh_idx],
            _pad: [0; 3],
        }
    }

    pub fn reads_all_baked<'g, 'f>(&self, pass: PassBuilder<'g, 'f>) -> PassBuilder<'g, 'f> {
        self.billboard_textures.iter().chain(&self.billboard_normals).fold(pass, |p, t| p.reads(*t))
    }
}