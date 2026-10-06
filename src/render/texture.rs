use rand::random;
use crate::wrappers::graphics::{GraphicsDevice, transition_barrier};
use windows::Win32::Graphics::Direct3D12::{
    D3D12_GPU_DESCRIPTOR_HANDLE, D3D12_HEAP_FLAG_NONE, D3D12_HEAP_PROPERTIES,
    D3D12_HEAP_TYPE_DEFAULT, D3D12_PLACED_SUBRESOURCE_FOOTPRINT, D3D12_RESOURCE_DESC,
    D3D12_RESOURCE_DIMENSION_TEXTURE2D, D3D12_RESOURCE_STATE_COPY_DEST,
    D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE, D3D12_TEXTURE_COPY_LOCATION,
    D3D12_TEXTURE_COPY_LOCATION_0, D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
    D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX, ID3D12Resource,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC};

#[derive(Debug)]
pub struct Texture {
    _res: ID3D12Resource,
    idx: u32
}

impl Texture {
    pub fn load(gd: &GraphicsDevice, bytes: &[u8]) -> anyhow::Result<Self> {
        let img = image::load_from_memory(bytes)?.to_rgba8();
        Self::from_rgba8(gd, img.width(), img.height(), &img)
    }

    pub fn from_rgba8(
        gd: &GraphicsDevice,
        width: u32,
        height: u32,
        bytes: &[u8],
    ) -> anyhow::Result<Self> {
        if bytes.len() != (width * height * 4) as usize {
            return Err(anyhow::Error::msg("Pixel data is not RGBA8!"));
        }

        let dev = gd.device();
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            Width: width as u64,
            Height: height,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            ..Default::default()
        };
        let heap = D3D12_HEAP_PROPERTIES {
            Type: D3D12_HEAP_TYPE_DEFAULT,
            ..Default::default()
        };
        let mut tex: Option<ID3D12Resource> = None;
        unsafe {
            dev.CreateCommittedResource(
                &heap,
                D3D12_HEAP_FLAG_NONE,
                &desc,
                D3D12_RESOURCE_STATE_COPY_DEST,
                None,
                &mut tex,
            )?;
        }
        let tex = tex.ok_or_else(|| anyhow::Error::msg("Could not create texture!"))?;

        let mut footprint = D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default();
        let mut total: u64 = 0;
        unsafe {
            dev.GetCopyableFootprints(
                &desc,
                0,
                1,
                0,
                Some(&mut footprint),
                None,
                None,
                Some(&mut total),
            );
        }
        let pitch = footprint.Footprint.RowPitch as usize;
        let row = width as usize * 4;
        let mut staging = vec![0u8; total as usize];
        for y in 0..height as usize {
            staging[y * pitch..y * pitch + row].copy_from_slice(&bytes[y * row..(y + 1) * row]);
        }
        let upload = gd.create_buffer(&staging, format!("Tex_{:4x}", random::<u16>()))?;

        gd.execute_now(|cmd| unsafe {
            let dst = D3D12_TEXTURE_COPY_LOCATION {
                pResource: std::mem::transmute_copy(&tex),
                Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    SubresourceIndex: 0,
                },
            };
            let src = D3D12_TEXTURE_COPY_LOCATION {
                pResource: std::mem::transmute_copy(&upload),
                Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    PlacedFootprint: footprint,
                },
            };
            cmd.CopyTextureRegion(&dst, 0, 0, 0, &src, None);
            cmd.ResourceBarrier(&[transition_barrier(
                &tex,
                D3D12_RESOURCE_STATE_COPY_DEST,
                D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
            )]);
        })?;

        let (cpu, _gpu, idx) = gd.create_srv()?;
        unsafe { dev.CreateShaderResourceView(&tex, None, cpu) };

        Ok(Self {
            _res: tex,
            idx
        })
    }

    pub fn index(&self) -> u32 {
        self.idx
    }
}
