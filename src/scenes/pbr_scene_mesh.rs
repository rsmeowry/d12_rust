use egui::Ui;
use crate::app::RenderOpts;
use crate::assets::{load_assimp_scene, LoadTex, LoadedScene, LodCount};
use crate::render::builtin::VertexPbr;
use crate::render::deferred::pbr_lighting::PbrLightingPass;
use crate::render::deferred::post::{BlitPass, PostProcessPass, PostProperties};
use crate::render::deferred::GBuffer;
use crate::render::graph::pass::ResLoad;
use crate::render::graph::FrameGraph;
use crate::render::lights::GpuLight;
use crate::render::mesh::DrawTopology;
use crate::render::pipeline::material::MaterialParams;
use crate::render::renderer::Renderer;
use crate::scenes::Scene;
use crate::time::Time;
use glam::{Mat4, Vec3};
use windows::Win32::Graphics::Direct3D12::{ID3D12Resource, D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE};

#[derive(Debug)]
pub struct PbrSampleMesh {
    gbuffer: GBuffer,

    model: LoadedScene,
    materials_buf: ID3D12Resource,

    lighting: PbrLightingPass,
    post: PostProcessPass,
    blit: BlitPass,
}

impl Scene for PbrSampleMesh {
    const NAME: &'static str = "PBR Mesh";

    fn ui(&mut self, ui: &mut Ui) -> anyhow::Result<()> {
        Ok(())
    }

    fn setup(renderer: &mut Renderer) -> anyhow::Result<Self> {
        let lighting = PbrLightingPass::new(renderer,
                                            "assets/textures/pbr/warm_reception_dinner_4k.hdr",
                                            "assets/textures/pbr/ibl_irradiance.dds",
                                            "assets/textures/pbr/ibl_specular.dds",
                                            "assets/textures/pbr/ibl_brdf_lut.dds",
        )?;

        let mut batch = renderer.gd.begin_upload()?;

        let mut materials = Vec::new();
        let scene = load_assimp_scene::<VertexPbr, _>(
            &renderer, &mut batch, "assets/models/bananas/bananas_1k.gltf",
            &mut materials,
            LoadTex::ALBEDO | LoadTex::NORMAL | LoadTex::ROUGHNESS | LoadTex::LIGHTMAP, DrawTopology::TriangleList,
            true, LodCount::NoLods,
            |mat, tex| MaterialParams::Pbr {
                albedo_tex: tex.albedo,
                normal_tex: tex.normal,
                metal_roughness_tex: tex.roughness,
                occlusion_tex: tex.lightmap,
                base_color: mat.base_color().map(|it| Vec3::new(it.x, it.y, it.z)).unwrap_or(Vec3::ONE),
                metallic: mat.metallic_factor().unwrap_or(1.0),
                roughness: mat.roughness_factor().unwrap_or(1.0),
            }.to_gpu_material()
        )?;

        let materials_buf = renderer.gd.create_default_buffer(&mut batch, bytemuck::cast_slice(&materials), D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE, "MaterialBuffer")?;

        batch.finish()?;

        let gbuffer = renderer.build_gbuffer::<VertexPbr>("deferred/GPassPBR.hlsl", DrawTopology::TriangleList)?;

        let post = PostProcessPass::new(renderer)?;
        let blit = BlitPass::new(post.output_tex(), false, renderer)?;

        Ok(Self {
            gbuffer,
            blit,

            model: scene,
            materials_buf,
            lighting,
            post
        })
    }

    fn build_graph<'a>(&'a mut self, renderer: &'a Renderer, _time: &Time, graph: &mut FrameGraph<'a>, opts: &mut RenderOpts) -> anyhow::Result<()> {
        let pass = if opts.use_wireframe { self.gbuffer.gpass_wireframe } else { self.gbuffer.gpass };
        let (mut objects, draws) = self.model.to_objects_and_draws();

        let draws = draws.into_iter().map(|it| (it, pass)).collect::<Vec<_>>();
        for obj in &mut objects {
            obj.model = Mat4::from_scale(6. * Vec3::ONE) * obj.model;
        }

        let objects_va = renderer.upload_objects(&objects)?;
        let materials_va = unsafe { self.materials_buf.GetGPUVirtualAddress() };

        let lights = vec![
            GpuLight::directional(Vec3::new(-0.4, -0.8, 0.5), Vec3::new(1., 0.95, 0.9), 0.6),
            // GpuLight::point(Vec3::new(3., 3., 0.), 8., Vec3::new(0.2, 0.6, 0.3), 16.),
            // GpuLight::point(Vec3::new(-3., 3., 0.), 8., Vec3::new(0.2, 0.3, 0.9), 16.),
        ];

        let gr = &renderer.gr;

        let rs = renderer.rs();
        let pipelines = renderer.pipelines();

        let post_props = PostProperties {
            albedo: gr.get(self.gbuffer.albedo)?.srv_ptr(),
            normals: gr.get(self.gbuffer.normals)?.srv_ptr(),
            materials: gr.get(self.gbuffer.materials)?.srv_ptr(),
            depth: gr.get(self.gbuffer.scene_depth)?.srv_ptr(),
            scene_color: gr.get(self.gbuffer.scene_color)?.srv_ptr(),
            enable_sobel: 0,
            enable_fly_eye: 0,
            ..Default::default()
        };

        // scene draw pass
        graph
            .add_pass("GPass")
            .writes_depth(self.gbuffer.scene_depth, ResLoad::Clear)
            .writes(self.gbuffer.albedo, ResLoad::Clear)
            .writes(self.gbuffer.normals, ResLoad::Clear)
            .writes(self.gbuffer.materials, ResLoad::Clear)
            .writes(self.gbuffer.scene_color, ResLoad::Clear)
            .exec(move |ctx| {
                rs.set_objects(ctx.cmd, objects_va);
                rs.set_materials(ctx.cmd, materials_va);
                let mut bound = None;
                for (i, (mesh, pso)) in draws.iter().enumerate() {
                    if bound != Some(*pso) {
                        pipelines.get(*pso).bind(ctx.cmd);
                        bound = Some(*pso);
                    }
                    rs.set_draw_index(ctx.cmd, i as u32);
                    mesh.draw(ctx.cmd);
                }
                Ok(())
            });

        self.lighting.add_pass(lights, renderer, None, None, 1.0, &self.gbuffer, graph, opts)?;
        self.post.add_pass(post_props, None, renderer, &self.gbuffer, graph)?;
        self.blit.add_pass(renderer, graph)?;

        Ok(())
    }
}

