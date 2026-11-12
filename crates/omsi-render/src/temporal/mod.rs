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
mod dlssg;
#[cfg(windows)]
mod fidelityfx;
#[cfg(windows)]
mod framegen;
mod frame;
#[cfg(target_os = "macos")]
mod metalfx;
#[cfg(windows)]
mod streamline;
#[cfg(windows)]
mod vulkan;
#[cfg(test)]
mod tests;

pub(crate) use frame::*;
#[cfg(windows)]
pub use framegen::{FrameGeneration, OwnSwapChain};

/// Frame generation is Windows-only (DirectX 12): never made elsewhere.
#[cfg(not(windows))]
#[derive(Clone)]
pub enum FrameGeneration {}

#[cfg(not(windows))]
impl FrameGeneration {
    pub(crate) fn prepare(&self, _: &wgpu::Texture, _: &wgpu::Texture, _: &wgpu::Texture, _: (u32, u32), _: &FrameConstants) {
        match *self {}
    }

    pub(crate) fn generated(&self) -> u64 {
        match *self {}
    }
    pub(crate) fn pause(&self) {
        match *self {}
    }
    pub(crate) fn hold(&self, _: Vec<wgpu::Texture>) {
        match *self {}
    }
    pub(crate) fn working(&self) -> bool {
        match *self {}
    }
}

#[cfg(not(windows))]
pub enum OwnSwapChain {}

#[cfg(not(windows))]
impl OwnSwapChain {
    pub fn frame_generation(&self) -> FrameGeneration {
        match *self {}
    }
    pub fn size(&self) -> (u32, u32) {
        match *self {}
    }
    pub fn current(&self) -> Option<wgpu::Texture> {
        match *self {}
    }
    pub fn present(&self, _: &wgpu::Device, _: &wgpu::Queue, _: &wgpu::Texture) {
        match *self {}
    }
    pub fn resize(&mut self, _: &wgpu::Device, _: (u32, u32)) {
        match *self {}
    }
    pub fn set_vsync(&mut self, _: bool) {
        match *self {}
    }
    pub fn ask_frames(&self, _: u32) -> bool {
        match *self {}
    }
}

/// What makes the in-between frames (the `frame_generation` setting): AMD FSR 3, or NVIDIA DLSS
/// with this many frames on screen per drawn one (2 to 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameGenerationKind {
    Fsr3,
    Dlss(u32),
}

impl FrameGenerationKind {
    pub fn parse(s: &str) -> Option<FrameGenerationKind> {
        match omsi_cfg::upscaler_files::frame_generation_word(s) {
            "fsr3" => Some(FrameGenerationKind::Fsr3),
            "dlss2x" => Some(FrameGenerationKind::Dlss(2)),
            "dlss3x" => Some(FrameGenerationKind::Dlss(3)),
            "dlss4x" => Some(FrameGenerationKind::Dlss(4)),
            _ => None,
        }
    }

    /// The setting's frame generation where it can run: AMD FSR 3 with DLSS or FSR 3 on (it takes
    /// their motion), NVIDIA DLSS with DLSS on (DLSS hands it its inputs); DirectX 12 and the
    /// vendor's DLLs either way.
    pub fn runnable(
        word: &str,
        temporal: Option<Temporal>,
        graphics_api: &str,
    ) -> Option<FrameGenerationKind> {
        use omsi_cfg::upscaler_files as files;
        let (kind, temporal) = (Self::parse(word)?, temporal?);
        let dir = files::program_dir();
        let runs = match kind {
            FrameGenerationKind::Fsr3 => files::frame_generation_available(&dir),
            FrameGenerationKind::Dlss(_) => {
                temporal.upscaler == TemporalUpscaler::Dlss
                    && files::dlss_frame_generation_available(&dir)
            }
        };
        (runs && files::first_api(graphics_api, temporal.upscaler.as_str(), &dir) == "dx12")
            .then_some(kind)
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

    /// The name the frame rate's corner shows.
    pub fn label(self) -> &'static str {
        match self {
            TemporalUpscaler::Dlss => "DLSS",
            TemporalUpscaler::Fsr3 => "FSR 3",
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
    /// With frame generation: the window's picture before the HUD (window-sized, the swap
    /// chain's format), drawn after the upscaler and read at the window's present.
    pub hudless: Option<&'a wgpu::Texture>,
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
        pub(crate) fn own_session(&self) -> bool {
            false
        }
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

    impl Upscaler {
        pub(crate) fn wait(&mut self) {}
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
