pub mod sponza_lights;
pub mod tesselated_sample;
pub mod testing_culling;
pub mod pbr_scene;

use crate::app::RenderOpts;
use crate::render::graph::FrameGraph;
use crate::render::renderer::Renderer;
use crate::time::Time;

pub trait Scene: Sized {
    const NAME: &'static str;

    fn setup(renderer: &mut Renderer) -> anyhow::Result<Self>;
    fn build_graph<'a>(&'a mut self, renderer: &'a Renderer, time: &Time, graph: &mut FrameGraph<'a>, opts: &mut RenderOpts) -> anyhow::Result<()>;
}

