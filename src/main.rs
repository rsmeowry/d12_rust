mod assets;
mod input;
mod render;
mod time;
mod wrappers;

use crate::assets::load_obj_mesh;
use crate::input::KeyCode;
use crate::render::builtin::{Vertex2, Vertex3, VertexPhong};
use crate::render::camera::Camera;
use crate::render::cbuffer::ConstBuffer;
use crate::render::mesh::Mesh;
use crate::render::pipeline::{Globals, Pipeline};
use crate::render::texture::Texture;
use crate::time::Time;
use crate::wrappers::event::WindowEvent;
use crate::wrappers::graphics::GraphicsDevice;
use crate::wrappers::window::{NativeWindow, WindowProps};
use glam::{Mat4, Quat, Vec2, Vec3, Vec4};

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct ObjectPhong {
    pub model: Mat4,

    pub cam_pos: Vec3,
    pub shininess: f32,
    pub ambient_color: Vec4,
    pub diffuse_color: Vec4,
    pub specular_color: Vec4,
}

fn main() -> anyhow::Result<()> {
    let mut window = NativeWindow::create(WindowProps {
        ..Default::default()
    })?;
    let mut time = Time::new()?;

    let mut gd = GraphicsDevice::new()?;
    gd.setup_resources(window.handle(), WIDTH, HEIGHT)?;

    let pipeline = Pipeline::<ObjectPhong>::new::<Vertex3>(
        &gd,
        include_str!("../assets/shaders/PhongTex.hlsl"),
    )?;
    // let mesh = cube_mesh_phong(&gd)?;
    let mesh = load_obj_mesh(
        &gd,
        include_bytes!("../assets/models/pusheen_hi_res.obj"),
        |pos, norm, uv, _col| Vertex3::new(pos, norm, uv),
    )?;
    let tex = Texture::load(&gd, include_bytes!("../assets/textures/pusheen_albedo.png"))?;

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
        _pad: 0.0,
        light_dir: Vec3::new(-0.4, -0.8, 0.5).normalize().into(),
        light_col: Vec4::new(1.0, 0.9, 0.9, 1.0),
    };
    let globals_cbuf = ConstBuffer::<Globals>::new(&gd, &globals)?;

    let mut object_data = ObjectPhong {
        model: Mat4::IDENTITY,
        cam_pos: cam.pos().into(),
        ambient_color: Vec4::new(0.1, 0.1, 0.1, 1.0),
        diffuse_color: Vec4::new(1.0, 1.0, 1.0, 1.0),
        specular_color: Vec4::new(1.0, 0.9, 0.7, 1.0),
        shininess: 128.0,
    };

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

        if let Some(size) = new_size {
            gd.resize(size.0, size.1)?;
            globals.resolution = Vec2::new(size.0 as f32, size.1 as f32);
            cam.resize(size.0, size.1);
            globals.view_proj = cam.view_proj();
        }

        // updating globals
        globals.time = time.game_time();
        globals.delta_time = time.delta_time();
        globals_cbuf.write(&globals);

        // workload simulator 2027
        gd.prepare_rt()?;
        gd.clear_color(Vec4::new(0.3, 0.4, 0.9, 1.))?;

        let cmd = gd.command_list()?;
        pipeline.bind(&cmd, &globals_cbuf);

        let model = Mat4::from_rotation_y(time.game_time());
        object_data.model = model;
        object_data.cam_pos = cam.pos().into();

        pipeline.set_per_obj_data(&cmd, &object_data);
        pipeline.set_tex(&cmd, &tex);
        mesh.draw(&cmd);
        gd.display_rt()?;

        let title = format!(
            "A DirectX12 Renderer | FPS: {}",
            (1. / time.delta_time()).round()
        );
        window.set_title(title)?;

        time.tick()?;
    }

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
        0, 1, 2, 0, 2, 3, 4, 6, 5, 4, 7, 6, 4, 5, 1, 4, 1, 0, 3, 2, 6, 3, 6, 7, 1, 5, 6, 1, 6, 2,
        4, 0, 3, 4, 3, 7,
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
