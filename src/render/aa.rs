use crate::render::aa::temporal::apply_taa;
use crate::render::renderer::Renderer;

pub mod temporal;

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum AntiAliasing {
    Disabled,
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