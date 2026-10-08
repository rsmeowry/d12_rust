pub mod signature;
pub mod desc;
pub mod material;

use crate::assets::shaders::{ShaderStorage, ShaderType};
use crate::render::pipeline::desc::PipelineDesc;
use crate::render::pipeline::signature::RootSignature;
use crate::wrappers::graphics::GraphicsDevice;
use anyhow::anyhow;
use glam::{Mat4, Vec2, Vec3, Vec4};
use windows::core::HSTRING;
use windows::Win32::Graphics::Direct3D::ID3DBlob;
use windows::Win32::Graphics::Direct3D12::{ID3D12Device, ID3D12GraphicsCommandList, ID3D12PipelineState, D3D12_DEPTH_STENCIL_DESC, D3D12_DEPTH_WRITE_MASK_ALL, D3D12_DEPTH_WRITE_MASK_ZERO, D3D12_FILL_MODE_SOLID, D3D12_FILL_MODE_WIREFRAME, D3D12_GRAPHICS_PIPELINE_STATE_DESC, D3D12_INPUT_LAYOUT_DESC, D3D12_RASTERIZER_DESC, D3D12_SHADER_BYTECODE};
use windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PipelineId(usize);

pub struct PipelineManager {
    root_signature: RootSignature,
    pipelines: Vec<(PipelineDesc, Pipeline)>
}

impl PipelineManager {
    pub fn new(gd: &GraphicsDevice) -> anyhow::Result<Self> {
        Ok(Self {
            pipelines: Vec::new(),
            root_signature: RootSignature::new(gd.device())?
        })
    }

    pub fn create(&mut self, gd: &GraphicsDevice, shaders: &ShaderStorage, desc: PipelineDesc) -> anyhow::Result<PipelineId> {
        let pipeline = Pipeline::new(gd.device(), &self.root_signature, shaders, &desc)?;
        self.pipelines.push((desc, pipeline));
        Ok(PipelineId(self.pipelines.len() - 1))
    }

    pub fn get(&self, id: PipelineId) -> &Pipeline {
        &self.pipelines[id.0].1
    }

    pub fn root_sig(&self) -> &RootSignature {
        &self.root_signature
    }

    pub fn rebuild_all(&mut self, gd: &GraphicsDevice, shaders: &ShaderStorage) -> anyhow::Result<()> {
        let rebuilt = self.pipelines.iter()
            .map(|(desc, _)| Pipeline::new(gd.device(), &self.root_signature, shaders, desc))
            .collect::<anyhow::Result<Vec<_>>>()?;
        // flusing because old PSOs are still used by frames in flight
        gd.flush_command_queue()?;
        for ((_, old), new) in self.pipelines.iter_mut().zip(rebuilt) {
            *old = new;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct Pipeline {
    pso: ID3D12PipelineState
}

impl Pipeline {
    fn new(dev: &ID3D12Device, rs: &RootSignature, shaders: &ShaderStorage, desc: &PipelineDesc) -> anyhow::Result<Self> {
        anyhow::ensure!(desc.rts.len() <= 8, "{}: max 8 render targets!!", desc.name);
        let stage = |ty| shaders.get_shader(&desc.shader, ty).map(bytecode).unwrap_or_default();
        let vs = shaders.get_shader(&desc.shader, ShaderType::Vertex)
            .ok_or_else(|| anyhow!("{}: no vertex shader in {}", desc.name, desc.shader.display()))?;

        let mut d = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
            pRootSignature: unsafe { std::mem::transmute_copy(&rs.rs) },
            VS: bytecode(vs),
            PS: stage(ShaderType::Fragment), // пусто = depth-only
            HS: stage(ShaderType::Hull),
            DS: stage(ShaderType::Domain),
            InputLayout: D3D12_INPUT_LAYOUT_DESC {
                pInputElementDescs: desc.input.as_ptr(),
                NumElements: desc.input.len() as u32,
            },
            RasterizerState: D3D12_RASTERIZER_DESC {
                FillMode: if desc.wireframe { D3D12_FILL_MODE_WIREFRAME } else { D3D12_FILL_MODE_SOLID },
                CullMode: desc.cull,
                DepthClipEnable: true.into(),
                ..Default::default()
            },
            SampleMask: u32::MAX,
            PrimitiveTopologyType: desc.topology,
            NumRenderTargets: desc.rts.len() as u32,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            ..Default::default()
        };
        for (i, fmt) in desc.rts.iter().enumerate() {
            d.RTVFormats[i] = *fmt;
            d.BlendState.RenderTarget[i] = desc.blend.to_directx();
        }
        if let Some(depth) = desc.depth {
            d.DSVFormat = depth.format;
            d.DepthStencilState = D3D12_DEPTH_STENCIL_DESC {
                DepthEnable: true.into(),
                DepthWriteMask: if depth.write { D3D12_DEPTH_WRITE_MASK_ALL } else { D3D12_DEPTH_WRITE_MASK_ZERO },
                DepthFunc: depth.func,
                ..Default::default()
            };
        }

        let pso: ID3D12PipelineState = unsafe { dev.CreateGraphicsPipelineState(&d)? };
        unsafe { pso.SetName(&HSTRING::from(format!("Brine::Pipeline_{}", desc.name)))? };
        Ok(Self { pso })
    }

    pub fn bind(&self, cmd: &ID3D12GraphicsCommandList) {
        unsafe { cmd.SetPipelineState(&self.pso) };
    }

    pub fn set_objects(&self, cmd: &ID3D12GraphicsCommandList, ptr: u64) {
        unsafe { cmd.SetGraphicsRootShaderResourceView(3, ptr); }
    }

    pub fn set_draw_index(&self, cmd: &ID3D12GraphicsCommandList, index: u32) {
        unsafe { cmd.SetGraphicsRoot32BitConstant(0, index, 0); }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Globals {
    pub view_proj: Mat4,
    pub resolution: Vec2,
    pub time: f32,
    pub delta_time: f32,
    pub light_dir: Vec3,
    pub _pad1: f32,
    pub camera_pos: Vec3,
    pub _pad2: f32,
    pub light_col: Vec4,
}

#[inline]
pub fn blob_bytes(b: &ID3DBlob) -> &[u8] {
    unsafe { std::slice::from_raw_parts(b.GetBufferPointer().cast(), b.GetBufferSize()) }
}

#[inline]
pub fn bytecode(b: &ID3DBlob) -> D3D12_SHADER_BYTECODE {
    unsafe {
        D3D12_SHADER_BYTECODE {
            pShaderBytecode: b.GetBufferPointer(),
            BytecodeLength: b.GetBufferSize(),
        }
    }
}
