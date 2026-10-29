use crate::wrappers::graphics::batching::UploadBatch;
use crate::wrappers::graphics::{transition_barrier, uav_barrier, GraphicsDevice};
use anyhow::anyhow;
use glam::Vec4;
use image::{DynamicImage, EncodableLayout, GrayImage, ImageBuffer, Rgba, Rgba32FImage, RgbaImage};
use std::cell::Cell;
use windows::core::HSTRING;
use windows::Win32::Graphics::Direct3D12::{ID3D12GraphicsCommandList, ID3D12Resource, D3D12_CLEAR_VALUE, D3D12_CLEAR_VALUE_0, D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING, D3D12_DEPTH_STENCIL_VALUE, D3D12_DEPTH_STENCIL_VIEW_DESC, D3D12_DEPTH_STENCIL_VIEW_DESC_0, D3D12_DSV_DIMENSION_TEXTURE2D, D3D12_DSV_DIMENSION_TEXTURE2DARRAY, D3D12_DSV_FLAG_NONE, D3D12_DSV_FLAG_READ_ONLY_DEPTH, D3D12_HEAP_FLAG_NONE, D3D12_HEAP_PROPERTIES, D3D12_HEAP_TYPE_DEFAULT, D3D12_PLACED_SUBRESOURCE_FOOTPRINT, D3D12_RESOURCE_DESC, D3D12_RESOURCE_DIMENSION_TEXTURE2D, D3D12_RESOURCE_DIMENSION_TEXTURE3D, D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL, D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET, D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS, D3D12_RESOURCE_FLAG_NONE, D3D12_RESOURCE_STATES, D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_DEPTH_WRITE, D3D12_RESOURCE_STATE_RENDER_TARGET, D3D12_RESOURCE_STATE_UNORDERED_ACCESS, D3D12_SHADER_RESOURCE_VIEW_DESC, D3D12_SHADER_RESOURCE_VIEW_DESC_0, D3D12_SRV_DIMENSION_TEXTURE2D, D3D12_SRV_DIMENSION_TEXTURE3D, D3D12_SRV_DIMENSION_TEXTURECUBE, D3D12_TEX2D_ARRAY_DSV, D3D12_TEX2D_DSV, D3D12_TEX2D_SRV, D3D12_TEX2D_UAV, D3D12_TEX3D_SRV, D3D12_TEXCUBE_SRV, D3D12_TEXTURE_COPY_LOCATION, D3D12_TEXTURE_COPY_LOCATION_0, D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT, D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX, D3D12_UAV_DIMENSION_TEXTURE2D, D3D12_UNORDERED_ACCESS_VIEW_DESC, D3D12_UNORDERED_ACCESS_VIEW_DESC_0};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT, DXGI_FORMAT_D24_UNORM_S8_UINT, DXGI_FORMAT_D32_FLOAT, DXGI_FORMAT_D32_FLOAT_S8X24_UINT, DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_FORMAT_R16G16_FLOAT, DXGI_FORMAT_R16_UNORM, DXGI_FORMAT_R24G8_TYPELESS, DXGI_FORMAT_R24_UNORM_X8_TYPELESS, DXGI_FORMAT_R32G32B32A32_FLOAT, DXGI_FORMAT_R32G8X24_TYPELESS, DXGI_FORMAT_R32_FLOAT, DXGI_FORMAT_R32_FLOAT_X8X24_TYPELESS, DXGI_FORMAT_R32_TYPELESS, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, DXGI_SAMPLE_DESC};

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum TextureKind {
    SampleSource, // SRV
    RenderTarget(Vec4), // SRV + RTV
    Depth, // SRV + DSV
    Storage, // UAV, writeable
}

#[derive(Debug, Copy, Clone)]
pub struct TextureDesc {
    pub kind: TextureKind,
    pub width: u32,
    pub height: u32,
    pub format: DXGI_FORMAT,
    pub mips: u32,
    pub is_cube: bool,
    pub depth: u32,
}

impl TextureDesc {
    pub fn layers(&self) -> u16 {
        if self.is_cube { 6 } else { 1 }
    }
}

#[derive(Debug)]
pub struct Texture {
    res: ID3D12Resource,
    desc: TextureDesc,
    state: Cell<D3D12_RESOURCE_STATES>,
    srv_ptr: u32,
    rtv_ptr: Option<u32>,
    dsv_ptr: Option<u32>,
    dsv_ro_ptr: Option<u32>,
    uav_ptr: Option<u32>,
    name: String,
}

impl Texture {
    pub fn new(gd: &GraphicsDevice, desc: TextureDesc, name: &str) -> anyhow::Result<Self> {
        let srv = gd.srv_heap()?.alloc()?;
        let rtv = matches!(desc.kind, TextureKind::RenderTarget(_)).then(|| gd.rtv_heap()?.alloc()).transpose()?;
        let dsv = matches!(desc.kind, TextureKind::Depth).then(|| -> anyhow::Result<u32> {
            let base = gd.dsv_heap()?.alloc()?;
            for _ in 1..desc.layers() {
                gd.dsv_heap()?.alloc()?;
            }
            Ok(base)
        }).transpose()?;
        let dsv_ro = (matches!(desc.kind, TextureKind::Depth) && !desc.is_cube).then(|| gd.dsv_heap()?.alloc()).transpose()?;
        let uav = matches!(desc.kind, TextureKind::Storage).then(|| gd.srv_heap()?.alloc()).transpose()?;
        let (res, state) = Self::load_resources(gd, &desc, name)?;

        let tex = Self {
            res,
            desc,
            state: Cell::new(state),
            srv_ptr: srv,
            rtv_ptr: rtv,
            dsv_ptr: dsv,
            dsv_ro_ptr: dsv_ro,
            uav_ptr: uav,
            name: name.into()
        };
        tex.write_views(gd)?;

        Ok(tex)
    }

    pub fn render_target(gd: &GraphicsDevice, format: DXGI_FORMAT, w: u32, h: u32, clear: Vec4, name: &str) -> anyhow::Result<Self> {
        Self::new(gd, TextureDesc { format, width: w, height: h, kind: TextureKind::RenderTarget(clear), mips: 1, is_cube: false, depth: 1 }, name)
    }

    pub fn depth(gd: &GraphicsDevice, w: u32, h: u32, name: &str) -> anyhow::Result<Self> {
        Self::new(gd, TextureDesc { format: DXGI_FORMAT_D32_FLOAT, width: w, height: h, kind: TextureKind::Depth, mips: 1, is_cube: false, depth: 1 }, name)
    }

    pub fn load(batch: &mut UploadBatch<'_>, bytes: &[u8], srgb: bool, mips: MipCount, name: &str) -> anyhow::Result<Self> {
        let img = image::load_from_memory(bytes)?.to_rgba8();
        Self::from_rgba8(batch, img.width(), img.height(), &img, srgb, mips, name)
    }

    pub fn load_from_mips(batch: &mut UploadBatch<'_>, mips: &[RgbaImage], srgb: bool, name: &str) -> anyhow::Result<Self> {
        let base = Self::new(batch.gd(), TextureDesc {
            kind: TextureKind::SampleSource,
            width: mips[0].width(), height: mips[0].height(),
            format: if srgb { DXGI_FORMAT_R8G8B8A8_UNORM_SRGB } else { DXGI_FORMAT_R8G8B8A8_UNORM },
            mips: mips.len() as u32,
            is_cube: false,
            depth: 1
        }, name)?;
        let mips = mips.into_iter().map(|it| it.as_bytes()).collect::<Vec<_>>();
        base.upload(batch, &mips)?;
        Ok(base)
    }

    pub fn cube_from_rgba8(batch: &mut UploadBatch, size: u32, faces: [&[u8]; 6], name: &str) -> anyhow::Result<Self> {
        let tex = Self::new(batch.gd(), TextureDesc {
            kind: TextureKind::SampleSource,
            width: size, height: size,
            format: DXGI_FORMAT_R8G8B8A8_UNORM,
            mips: 1,
            is_cube: true,
            depth: 1
        }, name)?;
        tex.upload(batch, &faces)?;
        Ok(tex)
    }

    pub fn from_rgba8(batch: &mut UploadBatch<'_>, width: u32, height: u32, bytes: &[u8], srgb: bool, mips: MipCount, name: &str) -> anyhow::Result<Self> {
        let mips = build_mips(RgbaImage::from_raw(width, height, bytes.to_vec())
            .ok_or_else(|| anyhow!("Invalid rgba8 buffer size for texture {name}"))?, mips);
        let base = Self::new(batch.gd(), TextureDesc {
            kind: TextureKind::SampleSource,
            width, height,
            format: if srgb { DXGI_FORMAT_R8G8B8A8_UNORM_SRGB } else { DXGI_FORMAT_R8G8B8A8_UNORM },
            mips: mips.len() as u32,
            is_cube: false,
            depth: 1
        }, name)?;
        let mips = mips.iter().map(|it|  it.as_bytes()).collect::<Vec<_>>();
        base.upload(batch, &mips)?;
        Ok(base)
    }

    pub fn load_luma16(batch: &mut UploadBatch<'_>, image: ImageBuffer<image::Luma<u16>, Vec<u16>>, mips: MipCount, name: &str) -> anyhow::Result<Self> {
        let width = image.width();
        let height = image.height();
        let mips = build_mips_luma(image, mips);
        let base = Self::new(batch.gd(), TextureDesc {
            kind: TextureKind::SampleSource,
            width, height,
            format: DXGI_FORMAT_R16_UNORM,
            mips: mips.len() as u32,
            is_cube: false,
            depth: 1
        }, name)?;
        let mips = mips.iter().map(|it|  it.as_bytes()).collect::<Vec<_>>();
        base.upload(batch, &mips)?;
        Ok(base)
    }

    pub fn from_rgba8_3d(batch: &mut UploadBatch<'_>, width: u32, height: u32, depth: u32, bytes: &[u8], name: &str) -> anyhow::Result<Self> {
        let base = Self::new(batch.gd(), TextureDesc {
            kind: TextureKind::SampleSource,
            width, height, depth,
            format: DXGI_FORMAT_R8G8B8A8_UNORM,
            mips: 1,
            is_cube: false,
        }, name)?;
        base.upload(batch, &[&bytes])?;
        Ok(base)
    }

    pub fn from_hdr(batch: &mut UploadBatch, bytes: &[u8], mips: MipCount, name: &str) -> anyhow::Result<Self> {
        let img = image::load_from_memory(bytes)?.to_rgba32f();
        let width = img.width();
        let height = img.height();
        let mips = build_mips_hdr(img, mips);
        let tex = Self::new(batch.gd(), TextureDesc {
            kind: TextureKind::SampleSource,
            width, height,
            format: DXGI_FORMAT_R32G32B32A32_FLOAT,
            mips: mips.len() as u32,
            is_cube: false,
            depth: 1
        }, name)?;
        let mips = mips.iter().map(|it| it.as_bytes()).collect::<Vec<_>>();
        tex.upload(batch, &mips)?;
        Ok(tex)
    }

    pub fn cube_from_hdr(batch: &mut UploadBatch, sides: [&[u8]; 6], name: &str) -> anyhow::Result<Self> {
        let images: Vec<Rgba32FImage> = sides.into_iter().map(|each| -> anyhow::Result<Rgba32FImage> { Ok(image::load_from_memory(each)?.to_rgba32f()) }).collect::<anyhow::Result<_>>()?;
        let tex = Self::new(batch.gd(), TextureDesc {
            kind: TextureKind::SampleSource,
            width: images[0].width(), height: images[0].height(),
            format: DXGI_FORMAT_R32G32B32A32_FLOAT,
            mips: 1,
            is_cube: true,
            depth: 1
        }, name)?;
        tex.upload(batch, &[
            bytemuck::cast_slice(images[0].as_raw()),
            bytemuck::cast_slice(images[1].as_raw()),
            bytemuck::cast_slice(images[2].as_raw()),
            bytemuck::cast_slice(images[3].as_raw()),
            bytemuck::cast_slice(images[4].as_raw()),
            bytemuck::cast_slice(images[5].as_raw()),
        ])?;
        Ok(tex)
    }

    pub fn from_dds(batch: &mut UploadBatch, bytes: &[u8], name: &str) -> anyhow::Result<Self> {
        let fetch_u32 = |offset: usize| -> anyhow::Result<u32> {
            let b = bytes.get(offset..offset + 4);
            b.map(|it| u32::from_le_bytes(it.try_into().unwrap())).ok_or_else(|| anyhow!("Failed to read DDS header"))
        };
        // https://learn.microsoft.com/en-us/windows/win32/direct3ddds/dds-header
        let height = fetch_u32(12)?; // +4 cus of first 4 bytes being "DDS "
        let width = fetch_u32(16)?;
        let mips = fetch_u32(28)?;
        let fmt = DXGI_FORMAT(fetch_u32(128)? as i32);
        let is_cube = fetch_u32(136)? & 0x4 != 0;

        let bytes_per_pixel = match fmt {
            DXGI_FORMAT_R16G16B16A16_FLOAT => 8,
            DXGI_FORMAT_R16G16_FLOAT => 4,
            DXGI_FORMAT_R32G32B32A32_FLOAT => 16,
            _f => anyhow::bail!("invalid DDS format!"),
        };

        let faces = if is_cube { 6 } else { 1};
        let mut off = 148;
        let mut subresources = Vec::with_capacity(faces * mips as usize);

        for _ in 0..faces {
            for mip in 0..mips {
                let len = ((width >> mip).max(1) * (height >> mip).max(1) * bytes_per_pixel) as usize;
                subresources.push(bytes.get(off..off + len).ok_or_else(|| anyhow!("Failed to read DDS data"))?);
                off += len;
            }
        }

        let tex = Self::new(batch.gd(), TextureDesc {
            kind: TextureKind::SampleSource,
            width,
            height,
            format: fmt,
            mips,
            is_cube, depth: 1 }, name)?;

        tex.upload(batch, &subresources)?;

        Ok(tex)
    }

    pub fn srv_ptr(&self) -> u32 { self.srv_ptr }
    pub fn rtv_ptr(&self) -> Option<u32> { self.rtv_ptr }
    pub fn dsv_ptr(&self) -> Option<u32> { self.dsv_ptr }
    pub fn dsv_slice(&self, slice: u32) -> Option<u32> { self.dsv_ptr.map(|it| it + slice) }
    pub fn dsv_ro_ptr(&self) -> Option<u32> { self.dsv_ro_ptr }
    pub fn uav_ptr(&self) -> Option<u32> { self.uav_ptr }
    pub fn desc(&self) -> &TextureDesc { &self.desc }
    pub fn resource(&self) -> &ID3D12Resource { &self.res }

    pub fn transition(&self, cmd: &ID3D12GraphicsCommandList, target_state: D3D12_RESOURCE_STATES) {
        let cur = self.state.get();
        if cur != target_state {
            unsafe {
                cmd.ResourceBarrier(&[transition_barrier(&self.res, cur, target_state)]);
                self.state.set(target_state);
            }
        }
    }

    pub fn ensure_unordered_access(&self, cmd: &ID3D12GraphicsCommandList) {
        let cur = self.state.get();
        if cur == D3D12_RESOURCE_STATE_UNORDERED_ACCESS {
            unsafe {
                cmd.ResourceBarrier(&[uav_barrier(&self.res)]);
            }
        } else {
            unsafe {
                cmd.ResourceBarrier(&[transition_barrier(&self.res, cur, D3D12_RESOURCE_STATE_UNORDERED_ACCESS)]);
                self.state.set(D3D12_RESOURCE_STATE_UNORDERED_ACCESS);
            }
        }
    }

    pub fn resize(&mut self, gd: &GraphicsDevice, new_width: u32, new_height: u32) -> anyhow::Result<()> {
        if new_width == self.desc.width && new_height == self.desc.height {
            return Ok(())
        }
        self.desc.width = new_width;
        self.desc.height = new_height;
        let (res, state) = Self::load_resources(gd, &self.desc, &self.name)?;
        self.res = res;
        self.state.set(state);
        self.write_views(gd)?;
        Ok(())
    }

    pub fn rewrite(&mut self, batch: &mut UploadBatch, new_width: u32, new_height: u32, new_data_rgba8: &[u8], mips: MipCount) -> anyhow::Result<()> {
        let mips = build_mips(RgbaImage::from_raw(new_width, new_height, new_data_rgba8.to_vec()).ok_or_else(|| anyhow!("Invalid size for texture {}!", self.name))?, mips);
        self.desc.mips = mips.len() as u32;
        self.desc.width = 0;
        self.resize(batch.gd(), new_width, new_height)?;
        let mips = mips.iter().map(|it| it.as_bytes()).collect::<Vec<_>>();
        self.upload(batch, &mips)
    }

    fn load_resources(gd: &GraphicsDevice, desc: &TextureDesc, name: &str) -> anyhow::Result<(ID3D12Resource, D3D12_RESOURCE_STATES)> {
        let fmt = formats(desc);
        let (flags, state, clear) = match desc.kind {
            TextureKind::SampleSource => (D3D12_RESOURCE_FLAG_NONE, D3D12_RESOURCE_STATE_COPY_DEST, None),
            TextureKind::RenderTarget(col) => (
                D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET,
                D3D12_RESOURCE_STATE_RENDER_TARGET,
                Some(D3D12_CLEAR_VALUE { Format: fmt.target, Anonymous: D3D12_CLEAR_VALUE_0 { Color: col.to_array() } }),
            ),
            TextureKind::Depth => (
                D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL,
                D3D12_RESOURCE_STATE_DEPTH_WRITE,
                Some(D3D12_CLEAR_VALUE {
                    Format: fmt.target,
                    Anonymous: D3D12_CLEAR_VALUE_0 {
                        DepthStencil: D3D12_DEPTH_STENCIL_VALUE { Depth: 0.0, Stencil: 0 }, // reversed z depth here
                    },
                }),
            ),
            TextureKind::Storage => {
                (
                    D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
                    D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
                    None
                )
            }
        };
        let rd = D3D12_RESOURCE_DESC {
            Dimension: if desc.depth > 1 { D3D12_RESOURCE_DIMENSION_TEXTURE3D } else { D3D12_RESOURCE_DIMENSION_TEXTURE2D },
            Width: desc.width as u64,
            Height: desc.height,
            DepthOrArraySize: if desc.depth > 1 { desc.depth as u16 } else { desc.layers() },
            MipLevels: desc.mips as u16,
            Format: fmt.resource,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Flags: flags,
            ..Default::default()
        };
        let heap = D3D12_HEAP_PROPERTIES { Type: D3D12_HEAP_TYPE_DEFAULT, ..Default::default() };
        let mut res: Option<ID3D12Resource> = None;
        unsafe {
            gd.device().CreateCommittedResource(
                &heap, D3D12_HEAP_FLAG_NONE, &rd,
                state, clear.as_ref().map(|c| c as *const _), &mut res
            )?;
        }
        let res = res.ok_or_else(|| anyhow!("Failed to create a texture {name}"))?;
        unsafe { res.SetName(&HSTRING::from(format!("Brine::Tex2D_{name}")))? };
        Ok((res, state))
    }

    fn write_views(&self, gd: &GraphicsDevice) -> anyhow::Result<()> {
        let fmt = formats(&self.desc);
        let dev = gd.device();
        unsafe {
            let srv = if !self.desc.is_cube {
                D3D12_SHADER_RESOURCE_VIEW_DESC {
                    Format: fmt.srv,
                    ViewDimension: if self.desc.depth > 1 { D3D12_SRV_DIMENSION_TEXTURE3D } else { D3D12_SRV_DIMENSION_TEXTURE2D },
                    Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
                    Anonymous: if self.desc.depth > 1 {
                        D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                            Texture3D: D3D12_TEX3D_SRV {
                                MipLevels: 1,
                                ..Default::default()
                            }
                        }
                    } else {
                        D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                            Texture2D: D3D12_TEX2D_SRV { MipLevels: u32::MAX, ..Default::default() }
                        }
                    }
                }
            } else {
                D3D12_SHADER_RESOURCE_VIEW_DESC {
                    Format: fmt.srv,
                    ViewDimension: D3D12_SRV_DIMENSION_TEXTURECUBE,
                    Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
                    Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                        TextureCube: D3D12_TEXCUBE_SRV { MipLevels: u32::MAX, ..Default::default() },
                    },
                }
            };
            dev.CreateShaderResourceView(&self.res, Some(&srv), gd.srv_heap()?.cpu(self.srv_ptr));

            if let Some(rtv) = self.rtv_ptr {
                dev.CreateRenderTargetView(&self.res, None, gd.rtv_heap()?.cpu(rtv));
            }

            if let Some(dsv) = self.dsv_ptr {
                if self.desc.is_cube {
                    for face in 0..self.desc.layers() {
                        let d = D3D12_DEPTH_STENCIL_VIEW_DESC {
                            Format: fmt.target,
                            ViewDimension: D3D12_DSV_DIMENSION_TEXTURE2DARRAY,
                            Flags: D3D12_DSV_FLAG_NONE,
                            Anonymous: D3D12_DEPTH_STENCIL_VIEW_DESC_0 { Texture2DArray: D3D12_TEX2D_ARRAY_DSV { MipSlice: 0, FirstArraySlice: face as u32, ArraySize: 1 } },
                        };
                        dev.CreateDepthStencilView(&self.res, Some(&d), gd.dsv_heap()?.cpu(dsv + face as u32));
                    }
                } else {
                    let d = D3D12_DEPTH_STENCIL_VIEW_DESC {
                        Format: fmt.target,
                        ViewDimension: D3D12_DSV_DIMENSION_TEXTURE2D,
                        Flags: D3D12_DSV_FLAG_NONE,
                        Anonymous: D3D12_DEPTH_STENCIL_VIEW_DESC_0 { Texture2D: D3D12_TEX2D_DSV { MipSlice: 0 } },
                    };
                    dev.CreateDepthStencilView(&self.res, Some(&d), gd.dsv_heap()?.cpu(dsv));
                }
            }

            if let Some(dsv_ro_ptr) = self.dsv_ro_ptr {
                let d = D3D12_DEPTH_STENCIL_VIEW_DESC {
                    Format: fmt.target,
                    ViewDimension: D3D12_DSV_DIMENSION_TEXTURE2D,
                    Flags: D3D12_DSV_FLAG_READ_ONLY_DEPTH,
                    Anonymous: D3D12_DEPTH_STENCIL_VIEW_DESC_0 { Texture2D: D3D12_TEX2D_DSV { MipSlice: 0 } },
                };
                dev.CreateDepthStencilView(&self.res, Some(&d), gd.dsv_heap()?.cpu(dsv_ro_ptr));
            }

            if let Some(uav_ptr) = self.uav_ptr {
                let d = D3D12_UNORDERED_ACCESS_VIEW_DESC {
                    Format: fmt.target,
                    ViewDimension: D3D12_UAV_DIMENSION_TEXTURE2D,
                    Anonymous: D3D12_UNORDERED_ACCESS_VIEW_DESC_0 { Texture2D: D3D12_TEX2D_UAV { MipSlice: 0, PlaneSlice: 0 } },
                };
                dev.CreateUnorderedAccessView(&self.res, None, Some(&d), gd.srv_heap()?.cpu(uav_ptr));
            }
        }
        Ok(())
    }

    fn upload(&self, batch: &mut UploadBatch<'_>, mips: &[&[u8]]) -> anyhow::Result<()> {
        let desc = unsafe { self.res.GetDesc() };
        let n = if desc.Dimension == D3D12_RESOURCE_DIMENSION_TEXTURE3D { desc.MipLevels as usize } else { desc.MipLevels as usize * desc.DepthOrArraySize as usize };
        anyhow::ensure!(mips.len() == n, "{}: {} mips given, resource has {}", self.name, mips.len(), n);

        let mut fps = vec![D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default(); n];
        let mut num_rows = vec![0u32; n];
        let mut row_size = vec![0u64; n];
        let mut total = 0u64; // needed staging buffer size
        unsafe {
            batch.gd().device().GetCopyableFootprints(
                &desc, 0, n as u32, 0,
                Some(fps.as_mut_ptr()),
                Some(num_rows.as_mut_ptr()),
                Some(row_size.as_mut_ptr()),
                Some(&mut total),
            );
        }

        let (staging, base, dest) = batch.alloc(total as usize, 512)?;

        let cmd = batch.cmd();
        for (i, mip) in mips.iter().enumerate() {
            let pitch = fps[i].Footprint.RowPitch as usize;
            let row = row_size[i] as usize;

            let mip_dst = unsafe { dest.add(fps[i].Offset as usize) };
            for (y, src) in mip.chunks_exact(row).enumerate() {
                unsafe { std::ptr::copy_nonoverlapping(src.as_ptr(), mip_dst.add(y * pitch), row) };
            }
        }
        for (i, mip_fp) in fps.iter().enumerate() {
            let mut fp = *mip_fp;
            fp.Offset += base;
            unsafe {
                let dst = D3D12_TEXTURE_COPY_LOCATION {
                    pResource: std::mem::transmute_copy(&self.res),
                    Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
                    Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 { SubresourceIndex: i as u32 },
                };
                let src = D3D12_TEXTURE_COPY_LOCATION {
                    pResource: std::mem::transmute_copy(&staging),
                    Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
                    Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 { PlacedFootprint: fp },
                };
                cmd.CopyTextureRegion(&dst, 0, 0, 0, &src, None);
            }
        }
        self.transition(cmd, D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE);
        Ok(())
    }
}

struct Formats {
    resource: DXGI_FORMAT,
    srv: DXGI_FORMAT,
    target: DXGI_FORMAT,
}

fn formats(desc: &TextureDesc) -> Formats {
    let f = desc.format;
    match f {
        DXGI_FORMAT_D32_FLOAT => Formats { resource: DXGI_FORMAT_R32_TYPELESS, srv: DXGI_FORMAT_R32_FLOAT, target: f },
        DXGI_FORMAT_D24_UNORM_S8_UINT => Formats {
            resource: DXGI_FORMAT_R24G8_TYPELESS, srv: DXGI_FORMAT_R24_UNORM_X8_TYPELESS, target: f,
        },
        DXGI_FORMAT_D32_FLOAT_S8X24_UINT => Formats {
            resource: DXGI_FORMAT_R32G8X24_TYPELESS, srv: DXGI_FORMAT_R32_FLOAT_X8X24_TYPELESS, target: f,
        },
        _ => Formats { resource: f, srv: f, target: f },
    }
}

#[derive(Debug, Copy, Clone)]
pub enum MipCount {
    All,
    Limit(u32)
}

macro_rules! mips_fns {
    ($($fn_name:ident : $img_ty:ty),+ $(,)?) => {
        $(
        pub fn $fn_name(img: $img_ty, mips: MipCount) -> Vec<$img_ty> {
            let max = match mips { MipCount::All => u32::MAX, MipCount::Limit(v) => v}.clamp(1, mip_count(img.width(), img.height()));
            let mut mips = vec![img];
            while let Some(prev) =
                mips.last().filter(|m| m.width() > 1 || m.height() > 1)
                && (mips.len() as u32) < max
            {
                let (w, h) = ((prev.width() / 2).max(1), (prev.height() / 2).max(1));
                let next = image::imageops::resize(prev, w, h, image::imageops::FilterType::Triangle);
                mips.push(next);
            }
            mips
        }
        )+

    };
}

mips_fns!(
    build_mips: RgbaImage,
    build_mips_luma: ImageBuffer<image::Luma<u16>, Vec<u16>>,
);

pub fn build_mips_hdr(img: Rgba32FImage, mips: MipCount) -> Vec<Rgba32FImage> {
    let max = match mips {
        MipCount::All => u32::MAX,
        MipCount::Limit(v) => v
    }.clamp(1, mip_count(img.width(), img.height()));
    let mut out = vec![img];
    while (out.len() as u32) < max {
        let prev = out.last().unwrap();
        let (w, h) = ((prev.width() / 2).max(1), (prev.height() / 2).max(1));
        // box 2x2
        let next = Rgba32FImage::from_fn(w, h, |x, y| {
            let at = |dx: u32, dy: u32| prev.get_pixel((2 * x + dx).min(prev.width() - 1), (2 * y + dy).min(prev.height() - 1)).0;
            let (a, b, c, d) = (at(0, 0), at(1, 0), at(0, 1), at(1, 1));
            Rgba(std::array::from_fn(|i| (a[i] + b[i] + c[i] + d[i]) * 0.25))
        });
        out.push(next);
    }
    out
}

fn mip_count(w: u32, h: u32) -> u32 {
    32 - w.max(h).leading_zeros()
}