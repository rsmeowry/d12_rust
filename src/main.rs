mod assets;
mod input;
mod render;
mod time;
mod wrappers;

use crate::assets::shaders::ShaderStorage;
use crate::assets::load_assimp_scene;
use crate::input::KeyCode;
use crate::render::builtin::{Vertex2, Vertex3, VertexPhong};
use crate::render::camera::Camera;
use crate::render::mesh::Mesh;
use crate::render::pipeline::desc::PipelineDesc;
use crate::render::pipeline::material::{GpuMaterial, MaterialParams};
use crate::render::pipeline::{Globals, PipelineManager};
use crate::render::texture::Texture;
use crate::render::vertex::Vertex;
use crate::time::Time;
use crate::wrappers::event::WindowEvent;
use crate::wrappers::graphics::GraphicsDevice;
use crate::wrappers::window::{NativeWindow, WindowProps};
use glam::{Mat4, Quat, Vec2, Vec3, Vec4};
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use windows::Win32::Graphics::Direct3D12::{D3D12_CULL_MODE_NONE, D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE, D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT, DXGI_FORMAT_R8G8B8A8_UNORM_SRGB};
use crate::wrappers::graphics::batching::UploadBatch;

pub const SCENE_FORMAT: DXGI_FORMAT = DXGI_FORMAT_R8G8B8A8_UNORM_SRGB;

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct ObjectData {
    pub model: Mat4,
    pub material_id: u32,
    pub _pad: [u32; 3]
}

fn main() -> anyhow::Result<()> {
    let mut window = NativeWindow::create(WindowProps {
        ..Default::default()
    })?;
    let mut time = Time::new()?;

    let mut gd = GraphicsDevice::new()?;
    gd.setup_resources(window.handle(), WIDTH, HEIGHT)?;
    let mut upload_batch = gd.begin_upload()?;

    // white texture must be the first one always !!
    let white_tex = Texture::from_rgba8(&mut upload_batch, 1, 1, &[255; 4], false, "builtin/WHITE")?;

    let mut depth = Texture::depth(&gd, WIDTH, HEIGHT, "SceneDepth")?;
    let mut scene_color = Texture::render_target(&gd, GraphicsDevice::BB_RTV_FORMAT, WIDTH, HEIGHT, Vec4::new(0.4, 0.5, 0.9, 1.), "SceneColor")?;

    let mut shaders = ShaderStorage::new(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/shaders"));
    shaders.compile()?;

    let mut pipeline_mgr = PipelineManager::new(&gd)?;
    let phong_desc = PipelineDesc {
        rts: vec![SCENE_FORMAT],
        name: "Phong",
        shader: "PhongTex.hlsl".into(),
        input: Vertex3::LAYOUT,
        ..Default::default()
    };
    let unlit_desc = PipelineDesc {
        rts: vec![SCENE_FORMAT],
        name: "Unlit",
        shader: "UnlitScroll.hlsl".into(),
        input: Vertex3::LAYOUT,
        ..Default::default()
    };
    let blit_desc = PipelineDesc {
        rts: vec![GraphicsDevice::BB_RTV_FORMAT],
        name: "Blit",
        shader: "Blit.hlsl".into(),
        input: &[],
        depth: None,
        cull: D3D12_CULL_MODE_NONE,
        ..Default::default()
    };
    let phong = pipeline_mgr.create(&gd, &shaders, phong_desc)?;
    let unlit = pipeline_mgr.create(&gd, &shaders, unlit_desc)?;
    let blit = pipeline_mgr.create(&gd, &shaders, blit_desc)?;

    let mut materials: Vec<GpuMaterial> = Vec::new();

    let checker_tex = Texture::from_rgba8(&mut upload_batch, 32, 32, &debug_checker_tex(32, 32), false, "builtin/CHECKER")?;
    // unlit sphere
    let sphere = load_assimp_scene(
        &gd, &mut upload_batch, "assets/models/Sphere.gltf",
        white_tex.srv_ptr(), &mut materials,
        |pos, norm, uv, _col| Vertex3::new(pos, norm, uv),
        |_mat, tex| MaterialParams::ScrollingTex {
            tex,
            color: Vec4::ONE,
            scroll_speed: 1.0,
        }.to_gpu_material()
    )?;

    // sponza !!
    let mut sponza_old = load_assimp_scene(
        &gd, &mut upload_batch, "assets/sponza_old/Sponza.gltf",
        white_tex.srv_ptr(), &mut materials,
        |pos, norm, uv, _col| Vertex3::new(pos, norm, uv),
        |_mat, tex| MaterialParams::Phong {
            tex,
            ambient: Vec3::new(0.05, 0.05, 0.05),
            diffuse: Vec3::ONE,
            specular: Vec3::ONE,
            shininess: 32.0,
            diffuse_k: 0.3,
            specular_k: 0.7,
        }.to_gpu_material()
    )?;

    let materials_buf = gd.create_default_buffer(&mut upload_batch, bytemuck::cast_slice(&materials), D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE, "MaterialBuffer")?;

    upload_batch.finish()?;

    let mut cam = Camera::new(
        WIDTH,
        HEIGHT,
        70.,
        Vec3::new(0., 2., -3.),
        Quat::from_rotation_x(30f32.to_radians()),
    );
    let mut globals = Globals {
        view_proj: cam.view_proj(),
        resolution: Vec2::new(800., 600.),
        time: 0.0,
        delta_time: 0.0,
        _pad1: 0.0,
        _pad2: 0.0,
        camera_pos: cam.pos(),
        light_dir: Vec3::new(-0.4, -0.8, 0.5).normalize(),
        light_col: Vec4::new(1.0, 0.9, 0.9, 1.0),
    };

    let mut camera_y = 0.;
    let mut last_title_update: f32 = 0.;

    'main: loop {
        window.update()?;

        let mut new_size: Option<(u32, u32)> = None;
        for e in window.events().try_iter() {
            match e {
                WindowEvent::Resized { width, height } => {
                    new_size = Some((width, height));
                }
                WindowEvent::CloseRequested => {
                    break 'main;
                }
            }
        }

        if window.input().is_pressed(KeyCode::H) {
            println!("{:?} {:?}", time.delta_time(), time.game_time());
        }

        if window.input().is_held(KeyCode::Left) {
            camera_y -= time.delta_time() * 50.;
            cam.set_rot(Quat::from_rotation_y(camera_y.to_radians()));
        }

        if window.input().is_held(KeyCode::Right) {
            camera_y += time.delta_time() * 50.;
            cam.set_rot(Quat::from_rotation_y(camera_y.to_radians()));
        }

        if window.input().is_held(KeyCode::A) {
            cam.set_pos(cam.pos() - cam.rot().mul_vec3(Vec3::X) * time.delta_time());
        }

        if window.input().is_held(KeyCode::D) {
            cam.set_pos(cam.pos() + cam.rot().mul_vec3(Vec3::X) * time.delta_time());
        }

        if window.input().is_held(KeyCode::W) {
            cam.set_pos(cam.pos() + cam.rot().mul_vec3(Vec3::Z) * time.delta_time());
        }

        if window.input().is_held(KeyCode::S) {
            cam.set_pos(cam.pos() - cam.rot().mul_vec3(Vec3::Z) * time.delta_time());
        }

        if window.input().is_held(KeyCode::Space) {
            cam.set_pos(cam.pos() + Vec3::Y * time.delta_time());
        }

        if window.input().is_held(KeyCode::V) {
            cam.set_pos(cam.pos() - Vec3::Y * time.delta_time());
        }

        if window.input().was_released(KeyCode::F5) {
            // recompile shaders and PSOs
            let r = shaders.recompile().and_then(|_| pipeline_mgr.rebuild_all(&gd, &shaders));
            if let Err(e) = r {
                eprintln!("Error performing a hot-reload:\n{}", e);
            }
        }

        if window.input().was_released(KeyCode::F6) {
            gd.flush_command_queue()?;
            let checker = debug_checker_tex(32, 32);
            let mut batch = gd.begin_upload()?;
            for tex in &mut sponza_old.textures {
                tex.rewrite(&mut batch, 32, 32, &checker)?;
            }
            batch.finish()?;
        }

        if let Some(size) = new_size {
            gd.resize(size.0, size.1)?;
            depth.resize(&gd, size.0, size.1)?;
            scene_color.resize(&gd, size.0, size.1)?;

            globals.resolution = Vec2::new(size.0 as f32, size.1 as f32);
            cam.resize(size.0, size.1);
            globals.view_proj = cam.view_proj();
        }

        // updating globals
        globals.time = time.game_time();
        globals.delta_time = time.delta_time();
        globals.view_proj = cam.view_proj();

        gd.prepare_rt()?;
        gd.set_render_targets(&[&scene_color], Some(&depth))?;
        gd.clear(&scene_color)?;
        gd.clear(&depth)?;

        globals.camera_pos = cam.pos();
        let cmd = gd.command_list()?;

        // frame allocator
        let alloc = &gd.frame_context()?.frame_alloc;

        // globals
        let globals_va = alloc.push(&globals)?;

        let rs = pipeline_mgr.root_sig();
        rs.bind(cmd, globals_va, gd.srv_heap_start()?);

        let mut objects = Vec::new();
        let mut draws = Vec::new();
        // sponza
        for (mesh_idx, model) in &sponza_old.mesh_indices {
            let (mesh, material) = &sponza_old.meshes[*mesh_idx];
            objects.push(ObjectData {
                model: *model,
                material_id: *material,
                _pad: [0; 3]
            });
            draws.push((mesh, phong));
        }
        // sphere
        for (mesh_idx, model) in &sphere.mesh_indices {
            let (mesh, material) = &sphere.meshes[*mesh_idx];
            objects.push(ObjectData {
                model: *model,
                material_id: *material,
                _pad: [0; 3]
            });
            draws.push((mesh, unlit));
        }

        // objects (model + material id)
        let objects_va = alloc.push_slice(&objects, 16)?;
        rs.set_objects(cmd, objects_va);

        // materials (material props)
        rs.set_materials(cmd, unsafe { materials_buf.GetGPUVirtualAddress() });
        let mut bound = None;
        for (i, (mesh, pso)) in draws.iter().enumerate() {
            if bound != Some(*pso) {
                pipeline_mgr.get(*pso).bind(cmd);
                bound = Some(*pso);
            }
            rs.set_draw_index(cmd, i as u32);
            mesh.draw(cmd);
        }

        // blitting scene_color to view
        scene_color.transition(cmd, D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE);
        gd.bind_backbuffer(None)?;
        pipeline_mgr.get(blit).bind(cmd);
        rs.set_draw_index(cmd, scene_color.srv_ptr());
        unsafe {
            cmd.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            cmd.DrawInstanced(3, 1, 0, 0);
        }

        gd.display_rt()?;

        if time.game_time() - 1. > last_title_update {
            last_title_update = time.game_time();
            let title = format!(
                "A DirectX12 Renderer | FPS: {}",
                (1. / time.delta_time()).round()
            );
            window.set_title(title)?;
        }

        time.tick()?;
    }
    gd.flush_command_queue()?;

    drop(window);

    Ok(())
}

#[allow(unused)]
fn cube_mesh(gd: &GraphicsDevice, upload: &mut UploadBatch<'_>) -> anyhow::Result<Mesh> {
    let vertices = [
        Vertex2::new(Vec3::new(-1.0, -1.0, -1.0), Vec4::new(0.0, 1.0, 0.0, 1.0)),
        Vertex2::new(Vec3::new(-1.0, 1.0, -1.0), Vec4::new(1.0, 0.0, 0.0, 1.0)),
        Vertex2::new(Vec3::new(1.0, 1.0, -1.0), Vec4::new(0.0, 0.0, 1.0, 1.0)),
        Vertex2::new(Vec3::new(1.0, -1.0, -1.0), Vec4::new(1.0, 1.0, 0.0, 1.0)),
        Vertex2::new(Vec3::new(-1.0, -1.0, 1.0), Vec4::new(1.0, 0.0, 1.0, 1.0)),
        Vertex2::new(Vec3::new(-1.0, 1.0, 1.0), Vec4::new(0.0, 1.0, 1.0, 1.0)),
        Vertex2::new(Vec3::new(1.0, 1.0, 1.0), Vec4::new(1.0, 1.0, 1.0, 1.0)),
        Vertex2::new(Vec3::new(1.0, -1.0, 1.0), Vec4::new(0.0, 0.0, 0.0, 1.0)),
    ];

    let indices: [u32; 36] = [
        0, 1, 2,  0, 2, 3,
        4, 6, 5,  4, 7, 6,
        4, 5, 1,  4, 1, 0,
        3, 2, 6,  3, 6, 7,
        1, 5, 6,  1, 6, 2,
        4, 0, 3,  4, 3, 7,
    ];

    Mesh::new(gd, upload, &vertices, &indices)
}

fn debug_checker_tex(w: u32, h: u32) -> Vec<u8> {
    (0..w * h)
        .flat_map(|i| {
            let (x, y) = (i % w, i / w);
            if (x / 4 + y / 4) % 2 == 0 { [255, 0, 255, 255] } else { [0, 0, 0, 255] }
        })
        .collect()
}

#[allow(unused)]
fn cube_mesh_phong(gd: &GraphicsDevice, upload: &mut UploadBatch<'_>) -> anyhow::Result<Mesh> {
    let col = Vec4::new(0.5, 0.6, 0.75, 1.);
    let vertices = [
        VertexPhong::new(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(0., 0., -1.), col),
        VertexPhong::new(Vec3::new(-1.0, 1.0, -1.0), Vec3::new(0., 0., -1.), col),
        VertexPhong::new(Vec3::new(1.0, 1.0, -1.0), Vec3::new(0., 0., -1.), col),
        VertexPhong::new(Vec3::new(1.0, -1.0, -1.0), Vec3::new(0., 0., -1.), col),
        VertexPhong::new(Vec3::new(-1., -1., 1.), Vec3::new(0., 0., 1.), col),
        VertexPhong::new(Vec3::new(1., -1., 1.), Vec3::new(0., 0., 1.), col),
        VertexPhong::new(Vec3::new(1., 1., 1.), Vec3::new(0., 0., 1.), col),
        VertexPhong::new(Vec3::new(-1., 1., 1.), Vec3::new(0., 0., 1.), col),
        VertexPhong::new(Vec3::new(-1., -1., 1.), Vec3::new(-1., 0., 0.), col),
        VertexPhong::new(Vec3::new(-1., 1., 1.), Vec3::new(-1., 0., 0.), col),
        VertexPhong::new(Vec3::new(-1., 1., -1.), Vec3::new(-1., 0., 0.), col),
        VertexPhong::new(Vec3::new(-1., -1., -1.), Vec3::new(-1., 0., 0.), col),
        VertexPhong::new(Vec3::new(1., -1., -1.), Vec3::new(1., 0., 0.), col),
        VertexPhong::new(Vec3::new(1., 1., -1.), Vec3::new(1., 0., 0.), col),
        VertexPhong::new(Vec3::new(1., 1., 1.), Vec3::new(1., 0., 0.), col),
        VertexPhong::new(Vec3::new(1., -1., 1.), Vec3::new(1., 0., 0.), col),
        VertexPhong::new(Vec3::new(-1., 1., -1.), Vec3::new(0., 1., 0.), col),
        VertexPhong::new(Vec3::new(-1., 1., 1.), Vec3::new(0., 1., 0.), col),
        VertexPhong::new(Vec3::new(1., 1., 1.), Vec3::new(0., 1., 0.), col),
        VertexPhong::new(Vec3::new(1., 1., -1.), Vec3::new(0., 1., 0.), col),
        VertexPhong::new(Vec3::new(-1., -1., 1.), Vec3::new(0., -1., 0.), col),
        VertexPhong::new(Vec3::new(-1., -1., -1.), Vec3::new(0., -1., 0.), col),
        VertexPhong::new(Vec3::new(1., -1., -1.), Vec3::new(0., -1., 0.), col),
        VertexPhong::new(Vec3::new(1., -1., 1.), Vec3::new(0., -1., 0.), col),
    ];

    let indices: [u32; 36] = [
        0, 1, 2, 0, 2, 3, // front
        4, 5, 6, 4, 6, 7, // back
        8, 9, 10, 8, 10, 11, // left
        12, 13, 14, 12, 14, 15, // right
        16, 17, 18, 16, 18, 19, // top
        20, 21, 22, 20, 22, 23, // bottom
    ];

    Mesh::new(gd, upload, &vertices, &indices)
}

#[allow(unused)]
fn tri_mesh(gd: &GraphicsDevice, upload: &mut UploadBatch<'_>) -> anyhow::Result<Mesh> {
    let vertices = [
        Vertex2 {
            pos: [0.0, 0.5, 0.5],
            col: [1., 0., 0., 1.],
        },
        Vertex2 {
            pos: [0.5, -0.5, 0.5],
            col: [0., 1., 0., 1.],
        },
        Vertex2 {
            pos: [-0.5, -0.5, 0.5],
            col: [0., 0., 1., 1.],
        },
    ];

    let indices = [0, 1, 2];

    Mesh::new(gd, upload, &vertices, &indices)
}
