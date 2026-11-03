use glam::{IVec3, UVec3, Vec3};
use windows::Win32::Graphics::Direct3D12::{D3D12_COMPARISON_FUNC_GREATER_EQUAL, D3D12_CULL_MODE_BACK, D3D12_CULL_MODE_NONE};
use crate::render::builtin::{Vertex3, VertexCube};
use crate::render::mesh::{DrawTopology, Mesh};
use crate::render::perf::frustum::AABB;
use crate::render::pipeline::desc::{Depth, PipelineDesc};
use crate::render::pipeline::PipelineId;
use crate::render::renderer::Renderer;
use crate::render::vertex::Vertex;
use crate::render::voxels::{index_to_uvec3, uvec3_to_index, VoxelMesher};
use crate::render::voxels::svo::VoxelTree;
use crate::wrappers::graphics::GraphicsDevice;

#[derive(Debug, Copy, Clone)]
pub struct NaiveBoxes;

impl VoxelMesher for NaiveBoxes {
    fn make_mesh(&self, tree: &VoxelTree, gd: &GraphicsDevice) -> anyhow::Result<Option<Mesh>> {
        let mut total_vertices = vec![];
        let mut total_indices = vec![];

        let d = tree.size();
        let flattened = tree.flatten_tree();
        for i in 0..(d * d * d) {
            let pos = index_to_uvec3(i, d);
            if flattened[i] == 0 {
                continue;
            }
            // +Z -Z +Y -Y +X -X
            let p = pos.as_ivec3();
            let neighbors = [IVec3::Z, -IVec3::Z, IVec3::Y, -IVec3::Y, IVec3::X, -IVec3::X].map(|o| solid_at(&flattened, d, p + o));

            let (gen_vertices, gen_indices) = cube((flattened[i] - 1) as u32, neighbors);
            let offset = total_vertices.len() as u32;
            total_vertices.extend(gen_vertices.into_iter().map(|each| VertexCube::new(
                [each.pos[0] + pos.x as f32, each.pos[1] + pos.y as f32, each.pos[2] + pos.z as f32],
                each.norm,
                each.tangent,
                each.uv,
                each.material,
            )));
            total_indices.extend(gen_indices.into_iter().map(|it| it + offset))
        }

        if total_vertices.is_empty() {
            return Ok(None);
        }

        let aabb = AABB::from_points(&total_vertices.iter().map(|it| it.pos.into()).collect::<Vec<Vec3>>());

        let mut batch = gd.begin_upload()?;
        let mesh = Mesh::new(gd, &mut batch, &total_vertices, &total_indices, DrawTopology::TriangleList, aabb, true)?;
        batch.finish()?;
        Ok(Some(mesh))
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

fn cube(material: u32, neighbors: [u8; 6]) -> (Vec<VertexCube>, Vec<u32>) {
    let mut vertices = vec![];
    let mut indices = vec![];
    if neighbors[0] == 0 {
        // front
        let base = vertices.len() as u32;
        vertices.extend_from_slice(&[
            VertexCube::new([-0.5, -0.5, 0.5], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [0.0, 0.0], material),
            VertexCube::new([0.5, -0.5, 0.5], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [1.0, 0.0], material),
            VertexCube::new([0.5, 0.5, 0.5], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [1.0, 1.0], material),
            VertexCube::new([-0.5, 0.5, 0.5], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [0.0, 1.0], material),
        ]);
        indices.extend_from_slice(&[base, base + 1, base + 2, base + 2, base + 3, base]);
    }
    if neighbors[1] == 0 {
        // back
        let base = vertices.len() as u32;
        vertices.extend_from_slice(&[
            VertexCube::new([0.5, -0.5, -0.5], [0.0, 0.0, -1.0], [-1.0, 0.0, 0.0, 1.0], [0.0, 0.0], material),
            VertexCube::new([-0.5, -0.5, -0.5], [0.0, 0.0, -1.0], [-1.0, 0.0, 0.0, 1.0], [1.0, 0.0], material),
            VertexCube::new([-0.5, 0.5, -0.5], [0.0, 0.0, -1.0], [-1.0, 0.0, 0.0, 1.0], [1.0, 1.0], material),
            VertexCube::new([0.5, 0.5, -0.5], [0.0, 0.0, -1.0], [-1.0, 0.0, 0.0, 1.0], [0.0, 1.0], material),
        ]);
        indices.extend_from_slice(&[base, base + 1, base + 2, base + 2, base + 3, base]);
    }
    if neighbors[2] == 0 {
        // top
        let base = vertices.len() as u32;
        vertices.extend_from_slice(&[
            VertexCube::new([-0.5, 0.5, -0.5], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [0.0, 0.0], material),
            VertexCube::new([0.5, 0.5, -0.5], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [1.0, 0.0], material),
            VertexCube::new([0.5, 0.5, 0.5], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [1.0, 1.0], material),
            VertexCube::new([-0.5, 0.5, 0.5], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [0.0, 1.0], material),
        ]);
        indices.extend_from_slice(&[base, base + 2, base + 1, base + 2, base, base + 3]);
    }
    if neighbors[3] == 0 {
        // bottom
        let base = vertices.len() as u32;
        vertices.extend_from_slice(&[
            VertexCube::new([-0.5, -0.5, 0.5], [0.0, -1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [0.0, 0.0], material),
            VertexCube::new([0.5, -0.5, 0.5], [0.0, -1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [1.0, 0.0], material),
            VertexCube::new([0.5, -0.5, -0.5], [0.0, -1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [1.0, 1.0], material),
            VertexCube::new([-0.5, -0.5, -0.5], [0.0, -1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [0.0, 1.0], material),
        ]);
        indices.extend_from_slice(&[base, base + 2, base + 1, base + 2, base, base + 3]);
    }
    if neighbors[4] == 0 {
        // right
        let base = vertices.len() as u32;
        vertices.extend_from_slice(&[
            VertexCube::new([0.5, -0.5, -0.5], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0, -1.0], [0.0, 0.0], material),
            VertexCube::new([0.5, -0.5, 0.5], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0, -1.0], [1.0, 0.0], material),
            VertexCube::new([0.5, 0.5, 0.5], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0, -1.0], [1.0, 1.0], material),
            VertexCube::new([0.5, 0.5, -0.5], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0, -1.0], [0.0, 1.0], material),
        ]);
        indices.extend_from_slice(&[base, base + 2, base + 1, base + 2, base, base + 3]);
    }
    if neighbors[5] == 0 {
        // left
        let base = vertices.len() as u32;
        vertices.extend_from_slice(&[
            VertexCube::new([-0.5, -0.5, 0.5], [-1.0, 0.0, 0.0], [0.0, 0.0, -1.0, -1.0], [0.0, 0.0], material),
            VertexCube::new([-0.5, -0.5, -0.5], [-1.0, 0.0, 0.0], [0.0, 0.0, -1.0, -1.0], [1.0, 0.0], material),
            VertexCube::new([-0.5, 0.5, -0.5], [-1.0, 0.0, 0.0], [0.0, 0.0, -1.0, -1.0], [1.0, 1.0], material),
            VertexCube::new([-0.5, 0.5, 0.5], [-1.0, 0.0, 0.0], [0.0, 0.0, -1.0, -1.0], [0.0, 1.0], material),
        ]);
        indices.extend_from_slice(&[base, base + 2, base + 1, base + 2, base, base + 3]);
    }
    (vertices, indices)
}

fn solid_at(flat: &[u8], d: usize, p: IVec3) -> u8 {
    let di = d as i32;
    if p.cmplt(IVec3::ZERO).any() || p.cmpge(IVec3::splat(di)).any() {
        return 0;
    }
    flat[uvec3_to_index(p.as_uvec3(), d)]
}