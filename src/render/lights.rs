use bytemuck::Zeroable;
use glam::Vec3;
use crate::render::builtin::Vertex3;

pub const LIGHT_DIRECTIONAL: u32 = 0;
pub const LIGHT_POINT: u32 = 1;
pub const LIGHT_SPOT: u32 = 2;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuLight {
    pub position: Vec3,
    pub range: f32,
    pub direction: Vec3,
    pub kind: u32,
    pub color: Vec3,
    pub intensity: f32,
    pub spot_inner_cos: f32,
    pub spot_outer_cos: f32,
    pub spot_radius: f32,
    pub _pad: f32,
}

impl GpuLight {
    pub fn directional(dir: Vec3, color: Vec3, intensity: f32) -> Self {
        Self { direction: dir.normalize(), kind: 0, color, intensity, ..Self::zeroed() }
    }
    pub fn point(pos: Vec3, range: f32, color: Vec3, intensity: f32) -> Self {
        Self { position: pos, range, kind: 1, color, intensity, ..Self::zeroed() }
    }
    pub fn spot(pos: Vec3, dir: Vec3, range: f32, inner_deg: f32, outer_deg: f32, color: Vec3, intensity: f32) -> Self {
        Self {
            position: pos, range, direction: dir.normalize(), kind: 2, color, intensity,
            spot_inner_cos: inner_deg.to_radians().cos(),
            spot_outer_cos: outer_deg.to_radians().cos(),
            spot_radius: range * outer_deg.to_radians().tan(),
            ..Self::zeroed()
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightingPass {
    pub albedo: u32,
    pub normal: u32,
    pub material: u32,
    pub depth: u32,
    pub dir_count: u32,
    pub debug_view: u32,
    pub _pad: [u32; 2],
    pub ambient: Vec3,
    pub _pad2: f32,
}

pub fn light_cone(details: u32) -> (Vec<Vertex3>, Vec<u32>) {
    let mut v = vec![
        Vertex3::new([0.0, 0.0, 0.0], [0.0; 3], [0.0; 2]),
        Vertex3::new([0.0, 0.0, 1.0], [0.0; 3], [0.0; 2]),
    ];
    for i in 0..details {
        let a = std::f32::consts::TAU * i as f32 / details as f32;
        v.push(Vertex3::new([a.cos(), a.sin(), 1.0], [0.0; 3], [0.0; 2]));
    }
    let mut idx = Vec::new();
    for i in 0..details {
        let (cur, next) = (2 + i, 2 + (i + 1) % details);
        idx.extend([0, next, cur]);
        idx.extend([1, cur, next]);
    }
    (v, idx)
}

// https://danielsieger.com/blog/2021/03/27/generating-spheres.html
pub fn unit_sphere(slices: u32, stacks: u32) -> (Vec<Vertex3>, Vec<u32>) {
    let mut v = Vec::with_capacity(((stacks + 1) * (slices + 1)) as usize);
    for j in 0..=stacks {
        let phi = std::f32::consts::PI * j as f32 / stacks as f32;
        let (sp, cp) = phi.sin_cos();
        for i in 0..=slices {
            let theta = std::f32::consts::TAU * i as f32 / slices as f32;
            let (st, ct) = theta.sin_cos();
            let p = [sp * ct, sp * st, cp];
            let uv = [i as f32 / slices as f32, j as f32 / stacks as f32];
            v.push(Vertex3::new(p, p, uv));
        }
    }

    let row = slices + 1;
    let mut idx = Vec::with_capacity((stacks * slices * 6) as usize);
    for j in 0..stacks {
        for i in 0..slices {
            let u = j * row + i;
            let l = u + row;
            if j != 0 {
                idx.extend([u, l, u + 1]);
            }
            if j != stacks - 1 {
                idx.extend([u + 1, l, l + 1]);
            }
        }
    }
    (v, idx)
}