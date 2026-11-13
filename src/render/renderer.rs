use crate::assets::shaders::ShaderStorage;
use crate::assets::LoadedScene;
use crate::render::camera::Camera;
use crate::render::graph::res::GraphResources;
use crate::render::graph::FrameGraph;
use crate::render::mesh::Mesh;
use crate::render::pipeline::desc::PipelineDesc;
use crate::render::pipeline::signature::RootSignature;
use crate::render::pipeline::{Globals, PipelineId, PipelineManager};
use crate::render::texture::{MipCount, Texture};
use crate::wrappers::graphics::GraphicsDevice;
use glam::{Mat4, Vec2};
use std::cell::Cell;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct3D12::ID3D12GraphicsCommandList;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT, DXGI_FORMAT_R10G10B10A2_UNORM, DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_FORMAT_R16G16_FLOAT, DXGI_FORMAT_R16G16_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM_SRGB};
use crate::app::RenderOpts;
use crate::render::mesh::mesh_pipeline::meshlet::MeshletMesh;

pub const SCENE_FORMAT: DXGI_FORMAT = DXGI_FORMAT_R16G16B16A16_FLOAT;
pub const GB_NORMALS: DXGI_FORMAT = DXGI_FORMAT_R10G10B10A2_UNORM;
pub const GB_ALBEDO: DXGI_FORMAT = DXGI_FORMAT_R8G8B8A8_UNORM_SRGB;
pub const GB_MATERIAL: DXGI_FORMAT = DXGI_FORMAT_R8G8B8A8_UNORM;
pub const GB_VELOCITY: DXGI_FORMAT = DXGI_FORMAT_R16G16_FLOAT;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable, PartialEq, Default)]
pub struct ObjectData {
    pub model: Mat4,
    pub prev_model: Mat4,
    pub material_id: u32,
    pub vb_srv: u32,
    pub ib_srv: u32,
    pub stride: u32,
}

impl ObjectData {
    pub fn new(model: Mat4, material_id: u32, mesh: &Mesh) -> Self {
        Self {
            model,
            prev_model: model,
            material_id,
            vb_srv: mesh.vb_srv(),
            stride: mesh.stride(),
            ib_srv: mesh.ib_srv(),
        }
    }
}

pub struct Renderer {
    pub gd: GraphicsDevice,
    pub gr: GraphResources,
    pub cam: Camera,
    white_tex: Texture,
    gray_tex: Texture,
    flat_normal_tex: Texture,
    pipeline_mgr: PipelineManager,
    shaders: ShaderStorage,
    frame_count: Cell<u32>,
    jitter: Cell<Vec2>,
    globals_va: Cell<u64>,
}

impl Renderer {
    pub fn new(hwnd: HWND, width: u32, height: u32, cam: Camera) -> anyhow::Result<Self> {
        let mut gd = GraphicsDevice::new()?;
        gd.setup_resources(hwnd, width, height)?;

        let mut shaders = ShaderStorage::new(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/shaders"))?;
        shaders.compile()?;

        let mut batch = gd.begin_upload()?;
        let white_tex = Texture::from_rgba8(&mut batch, 1, 1, &[255; 4], false, MipCount::All, "builtin/WHITE")?;
        let gray_tex = Texture::from_rgba8(&mut batch, 1, 1, &[80; 4], false, MipCount::All, "builtin/GRAY")?;
        let flat_normal_tex = Texture::from_rgba8(&mut batch, 1, 1, &[128, 128, 255, 255], false, MipCount::All, "builtin/FLAT_NORMAL")?;
        batch.finish()?;

        let gr = GraphResources::new(width, height, 1.0);

        Ok(Self {
            pipeline_mgr: PipelineManager::new(&gd)?,
            gr,
            white_tex,
            gray_tex,
            flat_normal_tex,
            gd,
            shaders,
            cam,
            frame_count: Cell::new(0),
            jitter: Cell::new(Vec2::ZERO),
            globals_va: Cell::new(0),
        })
    }

    pub fn white_tex(&self) -> &Texture {
        &self.white_tex
    }

    pub fn gray_tex(&self) -> &Texture {
        &self.gray_tex
    }

    pub fn flat_normal_tex(&self) -> &Texture {
        &self.flat_normal_tex
    }

    pub fn rs(&self) -> &RootSignature {
        self.pipeline_mgr.root_sig()
    }

    pub fn pipelines(&self) -> &PipelineManager {
        &self.pipeline_mgr
    }
    
    pub fn frame_count(&self) -> u32 { self.frame_count.get() }

    pub fn set_jitter(&self, j: Vec2) {
        self.jitter.set(j);
    }

    pub fn jitter(&self) -> Vec2 {
        self.jitter.get()
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

    pub fn set_render_scale(&mut self, scale: f32) -> anyhow::Result<()> {
        self.gd.flush_command_queue()?;
        self.gr.change_render_scale(scale, &self.gd)
    }

    // waits for this frame's slot, resets the command list and binds the root signature with globals.
    // can only use frame alloc after this
    pub fn begin_frame(&self, globals: &Globals) -> anyhow::Result<()> {
        self.frame_count.set(self.frame_count.get() + 1);
        self.gd.prepare_rt()?;
        let globals_va = self.gd.frame_context()?.frame_alloc.push(globals)?;
        self.globals_va.set(globals_va);
        self.rs().bind(self.gd.command_list()?, globals_va, self.gd.srv_heap_start()?);
        self.rs().bind_compute(self.gd.command_list()?, globals_va, self.gd.srv_heap_start()?);
        Ok(())
    }
    
    pub fn rebind_root(&self, cmd: &ID3D12GraphicsCommandList) -> anyhow::Result<()> {
        let (va, start) = (self.globals_va.get(), self.gd.srv_heap_start()?);
        unsafe { cmd.SetDescriptorHeaps(&[Some(self.gd.srv_heap()?.raw().clone())]) };
        self.rs().bind(cmd, va, start);
        self.rs().bind_compute(cmd, va, start);
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
    pub fn execute_graph(&self, opts: &RenderOpts, graph: FrameGraph) -> anyhow::Result<()> {
        graph.execute(&self.gd, &self.gr)?;
        self.gd.display_rt(opts)
    }

    pub fn heap_mark(&self) -> anyhow::Result<HeapMark> {
        let srv_mark = self.gd.srv_heap()?.mark();
        let dsv_mark = self.gd.dsv_heap()?.mark();
        let rtv_mark = self.gd.rtv_heap()?.mark();
        let res_mark = self.gr.mark();
        Ok(HeapMark {
            srv_mark,
            dsv_mark,
            rtv_mark,
            res_mark
        })
    }

    pub fn rewind(&mut self, mark: HeapMark) -> anyhow::Result<()> {
        self.gd.srv_heap()?.rewind(mark.srv_mark);
        self.gd.dsv_heap()?.rewind(mark.dsv_mark);
        self.gd.rtv_heap()?.rewind(mark.rtv_mark);
        self.gr.rewind(mark.res_mark);
        Ok(())
    }
}

#[derive(Debug, Copy, Clone)]
pub struct HeapMark {
    pub srv_mark: u32,
    pub dsv_mark: u32,
    pub rtv_mark: u32,
    pub res_mark: u32
}

impl LoadedScene {
    pub fn to_objects_and_draws_lod(&self, lod: usize) -> (Vec<ObjectData>, Vec<&Mesh>) {
        let mut objects = Vec::new();
        let mut draws = Vec::new();
        for (mesh_idx, model) in &self.mesh_indices {
            let (mesh, material) = if self.use_lods && lod > 0 { &self.mesh_lods[*mesh_idx][lod - 1] } else { &self.meshes[*mesh_idx] };
            objects.push(ObjectData::new(*model, *material, mesh));
            draws.push(mesh);
        }
        (objects, draws)
    }

    pub fn to_objects_and_draws(&self) -> (Vec<ObjectData>, Vec<&Mesh>) {
        let mut objects = Vec::new();
        let mut draws = Vec::new();
        for (mesh_idx, model) in &self.mesh_indices {
            let (mesh, material) = &self.meshes[*mesh_idx];
            objects.push(ObjectData::new(*model, *material, mesh));
            draws.push(mesh);
        }
        (objects, draws)
    }

    pub fn to_objects_and_draws_meshlets(&self) -> (Vec<ObjectData>, Vec<(&Mesh, &MeshletMesh)>) {
        let mut objects = Vec::new();
        let mut draws = Vec::new();
        for (mesh_idx, model) in &self.mesh_indices {
            let (mesh, material) = &self.meshes[*mesh_idx];
            objects.push(ObjectData::new(*model, *material, mesh));
            draws.push((mesh, &self.meshlet_meshes[*mesh_idx]));
        }
        (objects, draws)
    }

    pub fn fetch_mesh_lod(&self, idx: usize, lod: usize) -> &Mesh {
        if !self.use_lods || lod == 0 {
            &self.meshes.get(idx).unwrap().0
        } else {
            &self.mesh_lods.get(idx).unwrap().get(lod - 1).unwrap().0
        }
    }
}
