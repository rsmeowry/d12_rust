use std::ptr::null;
use windows::core::{w, PCSTR, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{AdjustWindowRect, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, PostQuitMessage, RegisterClassExW, ShowWindow, TranslateMessage, CW_USEDEFAULT, MSG, SW_SHOW, WM_DESTROY, WM_SIZE, WNDCLASSEXW, WNDPROC, WS_EX_APPWINDOW, WS_OVERLAPPEDWINDOW};

fn main() -> anyhow::Result<()> {
    let window_handle = unsafe {
        let h_instance: HINSTANCE = HINSTANCE(GetModuleHandleW(None)?.0);
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: h_instance,
            lpszClassName: w!("d12_rust"),
            ..Default::default()
        };
        RegisterClassExW(&wc);
        let mut win_rect = RECT {
            left: 0,
            top: 0,
            right: 800,
            bottom: 600,
        };
        AdjustWindowRect(&mut win_rect, WS_OVERLAPPEDWINDOW, false)?;
        CreateWindowExW(
            WS_EX_APPWINDOW, w!("d12_rust"), w!("Test Window D12"),
            WS_OVERLAPPEDWINDOW, CW_USEDEFAULT, CW_USEDEFAULT,
            win_rect.right - win_rect.left,
            win_rect.bottom - win_rect.top,
            None, None, Some(h_instance), None)?
    };

    unsafe { let _ = ShowWindow(window_handle, SW_SHOW); };

    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) }
            LRESULT(0)
        }
        WM_SIZE => {
            let (w, h) = ((lparam.0 & 0xFFFF) as u32, ((lparam.0 >> 16) & 0xFFFF) as u32);
            LRESULT(0)
        }
        _ => {
            unsafe {
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
    }
}