pub mod sponza_lights;
pub mod tesselated_sample;

use crate::app::Debugger;
use crate::render::graph::FrameGraph;
use crate::render::renderer::Renderer;
use crate::time::Time;

pub trait Scene: Sized {
    fn setup(renderer: &mut Renderer) -> anyhow::Result<Self>;
    fn build_graph<'a>(&'a self, renderer: &'a Renderer, time: &Time, wireframe: bool, graph: &mut FrameGraph<'a>, debugger: &mut Debugger) -> anyhow::Result<()>;
}

