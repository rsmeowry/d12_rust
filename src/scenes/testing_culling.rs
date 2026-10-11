use crate::app::RenderOpts;
use crate::assets::{load_assimp_scene, LoadTex};
use crate::render::builtin::Vertex3;
use crate::render::deferred::post::{BlitPass, PostProcessPass, PostProperties};
use crate::render::deferred::{DeferredLightingPass, GBuffer};
use crate::render::graph::pass::ResLoad;
use crate::render::graph::FrameGraph;
use crate::render::lights::GpuLight;
use crate::render::mesh::{DrawTopology, Mesh};
use crate::render::perf::Culler;
use crate::render::pipeline::material::MaterialParams;
use crate::render::pipeline::PipelineId;
use crate::render::renderer::{ObjectData, Renderer};
use crate::render::texture::Texture;
use crate::scenes::Scene;
use crate::time::Time;
use glam::{Mat4, Quat, Vec3};
use windows::Win32::Graphics::Direct3D12::{ID3D12Resource, D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE};

#[derive(Debug)]
pub struct CullingScene {
    gbuffer: GBuffer,

    textures: Vec<Texture>,
    objects: Vec<ObjectData>,
    mesh: Mesh,

    materials_buf: ID3D12Resource,

    culler: Culler,
    lighting: DeferredLightingPass,
    post: PostProcessPass,
    blit: BlitPass
}

impl Scene for CullingScene {
    const NAME: &'static str = "Pusheens (10K)";

    fn setup(renderer: &mut Renderer) -> anyhow::Result<Self> {
        let lighting = DeferredLightingPass::new(renderer)?;

        let mut batch = renderer.gd.begin_upload()?;
        let mut materials = Vec::new();
        let mut objects = Vec::<ObjectData>::new();
        let scene = load_assimp_scene::<Vertex3, _>(
            &renderer.gd, &mut batch, "assets/models/pusheen_holding_cookie/scene.gltf",
            renderer.white_tex().srv_ptr(), &mut materials,
            LoadTex::ALBEDO, DrawTopology::TriangleList,
            |_mat, tex| MaterialParams::Phong {
                tex: tex.albedo,
                ambient: Vec3::new(0.03, 0.03, 0.03),
                diffuse: Vec3::ONE,
                specular: Vec3::ONE,
                shininess: 32.0,
                diffuse_k: 0.4,
                specular_k: 0.7,
            }.to_gpu_material()
        )?;
        let (mesh, _model, textures) = scene.into_single_mesh();

        for _ in 0..10000 {
            let pos = Vec3::new(rand::random(), rand::random(), rand::random()) * 200. - 100.;
            let object_data = ObjectData {
                model: Mat4::from_scale_rotation_translation(4. * Vec3::ONE, Quat::IDENTITY, pos),
                material_id: 0,
                _pad: [0; 3],
            };
            objects.push(object_data);
        }

        let materials_buf = renderer.gd.create_default_buffer(&mut batch, bytemuck::cast_slice(&materials), D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE, "MaterialBuffer")?;

        batch.finish()?;

        let gbuffer = renderer.build_gbuffer::<Vertex3>("deferred/GPass.hlsl", DrawTopology::TriangleList)?;

        let post = PostProcessPass::new(renderer)?;
        let blit = BlitPass::new(post.output_tex(), false, renderer)?;

        let culler = Culler::new(&objects, &vec![&mesh; objects.len()]);

        Ok(Self {
            gbuffer,
            blit,

            culler,
            objects,
            mesh,
            textures,
            materials_buf,
            lighting,
            post
        })
    }

    fn build_graph<'a>(&'a mut self, renderer: &'a Renderer, _time: &Time, graph: &mut FrameGraph<'a>, opts: &mut RenderOpts) -> anyhow::Result<()> {
        let mut objects: Vec<ObjectData> = Vec::new();
        let mut draws: Vec<(&Mesh, PipelineId)> = Vec::new();

        let pass = if opts.use_wireframe { self.gbuffer.gpass_wireframe } else { self.gbuffer.gpass };

        let frustum = renderer.cam.frustum();
        self.culler.push_visible_raw(
            &mut objects,
            &mut draws,
            &self.objects,
            &vec![&self.mesh; self.objects.len()],
            pass,
            &frustum,
            opts
        );
        self.culler.finish(opts);


        let objects_va = renderer.upload_objects(&objects)?;
        let materials_va = unsafe { self.materials_buf.GetGPUVirtualAddress() };

        let lights = vec![
            GpuLight::directional(Vec3::new(-0.4, -0.8, 0.5), Vec3::new(1., 0.95, 0.9), 0.6),
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

