use std::cell::Cell;
use anyhow::{anyhow, ensure};
use glam::Vec4;
use crate::wrappers::graphics::{transition_barrier, GraphicsDevice};
use windows::core::HSTRING;
use windows::Win32::Graphics::Direct3D12::{ID3D12GraphicsCommandList, ID3D12Resource, D3D12_CLEAR_VALUE, D3D12_CLEAR_VALUE_0, D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING, D3D12_DEPTH_STENCIL_VALUE, D3D12_DEPTH_STENCIL_VIEW_DESC, D3D12_DEPTH_STENCIL_VIEW_DESC_0, D3D12_DSV_DIMENSION_TEXTURE2D, D3D12_DSV_FLAG_NONE, D3D12_HEAP_FLAG_ALLOW_WRITE_WATCH, D3D12_HEAP_FLAG_NONE, D3D12_HEAP_PROPERTIES, D3D12_HEAP_TYPE_DEFAULT, D3D12_PLACED_SUBRESOURCE_FOOTPRINT, D3D12_RESOURCE_DESC, D3D12_RESOURCE_DIMENSION_TEXTURE2D, D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL, D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET, D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS, D3D12_RESOURCE_FLAG_NONE, D3D12_RESOURCE_STATES, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_DEPTH_WRITE, D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE, D3D12_RESOURCE_STATE_RENDER_TARGET, D3D12_SHADER_RESOURCE_VIEW_DESC, D3D12_SHADER_RESOURCE_VIEW_DESC_0, D3D12_SRV_DIMENSION_TEXTURE2D, D3D12_TEX2D_DSV, D3D12_TEX2D_SRV, D3D12_TEXTURE_COPY_LOCATION, D3D12_TEXTURE_COPY_LOCATION_0, D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT, D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX};
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
    pub format: DXGI_FORMAT
}

#[derive(Debug)]
pub struct Texture {
    res: ID3D12Resource,
    desc: TextureDesc,
    state: Cell<D3D12_RESOURCE_STATES>,
    srv_ptr: u32,
    rtv_ptr: Option<u32>,
    dsv_ptr: Option<u32>,
    name: String,
}

impl Texture {
    pub fn new(gd: &GraphicsDevice, desc: TextureDesc, name: &str) -> anyhow::Result<Self> {
        let srv = gd.srv_heap()?.alloc()?;
        let rtv = matches!(desc.kind, TextureKind::RenderTarget(_)).then(|| gd.rtv_heap()?.alloc()).transpose()?;
        let dsv = matches!(desc.kind, TextureKind::Depth).then(|| gd.dsv_heap()?.alloc()).transpose()?;
        let (res, state) = Self::load_resources(gd, &desc, name)?;

        let tex = Self {
            res,
            desc,
            state: Cell::new(state),
            srv_ptr: srv,
            rtv_ptr: rtv,
            dsv_ptr: dsv,
            name: name.into()
        };
        tex.write_views(gd)?;

        Ok(tex)
    }

    pub fn render_target(gd: &GraphicsDevice, format: DXGI_FORMAT, w: u32, h: u32, clear: Vec4, name: &str) -> anyhow::Result<Self> {
        Self::new(gd, TextureDesc { format, width: w, height: h, kind: TextureKind::RenderTarget(clear) }, name)
    }

    pub fn depth(gd: &GraphicsDevice, w: u32, h: u32, name: &str) -> anyhow::Result<Self> {
        Self::new(gd, TextureDesc { format: DXGI_FORMAT_D32_FLOAT, width: w, height: h, kind: TextureKind::Depth }, name)
    }

    pub fn load(gd: &GraphicsDevice, bytes: &[u8], name: &str) -> anyhow::Result<Self> {
        let img = image::load_from_memory(bytes)?.to_rgba8();
        Self::from_rgba8(gd, img.width(), img.height(), &img, name)
    }

    pub fn from_rgba8(gd: &GraphicsDevice, width: u32, height: u32, bytes: &[u8], name: &str) -> anyhow::Result<Self> {
        ensure!(bytes.len() == (width * height * 4) as usize, "Invalid rgba8 buffer size! got {} but needed {}", bytes.len(), width * height * 4);
        let base = Self::new(gd, TextureDesc {
            kind: TextureKind::SampleSource,
            width,
            height,
            format: DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
        }, name)?;
        base.upload(&gd, bytes)?;
        Ok(base)
    }

    pub fn srv_ptr(&self) -> u32 { self.srv_ptr }
    pub fn rtv_ptr(&self) -> Option<u32> { self.rtv_ptr }
    pub fn dsv_ptr(&self) -> Option<u32> { self.dsv_ptr }
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

    fn load_resources(gd: &GraphicsDevice, desc: &TextureDesc, name: &str) -> anyhow::Result<(ID3D12Resource, D3D12_RESOURCE_STATES)> {
        let dev = gd.device();
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
            MipLevels: 1,
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
                    Texture2D: D3D12_TEX2D_SRV { MipLevels: 1, ..Default::default() }
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
        }
        Ok(())
    }

    fn upload(&self, gd: &GraphicsDevice, pixels: &[u8]) -> anyhow::Result<()> {
        let desc = unsafe { self.res.GetDesc() };

        let mut footprint = D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default();
        let mut num_rows = 0u32;
        let mut row_size = 0u64;
        let mut total = 0u64; // needed staging buffer size
        unsafe {
            gd.device().GetCopyableFootprints(
                &desc, 0, 1, 0,
                Some(&mut footprint),
                Some(&mut num_rows),
                Some(&mut row_size),
                Some(&mut total),
            );
        }

        let pitch = footprint.Footprint.RowPitch as usize;
        let row = row_size as usize;
        anyhow::ensure!(pixels.len() == row * num_rows as usize, "Expected {} bytes, got {}", row * num_rows as usize, pixels.len());

        let mut staging = vec![0u8; total as usize];
        for (y, src) in pixels.chunks_exact(row).enumerate() {
            staging[y * pitch..y * pitch + row].copy_from_slice(src);
        }
        let upload = gd.create_buffer(&staging, format!("Staging>>{:4x}", rand::random::<u32>()))?;

        gd.execute_now(|cmd| unsafe {
            let dst = D3D12_TEXTURE_COPY_LOCATION {
                pResource: std::mem::transmute_copy(&self.res),
                Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 { SubresourceIndex: 0 },
            };
            let src = D3D12_TEXTURE_COPY_LOCATION {
                pResource: std::mem::transmute_copy(&upload),
                Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 { PlacedFootprint: footprint },
            };
            cmd.CopyTextureRegion(&dst, 0, 0, 0, &src, None);
            self.transition(cmd, D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE);
        })?;
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