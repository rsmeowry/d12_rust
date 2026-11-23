use std::fmt::Debug;
use glam::Vec3;

#[derive(Debug, Copy, Clone)]
pub struct Contact {
    pub depth: f32,
    pub normal: Vec3,
}

pub trait Collider: Debug {
    fn contact(&self, pos: Vec3) -> Option<Contact>;
}

#[derive(Debug, Copy, Clone)]
pub struct Plane {
    pub point: Vec3,
    pub normal: Vec3,
}

#[derive(Debug, Copy, Clone)]
pub struct Sphere {
    pub point: Vec3,
    pub radius: f32
}

impl Collider for Plane {
    fn contact(&self, pos: Vec3) -> Option<Contact> {
        let d = self.normal.dot(pos) - self.point.y;
        (d < 0.0).then_some(Contact { normal: self.normal, depth: -d })
    }
}

impl Collider for Sphere {
    fn contact(&self, pos: Vec3) -> Option<Contact> {
        let r = pos - self.point;
        let dist = r.length();
        if dist >= self.radius {
            return None;
        }
        let normal = if dist > 1e-6 {
            r / dist
        } else {
            Vec3::Y
        };
        Some(Contact { normal, depth: self.radius - dist })
    }
}