use crate::app::RenderOpts;
use crate::assets::{load_assimp_scene, LoadTex, LodCount};
use crate::render::builtin::VertexTess;
use crate::render::deferred::lighting::PhongLightingPass;
use crate::render::deferred::post::{BlitPass, PostProcessPass, PostProperties};
use crate::render::graph::pass::ResLoad;
use crate::render::graph::FrameGraph;
use crate::render::lights::GpuLight;
use crate::render::mesh::{DrawTopology, Mesh};
use crate::render::pipeline::material::MaterialParams;
use crate::render::pipeline::PipelineId;
use crate::render::renderer::{ObjectData, Renderer, SCENE_FORMAT};
use crate::render::texture::{MipCount, Texture};
use crate::scenes::Scene;
use crate::time::Time;
use egui::Ui;
use glam::{Mat4, Vec3, Vec4};
use windows::Win32::Graphics::Direct3D12::{D3D12_CULL_MODE_NONE, D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE};
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use crate::render::deferred::bloom::{BloomPass, BloomProperties};
use crate::render::graph::res::{ResId, ResSize};
use crate::render::pipeline::desc::PipelineDesc;
use crate::wrappers::graphics::GraphicsDevice;

#[derive(Debug)]
pub struct CloudmarchScene {
    scene_color: ResId,
    noise_tex: Texture,
    cloudmarch_pipeline: PipelineId,
    post: PostProcessPass,
    blit: BlitPass,
    bloom: BloomPass,
}

impl Scene for CloudmarchScene {
    const NAME: &'static str = "Cloud marching";

    fn ui(&mut self, ui: &mut Ui) -> anyhow::Result<()> {
        Ok(())
    }

    fn setup(renderer: &mut Renderer) -> anyhow::Result<Self> {
        let cloudmarch_pipeline = renderer.create_pipeline(PipelineDesc {
            rts: vec![SCENE_FORMAT],
            name: "Cloudmarch",
            shader: "experiments/clouds/Cloudmarch.hlsl".into(),
            input: &[],
            depth: None,
            cull: D3D12_CULL_MODE_NONE,
            ..Default::default()
        })?;
        let scene_color = renderer.gr.color(&renderer.gd, "SceneColor", SCENE_FORMAT, Vec4::ZERO, ResSize::FitScreen)?;
        let post = PostProcessPass::new(renderer)?;
        let blit = BlitPass::new(post.output_tex(), false, renderer)?;
        let bloom = BloomPass::new(renderer)?;
        let mut batch = renderer.gd.begin_upload()?;
        let noise_tex = Texture::from_rgba8_3d(&mut batch, 64, 64, 64, &std::fs::read("assets/textures/noise/noise_3d_64.raw")?, "Noise3D")?;
        batch.finish();
        Ok(Self {
            noise_tex,
            cloudmarch_pipeline,
            scene_color,
            blit,
            post,
            bloom
        })
    }

    fn build_graph<'a>(&'a mut self, renderer: &'a Renderer, _time: &Time, graph: &mut FrameGraph<'a>, opts: &mut RenderOpts) -> anyhow::Result<()> {
        let lights = vec![
            GpuLight::directional(Vec3::new(-0.4, -0.8, 0.5), Vec3::new(1., 0.95, 0.9), 0.6),
        ];

        let gr = &renderer.gr;

        let rs = renderer.rs();
        let pipelines = renderer.pipelines();

        let post_props = PostProperties {
            albedo: renderer.white_tex().srv_ptr(),
            normals: renderer.white_tex().srv_ptr(),
            materials: renderer.white_tex().srv_ptr(),
            depth: renderer.white_tex().srv_ptr(),
            scene_color: gr.get(self.scene_color)?.srv_ptr(),
            enable_sobel: 0,
            enable_fly_eye: 0,
            ..Default::default()
        };

        // scene draw pass
        let pass = renderer.upload_cbuffer(&CloudmarchPass {
            noise_tex: self.noise_tex.srv_ptr()
        })?;

        let cloudmarch = self.cloudmarch_pipeline;
        graph
            .add_pass("Cloudmarch")
            .writes(self.scene_color, ResLoad::Clear)
            .exec(move |ctx| {
                rs.set_pass(ctx.cmd, pass);
                pipelines.get(cloudmarch).bind(ctx.cmd);
                rs.set_draw_index(ctx.cmd, 0);
                unsafe {
                    ctx.cmd.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                    ctx.cmd.DrawInstanced(3, 1, 0, 0);
                }
                Ok(())
            });

        let tex = self.bloom.add_pass(BloomProperties {
            ..Default::default()
        }, self.scene_color, renderer, graph)?;

        self.post.add_generic_pass(
            post_props, Some(tex),
            renderer, self.scene_color, self.scene_color, self.scene_color, self.scene_color, self.scene_color, graph)?;

        self.blit.add_pass(renderer, graph)?;

        Ok(())
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CloudmarchPass {
    noise_tex: u32
}