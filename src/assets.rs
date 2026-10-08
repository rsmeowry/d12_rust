pub mod shaders;

use crate::render::mesh::Mesh;
use crate::render::pipeline::material::GpuMaterial;
use crate::render::texture::{build_mips, MipCount, Texture};
use crate::render::vertex::Vertex;
use crate::wrappers::graphics::batching::UploadBatch;
use crate::wrappers::graphics::GraphicsDevice;
use asset_importer::postprocess::PostProcessSteps;
use asset_importer::TextureType;
use glam::{Mat4, Vec2, Vec3, Vec4};
use image::RgbaImage;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

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
    let mut texture_paths: Vec<(PathBuf, bool, MipCount)> = Vec::new();

    for material in scene.materials() {
        match material.texture(TextureType::BaseColor, 0) {
            Some(t) => {
                let path_parent = pathbuf.parent().unwrap();
                texture_paths.push((path_parent.join(&t.path), true, MipCount::All));
            }
            None => {
            }
        };
    }

    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let mut texture_mips: Vec<anyhow::Result<Vec<RgbaImage>>> = Vec::new();
    for chunk in texture_paths.chunks(threads) {
        let got: Vec<anyhow::Result<Vec<RgbaImage>>> = std::thread::scope(|s| {
            let handles = chunk.iter()
                .map(|(p, _srgb, mip)| s.spawn(move || -> anyhow::Result<_> {
                    println!("Loading texture: {p:?}");
                    let img = image::open(p)?.to_rgba8();
                    Ok(build_mips(img, *mip))
                })).collect::<Vec<_>>();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        texture_mips.extend(got);
        println!("Done loading texture chunk!")
    }


    let mut textures: Vec<Texture> = Vec::with_capacity(texture_mips.len());
    let mut by_path: HashMap<PathBuf, usize> = HashMap::new();
    for ((path, srgb, _mips), tex_mips) in texture_paths.iter().zip(texture_mips.into_iter()) {
        let tex_mips = tex_mips?;
        let loaded = Texture::load_from_mips(batch, &tex_mips, *srgb, path.file_name().unwrap().to_str().unwrap())?;
        let idx = textures.len();
        textures.push(loaded);
        by_path.insert(path.to_path_buf(), idx);
    }

    for material in scene.materials() {
        match material.texture(TextureType::BaseColor, 0) {
            Some(t) => {
                let path_parent = pathbuf.parent().unwrap();
                let idx = *by_path.get(&path_parent.join(&t.path)).unwrap();
                materials.push(make_material(&material, textures[idx].srv_ptr()));
            }
            None => {
                materials.push(make_material(&material, white_tex));
            }
        };
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