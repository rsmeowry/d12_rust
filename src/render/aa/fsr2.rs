use std::fmt::{Debug, Formatter};
use std::mem::{transmute_copy, ManuallyDrop};
use anyhow::{anyhow, bail};
use fsr_sys::{Context, ContextCreate, ContextDescription, ContextDestroy, ContextDispatch, Dimensions2D, DispatchDescription, ErrorCode, FloatCoords2D, Resource, ResourceStates, FFX_FSR2_ENABLE_AUTO_EXPOSURE, FFX_FSR2_ENABLE_DEPTH_INFINITE, FFX_FSR2_ENABLE_DEPTH_INVERTED, FFX_FSR2_ENABLE_HIGH_DYNAMIC_RANGE, FFX_OK};
use fsr_sys::d3d12::{GetCommandListDX12, GetInterfaceDX12, GetResourceDX12, GetScratchMemorySizeDX12};
use windows::Win32::Graphics::Direct3D12::D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING;
use windows_core::{w, Interface, PCWSTR};
use crate::render::deferred::GBuffer;
use crate::render::graph::FrameGraph;
use crate::render::graph::res::{ResId, ResSize};
use crate::render::renderer::{Renderer, SCENE_FORMAT};
use crate::time::Time;
use crate::render::graph::pass::ResLoad;
use crate::render::shadows::CAM_NEAR;
use crate::render::texture::Texture;

pub struct Fsr2Pass {
    ctx: Option<FsrCtx>, // display size
    last_frame: Option<(u32, (u32, u32))>, // frame idx + display size
    target: ResId,
}

impl Fsr2Pass {
    pub fn new(rd: &mut Renderer) -> anyhow::Result<Fsr2Pass> {
        let target = rd.gr.storage(&rd.gd, "FSR2_Target", SCENE_FORMAT, ResSize::FitScreen)?;
        Ok(Self {
            ctx: None,
            last_frame: None,
            target
        })
    }

    pub fn add_pass<'a>(&'a mut self, gbuf: &GBuffer, rd: &'a Renderer, time: &Time, graph: &mut FrameGraph<'a>) -> anyhow::Result<ResId> {
        let (rw, rh) = rd.gr.render_size();
        let key = (rd.frame_count(), (rw, rh));

        // just started or skipped a frame, or resolution changed
        let did_reset = if let Some(lf) = self.last_frame { lf.0 != key.0.wrapping_sub(1) || (lf.1.0 != key.1.0 || lf.1.1 != key.1.1) } else { true };
        self.last_frame = Some(key);

        let tgt = self.target;
        let color = rd.gr.get(gbuf.scene_color)?;
        let depth = rd.gr.get(gbuf.scene_depth)?;
        let velocity = rd.gr.get(gbuf.velocity)?;
        let target = rd.gr.get(self.target)?;

        let j = rd.jitter();
        let dt_ms = time.delta_time() * 1000.;
        let fov = rd.cam.fov_rad();
        let ctx = self.try_context_mut(rd)?;

        graph.add_pass("FSR2")
            .reads(gbuf.scene_color)
            .reads(gbuf.scene_depth)
            .reads(gbuf.velocity)
            .writes_uav(tgt, ResLoad::Preserve)
            .exec(move |pc| {
                let mut d = DispatchDescription {
                    color: fsr_res(color, ResourceStates::COMPUTE_READ, w!("FSR2_Color")),
                    depth: fsr_res(depth, ResourceStates::COMPUTE_READ, w!("FSR2_Depth")),
                    motionVectors: fsr_res(velocity, ResourceStates::COMPUTE_READ, w!("FSR2_Velocity")),
                    output: fsr_res(target, ResourceStates::UNORDERED_ACCESS, w!("FSR2_Target")),
                    jitterOffset: FloatCoords2D { x: j.x, y: j.y },
                    motionVectorScale: FloatCoords2D { x: rw as f32, y: rh as f32 },
                    renderSize: Dimensions2D { width: rw, height: rh },
                    frameTimeDelta: dt_ms,
                    preExposure: 1.0,
                    viewSpaceToMetersFactor: 1.0,
                    cameraNear: CAM_NEAR,
                    cameraFar: 1e12,
                    cameraFovAngleVertical: fov,
                    reset: did_reset,
                    commandList: unsafe { GetCommandListDX12(pc.cmd.as_raw()) },
                    ..Default::default()
                };
                unsafe {
                    fsr_ensure(ContextDispatch(&mut *ctx.ctx, &d))?;
                }
                // rebinding!!
                rd.rebind_root(pc.cmd)
            });

        Ok(tgt)
    }

    fn try_context_mut(&mut self, rd: &Renderer) -> anyhow::Result<&mut FsrCtx> {
        let display_size = rd.gr.screen_size();
        if self.ctx.as_ref().is_some_and(|c| c.display_size.0 != display_size.0 || c.display_size.1 != display_size.1) {
            // have context, but for different resolution, force recreate
            rd.gd.flush_command_queue()?;
            self.ctx = None;
        }
        if self.ctx.is_none() {
            unsafe {
                let mut dev = rd.gd.device().as_raw();
                let mut scratch = vec![0; GetScratchMemorySizeDX12().div_ceil(8)];
                let mut desc = ContextDescription {
                    device: dev,
                    flags: (FFX_FSR2_ENABLE_DEPTH_INVERTED |
                        FFX_FSR2_ENABLE_DEPTH_INFINITE |
                        FFX_FSR2_ENABLE_HIGH_DYNAMIC_RANGE |
                        FFX_FSR2_ENABLE_AUTO_EXPOSURE) as u32,
                    maxRenderSize: Dimensions2D { width: display_size.0, height: display_size.1 },
                    displaySize: Dimensions2D { width: display_size.0, height: display_size.1 },
                    ..Default::default()
                };
                fsr_ensure(GetInterfaceDX12(&mut desc.callbacks, dev, scratch.as_mut_ptr().cast(), scratch.len() * 8))?;
                let mut ctx = Box::default();
                fsr_ensure(ContextCreate(&mut *ctx, &desc))?;
                self.ctx = Some(FsrCtx {
                    display_size,
                    ctx,
                    scratch,
                });
            }
        }

        Ok(self.ctx.as_mut().unwrap())
    }
}

impl Debug for Fsr2Pass {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Fsr2Pass")
    }
}

struct FsrCtx {
    ctx: Box<Context>,
    scratch: Vec<u64>,
    display_size: (u32, u32)
}

impl Drop for FsrCtx {
    fn drop(&mut self) {
        unsafe {
            ContextDestroy(&mut *self.ctx);
        }
    }
}

fn fsr_ensure(err_c: ErrorCode) -> anyhow::Result<()> {
    if err_c == FFX_OK { Ok(()) } else { bail!("FSR2 Error: {err_c:#x}")}
}

fn fsr_res(tex: &Texture, state: ResourceStates, name: PCWSTR) -> Resource {
    unsafe { GetResourceDX12(std::ptr::null_mut(), tex.resource().as_raw(), name.0, state, D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING) }
}