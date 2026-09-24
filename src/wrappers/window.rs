use std::os::raw::c_void;
use std::thread::JoinHandle;
use flume::{Receiver, Sender};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use crate::wrappers::event::WindowEvent;

const WM_APP_DESTROY: u32 = WM_APP + 1;

#[derive(Debug, Clone)]
pub struct WindowProps {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub class: String,
}

impl Default for WindowProps {
    fn default() -> Self {
        Self {
            title: "A DirectX 12 Window".to_string(),
            width: 800,
            height: 600,
            class: "d12.app".to_string()
        }
    }
}

#[repr(transparent)]
#[derive(Debug, Copy, Clone)]
pub struct SendHwnd(pub HWND);
unsafe impl Send for SendHwnd {}

#[derive(Debug, Clone)]
struct WindowState {
    pub tx: Sender<WindowEvent>,
}

pub struct NativeWindow {
    rx: Receiver<WindowEvent>,
    hwnd: SendHwnd,
    thread_handle: Option<JoinHandle<()>>
}

impl NativeWindow {
    fn init_internal(hinst: HINSTANCE, props: WindowProps, state: &WindowState) -> anyhow::Result<SendHwnd> {
        let class_name: Vec<u16> = props.class.encode_utf16().chain(std::iter::once(0)).collect();
        let title: Vec<u16> = props.title.encode_utf16().chain(std::iter::once(0)).collect();
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: hinst,
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        let handle = SendHwnd(unsafe {
            RegisterClassExW(&wc);
            let mut win_rect = RECT {
                left: 0,
                top: 0,
                right: props.width as i32,
                bottom: props.height as i32,
            };
            AdjustWindowRect(&mut win_rect, WS_OVERLAPPEDWINDOW, false)?;
            CreateWindowExW(
                WS_EX_APPWINDOW, PCWSTR(class_name.as_ptr()), PCWSTR(title.as_ptr()),
                WS_OVERLAPPEDWINDOW, CW_USEDEFAULT, CW_USEDEFAULT,
                win_rect.right - win_rect.left,
                win_rect.bottom - win_rect.top,
                None, None, Some(hinst), Some(state as *const WindowState as *const c_void))
        }?);

        let _ = unsafe { ShowWindow(handle.0, SW_SHOW) };

        Ok(handle)
    }

    pub fn handle(&self) -> HWND {
        self.hwnd.0
    }

    pub fn create(props: WindowProps) -> anyhow::Result<Self> {
        let (event_tx, event_rx) = flume::unbounded();
        let (hwnd_tx, hwnd_rx) = flume::bounded::<anyhow::Result<SendHwnd>>(1);

        let state = WindowState { tx: event_tx };
        let thread_handle = std::thread::spawn(move || {
            let h_instance: HINSTANCE = unsafe { HINSTANCE(GetModuleHandleW(None).expect("Could not get module handle").0) };
            match Self::init_internal(h_instance, props, &state) {
                Ok(hwnd) => {
                    hwnd_tx.send(Ok(hwnd)).unwrap();
                },
                Err(err) => {
                    hwnd_tx.send(Err(anyhow::Error::from(err))).unwrap();
                    return;
                }
            }

            let mut msg = MSG::default();
            unsafe {
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        });

        let hwnd = hwnd_rx.recv()??;
        Ok(Self { hwnd, rx: event_rx, thread_handle: Some(thread_handle) })
    }

    pub fn events(&self) -> Receiver<WindowEvent> {
        self.rx.clone()
    }
}

impl Drop for NativeWindow {
    fn drop(&mut self) {
        unsafe { let _ = PostMessageW(Some(self.hwnd.0), WM_APP_DESTROY, WPARAM(0), LPARAM(0)); }
        if let Some(t) = self.thread_handle.take() {
            let _ = t.join();
        }
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_NCCREATE {
            let cs = &*(lparam.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
        }

        let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WindowState;

        if msg == WM_NCDESTROY {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        }

        let Some(state) = state.as_ref() else {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        };

        match msg {
            WM_SIZE => {
                let (w, h) = ((lparam.0 & 0xFFFF) as u32, ((lparam.0 >> 16) & 0xFFFF) as u32);
                state.tx.send(WindowEvent::Resized { width: w, height: h }).expect("Failed to send event");
                LRESULT(0)
            }
            WM_KEYDOWN => {
                state.tx.send(WindowEvent::KeyDown { key_code: wparam.0 as u32 }).expect("Failed to send event");
                LRESULT(0)
            }
            WM_CLOSE => {
                state.tx.send(WindowEvent::CloseRequested).expect("Failed to send event");
                LRESULT(0)
            }
            WM_APP_DESTROY => {
                unsafe { let _ = DestroyWindow(hwnd); }
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}