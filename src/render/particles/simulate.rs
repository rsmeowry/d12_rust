use std::cell::Cell;
use glam::{Vec2, Vec3};
use windows::Win32::Graphics::Direct3D12::{ID3D12CommandSignature, ID3D12GraphicsCommandList, ID3D12Resource, D3D12_BUFFER_UAV, D3D12_BUFFER_UAV_FLAG_NONE, D3D12_BUFFER_UAV_FLAG_RAW, D3D12_GPU_DESCRIPTOR_HANDLE, D3D12_RESOURCE_STATE_COPY_SOURCE, D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT, D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE, D3D12_RESOURCE_STATE_UNORDERED_ACCESS, D3D12_UAV_DIMENSION_BUFFER, D3D12_UNORDERED_ACCESS_VIEW_DESC, D3D12_UNORDERED_ACCESS_VIEW_DESC_0};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_R32_TYPELESS, DXGI_FORMAT_UNKNOWN};
use crate::render::deferred::GBuffer;
use crate::render::graph::FrameGraph;
use crate::render::particles::PARTICLE_STRIDE;
use crate::render::pipeline::desc::PipelineDesc;
use crate::render::pipeline::PipelineId;
use crate::render::renderer::Renderer;
use crate::wrappers::graphics::{transition_barrier, uav_barrier, GraphicsDevice};

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ComputeParticlesPass {
    pub depth_tex: u32,
    pub normal_tex: u32,
    pub counters_buf: u32,

    pub max_particles: u32,
    pub emitter_pos: Vec3,
    pub emit_count: u32,

    pub gravity: Vec3,
    pub dt: f32,
    pub color_a: Vec3,
    pub drag: f32,
    pub color_b: Vec3,
    pub sort_order: f32, // +z or -z
    pub life_range: Vec2,
    pub size_range: Vec2,
    pub velocity_base: Vec3,

    pub spread: f32,
    pub restitution: f32,
    pub thickness: f32,

    pub _pad: [u32; 2]
}

#[derive(Debug, Copy, Clone)]
pub struct SimulateParticleProps {
    pub emitter: Vec3,
    pub gravity: Vec3,
    pub rate: f32,
    pub spread: f32,
    pub drag: f32,
    pub lifetime: f32,
    pub velocity_base: Vec3,
}

impl Default for SimulateParticleProps {
    fn default() -> Self {
        Self {
            emitter: Vec3::new(0., 1., 0.),
            gravity: Vec3::new(0., -9.8, 0.),
            rate: 100.,
            spread: 1.5,
            drag: 0.3,
            lifetime: 1.0,
            velocity_base: Vec3::Z,
        }
    }
}


#[derive(Debug)]
pub struct ParticleSimulation {
    max: u32,
    particles_buf: ID3D12Resource,
    dead_list: ID3D12Resource,
    sort_list: ID3D12Resource,
    counters: ID3D12Resource,
    readback: ID3D12Resource,
    table: D3D12_GPU_DESCRIPTOR_HANDLE, // u0 pool, u1 deadlist, u2 sortlist
    counters_uav: u32,
    cs_init: PipelineId,
    cs_prepare: PipelineId,
    cs_emit: PipelineId,
    cs_simulate: PipelineId,
    initialized: Cell<bool>,
    emit_accum: Cell<f32>
}

impl ParticleSimulation {
    pub fn new(rd: &mut Renderer, max: u32) -> anyhow::Result<Self> {
        anyhow::ensure!(max.is_power_of_two() && max >= 256, "max number must be a power of two!");

        let mut cs = |name: &'static str| rd.create_pipeline(PipelineDesc {
            name,
            shader: format!("compute/particles/{name}.compute.hlsl").into(),
            ..Default::default()
        });
        let cs_init = cs("Init")?;
        let cs_prepare = cs("Prepare")?;
        let cs_emit = cs("Emit")?;
        let cs_simulate = cs("Simulate")?;

        let gd = &rd.gd;
        let dev = gd.device();

        let particles_buf = GraphicsDevice::create_uav_buffer(dev, (max * PARTICLE_STRIDE) as usize, "ParticlesPool")?;
        let dead_list = GraphicsDevice::create_uav_buffer(dev, (max * 4) as usize, "ParticlesDeadList")?;
        let sort_list = GraphicsDevice::create_uav_buffer(dev, (max * 4 * 2) as usize, "ParticlesSortList")?;
        let counters = GraphicsDevice::create_uav_buffer(dev, 8192 + 4, "ParticlesCounters")?;
        let readback = GraphicsDevice::create_readback_buffer(dev, 8, "ParticlesReadback")?;

        // descriptors
        let heap = gd.srv_heap()?;
        let first = heap.alloc()?;
        let table = heap.gpu(first);
        heap.alloc()?;
        heap.alloc()?;
        let counters_uav = heap.alloc()?;

        let view = |buf: &ID3D12Resource, stride: u32, counter: Option<u32>, slot: u32| unsafe {
            dev.CreateUnorderedAccessView(buf, counter.map(|_| &counters), Some(&D3D12_UNORDERED_ACCESS_VIEW_DESC {
                Format: DXGI_FORMAT_UNKNOWN,
                ViewDimension: D3D12_UAV_DIMENSION_BUFFER,
                Anonymous: D3D12_UNORDERED_ACCESS_VIEW_DESC_0 { Buffer: D3D12_BUFFER_UAV {
                    FirstElement: 0,
                    NumElements: max,
                    StructureByteStride: stride,
                    CounterOffsetInBytes: counter.unwrap_or(0) as u64,
                    Flags: D3D12_BUFFER_UAV_FLAG_NONE,
                }},
            }), heap.cpu(slot))
        };
        // particles
        view(&particles_buf, PARTICLE_STRIDE, None, first);
        // deadlist
        view(&dead_list, 4, Some(0), first + 1);
        // sortlist
        view(&sort_list, 4 * 2, Some(4096), first + 2);
        // counters
        unsafe {
            dev.CreateUnorderedAccessView(&counters, None, Some(&D3D12_UNORDERED_ACCESS_VIEW_DESC {
                Format: DXGI_FORMAT_R32_TYPELESS,
                ViewDimension: D3D12_UAV_DIMENSION_BUFFER,
                Anonymous: D3D12_UNORDERED_ACCESS_VIEW_DESC_0 { Buffer: D3D12_BUFFER_UAV {
                    NumElements: ((8192 + 4) / 4) as u32, Flags: D3D12_BUFFER_UAV_FLAG_RAW, ..Default::default()
                }},
            }), heap.cpu(counters_uav));
        }

        Ok(Self {
            max,
            particles_buf,
            dead_list,
            sort_list,
            counters,
            readback,
            table,
            counters_uav,
            cs_init,
            cs_prepare,
            cs_emit,
            cs_simulate,
            initialized: Cell::new(false),
            emit_accum: Cell::new(0.0),
        })
    }

    pub fn reset(&self) {
        self.initialized.set(false);
    }

    pub fn add_pass<'a>(&'a self, props: SimulateParticleProps, dt: f32, rd: &'a Renderer, gbuf: &GBuffer, graph: &mut FrameGraph<'a>) -> anyhow::Result<()> {
        // gathering sim data
        let dt = dt.min(1.0 / 30.);
        let acc = self.emit_accum.get() + props.rate * dt;
        self.emit_accum.set(acc.fract());
        let emit_amt = acc as u32;
        let should_init = !self.initialized.replace(true);

        // pass va
        let pass_va = rd.upload_cbuffer(&ComputeParticlesPass {
            depth_tex: rd.gr.get(gbuf.scene_depth)?.srv_ptr(),
            normal_tex: rd.gr.get(gbuf.normals)?.srv_ptr(),
            counters_buf: self.counters_uav,
            max_particles: self.max,
            emitter_pos: props.emitter,
            emit_count: emit_amt,
            gravity: props.gravity,
            dt,
            color_a: Vec3::ONE,
            drag: props.drag,
            color_b: Vec3::ZERO,
            sort_order: 1.0,
            life_range: Vec2::new(props.lifetime, props.lifetime),
            size_range: Vec2::new(0.1, 0.1),
            velocity_base: props.velocity_base,
            spread: props.spread,
            restitution: 0.5,
            thickness: 0.5,
            _pad: [0; 2],
        })?;


        let sim_data = SimData {
            pass_va,
            emit_amt,
            should_init
        };

        // adding a sim pass
        graph.add_pass("SimPass")
            .reads(gbuf.scene_depth)
            .reads(gbuf.normals)
            .has_side_effects()
            .exec(move |ctx| {
                self.perform_sim(ctx.cmd, rd, &sim_data)
            })
        ;

        Ok(())
    }

    fn perform_sim(&self, cmd: &ID3D12GraphicsCommandList, rd: &Renderer, sim: &SimData) -> anyhow::Result<()> {
        let rs = rd.rs();
        let pipelines = rd.pipelines();

        rs.set_compute_particles(cmd, self.table);
        rs.set_compute_pass(cmd, sim.pass_va);

        if sim.should_init {
            // init
            pipelines.get(self.cs_init).bind(cmd);
            unsafe {
                cmd.Dispatch(self.max.div_ceil(256), 1, 1);
                cmd.ResourceBarrier(&[uav_barrier(&self.particles_buf), uav_barrier(&self.dead_list), uav_barrier(&self.counters)]);
            }
        }

        // prepare
        pipelines.get(self.cs_prepare).bind(cmd);
        unsafe {
            cmd.Dispatch(1, 1, 1);
            cmd.ResourceBarrier(&[uav_barrier(&self.counters)]);
        }

        // emit
        if sim.emit_amt > 0 {
            pipelines.get(self.cs_emit).bind(cmd);
            unsafe {
                cmd.Dispatch(sim.emit_amt.div_ceil(64), 1, 1);
                cmd.ResourceBarrier(&[uav_barrier(&self.particles_buf), uav_barrier(&self.dead_list), uav_barrier(&self.counters)]);
            }
        }

        // simulate
        pipelines.get(self.cs_simulate).bind(cmd);
        unsafe {
            cmd.Dispatch(self.max.div_ceil(256), 1, 1);
        }

        // copying data for readback dbg and draw pass
        unsafe {
            cmd.ResourceBarrier(&[
                transition_barrier(&self.counters, D3D12_RESOURCE_STATE_UNORDERED_ACCESS, D3D12_RESOURCE_STATE_COPY_SOURCE),
                transition_barrier(&self.particles_buf, D3D12_RESOURCE_STATE_UNORDERED_ACCESS, D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE),
                transition_barrier(&self.sort_list, D3D12_RESOURCE_STATE_UNORDERED_ACCESS, D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE)
            ]);
            cmd.CopyBufferRegion(&self.readback, 0, &self.counters, 0, 4);
            cmd.CopyBufferRegion(&self.readback, 4, &self.counters, 4096, 4);
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
struct SimData {
    pass_va: u64,
    emit_amt: u32,
    should_init: bool
}