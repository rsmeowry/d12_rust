use crate::input::KeyCode;

#[derive(Debug, Clone)]
pub enum WindowEvent {
    Resized { width: u32, height: u32 },
    CloseRequested,
}

#[derive(Debug, Clone)]
pub enum InputEvent {
    LoseFocus,
    Key { key: KeyCode, down: bool },
    MouseDelta { dx: i32, dy: i32 },
    CursorPos { x: i32, y: i32 },
    Wheel(f32),
    Char(char)
}
