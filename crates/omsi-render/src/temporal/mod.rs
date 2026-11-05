//! Temporal upscaling: NVIDIA DLSS or AMD FSR 3 draw the picture smaller, take the last frames'
//! pixels along by each pixel's motion and bring it up to the window. Windows, on DirectX 12
//! or Vulkan. (The `upscaler` setting's other choices, Catmull-Rom and FSR 1, work on one frame and
//! need none of this: `encode_upscale`.)
//!
//! Neither vendor's code is linked or in the repository. The Windows x64 package carries AMD's
//! DLLs; NVIDIA's come from the launcher's optional installer. Without them the game neither
//! offers the choice nor changes anything (`omsi_cfg::upscaler_files`).

#[cfg(windows)]
mod d3d12;
#[cfg(windows)]
mod fidelityfx;
mod frame;
#[cfg(windows)]
mod streamline;
#[cfg(windows)]
mod vulkan;
#[cfg(test)]
mod tests;

pub(crate) use frame::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemporalUpscaler {
    Dlss,
    Fsr3,
}

impl TemporalUpscaler {
    pub fn parse(s: &str) -> Option<TemporalUpscaler> {
        match s.trim().to_ascii_lowercase().as_str() {
            "dlss" => Some(TemporalUpscaler::Dlss),
            "fsr3" => Some(TemporalUpscaler::Fsr3),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            TemporalUpscaler::Dlss => "dlss",
            TemporalUpscaler::Fsr3 => "fsr3",
        }
    }

    /// FSR 3 gives up the history faster than DLSS for the same mask: its panes take a weaker one.
    pub(crate) fn glass_mask_entry(self) -> &'static str {
        match self {
            TemporalUpscaler::Dlss => "fs_glass_mask",
            TemporalUpscaler::Fsr3 => "fs_glass_mask_fsr",
        }
    }

    /// Without the history FSR 3's jittered frames doubled the edges of a display's digits.
    pub(crate) fn masks_screens(self) -> bool {
        matches!(self, TemporalUpscaler::Dlss)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TemporalQuality {
    Native,
    #[default]
    Quality,
    Balanced,
    Performance,
    UltraPerformance,
}

impl TemporalQuality {
    pub fn parse(s: &str) -> TemporalQuality {
        match omsi_cfg::upscaler_files::quality_word(s) {
            "native" => TemporalQuality::Native,
            "balanced" => TemporalQuality::Balanced,
            "performance" => TemporalQuality::Performance,
            "ultra_performance" => TemporalQuality::UltraPerformance,
            _ => TemporalQuality::Quality,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            TemporalQuality::Native => "native",
            TemporalQuality::Quality => "quality",
            TemporalQuality::Balanced => "balanced",
            TemporalQuality::Performance => "performance",
            TemporalQuality::UltraPerformance => "ultra_performance",
        }
    }

    pub fn scale(self) -> f32 {
        match self {
            TemporalQuality::Native => 1.0,
            TemporalQuality::Quality => 0.667,
            TemporalQuality::Balanced => 0.58,
            TemporalQuality::Performance => 0.5,
            TemporalQuality::UltraPerformance => 0.333,
        }
    }
}

pub fn fallback_render_size(quality: TemporalQuality, (w, h): (u32, u32)) -> (u32, u32) {
    let s = quality.scale();
    (((w as f32 * s).round() as u32).max(1), ((h as f32 * s).round() as u32).max(1))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Temporal {
    pub upscaler: TemporalUpscaler,
    pub quality: TemporalQuality,
}

pub struct Inputs<'a> {
    pub color: &'a wgpu::Texture,
    /// Its depth from the prepass, reversed Z (1 near, 0 far).
    pub depth: &'a wgpu::Texture,
    pub motion: &'a wgpu::Texture,
    pub mask: &'a wgpu::Texture,
    pub output: &'a wgpu::Texture,
    pub render_size: (u32, u32),
    pub output_size: (u32, u32),
    /// Views of `color` (its plain UNORM one: the sRGB-encoded numbers), `depth`, `motion`, `mask` and
    /// `output`, in that order: Streamline on Vulkan is handed an image view with each image.
    pub views: [&'a wgpu::TextureView; 5],
}

/// This frame's camera as the upscalers want it. The matrices are unjittered and stored column
/// by column, which is exactly how Streamline (row vectors) reads its rows.
pub struct FrameConstants {
    pub view_to_clip: [[f32; 4]; 4],
    pub clip_to_view: [[f32; 4]; 4],
    pub clip_to_prev_clip: [[f32; 4]; 4],
    pub prev_clip_to_clip: [[f32; 4]; 4],
    pub jitter: [f32; 2],
    pub camera_pos: [f32; 3],
    pub camera_up: [f32; 3],
    pub camera_right: [f32; 3],
    pub camera_fwd: [f32; 3],
    pub near: f32,
    pub far: f32,
    pub fov: f32,
    pub aspect: f32,
    pub reset: bool,
    pub frame_ms: f32,
}

#[cfg(windows)]
pub(crate) fn dlss_vulkan_extensions() -> anyhow::Result<Vec<&'static std::ffi::CStr>> {
    streamline::vulkan_extensions()
}

#[cfg(windows)]
pub(crate) fn dlss_vulkan_abandon() {
    streamline::vulkan_abandon()
}

#[cfg(not(windows))]
pub(crate) fn dlss_vulkan_extensions() -> anyhow::Result<Vec<&'static std::ffi::CStr>> {
    anyhow::bail!("DLSS is Windows-only")
}

#[cfg(not(windows))]
pub(crate) fn dlss_vulkan_abandon() {}

#[cfg(not(windows))]
mod streamline {
    use super::{fallback_render_size, FrameConstants, Inputs, TemporalQuality};
    use anyhow::Result;

    pub struct Runtime;

    impl Runtime {
        pub(crate) fn wait(&mut self) {}
        pub fn load(_device: &wgpu::Device) -> Result<Runtime> {
            anyhow::bail!("DLSS is Windows-only (DirectX 12)")
        }
        pub fn render_size(&mut self, quality: TemporalQuality, out: (u32, u32)) -> (u32, u32) {
            fallback_render_size(quality, out)
        }
        pub fn evaluate(&mut self, _queue: &wgpu::Queue, _quality: TemporalQuality, _inputs: &Inputs, _c: &FrameConstants) -> Result<()> {
            anyhow::bail!("DLSS is Windows-only (DirectX 12)")
        }
    }
}

#[cfg(not(windows))]
mod fidelityfx {
    use super::{FrameConstants, Inputs};
    use anyhow::Result;

    pub struct Upscaler;

    impl Upscaler {
        pub(crate) fn wait(&mut self) {}
        pub fn load(_device: &wgpu::Device) -> Result<Upscaler> {
            anyhow::bail!("FSR 3 upscaling is Windows-only (DirectX 12)")
        }
        pub fn upscale(&mut self, _queue: &wgpu::Queue, _inputs: &Inputs, _c: &FrameConstants) -> Result<()> {
            anyhow::bail!("FSR 3 upscaling is Windows-only (DirectX 12)")
        }
    }
}
