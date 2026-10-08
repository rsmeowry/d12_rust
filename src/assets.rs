pub mod shaders;

use crate::render::mesh::Mesh;
use crate::render::pipeline::material::GpuMaterial;
use crate::render::texture::Texture;
use crate::render::vertex::Vertex;
use crate::wrappers::graphics::GraphicsDevice;
use asset_importer::postprocess::PostProcessSteps;
use asset_importer::TextureType;
use glam::{Mat4, Vec2, Vec3, Vec4};
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::Path;
use tobj::LoadOptions;
use crate::wrappers::graphics::batching::UploadBatch;

#[derive(Debug)]
pub struct MaterialDefinition {
    pub index: u32,
    pub texture: Texture,
}

pub struct LoadedScene {
    pub mesh_indices: Vec<(usize, Mat4)>,
    pub meshes: Vec<(Mesh, u32)>,
    pub textures: Vec<Texture>,
}

pub fn load_assimp_scene<V: Vertex, P: AsRef<Path>>(
    gd: &GraphicsDevice,
    batch: &mut UploadBatch<'_>,
    path: P,
    white_tex: u32,
    materials: &mut Vec<GpuMaterial>,
    make_vert: impl Fn(Vec3, Vec3, Vec2, Vec4) -> V,
    make_material: impl Fn(&asset_importer::material::Material, u32) -> GpuMaterial,
) -> anyhow::Result<LoadedScene> {
    let pathbuf = path.as_ref().to_path_buf();

    let scene = asset_importer::Importer::new().read_file(path)
        .with_post_process(PostProcessSteps::TRIANGULATE | PostProcessSteps::FLIP_UVS | PostProcessSteps::GEN_SMOOTH_NORMALS)
        .import()?;
    let base = materials.len() as u32;
    let mut textures = Vec::<Texture>::new();
    for material in scene.materials() {
        let albedo = match material.texture(TextureType::BaseColor, 0) {
            Some(t) => {
                let path_parent = pathbuf.parent().unwrap();
                println!("Loading texture: {}", t.path);
                let mut tex_file = File::open(path_parent.join(&t.path))?;
                let mut buf = Vec::with_capacity(1024 * 1024);
                tex_file.read_to_end(&mut buf)?;

                let tex_name: &Path = t.path.as_ref();
                let tex_name = tex_name.file_name().unwrap();
                let tex = Texture::load(batch, &buf, true, tex_name.to_str().unwrap())?;
                let idx = tex.srv_ptr();
                textures.push(tex);
                idx
            },
            None => white_tex,
        };
        materials.push(make_material(&material, albedo));
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
        Mesh::new(gd, batch, &vertices, &indices).map(|it| (it, base + mesh.material_index() as u32))
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
        textures,
    })
}