use windows::Win32::Graphics::Direct3D12::{ID3D12Device5, ID3D12GraphicsCommandList4, ID3D12Resource, D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_DESC, D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS, D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS_0, D3D12_ELEMENTS_LAYOUT_ARRAY, D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_PREFER_FAST_BUILD, D3D12_RAYTRACING_ACCELERATION_STRUCTURE_PREBUILD_INFO, D3D12_RAYTRACING_ACCELERATION_STRUCTURE_TYPE_TOP_LEVEL, D3D12_RAYTRACING_INSTANCE_DESC, D3D12_RAYTRACING_INSTANCE_FLAG_TRIANGLE_CULL_DISABLE};
use windows_core::Interface;
use crate::render::graph::FrameGraph;
use crate::render::mesh::Mesh;
use crate::render::renderer::{ObjectData, Renderer};
use crate::wrappers::graphics::{uav_barrier, GraphicsDevice};

#[derive(Debug)]
pub struct Tlas {
    result: ID3D12Resource,
    scratch: ID3D12Resource,
    max_instances: u32
}

impl Tlas {
    pub fn new(gd: &GraphicsDevice, max_instances: u32) -> anyhow::Result<Tlas> {
        let (scratch, result) = unsafe { Self::initial_build_tlas(gd, max_instances)? };

        Ok(Self {
            result,
            scratch,
            max_instances
        })
    }

    pub fn add_build_pass<'a>(&self, objects: &[ObjectData], meshes: &[&Mesh], rd: &'a Renderer, graph: &mut FrameGraph<'a>) -> anyhow::Result<()> {
        if meshes.len() > self.max_instances as usize {
            eprintln!("Too many instances! Optimize your scene!")
        }

        let build_instance_descs = meshes
            .into_iter()
            .zip(objects.into_iter())
            .enumerate()
            .filter(|mesh| mesh.1.0.blas_va().is_some())
            .take(self.max_instances as usize)
            .map(|(id, pair)| Self::build_instance_from_blas_mesh(id as u32, *pair.1, pair.0)).collect::<Vec<_>>();

        let instances_bytes = unsafe {
            std::slice::from_raw_parts(build_instance_descs.as_ptr() as *const u8, size_of_val(build_instance_descs.as_slice()))
        };
        let instances_va = rd.upload_slice(instances_bytes, 16)?;
        let result_va = unsafe { self.result.GetGPUVirtualAddress() };
        let result_barrier = uav_barrier(&self.result);
        let scratch = unsafe { self.scratch.GetGPUVirtualAddress() };

        graph.add_pass("TLAS_Build")
            .has_side_effects()
            .exec(move |ctx| {
                let list = ctx.cmd.cast::<ID3D12GraphicsCommandList4>()?;
                let desc = D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_DESC {
                    DestAccelerationStructureData: result_va,
                    Inputs: D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS {
                        Type: D3D12_RAYTRACING_ACCELERATION_STRUCTURE_TYPE_TOP_LEVEL,
                        Flags: D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_PREFER_FAST_BUILD,
                        NumDescs: build_instance_descs.len() as u32,
                        DescsLayout: D3D12_ELEMENTS_LAYOUT_ARRAY,
                        Anonymous: D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS_0 {
                            InstanceDescs: instances_va
                        },
                    },
                    SourceAccelerationStructureData: 0,
                    ScratchAccelerationStructureData: scratch,
                };
                unsafe {
                    list.BuildRaytracingAccelerationStructure(&desc, None);
                    list.ResourceBarrier(&[result_barrier]);
                }
                Ok(())
            });

        Ok(())
    }

    pub fn tlas_va(&self) -> u64 {
        unsafe { self.result.GetGPUVirtualAddress() }
    }

    fn build_instance_from_blas_mesh(id: u32, object: ObjectData, mesh: &Mesh) -> D3D12_RAYTRACING_INSTANCE_DESC {
        let mat = &object.model.transpose().to_cols_array();
        let inst = D3D12_RAYTRACING_INSTANCE_DESC {
            Transform: [mat[0], mat[1], mat[2], mat[3], mat[4], mat[5], mat[6], mat[7], mat[8], mat[9], mat[10], mat[11]], // 3x4 rows
            _bitfield1: id | (0xFF << 24),
            _bitfield2: (D3D12_RAYTRACING_INSTANCE_FLAG_TRIANGLE_CULL_DISABLE.0 as u32) << 24,
            AccelerationStructure: mesh.blas_va().unwrap(),
        };
        inst
    }

    unsafe fn initial_build_tlas(gd: &GraphicsDevice, max_instances: u32) -> anyhow::Result<(ID3D12Resource, ID3D12Resource)> {
        let as_inputs = D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS {
            Type: D3D12_RAYTRACING_ACCELERATION_STRUCTURE_TYPE_TOP_LEVEL,
            Flags: D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_PREFER_FAST_BUILD,
            NumDescs: max_instances,
            DescsLayout: D3D12_ELEMENTS_LAYOUT_ARRAY,
            Anonymous: D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS_0 {
                InstanceDescs: 0 // will fill out, should be a-okay
            },
        };

        let mut as_build_info = D3D12_RAYTRACING_ACCELERATION_STRUCTURE_PREBUILD_INFO::default();

        let dev = gd.device().cast::<ID3D12Device5>()?;
        dev.GetRaytracingAccelerationStructurePrebuildInfo(&as_inputs, &mut as_build_info);

        let scratch_size = as_build_info.ScratchDataSizeInBytes as usize;
        let res_size = as_build_info.ResultDataMaxSizeInBytes as usize;

        let tlas_scratch = GraphicsDevice::create_uav_buffer(&dev, scratch_size, "TLAS_Scratch")?;
        let tlas_result = GraphicsDevice::create_as_buffer(&dev, res_size, "TLAS_Result")?;

        Ok((tlas_scratch, tlas_result))
    }
}