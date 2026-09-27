use glam::Mat4;
use windows::core::{s, PCSTR};
use windows::Win32::Graphics::Direct3D12::{D3D12SerializeRootSignature, ID3D12GraphicsCommandList, ID3D12PipelineState, ID3D12RootSignature, D3D12_BLEND_ONE, D3D12_BLEND_OP_ADD, D3D12_BLEND_ZERO, D3D12_COLOR_WRITE_ENABLE_ALL, D3D12_COMPARISON_FUNC_LESS, D3D12_CULL_MODE_BACK, D3D12_DEPTH_STENCIL_DESC, D3D12_DEPTH_WRITE_MASK_ALL, D3D12_FILL_MODE_SOLID, D3D12_GRAPHICS_PIPELINE_STATE_DESC, D3D12_INPUT_LAYOUT_DESC, D3D12_LOGIC_OP_NOOP, D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE, D3D12_RASTERIZER_DESC, D3D12_RENDER_TARGET_BLEND_DESC, D3D12_ROOT_CONSTANTS, D3D12_ROOT_DESCRIPTOR_TABLE, D3D12_ROOT_PARAMETER, D3D12_ROOT_PARAMETER_0, D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS, D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE, D3D12_ROOT_SIGNATURE_DESC, D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT, D3D12_SHADER_BYTECODE, D3D12_SHADER_VISIBILITY_ALL, D3D12_SHADER_VISIBILITY_VERTEX, D3D_ROOT_SIGNATURE_VERSION_1};
use windows::Win32::Graphics::Direct3D::Fxc::D3DCompile;
use windows::Win32::Graphics::Direct3D::ID3DBlob;
use windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC;
use crate::render::vertex::Vertex;
use crate::wrappers::graphics::GraphicsDevice;

#[derive(Debug)]
pub struct Pipeline {
    root_sig: ID3D12RootSignature,
    pso: ID3D12PipelineState,
}

impl Pipeline {
    pub fn new<TVert: Vertex>(gd: &GraphicsDevice, shader: &str) -> anyhow::Result<Self> {
        let dev = gd.device();

        let param = D3D12_ROOT_PARAMETER {
            ParameterType: D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS,
            Anonymous: D3D12_ROOT_PARAMETER_0 {
                Constants: D3D12_ROOT_CONSTANTS { ShaderRegister: 0, RegisterSpace: 0, Num32BitValues: 16 }
            },
            ShaderVisibility: D3D12_SHADER_VISIBILITY_VERTEX,
        };
        let rs_desc = D3D12_ROOT_SIGNATURE_DESC {
            NumParameters: 1,
            pParameters: &param,
            Flags: D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT,
            ..Default::default()
        };
        let mut blob = None;
        unsafe { D3D12SerializeRootSignature(&rs_desc, D3D_ROOT_SIGNATURE_VERSION_1, &mut blob, None)? };
        let root_sig: ID3D12RootSignature = unsafe {
            dev
                .CreateRootSignature(
                    0,
                    blob_bytes(&blob.ok_or_else(|| anyhow::Error::msg("Failed to serialize root signature"))?
                    ))?
        };

        let vert = compile(shader, s!("vert"), s!("vs_5_1"))?;
        let frag = compile(shader, s!("frag"), s!("ps_5_1"))?;

        let layout = TVert::LAYOUT;

        let mut desc = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
            pRootSignature: unsafe { std::mem::transmute_copy(&root_sig) },
            VS: bytecode(&vert),
            PS: bytecode(&frag),
            InputLayout: D3D12_INPUT_LAYOUT_DESC { pInputElementDescs: layout.as_ptr(), NumElements: layout.len() as u32 },
            RasterizerState: D3D12_RASTERIZER_DESC {
                FillMode: D3D12_FILL_MODE_SOLID, CullMode: D3D12_CULL_MODE_BACK,
                DepthClipEnable: true.into(), ..Default::default()
            },
            DepthStencilState: D3D12_DEPTH_STENCIL_DESC {
                DepthEnable: true.into(), DepthWriteMask: D3D12_DEPTH_WRITE_MASK_ALL,
                DepthFunc: D3D12_COMPARISON_FUNC_LESS, ..Default::default()
            },
            SampleMask: u32::MAX,
            PrimitiveTopologyType: D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
            NumRenderTargets: 1,
            DSVFormat: GraphicsDevice::DS_FORMAT,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            ..Default::default()
        };
        desc.RTVFormats[0] = GraphicsDevice::BB_FORMAT;
        desc.BlendState.RenderTarget[0] = D3D12_RENDER_TARGET_BLEND_DESC {
            SrcBlend: D3D12_BLEND_ONE, DestBlend: D3D12_BLEND_ZERO, BlendOp: D3D12_BLEND_OP_ADD,
            SrcBlendAlpha: D3D12_BLEND_ONE, DestBlendAlpha: D3D12_BLEND_ZERO, BlendOpAlpha: D3D12_BLEND_OP_ADD,
            LogicOp: D3D12_LOGIC_OP_NOOP, RenderTargetWriteMask: D3D12_COLOR_WRITE_ENABLE_ALL.0 as u8,
            ..Default::default()
        };
        let pso = unsafe { dev.CreateGraphicsPipelineState(&desc)? };

        Ok(Self { root_sig, pso })
    }

    pub fn bind(&self, cmd: &ID3D12GraphicsCommandList, mvp: &Mat4) {
        unsafe {
            cmd.SetGraphicsRootSignature(&self.root_sig);
            cmd.SetPipelineState(&self.pso);
            cmd.SetGraphicsRoot32BitConstants(0, 16, mvp.as_ref().as_ptr().cast(), 0);
        }
    }
}

fn compile(src: &str, entry: PCSTR, target: PCSTR) -> anyhow::Result<ID3DBlob> {
    let (mut code, mut err) = (None, None);
    let r = unsafe { D3DCompile(src.as_ptr().cast(), src.len(), PCSTR::null(), None, None,
                                entry, target, 0, 0, &mut code, Some(&mut err)) };
    if r.is_err() {
        let msg = err.map(|e| String::from_utf8_lossy(blob_bytes(&e)).into_owned()).unwrap_or_default();
        anyhow::bail!("Failed to compile shader: {msg}");
    }
    Ok(code.unwrap())
}

#[inline]
fn blob_bytes(b: &ID3DBlob) -> &[u8] {
    unsafe { std::slice::from_raw_parts(b.GetBufferPointer().cast(), b.GetBufferSize()) }
}

#[inline]
fn bytecode(b: &ID3DBlob) -> D3D12_SHADER_BYTECODE {
    unsafe { D3D12_SHADER_BYTECODE { pShaderBytecode: b.GetBufferPointer(), BytecodeLength: b.GetBufferSize() } }
}