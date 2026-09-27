use crate::input::{InputDevice, KeyCode};
use crate::wrappers::event::{InputEvent, WindowEvent};
use flume::{Receiver, Sender};
use num_enum::TryFromPrimitive;
use std::os::raw::c_void;
use std::thread::JoinHandle;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::{
    GetRawInputData, HRAWINPUT, MOUSE_MOVE_ABSOLUTE, RAWINPUT, RAWINPUTDEVICE,
    RAWINPUTDEVICE_FLAGS, RAWINPUTHEADER, RID_INPUT, RIM_TYPEKEYBOARD, RIM_TYPEMOUSE,
    RegisterRawInputDevices,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::PCWSTR;

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
            class: "d12.app".to_string(),
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
    pub input_tx: Sender<InputEvent>,
}

pub struct NativeWindow {
    rx: Receiver<WindowEvent>,
    hwnd: SendHwnd,
    thread_handle: Option<JoinHandle<()>>,
    input_device: InputDevice,
}

impl NativeWindow {
    fn init_internal(
        hinst: HINSTANCE,
        props: WindowProps,
        state: &WindowState,
    ) -> anyhow::Result<SendHwnd> {
        let class_name: Vec<u16> = props
            .class
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let title: Vec<u16> = props
            .title
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
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
            let hwnd = CreateWindowExW(
                WS_EX_APPWINDOW,
                PCWSTR(class_name.as_ptr()),
                PCWSTR(title.as_ptr()),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                win_rect.right - win_rect.left,
                win_rect.bottom - win_rect.top,
                None,
                None,
                Some(hinst),
                Some(state as *const WindowState as *const c_void),
            )?;
            let devices = [
                // mouse
                RAWINPUTDEVICE {
                    usUsagePage: 0x01,
                    usUsage: 0x02,
                    dwFlags: RAWINPUTDEVICE_FLAGS(0),
                    hwndTarget: hwnd,
                },
                // keyboard
                RAWINPUTDEVICE {
                    usUsagePage: 0x01,
                    usUsage: 0x06,
                    dwFlags: RAWINPUTDEVICE_FLAGS(0),
                    hwndTarget: hwnd,
                },
            ];
            RegisterRawInputDevices(&devices, size_of::<RAWINPUTDEVICE>() as u32)?;
            hwnd
        });

        let _ = unsafe { ShowWindow(handle.0, SW_SHOW) };

        Ok(handle)
    }

    pub fn handle(&self) -> HWND {
        self.hwnd.0
    }

    pub fn input(&self) -> &InputDevice {
        &self.input_device
    }

    pub fn create(props: WindowProps) -> anyhow::Result<Self> {
        let (event_tx, event_rx) = flume::unbounded();
        let (input_tx, input_rx) = flume::unbounded();
        let (hwnd_tx, hwnd_rx) = flume::bounded::<anyhow::Result<SendHwnd>>(1);

        let state = WindowState {
            tx: event_tx,
            input_tx,
        };
        let thread_handle = std::thread::spawn(move || {
            let h_instance: HINSTANCE = unsafe {
                HINSTANCE(
                    GetModuleHandleW(None)
                        .expect("Could not get module handle")
                        .0,
                )
            };
            match Self::init_internal(h_instance, props, &state) {
                Ok(hwnd) => {
                    hwnd_tx.send(Ok(hwnd)).unwrap();
                }
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
        let device = InputDevice::new(input_rx);
        Ok(Self {
            hwnd,
            rx: event_rx,
            thread_handle: Some(thread_handle),
            input_device: device,
        })
    }

    pub fn events(&self) -> &Receiver<WindowEvent> {
        &self.rx
    }

    pub fn update(&mut self) -> anyhow::Result<()> {
        self.input_device.update()?;
        Ok(())
    }

    pub fn set_title<S: Into<String>>(&self, title: S) -> anyhow::Result<()> {
        let pcw = PCWSTR(
            title
                .into()
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect::<Vec<_>>()
                .as_ptr(),
        );

        unsafe {
            SetWindowTextW(self.hwnd.0, pcw)?;
        }

        Ok(())
    }
}

impl Drop for NativeWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = PostMessageW(Some(self.hwnd.0), WM_APP_DESTROY, WPARAM(0), LPARAM(0));
        }
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
                let (w, h) = (
                    (lparam.0 & 0xFFFF) as u32,
                    ((lparam.0 >> 16) & 0xFFFF) as u32,
                );
                state
                    .tx
                    .send(WindowEvent::Resized {
                        width: w,
                        height: h,
                    })
                    .expect("Failed to send event");
                LRESULT(0)
            }
            WM_CLOSE => {
                state
                    .tx
                    .send(WindowEvent::CloseRequested)
                    .expect("Failed to send event");
                LRESULT(0)
            }
            WM_APP_DESTROY => {
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            WM_INPUT => {
                let mut raw = RAWINPUT::default();
                let mut size = size_of::<RAWINPUT>() as u32;
                let read = GetRawInputData(
                    HRAWINPUT(lparam.0 as *mut c_void),
                    RID_INPUT,
                    Some(&mut raw as *mut RAWINPUT as *mut c_void),
                    &mut size,
                    size_of::<RAWINPUTHEADER>() as u32,
                );
                if read != u32::MAX {
                    if raw.header.dwType == RIM_TYPEMOUSE.0 {
                        let m = raw.data.mouse;
                        if m.usFlags.0 & MOUSE_MOVE_ABSOLUTE.0 == 0 {
                            state
                                .input_tx
                                .send(InputEvent::MouseDelta {
                                    dx: m.lLastX,
                                    dy: m.lLastY,
                                })
                                .expect("Failed to send input event");
                        }
                    } else if raw.header.dwType == RIM_TYPEKEYBOARD.0 {
                        let k = raw.data.keyboard;
                        let down = (k.Flags as u32 & RI_KEY_BREAK) == 0;
                        state
                            .input_tx
                            .send(InputEvent::Key {
                                key: KeyCode::try_from_primitive(k.VKey as u32)
                                    .unwrap_or(KeyCode::None),
                                down,
                            })
                            .expect("Failed to send input event");
                    }
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_KILLFOCUS => {
                state
                    .input_tx
                    .send(InputEvent::LoseFocus)
                    .expect("Failed to send input event");
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
