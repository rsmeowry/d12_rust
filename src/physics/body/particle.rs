use glam::Vec3;

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
}