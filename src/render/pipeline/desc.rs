use std::path::PathBuf;
use windows::Win32::Graphics::Direct3D12::{D3D12_BLEND_INV_SRC_ALPHA, D3D12_BLEND_ONE, D3D12_BLEND_OP_ADD, D3D12_BLEND_SRC_ALPHA, D3D12_COLOR_WRITE_ENABLE_ALL, D3D12_COMPARISON_FUNC, D3D12_COMPARISON_FUNC_LESS, D3D12_CULL_MODE, D3D12_CULL_MODE_BACK, D3D12_INPUT_ELEMENT_DESC, D3D12_PRIMITIVE_TOPOLOGY_TYPE, D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE, D3D12_RENDER_TARGET_BLEND_DESC};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT;
use crate::wrappers::graphics::GraphicsDevice;

#[derive(Debug, Clone)]
pub struct PipelineDesc {
    pub name: &'static str,
    pub shader: PathBuf,
    pub input: &'static [D3D12_INPUT_ELEMENT_DESC], // empty for fullscreen and particles
    pub rts: Vec<DXGI_FORMAT>, // no rts - shadows only
    pub depth: Option<Depth>, // no depth buffer for post
    pub blend: Blend,
    pub cull: D3D12_CULL_MODE,
    pub topology: D3D12_PRIMITIVE_TOPOLOGY_TYPE,
}

#[derive(Debug, Clone, Copy)]
pub struct Depth {
    pub format: DXGI_FORMAT,
    pub func: D3D12_COMPARISON_FUNC,
    pub write: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum Blend { Opaque, Alpha, Additive, Premultiplied }

impl Blend {
    pub fn to_directx(&self) -> D3D12_RENDER_TARGET_BLEND_DESC {
        let (src, dst, src_a, dst_a) = match self {
            Blend::Opaque => return D3D12_RENDER_TARGET_BLEND_DESC {
                BlendEnable: false.into(),
                RenderTargetWriteMask: D3D12_COLOR_WRITE_ENABLE_ALL.0 as u8,
                ..Default::default()
            },
            Blend::Alpha => (D3D12_BLEND_SRC_ALPHA, D3D12_BLEND_INV_SRC_ALPHA, D3D12_BLEND_ONE, D3D12_BLEND_INV_SRC_ALPHA),
            Blend::Additive => (D3D12_BLEND_ONE, D3D12_BLEND_ONE, D3D12_BLEND_ONE, D3D12_BLEND_ONE),
            Blend::Premultiplied => (D3D12_BLEND_ONE, D3D12_BLEND_INV_SRC_ALPHA, D3D12_BLEND_ONE, D3D12_BLEND_INV_SRC_ALPHA),
        };
        D3D12_RENDER_TARGET_BLEND_DESC {
            BlendEnable: true.into(),
            SrcBlend: src, DestBlend: dst, BlendOp: D3D12_BLEND_OP_ADD,
            SrcBlendAlpha: src_a, DestBlendAlpha: dst_a, BlendOpAlpha: D3D12_BLEND_OP_ADD,
            RenderTargetWriteMask: D3D12_COLOR_WRITE_ENABLE_ALL.0 as u8,
            ..Default::default()
        }
    }
}

impl Default for PipelineDesc {
    fn default() -> Self {
        Self {
            name: "unnamed",
            shader: PathBuf::new(),
            input: &[],
            rts: vec![GraphicsDevice::BB_FORMAT],
            depth: Some(Depth { format: GraphicsDevice::DS_FORMAT, func: D3D12_COMPARISON_FUNC_LESS, write: true }),
            blend: Blend::Opaque,
            cull: D3D12_CULL_MODE_BACK,
            topology: D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
        }
    }
}
