use anyhow::anyhow;
use glam::Vec4;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT, DXGI_FORMAT_D32_FLOAT};
use crate::render::texture::{Texture, TextureDesc, TextureKind};
use crate::wrappers::graphics::GraphicsDevice;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Default)]
pub struct ResId(pub usize);

#[derive(Debug, Clone)]
pub enum ResSize {
    Fixed(u32, u32),
    FitScreen,
    ScreenDiv(u32), // screen size divided by n, at least 1x1
}

impl ResSize {
    fn resolve(&self, screen_w: u32, screen_h: u32) -> (u32, u32) {
        match *self {
            ResSize::Fixed(w, h) => (w, h),
            ResSize::FitScreen => (screen_w, screen_h),
            ResSize::ScreenDiv(d) => ((screen_w / d).max(1), (screen_h / d).max(1)),
        }
    }
}

#[derive(Debug)]
pub struct GraphResources {
    resources: Vec<(ResSize, Texture)>,
    screen_w: u32,
    screen_h: u32
}

impl GraphResources {
    pub fn new(w: u32, h: u32) -> GraphResources {
        Self {
            resources: Vec::new(),
            screen_w: w,
            screen_h: h
        }
    }

    pub fn color(&mut self, gd: &GraphicsDevice, name: &str, format: DXGI_FORMAT, clear: Vec4, size: ResSize) -> anyhow::Result<ResId> {
        let (w, h) = size.resolve(self.screen_w, self.screen_h);
        let tex = Texture::render_target(gd, format, w, h, clear, name)?;
        let id = ResId(self.resources.len());
        self.resources.push((size, tex));
        Ok(id)
    }

    pub fn depth(&mut self, gd: &GraphicsDevice, name: &str, size: ResSize) -> anyhow::Result<ResId> {
        let (w, h) = size.resolve(self.screen_w, self.screen_h);
        let tex = Texture::depth(gd, w, h, name)?;
        let id = ResId(self.resources.len());
        self.resources.push((size, tex));
        Ok(id)
    }

    pub fn depth_cube(&mut self, gd: &GraphicsDevice, name: &str, size: u32) -> anyhow::Result<ResId> {
        let tex = Texture::new(gd, TextureDesc {
            kind: TextureKind::Depth, width: size, height: size,
            format: DXGI_FORMAT_D32_FLOAT, mips: 1, is_cube: true,
        }, name)?;
        let id = ResId(self.resources.len());
        self.resources.push((ResSize::Fixed(size, size), tex));
        Ok(id)
    }

    pub fn storage(&mut self, gd: &GraphicsDevice, name: &str, format: DXGI_FORMAT, size: ResSize) -> anyhow::Result<ResId> {
        let (w, h) = size.resolve(self.screen_w, self.screen_h);
        let tex = Texture::new(gd, TextureDesc {
            kind: TextureKind::Storage,
            width: w,
            height: h,
            format,
            mips: 1,
            is_cube: false
        }, name)?;
        let id = ResId(self.resources.len());
        self.resources.push((size, tex));
        Ok(id)
    }

    pub fn get(&self, id: ResId) -> anyhow::Result<&Texture> {
        self.resources.get(id.0).map(|it| &it.1).ok_or_else(|| anyhow!("No texture with id {id:?}!"))
    }

    pub fn on_resize(&mut self, gd: &GraphicsDevice, new_w: u32, new_h: u32) -> anyhow::Result<()> {
        if new_w == 0 || new_h == 0 {
            return Ok(()); // minimized, same as GraphicsDevice::resize
        }
        self.screen_w = new_w;
        self.screen_h = new_h;
        for (size, res) in &mut self.resources {
            if !matches!(size, ResSize::Fixed(..)) {
                let (w, h) = size.resolve(new_w, new_h);
                res.resize(gd, w, h)?;
            }
        }
        Ok(())
    }
}