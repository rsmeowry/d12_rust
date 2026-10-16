use crate::app::RenderOpts;
use crate::assets::{load_assimp_scene, LoadTex, LoadedScene};
use crate::render::builtin::VertexPbr;
use crate::render::deferred::post::{BlitPass, PostProcessPass, PostProperties};
use crate::render::deferred::GBuffer;
use crate::render::graph::pass::ResLoad;
use crate::render::graph::FrameGraph;
use crate::render::lights::{unit_sphere, GpuLight};
use crate::render::mesh::{DrawTopology, Mesh};
use crate::render::pipeline::material::MaterialParams;
use crate::render::pipeline::PipelineId;
use crate::render::renderer::{ObjectData, Renderer};
use crate::render::texture::{MipCount, Texture};
use crate::scenes::Scene;
use crate::time::Time;
use glam::{Mat4, Vec3};
use windows::Win32::Graphics::Direct3D12::{ID3D12Resource, D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE};
use crate::render::deferred::lighting::PhongLightingPass;
use crate::render::deferred::pbr_lighting::PbrLightingPass;
use crate::render::perf::frustum::AABB;

#[derive(Debug)]
pub struct PbrSample {
    gbuffer: GBuffer,

    sphere_mesh: Mesh,
    materials_buf: ID3D12Resource,
    objects: Vec<ObjectData>,

    lighting: PbrLightingPass,
    post: PostProcessPass,
    blit: BlitPass,
}

const SPHERES_AMT_X: usize = 6;
const SPHERES_AMT_Y: usize = 6;

impl Scene for PbrSample {
    const NAME: &'static str = "PBR Spheres";

    fn setup(renderer: &mut Renderer) -> anyhow::Result<Self> {
        let lighting = PbrLightingPass::new(renderer,
                                            "assets/textures/pbr/warm_reception_dinner_4k.hdr",
                                            "assets/textures/pbr/ibl_irradiance.dds",
                                            "assets/textures/pbr/ibl_specular.dds",
                                            "assets/textures/pbr/ibl_brdf_lut.dds",
        )?;

        let mut batch = renderer.gd.begin_upload()?;

        let (sphere_vertices, sphere_indices) = unit_sphere(24, 24, |pos, norm, tangent, binorm, uv| {
            VertexPbr::new(pos, norm, tangent, binorm, uv)
        });
        let sphere_mesh = Mesh::new(&renderer.gd, &mut batch, &sphere_vertices, &sphere_indices, DrawTopology::TriangleList, AABB { min: Vec3::ZERO, max: Vec3::ONE })?;

        let mut materials = Vec::new();
        let mut objects = Vec::new();
        for roughness in 0..SPHERES_AMT_X {
            let actual_roughness = roughness as f32 / SPHERES_AMT_X as f32;
            for metalness in 0..SPHERES_AMT_Y {
                let actual_metallness = metalness as f32 / SPHERES_AMT_Y as f32;
                materials.push(MaterialParams::Pbr {
                    albedo_tex: renderer.white_tex().srv_ptr(),
                    normal_tex: renderer.flat_normal_tex().srv_ptr(),
                    metal_roughness_tex: renderer.white_tex().srv_ptr(),
                    base_color: Vec3::new(0.9, 0.1, 0.1),
                    metallic: actual_metallness,
                    roughness: actual_roughness,
                }.to_gpu_material());
                objects.push(ObjectData {
                    model: Mat4::from_translation(Vec3::new(roughness as f32 * 2., metalness as f32 * 2., 0.)),
                    material_id: materials.len() as u32 - 1,
                    _pad: [0; 3],
                })
            }
        }

        let materials_buf = renderer.gd.create_default_buffer(&mut batch, bytemuck::cast_slice(&materials), D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE, "MaterialBuffer")?;

        batch.finish()?;

        let gbuffer = renderer.build_gbuffer::<VertexPbr>("deferred/GPassPBR.hlsl", DrawTopology::TriangleList)?;

        let post = PostProcessPass::new(renderer)?;
        let blit = BlitPass::new(post.output_tex(), false, renderer)?;

        Ok(Self {
            gbuffer,
            blit,

            sphere_mesh,
            objects,
            materials_buf,
            lighting,
            post
        })
    }

    fn build_graph<'a>(&'a mut self, renderer: &'a Renderer, _time: &Time, graph: &mut FrameGraph<'a>, opts: &mut RenderOpts) -> anyhow::Result<()> {
        let pass = if opts.use_wireframe { self.gbuffer.gpass_wireframe } else { self.gbuffer.gpass };
        let draws: Vec<(&Mesh, PipelineId)> = vec![(&self.sphere_mesh, pass); SPHERES_AMT_X * SPHERES_AMT_Y];

        let objects_va = renderer.upload_objects(&self.objects)?;
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

        self.lighting.add_pass(lights, renderer, None, &self.gbuffer, graph)?;
        self.post.add_pass(post_props, None, renderer, &self.gbuffer, graph)?;
        self.blit.add_pass(renderer, graph)?;

        Ok(())
    }
}

