use crate::input::KeyCode;
use crate::render::camera::Camera;
use crate::render::deferred::util::DebugTextPass;
use crate::render::graph::FrameGraph;
use crate::render::pipeline::Globals;
use crate::render::renderer::Renderer;
use crate::scenes::Scene;
use crate::time::Time;
use crate::ui::EguiPass;
use crate::wrappers::event::WindowEvent;
use crate::wrappers::window::{NativeWindow, WindowProps};
use egui_probe::{EguiProbe, Probe};
use glam::{EulerRot, Quat, Vec2, Vec3};

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;

pub struct App<S: Scene> {
    renderer: Renderer,

    is_focused: bool,
    egui_pass: EguiPass,
    egui: egui::Context,
    egui_ui_out: Option<(Vec<egui::ClippedPrimitive>, egui::TexturesDelta, f32)>,

    text: DebugTextPass,
    scene: S,

    globals: Globals,
    time: Time,
    last_title_update: f32,
    frames: u32,
    fps: u32,

    window: NativeWindow,
    timings: Vec<(&'static str, f32)>,

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

        let egui_pass = EguiPass::new(&mut renderer)?;

        Ok(Self {
            renderer,

            text,
            scene,

            globals,
            time,
            last_title_update: 0.,
            frames: 0,
            fps: 0,

            is_focused: true,
            egui: egui::Context::default(),
            egui_pass,
            egui_ui_out: None,

            window,
            timings: Vec::new(),

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
            self.is_focused = false;
            self.window.set_cursor_locked(false)?;
        }

        if input.was_released(KeyCode::F) {
            self.is_focused = true;
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
            self.opts.debug_view_type = DebugViewType::from_u32((self.opts.debug_view_type as u32 + 1) % 5);
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

        let egui_inputs = self.window.input_mut().collect_egui_input(self.globals.resolution.x as u32, self.globals.resolution.y as u32, self.time.game_time());

        let out = self.egui.run_ui(egui_inputs, |ctx| {
            egui::Window::new(format!("Brine: {}", S::NAME)).show(ctx, |ui| {
                ui.label(format!("FPS: {} | mspf: {:.2}ms", self.fps, (1.0 / self.fps as f32) * 1000.));

                egui::CollapsingHeader::new("Rendering Options").show(ui, |header_ui| {
                    Probe::new(&mut self.opts).show(header_ui);
                    egui::widgets::global_theme_preference_buttons(header_ui);
                });

                egui::CollapsingHeader::new("Scene info").show(ui, |header_ui| {
                    self.scene.ui(header_ui).unwrap();
                });

                egui::CollapsingHeader::new("Pass Timings").show(ui, |header_ui| {
                    for timing in &self.timings {
                        if timing.1 < 0.02 {
                            continue;
                        }
                        header_ui.label(format!("{}: {:.2}ms", timing.0, timing.1));
                    }
                })
            });
        });
        let prims = self.egui.tessellate(out.shapes, out.pixels_per_point);
        self.egui_ui_out = Some((prims, out.textures_delta, out.pixels_per_point));

        Ok(())
    }

    fn draw(&mut self) -> anyhow::Result<()> {
        self.renderer.begin_frame(&self.globals)?;

        let mut graph = FrameGraph::new();

        self.scene.build_graph(&self.renderer, &self.time, &mut graph, &mut self.opts)?;

        let cam_pos = self.renderer.cam.pos();

        let text = format!("FPS: {} | tpf: {:.2}ms\nCamera: {cam_pos:.2}\nScene: {}", self.fps, 1000. / self.fps as f32, S::NAME);
        if let Some((prims, tex, ppp)) = &mut self.egui_ui_out {
            self.egui_pass.update_textures(&self.renderer.gd, tex)?;
            self.egui_pass.add_pass(prims, *ppp, egui::Vec2::new(self.globals.resolution.x, self.globals.resolution.y), &self.renderer, &mut graph)?;
            tex.clear();
        }

        let k = 1.0 - (-self.time.delta_time() / TIMINGS_SMOOTHING_SECS).exp();
        let prev = std::mem::take(&mut self.timings);
        self.timings = graph.execute(&self.renderer.gd, &self.renderer.gr)?.into_iter().enumerate()
            .map(|(i, (name, ms))| match prev.get(i) {
                Some(&(prev_name, prev_ms)) if prev_name == name => (name, prev_ms + (ms - prev_ms) * k),
                _ => (name, ms),
            })
            .collect();

        self.renderer.gd.display_rt()
    }

    fn handle_camera_movement(&mut self) {
        if !self.is_focused {
            return;
        }
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
const TIMINGS_SMOOTHING_SECS: f32 = 0.5;

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

#[derive(Debug, Copy, Clone, Default, EguiProbe)]
pub enum CullingMode {
    Off,
    #[default]
    Frustum,
    Octree,
}

#[derive(Debug, Copy, Clone, Default, EguiProbe)]
pub struct RenderOpts {
    pub culling_mode: CullingMode,
    pub use_wireframe: bool,
    pub debug_view_type: DebugViewType,
    pub enable_dbg_text: bool,
}

#[repr(u32)]
#[derive(Debug, Copy, Clone, EguiProbe, Default)]
pub enum DebugViewType {
    #[default]
    Default,
    Normals,
    Metallic,
    Roughness,
    AO
}

impl DebugViewType {
    pub fn from_u32(u: u32) -> Self {
        match u {
            0 => Self::Default,
            1 => Self::Normals,
            2 => Self::Metallic,
            3 => Self::Roughness,
            4 => Self::AO,
            _ => Self::Default,
        }
    }
}