use windows::Win32::Graphics::Direct3D12::{D3D12SerializeRootSignature, ID3D12Device, ID3D12GraphicsCommandList, ID3D12RootSignature, D3D12_COMPARISON_FUNC_NEVER, D3D12_DESCRIPTOR_RANGE, D3D12_DESCRIPTOR_RANGE_TYPE_SRV, D3D12_DESCRIPTOR_RANGE_TYPE_UAV, D3D12_FILTER, D3D12_FILTER_ANISOTROPIC, D3D12_FILTER_MIN_MAG_MIP_LINEAR, D3D12_FILTER_MIN_MAG_MIP_POINT, D3D12_FLOAT32_MAX, D3D12_GPU_DESCRIPTOR_HANDLE, D3D12_ROOT_CONSTANTS, D3D12_ROOT_DESCRIPTOR, D3D12_ROOT_DESCRIPTOR_TABLE, D3D12_ROOT_PARAMETER, D3D12_ROOT_PARAMETER_0, D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS, D3D12_ROOT_PARAMETER_TYPE_CBV, D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE, D3D12_ROOT_PARAMETER_TYPE_SRV, D3D12_ROOT_PARAMETER_TYPE_UAV, D3D12_ROOT_SIGNATURE_DESC, D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT, D3D12_SHADER_VISIBILITY_ALL, D3D12_SHADER_VISIBILITY_PIXEL, D3D12_STATIC_BORDER_COLOR_OPAQUE_BLACK, D3D12_STATIC_SAMPLER_DESC, D3D12_TEXTURE_ADDRESS_MODE, D3D12_TEXTURE_ADDRESS_MODE_CLAMP, D3D12_TEXTURE_ADDRESS_MODE_WRAP, D3D_ROOT_SIGNATURE_VERSION_1};
use crate::render::pipeline::blob_bytes;

#[derive(Debug)]
pub struct RootSignature {
    pub rs: ID3D12RootSignature,
}

impl RootSignature {
    pub const DRAW: u32 = 0; // b0 - draw index
    pub const GLOBALS: u32 = 1; // b1 - root CBV
    pub const TEX2D: u32 = 2; // t0 space1 - textures
    pub const OBJECTS: u32 = 3; // t1 - objects data
    pub const TEX2D_ARRAY: u32 = 4; // t0 space2 - texture arrays
    pub const MATERIALS: u32 = 5; // t2 - materials
    pub const ROOT_UAV: u32 = 6; // u0 space0 - root uav buffer
    pub const UAV_TEX2D: u32 = 7; // u0 space1 - writeable tex2ds
    pub const UAV_BUF: u32 = 8; // u0 space2 - RWByteAddressBuffer[]

    pub fn new(dev: &ID3D12Device) -> anyhow::Result<Self> {
        let tex2d_range = D3D12_DESCRIPTOR_RANGE {
            RangeType: D3D12_DESCRIPTOR_RANGE_TYPE_SRV,
            NumDescriptors: u32::MAX, // unlimited !! cus its heap
            BaseShaderRegister: 0,
            RegisterSpace: 1, // space1
            OffsetInDescriptorsFromTableStart: 0,  // spans the wholeee heap
        };

        let tex2d_array_range = D3D12_DESCRIPTOR_RANGE {
            RangeType: D3D12_DESCRIPTOR_RANGE_TYPE_SRV,
            NumDescriptors: u32::MAX, // unlimited !! cus its heap
            BaseShaderRegister: 0,
            RegisterSpace: 2, // space2
            OffsetInDescriptorsFromTableStart: 0,  // spans the wholeee heap
        };

        let uav_tex_range = D3D12_DESCRIPTOR_RANGE {
            RangeType: D3D12_DESCRIPTOR_RANGE_TYPE_UAV,
            NumDescriptors: u32::MAX,
            BaseShaderRegister: 0,
            RegisterSpace: 1,
            OffsetInDescriptorsFromTableStart: 0,
        };

        let uav_buf_range = D3D12_DESCRIPTOR_RANGE {
            RangeType: D3D12_DESCRIPTOR_RANGE_TYPE_UAV,
            NumDescriptors: u32::MAX,
            BaseShaderRegister: 0,
            RegisterSpace: 2,
            OffsetInDescriptorsFromTableStart: 0,
        };

        let params = [
            // b0 - draw index
            D3D12_ROOT_PARAMETER {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS,
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    Constants: D3D12_ROOT_CONSTANTS {
                        ShaderRegister: 0,
                        RegisterSpace: 0,
                        Num32BitValues: 1, // just the draw index
                    },
                },
                ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
            },
            // b1 - globals
            D3D12_ROOT_PARAMETER {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_CBV,
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    Descriptor: D3D12_ROOT_DESCRIPTOR {
                        ShaderRegister: 1,
                        RegisterSpace: 0,
                    },
                },
                ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
            },
            // t0 space1 - tex
            D3D12_ROOT_PARAMETER {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE {
                        NumDescriptorRanges: 1,
                        pDescriptorRanges: &tex2d_range,
                    },
                },
                ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
            },
            // t1 - objects
            D3D12_ROOT_PARAMETER {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_SRV,
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    Descriptor: D3D12_ROOT_DESCRIPTOR {
                        ShaderRegister: 1,
                        RegisterSpace: 0,
                    }
                },
                ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL
            },
            // t0 space2 - tex2d range
            D3D12_ROOT_PARAMETER {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE {
                        NumDescriptorRanges: 1,
                        pDescriptorRanges: &tex2d_array_range,
                    },
                },
                ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
            },
            // t2 - materials
            D3D12_ROOT_PARAMETER {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_SRV,
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    Descriptor: D3D12_ROOT_DESCRIPTOR {
                        ShaderRegister: 2,
                        RegisterSpace: 0,
                    }
                },
                ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL
            },
            // u0 space0 - rw scratch
            D3D12_ROOT_PARAMETER {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_UAV,
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    Descriptor: D3D12_ROOT_DESCRIPTOR { ShaderRegister: 0, RegisterSpace: 0 },
                },
                ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
            },
            // u0 space 1 - rwtex2d
            D3D12_ROOT_PARAMETER {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE {
                        NumDescriptorRanges: 1,
                        pDescriptorRanges: &uav_tex_range
                    }
                },
                ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
            },
            // u0 space 2 - rwbuf
            D3D12_ROOT_PARAMETER {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE {
                        NumDescriptorRanges: 1,
                        pDescriptorRanges: &uav_buf_range
                    }
                },
                ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
            },
        ];

        let samplers = [
            static_sampler(0, D3D12_FILTER_MIN_MAG_MIP_LINEAR, D3D12_TEXTURE_ADDRESS_MODE_WRAP),
            static_sampler(1, D3D12_FILTER_MIN_MAG_MIP_LINEAR, D3D12_TEXTURE_ADDRESS_MODE_CLAMP),
            static_sampler(2, D3D12_FILTER_MIN_MAG_MIP_POINT, D3D12_TEXTURE_ADDRESS_MODE_CLAMP),
            D3D12_STATIC_SAMPLER_DESC {
                MaxAnisotropy: 16,
                ..static_sampler(3, D3D12_FILTER_ANISOTROPIC, D3D12_TEXTURE_ADDRESS_MODE_WRAP)
            },
        ];
        let rs_desc = D3D12_ROOT_SIGNATURE_DESC {
            NumParameters: params.len() as u32,
            pParameters: params.as_ptr(),
            NumStaticSamplers: samplers.len() as u32,
            pStaticSamplers: samplers.as_ptr(),
            Flags: D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT,
        };

        let (mut blob, mut err) = (None, None);
        let r = unsafe {
            D3D12SerializeRootSignature(&rs_desc, D3D_ROOT_SIGNATURE_VERSION_1, &mut blob, Some(&mut err))
        };
        if let Err(e) = r {
            let msg = err.map(|e| String::from_utf8_lossy(blob_bytes(&e)).into_owned()).unwrap_or_default();
            anyhow::bail!("Error in root signature: {e} {msg}");
        }
        let rs = unsafe { dev.CreateRootSignature(0, blob_bytes(&blob.unwrap()))? };
        Ok(Self { rs })
    }

    pub fn bind(&self, cmd: &ID3D12GraphicsCommandList, globals_va: u64, heap_start: D3D12_GPU_DESCRIPTOR_HANDLE) {
        unsafe {
            cmd.SetGraphicsRootSignature(&self.rs);
            cmd.SetGraphicsRootConstantBufferView(Self::GLOBALS, globals_va);
            cmd.SetGraphicsRootDescriptorTable(Self::TEX2D, heap_start);
            cmd.SetGraphicsRootDescriptorTable(Self::TEX2D_ARRAY, heap_start);
            cmd.SetGraphicsRootDescriptorTable(Self::UAV_TEX2D, heap_start);
            cmd.SetGraphicsRootDescriptorTable(Self::UAV_BUF, heap_start);
        }
    }

    pub fn bind_compute(&self, cmd: &ID3D12GraphicsCommandList, globals_va: u64, heap_start: D3D12_GPU_DESCRIPTOR_HANDLE) {
        unsafe {
            cmd.SetComputeRootSignature(&self.rs);
            cmd.SetComputeRootConstantBufferView(Self::GLOBALS, globals_va);
            cmd.SetComputeRootDescriptorTable(Self::TEX2D, heap_start);
            cmd.SetComputeRootDescriptorTable(Self::TEX2D_ARRAY, heap_start);
            cmd.SetComputeRootDescriptorTable(Self::UAV_TEX2D, heap_start);
            cmd.SetComputeRootDescriptorTable(Self::UAV_BUF, heap_start);
        }
    }

    pub fn set_objects(&self, cmd: &ID3D12GraphicsCommandList, va: u64) {
        unsafe { cmd.SetGraphicsRootShaderResourceView(Self::OBJECTS, va) };
    }

    pub fn set_materials(&self, cmd: &ID3D12GraphicsCommandList, va: u64) {
        unsafe { cmd.SetGraphicsRootShaderResourceView(Self::MATERIALS, va) };
    }

    pub fn set_draw_index(&self, cmd: &ID3D12GraphicsCommandList, index: u32) {
        unsafe { cmd.SetGraphicsRoot32BitConstant(Self::DRAW, index, 0) };
    }
}

fn static_sampler(reg: u32, filter: D3D12_FILTER, addr: D3D12_TEXTURE_ADDRESS_MODE) -> D3D12_STATIC_SAMPLER_DESC {
    D3D12_STATIC_SAMPLER_DESC {
        Filter: filter,
        AddressU: addr, AddressV: addr, AddressW: addr,
        MaxAnisotropy: 1,
        ComparisonFunc: D3D12_COMPARISON_FUNC_NEVER,
        BorderColor: D3D12_STATIC_BORDER_COLOR_OPAQUE_BLACK,
        MaxLOD: D3D12_FLOAT32_MAX,
        ShaderRegister: reg,
        ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
        ..Default::default()
    }
}