use glam::{Mat4, Vec3, Vec4};
use windows::Win32::Graphics::Direct3D12::{D3D12_COMPARISON_FUNC_GREATER_EQUAL, D3D12_COMPARISON_FUNC_LESS_EQUAL, D3D12_CULL_MODE_FRONT, D3D12_CULL_MODE_NONE};
use crate::render::builtin::Vertex3;
use crate::render::graph::FrameGraph;
use crate::render::graph::pass::{PassCtx, ResLoad};
use crate::render::graph::res::{ResId, ResSize};
use crate::render::lights::GpuLight;
use crate::render::mesh::Mesh;
use crate::render::pipeline::desc::{Depth, PipelineDesc};
use crate::render::pipeline::PipelineId;
use crate::render::renderer::{ObjectData, Renderer};
use crate::render::texture::Texture;
use crate::render::vertex::Vertex;
use crate::wrappers::graphics::GraphicsDevice;

#[derive(Debug)]
pub struct ShadowMap {
    pipeline: PipelineId,
    cascades: Vec<ResId>,
    size: u32,
}

#[derive(Debug)]
pub struct ShadowCascades {
    pub cascades: Vec<ResId>,
    pub view_proj: Vec<Mat4>,
    pub texel: Vec4
}

impl ShadowMap {
    pub fn new(renderer: &mut Renderer,  width: u32, height: u32) -> anyhow::Result<Self> {
        let tex = renderer.gr.depth(&renderer.gd, "ShadowMap", ResSize::Fixed(width, height))?;
        let cascades = vec![tex];
        let pipeline = renderer.create_pipeline(PipelineDesc {
            name: "ShadowDepth",
            shader: "deferred/ShadowDepth.hlsl".into(),
            input: Vertex3::LAYOUT,
            rts: vec![],
            depth: Some(Depth {
                format: GraphicsDevice::DS_FORMAT,
                func: D3D12_COMPARISON_FUNC_GREATER_EQUAL,
                write: true,
            }),
            cull: D3D12_CULL_MODE_NONE,
            depth_bias: -100,
            slope_bias: -2.0,
            depth_clip: false,
            ..Default::default()
        })?;
        Ok(Self {
            pipeline,
            cascades,
            size: width
        })
    }

    pub fn tex(&self) -> ResId {
        *self.cascades.first().unwrap()
    }

    pub fn add_pass<'f>(&'f self, main_light: GpuLight, renderer: &'f Renderer, graph: &mut FrameGraph<'f>, objects_va: u64, meshes: Vec<&'f Mesh>) -> anyhow::Result<ShadowCascades> {
        let diag: f32 = 40.;
        let proj = glam::camera::lh::proj::directx::orthographic(-diag / 2., diag / 2., -diag / 2., diag / 2., 100., 0.1);
        let view = glam::camera::lh::view::look_to_mat4(Vec3::ZERO - diag * main_light.direction, main_light.direction, Vec3::Y);
        let view_proj = proj * view;
        let lights_vp_va = renderer.upload_cbuffer(&view_proj)?;
        let pipelines = renderer.pipelines();
        let rs = renderer.rs();

        graph
            .add_pass("Shadow0")
            .writes_depth(self.cascades[0], ResLoad::Clear)
            .exec(move |ctx| {
                rs.set_pass(ctx.cmd, lights_vp_va);
                pipelines.get(self.pipeline).bind(ctx.cmd);
                rs.set_objects(ctx.cmd, objects_va);
                for (i, mesh) in meshes.iter().enumerate() {
                    rs.set_draw_index(ctx.cmd, i as u32);
                    mesh.draw(ctx.cmd);
                }
                Ok(())
            });

        let texel = 2.0 * diag / 2. / self.size as f32;
        Ok(ShadowCascades {
            cascades: vec![self.cascades[0]],
            view_proj: vec![view_proj],
            texel: Vec4::new(texel, texel, texel, texel),
        })
    }
}