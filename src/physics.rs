pub mod body;
pub mod contact;

use crate::physics::body::SoftBody;
use crate::physics::contact::Collider;
use crate::time::Time;

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

    pub fn interpolation_dt(&self) -> f32 {
        self.interp_dt
    }

    pub fn update(&mut self, time: &Time) {
        self.accum += time.delta_time().max(0.25); // to prevent the loop of doom

        while self.accum >= self.dt {
            self.tick();

            self.accum -= self.dt;
        }

        self.interp_dt = self.accum / self.dt;
    }

    fn tick(&mut self) {

    }
}