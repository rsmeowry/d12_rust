pub mod body;
pub mod contact;

use egui_probe::EguiProbe;
use glam::Vec3;
use crate::physics::body::SoftBody;
use crate::physics::contact::Collider;
use crate::time::Time;

#[derive(Debug, Copy, Clone, EguiProbe)]
pub struct PhysParams {
    pub gravity: [f32; 3],
    pub restitution: f32,
    pub k_friction: f32,
    pub rest_speed: f32,
}

#[derive(Debug)]
pub struct PhysicsSystem {
    accum: f32,
    dt: f32,
    interp_dt: f32,
    bodies: Vec<SoftBody>,
    static_colliders: Vec<Box<dyn Collider>>,
}

impl PhysicsSystem {
    pub fn new(freq: f32) -> PhysicsSystem {
        Self {
            accum: 0.,
            dt: 1. / freq,
            interp_dt: 0.01,
            static_colliders: Vec::new(),
            bodies: Vec::new()
        }
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

    pub fn update(&mut self, params: &PhysParams, time: &Time) {
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
}