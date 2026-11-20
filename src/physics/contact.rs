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

impl Collider for Plane {
    fn contact(&self, pos: Vec3) -> Option<Contact> {
        let d = self.normal.dot(pos) - self.point.y;
        (d < 0.0).then_some(Contact { normal: self.normal, depth: -d })
    }
}