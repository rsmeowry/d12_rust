use glam::{Quat, Vec2, Vec3, Vec4};
use windows::Win32::Graphics::Direct3D12::{ID3D12Resource, D3D12_CULL_MODE_NONE, D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE};
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use crate::assets::{load_assimp_scene, LoadedScene};
use crate::input::KeyCode;
use crate::render::builtin::Vertex3;
use crate::render::camera::Camera;
use crate::render::graph::FrameGraph;
use crate::render::graph::pass::ResLoad;
use crate::render::graph::res::{ResId, ResSize};
use crate::render::mesh::Mesh;
use crate::render::pipeline::desc::PipelineDesc;
use crate::render::pipeline::material::MaterialParams;
use crate::render::pipeline::{Globals, PipelineId};
use crate::render::renderer::{ObjectData, Renderer, SCENE_FORMAT};
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
    phong: PipelineId,
    wireframe: PipelineId,
    unlit: PipelineId,
    blit: PipelineId,
    text: PipelineId,

    _white_tex: Texture,
    text_tex: Texture,
    materials_buf: ID3D12Resource,
    sphere: LoadedScene,
    sponza: LoadedScene,

    cam: Camera,
    camera_y: f32,
    globals: Globals,
    time: Time,
    last_title_update: f32,
    frames: u32,
    fps: u32,

    window: NativeWindow,
    width: u32,

    use_wireframe: bool,
}

impl App {
    pub fn new() -> anyhow::Result<Self> {
        let window = NativeWindow::create(WindowProps::default())?;
        let time = Time::new()?;
        let mut renderer = Renderer::new(window.handle(), WIDTH, HEIGHT)?;

        let phong = renderer.create_pipeline(PipelineDesc {
            rts: vec![SCENE_FORMAT],
            name: "Phong",
            shader: "PhongTex.hlsl".into(),
            input: Vertex3::LAYOUT,
            ..Default::default()
        })?;
        let wireframe = renderer.create_pipeline(PipelineDesc {
            rts: vec![SCENE_FORMAT],
            name: "Phong",
            shader: "PhongTex.hlsl".into(),
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

        let gd = &renderer.gd;
        let mut batch = gd.begin_upload()?;

        // white texture must be the first one always !!
        let white_tex = Texture::from_rgba8(&mut batch, 1, 1, &[255; 4], false, MipCount::All, "builtin/WHITE")?;

        let text_tex = build_ascii_texture(&mut batch)?;

        let scene_color = renderer.gr.color(gd, "SceneColor", SCENE_FORMAT, Vec4::new(0.2, 0.3, 0.6, 1.0), ResSize::FitScreen)?;
        let scene_depth = renderer.gr.depth(gd, "SceneDepth", ResSize::FitScreen)?;

        let mut materials = Vec::new();
        // unlit sphere
        let sphere = load_assimp_scene(
            gd, &mut batch, "assets/models/Sphere.gltf",
            white_tex.srv_ptr(), &mut materials,
            |pos, norm, uv, _col| Vertex3::new(pos, norm, uv),
            |_mat, tex| MaterialParams::ScrollingTex {
                tex,
                color: Vec4::ONE,
                scroll_speed: 1.0,
            }.to_gpu_material()
        )?;

        // sponza !!
        let sponza = load_assimp_scene(
            gd, &mut batch, "assets/sponza_old/Sponza.gltf",
            white_tex.srv_ptr(), &mut materials,
            |pos, norm, uv, _col| Vertex3::new(pos, norm, uv),
            |_mat, tex| MaterialParams::Phong {
                tex,
                ambient: Vec3::new(0.05, 0.05, 0.05),
                diffuse: Vec3::ONE,
                specular: Vec3::ONE,
                shininess: 32.0,
                diffuse_k: 0.3,
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
            resolution: Vec2::new(WIDTH as f32, HEIGHT as f32),
            time: 0.0,
            delta_time: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
            camera_pos: cam.pos(),
            light_dir: Vec3::new(-0.4, -0.8, 0.5).normalize(),
            light_col: Vec4::new(1.0, 0.9, 0.9, 1.0),
        };

        Ok(Self {
            renderer,
            scene_color,
            scene_depth,
            phong,
            wireframe,
            unlit,
            blit,
            text,

            _white_tex: white_tex,
            text_tex,
            materials_buf,
            sphere,
            sponza,

            cam,
            camera_y: 0.,
            globals,
            time,
            last_title_update: 0.,
            frames: 0,
            fps: 0,

            window,
            width: WIDTH,

            use_wireframe: false,
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
            self.width = w;
        }
        Ok(true)
    }

    fn update(&mut self) -> anyhow::Result<()> {
        let input = self.window.input();
        let dt = self.time.delta_time();

        if input.is_pressed(KeyCode::H) {
            println!("{:?} {:?}", dt, self.time.game_time());
        }

        if input.is_held(KeyCode::Left) {
            self.camera_y -= dt * 50.;
            self.cam.set_rot(Quat::from_rotation_y(self.camera_y.to_radians()));
        }

        if input.is_held(KeyCode::Right) {
            self.camera_y += dt * 50.;
            self.cam.set_rot(Quat::from_rotation_y(self.camera_y.to_radians()));
        }

        let (right, forward) = (self.cam.rot().mul_vec3(Vec3::X), self.cam.rot().mul_vec3(Vec3::Z));
        for (key, dir) in [
            (KeyCode::A, -right), (KeyCode::D, right),
            (KeyCode::W, forward), (KeyCode::S, -forward),
            (KeyCode::Space, Vec3::Y), (KeyCode::V, -Vec3::Y),
        ] {
            if input.is_held(key) {
                self.cam.set_pos(self.cam.pos() + dir * dt);
            }
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

        self.globals.time = self.time.game_time();
        self.globals.delta_time = dt;
        self.globals.view_proj = self.cam.view_proj();
        self.globals.camera_pos = self.cam.pos();
        Ok(())
    }

    fn draw(&mut self) -> anyhow::Result<()> {
        let mut objects: Vec<ObjectData> = Vec::new();
        let mut draws: Vec<(&Mesh, PipelineId)> = Vec::new();
        let (phong, unlit) = if self.use_wireframe { (self.wireframe, self.wireframe) } else { (self.phong, self.unlit) };
        for (scene, pso) in [(&self.sphere, unlit), (&self.sponza, phong)] {
            let (obj, meshes) = scene.to_objects_and_draws();
            objects.extend(obj);
            draws.extend(meshes.into_iter().map(|m| (m, pso)));
        }
        let glyphs = display_text(
            &format!("FPS: {}", self.fps),
            self.width, 1.5, Vec4::new(1.0, 1.0, 0.0, 1.0), true);

        self.renderer.begin_frame(&self.globals)?;
        let objects_va = self.renderer.upload_objects(&objects)?;
        let materials_va = unsafe { self.materials_buf.GetGPUVirtualAddress() };

        let glyphs_va = self.renderer.upload_slice(bytemuck::cast_slice(&glyphs), 16)?;
        let text_tex = self.text_tex.srv_ptr();

        let rs = self.renderer.rs();
        let pipelines = self.renderer.pipelines();
        let mut graph = FrameGraph::new();

        // scene draw pass
        graph
            .add_pass("DrawScene")
            .writes_depth(self.scene_depth, ResLoad::Clear)
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

        // blit+present pass
        graph.add_pass("Blit")
            .reads(self.scene_color)
            .uses_backbuffer()
            .exec(|ctx| {
                pipelines.get(self.blit).bind(ctx.cmd);
                rs.set_draw_index(ctx.cmd, ctx.srv_ptr(self.scene_color)?);
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
}

fn debug_checker_tex(w: u32, h: u32) -> Vec<u8> {
    (0..w * h)
        .flat_map(|i| {
            let (x, y) = (i % w, i / w);
            if (x / 4 + y / 4) % 2 == 0 { [255, 0, 255, 255] } else { [0, 0, 0, 255] }
        })
        .collect()
}
