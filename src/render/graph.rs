use std::collections::HashSet;
use windows::Win32::Graphics::Direct3D12::{D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE, D3D12_RESOURCE_STATE_COMMON};
use crate::render::graph::builder::PassBuilder;
use crate::render::graph::pass::{Pass, PassCtx, ResLoad};
use crate::render::graph::res::{GraphResources, ResId};
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
        for pass in self.cull_passes() {
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

    fn cull_passes(self) -> Vec<Pass<'a>> {
        let mut needed_res: HashSet<ResId> = HashSet::new();
        let mut alive = vec![false; self.passes.len()];

        for (i, pass) in self.passes.iter().enumerate().rev() {
            // going from back to front
            let writes = pass.writes_color.iter().chain(pass.writes_depth.iter());
            let is_needed = pass.has_side_effects || writes.clone().any(|it| needed_res.contains(&it.0));
            if !is_needed {
                continue;
            }
            alive[i] = true;
            for (id, _) in pass.writes_color.iter().chain(pass.writes_depth.iter()) {
                needed_res.remove(id);
            }
            needed_res.extend(pass.reads.iter().copied());
            needed_res.extend(writes.filter(|it| it.1 == ResLoad::Preserve).map(|it| it.0))
        }

        self.passes.into_iter().zip(alive).filter_map(|it| it.1.then_some(it.0)).collect()
    }
}