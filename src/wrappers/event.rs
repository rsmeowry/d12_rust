#[derive(Debug, Clone)]
pub enum WindowEvent {
    Resized { width: u32, height: u32 },
    KeyDown { key_code: u32 },
    CloseRequested,
}