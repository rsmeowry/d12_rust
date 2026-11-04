use glam::{IVec3, UVec3, Vec3};
use windows::Win32::Graphics::Direct3D12::{D3D12_COMPARISON_FUNC_GREATER_EQUAL, D3D12_CULL_MODE_BACK, D3D12_CULL_MODE_NONE};
use crate::render::builtin::{Vertex3, VertexCube};
use crate::render::mesh::{DrawTopology, Mesh};
use crate::render::perf::frustum::AABB;
use crate::render::pipeline::desc::{Depth, PipelineDesc};
use crate::render::pipeline::PipelineId;
use crate::render::renderer::Renderer;
use crate::render::vertex::Vertex;
use crate::render::voxels::{index_to_ivec3, index_to_uvec3, uvec3_to_index, PaddedVolume, VoxelMeshData, VoxelMesher, CHUNK_DEPTH};
use crate::render::voxels::generate::{PADDED, PAD_LO};
use crate::render::voxels::svo::{Voxel, VoxelTree};
use crate::wrappers::graphics::batching::UploadBatch;
use crate::wrappers::graphics::GraphicsDevice;

#[derive(Debug, Copy, Clone)]
pub struct NaiveBoxes;

impl VoxelMesher for NaiveBoxes {
    fn make_vertices_and_indices(&self, tree: &VoxelTree, volume: &PaddedVolume) -> anyhow::Result<Option<VoxelMeshData>> {
        let mut total_vertices = vec![];
        let mut total_indices = vec![];

        let flattened = &volume.voxels;
        let d = PADDED;
        let n = 1 << CHUNK_DEPTH;
        let solid = |p: IVec3| solid_at(flattened, d, p).density < 0;

        for i in 0..(d * d * d) {
            let pos = index_to_ivec3(i as isize, d as isize);
            if flattened[i].density >= 0 {
                continue;
            }

            // +Z -Z +Y -Y +X -X
            let real_pos = pos - IVec3::splat(PAD_LO as i32);
            if real_pos.cmplt(IVec3::ZERO).any() || real_pos.cmpge(IVec3::splat(n)).any() {
                continue; // pad
            }

            let neighbors = [IVec3::Z, -IVec3::Z, IVec3::Y, -IVec3::Y, IVec3::X, -IVec3::X].map(|o| solid_at(&flattened, d, pos + o).density);
            cube(&mut total_vertices, &mut total_indices, pos, real_pos.as_vec3(),(flattened[i].material - 1) as u32, neighbors, &solid);
        }

        if total_vertices.is_empty() {
            return Ok(None)
        }

        let aabb = AABB::from_points(&total_vertices.iter().map(|it| it.pos.into()).collect::<Vec<Vec3>>());

        let vertices: Vec<u8> = bytemuck::cast_slice(&total_vertices).to_vec();

        Ok(Some(VoxelMeshData {
            vertices,
            indices: total_indices,
            aabb,
        }))
    }

    fn make_mesh(&self, mesh_data: VoxelMeshData, batch: &mut UploadBatch) -> anyhow::Result<Mesh> {
        Mesh::new_from_bytes::<VertexCube>(batch.gd(), batch, &mesh_data.vertices, bytemuck::cast_slice(&mesh_data.indices), mesh_data.indices.len() as u32, DrawTopology::TriangleList, mesh_data.aabb, true)
    }

    fn make_pipelines(&self, rd: &mut Renderer) -> anyhow::Result<(PipelineId, PipelineId, PipelineId)> {
        let (pso, pso_wireframe) = rd.build_gpass::<VertexCube>("deferred/voxel/GPassNaiveBoxes.hlsl", DrawTopology::TriangleList)?;
        let pso_shadows = rd.create_pipeline(PipelineDesc {
            name: "NaiveBoxesShadow",
            shader: "deferred/voxel/NaiveBoxesShadow.hlsl".into(),
            input: VertexCube::LAYOUT,
            rts: vec![],
            depth: Some(Depth { format: GraphicsDevice::DS_FORMAT, func: D3D12_COMPARISON_FUNC_GREATER_EQUAL, write: true }),
            cull: D3D12_CULL_MODE_BACK,
            depth_bias: -20,
            slope_bias: -1.0,
            depth_clip: false,
            ..Default::default()
        })?;

        Ok((pso, pso_wireframe, pso_shadows))
    }
}

fn cube(vertices: &mut Vec<VertexCube>, indices: &mut Vec<u32>, pos: IVec3, offset_pos: Vec3, material: u32, neighbors: [i8; 6], solid: &impl Fn(IVec3) -> bool) {
    if neighbors[0] >= 0 {
        // front
        let base = vertices.len() as u32;
        vertices.extend_from_slice(&[
            VertexCube::new(Vec3::new(-0.5, -0.5, 0.5) + offset_pos, [0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [0.0, 0.0], [material as u16, 0]),
            VertexCube::new(Vec3::new(0.5, -0.5, 0.5) + offset_pos, [0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [1.0, 0.0], [material as u16, 0]),
            VertexCube::new(Vec3::new(0.5, 0.5, 0.5) + offset_pos, [0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [1.0, 1.0], [material as u16, 0]),
            VertexCube::new(Vec3::new(-0.5, 0.5, 0.5) + offset_pos, [0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [0.0, 1.0], [material as u16, 0]),
        ]);
        finish_face(vertices, indices, base, pos, offset_pos, IVec3::Z, solid, [0, 1, 2, 2, 3, 0]);
    }
    if neighbors[1] >= 0 {
        // back
        let base = vertices.len() as u32;
        vertices.extend_from_slice(&[
            VertexCube::new(Vec3::new(0.5, -0.5, -0.5) + offset_pos, [0.0, 0.0, -1.0], [-1.0, 0.0, 0.0, 1.0], [0.0, 0.0], [material as u16, 0]),
            VertexCube::new(Vec3::new(-0.5, -0.5, -0.5) + offset_pos, [0.0, 0.0, -1.0], [-1.0, 0.0, 0.0, 1.0], [1.0, 0.0], [material as u16, 0]),
            VertexCube::new(Vec3::new(-0.5, 0.5, -0.5) + offset_pos, [0.0, 0.0, -1.0], [-1.0, 0.0, 0.0, 1.0], [1.0, 1.0], [material as u16, 0]),
            VertexCube::new(Vec3::new(0.5, 0.5, -0.5) + offset_pos, [0.0, 0.0, -1.0], [-1.0, 0.0, 0.0, 1.0], [0.0, 1.0], [material as u16, 0]),
        ]);
        finish_face(vertices, indices, base, pos, offset_pos, -IVec3::Z, solid, [0, 1, 2, 2, 3, 0]);
    }
    if neighbors[2] >= 0 {
        // top
        let base = vertices.len() as u32;
        vertices.extend_from_slice(&[
            VertexCube::new(Vec3::new(-0.5, 0.5, -0.5) + offset_pos, [0.0, 1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [0.0, 0.0], [material as u16, 0]),
            VertexCube::new(Vec3::new(0.5, 0.5, -0.5) + offset_pos, [0.0, 1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [1.0, 0.0], [material as u16, 0]),
            VertexCube::new(Vec3::new(0.5, 0.5, 0.5) + offset_pos, [0.0, 1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [1.0, 1.0], [material as u16, 0]),
            VertexCube::new(Vec3::new(-0.5, 0.5, 0.5) + offset_pos, [0.0, 1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [0.0, 1.0], [material as u16, 0]),
        ]);
        finish_face(vertices, indices, base, pos, offset_pos, IVec3::Y, solid, [0, 2, 1, 2, 0, 3]);
    }
    if neighbors[3] >= 0 {
        // bottom
        let base = vertices.len() as u32;
        vertices.extend_from_slice(&[
            VertexCube::new(Vec3::new(-0.5, -0.5, 0.5) + offset_pos, [0.0, -1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [0.0, 0.0], [material as u16, 0]),
            VertexCube::new(Vec3::new(0.5, -0.5, 0.5) + offset_pos, [0.0, -1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [1.0, 0.0], [material as u16, 0]),
            VertexCube::new(Vec3::new(0.5, -0.5, -0.5) + offset_pos, [0.0, -1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [1.0, 1.0], [material as u16, 0]),
            VertexCube::new(Vec3::new(-0.5, -0.5, -0.5) + offset_pos, [0.0, -1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [0.0, 1.0], [material as u16, 0]),
        ]);
        finish_face(vertices, indices, base, pos, offset_pos, -IVec3::Y, solid, [0, 2, 1, 2, 0, 3]);
    }
    if neighbors[4] >= 0 {
        // right
        let base = vertices.len() as u32;
        vertices.extend_from_slice(&[
            VertexCube::new(Vec3::new(0.5, -0.5, -0.5) + offset_pos, [1.0, 0.0, 0.0], [0.0, 0.0, 1.0, -1.0], [0.0, 0.0], [material as u16, 0]),
            VertexCube::new(Vec3::new(0.5, -0.5, 0.5) + offset_pos, [1.0, 0.0, 0.0], [0.0, 0.0, 1.0, -1.0], [1.0, 0.0], [material as u16, 0]),
            VertexCube::new(Vec3::new(0.5, 0.5, 0.5) + offset_pos, [1.0, 0.0, 0.0], [0.0, 0.0, 1.0, -1.0], [1.0, 1.0], [material as u16, 0]),
            VertexCube::new(Vec3::new(0.5, 0.5, -0.5) + offset_pos, [1.0, 0.0, 0.0], [0.0, 0.0, 1.0, -1.0], [0.0, 1.0], [material as u16, 0]),
        ]);
        finish_face(vertices, indices, base, pos, offset_pos, IVec3::X, solid, [0, 2, 1, 2, 0, 3]);
    }
    if neighbors[5] >= 0 {
        // left
        let base = vertices.len() as u32;
        vertices.extend_from_slice(&[
            VertexCube::new(Vec3::new(-0.5, -0.5, 0.5) + offset_pos, [-1.0, 0.0, 0.0], [0.0, 0.0, -1.0, -1.0], [0.0, 0.0], [material as u16, 0]),
            VertexCube::new(Vec3::new(-0.5, -0.5, -0.5) + offset_pos, [-1.0, 0.0, 0.0], [0.0, 0.0, -1.0, -1.0], [1.0, 0.0], [material as u16, 0]),
            VertexCube::new(Vec3::new(-0.5, 0.5, -0.5) + offset_pos, [-1.0, 0.0, 0.0], [0.0, 0.0, -1.0, -1.0], [1.0, 1.0], [material as u16, 0]),
            VertexCube::new(Vec3::new(-0.5, 0.5, 0.5) + offset_pos, [-1.0, 0.0, 0.0], [0.0, 0.0, -1.0, -1.0], [0.0, 1.0], [material as u16, 0]),
        ]);
        finish_face(vertices, indices, base, pos, offset_pos, -IVec3::X, solid, [0, 2, 1, 2, 0, 3]);
    }
}

fn solid_at(flat: &[Voxel], d: usize, p: IVec3) -> Voxel {
    let di = d as i32;
    if p.cmplt(IVec3::ZERO).any() || p.cmpge(IVec3::splat(di)).any() {
        return Voxel::AIR;
    }
    flat[uvec3_to_index(p.as_uvec3(), d)]
}

fn finish_face(vertices: &mut [VertexCube], indices: &mut Vec<u32>,
               base: u32, pos: IVec3, offset_pos: Vec3,
               n: IVec3, solid: &impl Fn(IVec3) -> bool, order: [u32; 6]) {
    let mut ao = [0u16; 4];
    for (k, v) in vertices[base as usize..].iter_mut().enumerate() {
        ao[k] = vertex_ao(solid, pos, n, Vec3::from(v.pos) - offset_pos);
        v.material_ao[1] = ao[k];
    }
    let flip = ao[0] + ao[2] > ao[1] + ao[3];
    indices.extend(order.map(|i| base + if flip { (i + 1) % 4 } else { i }));
}

fn vertex_ao(solid: &impl Fn(IVec3) -> bool, pos: IVec3, n: IVec3, corner: Vec3) -> u16 {
    let plane = IVec3::ONE - n.abs();
    let s = corner.signum().as_ivec3() * plane;
    let u = if plane.x != 0 { IVec3::new(s.x, 0, 0) } else { IVec3::new(0, s.y, 0) };
    let v = s - u;
    let front = pos + n;
    let (s1, s2, c) = (solid(front + u), solid(front + v), solid(front + s));
    if s1 && s2 { 0 } else { 3 - (s1 as u16 + s2 as u16 + c as u16) }
}