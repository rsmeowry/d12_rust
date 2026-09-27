use std::collections::HashMap;
use std::io::Cursor;
use glam::{Vec2, Vec3, Vec4};
use tobj::{LoadOptions, Material};
use crate::render::builtin::VertexPhong;
use crate::render::mesh::Mesh;
use crate::wrappers::graphics::GraphicsDevice;

pub fn load_obj_mesh(gd: &GraphicsDevice, bytes: &[u8]) -> anyhow::Result<Mesh> {
    let mut c = Cursor::new(bytes);
    let (models, _mats) = tobj::load_obj_buf(
        &mut c,
        &LoadOptions {
            triangulate: true,
            single_index: true,
            ..Default::default()
        },
        |p| { tobj::load_mtl_buf(&mut Cursor::new(vec![])) }
    )?;

    let mesh = &models.first().unwrap().mesh;
    let vertex_cnt = mesh.positions.len() / 3;
    let mut vertices = Vec::with_capacity(vertex_cnt);

    for i in 0..vertex_cnt {
        let pos = Vec3::new(
            mesh.positions[i * 3],
            mesh.positions[i * 3 + 1],
            mesh.positions[i * 3 + 2],
        );

        let normal = if !mesh.normals.is_empty() {
            Vec3::new(
                mesh.normals[i * 3],
                mesh.normals[i * 3 + 1],
                mesh.normals[i * 3 + 2],
            )
        } else {
            Vec3::ZERO
        };

        let _uv = if !mesh.texcoords.is_empty() {
            Vec2::new(mesh.texcoords[i * 2], 1.0 - mesh.texcoords[i * 2 + 1])
        } else {
            Vec2::ZERO
        };

        let col = if !mesh.vertex_color.is_empty() {
            Vec4::new(mesh.vertex_color[i * 3], mesh.vertex_color[i * 3 + 1], mesh.vertex_color[i * 3 + 2], 1.0)
        } else {
            Vec4::new(0.5, 0.6, 0.75, 1.)
        };

        vertices.push(VertexPhong::new(pos, normal, col))
    }

    Mesh::new(gd, &vertices, &mesh.indices)
}