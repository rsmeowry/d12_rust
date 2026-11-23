pub mod body;
pub mod contact;

use egui_probe::EguiProbe;
use glam::Vec3;
use crate::input::{InputDevice, KeyCode};
use crate::physics::body::particle::BodyParticle;
use crate::physics::body::{Grab, SoftBody};
use crate::physics::contact::Collider;
use crate::render::camera::Camera;
use crate::time::Time;

#[derive(Debug, Copy, Clone, EguiProbe)]
pub struct PhysParams {
    pub gravity: [f32; 3],
    pub restitution: f32,
    pub k_friction: f32,
    pub rest_speed: f32,
    pub air_drag: f32
}

#[derive(Debug)]
pub struct PhysicsSystem {
    accum: f32,
    dt: f32,
    interp_dt: f32,
    bodies: Vec<SoftBody>,
    static_colliders: Vec<Box<dyn Collider>>,
    click_depth: f32,
    is_dragging: bool,
}

impl PhysicsSystem {
    pub fn new(freq: f32) -> PhysicsSystem {
        Self {
            accum: 0.,
            dt: 1. / freq,
            interp_dt: 0.01,
            static_colliders: Vec::new(),
            bodies: Vec::new(),
            click_depth: 0.,
            is_dragging: false,
        }
    }

    pub fn get_body(&mut self, idx: usize) -> Option<&mut SoftBody> {
        self.bodies.get_mut(idx)
    }

    pub fn get_collider(&mut self, idx: usize) -> Option<&mut Box<dyn Collider>> {
        self.static_colliders.get_mut(idx)
    }

    pub fn add_body(&mut self, body: SoftBody) {
        self.bodies.push(body);
    }

    pub fn add_static_collider(&mut self, collider: Box<dyn Collider>) {
        self.static_colliders.push(collider);
    }

    pub fn fixed_dt(&self) -> f32 {
        self.dt
    }

    pub fn interpolation_dt(&self) -> f32 {
        self.interp_dt
    }

    pub fn update(&mut self, params: &PhysParams, time: &Time, camera: &Camera, input: &InputDevice) {
        // grabbing
        self.handle_grab(camera, input);

        self.accum += time.delta_time().min(0.25); // to prevent the loop of doom

        while self.accum >= self.dt {
            self.tick(params);

            self.accum -= self.dt;
        }

        self.interp_dt = self.accum / self.dt;
    }

    fn tick(&mut self, params: &PhysParams) {
        let colliders = &self.static_colliders;
        for body in self.bodies.iter_mut() {
            body.update(params, colliders, self.dt)
        }
    }

    fn handle_grab(&mut self, camera: &Camera, input: &InputDevice) {
        if input.is_pressed(KeyCode::LeftButton) {
            // just started pressing, setup grab
            let body = self.raycast_body(camera);
            if let Some(body) = body {
                let particle = Self::pick_particle(&body.particles, camera.pos(), camera.forward(), 0.3);
                if let Some((p_idx, p_pos)) = particle {
                    let depth = (p_pos - camera.pos()).dot(camera.forward());
                    let target = camera.pos() + camera.forward() * depth;
                    body.grab = Some(Grab {
                        particle: p_idx,
                        target,
                        k_mouse: 300.,
                        damping: 3.,
                    });
                    self.is_dragging = true;
                    self.click_depth = depth;
                }
            }
        } else if input.is_held(KeyCode::LeftButton) && self.is_dragging {
            // still holding, update grab
            let target = camera.pos() + camera.forward() * self.click_depth;
            for body in self.bodies.iter_mut() {
                if let Some(grab) = &mut body.grab {
                    grab.target = target;
                }
            }
        } else if input.was_released(KeyCode::LeftButton) && self.is_dragging {
            // just released, break grab
            self.is_dragging = false;
            for body in self.bodies.iter_mut() {
                body.grab = None;
            }
        }
    }

    fn raycast_body(&mut self, camera: &Camera) -> Option<&mut SoftBody> {
        let origin = camera.pos();
        let dir = camera.forward();
        let step = dir * 0.1;
        for i in 0..50 {
            let pos = origin + step * i as f32;
            for body in &mut self.bodies {
                if body.aabb().contains_point(pos) {
                    return Some(body);
                }
            }
        }
        None
    }

    fn pick_particle(particles: &[BodyParticle], origin: Vec3, dir: Vec3, radius: f32) -> Option<(usize, Vec3)> {
        let mut best: Option<(usize, f32, Vec3)> = None;
        for (i, p) in particles.iter().enumerate() {
            if p.mass.is_infinite() {
                continue;
            }
            let t = (p.position - origin).dot(dir).max(0.0);
            let closest = origin + dir * t;
            let d = (p.position - closest).length();
            if d < radius && best.map_or(true, |(_, bd, _)| d < bd) {
                best = Some((i, d, closest));
            }
        }
        best.map(|(i, _, _)| (i, particles[i].position))
    }
}
