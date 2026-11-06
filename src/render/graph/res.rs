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
    FitRender, // fit render resolution (may be affected by FSR)
    RenderDiv(u32),
}

impl ResSize {
    fn resolve(&self, screen: (u32, u32), render: (u32, u32)) -> (u32, u32) {
        match *self {
            ResSize::Fixed(w, h) => (w, h),
            ResSize::FitScreen => screen,
            ResSize::ScreenDiv(d) => ((screen.0 / d).max(1), (screen.1 / d).max(1)),
            ResSize::FitRender => render,
            ResSize::RenderDiv(d) => ((render.0 / d).max(1), (render.1 / d).max(1)),
        }
    }
}

#[derive(Debug)]
pub struct GraphResources {
    resources: Vec<(ResSize, Texture)>,
    screen_size: (u32, u32),
    render_scale: f32
}

impl GraphResources {
    pub fn new(w: u32, h: u32, render_scale: f32) -> GraphResources {
        Self {
            resources: Vec::new(),
            screen_size: (w, h),
            render_scale
        }
    }

    pub fn screen_size(&self) -> (u32, u32) {
        self.screen_size
    }

    pub fn render_scale(&self) -> f32 {
        self.render_scale
    }

    pub fn render_size(&self) -> (u32, u32) {
        ((self.screen_size.0 as f32 * self.render_scale).max(1.) as u32, (self.screen_size.1 as f32 * self.render_scale).max(1.) as u32)
    }

    pub fn color(&mut self, gd: &GraphicsDevice, name: &str, format: DXGI_FORMAT, clear: Vec4, size: ResSize) -> anyhow::Result<ResId> {
        let (w, h) = size.resolve(self.screen_size(), self.render_size());
        let tex = Texture::render_target(gd, format, w, h, clear, name)?;
        let id = ResId(self.resources.len());
        self.resources.push((size, tex));
        Ok(id)
    }

    pub fn depth(&mut self, gd: &GraphicsDevice, name: &str, size: ResSize) -> anyhow::Result<ResId> {
        let (w, h) = size.resolve(self.screen_size(), self.render_size());
        let tex = Texture::depth(gd, w, h, name)?;
        let id = ResId(self.resources.len());
        self.resources.push((size, tex));
        Ok(id)
    }

    pub fn depth_cube(&mut self, gd: &GraphicsDevice, name: &str, size: u32) -> anyhow::Result<ResId> {
        let tex = Texture::new(gd, TextureDesc {
            kind: TextureKind::Depth, width: size, height: size,
            format: DXGI_FORMAT_D32_FLOAT, mips: 1, is_cube: true,
            depth: 1,
        }, name)?;
        let id = ResId(self.resources.len());
        self.resources.push((ResSize::Fixed(size, size), tex));
        Ok(id)
    }

    pub fn storage(&mut self, gd: &GraphicsDevice, name: &str, format: DXGI_FORMAT, size: ResSize) -> anyhow::Result<ResId> {
        let (w, h) = size.resolve(self.screen_size(), self.render_size());
        let tex = Texture::new(gd, TextureDesc {
            kind: TextureKind::Storage,
            width: w,
            height: h,
            format,
            mips: 1,
            is_cube: false,
            depth: 1
        }, name)?;
        let id = ResId(self.resources.len());
        self.resources.push((size, tex));
        Ok(id)
    }

    pub fn get(&self, id: ResId) -> anyhow::Result<&Texture> {
        self.resources.get(id.0).map(|it| &it.1).ok_or_else(|| anyhow!("No texture with id {id:?}!"))
    }

    pub fn change_render_scale(&mut self, render_scale: f32, gd: &GraphicsDevice) -> anyhow::Result<()> {
        self.render_scale = render_scale.max(0.01);
        self.propagate_res_change(gd)
    }
    pub fn on_resize(&mut self, gd: &GraphicsDevice, new_w: u32, new_h: u32) -> anyhow::Result<()> {
        if new_w == 0 || new_h == 0 {
            return Ok(()); // minimized, same as GraphicsDevice::resize
        }
        self.screen_size = (new_w, new_h);
        self.propagate_res_change(gd)
    }

    fn propagate_res_change(&mut self, gd: &GraphicsDevice) -> anyhow::Result<()> {
        let ss = self.screen_size();
        let rs = self.render_size();
        for (size, res) in &mut self.resources {
            if !matches!(size, ResSize::Fixed(..)) {
                let (w, h) = size.resolve(ss, rs);
                res.resize(gd, w, h)?;
            }
        }
        Ok(())
    }
}