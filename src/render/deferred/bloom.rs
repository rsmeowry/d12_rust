use crate::render::graph::pass::ResLoad;
use crate::render::graph::res::{ResId, ResSize};
use crate::render::graph::FrameGraph;
use crate::render::pipeline::desc::PipelineDesc;
use crate::render::pipeline::PipelineId;
use crate::render::renderer::{Renderer, SCENE_FORMAT};
use anyhow::anyhow;
use glam::Vec4;

#[derive(Debug)]
pub struct BloomPass {
    down: PipelineId,
    up: PipelineId,
    down_mips: Vec<ResId>,
    up_mips: Vec<ResId>,
}

impl BloomPass {
    pub const LEVELS: u32 = 6;

    pub fn new(renderer: &mut Renderer) -> anyhow::Result<Self> {
        let down = renderer.create_pipeline(PipelineDesc {
            name: "BloomDown",
            shader: "compute/bloom/BloomDown.compute.hlsl".into(),
            ..Default::default()
        })?;
        let up = renderer.create_pipeline(PipelineDesc {
            name: "BloomUp",
            shader: "compute/bloom/BloomUp.compute.hlsl".into(),
            ..Default::default()
        })?;

        let down_mips = (1..=Self::LEVELS)
            .map(|i| renderer.gr.storage(&renderer.gd, &format!("BloomDown_{i}"), SCENE_FORMAT, ResSize::ScreenDiv(1 << i)))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let up_mips = (1..Self::LEVELS)
            .map(|i| renderer.gr.storage(&renderer.gd, &format!("BloomUp_{i}"), SCENE_FORMAT, ResSize::ScreenDiv(1 << i)))
            .collect::<anyhow::Result<Vec<_>>>()?;

        Ok(Self { down, up, down_mips, up_mips })
    }

    // returns final bloom texture
    pub fn add_pass<'a>(&'a self, props: BloomProperties, source: ResId, renderer: &'a Renderer, graph: &mut FrameGraph<'a>) -> anyhow::Result<ResId> {
        let gr = &renderer.gr;
        let rs = renderer.rs();
        let pipelines = renderer.pipelines();
        let uav = |id: ResId| gr.get(id)?.uav_ptr().ok_or_else(|| anyhow!("Bloom texture {id:?} has no UAV!"));

        let mut src = source;
        for (i, &dst) in self.down_mips.iter().enumerate() {
            let s = *gr.get(src)?.desc();
            let d = *gr.get(dst)?.desc();
            let va = renderer.upload_cbuffer(&BloomDownProps {
                target_size: [d.width, d.height],
                source_texel: [1.0 / s.width as f32, 1.0 / s.height as f32],
                threshold: props.threshold,
                knee: props.knee,
                is_first_pass: (i == 0) as u32,
                intensity: props.intensity,
                tint: [1.0, 0.9, 0.5, 1.0],
                scene_tex: gr.get(src)?.srv_ptr(),
                out_tex: uav(dst)?,
            })?;
            let down = self.down;
            graph.add_pass("BloomDown")
                .reads(src)
                .writes_uav(dst, ResLoad::Preserve)
                .exec(move |ctx| {
                    pipelines.get(down).bind(ctx.cmd);
                    rs.set_compute_pass(ctx.cmd, va);
                    unsafe { ctx.cmd.Dispatch(d.width.div_ceil(16), d.height.div_ceil(16), 1) };
                    Ok(())
                });
            src = dst;
        }

        // going from the smallest level up: up[i] = down[i] + tent(up[i + 1]), lowest level is just the last down
        let mut lower = src;
        for i in (0..self.up_mips.len()).rev() {
            let (same, out) = (self.down_mips[i], self.up_mips[i]);
            let l = *gr.get(lower)?.desc();
            let o = *gr.get(out)?.desc();
            let va = renderer.upload_cbuffer(&BloomUpProps {
                target_size: [o.width, o.height],
                lower_texel: [1.0 / l.width as f32, 1.0 / l.height as f32],
                radius: props.radius,
                lower_tex: gr.get(lower)?.srv_ptr(),
                same_tex: gr.get(same)?.srv_ptr(),
                out_tex: uav(out)?,
            })?;
            let up = self.up;
            graph.add_pass("BloomUp")
                .reads(lower)
                .reads(same)
                .writes_uav(out, ResLoad::Preserve)
                .exec(move |ctx| {
                    pipelines.get(up).bind(ctx.cmd);
                    rs.set_compute_pass(ctx.cmd, va);
                    unsafe { ctx.cmd.Dispatch(o.width.div_ceil(16), o.height.div_ceil(16), 1) };
                    Ok(())
                });
            lower = out;
        }

        Ok(lower)
    }
}

#[derive(Debug, Copy, Clone)]
pub struct BloomProperties {
    pub threshold: f32,
    pub knee: f32,
    pub radius: f32,
    pub intensity: f32,
}

impl Default for BloomProperties {
    fn default() -> Self {
        Self { threshold: 1.0, knee: 0.5, radius: 1.0, intensity: 1.0 }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct BloomDownProps {
    target_size: [u32; 2],
    source_texel: [f32; 2],
    threshold: f32,
    knee: f32,
    is_first_pass: u32,
    intensity: f32,
    tint: [f32; 4],
    scene_tex: u32,
    out_tex: u32,
}
const _: () = assert!(std::mem::offset_of!(BloomDownProps, tint) == 32);

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct BloomUpProps {
    target_size: [u32; 2],
    lower_texel: [f32; 2],
    radius: f32,
    lower_tex: u32,
    same_tex: u32,
    out_tex: u32,
}
