use crate::wrappers::event::InputEvent;
use flume::Receiver;
use glam::Vec2;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct InputDevice {
    mouse_pos: Vec2,
    mouse_delta: Vec2,
    pressed: HashSet<KeyCode>,
    released: HashSet<KeyCode>,
    held: HashSet<KeyCode>,
    rx: Receiver<InputEvent>,
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
            rx,
        }
    }

    fn handle_event(&mut self, event: InputEvent) -> anyhow::Result<()> {
        match event {
            InputEvent::LoseFocus => {
                self.held.clear();
            }
            InputEvent::Key { key, down } => {
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
        }
        Ok(())
    }

    pub(super) fn update(&mut self) -> anyhow::Result<()> {
        self.released.clear();
        self.pressed.clear();
        self.mouse_delta = Vec2::ZERO;

        let events = self.rx.try_iter().collect::<Vec<_>>();
        for ev in events {
            self.handle_event(ev)?;
        }

        self.mouse_pos += self.mouse_delta;

        Ok(())
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
    LeftShift = 160,
    RightShift = 161,
    LeftControl = 162,
    RightControl = 163,
    LeftAlt = 164,
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
}

//#endregion
