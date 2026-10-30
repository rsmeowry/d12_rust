use crate::scenes::experiment::cloudmarch::CloudmarchScene;
use crate::scenes::pbr_scene::PbrSample;
use crate::scenes::pbr_scene_mesh::PbrSampleMesh;
use crate::scenes::sponza_lights::SponzaWithLights;
use crate::scenes::terrain::TerrainScene;
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
    app::App::<SponzaWithLights>::new()?.run()
}
