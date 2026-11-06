use egui_probe::EguiProbe;

pub mod temporal;
pub mod fsr2;

#[derive(Debug, Copy, Clone, PartialEq, Default, EguiProbe)]
pub enum AntiAliasing {
    Disabled,
    #[default]
    Temporal,
    Fsr2
}

#[derive(Debug, Copy, Clone, PartialEq, Default, EguiProbe)]
pub enum FsrQuality {
    NativeAa,
    #[default]
    Quality,
    Balanced,
    Performance,
    UltraPerformance
}

impl FsrQuality {
    pub fn ratio(&self) -> f32 {
        match self {
            FsrQuality::NativeAa => 1.,
            FsrQuality::Quality => 1.5,
            FsrQuality::Balanced => 1.7,
            FsrQuality::Performance => 2.,
            FsrQuality::UltraPerformance => 3.
        }
    }
}