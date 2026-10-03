use std::collections::HashMap;
use std::fs::File;
use crate::render::mesh::Mesh;
use crate::render::vertex::Vertex;
use crate::wrappers::graphics::GraphicsDevice;
use glam::{Mat4, Vec2, Vec3, Vec4};
use std::io::{Cursor, Read};
use std::path::Path;
use anyhow::bail;
use asset_importer::postprocess::PostProcessSteps;
use asset_importer::TextureType;
use tobj::LoadOptions;
use crate::render::texture::Texture;

#[derive(Debug)]
pub struct MaterialDefinition {
    pub index: u32,
    pub texture: Texture,
}

pub struct LoadedScene {
    pub mesh_indices: Vec<(usize, Mat4)>,
    pub meshes: Vec<(Mesh, u32)>,
    pub materials: HashMap<u32, MaterialDefinition>,
}

pub fn load_assimp_scene<V: Vertex, P: AsRef<Path>>(
    gd: &GraphicsDevice,
    path: P,
    make_vert: impl Fn(Vec3, Vec3, Vec2, Vec4) -> V
) -> anyhow::Result<LoadedScene> {
    let pathbuf = path.as_ref().to_path_buf();

    let scene = asset_importer::Importer::new().read_file(path)
        .with_post_process(PostProcessSteps::TRIANGULATE | PostProcessSteps::FLIP_UVS | PostProcessSteps::GEN_SMOOTH_NORMALS)
        .import()?;
    let mut materials = HashMap::<u32, MaterialDefinition>::new();
    let mut i = 0;
    for material in scene.materials() {
        if material.texture_count(TextureType::BaseColor) > 0 {
            // loading texture
            let path_parent = pathbuf.parent().unwrap();
            println!("LOADING {}", material.texture(TextureType::BaseColor, 0).unwrap().path);
            let mut tex_file = File::open(path_parent.join(material.texture(TextureType::BaseColor, 0).unwrap().path))?;
            let mut buf = Vec::with_capacity(1024 * 1024);
            tex_file.read_to_end(&mut buf)?;

            let tex = Texture::load(gd, &buf)?;

            materials.insert(i, MaterialDefinition {
                index: i,
                texture: tex,
            });
        }
        i += 1;
    }

    let built: Vec<(Mesh, u32)> = scene.meshes().map(|mesh| {
        let positions = mesh
            .vertices().iter()
            .map(|it| Vec3::new(it.x, it.y, it.z)).collect::<Vec<_>>();

        let normals = mesh
            .normals().iter().flatten()
            .map(|it| Vec3::new(it.x, it.y, it.z)).collect::<Vec<_>>();

        let uvs = mesh
            .texture_coords(0).iter().flatten()
            .map(|it| Vec2::new(it.x, it.y)).collect::<Vec<_>>();

        let indices = mesh.triangle_indices_iter().collect::<Vec<u32>>();

        let mut vertices: Vec<V> = Vec::with_capacity(positions.len());
        for i in 0..positions.len() {
            let vert = make_vert(positions[i], normals[i], uvs[i], Vec4::ONE);
            vertices.push(vert);
        }
        Mesh::new(gd, &vertices, &indices).map(|it| (it, mesh.material_index() as u32))
    }).collect::<anyhow::Result<_>>()?;

    let mut out = Vec::new();
    let mut stack = vec![(scene.root_node().unwrap(), Mat4::IDENTITY)];
    while let Some((node, parent)) = stack.pop() {
        let world = parent * Mat4::from_cols_array_2d(&node.transformation().to_cols_array_2d());
        for mi in node.mesh_indices_iter() {
            out.push((mi, world));
        }
        for child in node.children() {
            stack.push((child, world));
        }
    }

    Ok(LoadedScene {
        mesh_indices: out,
        meshes: built,
        materials,
    })
}

pub fn load_obj_mesh<V: Vertex>(
    gd: &GraphicsDevice,
    bytes: &[u8],
    make_vert: impl Fn(Vec3, Vec3, Vec2, Vec4) -> V,
) -> anyhow::Result<Mesh> {
    let mut c = Cursor::new(bytes);
    let (models, _mats) = tobj::load_obj_buf(
        &mut c,
        &LoadOptions {
            triangulate: true,
            single_index: true,
            ..Default::default()
        },
        |_| tobj::load_mtl_buf(&mut Cursor::new(vec![])),
    )?;

    let mesh = &models.first().unwrap().mesh;
    let vertex_cnt = mesh.positions.len() / 3;
    let mut vertices: Vec<V> = Vec::with_capacity(vertex_cnt);

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

        let uv = if !mesh.texcoords.is_empty() {
            Vec2::new(mesh.texcoords[i * 2], 1.0 - mesh.texcoords[i * 2 + 1])
        } else {
            Vec2::ZERO
        };

        let col = if !mesh.vertex_color.is_empty() {
            Vec4::new(
                mesh.vertex_color[i * 3],
                mesh.vertex_color[i * 3 + 1],
                mesh.vertex_color[i * 3 + 2],
                1.0,
            )
        } else {
            Vec4::new(0.5, 0.6, 0.75, 1.)
        };

        vertices.push(make_vert(pos, normal, uv, col));
    }

    Mesh::new(gd, &vertices, &mesh.indices)
}
