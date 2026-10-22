use crate::render::deferred::GBuffer;
use crate::render::graph::pass::ResLoad;
use crate::render::graph::FrameGraph;
use crate::render::particles::simulate::ParticleSimulation;
use crate::render::pipeline::desc::PipelineDesc;
use crate::render::pipeline::PipelineId;
use crate::render::renderer::{Renderer, GB_ALBEDO, GB_MATERIAL, GB_NORMALS, SCENE_FORMAT};
use crate::render::texture::Texture;
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_POINTLIST;
use windows::Win32::Graphics::Direct3D12::{ID3D12CommandSignature, D3D12_COMMAND_SIGNATURE_DESC, D3D12_CULL_MODE_NONE, D3D12_DRAW_ARGUMENTS, D3D12_INDIRECT_ARGUMENT_DESC, D3D12_INDIRECT_ARGUMENT_TYPE_DRAW, D3D12_PRIMITIVE_TOPOLOGY_TYPE_POINT};

#[derive(Debug)]
pub struct ParticleRendering {
    tex: Texture,
    draw_pipeline: PipelineId,
    kind: ParticleKind,
    cmd_sig: ID3D12CommandSignature
}

impl ParticleRendering {
    pub fn new(rd: &mut Renderer, kind: ParticleKind, tex: Texture) -> anyhow::Result<Self> {
        let desc = match kind {
            ParticleKind::Opaque => {
                PipelineDesc {
                    name: "OpaqueParticles",
                    shader: "deferred/OpaqueParticles.hlsl".into(),
                    input: &[],
                    rts: vec![GB_ALBEDO, GB_NORMALS, GB_MATERIAL, SCENE_FORMAT],
                    cull: D3D12_CULL_MODE_NONE,
                    topology: D3D12_PRIMITIVE_TOPOLOGY_TYPE_POINT,
                    ..Default::default()
                }
            }
            ParticleKind::Soft => {
                todo!("soft particles...")
            }
        };
        let draw_pipeline = rd.create_pipeline(desc)?;

        let mut cmd_sig: Option<ID3D12CommandSignature> = None;
        unsafe {
            rd.gd.device().CreateCommandSignature(&D3D12_COMMAND_SIGNATURE_DESC {
                ByteStride: size_of::<D3D12_DRAW_ARGUMENTS>() as u32,
                NumArgumentDescs: 1,
                pArgumentDescs: &D3D12_INDIRECT_ARGUMENT_DESC {
                    Type: D3D12_INDIRECT_ARGUMENT_TYPE_DRAW,
                    ..Default::default()
                },
                NodeMask: 0,
            }, None, &mut cmd_sig)?;
        }

        Ok(Self {
            tex,
            draw_pipeline,
            kind,
            cmd_sig: cmd_sig.unwrap()
        })
    }

    pub fn add_pass<'a>(&'a self, rd: &'a Renderer, gbuf: &GBuffer, sim: &'a ParticleSimulation, graph: &mut FrameGraph<'a>) -> anyhow::Result<()> {
        let pipelines = rd.pipelines();
        let rs = rd.rs();

        let mut pass = graph.add_pass("ParticlesDraw");

        if self.kind == ParticleKind::Opaque {
            pass = pass.writes_depth(gbuf.scene_depth, ResLoad::Preserve);
        }

        pass = pass
            .writes(gbuf.albedo, ResLoad::Preserve)
            .writes(gbuf.normals, ResLoad::Preserve)
            .writes(gbuf.materials, ResLoad::Preserve)
            .writes(gbuf.scene_color, ResLoad::Preserve);

        let sig = &self.cmd_sig;
        let args = sim.indirect_args();
        let pass_va = rd.upload_cbuffer(&ParticleDrawPass {
            tex: self.tex.srv_ptr(),
            _pad: [0; 3],
        })?;
        pass.exec(move |ctx| {
            pipelines.get(self.draw_pipeline).bind(ctx.cmd);
            rs.set_pass(ctx.cmd, pass_va);
            rs.set_objects(ctx.cmd, sim.particles_buf_va());
            rs.set_materials(ctx.cmd, sim.sort_list_va());
            unsafe {
                ctx.cmd.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_POINTLIST);
                ctx.cmd.ExecuteIndirect(sig, 1, args, 0, None, 0);
            }
            Ok(())
        });

        Ok(())
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ParticleKind {
    Opaque,
    Soft
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ParticleDrawPass {
    pub tex: u32,
    pub _pad: [u32; 3]
}