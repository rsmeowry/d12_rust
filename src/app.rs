use std::cell::Cell;
use egui::TexturesDelta;
use crate::input::KeyCode;
use crate::render::camera::Camera;
use crate::render::deferred::util::DebugTextPass;
use crate::render::graph::FrameGraph;
use crate::render::pipeline::Globals;
use crate::render::renderer::{HeapMark, Renderer};
use crate::scenes::{Scene, SCENES};
use crate::time::Time;
use crate::ui::EguiPass;
use crate::wrappers::event::WindowEvent;
use crate::wrappers::window::{NativeWindow, WindowProps};
use egui_probe::{EguiProbe, Probe};
use glam::{EulerRot, Mat4, Quat, Vec2, Vec2Swizzles, Vec3};
use crate::render::aa::{AntiAliasing, FsrQuality};
use crate::render::aa::temporal::jitter_vec;
use crate::scenes::sponza_lights::{ShadowMode, SponzaWithLights};

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;

pub struct App {
    renderer: Renderer,
    cam_target: Vec3,
    tilt: Vec2,

    is_focused: bool,
    egui_pass: EguiPass,
    egui: egui::Context,
    egui_ui_out: Option<(Vec<egui::ClippedPrimitive>, egui::TexturesDelta, f32)>,

    text: DebugTextPass,
    active_scene: Box<dyn Scene>,
    active_scene_name: &'static str,
    begin_heap: HeapMark,
    pending_scene_swap: Option<u32>,

    globals: Globals,
    time: Time,
    last_title_update: f32,
    frames: u32,
    fps: u32,

    window: NativeWindow,
    timings: Vec<(&'static str, f32)>,
    prev_view_proj: Mat4,
    prev_render_scale: f32,

    opts: RenderOpts,
    controller: ControllerInfo,
}

impl App {
    pub fn new() -> anyhow::Result<Self> {
        let window = NativeWindow::create(WindowProps::default())?;
        window.set_cursor_locked(true)?;
        let time = Time::new()?;
        let cam = Camera::new(
            WIDTH,
            HEIGHT,
            70.,
            Vec3::new(0., 2., 0.),
            Quat::from_rotation_x(30f32.to_radians()),
        );

        let mut opts = RenderOpts::default();
        opts.render_scale = 1.;

        let globals = Globals {
            view_proj: cam.view_proj(),
            view_proj_nj: cam.view_proj(),
            inv_view_proj: cam.view_proj().inverse(),
            prev_view_proj: cam.view_proj(),
            resolution: Vec2::new(WIDTH as f32, HEIGHT as f32),
            time: 0.0,
            delta_time: 0.0,
            display_resolution: Vec2::new(WIDTH as f32, HEIGHT as f32),
            camera_pos: cam.pos(),
            _pad2: [0; 2],
            mip_bias: 0.,
        };

        let mut renderer = Renderer::new(window.handle(), WIDTH, HEIGHT, cam)?;
        let mut egui_pass = EguiPass::new(&mut renderer)?;

        // pre-generating EGUI textures so they are before the mark
        let mut graph = FrameGraph::new();
        egui_pass.add_pass(&[], 1., egui::Vec2::ZERO, &renderer, &mut graph)?;
        graph.execute(&renderer.gd, &renderer.gr)?;
        let tex = TexturesDelta::default();
        egui_pass.update_textures(&renderer.gd, &tex)?;
        renderer.gd.flush_command_queue()?;

        let text = DebugTextPass::new(&mut renderer)?;

        let begin_heap = renderer.heap_mark()?;

        let scene = SCENES[0];
        let active_scene = scene.1(&mut renderer)?;
        let active_scene_name = scene.0;

        Ok(Self {
            cam_target: renderer.cam.pos(),
            tilt: Vec2::ZERO,
            renderer,

            text,
            active_scene,
            active_scene_name,
            begin_heap,
            pending_scene_swap: None,

            globals,
            time,
            last_title_update: 0.,
            frames: 0,
            fps: 0,
            prev_view_proj: globals.view_proj,
            prev_render_scale: 1.0,

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
            self.globals.display_resolution = Vec2::new(w as f32, h as f32);
            let (rw, rh) = self.renderer.gr.render_size();
            self.globals.resolution = Vec2::new(rw as f32, rh as f32);
            self.globals.mip_bias = if self.opts.anti_aliasing == AntiAliasing::Fsr2 {
                self.opts.fsr_quality.ratio().recip().log2() - 1.0
            } else { 0.0 };
        }
        Ok(true)
    }

    fn update(&mut self) -> anyhow::Result<()> {
        // scene swapping
        if let Some(idx) = self.pending_scene_swap.take() {
            self.renderer.gd.flush_command_queue()?;
            self.renderer.rewind(self.begin_heap)?;
            self.active_scene = SCENES[idx as usize].1(&mut self.renderer)?;
            self.active_scene_name = SCENES[idx as usize].0;
            self.cam_target = Vec3::ZERO;
        }

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
            self.opts.debug_view_type = DebugViewType::from_u32((self.opts.debug_view_type as u32 + 1) % 7);
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

        // jitter (taa + fsr)
        let scale = if self.opts.anti_aliasing == AntiAliasing::Fsr2 { self.opts.fsr_quality.ratio().recip() } else { self.opts.render_scale.max(0.01) };
        if scale != self.renderer.gr.render_scale() {
            self.renderer.set_render_scale(scale)?;
            let (rw, rh) = self.renderer.gr.render_size();
            self.globals.resolution = Vec2::new(rw as f32, rh as f32);
            self.globals.mip_bias = if self.opts.anti_aliasing == AntiAliasing::Fsr2 {
                self.opts.fsr_quality.ratio().recip().log2() - 1.0
            } else { 0.0 };
        }

        let j = match self.opts.anti_aliasing {
            AntiAliasing::Disabled => Vec2::ZERO,
            AntiAliasing::Temporal => jitter_vec(self.renderer.frame_count(), 8),
            AntiAliasing::Fsr2 => jitter_vec(self.renderer.frame_count(), (8. * self.opts.fsr_quality.ratio() * self.opts.fsr_quality.ratio()).ceil() as u32),
        };
        self.renderer.set_jitter(j);

        let vp_nj = self.renderer.cam.view_proj();
        self.globals.view_proj_nj = vp_nj;
        let vp_j = Mat4::from_translation(Vec3::new(2.0 * j.x, -2.0 * j.y, 0.) / self.globals.resolution.xyy()) * vp_nj;

        self.globals.view_proj = vp_j;
        self.globals.prev_view_proj = self.prev_view_proj;
        self.prev_view_proj = self.globals.view_proj_nj;
        self.globals.inv_view_proj = vp_j.inverse();
        self.globals.camera_pos = self.renderer.cam.pos();

        let egui_inputs = self.window.input_mut().collect_egui_input(self.globals.display_resolution.x as u32, self.globals.display_resolution.y as u32, self.time.game_time());

        let out = self.egui.run_ui(egui_inputs, |ctx| {
            egui::Window::new(format!("Brine: {}", self.active_scene_name)).show(ctx, |ui| {
                ui.label(format!("FPS: {} | mspf: {:.2}ms", self.fps, (1.0 / self.fps as f32) * 1000.));

                egui::CollapsingHeader::new("Rendering Options").show(ui, |header_ui| {
                    Probe::new(&mut self.opts).show(header_ui);
                    egui::widgets::global_theme_preference_buttons(header_ui);
                });

                egui::ComboBox::from_label("Active scene").selected_text(self.active_scene_name)
                    .show_ui(ui, |ui| {
                        for (scene_idx, scene) in SCENES.iter().enumerate() {
                            ui.selectable_value(&mut self.pending_scene_swap, Some(scene_idx as u32), scene.0);
                        }
                    });

                egui::CollapsingHeader::new("Scene info").show(ui, |header_ui| {
                    self.active_scene.ui(header_ui).unwrap();
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
        
        self.active_scene.update(&mut self.renderer, &self.time)?;

        Ok(())
    }

    fn draw(&mut self) -> anyhow::Result<()> {
        self.renderer.begin_frame(&self.globals)?;

        let mut graph = FrameGraph::new();

        self.active_scene.build_graph(&self.renderer, &self.time, &mut graph, &mut self.opts)?;

        let cam_pos = self.renderer.cam.pos();

        if let Some((prims, tex, ppp)) = &mut self.egui_ui_out {
            self.egui_pass.update_textures(&self.renderer.gd, tex)?;
            self.egui_pass.add_pass(prims, *ppp, egui::Vec2::new(self.globals.display_resolution.x, self.globals.display_resolution.y), &self.renderer, &mut graph)?;
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

        self.renderer.gd.display_rt(&self.opts)
    }

    fn handle_camera_movement(&mut self) {
        if !self.is_focused {
            return;
        }
        let input = self.window.input();
        self.controller.yaw += (input.mouse_delta().x * MOUSE_SENSITIVITY).to_radians();
        self.controller.pitch += (input.mouse_delta().y * MOUSE_SENSITIVITY).to_radians();
        self.controller.pitch = self.controller.pitch.clamp(-MAX_PITCH, MAX_PITCH);

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

        let move_speed = MOVE_SPEED * (if input.is_held(KeyCode::LeftShift) { SHIFT_MUL } else { 2.5 }) * self.time.delta_time();
        for (btn, axis) in btn_axis {
            if input.is_held(btn) {
                self.cam_target += axis * move_speed;
            }
        }
        let speed = 7.0;
        let t = 1.0 - (-speed * self.time.delta_time()).exp();
        let old_pos = self.renderer.cam.pos();
        let new_pos = old_pos.lerp(self.cam_target, t);
        self.renderer.cam.set_pos(new_pos);
        let velocity = (new_pos - old_pos) / self.time.delta_time().max(1e-4);

        // tilt
        let side = velocity.dot(right);
        let fwd = velocity.dot(front);
        let max_angle = 3.0_f32.to_radians();
        let ref_speed = MOVE_SPEED * 2.5;
        let target = Vec2::new(
            (fwd / ref_speed).clamp(-1.0, 1.0) * max_angle,
            (-side / ref_speed).clamp(-1.0, 1.0) * max_angle,
        );
        let tt = 1.0 - (-10.0 * self.time.delta_time()).exp();
        self.tilt = self.tilt.lerp(target, tt);

        self.renderer.cam.set_rot(
            Quat::from_euler(EulerRot::YXZ, self.controller.yaw, self.controller.pitch, 0.)
                * Quat::from_euler(EulerRot::XYZ, self.tilt.x, 0., self.tilt.y),
        );
    }
}

const MOUSE_SENSITIVITY: f32 = 0.7;
const MAX_PITCH: f32 = std::f32::consts::FRAC_PI_2 - 0.01;
const MOVE_SPEED: f32 = 2.0;
const SHIFT_MUL: f32 = 20.0;
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
    pub vsync: bool,
    pub culling_mode: CullingMode,
    pub anti_aliasing: AntiAliasing,
    pub fsr_quality: FsrQuality,
    pub use_wireframe: bool,
    pub debug_view_type: DebugViewType,
    pub render_scale: f32,
}

#[repr(u32)]
#[derive(Debug, Copy, Clone, EguiProbe, Default)]
pub enum DebugViewType {
    #[default]
    Default,
    Normals,
    Metallic,
    Roughness,
    AO,
    Velocity,
    RtShadows
}

impl DebugViewType {
    pub fn from_u32(u: u32) -> Self {
        match u {
            0 => Self::Default,
            1 => Self::Normals,
            2 => Self::Metallic,
            3 => Self::Roughness,
            4 => Self::AO,
            5 => Self::Velocity,
            6 => Self::RtShadows,
            _ => Self::Default,
        }
    }
}