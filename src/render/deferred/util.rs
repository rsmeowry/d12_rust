use glam::Vec4;
use windows::Win32::Graphics::Direct3D12::D3D12_CULL_MODE_NONE;
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use crate::render::graph::FrameGraph;
use crate::render::pipeline::desc::PipelineDesc;
use crate::render::pipeline::PipelineId;
use crate::render::renderer::Renderer;
use crate::render::texture::Texture;
use crate::render::txt::{build_ascii_texture, display_text};
use crate::wrappers::graphics::GraphicsDevice;

#[derive(Debug)]
pub struct DebugTextPass {
    text: PipelineId,
    text_tex: Texture
}

impl DebugTextPass {
    pub fn new(renderer: &mut Renderer) -> anyhow::Result<Self> {
        let mut batch = renderer.gd.begin_upload()?;
        let text_tex = build_ascii_texture(&mut batch)?;
        batch.finish()?;

        let text = renderer.create_pipeline(PipelineDesc {
            rts: vec![GraphicsDevice::BB_RTV_FORMAT],
            name: "Text",
            shader: "Text.hlsl".into(),
            input: &[],
            depth: None,
            cull: D3D12_CULL_MODE_NONE,
            ..Default::default()
        })?;

        Ok(Self {
            text,
            text_tex
        })
    }

    pub fn add_pass<'a, S: Into<String>>(&'a self, text: S, width: usize, renderer: &'a Renderer, graph: &mut FrameGraph<'a>) -> anyhow::Result<()> {
        let glyphs = display_text(
            &text.into(), width as u32, 1.0,
            Vec4::new(1.0, 1.0, 0.0, 1.0), true);
        let glyphs_va = renderer.upload_slice(bytemuck::cast_slice(&glyphs), 16)?;
        let text_tex = self.text_tex.srv_ptr();

        let pipelines = renderer.pipelines();
        let rs = renderer.rs();

        graph.add_pass("TextOverlay")
            .uses_backbuffer()
            .exec(move |ctx| {
                pipelines.get(self.text).bind(ctx.cmd);
                rs.set_objects(ctx.cmd, glyphs_va);
                rs.set_draw_index(ctx.cmd, text_tex);
                unsafe {
                    ctx.cmd.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                    ctx.cmd.DrawInstanced(6, glyphs.len() as u32, 0, 0);
                }
                Ok(())
            });

        Ok(())
    }
}