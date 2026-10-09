use crate::wrappers::graphics::batching::UploadBatch;
use crate::wrappers::graphics::{transition_barrier, GraphicsDevice};
use anyhow::anyhow;
use glam::Vec4;
use image::RgbaImage;
use std::cell::Cell;
use windows::core::HSTRING;
use windows::Win32::Graphics::Direct3D12::{ID3D12GraphicsCommandList, ID3D12Resource, D3D12_CLEAR_VALUE, D3D12_CLEAR_VALUE_0, D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING, D3D12_DEPTH_STENCIL_VALUE, D3D12_DEPTH_STENCIL_VIEW_DESC, D3D12_DEPTH_STENCIL_VIEW_DESC_0, D3D12_DSV_DIMENSION_TEXTURE2D, D3D12_DSV_FLAG_NONE, D3D12_DSV_FLAG_READ_ONLY_DEPTH, D3D12_HEAP_FLAG_NONE, D3D12_HEAP_PROPERTIES, D3D12_HEAP_TYPE_DEFAULT, D3D12_PLACED_SUBRESOURCE_FOOTPRINT, D3D12_RESOURCE_DESC, D3D12_RESOURCE_DIMENSION_TEXTURE2D, D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL, D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET, D3D12_RESOURCE_FLAG_NONE, D3D12_RESOURCE_STATES, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_DEPTH_WRITE, D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE, D3D12_RESOURCE_STATE_RENDER_TARGET, D3D12_SHADER_RESOURCE_VIEW_DESC, D3D12_SHADER_RESOURCE_VIEW_DESC_0, D3D12_SRV_DIMENSION_TEXTURE2D, D3D12_TEX2D_DSV, D3D12_TEX2D_SRV, D3D12_TEXTURE_COPY_LOCATION, D3D12_TEXTURE_COPY_LOCATION_0, D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT, D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT, DXGI_FORMAT_D24_UNORM_S8_UINT, DXGI_FORMAT_D32_FLOAT, DXGI_FORMAT_D32_FLOAT_S8X24_UINT, DXGI_FORMAT_R24G8_TYPELESS, DXGI_FORMAT_R24_UNORM_X8_TYPELESS, DXGI_FORMAT_R32G8X24_TYPELESS, DXGI_FORMAT_R32_FLOAT, DXGI_FORMAT_R32_FLOAT_X8X24_TYPELESS, DXGI_FORMAT_R32_TYPELESS, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, DXGI_SAMPLE_DESC};

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum TextureKind {
    SampleSource, // SRV
    RenderTarget(Vec4), // SRV + RTV, clear color for now, wanna do skyboxes later
    Depth, // SRV + DSV
}

#[derive(Debug, Copy, Clone)]
pub struct TextureDesc {
    pub kind: TextureKind,
    pub width: u32,
    pub height: u32,
    pub format: DXGI_FORMAT,
    pub mips: u32
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
    name: String,
}

impl Texture {
    pub fn new(gd: &GraphicsDevice, desc: TextureDesc, name: &str) -> anyhow::Result<Self> {
        let srv = gd.srv_heap()?.alloc()?;
        let rtv = matches!(desc.kind, TextureKind::RenderTarget(_)).then(|| gd.rtv_heap()?.alloc()).transpose()?;
        let dsv = matches!(desc.kind, TextureKind::Depth).then(|| gd.dsv_heap()?.alloc()).transpose()?;
        let dsv_ro = matches!(desc.kind, TextureKind::Depth).then(|| gd.dsv_heap()?.alloc()).transpose()?;
        let (res, state) = Self::load_resources(gd, &desc, name)?;

        let tex = Self {
            res,
            desc,
            state: Cell::new(state),
            srv_ptr: srv,
            rtv_ptr: rtv,
            dsv_ptr: dsv,
            dsv_ro_ptr: dsv_ro,
            name: name.into()
        };
        tex.write_views(gd)?;

        Ok(tex)
    }

    pub fn render_target(gd: &GraphicsDevice, format: DXGI_FORMAT, w: u32, h: u32, clear: Vec4, name: &str) -> anyhow::Result<Self> {
        Self::new(gd, TextureDesc { format, width: w, height: h, kind: TextureKind::RenderTarget(clear), mips: 1 }, name)
    }

    pub fn depth(gd: &GraphicsDevice, w: u32, h: u32, name: &str) -> anyhow::Result<Self> {
        Self::new(gd, TextureDesc { format: DXGI_FORMAT_D32_FLOAT, width: w, height: h, kind: TextureKind::Depth, mips: 1 }, name)
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
        }, name)?;
        base.upload(batch, &mips)?;
        Ok(base)
    }

    pub fn from_rgba8(batch: &mut UploadBatch<'_>, width: u32, height: u32, bytes: &[u8], srgb: bool, mips: MipCount, name: &str) -> anyhow::Result<Self> {
        let mips = build_mips(RgbaImage::from_raw(width, height, bytes.to_vec())
            .ok_or_else(|| anyhow!("Invalid rgba8 buffer size for texture {name}"))?, mips);
        let base = Self::new(batch.gd(), TextureDesc {
            kind: TextureKind::SampleSource,
            width, height,
            format: if srgb { DXGI_FORMAT_R8G8B8A8_UNORM_SRGB } else { DXGI_FORMAT_R8G8B8A8_UNORM },
            mips: mips.len() as u32,
        }, name)?;
        base.upload(batch, &mips)?;
        Ok(base)
    }

    pub fn srv_ptr(&self) -> u32 { self.srv_ptr }
    pub fn rtv_ptr(&self) -> Option<u32> { self.rtv_ptr }
    pub fn dsv_ptr(&self) -> Option<u32> { self.dsv_ptr }
    pub fn dsv_ro_ptr(&self) -> Option<u32> { self.dsv_ro_ptr }
    pub fn desc(&self) -> &TextureDesc { &self.desc }

    pub fn transition(&self, cmd: &ID3D12GraphicsCommandList, target_state: D3D12_RESOURCE_STATES) {
        let cur = self.state.get();
        if cur != target_state {
            unsafe {
                cmd.ResourceBarrier(&[transition_barrier(&self.res, cur, target_state)]);
                self.state.set(target_state);
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
        };
        let rd = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            Width: desc.width as u64,
            Height: desc.height,
            DepthOrArraySize: 1,
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
            let srv = D3D12_SHADER_RESOURCE_VIEW_DESC {
                Format: fmt.srv,
                ViewDimension: D3D12_SRV_DIMENSION_TEXTURE2D,
                Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
                Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                    Texture2D: D3D12_TEX2D_SRV { MipLevels: u32::MAX, ..Default::default() }
                }
            };
            dev.CreateShaderResourceView(&self.res, Some(&srv), gd.srv_heap()?.cpu(self.srv_ptr));

            if let Some(rtv) = self.rtv_ptr {
                dev.CreateRenderTargetView(&self.res, None, gd.rtv_heap()?.cpu(rtv));
            }

            if let Some(dsv) = self.dsv_ptr {
                let d = D3D12_DEPTH_STENCIL_VIEW_DESC {
                    Format: fmt.target,
                    ViewDimension: D3D12_DSV_DIMENSION_TEXTURE2D,
                    Flags: D3D12_DSV_FLAG_NONE,
                    Anonymous: D3D12_DEPTH_STENCIL_VIEW_DESC_0 { Texture2D: D3D12_TEX2D_DSV { MipSlice: 0 } },
                };
                dev.CreateDepthStencilView(&self.res, Some(&d), gd.dsv_heap()?.cpu(dsv));
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
        }
        Ok(())
    }

    fn upload(&self, batch: &mut UploadBatch<'_>, mips: &[RgbaImage]) -> anyhow::Result<()> {
        let desc = unsafe { self.res.GetDesc() };
        let n = desc.MipLevels as usize;
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
            for (y, src) in mip.as_raw().chunks_exact(row).enumerate() {
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
        self.transition(cmd, D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE);
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

pub fn build_mips(img: RgbaImage, mips: MipCount) -> Vec<RgbaImage> {
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

fn mip_count(w: u32, h: u32) -> u32 {
    32 - w.max(h).leading_zeros()
}