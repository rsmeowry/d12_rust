use crate::scenes::sponza_lights::SponzaWithLights;
use crate::scenes::testing_culling::CullingScene;

pub mod assets;
pub mod input;
pub mod render;
pub mod time;
pub mod wrappers;
mod app;
mod scenes;
pub mod ui;

fn main() -> anyhow::Result<()> {
    app::App::<CullingScene>::new()?.run()
}
