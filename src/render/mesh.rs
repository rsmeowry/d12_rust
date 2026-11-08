pub mod mesh_pipeline;

use std::ptr::null;
use glam::Vec3;
use crate::render::vertex::Vertex;
use crate::wrappers::graphics::{transition_barrier, uav_barrier, GraphicsDevice};
use windows::Win32::Graphics::Direct3D::{D3D_PRIMITIVE_TOPOLOGY, D3D_PRIMITIVE_TOPOLOGY_3_CONTROL_POINT_PATCHLIST, D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST};
use windows::Win32::Graphics::Direct3D12::{ID3D12Device5, ID3D12GraphicsCommandList, ID3D12GraphicsCommandList4, ID3D12Resource, D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_DESC, D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS, D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS_0, D3D12_ELEMENTS_LAYOUT_ARRAY, D3D12_GPU_VIRTUAL_ADDRESS_AND_STRIDE, D3D12_INDEX_BUFFER_VIEW, D3D12_PRIMITIVE_TOPOLOGY_TYPE, D3D12_PRIMITIVE_TOPOLOGY_TYPE_PATCH, D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE, D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_PREFER_FAST_TRACE, D3D12_RAYTRACING_ACCELERATION_STRUCTURE_PREBUILD_INFO, D3D12_RAYTRACING_ACCELERATION_STRUCTURE_TYPE_BOTTOM_LEVEL, D3D12_RAYTRACING_GEOMETRY_DESC, D3D12_RAYTRACING_GEOMETRY_DESC_0, D3D12_RAYTRACING_GEOMETRY_FLAGS, D3D12_RAYTRACING_GEOMETRY_FLAG_OPAQUE, D3D12_RAYTRACING_GEOMETRY_TRIANGLES_DESC, D3D12_RAYTRACING_GEOMETRY_TYPE_TRIANGLES, D3D12_RESOURCE_STATE_GENERIC_READ, D3D12_RESOURCE_STATE_INDEX_BUFFER, D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE, D3D12_RESOURCE_STATE_RAYTRACING_ACCELERATION_STRUCTURE, D3D12_RESOURCE_STATE_VERTEX_AND_CONSTANT_BUFFER, D3D12_VERTEX_BUFFER_VIEW};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_R32G32B32_FLOAT, DXGI_FORMAT_R32_UINT};
use windows_core::Interface;
use crate::render::perf::frustum::AABB;
use crate::wrappers::graphics::batching::UploadBatch;

#[derive(Debug, PartialEq)]
pub struct Mesh {
    vb: ID3D12Resource,
    _ib: ID3D12Resource,
    blas: Option<ID3D12Resource>,
    vbv: D3D12_VERTEX_BUFFER_VIEW,
    ibv: D3D12_INDEX_BUFFER_VIEW,
    indices: u32,
    topology: DrawTopology,
    aabb: AABB,
    is_static: bool,
}

impl Mesh {
    pub fn new<TVert: Vertex>(
        gd: &GraphicsDevice,
        batch: &mut UploadBatch<'_>,
        vertices: &[TVert],
        indices: &[u32],
        topology: DrawTopology,
        aabb: AABB,
        is_static: bool,
        build_blas: bool,
    ) -> anyhow::Result<Self> {
        Self::new_from_bytes::<TVert>(gd, batch, bytemuck::cast_slice(vertices), bytemuck::cast_slice(indices), indices.len() as u32, topology, aabb, is_static, build_blas)
    }

    pub fn new_from_bytes<TVert: Vertex>(
        gd: &GraphicsDevice,
        batch: &mut UploadBatch<'_>,
        vertices: &[u8],
        indices: &[u8],
        index_count: u32,
        topology: DrawTopology,
        aabb: AABB,
        is_static: bool,
        build_blas: bool,
    ) -> anyhow::Result<Self> {
        let id: u16 = rand::random();

        let vb = gd.create_default_buffer(batch, vertices, D3D12_RESOURCE_STATE_VERTEX_AND_CONSTANT_BUFFER | D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE, format!("Mesh_VB_{:04x}", id))?;
        let ib = gd.create_default_buffer(batch, indices,  D3D12_RESOURCE_STATE_INDEX_BUFFER | D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE, format!("Mesh_IB_{:04x}", id))?;

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

        let blas = if build_blas { unsafe {
            Some(Self::build_blas::<TVert>(batch, id, &ib, &vb, (vertices.len() / size_of::<TVert>()) as u32, index_count)?)
        } } else { None };

        Ok(Self {
            vb,
            _ib: ib,
            blas,
            vbv,
            ibv,
            indices: index_count,
            topology,
            aabb,
            is_static
        })
    }

    pub fn vb_va(&self) -> u64 {
        unsafe { self.vb.GetGPUVirtualAddress() }
    }

    pub fn blas_va(&self) -> Option<u64> {
        self.blas.as_ref().map(|it| unsafe { it.GetGPUVirtualAddress() })
    }
    
    pub fn is_static(&self) -> bool { 
        self.is_static
    }

    pub fn index_count(&self) -> u32 { self.indices }

    pub fn aabb(&self) -> &AABB {
        &self.aabb
    }

    pub fn bind(&self, cmd: &ID3D12GraphicsCommandList) {
        unsafe {
            cmd.IASetPrimitiveTopology(self.topology.to_d3d());
            cmd.IASetVertexBuffers(0, Some(&[self.vbv]));
            cmd.IASetIndexBuffer(Some(&self.ibv));
        }
    }

    pub fn bounding_sphere(&self) -> (Vec3, f32) {
        let center = (self.aabb.min + self.aabb.max) * 0.5;
        (center, (self.aabb.max - self.aabb.min).length() * 0.5)
    }

    pub fn draw(&self, cmd: &ID3D12GraphicsCommandList) {
        unsafe {
            cmd.IASetPrimitiveTopology(self.topology.to_d3d());
            cmd.IASetVertexBuffers(0, Some(&[self.vbv]));
            cmd.IASetIndexBuffer(Some(&self.ibv));
            cmd.DrawIndexedInstanced(self.indices, 1, 0, 0, 0);
        }
    }

    pub fn draw_instanced(&self, cmd: &ID3D12GraphicsCommandList, instances: u32) {
        unsafe {
            cmd.IASetPrimitiveTopology(self.topology.to_d3d());
            cmd.IASetVertexBuffers(0, Some(&[self.vbv]));
            cmd.IASetIndexBuffer(Some(&self.ibv));
            cmd.DrawIndexedInstanced(self.indices, instances, 0, 0, 0);
        }
    }

    unsafe fn build_blas<T: Vertex>(batch: &mut UploadBatch, id: u16, ib: &ID3D12Resource, vb: &ID3D12Resource, vertex_count: u32, index_count: u32) -> anyhow::Result<ID3D12Resource> {
        let geom_desc = D3D12_RAYTRACING_GEOMETRY_DESC {
            Type: D3D12_RAYTRACING_GEOMETRY_TYPE_TRIANGLES,
            Flags: D3D12_RAYTRACING_GEOMETRY_FLAG_OPAQUE,
            Anonymous: D3D12_RAYTRACING_GEOMETRY_DESC_0 {
                Triangles: D3D12_RAYTRACING_GEOMETRY_TRIANGLES_DESC {
                    Transform3x4: 0,
                    IndexFormat: DXGI_FORMAT_R32_UINT,
                    VertexFormat: DXGI_FORMAT_R32G32B32_FLOAT,
                    IndexCount: index_count,
                    VertexCount: vertex_count,
                    IndexBuffer: ib.GetGPUVirtualAddress(),
                    VertexBuffer: D3D12_GPU_VIRTUAL_ADDRESS_AND_STRIDE {
                        StartAddress: vb.GetGPUVirtualAddress(),
                        StrideInBytes: size_of::<T>() as u64,
                    },
                }
            },
        };

        let as_inputs = D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS {
            Type: D3D12_RAYTRACING_ACCELERATION_STRUCTURE_TYPE_BOTTOM_LEVEL,
            Flags: D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_PREFER_FAST_TRACE,
            NumDescs: 1,
            DescsLayout: D3D12_ELEMENTS_LAYOUT_ARRAY,
            Anonymous: D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS_0 {
                pGeometryDescs: &geom_desc as *const _,
            },
        };

        let mut as_build_info = D3D12_RAYTRACING_ACCELERATION_STRUCTURE_PREBUILD_INFO::default();

        let dev = batch.gd().device().cast::<ID3D12Device5>()?;
        dev.GetRaytracingAccelerationStructurePrebuildInfo(&as_inputs, &mut as_build_info);

        let scratch_size = as_build_info.ScratchDataSizeInBytes as usize;
        let res_size = as_build_info.ResultDataMaxSizeInBytes as usize;
        let blas_scratch = GraphicsDevice::create_uav_buffer(&dev, scratch_size, format!("BLAS_Scratch_{id:04x}"))?;
        let blas_result = GraphicsDevice::create_as_buffer(&dev, res_size, format!("BLAS_Result_{id:04x}"))?;

        let as_desc = D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_DESC {
            DestAccelerationStructureData: blas_result.GetGPUVirtualAddress(),
            Inputs: as_inputs,
            ScratchAccelerationStructureData: blas_scratch.GetGPUVirtualAddress(),
            ..Default::default()
        };
        batch.leave_and_forget(blas_scratch);

        let cmd = batch.cmd().cast::<ID3D12GraphicsCommandList4>()?;
        cmd.BuildRaytracingAccelerationStructure(&as_desc, None);
        cmd.ResourceBarrier(&[uav_barrier(&blas_result)]);

        Ok(blas_result)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub enum DrawTopology {
    #[default]
    TriangleList,
    PatchList3
}

impl DrawTopology {
    pub fn to_d3d(&self) -> D3D_PRIMITIVE_TOPOLOGY {
        match self {
            DrawTopology::TriangleList => D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
            DrawTopology::PatchList3 => D3D_PRIMITIVE_TOPOLOGY_3_CONTROL_POINT_PATCHLIST,
        }
    }

    pub fn to_d3d12_type(&self) -> D3D12_PRIMITIVE_TOPOLOGY_TYPE {
        match self {
            DrawTopology::TriangleList => D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
            DrawTopology::PatchList3 => D3D12_PRIMITIVE_TOPOLOGY_TYPE_PATCH
        }
    }
}

