mod wrappers;
mod input;
mod time;
mod render;

use crate::input::KeyCode;
use crate::wrappers::event::WindowEvent;
use crate::wrappers::window::{NativeWindow, WindowProps};
use std::thread::sleep;
use std::time::Duration;
use glam::{Mat4, Vec4};
use windows::core::{w, PCWSTR};
use windows::Win32::UI::WindowsAndMessaging::SetWindowTextW;
use crate::render::builtin::Vertex2;
use crate::render::mesh::Mesh;
use crate::render::pipeline::Pipeline;
use crate::time::Time;
use crate::wrappers::graphics::GraphicsDevice;

fn main() -> anyhow::Result<()> {
    let mut window = NativeWindow::create(WindowProps {
        ..Default::default()
    })?;
    let mut time = Time::new()?;

    let mut gd = GraphicsDevice::new()?;
    gd.setup_resources(window.handle(), 800, 600)?;

    let pipeline = Pipeline::new::<Vertex2>(&gd, include_str!("../assets/Sample.hlsl"))?;
    let mesh = Mesh::new::<Vertex2>(&gd, &[
        Vertex2 { pos: [ 0.0,  0.5, 0.5], col: [1., 0., 0., 1.] },
        Vertex2 { pos: [ 0.5, -0.5, 0.5], col: [0., 1., 0., 1.] },
        Vertex2 { pos: [-0.5, -0.5, 0.5], col: [0., 0., 1., 1.] },
    ], &[0, 1, 2])?;

    'main: loop {
        window.update()?;

        for e in window.events().try_iter() {
            match e {
                WindowEvent::Resized { .. } => {}
                WindowEvent::KeyDown { .. } => {}
                WindowEvent::CloseRequested => {
                    break 'main;
                }
            }
        }

        if window.input().is_pressed(KeyCode::H) {
            println!("{:?} {:?}", time.delta_time(), time.game_time());
        }

        // workload simulator 2027
        gd.prepare_rt()?;
        gd.clear_color(Vec4::new(0.3, 0.4, 0.9, 1.))?;

        let cmd = gd.command_list()?;
        pipeline.bind(&cmd, &Mat4::IDENTITY);
        mesh.draw(&cmd);
        gd.display_rt()?;

        let title = format!("A DirectX12 Renderer | FPS: {}", (1. / time.delta_time()).round());
        window.set_title(title)?;

        time.tick()?;
    }

    drop(window);

    Ok(())
}
