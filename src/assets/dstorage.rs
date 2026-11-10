pub mod types;

use direct_storage::runtime_loaded::{DStorageCreateCompressionCodec, DStorageGetFactory};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::mem::ManuallyDrop;
use std::path::{Path, PathBuf};
use std::time::Instant;
use direct_storage::{readonly_copy, IDStorageCompressionCodec, IDStorageFactory, IDStorageFile, IDStorageQueue, IDStorageQueue2, DSTORAGE_COMPRESSION_BEST_RATIO, DSTORAGE_COMPRESSION_DEFAULT, DSTORAGE_COMPRESSION_FORMAT, DSTORAGE_COMPRESSION_FORMAT_GDEFLATE, DSTORAGE_COMPRESSION_FORMAT_NONE, DSTORAGE_COMPRESSION_SUPPORT_CPU_FALLBACK, DSTORAGE_COMPRESSION_SUPPORT_GPU_FALLBACK, DSTORAGE_COMPRESSION_SUPPORT_GPU_OPTIMIZED, DSTORAGE_DESTINATION, DSTORAGE_DESTINATION_BUFFER, DSTORAGE_DESTINATION_MULTIPLE_SUBRESOURCES, DSTORAGE_DESTINATION_TEXTURE_REGION, DSTORAGE_MAX_QUEUE_CAPACITY, DSTORAGE_PRIORITY_NORMAL, DSTORAGE_QUEUE_DESC, DSTORAGE_REQUEST, DSTORAGE_REQUEST_DESTINATION_BUFFER, DSTORAGE_REQUEST_DESTINATION_MULTIPLE_SUBRESOURCES, DSTORAGE_REQUEST_DESTINATION_TYPE, DSTORAGE_REQUEST_OPTIONS, DSTORAGE_REQUEST_SOURCE_FILE, DSTORAGE_SOURCE, DSTORAGE_SOURCE_FILE};
use glam::Mat4;
use serde::{Deserialize, Serialize};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Graphics::Direct3D12::{ID3D12Fence, ID3D12Resource, D3D12_FENCE_FLAGS, D3D12_FENCE_FLAG_NONE, D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT;
use windows::Win32::System::LibraryLoader::SetDllDirectoryW;
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject, INFINITE};
use windows_core::{Interface, HSTRING, PCSTR};
use crate::assets::LoadedScene;
use crate::render::mesh::{DrawTopology, Mesh};
use crate::render::mesh::mesh_pipeline::meshlet::MeshletMesh;
use crate::render::perf::frustum::AABB;
use crate::render::pipeline::material::GpuMaterial;
use crate::render::renderer::Renderer;
use crate::render::texture::{Texture, TextureDesc, TextureKind};
use crate::render::vertex::Vertex;
use crate::wrappers::graphics::batching::UploadBatch;
use crate::wrappers::graphics::GraphicsDevice;

#[derive(Debug, Serialize, Deserialize)]
pub struct DSHeader {
    pub textures: Vec<DSTexture>,
    pub meshes: Vec<DSMesh>,
    pub mesh_indices: Vec<(usize, Mat4)>,
    pub materials: Vec<(GpuMaterial, [i16; 8])>, // tex slots, -1 is skip
    pub vertex_stride: u32,
    pub compression: u8
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DSTexture {
    pub name: String,
    pub w: u32,
    pub h: u32,
    pub mips: u32,
    pub format: u32,
    pub srgb: bool,
    pub offset: u32,
    pub size: u32,
    pub stored_size: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DSMesh {
    pub vb: (u32, u32), // offset size
    pub ib: (u32, u32), // offset size
    pub index_count: u32,
    pub aabb_min: [f32; 3],
    pub aabb_max: [f32; 3],
    pub material_idx: u32,
    pub meshlet: Option<DSMeshlet>
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DSMeshlet {
    pub offset: u32,
    pub size: u32,
    pub meshlet_count: u32,
}

#[derive(Debug)]
pub struct DSBlobWriter {
    pub data: Vec<u8>,
    codec: Option<IDStorageCompressionCodec>
}

impl DSBlobWriter {
    pub fn new(use_gdeflate: bool) -> anyhow::Result<Self> {
        let dir = HSTRING::from(std::env::current_dir()?.join("libs").as_os_str());
        unsafe { SetDllDirectoryW(&dir)? };
        Ok(Self {
            data: Vec::new(),
            codec: if use_gdeflate { Some(unsafe { DStorageCreateCompressionCodec(DSTORAGE_COMPRESSION_FORMAT_GDEFLATE, 0)? }) } else { None }
        })
    }

    // offset + size
    pub fn push(&mut self, bytes: &[u8]) -> (u32, u32) {
        let off = self.data.len().next_multiple_of(16);
        self.data.resize(off, 0);
        self.data.extend_from_slice(bytes);
        (off as u32, bytes.len() as u32)
    }

    // also compresses if there is a codec
    pub fn push_compressed(&mut self, bytes: &[u8]) -> anyhow::Result<(u32, u32)> {
        let Some(codec) = &self.codec else {
            return Ok(self.push(bytes))
        };
        let bound = unsafe { codec.CompressBufferBound(bytes.len()) };
        let mut out = vec![0u8; bound];
        let mut n = 0usize;
        unsafe {
            codec.CompressBuffer(
                bytes.as_ptr().cast(),
                bytes.len(),
                DSTORAGE_COMPRESSION_BEST_RATIO,
                out.as_mut_ptr().cast(),
                bound,
                &mut n
            )?
        };
        out.truncate(n);
        Ok(self.push(&out))
    }

    pub fn uses_compression(&self) -> u8 {
        self.codec.is_some() as u8
    }

    pub fn flush(self, header: DSHeader, path: &Path) -> anyhow::Result<()> {
        // magic val (0xBA 0xDC 0x0D 0xE0 ) -> header size -> header postcard -> raw DStorage data
        let header_bytes = postcard::to_allocvec(&header)?;

        let mut file = File::create(path)?;
        let mut writer = BufWriter::new(&mut file);
        writer.write_all(&[0xBA, 0xDC, 0x0D, 0xE0])?;
        writer.write_all(header_bytes.len().to_le_bytes().as_ref())?;
        writer.write_all(&header_bytes)?;
        writer.write_all(&self.data)?;
        writer.flush()?;

        Ok(())
    }
}

#[derive(Debug)]
pub struct DSBlobUploader {
    pub source_path: PathBuf,
}

impl DSBlobUploader {
    pub fn new(source_path: &Path) -> anyhow::Result<Self> {
        let dir = HSTRING::from(std::env::current_dir()?.join("libs").as_os_str());
        unsafe { SetDllDirectoryW(&dir)?; }
        Ok(Self {
            source_path: source_path.into(),
        })
    }

    // just tthe same as load_assimp_scene
    pub fn load_scene<TVert: Vertex>(mut self, renderer: &Renderer, batch: &mut UploadBatch, materials: &mut Vec<GpuMaterial>) -> anyhow::Result<(LoadedScene, f32, f32)> {
        let t_io = Instant::now();

        let gd = &renderer.gd;
        let mut f = File::open(&self.source_path)?;
        let mut read = BufReader::new(&mut f);

        let mut magic = [0x00; 4];
        read.read_exact(&mut magic)?;
        if magic != [0xBA, 0xDC, 0x0D, 0xE0] {
            anyhow::bail!("Wrong magic!");
        }

        let mut hlen_bytes = [0x00; 8];
        read.read_exact(&mut hlen_bytes)?;
        let hlen = u64::from_le_bytes(hlen_bytes) as usize;

        let mut header_bytes = vec![0u8; hlen];
        read.read_exact(&mut header_bytes)?;
        let header: DSHeader = postcard::from_bytes(&header_bytes)?;

        if header.vertex_stride != size_of::<TVert>() as u32 {
            anyhow::bail!("Wrong vertex stride! Did you pick a wrong vertex?");
        }

        // all the rest of the data is loaded from dstorage file, drop this reader
        drop(read);

        let (factory, file, queue) = self.prepare_ds_props(gd)?;

        // preparing resources and requests
        let data_start = (4 + 8 + hlen) as u32;

        // textures
        let mut textures = Vec::with_capacity(header.textures.len());
        for t in &header.textures {
            let tex = Texture::new_streaming(gd, TextureDesc {
                kind: TextureKind::SampleSource,
                width: t.w,
                height: t.h,
                mips: t.mips,
                format: DXGI_FORMAT(t.format as i32),
                is_cube: false,
                depth: 1,
            }, &t.name)?;

            unsafe {
                queue.EnqueueRequest(&Self::dstorage_request(&file, data_start + t.offset, t.stored_size, t.size, DSTORAGE_COMPRESSION_FORMAT(header.compression), DSTORAGE_REQUEST_DESTINATION_MULTIPLE_SUBRESOURCES, DSTORAGE_DESTINATION {
                    MultipleSubresources: ManuallyDrop::new(DSTORAGE_DESTINATION_MULTIPLE_SUBRESOURCES {
                        Resource: readonly_copy(tex.resource()),
                        FirstSubresource: 0
                    })
                }))
            }
            textures.push(tex)
        }

        // meshes
        let mut bufs = Vec::with_capacity(header.meshes.len());
        for (i, m) in header.meshes.iter().enumerate() {
            let vb = GraphicsDevice::create_streaming_buffer(gd.device(), m.vb.1 as usize, format!("DStorage_VB_{i:4X}"))?;
            let ib = GraphicsDevice::create_streaming_buffer(gd.device(), m.ib.1 as usize, format!("DStorage_IB_{i:4X}"))?;

            Self::enqueue_buffer(&queue, &file, data_start, m.vb, &vb);
            Self::enqueue_buffer(&queue, &file, data_start, m.ib, &ib);

            let mb = m.meshlet.as_ref().map(|m| -> anyhow::Result<_> {
                let mb = GraphicsDevice::create_streaming_buffer(gd.device(), m.size as usize, format!("DStorage_ML_{i:4X}"))?;
                Self::enqueue_buffer(&queue, &file, data_start, (m.offset, m.size), &mb);
                Ok(mb)
            }).transpose()?;

            bufs.push((vb, ib, mb));
        }

        // submitting
        let fence: ID3D12Fence = unsafe { gd.device().CreateFence(0, D3D12_FENCE_FLAG_NONE)? };
        let event = unsafe { CreateEventW(None, false, false, None)? };
        unsafe {
            fence.SetEventOnCompletion(1, event)?;
            queue.EnqueueSignal(&fence, 1);
            queue.Submit();
            WaitForSingleObject(event, INFINITE);
            CloseHandle(event)?;
        }
        let err = unsafe { queue.RetrieveErrorRecord() };
        if err.FailureCount != 0 {
            anyhow::bail!("Failed to perform DStorage request: {:?}", err.FirstFailure.HResult);
        }
        let io_elapsed = t_io.elapsed().as_secs_f32() * 1000.;

        // fixing up states + batching
        let t_batch = Instant::now();
        for t in &textures {
            t.transition(batch.cmd(), D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE);
        }
        let mat_base = materials.len();
        for (gpu, slots) in &header.materials {
            let mut m = *gpu;
            for (dst, &s) in m.textures.iter_mut().zip(slots) {
                if s >= 0 {
                    *dst = textures[s as usize].srv_ptr();
                }
            }
            materials.push(m);
        }

        let mut meshes = Vec::new();
        let mut meshlets = Vec::new();
        for (m, (vb, ib, mb)) in header.meshes.iter().zip(bufs) {
            let aabb = AABB { min: m.aabb_min.into(), max: m.aabb_max.into() };
            meshes.push((
                Mesh::new_from_resources::<TVert>(
                    gd,
                    batch,
                    vb,
                    ib,
                    0xD500,
                    m.vb.1,
                    m.index_count,
                    DrawTopology::TriangleList,
                    aabb,
                    true,
                    true)?,
                mat_base as u32 + m.material_idx)
            );
            if let (Some(mb), Some(ml)) = (mb, &m.meshlet) {
                meshlets.push(MeshletMesh::build_resources::<TVert>(mb, ml.meshlet_count)?);
            }
        }

        let batch_elapsed = t_batch.elapsed().as_secs_f32() * 1000.;
        Ok((
            LoadedScene {
                mesh_indices: header.mesh_indices,
                meshes,
                mesh_lods: vec![],
                meshlet_meshes: meshlets,
                textures,
                use_lods: false
            },
            io_elapsed,
            batch_elapsed))
    }

    fn prepare_ds_props(&self, gd: &GraphicsDevice) -> anyhow::Result<(IDStorageFactory, IDStorageFile, IDStorageQueue)> {
        let factory: IDStorageFactory = unsafe { DStorageGetFactory()? };
        let file: IDStorageFile = unsafe { factory.OpenFile(&HSTRING::from(self.source_path.as_os_str()))? };
        let queue: IDStorageQueue = unsafe { factory.CreateQueue(&DSTORAGE_QUEUE_DESC {
            SourceType: DSTORAGE_REQUEST_SOURCE_FILE,
            Capacity: DSTORAGE_MAX_QUEUE_CAPACITY as u16,
            Priority: DSTORAGE_PRIORITY_NORMAL,
            Name: PCSTR::null(),
            Device: readonly_copy(gd.device()),
        })? };
        let support = unsafe {
            queue.cast::<IDStorageQueue2>()?.GetCompressionSupport(DSTORAGE_COMPRESSION_FORMAT_GDEFLATE)
        };
        let supp = match support {
            s if s.0 & DSTORAGE_COMPRESSION_SUPPORT_GPU_OPTIMIZED.0 != 0 => "GPU (optimized)",
            s if s.0 & DSTORAGE_COMPRESSION_SUPPORT_GPU_FALLBACK.0 != 0 => "GPU fallback",
            s if s.0 & DSTORAGE_COMPRESSION_SUPPORT_CPU_FALLBACK.0 != 0 => "CPU fallback",
            _ => "not supported :(",
        };
        println!("GDeflate status: {}", supp);

        Ok((factory, file, queue))
    }

    // https://github.com/hasenbanck/direct-storage-rs/blob/master/examples/hello_direct_storage/main.rs
    unsafe fn dstorage_request(file: &IDStorageFile, offset: u32, stored: u32, uncompressed: u32, compression: DSTORAGE_COMPRESSION_FORMAT, dst_type: DSTORAGE_REQUEST_DESTINATION_TYPE, dst: DSTORAGE_DESTINATION) -> DSTORAGE_REQUEST {
        let mut o = DSTORAGE_REQUEST_OPTIONS::default();
        o.set_CompressionFormat(compression);
        o.set_SourceType(DSTORAGE_REQUEST_SOURCE_FILE);
        o.set_DestinationType(dst_type);
        DSTORAGE_REQUEST {
            Options: o,
            Source: DSTORAGE_SOURCE {
                File: ManuallyDrop::new(DSTORAGE_SOURCE_FILE {
                    Source: readonly_copy(file),
                    Offset: offset as u64,
                    Size: stored
                }
                )
            },
            Destination: dst,
            UncompressedSize: uncompressed,
            CancellationTag: 0,
            Name: PCSTR::null(),
        }
    }

    // enqueues a NONCOMPRESSED(!!) buffer
    fn enqueue_buffer(queue: &IDStorageQueue, file: &IDStorageFile, data_start: u32, (off, size): (u32, u32), res: &ID3D12Resource) {
        unsafe { queue.EnqueueRequest(&Self::dstorage_request(
            file,
            data_start + off,
            size,
            size,
            DSTORAGE_COMPRESSION_FORMAT_NONE,
            DSTORAGE_REQUEST_DESTINATION_BUFFER,
            DSTORAGE_DESTINATION {
                Buffer: ManuallyDrop::new(DSTORAGE_DESTINATION_BUFFER {
                    Resource: readonly_copy(res),
                    Offset: 0,
                    Size: size
                })
            }))
        };
    }
}