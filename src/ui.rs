use crate::render::graph::FrameGraph;
use crate::render::pipeline::desc::{Blend, PipelineDesc};
use crate::render::pipeline::PipelineId;
use crate::render::renderer::Renderer;
use crate::render::texture::{MipCount, Texture};
use crate::wrappers::graphics::batching::UploadBatch;
use crate::wrappers::graphics::GraphicsDevice;
use egui::epaint::{ImageDelta, Primitive};
use egui::{ColorImage, ImageData, TextureId, TexturesDelta};
use std::collections::HashMap;
use std::sync::Arc;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use windows::Win32::Graphics::Direct3D12::{D3D12_CULL_MODE_NONE, D3D12_INDEX_BUFFER_VIEW};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R32_UINT;

#[derive(Debug)]
pub struct EguiPass {
    pass: PipelineId,
    textures: HashMap<u64, (Texture, Arc<ColorImage>)>,
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
            })?,
            textures: HashMap::new(),
        })
    }

    pub fn update_textures(&mut self, gd: &GraphicsDevice, delta: &TexturesDelta) -> anyhow::Result<()> {
        if delta.is_empty() {
            return Ok(());
        }

        gd.flush_command_queue()?;

        let mut batch = gd.begin_upload()?;

        for (tex, deltas) in &delta.set {
            if let TextureId::Managed(id) = tex {
                for delta in deltas {
                    let entry = self.textures.get_mut(id);
                    let ImageData::Color(new_color) = &delta.image;
                    match (entry, delta.pos) {
                        (Some((tex, color)), None) => {
                            // patching whole tex
                            *Arc::make_mut(color) = new_color.as_ref().clone();
                            tex.rewrite(
                                &mut batch,
                                new_color.size[0] as u32,
                                new_color.size[1] as u32,
                                bytemuck::cast_slice(&new_color.pixels),
                                MipCount::Limit(1)
                            )?;
                        },
                        (None, None) => {
                            // creating a new tex
                            self.textures.insert(*id, new_tex_from_delta(&mut batch, deltas)?);
                        }
                        (Some((tex, color)), Some(pos)) => {
                            // patching a segment
                            patch(Arc::make_mut(color), &new_color, pos);
                            tex.rewrite(
                                &mut batch,
                                color.size[0] as u32,
                                color.size[1] as u32,
                                bytemuck::cast_slice(&color.pixels),
                                MipCount::Limit(1)
                            )?;
                        },
                        (None, Some(_)) => anyhow::bail!("Tried to patch invalid tex")
                    }
                }
            }
        }

        batch.finish()?;

        Ok(())
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
            let tex = if let TextureId::Managed(tex_id) = msh.texture_id { self.textures.get(&tex_id) } else { None };
            let tex = tex.map(|it| it.0.srv_ptr()).unwrap_or(white);
            draws.push(EguiDraw {
                verts: renderer.upload_slice(bytemuck::cast_slice(&msh.vertices), 16)?,
                indices: renderer.upload_slice(bytemuck::cast_slice(&msh.indices), 4)?,
                count: msh.indices.len() as u32,
                tex: tex,
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

fn new_tex_from_delta(batch: &mut UploadBatch, deltas: &[ImageDelta]) -> anyhow::Result<(Texture, Arc<ColorImage>)> {
    let delta = deltas.first().ok_or(anyhow::anyhow!("No deltas found!"))?;
    if let Some(_) = delta.pos {
        anyhow::bail!("Can not create new tex from non-uninit deltas!")
    }
    let ImageData::Color(color) = &delta.image;
    Ok((
        Texture::from_rgba8(batch, delta.image.width() as u32, delta.image.height() as u32, bytemuck::cast_slice(&color.pixels), true, MipCount::Limit(1), "TextureDelta")?,
        color.clone()
    ))

}

fn patch(dst: &mut ColorImage, src: &Arc<ColorImage>, [x, y]: [usize; 2]) {
    let w = src.size[0];
    for row in 0..src.size[1] {
        let d = (y + row) * dst.size[0] + x;
        dst.pixels[d..d + w].copy_from_slice(&src.pixels[row * w..(row + 1) * w]);
    }
}