use glam::Vec3;
use crate::physics::contact::Contact;
use crate::physics::PhysParams;

#[derive(Debug, Clone)]
pub struct BodyParticle {
    pub position: Vec3,
    pub velocity: Vec3,
    pub force: Vec3,
    pub mass: f32,
}

impl BodyParticle {
    pub fn new(position: Vec3, mass: f32) -> Self {
        Self {
            position,
            velocity: Vec3::ZERO,
            force: Vec3::ZERO,
            mass,
        }
    }

    pub fn new_pinned(position: Vec3) -> Self {
        Self {
            position,
            velocity: Vec3::ZERO,
            force: Vec3::ZERO,
            mass: f32::INFINITY,
        }
    }

    pub fn resolve_collision(&mut self, contact: &Contact, params: &PhysParams) {
        if self.mass.is_infinite() { // pinned
            return;
        }

        self.position += contact.normal * contact.depth;
        let vn = self.velocity.dot(contact.normal);
        if vn >= 0. {
            return; // going opposite way
        }

        let v_t = self.velocity - vn * contact.normal;
        let vn_new = if -vn < params.rest_speed {
            0.0 // resting already
        } else {
            -params.restitution * vn
        };

        let delta_v = vn_new - vn;
        let t_len = v_t.length();
        let v_t_new = if t_len > 1e-9 {
            v_t * ((t_len - (params.k_friction * delta_v).min(t_len)) / t_len)
        } else {
            Vec3::ZERO
        }; // friction

        self.velocity = v_t_new + vn_new * contact.normal;
    }
}