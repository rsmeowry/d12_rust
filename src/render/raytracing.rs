pub mod tlas;
pub mod gi;

use glam::{IVec2, Vec2, Vec3};
use windows::Win32::Graphics::Direct3D12::{ID3D12Device5, ID3D12GraphicsCommandList4, ID3D12Resource, D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_DESC, D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS, D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS_0, D3D12_ELEMENTS_LAYOUT_ARRAY, D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_PREFER_FAST_BUILD, D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_PREFER_FAST_TRACE, D3D12_RAYTRACING_ACCELERATION_STRUCTURE_PREBUILD_INFO, D3D12_RAYTRACING_ACCELERATION_STRUCTURE_TYPE_BOTTOM_LEVEL, D3D12_RAYTRACING_ACCELERATION_STRUCTURE_TYPE_TOP_LEVEL, D3D12_RAYTRACING_INSTANCE_DESC, D3D12_RAYTRACING_INSTANCE_FLAG_TRIANGLE_CULL_DISABLE};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_FORMAT_R16G16_FLOAT, DXGI_FORMAT_R16_FLOAT};
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
    raw_tex: ResId,
    blur_tex: [ResId; 3],
    compute_pso: PipelineId,
    penumbra_pso: PipelineId,
    blur_pso: PipelineId,
    downscale_by: u32,
    sun_tan: f32,
}

impl RtShadowsPass {
    pub fn new(downscale_by: u32, sun_angle_deg: f32, renderer: &mut Renderer) -> anyhow::Result<Self> {
        let tlas = Tlas::new(&renderer.gd, 4096)?;
        let storage_tex = renderer.gr.storage(&renderer.gd, "RTShadows", DXGI_FORMAT_R16G16B16A16_FLOAT, ResSize::RenderDiv(downscale_by))?;
        let blur_tex = ["RTBlurRad", "RTBlurPing", "RTBlurPong"].map(|name| renderer.gr.storage(&renderer.gd, name, DXGI_FORMAT_R16_FLOAT, ResSize::RenderDiv(downscale_by)).expect("Failed to init RT Texture"));
        let compute_pso = renderer.create_pipeline(PipelineDesc {
            name: "RTShadowsPass",
            shader: "compute/raytracing/RTShadows.compute.hlsl".into(),
            ..Default::default()
        })?;
        let penumbra_pso = renderer.create_pipeline(PipelineDesc {
            name: "RTShadowsPenumbra",
            shader: "compute/raytracing/RTPenumbra.compute.hlsl".into(),
            ..Default::default()
        })?;
        let blur_pso = renderer.create_pipeline(PipelineDesc {
            name: "RTShadowsBlur",
            shader: "compute/raytracing/RTBlur.compute.hlsl".into(),
            ..Default::default()
        })?;

        Ok(Self {
            tlas,
            compute_pso,
            penumbra_pso,
            blur_pso,
            raw_tex: storage_tex,
            blur_tex,
            downscale_by,
            sun_tan: sun_angle_deg.to_radians().tan(),
        })
    }

    pub fn tlas(&self) -> &Tlas {
        &self.tlas
    }
    
    pub fn shadows_tex(&self) -> ResId {
        self.blur_tex[2]
    }

    pub fn add_pass<'a>(&self, objects: &[ObjectData], meshes: &[&Mesh], lights: &[GpuLight], gbuf: &'a GBuffer, rd: &'a Renderer, graph: &mut FrameGraph<'a>) -> anyhow::Result<()> {
        // building tlas
        self.tlas.add_build_pass(objects, meshes, rd, graph)?;

        let (rw, rh) = rd.gr.render_size();
        let raw_tex = rd.gr.get(self.raw_tex)?;
        let pass_res = Vec2::new(raw_tex.desc().width as f32, raw_tex.desc().height as f32);
        let pass_va = rd.upload_cbuffer(&RtShadowsPassProps {
            pass_res,
            out_tex: raw_tex.uav_ptr().unwrap(),
            normals_tex: rd.gr.get(gbuf.normals)?.srv_ptr(),
            sun_dir: lights[0].direction,
            depth_tex: rd.gr.get(gbuf.scene_depth)?.srv_ptr(),
            cam_forward: rd.cam.forward()
        })?;
        let penumbra_pass_va = rd.upload_cbuffer(&RtPenumbraPassProps {
            pass_res,
            raw_tex: raw_tex.srv_ptr(),
            out_tex: rd.gr.get(self.blur_tex[0])?.uav_ptr().unwrap(),
            tan_sun: self.sun_tan,
            proj_scale: rd.cam.proj().to_cols_array_2d()[1][1],
            max_radius: 16.,
            _pad: 0,
        })?;
        let tlas_va = self.tlas.tlas_va();
        let pipelines = rd.pipelines();
        let rs = rd.rs();
        let compute_pso = self.compute_pso;
        let penumbra_pso = self.penumbra_pso;
        let blur_pso = self.blur_pso;

        // compute shadows
        graph.add_pass("RTComputeShadows")
            .reads(gbuf.normals)
            .reads(gbuf.scene_depth)
            .writes_uav(self.raw_tex, ResLoad::Preserve)
            .exec(move |ctx| {
                pipelines.get(compute_pso).bind(ctx.cmd);
                rs.set_compute_pass(ctx.cmd, pass_va);
                rs.set_compute_tlas(ctx.cmd, tlas_va);
                unsafe {
                    ctx.cmd.Dispatch((pass_res.x as u32).div_ceil(8), (pass_res.y as u32).div_ceil(8), 1);
                }
                Ok(())
            });

        // penumbra extraction
        graph.add_pass("RTPenumbra")
            .reads(self.raw_tex)
            .writes_uav(self.blur_tex[0], ResLoad::Preserve)
            .exec(move |ctx| {
                pipelines.get(penumbra_pso).bind(ctx.cmd);
                rs.set_compute_pass(ctx.cmd, penumbra_pass_va);
                unsafe {
                    ctx.cmd.Dispatch((pass_res.x as u32).div_ceil(8), (pass_res.y as u32).div_ceil(8), 1);
                }
                Ok(())
            });

        for (axis, in_tex, out_tex, blur_pass) in [(IVec2::X, self.raw_tex, self.blur_tex[1], "RTBlurH"), (IVec2::Y, self.blur_tex[1], self.blur_tex[2], "RTBlurV")] {
            let pass_va = rd.upload_cbuffer(&RtBlurPassProps {
                axis,
                in_tex: rd.gr.get(in_tex)?.srv_ptr(),
                rad_tex: rd.gr.get(self.blur_tex[0])?.srv_ptr(),
                raw_tex: raw_tex.srv_ptr(),
                out_tex: rd.gr.get(out_tex)?.uav_ptr().unwrap(),
                depth_to_l: 0.02,
                _pad: 0,
            })?;

            graph.add_pass(blur_pass)
                .reads(self.blur_tex[0])
                .reads(in_tex)
                .reads(self.raw_tex)
                .writes_uav(out_tex, ResLoad::Preserve)
                .exec(move |ctx| {
                    pipelines.get(blur_pso).bind(ctx.cmd);
                    rs.set_compute_pass(ctx.cmd, pass_va);
                    unsafe {
                        ctx.cmd.Dispatch((pass_res.x as u32).div_ceil(8), (pass_res.y as u32).div_ceil(8), 1);
                    }
                    Ok(())
                });
        }

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
    depth_tex: u32,
    cam_forward: Vec3
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct RtPenumbraPassProps {
    pass_res: Vec2,
    raw_tex: u32,
    out_tex: u32,
    tan_sun: f32,
    proj_scale: f32,
    max_radius: f32,
    _pad: u32,
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct RtBlurPassProps {
    axis: IVec2,
    in_tex: u32,
    rad_tex: u32,
    raw_tex: u32,
    out_tex: u32,
    depth_to_l: f32,
    _pad: u32,
}