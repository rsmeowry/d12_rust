use crate::app::RenderOpts;
use crate::assets::{load_assimp_scene, LoadTex, LoadedScene, LodCount};
use crate::render::aa::temporal::TaaPass;
use crate::render::builtin::VertexPbr;
use crate::render::deferred::bloom::{BloomPass, BloomProperties};
use crate::render::deferred::pbr_lighting::PbrLightingPass;
use crate::render::deferred::post::{Blitter, PostProcessPass, PostProperties};
use crate::render::deferred::sky_velocity::SkyVelocityPass;
use crate::render::deferred::GBuffer;
use crate::render::graph::pass::ResLoad;
use crate::render::graph::FrameGraph;
use crate::render::lights::GpuLight;
use crate::render::mesh::{DrawTopology, Mesh};
use crate::render::pipeline::material::MaterialParams;
use crate::render::pipeline::PipelineId;
use crate::render::raytracing::gi::MAX_VPL_LEVELS;
use crate::render::renderer::{ObjectData, Renderer};
use crate::render::shadows::{ShadowMap, SHADOW_DISTANCE, SHADOW_EXTEND};
use crate::scenes::Scene;
use crate::time::Time;
use egui::Ui;
use egui_probe::EguiProbe;
use glam::Vec3;
use windows::Win32::Graphics::Direct3D12::{ID3D12Resource, D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE};
use crate::input::InputDevice;
use crate::physics::{PhysParams, PhysicsSystem};
use crate::physics::body::meshing::{SoftBodyRenderer, POINT_COUNT};
use crate::physics::body::{BodyParams, SoftBody};
use crate::physics::contact::Plane;
use crate::render::deferred::transparent::TransparentPass;

#[derive(Debug)]
pub struct JellyScene {
    gbuffer: GBuffer,
    empty_pso: PipelineId,
    scene: LoadedScene,

    materials_buf: ID3D12Resource,
    sky_velocity: SkyVelocityPass,
    taa: TaaPass,

    phys_params: PhysParams,
    physics: PhysicsSystem,
    sb_renderer: SoftBodyRenderer,

    shadows: ShadowMap,
    bloom: BloomPass,
    lighting: PbrLightingPass,
    post: PostProcessPass,
    blit: Blitter,
}

impl Scene for JellyScene {
    fn ui(&mut self, ui: &mut Ui) -> anyhow::Result<()> {
        if ui.button("Lock bottom face").clicked() {
            let body = self.physics.get_body(0).expect("oops");
            let p_count = body.particles.len() as f32;
            for idx in body.face_indices(1, 0) {
                let particle = body.particles.get_mut(idx).unwrap();
                if particle.mass.is_infinite() {
                    particle.mass = body.initial_mass / p_count;
                } else {
                    particle.mass = f32::INFINITY;
                }
            }
        }

        Ok(())
    }

    fn setup(renderer: &mut Renderer) -> anyhow::Result<Self> {
        let mut phys = PhysicsSystem::new(500.); // 500hz
        let gravity = Vec3::new(0., -9.81, 0.);
        let params = PhysParams {
            gravity: gravity.into(),
            restitution: 0.1,
            k_friction: 0.6,
            rest_speed: 3.0 * gravity.length() * phys.fixed_dt(),
            air_drag: 0.02
        };
        let body = SoftBody::new_cube(BodyParams {
            mass: 1.5,
            size: 3.0,
            ppa: POINT_COUNT,
            k_struct: 150.,
            k_shear: 75.,
            damping: 0.5,
        }, Vec3::new(0., 19., 0.));

        phys.add_static_collider(Box::new(Plane {
            point: Vec3::new(0., 0.3, 0.),
            normal: Vec3::Y,
        }));
        phys.add_body(body);

        let transparent_pass = TransparentPass::new::<VertexPbr, &str>(renderer, "JellyFFD", "deferred/jelly/JellyFFD.hlsl", "deferred/jelly/JellyFFDPrepass.hlsl")?;
        let blit = Blitter::new(renderer)?;

        let mut batch = renderer.gd.begin_upload()?;
        let mut materials = Vec::new();

        let sb_renderer = SoftBodyRenderer::new(Some("assets/models/jelly_scene/jelly_frog.gltf"), Vec3::new(0., 0., 0.), 0, MaterialParams::Jelly {
            density: 32.,
            refract_strength: 0.4,
            is_chroma: false,
            absorption: Vec3::new(0.3, 1.4, 0.8),
            jelly_color_a: Vec3::new(0.9, 0.4, 0.6),
            jelly_color_b: Vec3::new(0.9, 0.6, 0.2),
            spec_color: Vec3::new(0.8, 0.4, 0.1),
        }, &mut materials, &mut batch, renderer, blit.clone(), transparent_pass)?;

        let scene = load_assimp_scene::<VertexPbr, _>(
            &renderer, &mut batch, "assets/models/jelly_scene/jelly_scene.gltf",
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
            }.to_gpu_material(),
            None,
            Some("bakes/jelly_scene.bke".into())
        )?;

        let materials_buf = renderer.gd.create_default_buffer(&mut batch, bytemuck::cast_slice(&materials), D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE, "Sponza/MaterialBuffer")?;

        batch.finish()?;

        let gbuffer = renderer.build_gbuffer::<VertexPbr>("deferred/GPassPBR.hlsl", DrawTopology::TriangleList)?;
        let empty_pso = renderer.build_gpass::<VertexPbr>("deferred/GPassEmpty.hlsl", DrawTopology::TriangleList)?.0;

        let mut lighting = PbrLightingPass::new(renderer,
                                                "assets/textures/sky/sky.hdr",
                                                "assets/textures/sky/sky_irradiance.dds",
                                                "assets/textures/sky/sky_specular.dds",
                                                "assets/textures/pbr/ibl_brdf_lut.dds"
        )?;
        let shadows = ShadowMap::new(renderer, 1024, 4096, 512, 512, SHADOW_DISTANCE, SHADOW_EXTEND)?;
        lighting.save_local_shadows(&shadows);
        let taa = TaaPass::new(renderer)?;
        let _jelly_pass = TransparentPass::new::<VertexPbr, &str>(renderer, "Jelly", "deferred/Jelly.hlsl", "deferred/JellyPrepass.hlsl")?;

        let sky_velocity = SkyVelocityPass::new(renderer)?;
        let bloom = BloomPass::new(renderer)?;
        let post = PostProcessPass::new(renderer)?;

        Ok(Self {
            gbuffer,
            blit,
            empty_pso,

            physics: phys,
            phys_params: params,
            sb_renderer,

            taa,
            sky_velocity,
            scene,

            shadows,
            bloom,
            materials_buf,
            lighting,
            post,
        })
    }

    fn build_graph<'a>(&'a mut self, renderer: &'a Renderer, time: &Time, graph: &mut FrameGraph<'a>, opts: &mut RenderOpts) -> anyhow::Result<()> {
        let mut objects: Vec<ObjectData> = Vec::new();
        let mut draws: Vec<(&Mesh, PipelineId)> = Vec::new();
        let pass = if opts.use_wireframe { self.gbuffer.gpass_wireframe } else { self.gbuffer.gpass };

        // sponza
        let _frustum = renderer.cam.frustum();
        let (scene_obj, scene_draw) = self.scene.to_objects_and_draws();
        objects.extend_from_slice(&scene_obj);
        draws.extend(scene_draw.clone().into_iter().zip(std::iter::repeat(pass)));

        let objects_va = renderer.upload_objects(&objects)?;
        let materials_va = unsafe { self.materials_buf.GetGPUVirtualAddress() };

        let _t = time.game_time();
        let mut lights = vec![
            GpuLight::directional(Vec3::new(-0.3, -0.7, -0.15), Vec3::new(1., 0.95, 0.9), 5.0),
        ];

        let gr = &renderer.gr;

        let rs = renderer.rs();
        let pipelines = renderer.pipelines();

        // scene draw pass
        let empty_pass_va = renderer.upload_cbuffer(&0u32)?;
        graph
            .add_pass("GPass")
            .writes_depth(self.gbuffer.scene_depth, ResLoad::Clear)
            .writes_gbuffer(&self.gbuffer, ResLoad::Clear)
            .exec(move |ctx| {
                rs.set_objects(ctx.cmd, objects_va);
                rs.set_materials(ctx.cmd, materials_va);
                rs.set_pass(ctx.cmd, empty_pass_va);
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
        // sky velocity
        self.sky_velocity.add_pass(&self.gbuffer, renderer, graph)?;

        // shadowws (global and local)
        let all_objects_va = renderer.upload_objects(&scene_obj)?;
        let lambda = 0.6;
        let main_light = lights[0];
        self.shadows.update(lights[0], lambda, renderer);
        let shadow = self.shadows.add_global_shadows_pass(lambda, renderer, graph, &scene_obj, &scene_draw)?;
        self.shadows.add_local_shadows_pass(&mut lights, renderer, all_objects_va, &scene_obj, &scene_draw, graph)?;

        let (_lights_va, _light_count) = self.lighting.add_pass(None, lights, renderer, Some(shadow), None, None, 0.12, &self.gbuffer, graph, opts)?;

        let jelly_tex = self.sb_renderer.add_pass(self.lighting.irradiance_tex(), &main_light, &self.physics, renderer, &self.gbuffer, graph)?;

        let scene_color = self.taa.add_pass(jelly_tex, &self.gbuffer, renderer, graph)?;

        // bloom
        let bloom = self.bloom.add_pass(BloomProperties {
            threshold: 1.0,
            knee: 0.12,
            radius: 1.0,
            intensity: 1.0,
            ..Default::default()
        }, scene_color, renderer, graph)?;

        // post
        let post_props = PostProperties {
            albedo: gr.get(self.gbuffer.albedo)?.srv_ptr(),
            normals: gr.get(self.gbuffer.normals)?.srv_ptr(),
            materials: gr.get(self.gbuffer.materials)?.srv_ptr(),
            depth: gr.get(self.gbuffer.scene_depth)?.srv_ptr(),
            scene_color: gr.get(scene_color)?.srv_ptr(),
            enable_sobel: 0,
            enable_fly_eye: 0,
            enable_vignette: 1,
            ..Default::default()
        };
        self.post.add_pass(post_props, Some(scene_color), Some(bloom), renderer, &self.gbuffer, graph)?;
        self.blit.blit_to_screen(false, self.post.target_tex(), renderer, graph)?;

        Ok(())
    }

    fn update<'a>(&'a mut self, renderer: &'a mut Renderer, time: &Time, input: &InputDevice) -> anyhow::Result<()> {
        self.physics.update(&self.phys_params, time, &renderer.cam, input);
        Ok(())
    }
}

#[derive(Debug, Copy, Clone, EguiProbe, PartialEq)]
pub enum ShadowMode {
    Cascades,
    Raytraced
}

#[derive(Debug, EguiProbe)]
struct VplSettings {
    pub k: [f32; MAX_VPL_LEVELS as usize],
    pub threshold: f32,
    pub enabled: bool,
    pub ibl_strength: f32,
    pub sun_target_x: f32,
    pub sun_target_y: f32,
    pub sun_target_z: f32,
    pub sun_radius: f32,
}