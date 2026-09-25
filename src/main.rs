mod wrappers;
mod input;
mod time;
mod render;

use crate::input::KeyCode;
use crate::wrappers::event::WindowEvent;
use crate::wrappers::window::{NativeWindow, WindowProps};
use std::thread::sleep;
use std::time::Duration;
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

        sleep(Duration::from_millis(100)); // workload simulator 2026

        time.tick()?;
    }

    drop(window);

    Ok(())
}
