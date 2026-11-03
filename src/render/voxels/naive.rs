use glam::{IVec3, UVec3, Vec3};
use windows::Win32::Graphics::Direct3D12::{D3D12_COMPARISON_FUNC_GREATER_EQUAL, D3D12_CULL_MODE_BACK, D3D12_CULL_MODE_NONE};
use crate::render::builtin::{Vertex3, VertexCube};
use crate::render::mesh::{DrawTopology, Mesh};
use crate::render::perf::frustum::AABB;
use crate::render::pipeline::desc::{Depth, PipelineDesc};
use crate::render::pipeline::PipelineId;
use crate::render::renderer::Renderer;
use crate::render::vertex::Vertex;
use crate::render::voxels::{index_to_uvec3, uvec3_to_index, VoxelMeshData, VoxelMesher};
use crate::render::voxels::svo::{Voxel, VoxelTree};
use crate::wrappers::graphics::batching::UploadBatch;
use crate::wrappers::graphics::GraphicsDevice;

#[derive(Debug, Copy, Clone)]
pub struct NaiveBoxes;

impl VoxelMesher for NaiveBoxes {
    fn make_vertices_and_indices(&self, tree: &VoxelTree) -> anyhow::Result<Option<VoxelMeshData>> {
        let mut total_vertices = vec![];
        let mut total_indices = vec![];

        let d = tree.size();
        let flattened = tree.flatten_tree();
        for i in 0..(d * d * d) {
            let pos = index_to_uvec3(i, d);
            if flattened[i].material == 0 {
                continue;
            }
            // +Z -Z +Y -Y +X -X
            let p = pos.as_ivec3();
            let neighbors = [IVec3::Z, -IVec3::Z, IVec3::Y, -IVec3::Y, IVec3::X, -IVec3::X].map(|o| solid_at(&flattened, d, p + o).material);

            cube(&mut total_vertices, &mut total_indices, pos.as_vec3(),(flattened[i].material - 1) as u32, neighbors);
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

fn cube(vertices: &mut Vec<VertexCube>, indices: &mut Vec<u32>, offset_pos: Vec3, material: u32, neighbors: [u8; 6]) {
    if neighbors[0] == 0 {
        // front
        let base = vertices.len() as u32;
        vertices.extend_from_slice(&[
            VertexCube::new(Vec3::new(-0.5, -0.5, 0.5) + offset_pos, [0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [0.0, 0.0], material),
            VertexCube::new(Vec3::new(0.5, -0.5, 0.5) + offset_pos, [0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [1.0, 0.0], material),
            VertexCube::new(Vec3::new(0.5, 0.5, 0.5) + offset_pos, [0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [1.0, 1.0], material),
            VertexCube::new(Vec3::new(-0.5, 0.5, 0.5) + offset_pos, [0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [0.0, 1.0], material),
        ]);
        indices.extend_from_slice(&[base, base + 1, base + 2, base + 2, base + 3, base]);
    }
    if neighbors[1] == 0 {
        // back
        let base = vertices.len() as u32;
        vertices.extend_from_slice(&[
            VertexCube::new(Vec3::new(0.5, -0.5, -0.5) + offset_pos, [0.0, 0.0, -1.0], [-1.0, 0.0, 0.0, 1.0], [0.0, 0.0], material),
            VertexCube::new(Vec3::new(-0.5, -0.5, -0.5) + offset_pos, [0.0, 0.0, -1.0], [-1.0, 0.0, 0.0, 1.0], [1.0, 0.0], material),
            VertexCube::new(Vec3::new(-0.5, 0.5, -0.5) + offset_pos, [0.0, 0.0, -1.0], [-1.0, 0.0, 0.0, 1.0], [1.0, 1.0], material),
            VertexCube::new(Vec3::new(0.5, 0.5, -0.5) + offset_pos, [0.0, 0.0, -1.0], [-1.0, 0.0, 0.0, 1.0], [0.0, 1.0], material),
        ]);
        indices.extend_from_slice(&[base, base + 1, base + 2, base + 2, base + 3, base]);
    }
    if neighbors[2] == 0 {
        // top
        let base = vertices.len() as u32;
        vertices.extend_from_slice(&[
            VertexCube::new(Vec3::new(-0.5, 0.5, -0.5) + offset_pos, [0.0, 1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [0.0, 0.0], material),
            VertexCube::new(Vec3::new(0.5, 0.5, -0.5) + offset_pos, [0.0, 1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [1.0, 0.0], material),
            VertexCube::new(Vec3::new(0.5, 0.5, 0.5) + offset_pos, [0.0, 1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [1.0, 1.0], material),
            VertexCube::new(Vec3::new(-0.5, 0.5, 0.5) + offset_pos, [0.0, 1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [0.0, 1.0], material),
        ]);
        indices.extend_from_slice(&[base, base + 2, base + 1, base + 2, base, base + 3]);
    }
    if neighbors[3] == 0 {
        // bottom
        let base = vertices.len() as u32;
        vertices.extend_from_slice(&[
            VertexCube::new(Vec3::new(-0.5, -0.5, 0.5) + offset_pos, [0.0, -1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [0.0, 0.0], material),
            VertexCube::new(Vec3::new(0.5, -0.5, 0.5) + offset_pos, [0.0, -1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [1.0, 0.0], material),
            VertexCube::new(Vec3::new(0.5, -0.5, -0.5) + offset_pos, [0.0, -1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [1.0, 1.0], material),
            VertexCube::new(Vec3::new(-0.5, -0.5, -0.5) + offset_pos, [0.0, -1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [0.0, 1.0], material),
        ]);
        indices.extend_from_slice(&[base, base + 2, base + 1, base + 2, base, base + 3]);
    }
    if neighbors[4] == 0 {
        // right
        let base = vertices.len() as u32;
        vertices.extend_from_slice(&[
            VertexCube::new(Vec3::new(0.5, -0.5, -0.5) + offset_pos, [1.0, 0.0, 0.0], [0.0, 0.0, 1.0, -1.0], [0.0, 0.0], material),
            VertexCube::new(Vec3::new(0.5, -0.5, 0.5) + offset_pos, [1.0, 0.0, 0.0], [0.0, 0.0, 1.0, -1.0], [1.0, 0.0], material),
            VertexCube::new(Vec3::new(0.5, 0.5, 0.5) + offset_pos, [1.0, 0.0, 0.0], [0.0, 0.0, 1.0, -1.0], [1.0, 1.0], material),
            VertexCube::new(Vec3::new(0.5, 0.5, -0.5) + offset_pos, [1.0, 0.0, 0.0], [0.0, 0.0, 1.0, -1.0], [0.0, 1.0], material),
        ]);
        indices.extend_from_slice(&[base, base + 2, base + 1, base + 2, base, base + 3]);
    }
    if neighbors[5] == 0 {
        // left
        let base = vertices.len() as u32;
        vertices.extend_from_slice(&[
            VertexCube::new(Vec3::new(-0.5, -0.5, 0.5) + offset_pos, [-1.0, 0.0, 0.0], [0.0, 0.0, -1.0, -1.0], [0.0, 0.0], material),
            VertexCube::new(Vec3::new(-0.5, -0.5, -0.5) + offset_pos, [-1.0, 0.0, 0.0], [0.0, 0.0, -1.0, -1.0], [1.0, 0.0], material),
            VertexCube::new(Vec3::new(-0.5, 0.5, -0.5) + offset_pos, [-1.0, 0.0, 0.0], [0.0, 0.0, -1.0, -1.0], [1.0, 1.0], material),
            VertexCube::new(Vec3::new(-0.5, 0.5, 0.5) + offset_pos, [-1.0, 0.0, 0.0], [0.0, 0.0, -1.0, -1.0], [0.0, 1.0], material),
        ]);
        indices.extend_from_slice(&[base, base + 2, base + 1, base + 2, base, base + 3]);
    }
}

fn solid_at(flat: &[Voxel], d: usize, p: IVec3) -> Voxel {
    let di = d as i32;
    if p.cmplt(IVec3::ZERO).any() || p.cmpge(IVec3::splat(di)).any() {
        return Voxel { material: 0, density: 0};
    }
    flat[uvec3_to_index(p.as_uvec3(), d)]
}