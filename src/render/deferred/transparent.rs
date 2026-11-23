use std::path::PathBuf;
use glam::{Vec3, Vec4};
use windows::Win32::Graphics::Direct3D12::{D3D12_CULL_MODE_BACK, D3D12_CULL_MODE_FRONT, D3D12_CULL_MODE_NONE};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_R32G32_FLOAT, DXGI_FORMAT_R8G8B8A8_UNORM_SRGB};
use crate::render::deferred::GBuffer;
use crate::render::graph::FrameGraph;
use crate::render::graph::pass::ResLoad;
use crate::render::graph::res::{ResId, ResSize};
use crate::render::lights::GpuLight;
use crate::render::mesh::Mesh;
use crate::render::pipeline::desc::PipelineDesc;
use crate::render::pipeline::PipelineId;
use crate::render::renderer::{ObjectData, Renderer};
use crate::render::vertex::Vertex;

const PEEL_LAYERS: usize = 8;

#[derive(Debug)]
pub struct TransparentPass {
    prepass_pso: PipelineId,
    pso: PipelineId,
    peel_layers: [ResId; PEEL_LAYERS],
    target_tex: ResId,
}

impl TransparentPass {
    pub fn target_tex(&self) -> ResId {
        self.target_tex
    }

    pub fn new<V: Vertex, P: Into<PathBuf>>(rd: &mut Renderer, name: &'static str, path: P, prepass: P) -> anyhow::Result<Self> {
        let prepass_pso = rd.create_pipeline(PipelineDesc {
            name,
            shader: prepass.into(),
            input: V::LAYOUT,
            rts: vec![DXGI_FORMAT_R32G32_FLOAT],
            cull: D3D12_CULL_MODE_FRONT,
            depth: None,
            ..Default::default()
        })?;
        let pso = rd.create_pipeline(PipelineDesc {
            name,
            shader: path.into(),
            input: V::LAYOUT,
            rts: vec![DXGI_FORMAT_R8G8B8A8_UNORM_SRGB],
            depth: None,
            ..Default::default()
        })?;
        let peel_layers = [0; PEEL_LAYERS].map(|_| rd.gr.color(&rd.gd, "TransparentsPrepass", DXGI_FORMAT_R32G32_FLOAT, Vec4::ZERO, ResSize::FitRender).unwrap());
        let target_tex = rd.gr.color(&rd.gd, "TransparentsOut", DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, Vec4::ZERO, ResSize::FitRender)?;
        Ok(Self {
            pso,
            prepass_pso,
            peel_layers,
            target_tex,
        })
    }

    pub fn add_pass<'a>(&'a self, ffd_va: Option<u64>, irradiance_tex: u32, main_light: &GpuLight, mesh: &'a Mesh, object: &ObjectData, rd: &'a Renderer, gbuf: &GBuffer, graph: &mut FrameGraph<'a>) -> anyhow::Result<ResId> {
        let pass_va = rd.upload_cbuffer(&TransparentPassData {
            depth_tex: rd.gr.get(gbuf.scene_depth)?.srv_ptr(),
            scene_tex: rd.gr.get(gbuf.scene_color)?.srv_ptr(),
            prepass_texs: self.peel_layers.map(|it| rd.gr.get(it).unwrap().srv_ptr()),
            irradiance_tex,
            light_dir: main_light.direction,
            _pad2: 0,
            _pad: 0
        })?;
        let objects_va = rd.upload_objects(&[*object])?;

        let pso = self.pso;
        let prepass_pso = self.prepass_pso;
        let rs = rd.rs();
        let pipelines = rd.pipelines();

        for (idx, prepass_tex) in self.peel_layers.iter().enumerate() {
            let pass = rd.upload_cbuffer(&PrepassData {
                prev_layer: if idx == 0 { 0 } else { rd.gr.get(self.peel_layers[idx])?.srv_ptr() },
                has_prev: (idx != 0) as u32,
                depth_epsilon: 1e-6,
                _pad: 0,
            })?;
            graph
                .add_pass("TransparentsPrepass")
                .writes(*prepass_tex, ResLoad::Clear)
                .exec(move |ctx| {
                    pipelines.get(prepass_pso).bind(ctx.cmd);
                    if let Some(ffd) = ffd_va {
                        rs.set_ffd(ctx.cmd, ffd);
                        rs.set_pass(ctx.cmd, pass);
                    }
                    rs.set_objects(ctx.cmd, objects_va);
                    rs.set_draw_index(ctx.cmd, 0);
                    mesh.draw(ctx.cmd);
                    Ok(())
                });
        }


        graph
            .add_pass("Transparents")
            .reads(gbuf.scene_depth)
            .reads(gbuf.scene_color)
            .reads(*self.peel_layers.last().unwrap())
            .writes(self.target_tex, ResLoad::Preserve)
            .exec(move |ctx| {
                pipelines.get(pso).bind(ctx.cmd);
                if let Some(ffd) = ffd_va {
                    rs.set_ffd(ctx.cmd, ffd);
                }
                rs.set_objects(ctx.cmd, objects_va);
                rs.set_pass(ctx.cmd, pass_va);
                rs.set_draw_index(ctx.cmd, 0);
                mesh.draw(ctx.cmd);
                Ok(())
            });

        Ok(self.target_tex)
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct TransparentPassData {
    depth_tex: u32,
    scene_tex: u32,
    _pad: u32,
    irradiance_tex: u32,
    light_dir: Vec3,
    _pad2: u32,
    prepass_texs: [u32; PEEL_LAYERS],
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct PrepassData {
    prev_layer: u32,
    has_prev: u32,
    depth_epsilon: f32,
    _pad: u32
}