use bytemuck::Zeroable;
use glam::{Mat4, Vec3, Vec4};
use crate::assets::{load_assimp_scene, LoadTex, LodCount};
use crate::physics::body::SoftBody;
use crate::physics::PhysicsSystem;
use crate::render::builtin::VertexFfd;
use crate::render::deferred::GBuffer;
use crate::render::deferred::transparent::TransparentPass;
use crate::render::graph::FrameGraph;
use crate::render::graph::res::ResId;
use crate::render::lights::GpuLight;
use crate::render::mesh::{DrawTopology, Mesh};
use crate::render::perf::frustum::AABB;
use crate::render::pipeline::material::GpuMaterial;
use crate::render::renderer::{ObjectData, Renderer};
use crate::wrappers::graphics::MappedUploadBuffer;
use crate::wrappers::graphics::batching::UploadBatch;

pub const POINT_COUNT: usize = 4;

#[derive(Debug)]
pub struct SoftBodyRenderer {
    body_idx: usize,
    transparents: TransparentPass,
    body_mesh: Mesh,
    body_obj: ObjectData,
    buffer: MappedUploadBuffer<PointsBuffer>,
}

impl SoftBodyRenderer {
    pub fn new(mesh_path: Option<&str>, position: Vec3, body_idx: usize, batch: &mut UploadBatch, rd: &Renderer, pass: TransparentPass) -> anyhow::Result<Self> {
        let buffer = MappedUploadBuffer::<PointsBuffer>::new(&batch.gd())?;
        let (vertices, indices) = build_cube_mesh(POINT_COUNT as u32);
        let (body_mesh, body_obj) = if let Some(mesh_path) = mesh_path {
            let mut mats = vec![];
            let loaded = load_assimp_scene::<VertexFfd, &str>(rd, batch, mesh_path, &mut mats, LoadTex::empty(), DrawTopology::TriangleList, false, LodCount::NoLods, |_, _| GpuMaterial::zeroed(), Some(Box::new(bind_single_to_lattice)), None)?;
            let (mesh, model, _) = loaded.into_single_mesh();
            let mesh_obj = ObjectData::new(Mat4::from_translation(position) * model, 0, &mesh);
            (mesh, mesh_obj)
        } else {
            let mesh = Mesh::new(batch.gd(), batch, &vertices, &indices, DrawTopology::TriangleList, AABB::from_points(&[Vec3::NEG_ONE, Vec3::ONE]), false, false)?;
            let mesh_obj = ObjectData::new(Mat4::from_translation(position), 0, &mesh);
            (mesh, mesh_obj)
        };
        Ok(Self {
            body_idx,
            transparents: pass,
            buffer,
            body_obj,
            body_mesh,
        })
    }

    pub fn transparents_pass(&self) -> &TransparentPass {
        &self.transparents
    }

    pub fn add_pass<'a>(&'a self, irradiance_tex: u32, main_light: &GpuLight, phys: &PhysicsSystem, rd: &'a Renderer, gbuf: &'a GBuffer, graph: &mut FrameGraph<'a>) -> anyhow::Result<ResId> {
        let body: &SoftBody = &phys.bodies[self.body_idx];

        let mut points = [Vec4::ZERO; POINT_COUNT * POINT_COUNT * POINT_COUNT];
        for (p_idx, p) in body.particles.iter().enumerate() {
            points[p_idx] = Vec4::new(p.position.x, p.position.y, p.position.z, 0.0);
        }
        let frame_idx = rd.gd.frame_index()?;
        let buffer_va = self.buffer.va(&rd.gd)?;

        graph.add_pass("FFDCopyDataPass")
            .has_side_effects()
            .exec(move |_ctx| {
                unsafe {
                    *self.buffer.write_idx(frame_idx) = PointsBuffer { points };
                }
                Ok(())
            });

        self.transparents.add_pass(Some(buffer_va), irradiance_tex, main_light, &self.body_mesh, &self.body_obj, rd, gbuf, graph)
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct PointsBuffer {
    pub points: [Vec4; POINT_COUNT * POINT_COUNT * POINT_COUNT],
}

fn build_cube_mesh(ppa: u32) -> (Vec<VertexFfd>, Vec<u32>) {
    let faces = [
        (Vec3::X, Vec3::Y, Vec3::Z),
        (Vec3::NEG_X, Vec3::Z, Vec3::Y),
        (Vec3::Y, Vec3::Z, Vec3::X),
        (Vec3::NEG_Y, Vec3::X, Vec3::Z),
        (Vec3::Z, Vec3::X, Vec3::Y),
        (Vec3::NEG_Z, Vec3::Y, Vec3::X),
    ];
    let (mut vs, mut is) = (Vec::new(), Vec::new());

    for (nrm, ua, va) in faces {
        let base = vs.len() as u32;
        for a in 0..=ppa { for b in 0..=ppa {
            let (fa, fb) = (a as f32 / ppa as f32, b as f32 / ppa as f32);
            let p = Vec3::splat(0.5) + 0.5 * (nrm + (2.0 * fa - 1.0) * ua + (2.0 * fb - 1.0) * va);
            vs.push(VertexFfd { stu: p.to_array(), norm: nrm.to_array(), uv: [fa, fb] });
        }}
        for a in 0..ppa { for b in 0..ppa {
            let i00 = base + a * (ppa + 1) + b;
            let (i10, i01, i11) = (i00 + (ppa + 1), i00 + 1, i00 + (ppa + 1) + 1);
            is.extend_from_slice(&[i00, i10, i11, i00, i11, i01]);
        }}
    }
    (vs, is)
}

fn bind_single_to_lattice(all: &[VertexFfd], this: &mut VertexFfd) {
    let aabb = AABB::from_points(&all.iter().map(|it| it.stu.into()).collect::<Vec<_>>());
    let inv = 1.0 / (aabb.max - aabb.min);
    this.stu = ((Vec3::from(this.stu) - aabb.min) * inv).to_array();
}

fn bind_to_lattice(verts: &mut [VertexFfd], min: Vec3, max: Vec3) {
    let inv = 1.0 / (max - min);
    for v in verts.iter_mut() {
        v.stu = ((Vec3::from(v.stu) - min) * inv).to_array();
    }
}

fn lattice_aabb(points: &[Vec3]) -> (Vec3, Vec3) {
    points.iter().fold(
        (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
        |(lo, hi), &p| (lo.min(p), hi.max(p)),
    )
}