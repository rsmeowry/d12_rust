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
use crate::render::particles::draw::{MeshParticleMaterial, ParticleKind, ParticleRendering, ParticleShape};
use crate::render::particles::simulate::{ParticleSimulation, SimulateParticleProps};
use crate::render::perf::frustum::AABB;
use crate::render::perf::{Culler, NO_LOD};
use crate::render::pipeline::material::{GpuMaterial, MaterialParams};
use crate::render::pipeline::PipelineId;
use crate::render::renderer::{ObjectData, Renderer};
use crate::render::shadows::{ShadowMap, CASCADE_COUNT};
use crate::scenes::Scene;
use crate::time::Time;
use egui::Ui;
use glam::{Mat4, Quat, Vec3};
use windows::Win32::Graphics::Direct3D12::{ID3D12Resource, D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE};
use crate::render::terrain::Terrain;

#[derive(Debug)]
pub struct TerrainScene {
    gbuffer: GBuffer,
    terrain: Terrain,

    shadows: ShadowMap,
    bloom: BloomPass,
    lighting: PbrLightingPass,
    post: PostProcessPass,
    blit: BlitPass
}

impl Scene for TerrainScene {
    const NAME: &'static str = "Terrain";

    fn ui(&mut self, ui: &mut Ui) -> anyhow::Result<()> {
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
        let blit = BlitPass::new(post.output_tex(), false, renderer)?;

        let terrain = Terrain::new(renderer, "assets/textures/terrain/terrain", 16, 0.8, 120.)?;

        Ok(Self {
            gbuffer,
            terrain,
            blit,

            shadows,
            bloom,
            lighting,
            post,
        })
    }

    fn build_graph<'a>(&'a mut self, renderer: &'a Renderer, time: &Time, graph: &mut FrameGraph<'a>, opts: &mut RenderOpts) -> anyhow::Result<()> {
        let _t = time.game_time();
        let mut lights = vec![
            GpuLight::directional(Vec3::new(-0.95, -0.4, 0.), Vec3::new(1., 0.95, 0.9), 12.0),
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
        self.terrain.add_pass(renderer, &self.gbuffer, opts, graph)?;

        // shadowws (global and local)
        let mut all_objects: Vec<ObjectData> = Vec::new();
        let mut all_draws: Vec<&Mesh> = Vec::new();
        let lambda = 0.6;
        self.shadows.update(lights[0], lambda, renderer);
        let shadow = self.shadows.add_global_shadows_pass(lambda, renderer, graph, &all_objects, &all_draws)?;
        self.terrain.add_shadow_passes(renderer, self.shadows.dynamic_cascades(), graph)?;

        let (_lights_va, _light_count) = self.lighting.add_pass(lights, renderer, Some(shadow), 0.19, true, Vec3::new(5.8, 13.5, 33.1) * 1e-3, &self.gbuffer, graph, opts)?;

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
