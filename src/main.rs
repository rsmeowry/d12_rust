mod wrappers;

use windows::core::w;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{AdjustWindowRect, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, PostQuitMessage, RegisterClassExW, ShowWindow, TranslateMessage, CW_USEDEFAULT, MSG, SW_SHOW, WM_DESTROY, WM_SIZE, WNDCLASSEXW, WS_EX_APPWINDOW, WS_OVERLAPPEDWINDOW};
use crate::wrappers::event::WindowEvent;
use crate::wrappers::window::{NativeWindow, WindowProps};

fn main() -> anyhow::Result<()> {
    let window = NativeWindow::create(WindowProps {
        ..Default::default()
    })?;

    let rx = window.events();
    'window: while let Ok(w) = rx.recv() {
        match w {
            WindowEvent::Resized { .. } => {}
            WindowEvent::KeyDown { .. } => {}
            WindowEvent::CloseRequested => {
                break 'window;
            }
        }
        println!("{w:?}");
    }

    drop(window);

    Ok(())
}
