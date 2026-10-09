use crate::assets::{load_assimp_scene, LoadedScene};
use crate::render::builtin::Vertex3;
use crate::render::deferred::{DeferredLightingPass, GBuffer};
use crate::render::graph::pass::ResLoad;
use crate::render::graph::FrameGraph;
use crate::render::lights::GpuLight;
use crate::render::mesh::Mesh;
use crate::render::pipeline::material::MaterialParams;
use crate::render::pipeline::PipelineId;
use crate::render::deferred::post::{BlitPass, PostProcessPass, PostProperties};
use crate::render::renderer::{ObjectData, Renderer};
use crate::scenes::Scene;
use crate::time::Time;
use glam::Vec3;
use windows::Win32::Graphics::Direct3D12::{ID3D12Resource, D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE};

#[derive(Debug)]
pub struct SponzaWithLights {
    gbuffer: GBuffer,

    sponza: LoadedScene,

    materials_buf: ID3D12Resource,

    lighting: DeferredLightingPass,
    post: PostProcessPass,
    blit: BlitPass
}

impl Scene for SponzaWithLights {
    fn setup(renderer: &mut Renderer) -> anyhow::Result<Self> {
        let lighting = DeferredLightingPass::new(renderer)?;

        let mut batch = renderer.gd.begin_upload()?;

        let mut materials = Vec::new();
        let sponza = load_assimp_scene(
            &renderer.gd, &mut batch, "assets/sponza_old/Sponza.gltf",
            renderer.white_tex().srv_ptr(), &mut materials,
            |pos, norm, uv, _col| Vertex3::new(pos, norm, uv),
            |_mat, tex| MaterialParams::Phong {
                tex,
                ambient: Vec3::new(0.03, 0.03, 0.03),
                diffuse: Vec3::ONE,
                specular: Vec3::ONE,
                shininess: 32.0,
                diffuse_k: 0.4,
                specular_k: 0.7,
            }.to_gpu_material()
        )?;

        let materials_buf = renderer.gd.create_default_buffer(&mut batch, bytemuck::cast_slice(&materials), D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE, "Sponza/MaterialBuffer")?;

        batch.finish()?;

        let gbuffer = renderer.build_gbuffer("deferred/GPass.hlsl")?;

        let post = PostProcessPass::new(renderer)?;
        let blit = BlitPass::new(post.output_tex(), false, renderer)?;

        Ok(Self {
            gbuffer,
            blit,

            sponza,
            materials_buf,
            lighting,
            post
        })
    }

    fn build_graph<'a>(&'a self, renderer: &'a Renderer, time: &Time, graph: &mut FrameGraph<'a>) -> anyhow::Result<()> {
        let mut objects: Vec<ObjectData> = Vec::new();
        let mut draws: Vec<(&Mesh, PipelineId)> = Vec::new();

        for (scene, pso) in [(&self.sponza, self.gbuffer.gpass)] {
            let (obj, meshes) = scene.to_objects_and_draws();
            objects.extend(obj);
            draws.extend(meshes.into_iter().map(|m| (m, pso)));
        }

        let objects_va = renderer.upload_objects(&objects)?;
        let materials_va = unsafe { self.materials_buf.GetGPUVirtualAddress() };

        let t = time.game_time();
        let lights = vec![
            GpuLight::directional(Vec3::new(-0.4, -0.8, 0.5), Vec3::new(1., 0.95, 0.9), 0.6),
            GpuLight::point(Vec3::new(4. * t.sin(), 1., -1.2), 6., Vec3::new(1., 0.3, 0.2), 8.),
            GpuLight::point(Vec3::new(-4. * t.sin(), 1., 1.2), 6., Vec3::new(0.2, 0.5, 1.), 8.),
            GpuLight::point(Vec3::new(2., 2. + 2. * t.cos(), 0.), 8., Vec3::new(0.2, 0.9, 0.2), 8.),
            GpuLight::spot(Vec3::new(4., 2., -1.), Vec3::NEG_Y, 15., 15., 25.,  Vec3::ONE, 20.),
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
            enable_sobel: 1,
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

        self.lighting.add_pass(lights, renderer, &self.gbuffer, graph)?;
        self.post.add_pass(post_props, renderer, &self.gbuffer, graph)?;
        self.blit.add_pass(renderer, graph)?;

        Ok(())
    }
}

