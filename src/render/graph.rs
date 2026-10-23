use crate::render::graph::pass::{DepthUse, Pass, PassCtx, ResLoad};
use crate::render::graph::res::{GraphResources, ResId};
use crate::render::texture::Texture;
use crate::wrappers::graphics::GraphicsDevice;
use anyhow::bail;
use std::collections::HashSet;
use std::time::Instant;
use windows::Win32::Graphics::Direct3D12::{ID3D12QueryHeap, ID3D12Resource, D3D12_QUERY_TYPE_TIMESTAMP, D3D12_RANGE, D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE};

pub mod pass;
pub mod res;
pub mod builder;

pub struct FrameGraph<'a> {
    passes: Vec<Pass<'a>>,
}

impl<'a> FrameGraph<'a> {
    pub fn new() -> Self {
        Self {
            passes: Vec::with_capacity(8)
        }
    }

    pub fn execute(self, gd: &GraphicsDevice, res: &GraphResources) -> anyhow::Result<Vec<(&'static str, f32)>> {
        let cmd = gd.command_list()?;
        let base = gd.frame_index()? * GraphicsDevice::MAX_TS;
        let frame = gd.frame_context()?;
        let passes = self.cull_passes(res);
        let (query_heap, readback, ms_per_tick) = gd.timings()?;
        let gpu = read_timing_slot(readback, base, &frame.ts_names.borrow(), ms_per_tick)?;
        let names: Vec<&'static str> = passes.iter().map(|(_, p)| p.name).collect();

        unsafe {
            cmd.EndQuery(query_heap, D3D12_QUERY_TYPE_TIMESTAMP, base as u32);
        }

        for (pass_idx, pass) in passes {
            GraphicsDevice::begin_event(cmd, pass.name);
            // transition all into readable state
            for r in &pass.reads {
                res.get(*r)?.transition(cmd, D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE);
            }

            let colors: Vec<&Texture> = pass.writes_color.iter().map(|it| res.get(it.0)).collect::<anyhow::Result<_>>()?;
            let depth = pass.uses_depth.iter().map(|it| res.get(it.0).map(|j| (j, it.2))).next().transpose()?;

            if pass.uses_backbuffer {
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
            unsafe {
                cmd.EndQuery(query_heap, D3D12_QUERY_TYPE_TIMESTAMP, (base + pass_idx + 1) as u32);
            }
            GraphicsDevice::end_event(cmd);
        }
        unsafe {
            cmd.ResolveQueryData(query_heap, D3D12_QUERY_TYPE_TIMESTAMP, base as u32, names.len() as u32 + 1, readback, (base * 8) as u64);
        }
        *frame.ts_names.borrow_mut() = names;

        Ok(gpu)
    }

    fn cull_passes(self, res: &GraphResources) -> Vec<(usize, Pass<'a>)> {
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

        self.passes.into_iter().enumerate().zip(alive).filter_map(|it| it.1.then_some(it.0)).collect()
    }
}

fn writes_depth(pass: &Pass) -> Vec<(ResId, ResLoad)> {
    if let Some(d) = pass.uses_depth && let DepthUse::Write(load) = d.1 {
        vec![(d.0, load)]
    } else {
        vec![]
    }
}

fn read_timing_slot(buf: &ID3D12Resource, base: usize, names: &[&'static str], ms_per_tick: f64) -> anyhow::Result<Vec<(&'static str, f32)>> {
    let n = names.len() + 1;
    let range = D3D12_RANGE { Begin: base * 8, End: (base + n) * 8 };
    unsafe {
        let mut ptr = std::ptr::null_mut();
        buf.Map(0, Some(&range), Some(&mut ptr))?;

        let ts = std::slice::from_raw_parts(ptr.cast::<u64>().add(base), n);
        let out = names.iter().zip(ts.windows(2))
            .map(|(name, w)| (*name, (w[1].wrapping_sub(w[0]) as f64 * ms_per_tick) as f32))
            .collect();
        buf.Unmap(0, Some(&D3D12_RANGE::default()));
        Ok(out)
    }
}