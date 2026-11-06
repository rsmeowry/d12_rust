use windows::Win32::Graphics::Direct3D12::D3D12_CULL_MODE_NONE;
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use crate::render::deferred::GBuffer;
use crate::render::deferred::post::BlitProperties;
use crate::render::graph::FrameGraph;
use crate::render::graph::pass::ResLoad;
use crate::render::pipeline::desc::PipelineDesc;
use crate::render::pipeline::PipelineId;
use crate::render::renderer::{Renderer, GB_VELOCITY};

#[derive(Debug)]
pub struct SkyVelocityPass {
    pso: PipelineId,
}

impl SkyVelocityPass {
    pub fn new(rd: &mut Renderer) -> anyhow::Result<Self> {
        let pso = rd.create_pipeline(PipelineDesc {
            name: "SkyVelocity",
            shader: "deferred/SkyVelocity.hlsl".into(),
            rts: vec![GB_VELOCITY],
            depth: None,
            cull: D3D12_CULL_MODE_NONE,
            ..Default::default()
        })?;
        Ok(Self {
            pso
        })
    }

    pub fn add_pass<'a>(&self, gbuf: &GBuffer, renderer: &'a Renderer, graph: &mut FrameGraph<'a>) -> anyhow::Result<()> {
        let pass_va = renderer.upload_cbuffer(&[renderer.gr.get(gbuf.scene_depth)?.srv_ptr()])?;

        let rs = renderer.rs();
        let pipelines = renderer.pipelines();
        let pso = self.pso;

        graph.add_pass("SkyVelocity")
            .reads(gbuf.scene_depth)
            .writes(gbuf.velocity, ResLoad::Preserve)
            .exec(move |ctx| {
                pipelines.get(pso).bind(ctx.cmd);
                rs.set_pass(ctx.cmd, pass_va);
                rs.set_draw_index(ctx.cmd, 0);
                unsafe {
                    ctx.cmd.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                    ctx.cmd.DrawInstanced(3, 1, 0, 0);
                }
                Ok(())
            });

        Ok(())
    }
}
