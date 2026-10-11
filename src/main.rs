use crate::scenes::testing_culling::CullingScene;

pub mod assets;
pub mod input;
pub mod render;
pub mod time;
pub mod wrappers;
mod app;
mod scenes;

fn main() -> anyhow::Result<()> {
    app::App::<CullingScene>::new()?.run()
}
