use crate::app::RenderOpts;
use crate::render::builtin::VertexPbr;
use crate::render::deferred::bloom::{BloomPass, BloomProperties};
use crate::render::deferred::pbr_lighting::{sun_transmittance, AtmosphereProps, PbrLightingPass};
use crate::render::deferred::post::{BlitPass, PostProcessPass, PostProperties};
use crate::render::deferred::GBuffer;
use crate::render::graph::FrameGraph;
use crate::render::lights::GpuLight;
use crate::render::mesh::DrawTopology;
use crate::render::pipeline::material::{GpuMaterial, MaterialParams};
use crate::render::renderer::Renderer;
use crate::render::shadows::ShadowMap;
use crate::render::voxels::naive::NaiveBoxes;
use crate::render::voxels::VoxelWorld;
use crate::scenes::Scene;
use crate::time::Time;
use crate::ui::ShowUi;
use egui::{Slider, Ui};
use glam::{IVec3, Vec3};
use std::f32::consts::PI;
use windows::Win32::Graphics::Direct3D12::{ID3D12Resource, D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE};
use crate::render::texture::{MipCount, Texture};

#[derive(Debug)]
pub struct VoxelsScene {
    gbuffer: GBuffer,
    world: VoxelWorld,
    materials_buf: ID3D12Resource,
    voxel_textures: Vec<Texture>,

    sun_t: f32,
    atmosphere: AtmosphereProps,
    shadows: ShadowMap,
    bloom: BloomPass,
    lighting: PbrLightingPass,
    post: PostProcessPass,
    blit: BlitPass
}

impl Scene for VoxelsScene {
    const NAME: &'static str = "Voxel world";

    fn ui(&mut self, ui: &mut Ui) -> anyhow::Result<()> {
        self.atmosphere.show_ui(ui);
        ui.horizontal(|ui| {
            ui.label("Sun");
            ui.add(Slider::new(&mut self.sun_t, -PI..=PI).step_by(0.1))
        });
        Ok(())
    }

    fn setup(renderer: &mut Renderer) -> anyhow::Result<Self> {
        let gbuffer = renderer.build_gbuffer::<VertexPbr>("deferred/GPassPBR.hlsl", DrawTopology::TriangleList)?;

        let mut lighting = PbrLightingPass::new(renderer,
                                                "assets/textures/sky/sky.hdr",
                                                "assets/textures/sky/sky_irradiance.dds",
                                                "assets/textures/sky/sky_specular.dds",
                                                "assets/textures/pbr/ibl_brdf_lut.dds"
        )?;
        let shadows = ShadowMap::new(renderer, 1024, 2048, 512, 512)?;
        lighting.save_local_shadows(&shadows);
        let bloom = BloomPass::new(renderer)?;
        let post = PostProcessPass::new(renderer)?;

        let mut batch = renderer.gd.begin_upload()?;
        let mut materials: Vec<GpuMaterial> = vec![];
        let mut voxel_textures: Vec<Texture> = vec![];
        for (tex, metal, rough) in [("dirt", 0.2, 0.7), ("diamond_block", 0.7, 0.6)] {
            // todo: move to atlases
            let diffuse = Texture::load(&mut batch, &std::fs::read(format!("assets/textures/voxels/{tex}/{tex}_diffuse.png"))?, true, MipCount::All, &format!("{tex}_diffuse"))?;
            let normal = Texture::load(&mut batch, &std::fs::read(format!("assets/textures/voxels/{tex}/{tex}_normal.png"))?, true, MipCount::All, &format!("{tex}_normal"))?;
            let metallic = Texture::load(&mut batch, &std::fs::read(format!("assets/textures/voxels/{tex}/{tex}_metallic.png"))?, true, MipCount::All, &format!("{tex}_metal"))?;
            let ao = Texture::load(&mut batch, &std::fs::read(format!("assets/textures/voxels/{tex}/{tex}_ao.png"))?, true, MipCount::All, &format!("{tex}_ao"))?;
            materials.push(MaterialParams::Voxel {
                albedo_tex: diffuse.srv_ptr(),
                normal_tex: normal.srv_ptr(),
                metal_tex: metallic.srv_ptr(),
                occlusion_tex: ao.srv_ptr(),
                metallic: metal,
                roughness: rough,
            }.to_gpu_material());
            voxel_textures.extend([diffuse, normal, metallic, ao]);
        }
        let materials_buf = renderer.gd.create_default_buffer(&mut batch, bytemuck::cast_slice(&materials), D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE, "MaterialBuffer")?;
        batch.finish()?;

        let blit = BlitPass::new(post.output_tex(), false, renderer)?;

        let mut world = VoxelWorld::new(Box::new(NaiveBoxes), renderer, vec![0; 8], unsafe { materials_buf.GetGPUVirtualAddress() })?;

        world.build_chunk(IVec3::ZERO, &renderer.gd)?;

        Ok(Self {
            gbuffer,
            world,
            materials_buf,
            voxel_textures,

            blit,
            atmosphere: AtmosphereProps {
                beta_mie: 0.05,
                h_mie: 0.5,
                mie_g: 0.8,
                ..Default::default()
            },

            sun_t: -2.5416,
            shadows,
            bloom,
            lighting,
            post,
        })
    }

    fn build_graph<'a>(&'a mut self, renderer: &'a Renderer, time: &Time, graph: &mut FrameGraph<'a>, opts: &mut RenderOpts) -> anyhow::Result<()> {
        let sun_dir = Vec3::new(self.sun_t.cos(), self.sun_t.sin(), 0.);
        let sun_color = sun_transmittance(&self.atmosphere, renderer.cam.pos().y, -sun_dir);
        let lights = vec![GpuLight::directional(sun_dir, sun_color, 12.0)];

        let gr = &renderer.gr;

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
        self.world.select_and_add_pass(&renderer.cam.frustum(), renderer.cam.pos(), renderer, &self.gbuffer, opts, graph)?;

        // shadowws (global and local)
        // let mut all_objects: Vec<ObjectData> = Vec::new();
        // let mut all_draws: Vec<&Mesh> = Vec::new();
        // let lambda = 0.6;
        // self.shadows.update(lights[0], lambda, renderer);
        // let shadow = self.shadows.add_global_shadows_pass(lambda, renderer, graph, &all_objects, &all_draws)?;
        // self.terrain.add_shadow_passes(renderer, self.shadows.dynamic_cascades(), graph)?;

        let (_lights_va, _light_count) = self.lighting.add_pass(lights, renderer, None, Some(self.atmosphere), 0.19, &self.gbuffer, graph, opts)?;

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

        Ok(())
    }
}
