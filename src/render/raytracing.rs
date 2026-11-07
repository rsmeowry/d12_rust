pub mod tlas;

use windows::Win32::Graphics::Direct3D12::{ID3D12Device5, ID3D12GraphicsCommandList4, ID3D12Resource, D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_DESC, D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS, D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS_0, D3D12_ELEMENTS_LAYOUT_ARRAY, D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_PREFER_FAST_BUILD, D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_PREFER_FAST_TRACE, D3D12_RAYTRACING_ACCELERATION_STRUCTURE_PREBUILD_INFO, D3D12_RAYTRACING_ACCELERATION_STRUCTURE_TYPE_BOTTOM_LEVEL, D3D12_RAYTRACING_ACCELERATION_STRUCTURE_TYPE_TOP_LEVEL, D3D12_RAYTRACING_INSTANCE_DESC, D3D12_RAYTRACING_INSTANCE_FLAG_TRIANGLE_CULL_DISABLE};
use windows_core::Interface;
use crate::render::graph::FrameGraph;
use crate::render::mesh::Mesh;
use crate::render::raytracing::tlas::Tlas;
use crate::render::renderer::{ObjectData, Renderer};
use crate::wrappers::graphics::{uav_barrier, GraphicsDevice};

#[derive(Debug)]
pub struct RtShadowsPass {
    tlas: Tlas
}

impl RtShadowsPass {
    pub fn new(renderer: &mut Renderer) -> anyhow::Result<Self> {
        let tlas = Tlas::new(&renderer.gd, 4096)?;

        Ok(Self {
            tlas
        })
    }

    pub fn add_pass<'a>(&self, objects: &[ObjectData], meshes: &[&Mesh], rd: &'a Renderer, graph: &mut FrameGraph<'a>) -> anyhow::Result<()> {
        self.tlas.add_build_pass(objects, meshes, rd, graph)?;

        Ok(())
    }
}