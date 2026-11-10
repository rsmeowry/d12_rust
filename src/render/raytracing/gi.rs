use std::f32::consts::PI;
use glam::{Vec3, Vec4};
use serde::{Deserialize, Serialize};
use windows::Win32::Graphics::Direct3D12::{ID3D12Resource, D3D12_RANGE, D3D12_RESOURCE_STATE_COPY_SOURCE, D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE, D3D12_RESOURCE_STATE_UNORDERED_ACCESS};
use crate::render::graph::FrameGraph;
use crate::render::lights::{GpuLight, LIGHT_POINT, LIGHT_SPOT};
use crate::render::pipeline::desc::PipelineDesc;
use crate::render::pipeline::PipelineId;
use crate::render::raytracing::RtShadowsPass;
use crate::render::renderer::{ObjectData, Renderer};
use crate::wrappers::graphics::{transition_barrier, GraphicsDevice};

pub const MAX_VPL_LEVELS: u32 = 5;

#[derive(Debug)]
pub struct RtGiBakeryPass {
    bake_pso: PipelineId,
    vpl_buffer: ID3D12Resource,
    vpl_uav: u32,
    readback: ID3D12Resource,
    max_vpl: usize,
}

impl RtGiBakeryPass {
    pub fn new(max_vpl: usize, rd: &mut Renderer) -> anyhow::Result<Self> {
        let bake_pso = rd.create_pipeline(PipelineDesc {
            name: "VplBake",
            shader: "compute/raytracing/VplBake.compute.hlsl".into(),
            ..Default::default()
        })?;
        let vpl_buffer = GraphicsDevice::create_uav_buffer(rd.gd.device(), size_of::<GpuVpl>() * max_vpl, "RTGIVplBuffer")?;
        let vpl_uav = rd.gd.create_raw_uav(&vpl_buffer, (size_of::<GpuVpl>() * max_vpl) as u32)?;
        let readback = GraphicsDevice::create_readback_buffer(rd.gd.device(), size_of::<GpuVpl>() * max_vpl, "RTGIVplReadback")?;

        Ok(Self {
            bake_pso,
            vpl_buffer,
            vpl_uav,
            readback,
            max_vpl
        })
    }

    pub fn add_copy_pass<'a>(&'a self, graph: &mut FrameGraph<'a>) {
        let size = (size_of::<GpuVpl>() * self.max_vpl) as u64;
        graph.add_pass("VplReadbackCopy").has_side_effects().exec(move |ctx| {
            unsafe { ctx.cmd.CopyBufferRegion(&self.readback, 0, &self.vpl_buffer, 0, size) };
            Ok(())
        });
    }

    pub fn read(&self, gd: &GraphicsDevice) -> anyhow::Result<Vec<GpuVpl>> {
        gd.flush_command_queue()?; // last frame should have copied
        unsafe {
            let mut ptr = std::ptr::null_mut();
            self.readback.Map(0, None, Some(&mut ptr))?;
            let all = std::slice::from_raw_parts(ptr as *const GpuVpl, self.max_vpl).to_vec();
            self.readback.Unmap(0, Some(&D3D12_RANGE::default()));
            Ok(all.into_iter().filter(|v| v.intensity != Vec3::ZERO).collect())
        }
    }

    pub fn add_bake_pass<'a>(&self, rd: &'a Renderer, scene_center: Vec3, scene_radius: f32, all_objects_va: u64, materials_va: u64, tlas_va: u64, lights: &[GpuLight], graph: &mut FrameGraph<'a>) -> anyhow::Result<()> {
        let bake_pso = self.bake_pso;
        let rs = rd.rs();
        let pipelines = rd.pipelines();

        for light_idx in 0..lights.len() {
            let light = lights[light_idx];
            let pass = VplBakeProps {
                light_pos: light.position,
                light_type: light.kind,
                light_dir: light.direction,
                spot_outer_cos: light.spot_outer_cos,
                spot_inner_cos: light.spot_inner_cos,
                base_flux: light_flux(&light, scene_radius),
                scene_center,
                scene_radius,
                max_vpl: self.max_vpl as u32,
                path_offset: light_idx as u32,
                vpl_buf: self.vpl_uav,
                max_levels: MAX_VPL_LEVELS,
                seed: rand::random(),
                albedo_mip: 0.0,
                _pad: [0; 2],
            };
            let pass_va = rd.upload_cbuffer(&pass)?;
            let path_count = self.max_vpl / MAX_VPL_LEVELS as usize;

            graph.add_pass("RTGIBakePass")
                .has_side_effects()
                .exec(move |ctx| {
                    pipelines.get(bake_pso).bind(ctx.cmd);
                    rs.set_compute_pass(ctx.cmd, pass_va);
                    rs.set_compute_objects(ctx.cmd, all_objects_va);
                    rs.set_compute_materials(ctx.cmd, materials_va);
                    rs.set_compute_tlas(ctx.cmd, tlas_va);
                    unsafe {
                        ctx.cmd.Dispatch(path_count.div_ceil(64) as u32, 1, 1);
                    }
                    Ok(())
                });
        }


        Ok(())
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct VplBakeProps {
    light_pos: Vec3,
    light_type: u32,
    light_dir: Vec3,
    spot_outer_cos: f32,
    base_flux: Vec3, // lumen
    path_offset: u32,
    scene_center: Vec3, // just for directional
    scene_radius: f32,
    max_vpl: u32,
    vpl_buf: u32,
    max_levels: u32,
    seed: u32,
    albedo_mip: f32,
    spot_inner_cos: f32,
    _pad: [u32; 2]
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable, Serialize, Deserialize)]
pub struct GpuVpl {
    pos: Vec3,
    level: u32,
    normal: Vec3,
    _pad1: u32,
    intensity: Vec3,
    _pad2: u32,
}

fn light_flux(l: &GpuLight, scene_radius: f32) -> Vec3 {
    let i = l.color * l.intensity;
    match l.kind {
        LIGHT_POINT => i * 4.0 * PI,
        LIGHT_SPOT => i * 2.0 * PI * (1.0 - l.spot_outer_cos),
        _ => i * PI * scene_radius * scene_radius,
    }
}