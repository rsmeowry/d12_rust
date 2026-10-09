use std::fs::File;
use std::io::Read;
use glam::{EulerRot, Quat, Vec2, Vec3, Vec4};
use windows::Win32::Graphics::Direct3D12::{ID3D12Resource, D3D12_COMPARISON_FUNC_LESS_EQUAL, D3D12_CULL_MODE_FRONT, D3D12_CULL_MODE_NONE, D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE};
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use crate::assets::{load_assimp_scene, LoadedScene};
use crate::input::{InputDevice, KeyCode};
use crate::render::builtin::Vertex3;
use crate::render::camera::Camera;
use crate::render::graph::FrameGraph;
use crate::render::graph::pass::ResLoad;
use crate::render::graph::res::{ResId, ResSize};
use crate::render::lights::{light_cone, unit_sphere, GpuLight, LightingPass, LIGHT_POINT, LIGHT_SPOT};
use crate::render::mesh::Mesh;
use crate::render::pipeline::desc::{Blend, Depth, PipelineDesc};
use crate::render::pipeline::material::MaterialParams;
use crate::render::pipeline::{Globals, PipelineId};
use crate::render::renderer::{ObjectData, Renderer, GB_ALBEDO, GB_MATERIAL, GB_NORMALS, SCENE_FORMAT};
use crate::render::texture::{MipCount, Texture};
use crate::render::txt::{build_ascii_texture, display_text};
use crate::render::vertex::Vertex;
use crate::time::Time;
use crate::wrappers::event::WindowEvent;
use crate::wrappers::graphics::GraphicsDevice;
use crate::wrappers::window::{NativeWindow, WindowProps};

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;

pub struct App {
    renderer: Renderer,
    scene_color: ResId,
    scene_depth: ResId,
    scene_normals: ResId,
    scene_albedo: ResId,
    scene_material: ResId,
    gpass: PipelineId,
    wireframe: PipelineId,
    unlit: PipelineId,
    blit: PipelineId,
    text: PipelineId,
    lights: PipelineId,
    lights_fullscreen: PipelineId,

    _white_tex: Texture,
    text_tex: Texture,
    sky_tex: Texture,
    materials_buf: ID3D12Resource,

    point_light_mesh: Mesh,
    spot_light_mesh: Mesh,
    sponza: LoadedScene,

    cam: Camera,
    globals: Globals,
    time: Time,
    last_title_update: f32,
    frames: u32,
    fps: u32,

    window: NativeWindow,

    use_wireframe: bool,
    dbg_view: u32,
    controller: ControllerInfo
}

impl App {
    pub fn new() -> anyhow::Result<Self> {
        let window = NativeWindow::create(WindowProps::default())?;
        window.set_cursor_locked(true)?;
        let time = Time::new()?;
        let mut renderer = Renderer::new(window.handle(), WIDTH, HEIGHT)?;

        let gbuffer_rts = vec![GB_ALBEDO, GB_NORMALS, GB_MATERIAL, SCENE_FORMAT];
        let gpass = renderer.create_pipeline(PipelineDesc {
            rts: gbuffer_rts.clone(),
            name: "GPass",
            shader: "deferred/GPass.hlsl".into(),
            input: Vertex3::LAYOUT,
            ..Default::default()
        })?;
        let wireframe = renderer.create_pipeline(PipelineDesc {
            rts: gbuffer_rts.clone(),
            name: "WireFrame",
            shader: "deferred/GPass.hlsl".into(),
            input: Vertex3::LAYOUT,
            wireframe: true,
            ..Default::default()
        })?;
        let unlit = renderer.create_pipeline(PipelineDesc {
            rts: vec![SCENE_FORMAT],
            name: "Unlit",
            shader: "UnlitScroll.hlsl".into(),
            input: Vertex3::LAYOUT,
            ..Default::default()
        })?;
        let blit = renderer.create_pipeline(PipelineDesc {
            rts: vec![GraphicsDevice::BB_RTV_FORMAT],
            name: "Blit",
            shader: "Blit.hlsl".into(),
            input: &[],
            depth: None,
            cull: D3D12_CULL_MODE_NONE,
            ..Default::default()
        })?;
        let text = renderer.create_pipeline(PipelineDesc {
            rts: vec![GraphicsDevice::BB_RTV_FORMAT],
            name: "Text",
            shader: "Text.hlsl".into(),
            input: &[],
            depth: None,
            cull: D3D12_CULL_MODE_NONE,
            ..Default::default()
        })?;
        let lights = renderer.create_pipeline(PipelineDesc {
            rts: vec![SCENE_FORMAT],
            cull: D3D12_CULL_MODE_FRONT,
            depth: Some(Depth {
                format: GraphicsDevice::DS_FORMAT,
                func: D3D12_COMPARISON_FUNC_LESS_EQUAL,
                write: false,
            }),
            depth_clip: false,
            blend: Blend::Additive,
            input: Vertex3::LAYOUT,
            name: "Lights",
            shader: "deferred/Lights.hlsl".into(),
            ..Default::default()
        })?;
        let lights_fullscreen = renderer.create_pipeline(PipelineDesc {
            rts: vec![SCENE_FORMAT],
            name: "LightsFullscreen",
            shader: "deferred/LightsFullscreen.hlsl".into(),
            input: &[],
            depth: None,
            cull: D3D12_CULL_MODE_NONE,
            blend: Blend::Additive,
            ..Default::default()
        })?;

        let gd = &renderer.gd;
        let mut batch = gd.begin_upload()?;

        // white texture must be the first one always !!
        let white_tex = Texture::from_rgba8(&mut batch, 1, 1, &[255; 4], false, MipCount::All, "builtin/WHITE")?;
        let mut file = File::open("assets/textures/sky/sky.hdr")?;
        let mut sky_buf = Vec::new();
        file.read_to_end(&mut sky_buf)?;
        let sky_tex = Texture::from_hdr(&mut batch, &sky_buf, "Sky")?;

        let text_tex = build_ascii_texture(&mut batch)?;

        let scene_color = renderer.gr.color(gd, "SceneColor", SCENE_FORMAT, Vec4::new(0.2, 0.3, 0.6, 1.0), ResSize::FitScreen)?;
        let scene_depth = renderer.gr.depth(gd, "SceneDepth", ResSize::FitScreen)?;
        let scene_albedo = renderer.gr.color(gd, "SceneAlbedo", GB_ALBEDO, Vec4::ZERO, ResSize::FitScreen)?;
        let scene_material = renderer.gr.color(gd, "SceneSpecular", GB_MATERIAL, Vec4::ZERO, ResSize::FitScreen)?;
        let scene_normals = renderer.gr.color(gd, "SceneNormals", GB_NORMALS, Vec4::ZERO, ResSize::FitScreen)?;

        let mut materials = Vec::new();
        let point_light_mesh = unit_sphere(8, 8);
        let point_light_mesh = Mesh::new(&gd, &mut batch, &point_light_mesh.0, &point_light_mesh.1)?;
        let spot_light_mesh = light_cone(8);
        let spot_light_mesh = Mesh::new(&gd, &mut batch, &spot_light_mesh.0, &spot_light_mesh.1)?;

        // sponza !!
        let sponza = load_assimp_scene(
            gd, &mut batch, "assets/sponza_old/Sponza.gltf",
            white_tex.srv_ptr(), &mut materials,
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

        let materials_buf = gd.create_default_buffer(&mut batch, bytemuck::cast_slice(&materials), D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE, "MaterialBuffer")?;
        batch.finish()?;

        let cam = Camera::new(
            WIDTH,
            HEIGHT,
            70.,
            Vec3::new(0., 2., -3.),
            Quat::from_rotation_x(30f32.to_radians()),
        );
        let globals = Globals {
            view_proj: cam.view_proj(),
            inv_view_proj: cam.view_proj().inverse(),
            resolution: Vec2::new(WIDTH as f32, HEIGHT as f32),
            time: 0.0,
            delta_time: 0.0,
            _pad2: 0.0,
            camera_pos: cam.pos(),
        };

        Ok(Self {
            renderer,
            scene_color,
            scene_depth,
            scene_albedo,
            scene_normals,
            scene_material,

            gpass,
            wireframe,
            unlit,
            blit,
            text,
            lights,
            lights_fullscreen,

            _white_tex: white_tex,
            text_tex,
            sky_tex,

            materials_buf,

            sponza,
            point_light_mesh,
            spot_light_mesh,

            cam,
            globals,
            time,
            last_title_update: 0.,
            frames: 0,
            fps: 0,

            window,

            use_wireframe: false,
            dbg_view: 0,
            controller: ControllerInfo::default()
        })
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        while self.poll_events()? {
            self.update()?;
            self.draw()?;

            self.frames += 1;
            if self.time.game_time() - 1. > self.last_title_update {
                self.last_title_update = self.time.game_time();
                self.fps = self.frames;
                self.frames = 0;
            }

            self.time.tick()?;
        }
        self.renderer.gd.flush_command_queue()
    }

    fn poll_events(&mut self) -> anyhow::Result<bool> {
        self.window.update()?;

        let mut new_size = None;
        for e in self.window.events().try_iter() {
            match e {
                WindowEvent::Resized { width, height } if width > 0 && height > 0 => {
                    new_size = Some((width, height));
                }
                WindowEvent::Resized { .. } => {}
                WindowEvent::CloseRequested => return Ok(false),
            }
        }

        if let Some((w, h)) = new_size {
            self.renderer.resize(w, h)?;
            self.cam.resize(w, h);
            self.globals.resolution = Vec2::new(w as f32, h as f32);
        }
        Ok(true)
    }

    fn update(&mut self) -> anyhow::Result<()> {
        self.handle_camera_movement();

        let input = self.window.input();
        let dt = self.time.delta_time();

        if input.is_pressed(KeyCode::Escape) {
            self.window.set_cursor_locked(false)?;
        }

        if input.is_pressed(KeyCode::LeftButton) {
            self.window.set_cursor_locked(true)?;
        }

        if input.was_released(KeyCode::F5) {
            if let Err(e) = self.renderer.reload_shaders() {
                eprintln!("Error performing a hot-reload:\n{}", e);
            }
        }

        if input.was_released(KeyCode::F6) {
            let gd = &self.renderer.gd;
            gd.flush_command_queue()?;
            let checker = debug_checker_tex(32, 32);
            let mut batch = gd.begin_upload()?;
            for tex in &mut self.sponza.textures {
                tex.rewrite(&mut batch, 32, 32, &checker, MipCount::All)?;
            }
            batch.finish()?;
        }

        if input.was_released(KeyCode::F7) {
            self.use_wireframe = !self.use_wireframe;
        }

        if input.was_released(KeyCode::F8) {
            self.dbg_view = (self.dbg_view + 1) % 4;
        }

        self.globals.time = self.time.game_time();
        self.globals.delta_time = dt;
        self.globals.view_proj = self.cam.view_proj();
        self.globals.inv_view_proj = self.cam.view_proj().inverse();
        self.globals.camera_pos = self.cam.pos();
        Ok(())
    }

    fn draw(&mut self) -> anyhow::Result<()> {
        let mut objects: Vec<ObjectData> = Vec::new();
        let mut draws: Vec<(&Mesh, PipelineId)> = Vec::new();
        let (phong, unlit) = if self.use_wireframe { (self.wireframe, self.wireframe) } else { (self.gpass, self.unlit) };
        for (scene, pso) in [(&self.sponza, phong)] {
            let (obj, meshes) = scene.to_objects_and_draws();
            objects.extend(obj);
            draws.extend(meshes.into_iter().map(|m| (m, pso)));
        }
        let glyphs = display_text(
            &format!("FPS: {}", self.fps),
            self.globals.resolution.x as u32, 1.0, Vec4::new(1.0, 1.0, 0.0, 1.0), true);

        self.renderer.begin_frame(&self.globals)?;
        let objects_va = self.renderer.upload_objects(&objects)?;
        let materials_va = unsafe { self.materials_buf.GetGPUVirtualAddress() };

        let glyphs_va = self.renderer.upload_slice(bytemuck::cast_slice(&glyphs), 16)?;
        let text_tex = self.text_tex.srv_ptr();

        let t = self.time.game_time();
        let mut lights = vec![
            GpuLight::directional(Vec3::new(-0.4, -0.8, 0.5), Vec3::new(1., 0.95, 0.9), 0.6),
            GpuLight::point(Vec3::new(4. * t.sin(), 1., -1.2), 6., Vec3::new(1., 0.3, 0.2), 8.),
            GpuLight::point(Vec3::new(-4. * t.sin(), 1., 1.2), 6., Vec3::new(0.2, 0.5, 1.), 8.),
            GpuLight::point(Vec3::new(2., 2. + 2. * t.cos(), 0.), 8., Vec3::new(0.2, 0.9, 0.2), 8.),
            GpuLight::spot(Vec3::new(4., 2., -1.), Vec3::NEG_Y, 15., 15., 25.,  Vec3::ONE, 20.),
        ];
        lights.sort_by_key(|l| l.kind);
        let first_point = lights.partition_point(|l| l.kind < LIGHT_POINT) as u32;
        let first_spot = lights.partition_point(|l| l.kind < LIGHT_SPOT) as u32;
        let point_count = first_spot - first_point;
        let spot_count = lights.len() as u32 - first_spot;
        let lights_va = self.renderer.upload_slice(bytemuck::cast_slice(&lights), 16)?;
        let gr = &self.renderer.gr;
        let light_pass_va = self.renderer.upload_slice(bytemuck::bytes_of(&LightingPass {
            albedo: gr.get(self.scene_albedo)?.srv_ptr(),
            normal: gr.get(self.scene_normals)?.srv_ptr(),
            material: gr.get(self.scene_material)?.srv_ptr(),
            depth: gr.get(self.scene_depth)?.srv_ptr(),
            dir_count: first_point,
            debug_view: self.dbg_view,
            sky_tex: self.sky_tex.srv_ptr(),
            _pad: 0.,
            ambient: Vec3::ONE * 0.07,
            _pad2: 0.,
        }), 256)?;

        let blit_va = self.renderer.upload_slice(bytemuck::bytes_of(&BlitProperties {
            source_tex: gr.get(self.scene_color)?.srv_ptr(),
            dither_tex: gr.get(self.scene_color)?.srv_ptr(),
            _pad: [0; 18],
        }), 16)?;

        let rs = self.renderer.rs();
        let pipelines = self.renderer.pipelines();
        let mut graph = FrameGraph::new();

        // scene draw pass
        graph
            .add_pass("GPass")
            .writes_depth(self.scene_depth, ResLoad::Clear)
            .writes(self.scene_albedo, ResLoad::Clear)
            .writes(self.scene_normals, ResLoad::Clear)
            .writes(self.scene_material, ResLoad::Clear)
            .writes(self.scene_color, ResLoad::Clear)
            .exec(|ctx| {
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

        // directional light
        graph
            .add_pass("LightsFullscreen")
            .reads(self.scene_albedo)
            .reads(self.scene_normals)
            .reads(self.scene_material)
            .reads(self.scene_depth)
            .writes(self.scene_color, ResLoad::Preserve)
            .exec(|ctx| {
                pipelines.get(self.lights_fullscreen).bind(ctx.cmd);
                rs.set_pass(ctx.cmd, light_pass_va);
                rs.set_lights(ctx.cmd, lights_va);
                unsafe {
                    ctx.cmd.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                    ctx.cmd.DrawInstanced(3, 1, 0, 0);
                }
                Ok(())
            });

        // lights volumes
        graph.add_pass("LightVolumes")
            .reads(self.scene_albedo)
            .reads(self.scene_normals)
            .reads(self.scene_material)
            .reads_depth(self.scene_depth)                     // DSV read-only + SRV
            .writes(self.scene_color, ResLoad::Preserve)
            .exec(|ctx| {
                pipelines.get(self.lights).bind(ctx.cmd);
                rs.set_pass(ctx.cmd, light_pass_va);
                rs.set_lights(ctx.cmd, lights_va);
                if point_count > 0 {
                    rs.set_draw_index(ctx.cmd, first_point);
                    self.point_light_mesh.draw_instanced(ctx.cmd, point_count);
                }
                if spot_count > 0 {
                    rs.set_draw_index(ctx.cmd, first_spot);
                    self.spot_light_mesh.draw_instanced(ctx.cmd, spot_count);
                }
                Ok(())
            });

        // blit+present pass
        graph.add_pass("Blit")
            .reads(self.scene_color)
            .uses_backbuffer()
            .exec(|ctx| {
                pipelines.get(self.blit).bind(ctx.cmd);
                rs.set_objects(ctx.cmd, blit_va);
                rs.set_draw_index(ctx.cmd, 0);
                unsafe {
                    ctx.cmd.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                    ctx.cmd.DrawInstanced(3, 1, 0, 0);
                }
                Ok(())
            });

        // debug text pass
        graph.add_pass("TextOverlay")
            .uses_backbuffer()
            .exec(|ctx| {
                pipelines.get(self.text).bind(ctx.cmd);
                rs.set_objects(ctx.cmd, glyphs_va);
                rs.set_draw_index(ctx.cmd, text_tex);
                unsafe {
                    ctx.cmd.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                    ctx.cmd.DrawInstanced(6, glyphs.len() as u32, 0, 0);
                }
                Ok(())
            });

        self.renderer.execute_graph(graph)
    }

    fn handle_camera_movement(&mut self) {
        let input = self.window.input();
        self.controller.yaw += (input.mouse_delta().x * MOUSE_SENSITIVITY).to_radians();
        self.controller.pitch += (input.mouse_delta().y * MOUSE_SENSITIVITY).to_radians();
        self.controller.pitch = self.controller.pitch.clamp(-MAX_PITCH, MAX_PITCH);
        self.cam.set_rot(Quat::from_euler(EulerRot::YXZ, self.controller.yaw, self.controller.pitch, 0.));

        let camera_rot_2d = Quat::from_rotation_y(self.controller.yaw);
        let left = camera_rot_2d * Vec3::NEG_X;
        let right = -left;
        let front = camera_rot_2d * Vec3::Z;
        let back = -front;

        let btn_axis = vec![
            (KeyCode::A, left),
            (KeyCode::D, right),
            (KeyCode::W, front),
            (KeyCode::S, back),
            (KeyCode::Space, 2. * Vec3::Y),
            (KeyCode::LeftControl, 2. * Vec3::NEG_Y)
        ];

        let mut pos = self.cam.pos();
        let move_speed = MOVE_SPEED * (if input.is_held(KeyCode::LeftShift) { SHIFT_MUL } else { 1.0 }) * self.time.delta_time();
        for (btn, axis) in btn_axis {
            if input.is_held(btn) {
                pos += axis * move_speed;
            }
        }
        self.cam.set_pos(pos);
    }
}

fn debug_checker_tex(w: u32, h: u32) -> Vec<u8> {
    (0..w * h)
        .flat_map(|i| {
            let (x, y) = (i % w, i / w);
            if (x / 4 + y / 4) % 2 == 0 { [255, 0, 255, 255] } else { [0, 0, 0, 255] }
        })
        .collect()
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct BlitProperties {
    pub source_tex: u32,
    pub dither_tex: u32,
    pub _pad: [u32; 18]
}

const MOUSE_SENSITIVITY: f32 = 1.0;
const MAX_PITCH: f32 = std::f32::consts::FRAC_PI_2 - 0.01;
const MOVE_SPEED: f32 = 2.0;
const SHIFT_MUL: f32 = 2.0;

#[derive(Debug, Clone)]
struct ControllerInfo {
    pub yaw: f32,
    pub pitch: f32,
}

impl Default for ControllerInfo {
    fn default() -> Self {
        Self {
            yaw: 40f32.to_radians(),
            pitch: 30f32.to_radians(),
        }
    }
}