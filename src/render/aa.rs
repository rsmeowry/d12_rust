use egui_probe::EguiProbe;

pub mod temporal;

#[derive(Debug, Copy, Clone, PartialEq, Default, EguiProbe)]
pub enum AntiAliasing {
    Disabled,
    #[default]
    Temporal
}