use crate::render::vertex::Vertex;
use crate::wrappers::graphics::GraphicsDevice;
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use windows::Win32::Graphics::Direct3D12::{
    D3D12_INDEX_BUFFER_VIEW, D3D12_VERTEX_BUFFER_VIEW, ID3D12GraphicsCommandList, ID3D12Resource,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R32_UINT;

#[derive(Debug)]
pub struct Mesh {
    _vb: ID3D12Resource,
    _ib: ID3D12Resource,
    vbv: D3D12_VERTEX_BUFFER_VIEW,
    ibv: D3D12_INDEX_BUFFER_VIEW,
    indices: u32,
}

impl Mesh {
    pub fn new<TVert: Vertex>(
        gd: &GraphicsDevice,
        vertices: &[TVert],
        indices: &[u32],
    ) -> anyhow::Result<Self> {
        let id: u16 = rand::random();
        let vb = gd.create_buffer(bytemuck::cast_slice(vertices), format!("Mesh_VB_{:4x}", id))?;
        let ib = gd.create_buffer(bytemuck::cast_slice(indices), format!("Mesh_IB_{:4x}", id))?;
        let vbv = D3D12_VERTEX_BUFFER_VIEW {
            BufferLocation: unsafe { vb.GetGPUVirtualAddress() },
            SizeInBytes: size_of_val(vertices) as u32,
            StrideInBytes: size_of::<TVert>() as u32,
        };
        let ibv = D3D12_INDEX_BUFFER_VIEW {
            BufferLocation: unsafe { ib.GetGPUVirtualAddress() },
            SizeInBytes: size_of_val(indices) as u32,
            Format: DXGI_FORMAT_R32_UINT,
        };

        Ok(Self {
            _vb: vb,
            _ib: ib,
            vbv,
            ibv,
            indices: indices.len() as u32,
        })
    }

    pub fn draw(&self, cmd: &ID3D12GraphicsCommandList) {
        unsafe {
            cmd.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            cmd.IASetVertexBuffers(0, Some(&[self.vbv]));
            cmd.IASetIndexBuffer(Some(&self.ibv));
            cmd.DrawIndexedInstanced(self.indices, 1, 0, 0, 0);
        }
    }
}
