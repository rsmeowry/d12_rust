use std::path::Path;
use glam::Vec3;
use windows::Win32::Graphics::Direct3D12::{D3D12_COMPARISON_FUNC_LESS_EQUAL, D3D12_CULL_MODE_FRONT, D3D12_CULL_MODE_NONE};
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use crate::render::builtin::{aabb_from_vertices3, Vertex3};
use crate::render::deferred::GBuffer;
use crate::render::graph::FrameGraph;
use crate::render::graph::pass::ResLoad;
use crate::render::graph::res::ResId;
use crate::render::lights::{light_cone, unit_sphere, GpuLight, LIGHT_POINT, LIGHT_SPOT};
use crate::render::mesh::{DrawTopology, Mesh};
use crate::render::pipeline::desc::{Blend, Depth, PipelineDesc};
use crate::render::pipeline::PipelineId;
use crate::render::renderer::{Renderer, SCENE_FORMAT};
use crate::render::shadows::{GpuCascades, ShadowCascades, ShadowMap};
use crate::render::texture::Texture;
use crate::render::vertex::Vertex;
use crate::wrappers::graphics::GraphicsDevice;

#[derive(Debug)]
pub struct PhongLightingPass {
    lights: PipelineId,
    lights_fullscreen: PipelineId,
    point_light_mesh: Mesh,
    spot_light_mesh: Mesh,
    sky_tex: Texture,
    local_shadows: Vec<ResId>,
}

impl PhongLightingPass {
    pub fn new<P: AsRef<Path>>(renderer: &mut Renderer, sky_path: P) -> anyhow::Result<Self> {
        let mut batch = renderer.gd.begin_upload()?;

        // let sky_tex = Texture::cube_from_hdr(&mut batch, [
        //     &std::fs::read("assets/textures/cubemap/sky/left.hdr")?,
        //     &std::fs::read("assets/textures/cubemap/sky/right.hdr")?,
        //     &std::fs::read("assets/textures/cubemap/sky/up.hdr")?,
        //     &std::fs::read("assets/textures/cubemap/sky/down.hdr")?,
        //     &std::fs::read("assets/textures/cubemap/sky/front.hdr")?,
        //     &std::fs::read("assets/textures/cubemap/sky/back.hdr")?,
        // ], "Sky")?;
        let sky_tex = Texture::from_hdr(&mut batch, &std::fs::read(sky_path)?, "Sky")?;

        let point_light_mesh = unit_sphere(8, 8, |pos, norm, _tangent, _binormal, uv| { Vertex3::new(pos, norm, uv) });
        let point_light_mesh = Mesh::new(&renderer.gd, &mut batch, &point_light_mesh.0, &point_light_mesh.1, DrawTopology::TriangleList, aabb_from_vertices3(&point_light_mesh.0), false)?;
        let spot_light_mesh = light_cone(8);
        let spot_light_mesh = Mesh::new(&renderer.gd, &mut batch, &spot_light_mesh.0, &spot_light_mesh.1, DrawTopology::TriangleList, aabb_from_vertices3(&spot_light_mesh.0), false)?;

        batch.finish()?;

        let lights = renderer.create_pipeline(PipelineDesc {
            rts: vec![SCENE_FORMAT],
            cull: D3D12_CULL_MODE_FRONT,
            depth: Some(Depth {
                format: GraphicsDevice::DS_FORMAT,
                func: D3D12_COMPARISON_FUNC_LESS_EQUAL,
                write: false,
            }),
            depth_clip: false,
            blend: Blend::Additive,
            input: Vertex3::LAYOUT,
            name: "Lights",
            shader: "deferred/Lights.hlsl".into(),
            ..Default::default()
        })?;
        let lights_fullscreen = renderer.create_pipeline(PipelineDesc {
            rts: vec![SCENE_FORMAT],
            name: "LightsFullscreen",
            shader: "deferred/LightsFullscreen.hlsl".into(),
            input: &[],
            depth: None,
            cull: D3D12_CULL_MODE_NONE,
            blend: Blend::Additive,
            ..Default::default()
        })?;

        Ok(Self {
            lights,
            lights_fullscreen,
            point_light_mesh,
            spot_light_mesh,
            sky_tex,
            local_shadows: vec![]
        })
    }

    pub fn save_local_shadows(&mut self, map: &ShadowMap) {
        self.local_shadows.extend(map.fetch_all_local_shadows());
    }

    pub fn add_pass<'a>(&'a self, mut lights: Vec<GpuLight>, renderer: &'a Renderer, shadows: Option<ShadowCascades>, gbuffer: &GBuffer, graph: &mut FrameGraph<'a>) -> anyhow::Result<(u64, u32)> {
        lights.sort_by_key(|l| l.kind);
        let first_point = lights.partition_point(|l| l.kind < LIGHT_POINT) as u32;
        let first_spot = lights.partition_point(|l| l.kind < LIGHT_SPOT) as u32;
        let point_count = first_spot - first_point;
        let spot_count = lights.len() as u32 - first_spot;
        let lights_va = renderer.upload_slice(bytemuck::cast_slice(&lights), 16)?;
        let gr = &renderer.gr;

        let light_pass_va = renderer.upload_slice(bytemuck::bytes_of(&LightingPass {
            albedo: gr.get(gbuffer.albedo)?.srv_ptr(),
            normal: gr.get(gbuffer.normals)?.srv_ptr(),
            material: gr.get(gbuffer.materials)?.srv_ptr(),
            depth: gr.get(gbuffer.scene_depth)?.srv_ptr(),
            dir_count: first_point,
            debug_view: 0,
            sky_tex: self.sky_tex.srv_ptr(),
            _pad: 0.,
            ambient: Vec3::ONE * 0.07,
            _pad2: 0.,
            csm: GpuCascades::new(shadows.as_ref(), gr, renderer.cam.forward())?,
            irradiance_tex: 0,
            specular_tex: 0,
            brdf_lut_tex: 0,
            specular_mips: 0.
        }), 256)?;

        let rs = renderer.rs();
        let pipelines = renderer.pipelines();

        // directional light
        let mut pass = graph
            .add_pass("LightsFullscreen")
            .reads(gbuffer.albedo)
            .reads(gbuffer.normals)
            .reads(gbuffer.materials)
            .reads(gbuffer.scene_depth)
            .writes(gbuffer.scene_color, ResLoad::Preserve);

        for local_shadows in &self.local_shadows {
            pass = pass.reads(*local_shadows);
        }

        if let Some(shadows) = shadows {
            for cascade in shadows.cascades {
                pass = pass.reads(cascade);
            }
        }

        pass.exec(move |ctx| {
            pipelines.get(self.lights_fullscreen).bind(ctx.cmd);
            rs.set_pass(ctx.cmd, light_pass_va);
            rs.set_lights(ctx.cmd, lights_va);
            unsafe {
                ctx.cmd.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                ctx.cmd.DrawInstanced(3, 1, 0, 0);
            }
            Ok(())
        });

        // lights volumes
        graph.add_pass("LightVolumes")
            .reads(gbuffer.albedo)
            .reads(gbuffer.normals)
            .reads(gbuffer.materials)
            .reads_depth(gbuffer.scene_depth)
            .writes(gbuffer.scene_color, ResLoad::Preserve)
            .exec(move |ctx| {
                pipelines.get(self.lights).bind(ctx.cmd);
                rs.set_pass(ctx.cmd, light_pass_va);
                rs.set_lights(ctx.cmd, lights_va);
                if point_count > 0 {
                    rs.set_draw_index(ctx.cmd, first_point);
                    self.point_light_mesh.draw_instanced(ctx.cmd, point_count);
                }
                if spot_count > 0 {
                    rs.set_draw_index(ctx.cmd, first_spot);
                    self.spot_light_mesh.draw_instanced(ctx.cmd, spot_count);
                }
                Ok(())
            });

        Ok((lights_va, lights.len() as u32))
    }
}


#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightingPass {
    pub albedo: u32,
    pub normal: u32,
    pub material: u32,
    pub depth: u32,
    pub dir_count: u32,
    pub debug_view: u32,
    pub sky_tex: u32,
    pub _pad: f32,
    pub ambient: Vec3,
    pub _pad2: f32,
    pub csm: GpuCascades,
    pub irradiance_tex: u32,
    pub specular_tex: u32,
    pub brdf_lut_tex: u32,
    pub specular_mips: f32
}
const _: () = assert!(std::mem::offset_of!(LightingPass, csm) == 48);
