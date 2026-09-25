mod wrappers;
mod input;
mod time;
mod render;

use crate::input::KeyCode;
use crate::wrappers::event::WindowEvent;
use crate::wrappers::window::{NativeWindow, WindowProps};
use std::thread::sleep;
use std::time::Duration;
use glam::Vec4;
use crate::time::Time;
use crate::wrappers::graphics::GraphicsDevice;

fn main() -> anyhow::Result<()> {
    let mut window = NativeWindow::create(WindowProps {
        ..Default::default()
    })?;
    let mut time = Time::new()?;

    let mut gd = GraphicsDevice::new()?;
    gd.setup_resources(window.handle(), 800, 600)?;

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
        gd.display_rt()?;

        time.tick()?;
    }

    drop(window);

    Ok(())
}
