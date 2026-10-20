use crate::wrappers::event::InputEvent;
use flume::Receiver;
use glam::Vec2;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::collections::HashSet;
use egui::{pos2, Modifiers, MouseWheelSource, PointerButton, Pos2, TouchPhase};
use egui::MouseWheelUnit::Line;

#[derive(Debug, Clone)]
pub struct InputDevice {
    mouse_pos: Vec2,
    mouse_delta: Vec2,
    pressed: HashSet<KeyCode>,
    released: HashSet<KeyCode>,
    held: HashSet<KeyCode>,
    rx: Receiver<InputEvent>,
    egui_events: Vec<egui::Event>,
    egui_cursor_pos: egui::Pos2
}

#[allow(unused)]
impl InputDevice {
    pub fn new(rx: Receiver<InputEvent>) -> Self {
        Self {
            mouse_pos: Default::default(),
            mouse_delta: Default::default(),
            pressed: Default::default(),
            released: Default::default(),
            held: Default::default(),
            egui_events: Default::default(),
            rx,
            egui_cursor_pos: Default::default()
        }
    }

    fn handle_event(&mut self, event: InputEvent) -> anyhow::Result<()> {
        match event {
            InputEvent::LoseFocus => {
                self.held.clear();
                self.egui_events.push(egui::Event::WindowFocused(false));
            }
            InputEvent::Key { key, down } => {
                let repeat = down && self.held.contains(&key);
                let ctrl = self.held.contains(&KeyCode::LeftControl);
                let modifiers = Modifiers { alt: self.held.contains(&KeyCode::LeftAlt), ctrl, shift: self.held.contains(&KeyCode::LeftShift), mac_cmd: false, command: ctrl };

                let button = match key {
                    KeyCode::LeftButton => Some(egui::PointerButton::Primary),
                    KeyCode::RightButton => Some(egui::PointerButton::Secondary),
                    KeyCode::MiddleButton => Some(egui::PointerButton::Middle),
                    _ => None,
                };
                if let Some(button) = button {
                    self.egui_events.push(egui::Event::PointerButton { pos: self.egui_cursor_pos, button, pressed: down, modifiers });
                }

                let egui_key = match key {
                    KeyCode::Back => Some(egui::Key::Backspace),
                    KeyCode::Delete => Some(egui::Key::Delete),
                    KeyCode::Enter => Some(egui::Key::Enter),
                    KeyCode::Tab => Some(egui::Key::Tab),
                    KeyCode::Left => Some(egui::Key::ArrowLeft),
                    KeyCode::Right => Some(egui::Key::ArrowRight),
                    KeyCode::Up => Some(egui::Key::ArrowUp),
                    KeyCode::Down => Some(egui::Key::ArrowDown),
                    KeyCode::Home => Some(egui::Key::Home),
                    KeyCode::End => Some(egui::Key::End),
                    KeyCode::Escape => Some(egui::Key::Escape),
                    _ => None,
                };
                if let Some(k) = egui_key {
                    self.egui_events.push(egui::Event::Key { key: k, physical_key: None, pressed: down, repeat, modifiers });
                }

                if down && !self.held.contains(&key) {
                    self.pressed.insert(key);
                    self.held.insert(key);
                }
                if !down && self.held.contains(&key) {
                    self.released.insert(key);
                    self.held.remove(&key);
                }
            }
            InputEvent::MouseDelta { dx, dy } => {
                self.mouse_delta += Vec2::new(dx as f32, dy as f32);
            }
            InputEvent::CursorPos { x, y } => {
                self.egui_cursor_pos.x = x as f32;
                self.egui_cursor_pos.y = y as f32;
                self.egui_events.push(egui::Event::PointerMoved(pos2(x as f32, y as f32)));
            }
            InputEvent::Wheel(whl) => {
                self.egui_events.push(egui::Event::MouseWheel { source: MouseWheelSource::Wheel, phase: TouchPhase::Move,  unit: Line, modifiers: Modifiers::default(), delta: egui::Vec2::new(0., whl)})
            }
            InputEvent::Char(c) => {
                self.egui_events.push(egui::Event::Text(c.to_string()))
            }
        }
        Ok(())
    }

    pub(super) fn update(&mut self) -> anyhow::Result<()> {
        self.released.clear();
        self.pressed.clear();
        self.mouse_delta = Vec2::ZERO;
        self.egui_events.clear();

        let events = self.rx.try_iter().collect::<Vec<_>>();
        for ev in events {
            self.handle_event(ev)?;
        }

        self.mouse_pos += self.mouse_delta;

        Ok(())
    }

    pub fn collect_egui_input(&mut self, w: u32, h: u32, time: f32) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Default::default(), egui::Vec2::new(w as f32, h as f32))),
            time: Some(time as f64),
            events: std::mem::take(&mut self.egui_events),
            focused: true,
            ..Default::default()
        }
    }

    pub fn is_pressed(&self, key: KeyCode) -> bool {
        self.pressed.contains(&key)
    }
    pub fn is_held(&self, key: KeyCode) -> bool {
        self.held.contains(&key)
    }
    pub fn was_released(&self, key: KeyCode) -> bool {
        self.released.contains(&key)
    }

    pub fn mouse_pos(&self) -> Vec2 {
        self.mouse_pos
    }
    pub fn mouse_delta(&self) -> Vec2 {
        self.mouse_delta
    }
}

//#region keys enum
#[repr(u32)]
#[derive(
    Copy,
    Clone,
    Debug,
    Ord,
    PartialOrd,
    Eq,
    PartialEq,
    Hash,
    TryFromPrimitive,
    IntoPrimitive,
    Default,
)]
pub enum KeyCode {
    #[default]
    None = 0,

    LeftButton = 500,
    RightButton = 501,
    MiddleButton = 502,
    MouseButtonX1 = 503,
    MouseButtonX2 = 504,
    MouseButtonX3 = 505,
    WheelUp = 506,
    WheelDown = 507,

    Back = 8,
    Tab = 9,
    Enter = 13,
    Pause = 19,
    CapsLock = 20,
    Kana = 21,
    Kanji = 25,
    Escape = 27,
    ImeConvert = 28,
    ImeNoConvert = 29,
    Space = 32,
    PageUp = 33,
    PageDown = 34,
    End = 35,
    Home = 36,
    Left = 37,
    Up = 38,
    Right = 39,
    Down = 40,
    Select = 41,
    Print = 42,
    Execute = 43,
    PrintScreen = 44,
    Insert = 45,
    Delete = 46,
    Help = 47,
    D0 = 48,
    D1 = 49,
    D2 = 50,
    D3 = 51,
    D4 = 52,
    D5 = 53,
    D6 = 54,
    D7 = 55,
    D8 = 56,
    D9 = 57,
    A = 65,
    B = 66,
    C = 67,
    D = 68,
    E = 69,
    F = 70,
    G = 71,
    H = 72,
    I = 73,
    J = 74,
    K = 75,
    L = 76,
    M = 77,
    N = 78,
    O = 79,
    P = 80,
    Q = 81,
    R = 82,
    S = 83,
    T = 84,
    U = 85,
    V = 86,
    W = 87,
    X = 88,
    Y = 89,
    Z = 90,
    LeftWindows = 91,
    RightWindows = 92,
    Apps = 93,
    Sleep = 95,
    NumPad0 = 96,
    NumPad1 = 97,
    NumPad2 = 98,
    NumPad3 = 99,
    NumPad4 = 100,
    NumPad5 = 101,
    NumPad6 = 102,
    NumPad7 = 103,
    NumPad8 = 104,
    NumPad9 = 105,
    Multiply = 106,
    Add = 107,
    Separator = 108,
    Subtract = 109,
    Decimal = 110,
    Divide = 111,
    F1 = 112,
    F2 = 113,
    F3 = 114,
    F4 = 115,
    F5 = 116,
    F6 = 117,
    F7 = 118,
    F8 = 119,
    F9 = 120,
    F10 = 121,
    F11 = 122,
    F12 = 123,
    F13 = 124,
    F14 = 125,
    F15 = 126,
    F16 = 127,
    F17 = 128,
    F18 = 129,
    F19 = 130,
    F20 = 131,
    F21 = 132,
    F22 = 133,
    F23 = 134,
    F24 = 135,
    NumLock = 144,
    Scroll = 145,
    LeftShift = 16,
    RightShift = 161,
    LeftControl = 17,
    RightControl = 163,
    LeftAlt = 18,
    RightAlt = 165,
    BrowserBack = 166,
    BrowserForward = 167,
    BrowserRefresh = 168,
    BrowserStop = 169,
    BrowserSearch = 170,
    BrowserFavorites = 171,
    BrowserHome = 172,
    VolumeMute = 173,
    VolumeDown = 174,
    VolumeUp = 175,
    MediaNextTrack = 176,
    MediaPreviousTrack = 177,
    MediaStop = 178,
    MediaPlayPause = 179,
    LaunchMail = 180,
    SelectMedia = 181,
    LaunchApplication1 = 182,
    LaunchApplication2 = 183,
    OemSemicolon = 186,
    OemPlus = 187,
    OemComma = 188,
    OemMinus = 189,
    OemPeriod = 190,
    OemQuestion = 191,
    OemTilde = 192,
    ChatPadGreen = 202,
    ChatPadOrange = 203,
    OemOpenBrackets = 219,
    OemPipe = 220,
    OemCloseBrackets = 221,
    OemQuotes = 222,
    Oem8 = 223,
    OemBackslash = 226,
    ProcessKey = 229,
    OemCopy = 242,
    OemAuto = 243,
    OemEnlW = 244,
    Attn = 246,
    Crsel = 247,
    Exsel = 248,
    EraseEof = 249,
    Play = 250,
    Zoom = 251,
    Pa1 = 253,
    OemClear = 254,
    Shift,
}

//#endregion
