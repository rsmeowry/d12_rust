use glam::Vec3;

pub mod simulate;
pub mod draw;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Particle {
    pub pos: Vec3,
    pub lifetime_seconds: f32,
    pub velocity: Vec3,
    pub size: f32,
    pub color: Vec3,
    pub max_lifetime: f32,
}
const PARTICLE_STRIDE: u32 = size_of::<Particle>() as u32;
