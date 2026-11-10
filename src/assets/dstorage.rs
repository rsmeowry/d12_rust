pub mod types;

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use glam::Mat4;
use serde::{Deserialize, Serialize};
use crate::render::pipeline::material::GpuMaterial;

#[derive(Debug, Serialize, Deserialize)]
pub struct DSHeader {
    pub textures: Vec<DSTexture>,
    pub meshes: Vec<DSMesh>,
    pub mesh_indices: Vec<(usize, Mat4)>,
    pub materials: Vec<(GpuMaterial, [i16; 8])>, // tex slots, -1 is skip
    pub vertex_stride: u32
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DSTexture {
    pub name: String,
    pub w: u32,
    pub h: u32,
    pub mips: u32,
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
    pub material_idx: u32,
    pub meshlet: Option<DSMeshlet>
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DSMeshlet {
    pub offset: u32,
    pub size: u32,
    pub meshlet_count: u32,
}

#[derive(Debug, Default)]
pub struct DSBlobWriter {
    pub data: Vec<u8>
}

impl DSBlobWriter {
    // offset + size
    pub fn push(&mut self, bytes: &[u8]) -> (u32, u32) {
        let off = self.data.len().next_multiple_of(16);
        self.data.resize(off, 0);
        self.data.extend_from_slice(bytes);
        (off as u32, bytes.len() as u32)
    }

    pub fn flush(self, header: DSHeader, path: &Path) -> anyhow::Result<()> {
        // magic val (0xBA 0xDC 0x0D 0xE0 ) -> header size -> header postcard -> raw DStorage data
        let header_bytes = postcard::to_allocvec(&header)?;

        let mut file = File::create(path)?;
        let mut writer = BufWriter::new(&mut file);
        writer.write_all(&[0xBA, 0xDC, 0x0D, 0xE0])?;
        writer.write_all(header_bytes.len().to_le_bytes().as_ref())?;
        writer.write_all(&header_bytes)?;
        writer.write_all(&self.data)?;
        writer.flush()?;

        Ok(())
    }
}