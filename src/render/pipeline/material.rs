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
        displacement: u32,
        ambient: Vec3,
        diffuse: Vec3,
        specular: Vec3,
        shininess: f32,
        diffuse_k: f32,
        specular_k: f32,
    },
    Pbr {
        albedo_tex: u32,
        normal_tex: u32,
        metal_roughness_tex: u32,
        occlusion_tex: u32,
        base_color: Vec3,
        metallic: f32,
        roughness: f32
    },
    Billboard {
        albedo_tex: u32,
        normal_tex: u32,
        material_tex: u32,
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
            MaterialParams::PhongTess { albedo, normal, displacement, ambient, diffuse, specular, shininess, diffuse_k, specular_k } => {
                GpuMaterial {
                    textures: [albedo, normal, displacement, 0, 0, 0, 0, 0],
                    unique_params: [
                        ambient.x, ambient.y, ambient.z, diffuse.x,
                        diffuse.y, diffuse.z, specular.x, specular.y,
                        specular.z, shininess, diffuse_k, specular_k,
                        0., 0., 0., 0.
                    ]
                }
            }
            MaterialParams::Pbr { albedo_tex, normal_tex, metal_roughness_tex, occlusion_tex, base_color, metallic, roughness } => {
                GpuMaterial {
                    textures: [albedo_tex, normal_tex, metal_roughness_tex, occlusion_tex, 0, 0, 0, 0],
                    unique_params: [
                        base_color.x, base_color.y, base_color.z, metallic,
                        roughness, 0., 0., 0.,
                        0., 0., 0., 0.,
                        0., 0., 0., 0.
                    ]
                }
            }
            MaterialParams::Billboard { albedo_tex, normal_tex, material_tex } => {
                GpuMaterial {
                    textures: [albedo_tex, normal_tex, material_tex, 0, 0, 0, 0, 0],
                    unique_params: [0.; 16],
                }
            }
        }
    }
}