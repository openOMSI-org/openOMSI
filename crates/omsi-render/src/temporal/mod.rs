//! Temporal upscaling: NVIDIA DLSS, AMD FSR 3 or Apple MetalFX draw the picture smaller, take
//! the last frames' pixels along by each pixel's motion and bring it up to the window. DLSS and
//! FSR 3 on Windows, on DirectX 12 or Vulkan; MetalFX on a Mac with Apple silicon, on Metal.
//! (The `upscaler` setting's other choices, Catmull-Rom and FSR 1, work on one frame and need
//! none of this: `encode_upscale`.)
//!
//! No vendor's code is linked or in the repository. The Windows x64 package carries AMD's
//! DLLs; NVIDIA's come from the launcher's optional installer; MetalFX is part of macOS 13 and
//! newer. Without them the game neither offers the choice nor changes anything
//! (`omsi_cfg::upscaler_files`).

#[cfg(windows)]
mod d3d12;
#[cfg(windows)]
mod fidelityfx;
mod frame;
#[cfg(target_os = "macos")]
mod metalfx;
#[cfg(target_os = "macos")]
pub(crate) mod metalfx_frames;
#[cfg(windows)]
mod streamline;
#[cfg(windows)]
mod vulkan;
#[cfg(test)]
mod tests;

pub(crate) use frame::*;
#[cfg(target_os = "macos")]
pub use metalfx_frames::{FrameGeneration, GenerationWindow};

/// What makes the in-between frames (the `frame_generation` setting): Apple MetalFX, on a Mac.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameGenerationKind {
    MetalFx,
}

impl FrameGenerationKind {
    pub fn parse(s: &str) -> Option<FrameGenerationKind> {
        match omsi_cfg::upscaler_files::frame_generation_word(s) {
            "metalfx" => Some(FrameGenerationKind::MetalFx),
            _ => None,
        }
    }

    /// The upscaler it takes its depth and motion vectors from, and cannot run without.
    pub fn upscaler(self) -> TemporalUpscaler {
        match self {
            FrameGenerationKind::MetalFx => TemporalUpscaler::MetalFx,
        }
    }
}

/// An upscaled frame as frame generation takes it at the window's present.
pub(crate) struct GenerationInputs {
    /// The window's picture before the HUD: this frame's, and the last frame's (None: there is
    /// none of this size to go from).
    pub picture: wgpu::Texture,
    pub last_picture: Option<wgpu::Texture>,
    /// The upscaler's own, at the size the scene was drawn.
    pub depth: wgpu::Texture,
    pub motion: wgpu::Texture,
    pub render_size: (u32, u32),
    pub output_size: (u32, u32),
    pub jitter: [f32; 2],
    pub near: f32,
    pub far: f32,
    /// Vertical, in radians.
    pub fov: f32,
    pub aspect: f32,
    /// Since the last frame.
    pub seconds: f32,
    /// The camera jumped: nothing lies between the last frame and this one.
    pub cut: bool,
    pub scaler: Option<metalfx::RawScaler>,
}

/// Frame generation is MetalFX's, on a Mac: never made elsewhere.
#[cfg(not(target_os = "macos"))]
#[derive(Clone)]
pub enum FrameGeneration {}

#[cfg(not(target_os = "macos"))]
impl FrameGeneration {
    pub(crate) fn working(&self) -> bool {
        match *self {}
    }
    pub(crate) fn generated(&self) -> u64 {
        match *self {}
    }
    pub(crate) fn prepare(&self, _: GenerationInputs) {
        match *self {}
    }
    pub(crate) fn wait(&self) {
        match *self {}
    }
}

#[cfg(not(target_os = "macos"))]
pub enum GenerationWindow {}

#[cfg(not(target_os = "macos"))]
impl GenerationWindow {
    pub(crate) fn new(_: &wgpu::Device, _: &wgpu::Queue, _: wgpu::TextureFormat, _: (u32, u32)) -> anyhow::Result<GenerationWindow> {
        anyhow::bail!("frame generation is Apple MetalFX's (macOS 26 and newer)")
    }
    pub(crate) fn handle(&self) -> FrameGeneration {
        match *self {}
    }
    pub(crate) fn working(&self) -> bool {
        match *self {}
    }
    pub(crate) fn resize(&mut self, _: &wgpu::Device, _: &wgpu::Queue, _: (u32, u32)) {
        match *self {}
    }
    pub(crate) fn view(&self) -> wgpu::TextureView {
        match *self {}
    }
    pub(crate) fn texture(&self) -> wgpu::Texture {
        match *self {}
    }
    pub(crate) fn present(&self, _: &wgpu::Surface<'_>, _: &wgpu::Device, _: &wgpu::Queue, _: wgpu::SurfaceTexture) {
        match *self {}
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemporalUpscaler {
    Dlss,
    Fsr3,
    MetalFx,
}

impl TemporalUpscaler {
    pub fn parse(s: &str) -> Option<TemporalUpscaler> {
        match s.trim().to_ascii_lowercase().as_str() {
            "dlss" => Some(TemporalUpscaler::Dlss),
            "fsr3" => Some(TemporalUpscaler::Fsr3),
            "metalfx" => Some(TemporalUpscaler::MetalFx),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            TemporalUpscaler::Dlss => "dlss",
            TemporalUpscaler::Fsr3 => "fsr3",
            TemporalUpscaler::MetalFx => "metalfx",
        }
    }

    /// FSR 3 gives up the history faster than DLSS for the same mask: its panes take a weaker one.
    /// MetalFX reads its mask as FSR 3 does (1 is "no history at all"), and takes FSR 3's.
    pub(crate) fn glass_mask_entry(self) -> &'static str {
        match self {
            TemporalUpscaler::Dlss => "fs_glass_mask",
            TemporalUpscaler::Fsr3 | TemporalUpscaler::MetalFx => "fs_glass_mask_fsr",
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

/// The size MetalFX draws at, and whether it is larger than the quality asked for: Apple's
/// scaler goes up to `max_scale` times a side (`supportedInputContentMaxScaleForDevice`), which
/// is less than Ultra performance's three on most Macs.
pub fn metalfx_render_size(quality: TemporalQuality, out: (u32, u32), max_scale: f32) -> ((u32, u32), bool) {
    let asked = fallback_render_size(quality, out);
    let least = |side: u32| ((side as f32 / max_scale.max(1.0)).ceil() as u32).clamp(1, side.max(1));
    let size = (asked.0.max(least(out.0)), asked.1.max(least(out.1)));
    (size, size != asked)
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

#[cfg(not(target_os = "macos"))]
mod metalfx {
    use super::{fallback_render_size, FrameConstants, Inputs, TemporalQuality};
    use anyhow::Result;

    pub struct Upscaler;
    /// (never made: there is no scaler off a Mac)
    pub(crate) type RawScaler = std::convert::Infallible;

    impl Upscaler {
        pub(crate) fn wait(&mut self) {}
        pub(crate) fn scaler(&self) -> Option<RawScaler> {
            None
        }
        pub fn load(_device: &wgpu::Device) -> Result<Upscaler> {
            anyhow::bail!("MetalFX upscaling is macOS-only (Metal)")
        }
        pub fn render_size(&mut self, quality: TemporalQuality, out: (u32, u32)) -> (u32, u32) {
            fallback_render_size(quality, out)
        }
        pub fn upscale(&mut self, _queue: &wgpu::Queue, _inputs: &Inputs, _c: &FrameConstants) -> Result<()> {
            anyhow::bail!("MetalFX upscaling is macOS-only (Metal)")
        }
    }
}
