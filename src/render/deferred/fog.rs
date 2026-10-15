use crate::render::deferred::GBuffer;
use crate::render::graph::pass::ResLoad;
use crate::render::graph::res::{ResId, ResSize};
use crate::render::graph::FrameGraph;
use crate::render::pipeline::desc::{Blend, PipelineDesc};
use crate::render::pipeline::PipelineId;
use crate::render::renderer::{Renderer, SCENE_FORMAT};
use crate::render::shadows::{GpuCascades, ShadowCascades, ShadowMap};
use anyhow::anyhow;
use glam::Vec3;
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use windows::Win32::Graphics::Direct3D12::D3D12_CULL_MODE_NONE;
use crate::render::texture::Texture;

#[derive(Debug)]
pub struct FogPass {
    march: PipelineId,
    composite: PipelineId,
    fog_noise: Texture,
    fog_out: ResId, // rgb: scattered light, a: transmittance
    local_shadows: Vec<ResId>
}

impl FogPass {
    pub const DOWNSCALE: u32 = 4;

    pub fn new(shadows: Option<&ShadowMap>, renderer: &mut Renderer) -> anyhow::Result<Self> {
        let march = renderer.create_pipeline(PipelineDesc {
            name: "FogRaymarch",
            shader: "compute/fog/VolumetricFog.compute.hlsl".into(),
            ..Default::default()
        })?;
        let composite = renderer.create_pipeline(PipelineDesc {
            rts: vec![SCENE_FORMAT],
            name: "FogComposite",
            shader: "compute/fog/VolumetricComposite.hlsl".into(),
            input: &[],
            depth: None,
            cull: D3D12_CULL_MODE_NONE,
            blend: Blend::Premultiplied,
            ..Default::default()
        })?;
        let fog_tex = renderer.gr.storage(&renderer.gd, "Fog", SCENE_FORMAT, ResSize::ScreenDiv(Self::DOWNSCALE))?;

        let mut batch = renderer.gd.begin_upload()?;
        let fog_noise = Texture::from_rgba8_3d(&mut batch, 64, 64, 64, &std::fs::read("assets/textures/noise/noise_3d_64.raw")?, "Noise3D")?;
        batch.finish()?;

        let mut local_shadows = vec![];

        if let Some(shadows) = shadows {
            local_shadows = shadows.fetch_all_local_shadows();
        }

        Ok(Self { march, composite, fog_out: fog_tex, fog_noise, local_shadows })
    }

    pub fn add_pass<'a>(&'a self, props: FogProperties, renderer: &'a Renderer, shadows: Option<ShadowCascades>, lights_va: u64,  gbuffer: &GBuffer, graph: &mut FrameGraph<'a>) -> anyhow::Result<()> {
        let gr = &renderer.gr;
        let rs = renderer.rs();
        let pipelines = renderer.pipelines();

        let fog = gr.get(self.fog_out)?;
        let size = [fog.desc().width, fog.desc().height];
        let depth = gr.get(gbuffer.scene_depth)?.srv_ptr();

        let march_va = renderer.upload_cbuffer(&FogMarchProps {
            csm: GpuCascades::new(shadows.as_ref(), gr, renderer.cam.forward())?,
            out_size: size,
            depth_tex: depth,
            out_tex: fog.uav_ptr().ok_or_else(|| anyhow!("Fog texture has no uvav"))?,
            downscale: Self::DOWNSCALE,
            max_distance: props.max_distance,
            step_distance: props.step_distance.max(0.05),
            density: props.density,
            fog_color: props.color,
            noise_tex: self.fog_noise.srv_ptr(),
            light_count: self.local_shadows.len() as u32,
            _pad: [0; 3],
        })?;
        let composite_va = renderer.upload_cbuffer(&FogCompositeProps {
            fog_tex: fog.srv_ptr(),
            depth_tex: depth,
            downscale: Self::DOWNSCALE,
            _pad: 0,
            fog_size: size,
        })?;

        let mut march = graph.add_pass("FogRaymarch")
            .reads(gbuffer.scene_depth)
            .writes_uav(self.fog_out, ResLoad::Preserve);

        if let Some(s) = &shadows {
            for cascade in s.cascades {
                march = march.reads(cascade);
            }
        }

        for local in &self.local_shadows {
            march = march.reads(*local);
        }
        
        march.exec(move |ctx| {
            pipelines.get(self.march).bind(ctx.cmd);
            rs.set_compute_pass(ctx.cmd, march_va);
            rs.set_compute_lights(ctx.cmd, lights_va);
            unsafe {
                ctx.cmd.Dispatch(size[0].div_ceil(8), size[1].div_ceil(8), 1)
            };
            Ok(())
        });

        graph.add_pass("FogComposite")
            .reads(self.fog_out)
            .reads(gbuffer.scene_depth)
            .writes(gbuffer.scene_color, ResLoad::Preserve)
            .exec(move |ctx| {
                pipelines.get(self.composite).bind(ctx.cmd);
                rs.set_pass(ctx.cmd, composite_va);
                unsafe {
                    ctx.cmd.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                    ctx.cmd.DrawInstanced(3, 1, 0, 0);
                }
                Ok(())
            });

        Ok(())
    }
}

#[derive(Debug, Copy, Clone)]
pub struct FogProperties {
    pub max_distance: f32,
    pub step_distance: f32,
    pub density: f32,
    pub color: Vec3,
}

impl Default for FogProperties {
    fn default() -> Self {
        Self { 
            max_distance: 100.0,
            step_distance: 0.5,
            density: 0.02,
            color: Vec3::new(0.5, 0.55, 0.6)
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct FogMarchProps {
    csm: GpuCascades,
    out_size: [u32; 2],
    depth_tex: u32,
    out_tex: u32,
    downscale: u32,
    max_distance: f32,
    step_distance: f32,
    density: f32,
    fog_color: Vec3,
    noise_tex: u32,
    light_count: u32,
    _pad: [u32; 3]
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct FogCompositeProps {
    fog_tex: u32,
    depth_tex: u32,
    downscale: u32,
    _pad: u32,
    fog_size: [u32; 2],
}
