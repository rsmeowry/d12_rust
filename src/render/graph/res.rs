use anyhow::anyhow;
use glam::Vec4;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT;
use crate::render::texture::Texture;
use crate::wrappers::graphics::GraphicsDevice;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ResId(usize);

#[derive(Debug, Clone)]
pub enum ResSize {
    Fixed(u32, u32),
    FitScreen
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
        let (w, h) = match size {
            ResSize::Fixed(w, h) => (w, h),
            ResSize::FitScreen => (self.screen_w, self.screen_h),
        };
        let tex = Texture::render_target(gd, format, w, h, clear, name)?;
        let id = ResId(self.resources.len());
        self.resources.push((size, tex));
        Ok(id)
    }

    pub fn depth(&mut self, gd: &GraphicsDevice, name: &str, size: ResSize) -> anyhow::Result<ResId> {
        let (w, h) = match size {
            ResSize::Fixed(w, h) => (w, h),
            ResSize::FitScreen => (self.screen_w, self.screen_h),
        };
        let tex = Texture::depth(gd, w, h, name)?;
        let id = ResId(self.resources.len());
        self.resources.push((size, tex));
        Ok(id)
    }

    pub fn get(&self, id: ResId) -> anyhow::Result<&Texture> {
        self.resources.get(id.0).map(|it| &it.1).ok_or_else(|| anyhow!("No texture with id {id:?}!"))
    }

    pub fn on_resize(&mut self, gd: &GraphicsDevice, new_w: u32, new_h: u32) -> anyhow::Result<()> {
        for (size, res) in &mut self.resources {
            if let ResSize::FitScreen = size {
                res.resize(gd, new_w, new_h)?;
            }
        }
        Ok(())
    }
}