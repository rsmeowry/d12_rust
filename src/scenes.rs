pub mod sponza_lights;
pub mod tesselated_sample;
pub mod testing_culling;
pub mod pbr_scene;
pub mod pbr_scene_mesh;
pub mod experiment;
pub mod terrain;
pub mod voxels;
pub mod meshlets_scene;

use crate::app::RenderOpts;
use crate::render::graph::FrameGraph;
use crate::render::renderer::Renderer;
use crate::scenes::meshlets_scene::MeshletsScene;
use crate::scenes::pbr_scene::PbrSample;
use crate::scenes::pbr_scene_mesh::PbrSampleMesh;
use crate::scenes::sponza_lights::SponzaWithLights;
use crate::scenes::terrain::TerrainScene;
use crate::scenes::tesselated_sample::TesselatedSample;
use crate::scenes::testing_culling::CullingScene;
use crate::scenes::voxels::VoxelsScene;
use crate::time::Time;

pub trait Scene {
    fn ui(&mut self, ui: &mut egui::Ui) -> anyhow::Result<()>;
    fn setup(renderer: &mut Renderer) -> anyhow::Result<Self> where Self: Sized;
    fn build_graph<'a>(&'a mut self, renderer: &'a Renderer, time: &Time, graph: &mut FrameGraph<'a>, opts: &mut RenderOpts) -> anyhow::Result<()>;
    fn update<'a>(&'a mut self, renderer: &'a mut Renderer, time: &Time) -> anyhow::Result<()>;
}

fn boxed_setup<S: Scene + 'static>(rd: &mut Renderer) -> anyhow::Result<Box<dyn Scene>> {
    Ok(Box::new(S::setup(rd)?))
}

pub type SceneBuild = fn(&mut Renderer) -> anyhow::Result<Box<dyn Scene>>;
pub const SCENES: &[(&str, SceneBuild)] = &[
    ("Sponza", boxed_setup::<SponzaWithLights>),
    ("Voxel terrain", boxed_setup::<VoxelsScene>),
    ("Culling w/ pusheens", boxed_setup::<CullingScene>),
    ("Terrain", boxed_setup::<TerrainScene>),
    ("PBR Spheres", boxed_setup::<PbrSample>),
    ("PBR Mesh", boxed_setup::<PbrSampleMesh>),
    ("PBR Meshlets", boxed_setup::<MeshletsScene>),
    ("Tesselation", boxed_setup::<TesselatedSample>),
];