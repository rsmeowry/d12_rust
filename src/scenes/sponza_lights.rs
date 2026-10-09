use glam::{Vec3, Vec4};
use windows::Win32::Graphics::Direct3D12::{ID3D12Resource, D3D12_CULL_MODE_NONE, D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE};
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use crate::assets::{load_assimp_scene, LoadedScene};
use crate::render::builtin::Vertex3;
use crate::render::deferred::{BlitProperties, DeferredLightingPass, GBuffer};
use crate::render::graph::FrameGraph;
use crate::render::graph::pass::ResLoad;
use crate::render::graph::res::{ResId, ResSize};
use crate::render::lights::GpuLight;
use crate::render::mesh::Mesh;
use crate::render::pipeline::desc::PipelineDesc;
use crate::render::pipeline::material::MaterialParams;
use crate::render::pipeline::PipelineId;
use crate::render::renderer::{ObjectData, Renderer, SCENE_FORMAT};
use crate::scenes::Scene;
use crate::time::Time;
use crate::wrappers::graphics::GraphicsDevice;

#[derive(Debug)]
pub struct SponzaWithLights {
    gbuffer: GBuffer,

    post: PipelineId,
    blit: PipelineId,

    post_intermediate: ResId,

    sponza: LoadedScene,

    materials_buf: ID3D12Resource,

    lighting: DeferredLightingPass
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
        let blit = renderer.create_pipeline(PipelineDesc {
            rts: vec![GraphicsDevice::BB_RTV_FORMAT],
            name: "Blit",
            shader: "Blit.hlsl".into(),
            input: &[],
            depth: None,
            cull: D3D12_CULL_MODE_NONE,
            ..Default::default()
        })?;
        let post = renderer.create_pipeline(PipelineDesc {
            rts: vec![SCENE_FORMAT],
            name: "Post",
            shader: "Post.hlsl".into(),
            input: &[],
            depth: None,
            cull: D3D12_CULL_MODE_NONE,
            ..Default::default()
        })?;

        let post_intermediate = renderer.gr.color(&renderer.gd, "PostOutput", SCENE_FORMAT, Vec4::ZERO, ResSize::FitScreen)?;

        Ok(Self {
            gbuffer,
            blit,
            post,
            post_intermediate,

            sponza,
            materials_buf,
            lighting,
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

        let blit_va = renderer.upload_slice(bytemuck::bytes_of(&BlitProperties {
            source_tex: gr.get(self.post_intermediate)?.srv_ptr(),
            dither_tex: gr.get(self.post_intermediate)?.srv_ptr(),
            _pad: [0; 18],
        }), 16)?;

        let post_va = renderer.upload_slice(bytemuck::bytes_of(&PostProperties {
            albedo: gr.get(self.gbuffer.albedo)?.srv_ptr(),
            normals: gr.get(self.gbuffer.normals)?.srv_ptr(),
            materials: gr.get(self.gbuffer.materials)?.srv_ptr(),
            depth: gr.get(self.gbuffer.scene_depth)?.srv_ptr(),
            scene_color: gr.get(self.gbuffer.scene_color)?.srv_ptr(),
            enable_sobel: 1,
            enable_vignette: 1,
            vignette_strength: 0.5,
            enable_posterization: 1,
            posterization_levels: 32,
            _pad: [0.; 10],
        }), 16)?;

        let rs = renderer.rs();
        let pipelines = renderer.pipelines();

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

        graph.add_pass("Post")
            .reads(self.gbuffer.albedo)
            .reads(self.gbuffer.normals)
            .reads(self.gbuffer.materials)
            .reads(self.gbuffer.scene_color)
            .reads(self.gbuffer.scene_depth)
            .writes(self.post_intermediate, ResLoad::Clear)
            .exec(move |ctx| {
                pipelines.get(self.post).bind(ctx.cmd);
                rs.set_objects(ctx.cmd, post_va);
                rs.set_draw_index(ctx.cmd, 0);
                unsafe {
                    ctx.cmd.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                    ctx.cmd.DrawInstanced(3, 1, 0, 0);
                }
                Ok(())
            });

        // blit+present pass
        graph.add_pass("Blit")
            .reads(self.post_intermediate)
            .uses_backbuffer()
            .exec(move |ctx| {
                pipelines.get(self.blit).bind(ctx.cmd);
                rs.set_objects(ctx.cmd, blit_va);
                rs.set_draw_index(ctx.cmd, 0);
                unsafe {
                    ctx.cmd.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                    ctx.cmd.DrawInstanced(3, 1, 0, 0);
                }
                Ok(())
            });

        Ok(())
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct PostProperties {
    pub albedo: u32,
    pub normals: u32,
    pub materials: u32,
    pub depth: u32,
    pub scene_color: u32,
    pub enable_sobel: u32,
    pub enable_vignette: u32,
    pub vignette_strength: f32,
    pub enable_posterization: u32,
    pub posterization_levels: u32,
    pub _pad: [f32; 10]
}