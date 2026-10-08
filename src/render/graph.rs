use windows::Win32::Graphics::Direct3D12::{D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE, D3D12_RESOURCE_STATE_COMMON};
use crate::render::graph::builder::PassBuilder;
use crate::render::graph::pass::{Pass, PassCtx, ResLoad};
use crate::render::graph::res::GraphResources;
use crate::render::texture::Texture;
use crate::wrappers::graphics::GraphicsDevice;

pub mod pass;
pub mod res;
pub mod builder;

pub struct FrameGraph<'a> {
    passes: Vec<Pass<'a>>
}

impl<'a> FrameGraph<'a> {
    pub fn new() -> Self {
        Self { passes: Vec::new() }
    }

    pub fn execute(self, gd: &GraphicsDevice, res: &GraphResources) -> anyhow::Result<()> {
        let cmd = gd.command_list()?;
        for pass in self.passes {
            // transition all into readable state
            for r in &pass.reads {
                res.get(*r)?.transition(cmd, D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE);
            }

            let colors: Vec<&Texture> = pass.writes_color.iter().map(|it| res.get(it.0)).collect::<anyhow::Result<_>>()?;
            let depth = pass.writes_depth.iter().map(|it| res.get(it.0)).next().transpose()?;
            if pass.has_side_effects {
                gd.bind_backbuffer(depth)?;
            } else if !colors.is_empty() || depth.is_some() {
                gd.set_render_targets(&colors, depth)?;
            }
            for (id, load) in pass.writes_color.iter().chain(pass.writes_depth.iter()) {
                if *load == ResLoad::Clear {
                    gd.clear(res.get(*id)?)?;
                }
            }
            (pass.exec)(&PassCtx::new(&cmd, gd, res))?;
        }
        Ok(())
    }
}