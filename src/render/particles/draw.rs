use crate::render::builtin::VertexPbr;
use crate::render::deferred::GBuffer;
use crate::render::graph::pass::ResLoad;
use crate::render::graph::FrameGraph;
use crate::render::mesh::Mesh;
use crate::render::particles::simulate::ParticleSimulation;
use crate::render::pipeline::desc::PipelineDesc;
use crate::render::pipeline::material::GpuMaterial;
use crate::render::pipeline::PipelineId;
use crate::render::renderer::{Renderer, GB_ALBEDO, GB_MATERIAL, GB_NORMALS, SCENE_FORMAT};
use crate::render::texture::Texture;
use crate::render::vertex::Vertex;
use crate::wrappers::graphics::transition_barrier;
use glam::Vec3;
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_POINTLIST;
use windows::Win32::Graphics::Direct3D12::{ID3D12CommandSignature, ID3D12Resource, D3D12_COMMAND_SIGNATURE_DESC, D3D12_CULL_MODE_NONE, D3D12_DRAW_ARGUMENTS, D3D12_DRAW_INDEXED_ARGUMENTS, D3D12_INDIRECT_ARGUMENT_DESC, D3D12_INDIRECT_ARGUMENT_TYPE_DRAW, D3D12_INDIRECT_ARGUMENT_TYPE_DRAW_INDEXED, D3D12_PRIMITIVE_TOPOLOGY_TYPE_POINT, D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE, D3D12_RESOURCE_STATE_COMMON, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT};

#[derive(Debug)]
pub enum ParticleShape {
    Billboard { tex: Texture },
    Mesh {
        mesh: Mesh,
        material: MeshParticleMaterial,
        textures: Vec<Texture>,
    },
}

#[derive(Debug, Copy, Clone)]
pub struct MeshParticleMaterial {
    pub albedo_tex: u32,
    pub normal_tex: u32,
    pub metal_roughness_tex: u32,
    pub occlusion_tex: u32,
    pub base_color: Vec3,
    pub metallic: f32,
    pub roughness: f32,
}

impl MeshParticleMaterial {
    pub fn from_gpu(m: &GpuMaterial) -> Self {
        let p = &m.unique_params;
        Self {
            albedo_tex: m.textures[0],
            normal_tex: m.textures[1],
            metal_roughness_tex: m.textures[2],
            occlusion_tex: m.textures[3],
            base_color: Vec3::new(p[0], p[1], p[2]),
            metallic: p[3],
            roughness: p[4],
        }
    }
}

#[derive(Debug)]
pub struct ParticleRendering {
    shape: ParticleShape,
    kind: ParticleKind,
    draw_pipeline: PipelineId,
    cmd_sig: ID3D12CommandSignature,
    args: ID3D12Resource,
    count_offset: u64,
}

impl ParticleRendering {
    pub fn new(rd: &mut Renderer, kind: ParticleKind, shape: ParticleShape) -> anyhow::Result<Self> {

        let gbuffer = vec![GB_ALBEDO, GB_NORMALS, GB_MATERIAL, SCENE_FORMAT];
        let (desc, arg_type, stride, initial_args, count_offset) = match &shape {
            ParticleShape::Billboard { .. } => (
                PipelineDesc {
                    name: "OpaqueParticles",
                    shader: "deferred/OpaqueParticles.hlsl".into(),
                    input: &[],
                    rts: gbuffer,
                    cull: D3D12_CULL_MODE_NONE,
                    topology: D3D12_PRIMITIVE_TOPOLOGY_TYPE_POINT,
                    ..Default::default()
                },
                D3D12_INDIRECT_ARGUMENT_TYPE_DRAW,
                size_of::<D3D12_DRAW_ARGUMENTS>(),
                vec![0u32, 1, 0, 0], // same, <cnt>, 1 index, ...
                0u64,
            ),
            ParticleShape::Mesh { mesh, .. } => (
                PipelineDesc {
                    name: "MeshParticles",
                    shader: "deferred/MeshParticles.hlsl".into(),
                    input: VertexPbr::LAYOUT,
                    rts: gbuffer,
                    topology: D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
                    ..Default::default()
                },
                D3D12_INDIRECT_ARGUMENT_TYPE_DRAW_INDEXED,
                size_of::<D3D12_DRAW_INDEXED_ARGUMENTS>(),
                vec![mesh.index_count(), 0, 0, 0, 0], // <indices> <instances> <start> <base vert> <start inst>
                4u64,
            ),
        };
        let draw_pipeline = rd.create_pipeline(desc)?;

        let mut batch = rd.gd.begin_upload()?;
        let args = rd.gd.create_default_buffer(&mut batch, bytemuck::cast_slice(&initial_args), D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT, "ParticleDrawArgs")?;
        batch.finish()?;

        let mut cmd_sig: Option<ID3D12CommandSignature> = None;
        unsafe {
            rd.gd.device().CreateCommandSignature(&D3D12_COMMAND_SIGNATURE_DESC {
                ByteStride: stride as u32,
                NumArgumentDescs: 1,
                pArgumentDescs: &D3D12_INDIRECT_ARGUMENT_DESC { Type: arg_type, ..Default::default() },
                NodeMask: 0,
            }, None, &mut cmd_sig)?;
        }

        Ok(Self {
            shape,
            kind,
            draw_pipeline,
            cmd_sig: cmd_sig.unwrap(),
            args,
            count_offset
        })
    }

    pub fn add_pass<'a>(&'a self, rd: &'a Renderer, gbuf: &GBuffer, sim: &'a ParticleSimulation, graph: &mut FrameGraph<'a>) -> anyhow::Result<()> {
        let pass_va = match &self.shape {
            ParticleShape::Billboard { tex } => rd.upload_cbuffer(&ParticleDrawPass { tex: tex.srv_ptr(), _pad: [0; 3] })?,
            ParticleShape::Mesh { mesh, material, .. } => {
                let (center, radius) = mesh.bounding_sphere();
                rd.upload_cbuffer(&MeshParticlePass {
                    albedo_tex: material.albedo_tex,
                    normal_tex: material.normal_tex,
                    metal_roughness_tex: material.metal_roughness_tex,
                    occlusion_tex: material.occlusion_tex,
                    base_color: material.base_color,
                    metallic: material.metallic,
                    mesh_center: center,
                    mesh_scale: 1.0 / radius,
                    roughness: material.roughness,
                    _pad: [0; 3],
                })?
            }
        };
        let pipelines = rd.pipelines();
        let rs = rd.rs();

        graph.add_pass("ParticlesDraw")
            .writes_depth(gbuf.scene_depth, ResLoad::Preserve)
            .writes(gbuf.albedo, ResLoad::Preserve)
            .writes(gbuf.normals, ResLoad::Preserve)
            .writes(gbuf.materials, ResLoad::Preserve)
            .writes(gbuf.scene_color, ResLoad::Preserve)
            .exec(move |ctx| {
                let cmd = ctx.cmd;
                // alive counter goes to count
                unsafe {
                    cmd.ResourceBarrier(&[transition_barrier(&self.args, D3D12_RESOURCE_STATE_COMMON, D3D12_RESOURCE_STATE_COPY_DEST)]);
                    cmd.CopyBufferRegion(&self.args, self.count_offset, sim.counters(), ParticleSimulation::ALIVE_COUNTER_OFFSET, 4);
                    cmd.ResourceBarrier(&[transition_barrier(&self.args, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT)]);
                }
                // pool and sortlist are needed both for meshes and billboards
                pipelines.get(self.draw_pipeline).bind(cmd);
                rs.set_pass(cmd, pass_va);
                rs.set_objects(cmd, sim.particles_buf_va());
                rs.set_materials(cmd, sim.sort_list_va());
                // geometry
                match &self.shape {
                    ParticleShape::Billboard { .. } =>
                        unsafe {
                            cmd.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_POINTLIST)
                        },
                    ParticleShape::Mesh { mesh, .. } => mesh.bind(cmd),
                }
                unsafe {
                    cmd.ExecuteIndirect(&self.cmd_sig, 1, &self.args, 0, None, 0);
                }
                Ok(())
            });
        Ok(())
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ParticleKind {
    Opaque,
    Soft,
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ParticleDrawPass {
    pub tex: u32,
    pub _pad: [u32; 3],
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct MeshParticlePass {
    albedo_tex: u32,
    normal_tex: u32,
    metal_roughness_tex: u32,
    occlusion_tex: u32,
    base_color: Vec3,
    metallic: f32,
    mesh_center: Vec3,
    mesh_scale: f32,
    roughness: f32,
    _pad: [u32; 3],
}