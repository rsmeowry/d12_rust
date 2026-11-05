use glam::{Vec2, Vec3};
use crate::render::graph::FrameGraph;
use crate::render::graph::pass::ResLoad;
use crate::render::graph::res::{ResId, ResSize};
use crate::render::pipeline::desc::PipelineDesc;
use crate::render::pipeline::PipelineId;
use crate::render::renderer::{Renderer, SCENE_FORMAT};
use crate::time::Time;

pub fn halton_sequence(mut index: u32, base: u32) -> f32 {
    let mut f = 1.;
    let mut res = 0.;
    let base_f = base as f32;

    while(index > 0) {
        f /= base_f;
        res += f * (index % base) as f32;
        index /= base;
    }

    res
}

pub fn jitter_vec(frame: u32, phases: u32) -> Vec2 {
    let frame_idx = (frame % phases + 1) as u32;
    Vec2::new(halton_sequence(frame_idx, 2), halton_sequence(frame_idx, 3)) - 0.5
}

#[derive(Debug)]
pub struct TaaPass {
    history: [ResId; 2],
    cs: PipelineId,
}

impl TaaPass {
    pub fn new(rd: &mut Renderer) -> anyhow::Result<Self> {
        let history = [0, 1].map(|each| rd.gr.storage(&rd.gd, &format!("TAA_History_{each}"), SCENE_FORMAT, ResSize::FitScreen).expect("Failed to init TAA storage texture"));
        let cs = rd.create_pipeline(PipelineDesc {
            name: "TAAResolve",
            shader: "compute/aa/TAAResolve.compute.hlsl".into(),
            ..Default::default()
        })?;
        Ok(Self {
            history,
            cs
        })
    }

    fn prev_current(&self, frame_count: u32) -> (ResId, ResId) {
        (self.history[(frame_count as usize & 1) ^ 1], self.history[frame_count as usize & 1])
    }

    /// returns current texture (uav, just written)
    pub fn add_pass<'a>(&self, scene_color: ResId, rd: &'a Renderer, graph: &mut FrameGraph<'a>) -> anyhow::Result<ResId> {
        let (prev, current) = self.prev_current(rd.frame_count());
        let rs = rd.rs();
        let pipelines = rd.pipelines();
        let pass_va = rd.upload_cbuffer(&TaaData {
            prev_tex: rd.gr.get(prev)?.srv_ptr(),
            current_tex: rd.gr.get(scene_color)?.srv_ptr(),
            out_tex: rd.gr.get(current)?.uav_ptr().unwrap(),
        })?;
        let d = rd.gr.get(scene_color)?.desc();
        let cs = self.cs;
        graph.add_pass("TAA_Resolve")
            .reads(prev)
            .reads(scene_color)
            .writes_uav(current, ResLoad::Preserve)
            .exec(move |ctx| {
                pipelines.get(cs).bind(ctx.cmd);
                rs.set_compute_pass(ctx.cmd, pass_va);
                unsafe { ctx.cmd.Dispatch(d.width.div_ceil(32), d.height.div_ceil(32), 1) };
                Ok(())
            });

        Ok(current)
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct TaaData {
    prev_tex: u32,
    current_tex: u32,
    out_tex: u32,
}