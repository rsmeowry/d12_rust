use std::path::PathBuf;
use glam::Vec4;
use windows::Win32::Graphics::Direct3D12::D3D12_CULL_MODE_FRONT;
use crate::render::deferred::GBuffer;
use crate::render::graph::FrameGraph;
use crate::render::graph::pass::ResLoad;
use crate::render::graph::res::{ResId, ResSize};
use crate::render::mesh::Mesh;
use crate::render::pipeline::desc::{Blend, PipelineDesc};
use crate::render::pipeline::PipelineId;
use crate::render::renderer::{ObjectData, Renderer, SCENE_FORMAT};
use crate::render::vertex::Vertex;

#[derive(Debug)]
pub struct TransparentPass {
    pso: PipelineId,
    target_tex: ResId,
}

impl TransparentPass {
    pub fn target_tex(&self) -> ResId {
        self.target_tex
    }

    pub fn new<V: Vertex, P: Into<PathBuf>>(rd: &mut Renderer, name: &'static str, path: P) -> anyhow::Result<Self> {
        let pso = rd.create_pipeline(PipelineDesc {
            name,
            shader: path.into(),
            input: V::LAYOUT,
            rts: vec![SCENE_FORMAT],
            ..Default::default()
        })?;
        let target_tex = rd.gr.color(&rd.gd, "TransparentsOut", SCENE_FORMAT, Vec4::ZERO, ResSize::FitRender)?;
        Ok(Self {
            pso,
            target_tex,
        })
    }

    pub fn add_pass<'a>(&'a self, mesh: &'a Mesh, object: &ObjectData, rd: &'a Renderer, gbuf: &GBuffer, graph: &mut FrameGraph<'a>) -> anyhow::Result<ResId> {
        let pass_va = rd.upload_cbuffer(&TransparentPassData {
            depth_tex: rd.gr.get(gbuf.scene_depth)?.srv_ptr(),
            scene_tex: rd.gr.get(gbuf.scene_color)?.srv_ptr()
        })?;
        let objects_va = rd.upload_objects(&[*object])?;

        let pso = self.pso;
        let rs = rd.rs();
        let pipelines = rd.pipelines();

        graph
            .add_pass("Transparents")
            .reads(gbuf.scene_depth)
            .reads(gbuf.scene_color)
            .writes(self.target_tex, ResLoad::Preserve)
            .exec(move |ctx| {
                rs.set_objects(ctx.cmd, objects_va);
                rs.set_pass(ctx.cmd, pass_va);
                pipelines.get(self.pso).bind(ctx.cmd);
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
    scene_tex: u32
}