use crate::render::cbuffer::ConstBuffer;
use crate::render::texture::Texture;
use crate::render::vertex::Vertex;
use crate::wrappers::graphics::GraphicsDevice;
use glam::{Mat4, Vec2, Vec3, Vec4};
use std::collections::HashMap;
use std::ffi::c_void;
use std::marker::PhantomData;
use windows::core::{s, PCSTR};
use windows::Win32::Foundation::E_FAIL;
use windows::Win32::Graphics::Direct3D::Fxc::{D3DCompile, D3DCOMPILE_DEBUG};
use windows::Win32::Graphics::Direct3D::{ID3DBlob, ID3DInclude, ID3DInclude_Impl, D3D_INCLUDE_TYPE};
use windows::Win32::Graphics::Direct3D12::{D3D12SerializeRootSignature, ID3D12GraphicsCommandList, ID3D12PipelineState, ID3D12RootSignature, D3D12_BLEND_ONE, D3D12_BLEND_OP_ADD, D3D12_BLEND_ZERO, D3D12_COLOR_WRITE_ENABLE_ALL, D3D12_COMPARISON_FUNC_LESS, D3D12_COMPARISON_FUNC_NEVER, D3D12_CULL_MODE_BACK, D3D12_DEPTH_STENCIL_DESC, D3D12_DEPTH_WRITE_MASK_ALL, D3D12_DESCRIPTOR_RANGE, D3D12_DESCRIPTOR_RANGE_OFFSET_APPEND, D3D12_DESCRIPTOR_RANGE_TYPE_SRV, D3D12_FILL_MODE_SOLID, D3D12_FILTER_MIN_MAG_MIP_LINEAR, D3D12_FLOAT32_MAX, D3D12_GRAPHICS_PIPELINE_STATE_DESC, D3D12_INPUT_LAYOUT_DESC, D3D12_LOGIC_OP_NOOP, D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE, D3D12_RASTERIZER_DESC, D3D12_RENDER_TARGET_BLEND_DESC, D3D12_ROOT_CONSTANTS, D3D12_ROOT_DESCRIPTOR, D3D12_ROOT_DESCRIPTOR_TABLE, D3D12_ROOT_PARAMETER, D3D12_ROOT_PARAMETER_0, D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS, D3D12_ROOT_PARAMETER_TYPE_CBV, D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE, D3D12_ROOT_SIGNATURE_DESC, D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT, D3D12_SHADER_BYTECODE, D3D12_SHADER_VISIBILITY_ALL, D3D12_SHADER_VISIBILITY_PIXEL, D3D12_STATIC_BORDER_COLOR_OPAQUE_BLACK, D3D12_STATIC_SAMPLER_DESC, D3D12_TEXTURE_ADDRESS_MODE_WRAP, D3D_ROOT_SIGNATURE_VERSION_1};
use windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC;

#[derive(Debug)]
pub struct Pipeline<TParam> {
    root_sig: ID3D12RootSignature,
    pso: ID3D12PipelineState,
    _t: PhantomData<TParam>,
}

impl<TParam: bytemuck::Pod> Pipeline<TParam> {
    const PARAM_DWORDS: u32 = {
        assert!(size_of::<TParam>() % 4 == 0 &&
                    size_of::<TParam>() / 4 <= 61, "Per object params too big for root constants!");
        (size_of::<TParam>() / 4) as u32
    };

    pub fn new<TVert: Vertex>(gd: &GraphicsDevice, shader: &str) -> anyhow::Result<Self> {
        let dev = gd.device();

        let srv_range = D3D12_DESCRIPTOR_RANGE {
            RangeType: D3D12_DESCRIPTOR_RANGE_TYPE_SRV,
            NumDescriptors: 1,
            BaseShaderRegister: 0,
            RegisterSpace: 0,
            OffsetInDescriptorsFromTableStart: D3D12_DESCRIPTOR_RANGE_OFFSET_APPEND,
        };

        let params = [
            // b0 - model matrix
            D3D12_ROOT_PARAMETER {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS,
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    Constants: D3D12_ROOT_CONSTANTS { ShaderRegister: 0, RegisterSpace: 0, Num32BitValues: Self::PARAM_DWORDS }
                },
                ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
            },
            // b1 - globals
            D3D12_ROOT_PARAMETER {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_CBV,
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    Descriptor: D3D12_ROOT_DESCRIPTOR { ShaderRegister: 1, RegisterSpace: 0, }
                },
                ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
            },
            // t0 - tex
            D3D12_ROOT_PARAMETER {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE { NumDescriptorRanges: 1, pDescriptorRanges: &srv_range }
                },
                ShaderVisibility: D3D12_SHADER_VISIBILITY_PIXEL,
            },
        ];

        let sampler = D3D12_STATIC_SAMPLER_DESC {
            Filter: D3D12_FILTER_MIN_MAG_MIP_LINEAR,
            AddressU: D3D12_TEXTURE_ADDRESS_MODE_WRAP,
            AddressV: D3D12_TEXTURE_ADDRESS_MODE_WRAP,
            AddressW: D3D12_TEXTURE_ADDRESS_MODE_WRAP,
            MipLODBias: 0.,
            MaxAnisotropy: 1,
            ComparisonFunc: D3D12_COMPARISON_FUNC_NEVER,
            BorderColor: D3D12_STATIC_BORDER_COLOR_OPAQUE_BLACK,
            MinLOD: 0.,
            MaxLOD: D3D12_FLOAT32_MAX,
            ShaderRegister: 0,
            RegisterSpace: 0,
            ShaderVisibility: D3D12_SHADER_VISIBILITY_PIXEL,
        };

        let rs_desc = D3D12_ROOT_SIGNATURE_DESC {
            NumParameters: params.len() as u32,
            pParameters: params.as_ptr(),
            Flags: D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT,
            NumStaticSamplers: 1,
            pStaticSamplers: &sampler,
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

        Ok(Self { root_sig, pso, _t: PhantomData })
    }

    pub fn bind(&self, cmd: &ID3D12GraphicsCommandList, globals: &ConstBuffer<Globals>) {
        unsafe {
            cmd.SetGraphicsRootSignature(&self.root_sig);
            cmd.SetPipelineState(&self.pso);
            cmd.SetGraphicsRootConstantBufferView(1, globals.addr());
        }
    }

    pub fn set_per_obj_data(&self, cmd: &ID3D12GraphicsCommandList, data: &TParam) {
        unsafe {
            cmd.SetGraphicsRoot32BitConstants(0, Self::PARAM_DWORDS, (data as *const TParam).cast(), 0);
        }
    }

    pub fn set_tex(&self, cmd: &ID3D12GraphicsCommandList, tex: &Texture) {
        unsafe {
            cmd.SetGraphicsRootDescriptorTable(2, tex.srv());
        }
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
    pub _pad: f32,
    pub light_col: Vec4,
}

fn compile(src: &str, entry: PCSTR, target: PCSTR) -> anyhow::Result<ID3DBlob> {
    let (mut code, mut err) = (None, None);

    let includes = ShaderIncludes::new();
    let include = ID3DInclude::new(&includes);

    let r = unsafe {
        D3DCompile(
            src.as_ptr() as *const c_void,
            src.len(),
            s!("anonymous"),
            None,
            &*include,
            entry,
            target,
            D3DCOMPILE_DEBUG,
            0,
            &mut code,
            Some(&mut err),
        )
    };

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

macro_rules! shader_files {
    ($($path:literal),* $(,)?) => {
        [$(($path, include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/shaders/", $path)))),*]
    };
}

#[derive(Debug)]
struct ShaderIncludes {
    files: HashMap<&'static str, &'static str>,
}

impl ShaderIncludes {
    pub fn new() -> Self {
        Self {
            files: HashMap::from(shader_files![
                "Core.hlsl",
                "Simple.hlsl",
                "Phong.hlsl",
                "PhongTex.hlsl"
            ]),
        }
    }

    pub fn get(&self, name: &str) -> Option<&'static str> {
        self.files.get(name).copied()
    }
}

#[allow(E0407)]
impl ID3DInclude_Impl for ShaderIncludes {
    fn Open(
        &self,
        _include_type: D3D_INCLUDE_TYPE,
        pfilename: &PCSTR,
        _pparentdata: *const c_void,
        ppdata: *mut *mut c_void,
        pbytes: *mut u32,
    ) -> windows::core::Result<()> {
        let name = unsafe { pfilename.to_string() }.map_err(|_| windows::core::Error::from(E_FAIL))?;
        let name = name.replace('\\', "/");
        let key = name.trim_start_matches("./");

        let src = self.files.get(key).ok_or_else(|| windows::core::Error::from(E_FAIL))?;
        unsafe {
            *ppdata = src.as_ptr() as *mut c_void;
            *pbytes = src.len() as u32;
        }
        Ok(())
    }

    fn Close(&self, _pdata: *const c_void) -> windows::core::Result<()> {
        Ok(())
    }
}