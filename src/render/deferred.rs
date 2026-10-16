use crate::render::builtin::{aabb_from_vertices3, Vertex3};
use crate::render::graph::pass::ResLoad;
use crate::render::graph::res::{ResId, ResSize};
use crate::render::graph::FrameGraph;
use crate::render::lights::{light_cone, unit_sphere, GpuLight, LIGHT_POINT, LIGHT_SPOT};
use crate::render::mesh::{DrawTopology, Mesh};
use crate::render::pipeline::desc::{Blend, Depth, PipelineDesc};
use crate::render::pipeline::PipelineId;
use crate::render::renderer::{Renderer, GB_ALBEDO, GB_MATERIAL, GB_NORMALS, SCENE_FORMAT};
use crate::render::shadows::{GpuCascades, ShadowCascades, ShadowMap};
use crate::render::texture::Texture;
use crate::render::vertex::Vertex;
use crate::wrappers::graphics::GraphicsDevice;
use glam::{Vec3, Vec4, Vec4Swizzles};
use std::io::Read;
use std::path::Path;
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use windows::Win32::Graphics::Direct3D12::{D3D12_COMPARISON_FUNC_LESS_EQUAL, D3D12_CULL_MODE_FRONT, D3D12_CULL_MODE_NONE};

pub mod post;
pub mod util;
pub mod bloom;
pub mod fog;
pub mod lighting;
pub mod pbr_lighting;

#[derive(Debug, Copy, Clone)]
pub struct GBuffer {
    pub albedo: ResId,
    pub normals: ResId,
    pub materials: ResId,
    pub scene_depth: ResId,
    pub scene_color: ResId,

    pub gpass: PipelineId,
    pub gpass_wireframe: PipelineId,
}

impl Renderer {
    pub fn build_gbuffer<V: Vertex>(&mut self, pass_shader: &'static str, topology: DrawTopology) -> anyhow::Result<GBuffer> {
        let scene_color = self.gr.color(&self.gd, "SceneColor", SCENE_FORMAT, Vec4::ZERO, ResSize::FitScreen)?; // zero: lighting/sky are blended additively on top
        let scene_depth = self.gr.depth(&self.gd, "SceneDepth", ResSize::FitScreen)?;
        let albedo = self.gr.color(&self.gd, "SceneAlbedo", GB_ALBEDO, Vec4::ZERO, ResSize::FitScreen)?;
        let materials = self.gr.color(&self.gd, "SceneSpecular", GB_MATERIAL, Vec4::ZERO, ResSize::FitScreen)?;
        let normals = self.gr.color(&self.gd, "SceneNormals", GB_NORMALS, Vec4::ZERO, ResSize::FitScreen)?;

        let gpass = self.create_pipeline(PipelineDesc {
            rts: vec![GB_ALBEDO, GB_NORMALS, GB_MATERIAL, SCENE_FORMAT],
            name: "GPass",
            shader: pass_shader.into(),
            input: V::LAYOUT,
            topology: topology.to_d3d12_type(),
            ..Default::default()
        })?;
        let gpass_wireframe = self.create_pipeline(PipelineDesc {
            rts: vec![GB_ALBEDO, GB_NORMALS, GB_MATERIAL, SCENE_FORMAT],
            name: "GPassWireframe",
            shader: pass_shader.into(),
            input: V::LAYOUT,
            wireframe: true,
            topology: topology.to_d3d12_type(),
            ..Default::default()
        })?;

        Ok(GBuffer {
            albedo,
            normals,
            materials,
            scene_depth,
            scene_color,
            gpass,
            gpass_wireframe,
        })
    }
}

