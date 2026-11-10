use std::f32::consts::PI;
use std::fs::File;
use std::io::{Cursor, Write};
use crate::app::RenderOpts;
use crate::assets::{load_assimp_scene, LoadTex, LoadedScene, LodCount};
use crate::render::builtin::VertexPbr;
use crate::render::deferred::bloom::{BloomPass, BloomProperties};
use crate::render::deferred::fog::{FogPass, FogProperties};
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
use crate::render::shadows::{ShadowMap, CASCADE_COUNT, SHADOW_DISTANCE, SHADOW_EXTEND};
use crate::scenes::Scene;
use crate::time::Time;
use egui::{ComboBox, Ui};
use egui_probe::{EguiProbe, Probe};
use glam::{Mat4, Quat, Vec3};
use windows::Win32::Graphics::Direct3D12::{ID3D12Resource, D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE};
use crate::render::aa::AntiAliasing;
use crate::render::aa::fsr2::Fsr2Pass;
use crate::render::aa::temporal::TaaPass;
use crate::render::deferred::sky_velocity::SkyVelocityPass;
use crate::render::raytracing::gi::{GpuVpl, RtGiBakeryPass, MAX_VPL_LEVELS};
use crate::render::raytracing::RtShadowsPass;

const MAX_PARTICLES: usize = 4096;

#[derive(Debug)]
pub struct SponzaWithLights {
    gbuffer: GBuffer,

    shadow_mode: ShadowMode,
    sponza: LoadedScene,
    sphere: Mesh,
    sphere_mat_idx: u32,

    materials_buf: ID3D12Resource,
    sky_velocity: SkyVelocityPass,
    taa: TaaPass,
    fsr: Fsr2Pass,
    rt_shadows: RtShadowsPass,
    rt_gi_bakery: RtGiBakeryPass,

    shadows: ShadowMap,
    culler: Culler,
    bloom: BloomPass,
    lighting: PbrLightingPass,
    post: PostProcessPass,
    blit: BlitPass,
    debug_lines: Vec<String>,
    should_rebake: bool,
    should_dump_gi: bool,
    pending_gi_read: bool,
    baked_vpls: Vec<GpuVpl>,
    vpl_settings: VplSettings
}

impl Scene for SponzaWithLights {
    const NAME: &'static str = "Sponza (w/ lights)";

    fn ui(&mut self, ui: &mut Ui) -> anyhow::Result<()> {
        ui.horizontal(|ui| {
            ComboBox::from_label("Shadows mode")
                .selected_text(format!("{:?}", self.shadow_mode))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.shadow_mode, ShadowMode::Cascades, "Cascades");
                    ui.selectable_value(&mut self.shadow_mode, ShadowMode::Raytraced, "Raytraced");
                });
        });

        ui.spacing();

        ui.heading("Cascades");
        for line in std::mem::take(&mut self.debug_lines) {
            ui.label(line);
        }

        if ui.button("Rebake GI").clicked() {
            self.should_rebake = true;
        }

        if ui.button("Dump GI").clicked() {
            self.should_dump_gi = true;
        }

        if ui.button("Load GI Dump").clicked() {
            self.baked_vpls = postcard::from_bytes(&std::fs::read("dumps/baked_vpl.dump")?)?;
        }

        Probe::new(&mut self.vpl_settings).with_header("VPLs").show(ui);
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
        let sphere = Mesh::new(batch.gd(), &mut batch, &sphere.0, &sphere.1, DrawTopology::TriangleList, AABB { min: Vec3::ZERO, max: Vec3::ONE }, false, true)?;
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
        let shadows = ShadowMap::new(renderer, 1024, 2048, 512, 512, SHADOW_DISTANCE, SHADOW_EXTEND)?;
        lighting.save_local_shadows(&shadows);
        let taa = TaaPass::new(renderer)?;
        let fsr = Fsr2Pass::new(renderer)?;

        let sky_velocity = SkyVelocityPass::new(renderer)?;
        let bloom = BloomPass::new(renderer)?;
        let post = PostProcessPass::new(renderer)?;
        let blit = BlitPass::new(post.output_tex(), false, renderer)?;
        let rt_shadows = RtShadowsPass::new(1, 0.27, renderer)?;
        let rt_gi_bakery = RtGiBakeryPass::new(8192, renderer)?;

        let (objects, meshes) = sponza.to_objects_and_draws();
        let culler = Culler::new(&objects, &meshes);

        Ok(Self {
            gbuffer,
            blit,

            sphere,
            sphere_mat_idx,
            taa,
            fsr,
            sky_velocity,

            shadows,
            rt_shadows,
            rt_gi_bakery,
            culler,
            bloom,
            sponza,
            materials_buf,
            lighting,
            post,
            debug_lines: vec![],
            shadow_mode: ShadowMode::Cascades,
            should_rebake: false,
            should_dump_gi: false,
            pending_gi_read: false,
            baked_vpls: vec![],
            vpl_settings: VplSettings {
                k: [2., 3., 4., 16., 32.],
                enabled: false,
                threshold: 0.01,
                ibl_strength: 0.03,
                sun_target_x: 0.,
                sun_target_y: 0.,
                sun_target_z: 0.,
                sun_radius: 15.0,
            }
        })
    }

    fn build_graph<'a>(&'a mut self, renderer: &'a Renderer, time: &Time, graph: &mut FrameGraph<'a>, opts: &mut RenderOpts) -> anyhow::Result<()> {
        // dumping GI if needed
        if self.pending_gi_read {
            self.pending_gi_read = false;
            let vpls = self.rt_gi_bakery.read(&renderer.gd)?;
            self.baked_vpls = vpls.clone();
            std::fs::create_dir_all("dumps")?;
            let bytes = postcard::to_allocvec(&vpls)?;
            std::fs::write("dumps/baked_vpl.dump", &bytes)?;
        }

        if self.should_dump_gi {
            self.should_dump_gi = false;
            self.rt_gi_bakery.add_copy_pass(graph);
            self.pending_gi_read = true;
        }

        let mut objects: Vec<ObjectData> = Vec::new();
        let mut draws: Vec<(&Mesh, PipelineId)> = Vec::new();
        let pass = if opts.use_wireframe { self.gbuffer.gpass_wireframe } else { self.gbuffer.gpass };

        // sphere
        let t = time.game_time();
        let prev_t = time.game_time() - time.delta_time();
        let sphere_obj = ObjectData {
            model: Mat4::from_scale_rotation_translation(0.5 * Vec3::ONE, Quat::IDENTITY, Vec3::new(-2., 2. + t.sin(), -1.)),
            prev_model: Mat4::from_scale_rotation_translation(0.5 * Vec3::ONE, Quat::IDENTITY, Vec3::new(-2., 2. + prev_t.sin(), -1.)),
            material_id: self.sphere_mat_idx,
            vb_srv: self.sphere.vb_srv(),
            ib_srv: self.sphere.ib_srv(),
            stride: self.sphere.stride(),
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
            GpuLight::spot(Vec3::new(4., 5., 0.), Quat::from_rotation_y(PI) * Vec3::Z, 15., 15., 25.,  Vec3::new(1.0, 0.4, 0.2), 100.),
            GpuLight::spot(Vec3::new(4., 2., 0.), Quat::from_rotation_y(-PI) * Vec3::Z, 15., 15., 25.,  Vec3::new(1.0, 0.4, 0.2), 100.),
        ];

        if self.vpl_settings.enabled {
            let s = &self.vpl_settings;
            lights.extend(self.baked_vpls.iter().map(|v| GpuLight::vpl(*v, s.k[v.level as usize], s.threshold)));
        }

        let gr = &renderer.gr;

        let rs = renderer.rs();
        let pipelines = renderer.pipelines();

        // scene draw pass
        graph
            .add_pass("GPass")
            .writes_depth(self.gbuffer.scene_depth, ResLoad::Clear)
            .writes_gbuffer(&self.gbuffer, ResLoad::Clear)
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

        // sky velocity
        self.sky_velocity.add_pass(&self.gbuffer, renderer, graph)?;

        let (mut all_objects, mut all_meshes) = self.sponza.to_objects_and_draws();
        all_objects.push(sphere_obj);
        all_meshes.push(&self.sphere);

        // rebaking gi if needed
        if self.should_rebake {
            self.should_rebake = false;
            self.rt_shadows.tlas().add_build_pass(&all_objects, &all_meshes, renderer, graph)?;
            let all_objects_va = renderer.upload_objects(&all_objects)?;
            self.rt_gi_bakery.add_bake_pass(renderer, Vec3::new(self.vpl_settings.sun_target_x, self.vpl_settings.sun_target_y, self.vpl_settings.sun_target_z), self.vpl_settings.sun_radius, all_objects_va, materials_va, self.rt_shadows.tlas().tlas_va(), &lights, graph)?;
        }

        if self.shadow_mode == ShadowMode::Cascades {
            // shadowws (global and local)
            let all_objects_va = renderer.upload_objects(&all_objects)?;
            let lambda = 0.6;
            self.shadows.update(lights[0], lambda, renderer);
            let shadow = self.shadows.add_global_shadows_pass(lambda, renderer, graph, &all_objects, &all_meshes)?;
            self.shadows.add_local_shadows_pass(&mut lights, renderer, all_objects_va, &all_objects, &all_meshes, graph)?;

            let (_lights_va, _light_count) = self.lighting.add_pass(None, lights, renderer, Some(shadow), None, None,self.vpl_settings.ibl_strength, &self.gbuffer, graph, opts)?;

            // debug
            for i in 0..CASCADE_COUNT {
                self.debug_lines.push(format!("cascade {i}: <= {:.1}m | {} drawn", shadow.splits[i], shadow.drawn[i]))
            }
        } else if self.shadow_mode == ShadowMode::Raytraced {
            self.rt_shadows.add_pass(&all_objects, &all_meshes, &lights, &self.gbuffer, renderer, graph)?;
            let (_lights_va, _light_count) = self.lighting.add_pass(None, lights, renderer, None, Some(&self.rt_shadows), None, self.vpl_settings.ibl_strength, &self.gbuffer, graph, opts)?;
        }

        let scene_color = match opts.anti_aliasing {
            AntiAliasing::Disabled => self.gbuffer.scene_color,
            AntiAliasing::Temporal => self.taa.add_pass(&self.gbuffer, renderer, graph)?,
            AntiAliasing::Fsr2 => self.fsr.add_pass(&self.gbuffer, renderer, time, graph)?
        };

        // bloom
        let bloom = self.bloom.add_pass(BloomProperties {
            threshold: 0.5,
            knee: 0.25,
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
        self.blit.add_pass(renderer, graph)?;

        Ok(())
    }

    fn update<'a>(&'a mut self, renderer: &'a mut Renderer, time: &Time) -> anyhow::Result<()> {
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