use glam::Vec3;
use windows::Win32::Graphics::Direct3D12::{D3D12_COMPARISON_FUNC_GREATER_EQUAL, D3D12_CULL_MODE_BACK, D3D12_CULL_MODE_NONE};
use crate::render::builtin::{Vertex3, VertexCube};
use crate::render::mesh::{DrawTopology, Mesh};
use crate::render::perf::frustum::AABB;
use crate::render::pipeline::desc::{Depth, PipelineDesc};
use crate::render::pipeline::PipelineId;
use crate::render::renderer::Renderer;
use crate::render::vertex::Vertex;
use crate::render::voxels::{index_to_uvec3, VoxelMesher};
use crate::render::voxels::svo::VoxelTree;
use crate::wrappers::graphics::GraphicsDevice;

#[derive(Debug, Copy, Clone)]
pub struct NaiveBoxes;

impl VoxelMesher for NaiveBoxes {
    fn make_mesh(&self, tree: &VoxelTree, gd: &GraphicsDevice) -> anyhow::Result<Mesh> {
        let mut total_vertices = vec![];
        let mut total_indices = vec![];

        let d = tree.size();
        let flattened = tree.flatten_tree();
        for i in 0..(d * d * d) {
            let pos = index_to_uvec3(i, d);
            if flattened[i] == 0 {
                continue;
            }
            let (gen_vertices, gen_indices) = cube((flattened[i] - 1) as u32);
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

        let aabb = AABB::from_points(&total_vertices.iter().map(|it| it.pos.into()).collect::<Vec<Vec3>>());

        let mut batch = gd.begin_upload()?;
        let mesh = Mesh::new(gd, &mut batch, &total_vertices, &total_indices, DrawTopology::TriangleList, aabb, true)?;
        batch.finish()?;
        Ok(mesh)
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

fn cube(material: u32) -> (Vec<VertexCube>, Vec<u32>) {
    let vertices = vec![
        VertexCube::new([-0.5, -0.5, 0.5], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [0.0, 0.0], material),
        VertexCube::new([0.5, -0.5, 0.5], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [1.0, 0.0], material),
        VertexCube::new([0.5, 0.5, 0.5], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [1.0, 1.0], material),
        VertexCube::new([-0.5, 0.5, 0.5], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [0.0, 1.0], material),

        VertexCube::new([0.5, -0.5, -0.5], [0.0, 0.0, -1.0], [-1.0, 0.0, 0.0, 1.0], [0.0, 0.0], material),
        VertexCube::new([-0.5, -0.5, -0.5], [0.0, 0.0, -1.0], [-1.0, 0.0, 0.0, 1.0], [1.0, 0.0], material),
        VertexCube::new([-0.5, 0.5, -0.5], [0.0, 0.0, -1.0], [-1.0, 0.0, 0.0, 1.0], [1.0, 1.0], material),
        VertexCube::new([0.5, 0.5, -0.5], [0.0, 0.0, -1.0], [-1.0, 0.0, 0.0, 1.0], [0.0, 1.0], material),

        VertexCube::new([-0.5, 0.5, -0.5], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [0.0, 0.0], material),
        VertexCube::new([0.5, 0.5, -0.5], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [1.0, 0.0], material),
        VertexCube::new([0.5, 0.5, 0.5], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [1.0, 1.0], material),
        VertexCube::new([-0.5, 0.5, 0.5], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [0.0, 1.0], material),

        VertexCube::new([-0.5, -0.5, 0.5], [0.0, -1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [0.0, 0.0], material),
        VertexCube::new([0.5, -0.5, 0.5], [0.0, -1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [1.0, 0.0], material),
        VertexCube::new([0.5, -0.5, -0.5], [0.0, -1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [1.0, 1.0], material),
        VertexCube::new([-0.5, -0.5, -0.5], [0.0, -1.0, 0.0], [1.0, 0.0, 0.0, -1.0], [0.0, 1.0], material),

        VertexCube::new([0.5, -0.5, -0.5], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0, -1.0], [0.0, 0.0], material),
        VertexCube::new([0.5, -0.5, 0.5], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0, -1.0], [1.0, 0.0], material),
        VertexCube::new([0.5, 0.5, 0.5], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0, -1.0], [1.0, 1.0], material),
        VertexCube::new([0.5, 0.5, -0.5], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0, -1.0], [0.0, 1.0], material),

        VertexCube::new([-0.5, -0.5, 0.5], [-1.0, 0.0, 0.0], [0.0, 0.0, -1.0, -1.0], [0.0, 0.0], material),
        VertexCube::new([-0.5, -0.5, -0.5], [-1.0, 0.0, 0.0], [0.0, 0.0, -1.0, -1.0], [1.0, 0.0], material),
        VertexCube::new([-0.5, 0.5, -0.5], [-1.0, 0.0, 0.0], [0.0, 0.0, -1.0, -1.0], [1.0, 1.0], material),
        VertexCube::new([-0.5, 0.5, 0.5], [-1.0, 0.0, 0.0], [0.0, 0.0, -1.0, -1.0], [0.0, 1.0], material),
    ];

    let indices = vec![
        0, 1, 2, 2, 3, 0,
        4, 5, 6, 6, 7, 4,
        8, 10, 9, 10, 8, 11,
        12, 14, 13, 14, 12, 15,
        16, 18, 17, 18, 16, 19,
        20, 22, 21, 22, 20, 23,
    ];

    (vertices, indices)
}