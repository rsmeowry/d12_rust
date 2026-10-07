mod assets;
mod input;
mod render;
mod time;
mod wrappers;

use crate::assets::load_assimp_scene;
use crate::input::KeyCode;
use crate::render::builtin::{Vertex2, Vertex3, VertexPhong};
use crate::render::camera::Camera;
use crate::render::cbuffer::ConstBuffer;
use crate::render::mesh::Mesh;
use crate::render::pipeline::{Globals, Pipeline, PipelineManager};
use crate::time::Time;
use crate::wrappers::event::WindowEvent;
use crate::wrappers::graphics::GraphicsDevice;
use crate::wrappers::window::{NativeWindow, WindowProps};
use glam::{Mat4, Quat, Vec2, Vec3, Vec4};
use crate::assets::shaders::ShaderStorage;
use crate::render::pipeline::desc::PipelineDesc;
use crate::render::vertex::Vertex;

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct ObjectPhong {
    pub model: Mat4,

    pub ambient_color: Vec4,
    pub diffuse_color: Vec4,
    pub specular_color: Vec4,

    pub shininess: f32,
    pub diffuse_k: f32,
    pub specular_k: f32,
    pub albedo_tex: u32,
}

fn main() -> anyhow::Result<()> {
    let mut window = NativeWindow::create(WindowProps {
        ..Default::default()
    })?;
    let mut time = Time::new()?;

    let mut gd = GraphicsDevice::new()?;
    gd.setup_resources(window.handle(), WIDTH, HEIGHT)?;

    let mut shaders = ShaderStorage::new(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/shaders"));
    shaders.compile()?;

    let mut pipeline_mgr = PipelineManager::new(&gd)?;
    let phong_desc = PipelineDesc {
        name: "Phong",
        shader: "PhongTex.hlsl".into(),
        input: Vertex3::LAYOUT,
        ..Default::default()
    };
    let phong = pipeline_mgr.create(&gd, &shaders, phong_desc)?;

    // sponza !!
    let sponza_old = load_assimp_scene(&gd, "assets/sponza_old/Sponza.gltf", |pos, norm, uv, _col| Vertex3::new(pos, norm, uv))?;

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
    let globals_cbuf = ConstBuffer::<Globals>::new(&gd, &globals)?;

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

        if let Some(size) = new_size {
            gd.resize(size.0, size.1)?;
            globals.resolution = Vec2::new(size.0 as f32, size.1 as f32);
            cam.resize(size.0, size.1);
            globals.view_proj = cam.view_proj();
        }

        // updating globals
        globals.time = time.game_time();
        globals.delta_time = time.delta_time();
        globals.view_proj = cam.view_proj();

        gd.prepare_rt()?;
        gd.clear_color(Vec4::new(0.3, 0.4, 0.9, 1.))?;
        globals.camera_pos = cam.pos();
        globals_cbuf.write(gd.frame_index()?, &globals);
        let cmd = gd.command_list()?;

        // frame allocator
        let alloc = &gd.frame_context()?.frame_alloc;
        let globals_va = alloc.push(&globals)?;
        let rs = pipeline_mgr.root_sig();
        rs.bind(cmd, globals_va, gd.srv_heap_start()?);
        pipeline_mgr.get(phong).bind(cmd);

        let mut objects = Vec::new();
        let mut draws = Vec::new();
        for (mesh_idx, model) in &sponza_old.mesh_indices {
            let (mesh, mat_idx) = &sponza_old.meshes[*mesh_idx];
            let Some(material) = sponza_old.materials.get(mat_idx) else { continue };
            objects.push(ObjectPhong {
                model: *model,
                ambient_color: Vec4::new(0.15, 0.15, 0.15, 1.),
                diffuse_color: Vec4::ONE,
                specular_color: Vec4::ONE,
                shininess: 32.0,
                diffuse_k: 0.4,
                specular_k: 0.7,
                albedo_tex: material.texture.index()
            });
            draws.push(mesh);
        }

        let objects_va = alloc.push_slice(&objects, 16)?;
        rs.set_objects(cmd, objects_va);
        for (i, mesh) in draws.iter().enumerate() {
            rs.set_draw_index(cmd, i as u32);
            mesh.draw(cmd);
        }

        // pipeline.set_per_obj_data(cmd, &object_data);
        // pipeline.set_tex(cmd, &tex);
        // mesh.draw(cmd);

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
fn cube_mesh(gd: &GraphicsDevice) -> anyhow::Result<Mesh> {
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

    Mesh::new(gd, &vertices, &indices)
}

#[allow(unused)]
fn cube_mesh_phong(gd: &GraphicsDevice) -> anyhow::Result<Mesh> {
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

    Mesh::new(gd, &vertices, &indices)
}

#[allow(unused)]
fn tri_mesh(gd: &GraphicsDevice) -> anyhow::Result<Mesh> {
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

    Mesh::new(gd, &vertices, &indices)
}
