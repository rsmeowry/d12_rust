use crate::app::RenderOpts;
use crate::assets::{load_assimp_scene, LoadTex, LoadedScene};
use crate::render::builtin::VertexTess;
use crate::render::deferred::post::{BlitPass, PostProcessPass, PostProperties};
use crate::render::deferred::{DeferredLightingPass, GBuffer};
use crate::render::graph::pass::ResLoad;
use crate::render::graph::FrameGraph;
use crate::render::lights::GpuLight;
use crate::render::mesh::{DrawTopology, Mesh};
use crate::render::pipeline::material::MaterialParams;
use crate::render::pipeline::PipelineId;
use crate::render::renderer::{ObjectData, Renderer};
use crate::render::texture::{MipCount, Texture};
use crate::scenes::Scene;
use crate::time::Time;
use glam::{Mat4, Vec3};
use windows::Win32::Graphics::Direct3D12::{ID3D12Resource, D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE};

#[derive(Debug)]
pub struct TesselatedSample {
    gbuffer: GBuffer,

    object: LoadedScene,
    materials_buf: ID3D12Resource,

    _displacement: Texture,
    lighting: DeferredLightingPass,
    post: PostProcessPass,
    blit: BlitPass,
}

impl Scene for TesselatedSample {
    const NAME: &'static str = "Tesselation (bricks)";

    fn setup(renderer: &mut Renderer) -> anyhow::Result<Self> {
        let lighting = DeferredLightingPass::new(renderer)?;

        let mut batch = renderer.gd.begin_upload()?;

        let displacement_map = Texture::load(&mut batch, &std::fs::read("assets/models/bricks/brick_displacement.png")?, false, MipCount::All, "Displacement")?;

        let mut materials = Vec::new();
        let object = load_assimp_scene::<VertexTess, _>(
            &renderer.gd, &mut batch, "assets/models/bricks/BrickSphere.gltf",
            renderer.white_tex().srv_ptr(), &mut materials,
            LoadTex::ALBEDO | LoadTex::NORMAL, DrawTopology::PatchList3,
            |_mat, tex| MaterialParams::PhongTess {
                albedo: tex.albedo,
                normal: tex.normal,
                displacement: displacement_map.srv_ptr(),
                ambient: Vec3::new(0.03, 0.03, 0.03),
                diffuse: Vec3::ONE,
                specular: Vec3::ONE,
                shininess: 64.0,
                diffuse_k: 0.4,
                specular_k: 0.8,
            }.to_gpu_material()
        )?;

        let materials_buf = renderer.gd.create_default_buffer(&mut batch, bytemuck::cast_slice(&materials), D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE, "MaterialBuffer")?;

        batch.finish()?;

        let gbuffer = renderer.build_gbuffer::<VertexTess>("deferred/GPassTess.hlsl", DrawTopology::PatchList3)?;

        let post = PostProcessPass::new(renderer)?;
        let blit = BlitPass::new(post.output_tex(), false, renderer)?;

        Ok(Self {
            gbuffer,
            blit,

            _displacement: displacement_map,
            object,
            materials_buf,
            lighting,
            post
        })
    }

    fn build_graph<'a>(&'a mut self, renderer: &'a Renderer, _time: &Time, graph: &mut FrameGraph<'a>, opts: &mut RenderOpts) -> anyhow::Result<()> {
        let mut objects: Vec<ObjectData> = Vec::new();
        let mut draws: Vec<(&Mesh, PipelineId)> = Vec::new();

        let pass = if opts.use_wireframe { self.gbuffer.gpass_wireframe } else { self.gbuffer.gpass };

        for (scene, pso) in [(&self.object, pass)] {
            let (obj, meshes) = scene.to_objects_and_draws();
            objects.extend(obj);
            draws.extend(meshes.into_iter().map(|m| (m, pso)));
        }

        for obj in &mut objects {
            obj.model *= Mat4::from_scale(2. * Vec3::ONE);
        }

        let objects_va = renderer.upload_objects(&objects)?;
        let materials_va = unsafe { self.materials_buf.GetGPUVirtualAddress() };

        let lights = vec![
            GpuLight::directional(Vec3::new(-0.4, -0.8, 0.5), Vec3::new(1., 0.95, 0.9), 0.6),
            GpuLight::point(Vec3::new(3., 3., 0.), 8., Vec3::new(0.2, 0.6, 0.3), 16.),
            GpuLight::point(Vec3::new(-3., 3., 0.), 8., Vec3::new(0.2, 0.3, 0.9), 16.),
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

        self.lighting.add_pass(lights, renderer, &self.gbuffer, graph)?;
        self.post.add_pass(post_props, renderer, &self.gbuffer, graph)?;
        self.blit.add_pass(renderer, graph)?;

        Ok(())
    }
}

