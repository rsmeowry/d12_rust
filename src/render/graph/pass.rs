use anyhow::anyhow;
use crate::render::graph::res::{GraphResources, ResId};
use crate::wrappers::graphics::GraphicsDevice;
use windows::Win32::Graphics::Direct3D12::ID3D12GraphicsCommandList;

pub struct Pass<'a> {
    pub name: &'static str,
    pub reads: Vec<ResId>,
    pub writes_color: Vec<(ResId, ResLoad)>,
    pub writes_uav: Vec<(ResId, ResLoad)>,
    pub uses_depth: Option<(ResId, DepthUse, u32)>,
    pub partial_depth: bool,
    pub has_side_effects: bool, // if uses compute buffers or backbuffer
    pub uses_backbuffer: bool, // if writes to backbuffer
    pub exec: Box<dyn FnOnce(&PassCtx) -> anyhow::Result<()> + 'a>,
}

pub struct PassCtx<'a> {
    pub cmd: &'a ID3D12GraphicsCommandList,
    pub gd: &'a GraphicsDevice,
    resources: &'a GraphResources,
}

impl<'a> PassCtx<'a> {
    pub fn new(cmd: &'a ID3D12GraphicsCommandList, gd: &'a GraphicsDevice, resources: &'a GraphResources) -> Self {
        Self { cmd, gd, resources }
    }

    pub fn srv_ptr(&self, id: ResId) -> anyhow::Result<u32> {
        self.resources.get(id).map(|it| it.srv_ptr())
    }

    pub fn uav_ptr(&self, id: ResId) -> anyhow::Result<u32> {
        self.resources.get(id).map(|it| it.uav_ptr())?.ok_or_else(|| anyhow!("Texture {id:?} does not allow for unordered access!"))
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ResLoad {
    Clear,
    Preserve
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DepthUse {
    Write(ResLoad),
    Test
}

pub enum Target {
    None,
    Texture,
    Backbuffer
}