use egui_probe::EguiProbe;
use crate::render::aa::temporal::apply_taa;
use crate::render::renderer::Renderer;

pub mod temporal;

#[derive(Debug, Copy, Clone, PartialEq, Default, EguiProbe)]
pub enum AntiAliasing {
    Disabled,
    #[default]
    Temporal
}

impl AntiAliasing {
    pub fn apply(&self, rd: &mut Renderer) {
        match self {
            AntiAliasing::Disabled => {}
            AntiAliasing::Temporal => apply_taa(rd),
        }
    }
}