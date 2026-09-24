mod wrappers;
mod input;

use crate::input::KeyCode;
use crate::wrappers::event::WindowEvent;
use crate::wrappers::window::{NativeWindow, WindowProps};
use std::thread::sleep;
use std::time::Duration;

fn main() -> anyhow::Result<()> {
    let mut window = NativeWindow::create(WindowProps {
        ..Default::default()
    })?;

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

        if window.input().is_held(KeyCode::H) {
            println!("H!");
        }
        if window.input().is_held(KeyCode::W) {
            println!("W!");
        }

        sleep(Duration::from_millis(100));
    }

    drop(window);

    Ok(())
}
