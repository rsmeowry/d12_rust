use serde::{Deserialize, Serialize};
use crate::render::pipeline::material::GpuMaterial;

#[derive(Debug, Serialize, Deserialize)]
pub struct DSHeader {
    pub textures: Vec<DSTexture>,
    pub meshes: Vec<DSMesh>,
    pub material: u32,
    pub meshlets: Option<(u32, u32, u32)>, // offset size count
    pub mesh_indices: Vec<(usize, [f32; 16])>,
    pub materials: Vec<(GpuMaterial, [i16; 8])> // tex slots, -1 is skip
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DSTexture {
    pub name: String,
    pub w: u32,
    pub h: u32,
    pub mips: f32,
    pub format: u32,
    pub srgb: bool,
    pub offset: u32,
    pub size: u32
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DSMesh {
    pub vb: (u32, u32), // offset size
    pub ib: (u32, u32), // offset size
    pub index_count: u32,
    pub aabb_min: [f32; 3],
    pub aabb_max: [f32; 3],
}