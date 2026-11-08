pub mod meshlet;

use crate::assets::shaders::{ShaderStorage, ShaderType};
use crate::render::pipeline::bytecode;
use crate::render::pipeline::desc::PipelineDesc;
use crate::render::pipeline::signature::RootSignature;
use std::ffi::c_void;
use windows::Win32::Graphics::Direct3D12::{ID3D12Device, ID3D12Device14, ID3D12PipelineState, D3D12_BLEND_DESC, D3D12_DEPTH_STENCIL_DESC, D3D12_DEPTH_WRITE_MASK_ALL, D3D12_DEPTH_WRITE_MASK_ZERO, D3D12_FILL_MODE_SOLID, D3D12_FILL_MODE_WIREFRAME, D3D12_PIPELINE_STATE_STREAM_DESC, D3D12_PIPELINE_STATE_SUBOBJECT_TYPE, D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_AS, D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_BLEND, D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_DEPTH_STENCIL, D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_DEPTH_STENCIL_FORMAT, D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_MS, D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_PS, D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_RASTERIZER, D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_RENDER_TARGET_FORMATS, D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_ROOT_SIGNATURE, D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_SAMPLE_DESC, D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_SAMPLE_MASK, D3D12_RASTERIZER_DESC, D3D12_RT_FORMAT_ARRAY, D3D12_SHADER_BYTECODE};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT, DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC};
use windows_core::{Interface, HSTRING};

pub fn create_mesh_pipeline_state(dev: &ID3D12Device, rs: &RootSignature, shaders: &ShaderStorage, desc: &PipelineDesc) -> anyhow::Result<ID3D12PipelineState> {
    let dev = dev.cast::<ID3D12Device14>()?;
    let ps = shaders.get_shader(&desc.shader, ShaderType::Fragment).map(bytecode).expect("No fragment shader in mesh pipeline!");
    let amp = shaders.get_shader(&desc.shader, ShaderType::Amplification).map(bytecode).unwrap_or_default();
    let ms = shaders.get_shader(&desc.shader, ShaderType::Mesh).map(bytecode).expect("No mesh shader in mesh pipeline!");
    let depth = desc.depth.expect("No depth desc in mesh pipeline!");
    let mut rtv_formats = [DXGI_FORMAT_UNKNOWN; 8];
    for i in 0..desc.rts.len() {
        rtv_formats[i] = desc.rts[i];
    }
    let mut stream = MeshPsoStream {
        rs: SubResource {
            ty: D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_ROOT_SIGNATURE,
            data: unsafe { std::mem::transmute_copy(&rs.rs) },
        },
        amp: SubResource {
            ty: D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_AS,
            data: amp,
        },
        ms: SubResource {
            ty: D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_MS,
            data: ms,
        },
        ps: SubResource {
            ty: D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_PS,
            data: ps,
        },
        raster: SubResource {
            ty: D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_RASTERIZER,
            data: D3D12_RASTERIZER_DESC {
                FillMode: if desc.wireframe { D3D12_FILL_MODE_WIREFRAME } else { D3D12_FILL_MODE_SOLID },
                CullMode: desc.cull,
                DepthClipEnable: desc.depth_clip.into(),
                DepthBias: desc.depth_bias,
                SlopeScaledDepthBias: desc.slope_bias,
                ..Default::default()
            },
        },
        blend: SubResource {
            ty: D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_BLEND,
            data: D3D12_BLEND_DESC {
                AlphaToCoverageEnable: Default::default(),
                IndependentBlendEnable: Default::default(),
                RenderTarget: [desc.blend.to_directx(); 8],
            },
        },
        depth: SubResource {
            ty: D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_DEPTH_STENCIL,
            data: D3D12_DEPTH_STENCIL_DESC {
                DepthEnable: true.into(),
                DepthWriteMask: if depth.write { D3D12_DEPTH_WRITE_MASK_ALL } else { D3D12_DEPTH_WRITE_MASK_ZERO },
                DepthFunc: depth.func,
                ..Default::default()
            },
        },
        dsv: SubResource {
            ty: D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_DEPTH_STENCIL_FORMAT,
            data: depth.format,
        },
        rtvs: SubResource {
            ty: D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_RENDER_TARGET_FORMATS,
            data: D3D12_RT_FORMAT_ARRAY {
                RTFormats: rtv_formats,
                NumRenderTargets: desc.rts.len() as u32,
            },
        },
        sample: SubResource {
            ty: D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_SAMPLE_DESC,
            data: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        },
        mask: SubResource {
            ty: D3D12_PIPELINE_STATE_SUBOBJECT_TYPE_SAMPLE_MASK,
            data: u32::MAX,
        },
    };
    let stream_desc = D3D12_PIPELINE_STATE_STREAM_DESC {
        SizeInBytes: size_of::<MeshPsoStream>(),
        pPipelineStateSubobjectStream: ((&mut stream) as *mut _) as *mut c_void,
    };
    unsafe {
        let pipeline: ID3D12PipelineState = dev.CreatePipelineState(&stream_desc as *const _)?;
        pipeline.SetName(&HSTRING::from(format!("Brine::MeshPipeline_{}", desc.name)))?;
        return Ok(pipeline)
    }
}

#[repr(C, align(8))]
#[derive(Debug, Copy, Clone)]
pub struct SubResource<T> {
    ty: D3D12_PIPELINE_STATE_SUBOBJECT_TYPE,
    data: T
}

// genuinely stupid
// https://docs.xbox.com/build/core-features/graphics/newfeatures/ms/ms-api-design
#[repr(C)]
struct MeshPsoStream {
    rs: SubResource<*mut c_void>,              // ROOT_SIGNATURE, raw-указатель
    amp: SubResource<D3D12_SHADER_BYTECODE>,    // AS; пустой байткод = AS нет
    ms: SubResource<D3D12_SHADER_BYTECODE>,
    ps: SubResource<D3D12_SHADER_BYTECODE>,
    raster: SubResource<D3D12_RASTERIZER_DESC>,
    blend: SubResource<D3D12_BLEND_DESC>,
    depth: SubResource<D3D12_DEPTH_STENCIL_DESC>,
    dsv: SubResource<DXGI_FORMAT>,
    rtvs: SubResource<D3D12_RT_FORMAT_ARRAY>,
    sample: SubResource<DXGI_SAMPLE_DESC>,
    mask: SubResource<u32>,
}