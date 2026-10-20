use egui::epaint::Primitive;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct3D12::{D3D12_CULL_MODE_NONE, D3D12_INDEX_BUFFER_VIEW};
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R32_UINT;
use crate::render::graph::FrameGraph;
use crate::render::pipeline::desc::{Blend, PipelineDesc};
use crate::render::pipeline::PipelineId;
use crate::render::renderer::Renderer;
use crate::wrappers::graphics::GraphicsDevice;

#[derive(Debug)]
pub struct EguiPass {
    pass: PipelineId
}

impl EguiPass {
    pub fn new(rd: &mut Renderer) -> anyhow::Result<Self> {
        Ok(Self {
            pass: rd.create_pipeline(PipelineDesc {
                name: "Egui",
                shader: "ext/Egui.hlsl".into(),
                rts: vec![GraphicsDevice::BB_RTV_FORMAT],
                depth: None,
                blend: Blend::Premultiplied,
                cull: D3D12_CULL_MODE_NONE,
                ..Default::default()
            })?
        })
    }

    pub fn add_pass<'a>(&'a self,
                        prims: &[egui::ClippedPrimitive],
                        ppp: f32,
                        screen: egui::Vec2,
                        renderer: &'a Renderer,
                        graph: &mut FrameGraph<'a>) -> anyhow::Result<()> {
        let white = renderer.white_tex().srv_ptr();

        let mut draws = Vec::with_capacity(prims.len());

        for prim in prims {
            let Primitive::Mesh(msh) = &prim.primitive else { continue };
            if msh.indices.is_empty() {
                continue;
            }
            let min_x_px = (prim.clip_rect.min.x * ppp).round().clamp(0., screen.x) as i32;
            let max_x_px = (prim.clip_rect.max.x * ppp).round().clamp(0., screen.x) as i32;
            let min_y_px = (prim.clip_rect.min.y * ppp).round().clamp(0., screen.y) as i32;
            let max_y_px = (prim.clip_rect.max.y * ppp).round().clamp(0., screen.y) as i32;
            let scissor = RECT {
                left: min_x_px,
                top: min_y_px,
                right: max_x_px,
                bottom: max_y_px,
            };
            draws.push(EguiDraw {
                verts: renderer.upload_slice(bytemuck::cast_slice(&msh.vertices), 16)?,
                indices: renderer.upload_slice(bytemuck::cast_slice(&msh.indices), 4)?,
                count: msh.indices.len() as u32,
                tex: white,
                scissor,
            })
        }

        if draws.is_empty() {
            return Ok(())
        }

        let pipelines = renderer.pipelines();
        let rs = renderer.rs();
        graph
            .add_pass("EguiPass")
            .uses_backbuffer()
            .exec(move |ctx| {
                pipelines.get(self.pass).bind(ctx.cmd);
                unsafe {
                    ctx.cmd.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                }
                for draw in draws {
                    rs.set_objects(ctx.cmd, draw.verts);
                    rs.set_draw_index(ctx.cmd, draw.tex);
                    unsafe {
                        ctx.cmd.IASetIndexBuffer(Some(&D3D12_INDEX_BUFFER_VIEW {
                            BufferLocation: draw.indices,
                            SizeInBytes: (draw.count * 4) as u32,
                            Format: DXGI_FORMAT_R32_UINT,
                        }));
                        ctx.cmd.RSSetScissorRects(&[draw.scissor]);
                        ctx.cmd.DrawIndexedInstanced(draw.count, 1, 0, 0, 0);
                    }
                }
                Ok(())
            });

        Ok(())
    }
}

struct EguiDraw {
    verts: u64,
    indices: u64,
    count: u32,
    tex: u32,
    scissor: RECT,
}