use egui::{Atoms, Checkbox, Ui};
use crate::app::RenderOpts;
use crate::assets::{load_assimp_scene, LoadTex, LoadedScene, LodCount};
use crate::render::builtin::VertexPbr;
use crate::render::deferred::pbr_lighting::PbrLightingPass;
use crate::render::deferred::post::{BlitPass, PostProcessPass, PostProperties};
use crate::render::deferred::GBuffer;
use crate::render::graph::pass::ResLoad;
use crate::render::graph::FrameGraph;
use crate::render::lights::GpuLight;
use crate::render::mesh::DrawTopology;
use crate::render::pipeline::material::MaterialParams;
use crate::render::renderer::Renderer;
use crate::scenes::Scene;
use crate::time::Time;
use glam::{Mat4, Vec3};
use windows::Win32::Graphics::Direct3D12::{ID3D12GraphicsCommandList2, ID3D12GraphicsCommandList6, ID3D12Resource, D3D12_RANGE, D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE, D3D12_RESOURCE_STATE_COMMON, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_COPY_SOURCE, D3D12_RESOURCE_STATE_UNORDERED_ACCESS, D3D12_WRITEBUFFERIMMEDIATE_PARAMETER};
use windows_core::Interface;
use crate::render::aa::AntiAliasing;
use crate::render::aa::fsr2::Fsr2Pass;
use crate::render::deferred::sky_velocity::SkyVelocityPass;
use crate::render::raytracing::RtShadowsPass;
use crate::wrappers::graphics::{transition_barrier, GraphicsDevice};

#[derive(Debug)]
pub struct MeshletsScene {
    gbuffer: GBuffer,

    meshlet_debug: bool,
    model: LoadedScene,
    materials_buf: ID3D12Resource,
    counter: ID3D12Resource,
    readback: ID3D12Resource,
    visible_meshlets: u32,

    sky_velocity: SkyVelocityPass,
    rt_shadows_pass: RtShadowsPass,
    fsr_pass: Fsr2Pass,
    lighting: PbrLightingPass,
    post: PostProcessPass,
    blit: BlitPass,
}

impl Scene for MeshletsScene {
    fn ui(&mut self, ui: &mut Ui) -> anyhow::Result<()> {
        ui.add(Checkbox::new(&mut self.meshlet_debug, "Meshlet debug"));
        let total: u32 = self.model.meshlet_meshes.iter().map(|m| m.count()).sum();
        ui.label(format!("Visible: {}/{total}", self.visible_meshlets));
        Ok(())
    }

    fn setup(renderer: &mut Renderer) -> anyhow::Result<Self> {
        let lighting = PbrLightingPass::new(renderer,
                                            "assets/textures/pbr/warm_reception_dinner_4k.hdr",
                                            "assets/textures/pbr/ibl_irradiance.dds",
                                            "assets/textures/pbr/ibl_specular.dds",
                                            "assets/textures/pbr/ibl_brdf_lut.dds",
        )?;

        let mut batch = renderer.gd.begin_upload()?;

        let mut materials = Vec::new();
        let scene = load_assimp_scene::<VertexPbr, _>(
            &renderer, &mut batch, "assets/models/stanford_dragon/dragon_vrip.ply",
            &mut materials,
            LoadTex::ALBEDO | LoadTex::NORMAL | LoadTex::ROUGHNESS | LoadTex::LIGHTMAP, DrawTopology::TriangleList,
            true, LodCount::Meshlets,
            |mat, tex| MaterialParams::Pbr {
                albedo_tex: tex.albedo,
                normal_tex: tex.normal,
                metal_roughness_tex: tex.roughness,
                occlusion_tex: tex.lightmap,
                base_color: Vec3::new(247. / 256., 193. / 256., 56. / 256.),
                metallic: 0.7,
                roughness: 0.4,
            }.to_gpu_material(),
            Some("bakes/baked_meshlets.bke".into())
            // None,
        )?;

        let materials_buf = renderer.gd.create_default_buffer(&mut batch, bytemuck::cast_slice(&materials), D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE, "MaterialBuffer")?;

        batch.finish()?;

        let gbuffer = renderer.build_gbuffer::<VertexPbr>("mesh/GPassMeshlets.hlsl", DrawTopology::TriangleList)?;

        let post = PostProcessPass::new(renderer)?;
        let blit = BlitPass::new(post.output_tex(), false, renderer)?;
        let rt_shadows = RtShadowsPass::new(1, 0.3, renderer)?;
        let fsr = Fsr2Pass::new(renderer)?;
        let sky_velocity = SkyVelocityPass::new(renderer)?;

        let dev = renderer.gd.device();
        let counter = GraphicsDevice::create_uav_buffer(dev, 4, "MeshletCounter")?;
        let readback = GraphicsDevice::create_readback_buffer(dev, 4 * GraphicsDevice::FRAMES_IN_FLIGHT, "MeshletReadback")?;

        Ok(Self {
            gbuffer,
            blit,
            counter,
            readback,
            visible_meshlets: 0,

            meshlet_debug: false,
            model: scene,
            materials_buf,
            rt_shadows_pass: rt_shadows,
            fsr_pass: fsr,
            sky_velocity,
            lighting,
            post
        })
    }

    fn build_graph<'a>(&'a mut self, renderer: &'a Renderer, time: &Time, graph: &mut FrameGraph<'a>, opts: &mut RenderOpts) -> anyhow::Result<()> {
        // readback
        let off = renderer.gd.frame_index()? * 4;
        unsafe {
            let mut ptr = std::ptr::null_mut();
            self.readback.Map(0, Some(&D3D12_RANGE { Begin: off, End: off + 4 }), Some(&mut ptr))?;
            self.visible_meshlets = ptr.cast::<u8>().add(off).cast::<u32>().read_unaligned();
            self.readback.Unmap(0, Some(&D3D12_RANGE::default()));
        }
        let (counter, readback) = (&self.counter, &self.readback);

        let pass = if opts.use_wireframe { self.gbuffer.gpass_wireframe } else { self.gbuffer.gpass };
        let (mut objects, draws) = self.model.to_objects_and_draws_meshlets();
        let (_, all_draws) = self.model.to_objects_and_draws();

        let draws = draws.into_iter().map(|it| (it, pass)).collect::<Vec<_>>();
        for obj in &mut objects {
            obj.model = Mat4::from_scale(10. * Vec3::ONE) * obj.model;
            obj.prev_model = obj.model;
        }

        let objects_va = renderer.upload_objects(&objects)?;
        let materials_va = unsafe { self.materials_buf.GetGPUVirtualAddress() };

        let lights = vec![
            GpuLight::directional(Vec3::new(-0.4, -0.8, 0.5), Vec3::new(1., 0.9, 0.6), 12.),
            // GpuLight::point(Vec3::new(3., 3., 0.), 8., Vec3::new(0.2, 0.6, 0.3), 16.),
            // GpuLight::point(Vec3::new(-3., 3., 0.), 8., Vec3::new(0.2, 0.3, 0.9), 16.),
        ];

        let gr = &renderer.gr;

        let rs = renderer.rs();
        let pipelines = renderer.pipelines();

        // scene draw pass
        let dbg_va = renderer.upload_cbuffer(&[if self.meshlet_debug { 1 } else { 0}])?;
        graph
            .add_pass("GPass")
            .writes_depth(self.gbuffer.scene_depth, ResLoad::Clear)
            .writes_gbuffer(&self.gbuffer, ResLoad::Clear)
            .exec(move |ctx| {
                // more readbacking,,
                let counter_va = unsafe { counter.GetGPUVirtualAddress() };
                let cmd2 = ctx.cmd.cast::<ID3D12GraphicsCommandList2>()?;
                unsafe {
                    ctx.cmd.ResourceBarrier(&[transition_barrier(counter, D3D12_RESOURCE_STATE_COMMON, D3D12_RESOURCE_STATE_COPY_DEST)]);
                    cmd2.WriteBufferImmediate(1, &D3D12_WRITEBUFFERIMMEDIATE_PARAMETER { Dest: counter_va, Value: 0 }, None);
                    ctx.cmd.ResourceBarrier(&[transition_barrier(counter, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_UNORDERED_ACCESS)]);
                }
                rs.set_root_uav(ctx.cmd, counter_va);

                rs.set_pass(ctx.cmd, dbg_va);
                rs.set_objects(ctx.cmd, objects_va);
                rs.set_materials(ctx.cmd, materials_va);
                let mut bound = None;
                let cmd6 = ctx.cmd.cast::<ID3D12GraphicsCommandList6>()?;
                for (i, ((mesh, meshlets), pso)) in draws.iter().enumerate() {
                    if bound != Some(*pso) {
                        pipelines.get(*pso).bind(ctx.cmd);
                        bound = Some(*pso);
                    }
                    rs.set_meshlet_data(ctx.cmd, mesh.vb_va(), meshlets.va());
                    rs.set_draw_index(ctx.cmd, i as u32);
                    unsafe {
                        cmd6.DispatchMesh(meshlets.count().div_ceil(32), 1, 1)
                    };
                }

                unsafe {
                    ctx.cmd.ResourceBarrier(&[transition_barrier(counter, D3D12_RESOURCE_STATE_UNORDERED_ACCESS, D3D12_RESOURCE_STATE_COPY_SOURCE)]);
                    ctx.cmd.CopyBufferRegion(readback, off as u64, counter, 0, 4);
                }
                Ok(())
            });

        self.sky_velocity.add_pass(&self.gbuffer, renderer, graph)?;
        self.rt_shadows_pass.add_pass(&objects, &all_draws, &lights, &self.gbuffer, renderer, graph)?;
        self.lighting.add_pass(None, lights, renderer, None, Some(&self.rt_shadows_pass), None, 0.4, &self.gbuffer, graph, opts)?;
        let scene_color = if opts.anti_aliasing == AntiAliasing::Fsr2 {
            self.fsr_pass.add_pass(&self.gbuffer, renderer, time, graph)?
        } else {
            self.gbuffer.scene_color
        };
        let post_props = PostProperties {
            albedo: gr.get(self.gbuffer.albedo)?.srv_ptr(),
            normals: gr.get(self.gbuffer.normals)?.srv_ptr(),
            materials: gr.get(self.gbuffer.materials)?.srv_ptr(),
            depth: gr.get(self.gbuffer.scene_depth)?.srv_ptr(),
            scene_color: gr.get(scene_color)?.srv_ptr(),
            enable_sobel: 0,
            enable_fly_eye: 0,
            ..Default::default()
        };
        self.post.add_pass(post_props, Some(scene_color), None, renderer, &self.gbuffer, graph)?;
        self.blit.add_pass(renderer, graph)?;

        Ok(())
    }

    fn update<'a>(&'a mut self, renderer: &'a mut Renderer, time: &Time) -> anyhow::Result<()> {
        Ok(())
    }
}
