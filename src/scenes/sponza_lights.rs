use crate::app::RenderOpts;
use crate::assets::{load_assimp_scene, LoadTex, LoadedScene, LodCount};
use crate::render::builtin::VertexPbr;
use crate::render::deferred::bloom::{BloomPass, BloomProperties};
use crate::render::deferred::fog::FogPass;
use crate::render::deferred::pbr_lighting::PbrLightingPass;
use crate::render::deferred::post::{BlitPass, PostProcessPass, PostProperties};
use crate::render::deferred::GBuffer;
use crate::render::graph::pass::ResLoad;
use crate::render::graph::FrameGraph;
use crate::render::lights::{unit_sphere, GpuLight};
use crate::render::mesh::{DrawTopology, Mesh};
use crate::render::perf::frustum::AABB;
use crate::render::perf::{Culler, NO_LOD};
use crate::render::pipeline::material::MaterialParams;
use crate::render::pipeline::PipelineId;
use crate::render::renderer::{ObjectData, Renderer};
use crate::render::shadows::{ShadowMap, CASCADE_COUNT};
use crate::scenes::Scene;
use crate::time::Time;
use egui::Ui;
use glam::{Mat4, Quat, Vec3};
use windows::Win32::Graphics::Direct3D12::{ID3D12Resource, D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE};
use crate::render::particles::simulate::{ComputeParticlesPass, ParticleSimulation, SimulateParticleProps};

#[derive(Debug)]
pub struct SponzaWithLights {
    gbuffer: GBuffer,

    sponza: LoadedScene,
    sphere: Mesh,
    sphere_mat_idx: u32,

    materials_buf: ID3D12Resource,

    particles: ParticleSimulation,

    shadows: ShadowMap,
    culler: Culler,
    bloom: BloomPass,
    fog: FogPass,
    lighting: PbrLightingPass,
    post: PostProcessPass,
    blit: BlitPass,
    debug_lines: Vec<String>,
}

impl Scene for SponzaWithLights {
    const NAME: &'static str = "Sponza (w/ lights)";

    fn ui(&mut self, ui: &mut Ui) -> anyhow::Result<()> {
        for line in std::mem::take(&mut self.debug_lines) {
            ui.label(line);
        }
        Ok(())
    }

    fn setup(renderer: &mut Renderer) -> anyhow::Result<Self> {
        let mut batch = renderer.gd.begin_upload()?;

        let mut materials = Vec::new();
        let sponza = load_assimp_scene::<VertexPbr, _>(
            &renderer, &mut batch, "assets/sponza_old/Sponza.gltf",
            &mut materials,
            LoadTex::ALBEDO | LoadTex::NORMAL | LoadTex::ROUGHNESS | LoadTex::LIGHTMAP, DrawTopology::TriangleList,
            true, LodCount::NoLods,
            |mat, tex| MaterialParams::Pbr {
                albedo_tex: tex.albedo,
                normal_tex: tex.normal,
                metal_roughness_tex: tex.roughness,
                occlusion_tex: tex.lightmap,
                base_color: mat.base_color().map(|it| Vec3::new(it.x, it.y, it.z)).unwrap_or(Vec3::ONE),
                metallic: mat.metallic_factor().unwrap_or(0.5),
                roughness: mat.roughness_factor().unwrap_or(0.5),
            }.to_gpu_material()
        )?;
        let sphere = unit_sphere(32, 32, |pos, norm, tan, binorm, uv| VertexPbr::new(pos, norm, tan, binorm, uv));
        let sphere = Mesh::new(batch.gd(), &mut batch, &sphere.0, &sphere.1, DrawTopology::TriangleList, AABB { min: Vec3::ZERO, max: Vec3::ONE }, false)?;
        let sphere_mat_idx = materials.len() as u32;
        materials.push(MaterialParams::Pbr {
            albedo_tex: renderer.white_tex().srv_ptr(),
            normal_tex: renderer.flat_normal_tex().srv_ptr(),
            metal_roughness_tex: renderer.white_tex().srv_ptr(),
            occlusion_tex: renderer.white_tex().srv_ptr(),
            base_color: Vec3::new(0.8, 0.8, 0.7),
            metallic: 0.7,
            roughness: 0.3
        }.to_gpu_material());

        let materials_buf = renderer.gd.create_default_buffer(&mut batch, bytemuck::cast_slice(&materials), D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE, "Sponza/MaterialBuffer")?;

        batch.finish()?;

        let gbuffer = renderer.build_gbuffer::<VertexPbr>("deferred/GPassPBR.hlsl", DrawTopology::TriangleList)?;

        let mut lighting = PbrLightingPass::new(renderer,
                                                "assets/textures/sky/sponza/garden_sky.hdr",
                                                "assets/textures/sky/sponza/garden_sky_irradiance.dds",
                                                "assets/textures/sky/sponza/garden_sky_specular.dds",
                                                "assets/textures/pbr/ibl_brdf_lut.dds"
        )?;
        let shadows = ShadowMap::new(renderer, 1024, 2048, 512, 512)?;
        lighting.save_local_shadows(&shadows);
        let bloom = BloomPass::new(renderer)?;
        let fog = FogPass::new(Some(&shadows), renderer)?;
        let post = PostProcessPass::new(renderer)?;
        let blit = BlitPass::new(post.output_tex(), false, renderer)?;

        let particles = ParticleSimulation::new(renderer, 4096)?;

        let (objects, meshes) = sponza.to_objects_and_draws();
        let culler = Culler::new(&objects, &meshes);
        
        Ok(Self {
            gbuffer,
            blit,

            sphere,
            sphere_mat_idx,

            particles,
            shadows,
            culler,
            bloom,
            fog,
            sponza,
            materials_buf,
            lighting,
            post,
            debug_lines: vec![]
        })
    }

    fn build_graph<'a>(&'a mut self, renderer: &'a Renderer, time: &Time, graph: &mut FrameGraph<'a>, opts: &mut RenderOpts) -> anyhow::Result<()> {
        let mut objects: Vec<ObjectData> = Vec::new();
        let mut draws: Vec<(&Mesh, PipelineId)> = Vec::new();
        let pass = if opts.use_wireframe { self.gbuffer.gpass_wireframe } else { self.gbuffer.gpass };

        // sphere
        let sphere_obj = ObjectData {
            model: Mat4::from_scale_rotation_translation(0.5 * Vec3::ONE, Quat::IDENTITY, Vec3::new(-2., 2. + time.game_time().sin(), -1.)),
            material_id: self.sphere_mat_idx,
            _pad: [0; 3],
        };
        objects.push(sphere_obj);
        draws.push((&self.sphere, pass));

        // sponza
        let frustum = renderer.cam.frustum();
        for (scene, pso) in [(&self.sponza, pass)] {
            self.culler.push_visible(
                &mut objects,
                &mut draws,
                scene,
                pso,
                &frustum,
                opts,
                NO_LOD
            );
        }
        self.culler.finish(opts);

        let objects_va = renderer.upload_objects(&objects)?;
        let materials_va = unsafe { self.materials_buf.GetGPUVirtualAddress() };

        let _t = time.game_time();
        let mut lights = vec![
            GpuLight::directional(Vec3::new(-0.3, -0.7, -0.15), Vec3::new(1., 0.95, 0.9), 12.0),
            // GpuLight::point(Vec3::new(4. * t.sin(), 2., -1.2), 12., Vec3::new(1., 0.3, 0.2), 12.),
            // GpuLight::point(Vec3::new(-4. * t.sin(), 1., 1.2), 6., Vec3::new(0.2, 0.5, 1.), 8.),
            // GpuLight::point(Vec3::new(2., 2. + 2. * t.cos(), 0.), 8., Vec3::new(0.2, 0.9, 0.2), 8.),
            GpuLight::spot(Vec3::new(4., 5., 0.), Quat::from_rotation_y(time.game_time()) * Vec3::Z, 15., 15., 25.,  Vec3::new(1.0, 0.4, 0.2), 100.),
            GpuLight::spot(Vec3::new(4., 2., 0.), Quat::from_rotation_y(-time.game_time()) * Vec3::Z, 15., 15., 25.,  Vec3::new(1.0, 0.4, 0.2), 100.),
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
            enable_vignette: 1,
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

        // particles: simulate
        self.particles.add_pass(SimulateParticleProps {
            emitter: Vec3::new(0., 3., 0.),
            gravity: Default::default(),
            rate: 256.0,
            spread: 0.8,
            ..Default::default()
        }, time.delta_time(), renderer, &self.gbuffer, graph)?;

        let (mut all_objects, mut all_meshes) = self.sponza.to_objects_and_draws();
        all_objects.push(sphere_obj);
        all_meshes.push(&self.sphere);

        // shadowws (global and local)
        let all_objects_va = renderer.upload_objects(&all_objects)?;
        let lambda = 0.6;
        self.shadows.update(lights[0], lambda, renderer);
        let shadow = self.shadows.add_global_shadows_pass(lambda, renderer, graph, &all_objects, &all_meshes)?;
        self.shadows.add_local_shadows_pass(&mut lights, renderer, all_objects_va, &all_objects, &all_meshes, graph)?;

        let (_lights_va, _light_count) = self.lighting.add_pass(lights, renderer, Some(shadow), 0.19, &self.gbuffer, graph, opts)?;

        // self.fog.add_pass(FogProperties {
        //     density: 0.12,
        //     ..Default::default()
        // }, renderer, Some(shadow), lights_va, light_count, &self.gbuffer, graph)?;

        // bloom
        let bloom = self.bloom.add_pass(BloomProperties {
            threshold: 0.5,
            knee: 0.25,
            radius: 1.0,
            intensity: 1.0,
            ..Default::default()
        }, self.gbuffer.scene_color, renderer, graph)?;
        self.post.add_pass(post_props, Some(bloom), renderer, &self.gbuffer, graph)?;
        self.blit.add_pass(renderer, graph)?;

        // debug
        for i in 0..CASCADE_COUNT {
            self.debug_lines.push(format!("cascade {i}: <= {:.1}m | {} drawn", shadow.splits[i], shadow.drawn[i]))
        }

        Ok(())
    }
}
