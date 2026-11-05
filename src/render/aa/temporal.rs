use glam::Vec2;
use crate::render::renderer::Renderer;
use crate::time::Time;

pub fn halton_sequence(mut index: u32, base: u32) -> f32 {
    let mut f = 1.;
    let mut res = 0.;
    let base_f = base as f32;

    while(index > 0) {
        f /= base_f;
        res += f * (index % base) as f32;
        index /= base;
    }

    res
}

pub fn jitter_vec(frame: u32, phases: u32) -> Vec2 {
    let frame_idx = (frame % phases + 1) as u32;
    Vec2::new(halton_sequence(frame_idx, 2), halton_sequence(frame_idx, 3)) - 0.5
}

pub fn apply_taa(rd: &mut Renderer) {

}