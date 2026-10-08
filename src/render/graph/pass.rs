use anyhow::anyhow;
use windows::Win32::Graphics::Direct3D12::ID3D12GraphicsCommandList;
use crate::render::graph::res::{GraphResources, ResId};
use crate::wrappers::graphics::GraphicsDevice;

pub struct Pass<'a> {
    pub name: &'static str,
    pub reads: Vec<ResId>,
    pub writes_color: Vec<(ResId, ResLoad)>,
    pub writes_depth: Option<(ResId, ResLoad)>,
    pub has_side_effects: bool, // if writes to backbuffer
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResLoad {
    Clear,
    Preserve
}

pub enum Target {
    None,
    Texture,
    Backbuffer
}