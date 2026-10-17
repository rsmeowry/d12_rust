pub mod shaders;

use crate::render::mesh::{DrawTopology, Mesh};
use crate::render::perf::frustum::AABB;
use crate::render::pipeline::material::GpuMaterial;
use crate::render::renderer::Renderer;
use crate::render::texture::{build_mips, MipCount, Texture};
use crate::render::vertex::{FromVertexProps, LodVertex, Vertex};
use crate::wrappers::graphics::batching::UploadBatch;
use asset_importer::postprocess::PostProcessSteps;
use asset_importer::{Material, TextureType};
use bitflags::{bitflags, bitflags_match};
use glam::{Mat4, Vec2, Vec3, Vec4};
use image::RgbaImage;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use crate::wrappers::graphics::GraphicsDevice;

#[derive(Debug)]
pub struct MaterialDefinition {
    pub index: u32,
    pub texture: Texture,
}

#[derive(Debug)]
pub struct LoadedScene {
    pub mesh_indices: Vec<(usize, Mat4)>,
    pub meshes: Vec<(Mesh, u32)>,
    pub mesh_lods: Vec<Vec<(Mesh, u32)>>,
    pub textures: Vec<Texture>,
    pub use_lods: bool,
}

impl LoadedScene {
    pub fn into_single_mesh(self) -> (Mesh, Mat4, Vec<Texture>) {
        let mesh = self.meshes.into_iter().next().unwrap();
        (mesh.0, self.mesh_indices[0].1, self.textures)
    }
}

#[derive(Debug)]
pub struct TextureSet {
    pub white: u32,
    pub albedo: u32,
    pub normal: u32,
    pub roughness: u32,
    pub lightmap: u32,
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct LoadTex: u32 {
        const ALBEDO = 0b00000001;
        const NORMAL = 0b00000010;
        const ROUGHNESS = 0b00000100;
        const LIGHTMAP = 0b00001000;
    }
}

pub fn load_assimp_scene<V: Vertex + FromVertexProps + LodVertex, P: AsRef<Path>>(
    renderer: &Renderer,
    batch: &mut UploadBatch<'_>,
    path: P,
    materials: &mut Vec<GpuMaterial>,
    load_textures: LoadTex,
    topology: DrawTopology,
    make_material: impl Fn(&asset_importer::material::Material, TextureSet) -> GpuMaterial,
) -> anyhow::Result<LoadedScene> {
    let pathbuf = path.as_ref().to_path_buf();

    let scene = asset_importer::Importer::new().read_file(path)
        .with_post_process(PostProcessSteps::TRIANGULATE |
            PostProcessSteps::FLIP_UVS |
            PostProcessSteps::GEN_SMOOTH_NORMALS |
            PostProcessSteps::CALC_TANGENT_SPACE
        )
        .import()?;
    let base = materials.len() as u32;
    let mut texture_paths: Vec<(PathBuf, bool, MipCount, TextureType)> = Vec::new();

    for material in scene.materials() {
        let mut load_single_tex = |_mat: &Material, ty: TextureType, flag: LoadTex| {
            if load_textures.contains(flag) {
                match material.texture(ty, 0) {
                    Some(t) => {
                        let path_parent = pathbuf.parent().unwrap();
                        texture_paths.push((path_parent.join(&t.path), ty == TextureType::BaseColor, MipCount::All, ty));
                    }
                    None => {
                    }
                };
            }
        };

        load_single_tex(&material, TextureType::BaseColor, LoadTex::ALBEDO);
        load_single_tex(&material, TextureType::Normals, LoadTex::NORMAL);
        load_single_tex(&material, TextureType::DiffuseRoughness, LoadTex::ROUGHNESS);
        load_single_tex(&material, TextureType::Lightmap, LoadTex::LIGHTMAP);
    }

    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let mut texture_mips: Vec<anyhow::Result<(Vec<RgbaImage>, TextureType)>> = Vec::new();
    for chunk in texture_paths.chunks(threads) {
        let got: Vec<anyhow::Result<(Vec<RgbaImage>, TextureType)>> = std::thread::scope(|s| {
            let handles = chunk.iter()
                .map(|(p, _srgb, mip, ty)| s.spawn(move || -> anyhow::Result<_> {
                    println!("Loading texture: {p:?} of type {ty:?}");
                    let img = image::open(p)?.to_rgba8();
                    Ok((build_mips(img, *mip), *ty))
                })).collect::<Vec<_>>();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        texture_mips.extend(got);
        println!("Done loading texture chunk!")
    }


    let mut textures: Vec<Texture> = Vec::with_capacity(texture_mips.len());
    let mut by_path: HashMap<PathBuf, usize> = HashMap::new();
    for ((path, srgb, _mips, _tex_type), tex_mips) in texture_paths.iter().zip(texture_mips.into_iter()) {
        let tex_mips = tex_mips?;
        let loaded = Texture::load_from_mips(batch, &tex_mips.0, *srgb, path.file_name().unwrap().to_str().unwrap())?;
        let idx = textures.len();
        textures.push(loaded);
        by_path.insert(path.to_path_buf(), idx);
    }

    for material in scene.materials() {
        let mut tex_set = TextureSet {
            white: renderer.white_tex().srv_ptr(),
            albedo: renderer.white_tex().srv_ptr(),
            normal: renderer.flat_normal_tex().srv_ptr(),
            roughness: renderer.white_tex().srv_ptr(),
            lightmap: renderer.white_tex().srv_ptr(),
        };
        for flag in load_textures.iter() {
            let ty = bitflags_match!(flag, {
                LoadTex::ALBEDO => TextureType::BaseColor,
                LoadTex::NORMAL => TextureType::Normals,
                LoadTex::ROUGHNESS => TextureType::DiffuseRoughness,
                LoadTex::LIGHTMAP => TextureType::Lightmap,
                _ => TextureType::BaseColor,
            });
            let idx = match material.texture(ty, 0) {
                Some(t) => {
                    let path_parent = pathbuf.parent().unwrap();
                    textures[*by_path.get(&path_parent.join(&t.path)).unwrap()].srv_ptr()
                }
                None => continue, // keep the white / flat-normal fallback
            } as u32;
            bitflags_match!(flag, {
                LoadTex::ALBEDO => tex_set.albedo = idx,
                LoadTex::NORMAL => tex_set.normal = idx,
                LoadTex::ROUGHNESS => tex_set.roughness = idx,
                LoadTex::LIGHTMAP => tex_set.lightmap = idx,
                _ => { },
            });
        }
        materials.push(make_material(&material, tex_set))
    }


    let built: Vec<(Mesh, u32, Vec<V>, Vec<u32>)> = scene.meshes().map(|mesh| {
        let positions = mesh
            .vertices().iter()
            .map(|it| Vec3::new(it.x, it.y, it.z)).collect::<Vec<_>>();

        let normals = mesh
            .normals().iter().flatten()
            .map(|it| Vec3::new(it.x, it.y, it.z)).collect::<Vec<_>>();

        let uvs = mesh
            .texture_coords(0).iter().flatten()
            .map(|it| Vec2::new(it.x, it.y)).collect::<Vec<_>>();

        let binormals = mesh
            .bitangents().iter().flatten()
            .map(|it| Vec3::new(it.x, it.y, it.z)).collect::<Vec<_>>();

        let tangents = mesh
            .tangents().iter().flatten()
            .map(|it| Vec3::new(it.x, it.y, it.z)).collect::<Vec<_>>();

        let indices = mesh.triangle_indices_iter().collect::<Vec<u32>>();

        let mut vertices: Vec<V> = Vec::with_capacity(positions.len());
        for i in 0..positions.len() {
            let vert = V::from_vertex_props(positions[i], normals[i], tangents[i], binormals[i], uvs[i], Vec4::ONE);
            vertices.push(vert);
        }
        let vc = vertices.clone();
        let ic = indices.clone();
        Mesh::new(&renderer.gd, batch, &vertices, &indices, topology, AABB::from_points(&positions)).map(|it| (it, base + mesh.material_index() as u32, vc, ic))
    }).collect::<anyhow::Result<_>>()?;

    let mut optimized: Vec<Vec<(Mesh, u32)>> = Vec::new();
    for (mesh, idx, vertices, indices) in &built {
        optimized.push(optimize_mesh(batch, *mesh.aabb(), vertices, indices, *idx));
    }

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
        meshes: built.into_iter().map(|it| (it.0, it.1)).collect(),
        textures,
        mesh_lods: optimized,
        use_lods: true
    })
}

fn optimize_mesh<TVert: LodVertex>(batch: &mut UploadBatch, aabb: AABB, vertices: &[TVert], indices: &[u32], idx: u32) -> Vec<(Mesh, u32)> {
    let target_levels = vec![
        32,
        16,
        8,
    ];

    let mut collapsed_vertices: Vec<Vec<TVert>> = Vec::new();
    let mut collapsed_indices: Vec<Vec<u32>> = Vec::new();

    for level in &target_levels {
        optimize_mesh_to_level(aabb, *level as u32, &vertices, &indices, &mut collapsed_vertices, &mut collapsed_indices);
    }

    let mut meshes = Vec::new();
    for (i, _) in target_levels.iter().enumerate() {
        let mesh = Mesh::new(batch.gd(), batch, &collapsed_vertices[i], &collapsed_indices[i], DrawTopology::TriangleList, aabb).unwrap();
        meshes.push((mesh, idx));
    }

    meshes
}

fn optimize_mesh_to_level<TVert: LodVertex + bytemuck::Zeroable + Copy>(aabb: AABB, target_cnt: u32, vertices: &[TVert], indices: &[u32], collapsed_vertices: &mut Vec<Vec<TVert>>, collapsed_indices: &mut Vec<Vec<u32>>) {
    let cell_size = (aabb.max - aabb.min).length() / target_cnt as f32;
    let mut cells: HashMap<(i32, i32, i32), usize> = HashMap::new();
    let mut sums: Vec<(TVert, u32)> = Vec::new();
    let mut remap: Vec<u32> = Vec::with_capacity(vertices.len());

    for vertex in vertices {
        let pos = vertex.pos();
        let resized = (pos / cell_size).floor();
        let (a, b, c) = (resized[0] as i32, resized[1] as i32, resized[2] as i32);
        let id = *cells.entry((a, b, c)).or_insert_with(|| {
            sums.push((bytemuck::Zeroable::zeroed(), 0));
            sums.len() - 1
        });
        let (sum, count) = sums.get_mut(id).unwrap();
        sum.add(vertex);
        *count += 1;
        remap.push(id as u32);
    }

    let collapsed = sums
        .iter_mut()
        .map(|(vert, count)| {
            vert.div(*count as f32);
            *vert
        })
        .collect::<Vec<_>>();

    let mut collapsed_idx: Vec<u32> = Vec::new();
    for chunk in indices.chunks_exact(3) {
        let a: u32 = remap[chunk[0] as usize];
        let b: u32 = remap[chunk[1] as usize];
        let c: u32 = remap[chunk[2] as usize];
        if a != b && a != c && b != c {
            collapsed_idx.extend(chunk);
        }
    }

    collapsed_vertices.push(collapsed);
    collapsed_indices.push(collapsed_idx);
}