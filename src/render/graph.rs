use crate::render::graph::pass::{DepthUse, Pass, PassCtx, ResLoad};
use crate::render::graph::res::{GraphResources, ResId};
use crate::render::texture::Texture;
use crate::wrappers::graphics::GraphicsDevice;
use anyhow::bail;
use std::collections::HashSet;
use windows::Win32::Graphics::Direct3D12::D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE;

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
        for pass in self.cull_passes(res) {
            // transition all into readable state
            for r in &pass.reads {
                res.get(*r)?.transition(cmd, D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE);
            }

            let colors: Vec<&Texture> = pass.writes_color.iter().map(|it| res.get(it.0)).collect::<anyhow::Result<_>>()?;
            let depth = pass.uses_depth.iter().map(|it| res.get(it.0).map(|j| (j, it.2))).next().transpose()?;

            if pass.has_side_effects {
                gd.bind_backbuffer(depth.map(|d| d.0))?;
            } else if !colors.is_empty() || depth.is_some() {
                gd.set_render_targets(&colors, depth, pass.uses_depth.is_some_and(|d| d.1 == DepthUse::Test))?;
            }

            for (id, load) in &pass.writes_color {
                if *load == ResLoad::Clear { gd.clear(res.get(*id)?)?; }
            }
            if let Some((id, DepthUse::Write(ResLoad::Clear), slice)) = pass.uses_depth {
                gd.clear_depth(res.get(id)?, slice)?;
            }

            // UAVs
            for (id, load) in pass.writes_uav {
                let res = res.get(id)?;
                res.ensure_unordered_access(gd.command_list()?);

                if load == ResLoad::Clear {
                    bail!("Graph does not currently support clearing UAVs")
                }
            }

            (pass.exec)(&PassCtx::new(&cmd, gd, res))?;
        }
        Ok(())
    }

    fn cull_passes(self, res: &GraphResources) -> Vec<Pass<'a>> {
        let mut needed_res: HashSet<ResId> = HashSet::new();
        let mut alive = vec![false; self.passes.len()];

        for (i, pass) in self.passes.iter().enumerate().rev() {
            // going from back to front
            let depth = writes_depth(&pass);
            let writes = pass.writes_color.iter().chain(depth.iter()).chain(pass.writes_uav.iter());
            let is_needed = pass.has_side_effects || writes.clone().any(|it| needed_res.contains(&it.0));
            if !is_needed {
                continue;
            }
            alive[i] = true;
            for (id, _) in pass.writes_color.iter().chain(writes_depth(&pass).iter()) {
                if !res.get(*id).unwrap().desc().is_cube { needed_res.remove(id); }
            }
            if let Some(depth) = pass.uses_depth && depth.1 == DepthUse::Test {
                needed_res.insert(depth.0);
            }
            needed_res.extend(pass.reads.iter().copied());
            needed_res.extend(writes.filter(|it| it.1 == ResLoad::Preserve).map(|it| it.0))
        }

        self.passes.into_iter().zip(alive).filter_map(|it| it.1.then_some(it.0)).collect()
    }
}

fn writes_depth(pass: &Pass) -> Vec<(ResId, ResLoad)> {
    if let Some(d) = pass.uses_depth && let DepthUse::Write(load) = d.1 {
        vec![(d.0, load)]
    } else {
        vec![]
    }
}