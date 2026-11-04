use glam::{IVec3, Mat3, Vec2, Vec3};
use windows::Win32::Graphics::Direct3D12::{D3D12_COMPARISON_FUNC_GREATER_EQUAL, D3D12_CULL_MODE_BACK, D3D12_CULL_MODE_NONE};
use crate::render::builtin::{VertexCube, VertexSmooth};
use crate::render::mesh::{DrawTopology, Mesh};
use crate::render::perf::frustum::AABB;
use crate::render::pipeline::desc::{Depth, PipelineDesc};
use crate::render::pipeline::PipelineId;
use crate::render::renderer::Renderer;
use crate::render::vertex::{octahedral_encode_normal, Vertex};
use crate::render::voxels::svo::VoxelTree;
use crate::render::voxels::{index_to_ivec3, index_to_uvec3, ivec3_to_index, uvec3_to_index, PaddedVolume, VoxelMeshData, VoxelMesher, CHUNK_DEPTH};
use crate::render::voxels::generate::{PADDED, PAD_LO};
use crate::wrappers::graphics::batching::UploadBatch;
use crate::wrappers::graphics::GraphicsDevice;

const EDGES: [(u8, u8); 12] = [
    (0, 1), (2, 3), (4, 5), (6, 7), // along x (bit 0)
    (0, 2), (1, 3), (4, 6), (5, 7), // along y (bit 1)
    (0, 4), (1, 5), (2, 6), (3, 7), // along z (bit 2)
];

const EDGE_MASK: [u16; 256] = compute_edge_table();


#[derive(Debug, Copy, Clone)]
pub struct DualContouring;

impl VoxelMesher for DualContouring {
    fn make_vertices_and_indices<'a>(&self, tree: &VoxelTree, volume: &PaddedVolume) -> anyhow::Result<Option<VoxelMeshData>> {
        let mut vertices = Vec::new();
        let mut aabb_points = Vec::new();
        let mut indices = Vec::new();
        let mut vertex_of = vec![u32::MAX; (CELLS * CELLS * CELLS) as usize];

        const N: i32 = 32;
        const CELLS: i32 = N + 1;
        // fetches cell index
        let cell_idx = |c: IVec3| (c.x + CELLS * (c.y + CELLS * c.z)) as usize;
        // corner from edge
        let corner = |i: u8| IVec3::new((i & 1) as i32, ((i >> 1) & 1) as i32, ((i >> 2) & 1) as i32);
        // density at pos
        let dens = |p: IVec3| volume.voxels[ivec3_to_index(p, PADDED as isize)].density as f32 / 127.;
        // gradient at corner
        let grad = |c: IVec3| Vec3::new(
            dens(c + IVec3::X) - dens(c - IVec3::X),
            dens(c + IVec3::Y) - dens(c - IVec3::Y),
            dens(c + IVec3::Z) - dens(c - IVec3::Z),
        );

        let d = (1 << CHUNK_DEPTH) as isize;
        let n = CELLS;
        for i in 0..=(n * n * n) {
            let real_pos = index_to_ivec3(i as isize, CELLS as isize);
            let pos = real_pos + IVec3::splat(PAD_LO as i32);
            if real_pos.cmplt(IVec3::ZERO).any() || real_pos.cmpge(IVec3::splat(n)).any() {
                continue; // pad
            }

            // classification
            // fetching density at edges. a cell only needs vertices if its
            // edges have different density signs
            let mut mask = 0u8;
            for corner in 0..8 {
                let o = IVec3::new(corner & 1, (corner >> 1) & 1, (corner >> 2) & 1);
                if dens(pos + o) < 0. {
                    mask |= 1 << corner;
                }
            }
            if mask == 0 || mask == 0xFF { // either all solid or all air
                continue;
            }

            // iterating over every edge with differing signs
            let mut edge: u16 = EDGE_MASK[mask as usize];
            let mut qef = Qef {
                ata: [0.; 6],
                atb: Default::default(),
                btb: 0.0,
                mass: Default::default(),
                count: 0.0,
                normals: Vec3::ZERO
            }; // quadratic error function
            let mut material = 0u8; // also picking material here
            while edge != 0 {
                let e = edge.trailing_zeros() as usize;
                edge &= edge - 1;
                let (a, b) = EDGES[e];
                let (corn_a, corn_b) = (corner(a), corner(b));
                let (ca, cb) = (pos + corn_a, pos + corn_b);
                let (da, db) = (dens(ca), dens(cb));
                let t = da / (da - db);
                let pos = corn_a.as_vec3() + t * (corn_b - corn_a).as_vec3(); // cell local pos, 0..1
                let norm = grad(ca).lerp(grad(cb), t).normalize_or_zero();
                qef.add(pos, norm);

                material = volume.voxels[ivec3_to_index(if da < 0. { ca } else { cb }, PADDED as isize)].material;
            }
            let (pos_cellwise, normal) = qef.solve(0.05);

            // vertex !!
            let pos_chunkwise = real_pos.as_vec3() + pos_cellwise;
            let pos_quantized = (pos_chunkwise * 1024.).round().as_u16vec3();
            let vertex_index = vertices.len() as u32;
            vertices.push(VertexSmooth {
                pos_material: [pos_quantized.x, pos_quantized.y, pos_quantized.z, material as u16],
                normal: octahedral_encode_normal(normal).into(),
            });
            aabb_points.push(pos_chunkwise);

            // indices now
            vertex_of[cell_idx(real_pos)] = vertex_index;
            let solid0 = mask & 1 != 0;
            for (bit, axis, u, v) in [
                (1u8 << 1, 0usize, IVec3::Y, IVec3::Z), // corner 0 -> 1 (+X)
                (1u8 << 2, 1, IVec3::Z, IVec3::X), // corner 0 -> 2 (+Y)
                (1u8 << 4, 2, IVec3::X, IVec3::Y), // corner 0 -> 4 (+Z)
            ] {
                if solid0 == (mask & bit != 0) { // sign didn't change
                    continue;
                }
                let (cu, cv) = (real_pos.dot(u), real_pos.dot(v));
                if real_pos[axis] >= N { //ownership along axis
                    continue;
                }
                if cu < 1 || cv < 1 {
                    continue;
                }

                let q = [real_pos - u - v, real_pos - v, real_pos, real_pos - u].map(|n| vertex_of[cell_idx(n)]);
                debug_assert!(q.iter().all(|&i| i != u32::MAX), "missing cell vertex at {real_pos} axis {axis}");

                // if solid air is along +axis, the face points +axis
                if solid0 {
                    indices.extend_from_slice(&[q[0], q[1], q[2], q[0], q[2], q[3]]);
                } else {
                    indices.extend_from_slice(&[q[0], q[2], q[1], q[0], q[3], q[2]]);
                }
            }
        }

        if vertices.is_empty() || indices.is_empty() {
            return Ok(None)
        }

        Ok(Some(VoxelMeshData {
            vertices: bytemuck::cast_slice(&vertices).to_vec(),
            indices,
            aabb: AABB::from_points(&aabb_points),
        }))
    }

    fn make_mesh(&self, mesh_data: VoxelMeshData, batch: &mut UploadBatch) -> anyhow::Result<Mesh> {
        Mesh::new_from_bytes::<VertexSmooth>(batch.gd(), batch, &mesh_data.vertices, bytemuck::cast_slice(&mesh_data.indices), mesh_data.indices.len() as u32, DrawTopology::TriangleList, mesh_data.aabb, true)
    }

    fn make_pipelines(&self, rd: &mut Renderer) -> anyhow::Result<(PipelineId, PipelineId, PipelineId)> {
        let (pso, pso_wireframe) = rd.build_gpass::<VertexSmooth>("deferred/voxel/GPassSmooth.hlsl", DrawTopology::TriangleList)?;
        let pso_shadows = rd.create_pipeline(PipelineDesc {
            name: "GPassSmoothShadow",
            shader: "deferred/voxel/GPassSmoothShadow.hlsl".into(),
            input: VertexSmooth::LAYOUT,
            rts: vec![],
            depth: Some(Depth { format: GraphicsDevice::DS_FORMAT, func: D3D12_COMPARISON_FUNC_GREATER_EQUAL, write: true }),
            cull: D3D12_CULL_MODE_NONE,
            depth_bias: -20,
            slope_bias: -1.0,
            depth_clip: false,
            ..Default::default()
        })?;

        Ok((pso, pso_wireframe, pso_shadows))
    }
}

const fn compute_edge_table() -> [u16; 256] {
    let mut masks = [0u16; 256];
    let mut i = 0;
    while i < 256 {
        let mask = i;
        let mut edge = 0;
        while edge < 12 {
            let (a, b) = EDGES[edge];
            if ((mask >> a) ^ (mask >> b)) & 1 != 0 {
                masks[i] |= 1 << edge;
            }
            edge += 1;
        }
        i += 1;
    }
    masks
}

// https://www.mattkeeter.com/projects/qef/
// https://github.com/larsbrubaker/tessellation/blob/main/src/qef.rs
// pain

struct Qef {
    ata: [f32; 6], // xx xy xz yy yz zz
    atb: Vec3,
    btb: f32,
    mass: Vec3,
    normals: Vec3,
    count: f32,
}

impl Qef {
    fn add(&mut self, point: Vec3, norm: Vec3) {
        self.ata[0] += norm.x * norm.x;
        self.ata[1] += norm.x * norm.y;
        self.ata[2] += norm.x * norm.z;
        self.ata[3] += norm.y * norm.y;
        self.ata[4] += norm.y * norm.z;
        self.ata[5] += norm.z * norm.z;

        let np = norm.dot(point);
        self.atb += norm * np;
        self.mass += point;
        self.btb += np * np;
        self.count += 1.;

        self.normals += norm;
    }

    fn solve(&self, lambda: f32) -> (Vec3, Vec3) {
        let mean = self.mass / self.count;
        let [xx, xy, xz, yy, yz, zz] = self.ata;
        let ata = Mat3::from_cols(
            Vec3::new(xx, xy, xz),
            Vec3::new(xy, yy, yz),
            Vec3::new(xz, yz, zz),
        );
        let b_rel_mean = self.atb - ata * mean;
        let res_norm = self.normals.normalize_or_zero();
        let y = (ata + Mat3::from_diagonal(Vec3::splat(lambda))).inverse() * b_rel_mean;
        ((mean + y).clamp(Vec3::ZERO, Vec3::ONE), res_norm)
    }
}