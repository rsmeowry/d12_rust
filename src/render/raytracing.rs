pub mod tlas;

use glam::{Vec2, Vec3};
use windows::Win32::Graphics::Direct3D12::{ID3D12Device5, ID3D12GraphicsCommandList4, ID3D12Resource, D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_DESC, D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS, D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS_0, D3D12_ELEMENTS_LAYOUT_ARRAY, D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_PREFER_FAST_BUILD, D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_PREFER_FAST_TRACE, D3D12_RAYTRACING_ACCELERATION_STRUCTURE_PREBUILD_INFO, D3D12_RAYTRACING_ACCELERATION_STRUCTURE_TYPE_BOTTOM_LEVEL, D3D12_RAYTRACING_ACCELERATION_STRUCTURE_TYPE_TOP_LEVEL, D3D12_RAYTRACING_INSTANCE_DESC, D3D12_RAYTRACING_INSTANCE_FLAG_TRIANGLE_CULL_DISABLE};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R16G16_FLOAT;
use windows_core::Interface;
use crate::render::deferred::GBuffer;
use crate::render::graph::FrameGraph;
use crate::render::graph::pass::ResLoad;
use crate::render::graph::res::{ResId, ResSize};
use crate::render::lights::GpuLight;
use crate::render::mesh::Mesh;
use crate::render::pipeline::desc::PipelineDesc;
use crate::render::pipeline::PipelineId;
use crate::render::raytracing::tlas::Tlas;
use crate::render::renderer::{ObjectData, Renderer};
use crate::render::texture::Texture;
use crate::wrappers::graphics::{uav_barrier, GraphicsDevice};

#[derive(Debug)]
pub struct RtShadowsPass {
    tlas: Tlas,
    storage_tex: ResId,
    compute_pso: PipelineId,
    downscale_by: u32,
}

impl RtShadowsPass {
    pub fn new(downscale_by: u32, renderer: &mut Renderer) -> anyhow::Result<Self> {
        let tlas = Tlas::new(&renderer.gd, 4096)?;
        let storage_tex = renderer.gr.storage(&renderer.gd, "RTShadows", DXGI_FORMAT_R16G16_FLOAT, ResSize::RenderDiv(downscale_by))?;
        let compute_pso = renderer.create_pipeline(PipelineDesc {
            name: "RTShadowsPass",
            shader: "compute/raytracing/RTShadows.compute.hlsl".into(),
            ..Default::default()
        })?;

        Ok(Self {
            tlas,
            compute_pso,
            storage_tex,
            downscale_by
        })
    }
    
    pub fn shadows_tex(&self) -> ResId {
        self.storage_tex
    }

    pub fn add_pass<'a>(&self, objects: &[ObjectData], meshes: &[&Mesh], lights: &[GpuLight], gbuf: &'a GBuffer, rd: &'a Renderer, graph: &mut FrameGraph<'a>) -> anyhow::Result<()> {
        // building tlas
        self.tlas.add_build_pass(objects, meshes, rd, graph)?;

        let (rw, rh) = rd.gr.render_size();
        let tex = rd.gr.get(self.storage_tex)?;
        let pass_res = Vec2::new(tex.desc().width as f32, tex.desc().height as f32);
        let pass_va = rd.upload_cbuffer(&RtShadowsPassProps {
            pass_res,
            out_tex: tex.uav_ptr().unwrap(),
            normals_tex: rd.gr.get(gbuf.normals)?.srv_ptr(),
            sun_dir: lights[0].direction,
            depth_tex: rd.gr.get(gbuf.scene_depth)?.srv_ptr(),
        })?;
        let tlas_va = self.tlas.tlas_va();
        let pipelines = rd.pipelines();
        let rs = rd.rs();
        let compute_pso = self.compute_pso;

        // compute shadows
        graph.add_pass("RTComputeShadows")
            .reads(gbuf.normals)
            .reads(gbuf.scene_depth)
            .has_side_effects()
            .writes_uav(self.storage_tex, ResLoad::Preserve)
            .exec(move |ctx| {
                pipelines.get(compute_pso).bind(ctx.cmd);
                rs.set_compute_pass(ctx.cmd, pass_va);
                rs.set_compute_tlas(ctx.cmd, tlas_va);
                unsafe {
                    ctx.cmd.Dispatch((pass_res.x as u32).div_ceil(8), (pass_res.y as u32).div_ceil(8), 1);
                }
                Ok(())
            });

        Ok(())
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct RtShadowsPassProps {
    pass_res: Vec2,
    out_tex: u32,
    normals_tex: u32,
    sun_dir: Vec3,
    depth_tex: u32
}