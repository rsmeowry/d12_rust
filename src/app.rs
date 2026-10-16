use crate::input::KeyCode;
use crate::render::camera::Camera;
use crate::render::deferred::util::DebugTextPass;
use crate::render::graph::FrameGraph;
use crate::render::pipeline::Globals;
use crate::render::renderer::Renderer;
use crate::scenes::Scene;
use crate::time::Time;
use crate::wrappers::event::WindowEvent;
use crate::wrappers::window::{NativeWindow, WindowProps};
use glam::{EulerRot, Quat, Vec2, Vec3};
use std::borrow::Cow;

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;

pub struct App<S: Scene> {
    renderer: Renderer,

    text: DebugTextPass,
    scene: S,

    globals: Globals,
    time: Time,
    last_title_update: f32,
    frames: u32,
    fps: u32,

    window: NativeWindow,

    opts: RenderOpts,
    controller: ControllerInfo
}

impl<S: Scene> App<S> {
    pub fn new() -> anyhow::Result<Self> {
        let window = NativeWindow::create(WindowProps::default())?;
        window.set_cursor_locked(true)?;
        let time = Time::new()?;
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

        let mut renderer = Renderer::new(window.handle(), WIDTH, HEIGHT, cam)?;

        let scene = S::setup(&mut renderer)?;
        let text = DebugTextPass::new(&mut renderer)?;

        let mut opts = RenderOpts::default();
        opts.enable_dbg_text = true;

        Ok(Self {
            renderer,

            text,
            scene,

            globals,
            time,
            last_title_update: 0.,
            frames: 0,
            fps: 0,

            window,

            opts,
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
            self.renderer.cam.resize(w, h);
            self.globals.resolution = Vec2::new(w as f32, h as f32);
        }
        Ok(true)
    }

    fn update(&mut self) -> anyhow::Result<()> {
        self.handle_camera_movement();

        let input = self.window.input();
        let dt = self.time.delta_time();

        if input.was_released(KeyCode::Escape) {
            self.window.set_cursor_locked(false)?;
        }

        if input.was_released(KeyCode::F) {
            self.window.set_cursor_locked(true)?;
        }

        if input.was_released(KeyCode::F3) {
            self.opts.enable_dbg_text = !self.opts.enable_dbg_text;
        }

        if input.was_released(KeyCode::F4) {
            self.window.toggle_fullscreen()?;
        }

        if input.was_released(KeyCode::F5) {
            if let Err(e) = self.renderer.reload_shaders() {
                eprintln!("Error performing a hot-reload:\n{}", e);
            }
        }

        if input.was_released(KeyCode::F6) {
            self.opts.use_wireframe = !self.opts.use_wireframe;
        }

        if input.was_released(KeyCode::F7) {
            self.opts.debug_view_type = (self.opts.debug_view_type + 1) % 4;
        }

        if input.was_released(KeyCode::F8) {
            self.opts.culling_mode = match self.opts.culling_mode {
                CullingMode::Off => CullingMode::Frustum,
                CullingMode::Frustum => CullingMode::Octree,
                CullingMode::Octree => CullingMode::Off,
            }
        }

        self.globals.time = self.time.game_time();
        self.globals.delta_time = dt;
        self.globals.view_proj = self.renderer.cam.view_proj();
        self.globals.inv_view_proj = self.renderer.cam.view_proj().inverse();
        self.globals.camera_pos = self.renderer.cam.pos();
        Ok(())
    }

    fn draw(&mut self) -> anyhow::Result<()> {
        self.renderer.begin_frame(&self.globals)?;

        let mut graph = FrameGraph::new();

        self.scene.build_graph(&self.renderer, &self.time, &mut graph, &mut self.opts)?;

        let cam_pos = self.renderer.cam.pos();

        let mut text = format!("FPS: {} | tpf: {:.2}ms\nCamera: {cam_pos:.2}\nScene: {}", self.fps, 1000. / self.fps as f32, S::NAME);

        for line in &self.opts.debug_lines {
            text.push('\n');
            text.push_str(&line);
        }
        self.opts.debug_lines.clear();

        if self.opts.enable_dbg_text {
            self.text.add_pass(text, self.globals.resolution.x as usize, &self.renderer, &mut graph)?;
        }

        graph.execute(&self.renderer.gd, &self.renderer.gr)?;

        self.renderer.gd.display_rt()
    }

    fn handle_camera_movement(&mut self) {
        let input = self.window.input();
        self.controller.yaw += (input.mouse_delta().x * MOUSE_SENSITIVITY).to_radians();
        self.controller.pitch += (input.mouse_delta().y * MOUSE_SENSITIVITY).to_radians();
        self.controller.pitch = self.controller.pitch.clamp(-MAX_PITCH, MAX_PITCH);
        self.renderer.cam.set_rot(Quat::from_euler(EulerRot::YXZ, self.controller.yaw, self.controller.pitch, 0.));

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

        let mut pos = self.renderer.cam.pos();
        let move_speed = MOVE_SPEED * (if input.is_held(KeyCode::LeftShift) { SHIFT_MUL } else { 1.0 }) * self.time.delta_time();
        for (btn, axis) in btn_axis {
            if input.is_held(btn) {
                pos += axis * move_speed;
            }
        }
        self.renderer.cam.set_pos(pos);
    }
}

const MOUSE_SENSITIVITY: f32 = 0.7;
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

#[derive(Debug, Clone, Default)]
pub enum CullingMode {
    #[default]
    Off,
    Frustum,
    Octree,
}

#[derive(Debug, Clone, Default)]
pub struct RenderOpts {
    debug_lines: Vec<Cow<'static, str>>,
    pub culling_mode: CullingMode,
    pub use_wireframe: bool,
    pub debug_view_type: u32,
    pub enable_dbg_text: bool,
}

impl RenderOpts {
    pub fn push_debug_line(&mut self, line: Cow<'static, str>) {
        self.debug_lines.push(line);
    }
}