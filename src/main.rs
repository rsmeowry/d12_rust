mod wrappers;
mod input;
mod time;
mod render;

use crate::input::KeyCode;
use crate::render::builtin::Vertex2;
use crate::render::cbuffer::ConstBuffer;
use crate::render::mesh::Mesh;
use crate::render::pipeline::{Globals, Pipeline};
use crate::time::Time;
use crate::wrappers::event::WindowEvent;
use crate::wrappers::graphics::GraphicsDevice;
use crate::wrappers::window::{NativeWindow, WindowProps};
use glam::{Mat4, Vec2, Vec3, Vec4};

fn main() -> anyhow::Result<()> {
    let mut window = NativeWindow::create(WindowProps {
        ..Default::default()
    })?;
    let mut time = Time::new()?;

    let mut gd = GraphicsDevice::new()?;
    gd.setup_resources(window.handle(), 800, 600)?;

    let pipeline = Pipeline::new::<Vertex2>(&gd, include_str!("../assets/shaders/Sample.hlsl"))?;
    let mesh = cube_mesh(&gd)?;

    let view = glam::camera::lh::view::look_at_mat4(Vec3::new(0., 2., -5.), Vec3::ZERO, Vec3::Y);
    let mut proj = glam::camera::lh::proj::directx::perspective(60f32.to_radians(), 800. / 600., 0.1, 100.);
    let mut view_proj = proj * view;
    let mut globals = Globals {
        view_proj,
        resolution: Vec2::new(800., 600.),
        time: 0.0,
        delta_time: 0.0,
    };
    let globals_cbuf = ConstBuffer::<Globals>::new(&gd, &globals)?;

    'main: loop {
        window.update()?;

        let mut new_size: Option<(u32, u32)> = None;
        for e in window.events().try_iter() {
            match e {
                WindowEvent::Resized { width, height } => {
                    new_size = Some((width, height));
                }
                WindowEvent::KeyDown { .. } => {}
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
            proj = glam::camera::lh::proj::directx::perspective(60f32.to_radians(), size.0 as f32 / size.1 as f32, 0.1, 100.);
            view_proj = proj * view;
            globals.view_proj = view_proj;
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

        pipeline.set_model(&cmd, &model);
        mesh.draw(&cmd);
        gd.display_rt()?;

        let title = format!("A DirectX12 Renderer | FPS: {}", (1. / time.delta_time()).round());
        window.set_title(title)?;

        time.tick()?;
    }

    drop(window);

    Ok(())
}

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
        0, 1, 2, 0, 2, 3,
        4, 6, 5, 4, 7, 6,
        4, 5, 1, 4, 1, 0,
        3, 2, 6, 3, 6, 7,
        1, 5, 6, 1, 6, 2,
        4, 0, 3, 4, 3, 7,
    ];

    Mesh::new(gd, &vertices, &indices)
}

fn tri_mesh(gd: &GraphicsDevice) -> anyhow::Result<Mesh> {
    let vertices = [
        Vertex2 { pos: [ 0.0,  0.5, 0.5], col: [1., 0., 0., 1.] },
        Vertex2 { pos: [ 0.5, -0.5, 0.5], col: [0., 1., 0., 1.] },
        Vertex2 { pos: [-0.5, -0.5, 0.5], col: [0., 0., 1., 1.] }
    ];

    let indices = [0, 1, 2];

    Mesh::new(gd, &vertices, &indices)
}