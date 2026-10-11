use crate::assets::shaders::ShaderStorage;
use crate::assets::LoadedScene;
use crate::render::graph::res::GraphResources;
use crate::render::graph::FrameGraph;
use crate::render::mesh::Mesh;
use crate::render::pipeline::desc::PipelineDesc;
use crate::render::pipeline::signature::RootSignature;
use crate::render::pipeline::{Globals, PipelineId, PipelineManager};
use crate::render::texture::{MipCount, Texture};
use crate::wrappers::graphics::GraphicsDevice;
use glam::Mat4;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT, DXGI_FORMAT_R10G10B10A2_UNORM, DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM_SRGB};
use crate::render::camera::Camera;

pub const SCENE_FORMAT: DXGI_FORMAT = DXGI_FORMAT_R16G16B16A16_FLOAT;
pub const GB_NORMALS: DXGI_FORMAT = DXGI_FORMAT_R10G10B10A2_UNORM;
pub const GB_ALBEDO: DXGI_FORMAT = DXGI_FORMAT_R8G8B8A8_UNORM_SRGB;
pub const GB_MATERIAL: DXGI_FORMAT = DXGI_FORMAT_R8G8B8A8_UNORM;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable, PartialEq)]
pub struct ObjectData {
    pub model: Mat4,
    pub material_id: u32,
    pub _pad: [u32; 3]
}

pub struct Renderer {
    pub gd: GraphicsDevice,
    pub gr: GraphResources,
    pub cam: Camera,
    white_tex: Texture,
    pipeline_mgr: PipelineManager,
    shaders: ShaderStorage,
}

impl Renderer {
    pub fn new(hwnd: HWND, width: u32, height: u32, cam: Camera) -> anyhow::Result<Self> {
        let mut gd = GraphicsDevice::new()?;
        gd.setup_resources(hwnd, width, height)?;

        let mut shaders = ShaderStorage::new(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/shaders"));
        shaders.compile()?;

        let mut batch = gd.begin_upload()?;
        let white_tex = Texture::from_rgba8(&mut batch, 1, 1, &[255; 4], false, MipCount::All, "builtin/WHITE")?;
        batch.finish()?;

        Ok(Self {
            pipeline_mgr: PipelineManager::new(&gd)?,
            gr: GraphResources::new(width, height),
            white_tex,
            gd,
            shaders,
            cam
        })
    }

    pub fn white_tex(&self) -> &Texture {
        &self.white_tex
    }

    pub fn rs(&self) -> &RootSignature {
        self.pipeline_mgr.root_sig()
    }

    pub fn pipelines(&self) -> &PipelineManager {
        &self.pipeline_mgr
    }

    pub fn create_pipeline(&mut self, desc: PipelineDesc) -> anyhow::Result<PipelineId> {
        self.pipeline_mgr.create(&self.gd, &self.shaders, desc)
    }

    // recompiles shaders and rebuilds all psos
    pub fn reload_shaders(&mut self) -> anyhow::Result<()> {
        self.shaders.recompile()?;
        self.pipeline_mgr.rebuild_all(&self.gd, &self.shaders)
    }

    pub fn resize(&mut self, width: u32, height: u32) -> anyhow::Result<()> {
        self.gd.resize(width, height)?;
        self.gr.on_resize(&self.gd, width, height)
    }

    // waits for this frame's slot, resets the command list and binds the root signature with globals.
    // can only use frame alloc after this
    pub fn begin_frame(&self, globals: &Globals) -> anyhow::Result<()> {
        self.gd.prepare_rt()?;
        let globals_va = self.gd.frame_context()?.frame_alloc.push(globals)?;
        self.rs().bind(self.gd.command_list()?, globals_va, self.gd.srv_heap_start()?);
        Ok(())
    }

    pub fn upload_objects(&self, objects: &[ObjectData]) -> anyhow::Result<u64> {
        self.gd.frame_context()?.frame_alloc.push_slice(objects, 16)
    }

    pub fn upload_slice(&self, slice: &[u8], align: usize) -> anyhow::Result<u64> {
        self.gd.frame_context()?.frame_alloc.push_slice(slice, align)
    }

    pub fn upload_cbuffer<T: bytemuck::Pod>(&self, buf: &T) -> anyhow::Result<u64> {
        self.gd.frame_context()?.frame_alloc.push(buf)
    }

    // executes the graph and presents
    pub fn execute_graph(&self, graph: FrameGraph) -> anyhow::Result<()> {
        graph.execute(&self.gd, &self.gr)?;
        self.gd.display_rt()
    }
}

impl LoadedScene {
    pub fn to_objects_and_draws(&self) -> (Vec<ObjectData>, Vec<&Mesh>) {
        let mut objects = Vec::new();
        let mut draws = Vec::new();
        for (mesh_idx, model) in &self.mesh_indices {
            let (mesh, material) = &self.meshes[*mesh_idx];
            objects.push(ObjectData {
                model: *model,
                material_id: *material,
                _pad: [0; 3]
            });
            draws.push(mesh);
        }
        (objects, draws)
    }
}
