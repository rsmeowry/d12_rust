use egui_probe::EguiProbe;
use glam::Vec3;
use crate::physics::body::particle::BodyParticle;
use crate::physics::body::spring::{Spring, SpringKind};
use crate::physics::contact::Collider;
use crate::physics::{PhysParams, PhysicsSystem};
use crate::render::perf::frustum::AABB;

pub mod particle;
pub mod spring;
pub mod meshing;

#[derive(Debug, Copy, Clone, EguiProbe)]
pub struct BodyParams {
    pub mass: f32, // kg
    pub size: f32, // m
    pub ppa: usize, // points-per-axis
    pub k_struct: f32, // n/m
    pub k_shear: f32, // n/m
    pub damping: f32, // n*s/m
}

#[derive(Debug, Clone)]
pub struct Grab {
    pub particle: usize,
    pub target: Vec3,
    pub k_mouse: f32,
    pub damping: f32
}

#[derive(Debug, Clone)]
pub struct SoftBody {
    pub particles: Vec<BodyParticle>,
    pub springs: Vec<Spring>,
    pub ppa: usize, // particles-per-axis a ne dopsa
    initial_positions: Vec<Vec3>,
    pub grab: Option<Grab>,
    pub initial_mass: f32
}

impl SoftBody {
    pub fn new_cube(params: BodyParams, origin: Vec3) -> Self {
        let ppa = params.ppa;
        let height = params.size / (ppa - 1) as f32;
        let mass = params.mass / (ppa * ppa * ppa) as f32;

        // particles
        let mut particles = Vec::with_capacity(ppa * ppa * ppa);
        for z in 0..ppa {
            for y in 0..ppa {
                for x in 0..ppa {
                    let local_pos = Vec3::new(x as f32, y as f32, z as f32) * height - Vec3::new(params.size * 0.5, 0., params.size * 0.5);
                    particles.push(BodyParticle::new(origin + local_pos, mass));
                }
            }
        }

        // placing springs
        let mut springs = Vec::new();
        for x in 0..(ppa as i32) {
            for y in 0..(ppa as i32) {
                for z in 0..(ppa as i32) {
                    for dz in -1..=1i32 {
                        for dy in -1..=1i32 {
                            for dx in -1..=1i32 {

                                if (dz, dy, dx) <= (0, 0, 0) {
                                    continue;
                                }

                                let (x2, y2, z2) = (x + dx, y + dy, z + dz);
                                if !(0..(ppa as i32)).contains(&x2) || !(0..(ppa as i32)).contains(&y2) || !(0..(ppa as i32)).contains(&z2) {
                                    continue;
                                }

                                let nz = (dx != 0) as u8 + (dy != 0) as u8 + (dz != 0) as u8;
                                let (kind, k_stiff) = if nz == 1 {
                                    (SpringKind::Structural, params.k_struct)
                                } else {
                                    (SpringKind::Shear, params.k_shear)
                                };

                                let from = Self::idx(ppa, x as usize, y as usize, z as usize);
                                let to = Self::idx(ppa, x2 as usize, y2 as usize, z2 as usize);
                                let rest_len = (particles[to].position - particles[from].position).length();
                                springs.push(Spring {
                                    from,
                                    to,
                                    rest_len,
                                    k: k_stiff,
                                    c: params.damping,
                                    kind,
                                })
                            }
                        }
                    }
                }
            }
        }

        let initial_positions = particles.iter().map(|particle| particle.position).collect();
        Self {
            particles,
            springs,
            ppa,
            initial_positions,
            grab: None,
            initial_mass: params.mass,
        }
    }

    pub fn aabb(&self) -> AABB {
        let points = self.particles.iter().map(|particle| particle.position).collect::<Vec<_>>();
        AABB::from_points(&points)
    }

    pub fn reset(&mut self) {
        for (q, &p0) in self.particles.iter_mut().zip(&self.initial_positions) {
            q.position = p0;
            q.velocity = Vec3::ZERO;
            q.force = Vec3::ZERO;
        }
    }

    pub fn face_indices(&self, axis: usize, layer: usize) -> Vec<usize> {
        let ppa = self.ppa;
        let mut v = Vec::with_capacity(ppa * ppa);
        for a in 0..ppa {
            for b in 0..ppa {
                v.push(match axis {
                    0 => Self::idx(ppa, layer, a, b),
                    1 => Self::idx(ppa, a, layer, b),
                    _ => Self::idx(ppa, a, b, layer),
                });
            }
        }
        v
    }

    pub fn set_stiffness(&mut self, k_struct: f32, k_shear: f32, c: f32) {
        for s in &mut self.springs {
            s.k = match s.kind {
                SpringKind::Structural => k_struct,
                SpringKind::Shear => k_shear
            };
            s.c = c;
        }
    }

    pub fn update(&mut self, params: &PhysParams, colliders: &[Box<dyn Collider>], dt: f32) {
        for p in self.particles.iter_mut() {
            p.force = if p.mass.is_finite() {
                Vec3::from(params.gravity) * p.mass
            } else {
                Vec3::ZERO
            };
        }

        for p in self.particles.iter_mut().filter(|p| p.mass.is_finite()) {
            p.force -= params.air_drag * p.velocity;
        }

        for s in self.springs.iter_mut() {
            s.apply(&mut self.particles);
        }

        if let Some(g) = &self.grab {
            let p = &mut self.particles[g.particle];
            let f = g.k_mouse * (g.target - p.position) - g.damping * p.velocity;
            p.force += f;
        }

        // exclude pinned
        for p in self.particles.iter_mut().filter(|p| p.mass.is_finite()) {
            p.velocity += p.force / p.mass * dt;
            p.position += p.velocity * dt;
        }

        for p in self.particles.iter_mut() {
            for col in colliders {
                if let Some(c) = col.contact(p.position) {
                    p.resolve_collision(&c, params);
                }
            }
        }
    }

    pub fn idx(n: usize, x: usize, y: usize, z: usize) -> usize {
        x + n * (y + n * z)
    }
}