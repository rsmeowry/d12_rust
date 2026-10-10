use glam::{Vec3, Vec4};

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuMaterial {
    pub textures: [u32; 8], // up to 8 textures
    pub unique_params: [f32; 16], // 16 floats worth of params
}

#[derive(Debug, Clone)]
pub enum MaterialParams {
    ScrollingTex {
        tex: u32,
        color: Vec4,
        scroll_speed: f32
    },
    Phong {
        tex: u32, // 1
        ambient: Vec3, // 4
        diffuse: Vec3, // 7
        specular: Vec3, // 10
        shininess: f32, // 11
        diffuse_k: f32, // 12
        specular_k: f32, // 13
    },
    PhongTess {
        albedo: u32,
        normal: u32,
        ambient: Vec3,
        diffuse: Vec3,
        specular: Vec3,
        shininess: f32,
        diffuse_k: f32,
        specular_k: f32,
    }
}

impl MaterialParams {
    pub fn to_gpu_material(&self) -> GpuMaterial {
        match *self {
            MaterialParams::ScrollingTex { tex, color, scroll_speed } => {
                GpuMaterial {
                    textures: [tex, 0, 0, 0, 0, 0, 0, 0],
                    unique_params: [
                        color.x, color.y, color.z, color.w,
                        scroll_speed, 0., 0., 0.,
                        0., 0., 0., 0.,
                        0., 0., 0., 0.,
                    ],
                }
            }
            MaterialParams::Phong { tex, ambient, diffuse, specular, shininess, diffuse_k, specular_k } => {
                GpuMaterial {
                    textures: [tex, 0, 0, 0, 0, 0, 0, 0],
                    unique_params: [
                        ambient.x, ambient.y, ambient.z, diffuse.x,
                        diffuse.y, diffuse.z, specular.x, specular.y,
                        specular.z, shininess, diffuse_k, specular_k,
                        0., 0., 0., 0.
                    ]
                }
            }
            MaterialParams::PhongTess { albedo, normal, ambient, diffuse, specular, shininess, diffuse_k, specular_k } => {
                GpuMaterial {
                    textures: [albedo, normal, 0, 0, 0, 0, 0, 0],
                    unique_params: [
                        ambient.x, ambient.y, ambient.z, diffuse.x,
                        diffuse.y, diffuse.z, specular.x, specular.y,
                        specular.z, shininess, diffuse_k, specular_k,
                        0., 0., 0., 0.
                    ]
                }
            }
        }
    }
}