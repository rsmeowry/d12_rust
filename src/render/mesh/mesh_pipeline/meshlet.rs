use meshopt::VertexDataAdapter;
use windows::Win32::Graphics::Direct3D12::{ID3D12Resource, D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE, D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE};
use crate::render::mesh::Mesh;
use crate::render::vertex::Vertex;
use crate::wrappers::graphics::batching::UploadBatch;

#[derive(Debug)]
pub struct MeshletMesh {
    mb: ID3D12Resource,
    meshlet_count: u32,
}

impl MeshletMesh {
    pub fn upload(batch: &mut UploadBatch, bytes: &[u8], meshlet_count: u32) -> anyhow::Result<Self> {
        let mb = batch.gd().create_default_buffer(batch, &bytes, D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE, "MeshletsBuffer")?;
        Ok(Self {
            mb,
            meshlet_count: meshlet_count,
        })
    }

    pub fn build_bytes<V: Vertex>(vertices: &[u8], indices: &[u32]) -> anyhow::Result<(Vec<u8>, u32)> {
        let vda = VertexDataAdapter::new(vertices, size_of::<V>(), 0)?;
        let meshlets = meshopt::build_meshlets(indices, &vda, 64, 124, 0.25);
        let mut gpu = Vec::with_capacity(meshlets.len());
        let mut verts = Vec::<u32>::new();
        let mut tris = Vec::<u32>::new();
        for meshlet in meshlets.iter() {
            let b = meshopt::compute_meshlet_bounds(meshlet, &vda);
            gpu.push(GpuMeshlet {
                vertex_offset: verts.len() as u32,
                triangle_offset: tris.len() as u32,
                vertex_count: meshlet.vertices.len() as u32,
                triangle_count: meshlet.triangles.len() as u32 / 3,
                center: b.center,
                radius: b.radius,
                cone_axis: b.cone_axis,
                cone_cutoff: b.cone_cutoff,
            });
            verts.extend_from_slice(&meshlet.vertices);
            tris.extend(meshlet.triangles.chunks_exact(3).map(|t| t[0] as u32 | (t[1] as u32) << 8 | (t[2] as u32) << 16));
        }

        let vert_offset = (size_of::<MeshletHeader>() + gpu.len() * size_of::<GpuMeshlet>()) as u32;
        let header = MeshletHeader {
            meshlet_count: gpu.len() as u32,
            vert_offset,
            tri_offset: vert_offset + verts.len() as u32 * 4,
            _pad: 0,
        };

        let mut bytes = Vec::new();
        bytes.extend_from_slice(bytemuck::cast_slice(&[header]));
        bytes.extend_from_slice(&bytemuck::cast_slice(&gpu));
        bytes.extend_from_slice(bytemuck::cast_slice(&verts));
        bytes.extend_from_slice(bytemuck::cast_slice(&tris));

        Ok((bytes, meshlets.len() as u32))
    }

    pub fn va(&self) -> u64 {
        unsafe { self.mb.GetGPUVirtualAddress() }
    }

    pub fn count(&self) -> u32 {
        self.meshlet_count
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct MeshletHeader {
    meshlet_count: u32,
    vert_offset: u32,
    tri_offset: u32,
    _pad: u32,
}

#[repr(C)] #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuMeshlet {
    vertex_offset: u32,
    triangle_offset: u32,
    vertex_count: u32,
    triangle_count: u32,
    center: [f32; 3],
    radius: f32,
    cone_axis: [f32; 3],
    cone_cutoff: f32,
}