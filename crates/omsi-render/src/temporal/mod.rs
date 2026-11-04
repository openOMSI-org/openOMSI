//! Temporal upscaling: NVIDIA DLSS or AMD FSR 3 draw the picture smaller, take the last frames'
//! pixels along by each pixel's motion and bring it up to the window. Windows, on DirectX 12
//! or Vulkan. (The `upscaler` setting's other choices, Catmull-Rom and FSR 1, work on one frame and
//! need none of this: `encode_upscale`.)
//!
//! Neither vendor's code is linked or in the repository. Their DLLs are opened from the game's
//! folder when the first frame asks for them (`omsi_cfg::upscaler_files`), and without them the
//! game neither offers the choice nor changes anything.
//!
//! How a frame goes is in `frame.rs`, the two vendors' APIs in `streamline.rs` and
//! `fidelityfx.rs`, the motion's shaders in `motion.wgsl` (`pipelines/temporal.rs`).

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

/// Which upscaler draws the picture: what `upscaler=` names in settings.cfg beside the
/// single-frame ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemporalUpscaler {
    /// NVIDIA DLSS through Streamline (GeForce RTX).
    Dlss,
    /// AMD FSR 3 through FidelityFX (any card on DirectX 12).
    Fsr3,
}

impl TemporalUpscaler {
    /// The settings' word; None for the single-frame upscalers and anything unknown.
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
}

/// How much the upscaler scales up: the lower the quality, the smaller the picture drawn.
/// `Native` draws at the window's size and only smooths the edges (DLSS calls it DLAA).
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
    /// `upscale_quality=` in settings.cfg; anything unknown is the default.
    pub fn parse(s: &str) -> TemporalQuality {
        match s.trim().to_ascii_lowercase().as_str() {
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

    /// Width and height of the drawn picture over the window's, by the ratios the vendors
    /// publish (FSR's are DLSS's to a pixel).
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

/// The picture's size for a window of `(w, h)` at this quality, by the published ratios
/// (rounded, never below one pixel).
pub fn fallback_render_size(quality: TemporalQuality, (w, h): (u32, u32)) -> (u32, u32) {
    let s = quality.scale();
    (((w as f32 * s).round() as u32).max(1), ((h as f32 * s).round() as u32).max(1))
}

/// Temporal upscaling as the settings ask for it (`RenderOptions::temporal`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Temporal {
    pub upscaler: TemporalUpscaler,
    pub quality: TemporalQuality,
}

/// The textures of one upscaled frame (the vendors get the D3D12 resources under them).
pub struct Inputs<'a> {
    /// The finished 3D picture, drawn small.
    pub color: &'a wgpu::Texture,
    /// Its depth from the prepass, reversed Z (1 near, 0 far).
    pub depth: &'a wgpu::Texture,
    /// Per pixel, how far its point lies from where it was a frame ago (Rg16Float, pixels).
    pub motion: &'a wgpu::Texture,
    /// Where glass, its reflections and the rain on it lie (R16Float, see `vs_mask`): DLSS's
    /// bias-current-colour hint, FSR 3's reactive mask.
    pub mask: &'a wgpu::Texture,
    /// Where the window-sized picture goes (Rgba8Unorm, a storage texture).
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
    /// This frame's sub-pixel shift of the projection, in pixels.
    pub jitter: [f32; 2],
    pub camera_pos: [f32; 3],
    pub camera_up: [f32; 3],
    pub camera_right: [f32; 3],
    pub camera_fwd: [f32; 3],
    pub near: f32,
    pub far: f32,
    /// Vertical field of view in radians.
    pub fov: f32,
    pub aspect: f32,
    /// Throw the accumulated history away: the first frame, a new size, a camera cut.
    pub reset: bool,
    /// Since the last frame, in milliseconds.
    pub frame_ms: f32,
}

/// For DLSS on Vulkan, before wgpu makes the device: Streamline started and the device
/// extensions it needs (`streamline::vulkan_extensions`).
#[cfg(windows)]
pub(crate) fn dlss_vulkan_extensions() -> anyhow::Result<Vec<&'static std::ffi::CStr>> {
    streamline::vulkan_extensions()
}

/// Streamline started by `dlss_vulkan_extensions` let go again (the device was made without).
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

/// The vendors' runtimes outside Windows, which never run: `Renderer::new` keeps
/// `RenderOptions::temporal` to DirectX 12.
#[cfg(not(windows))]
mod streamline {
    use super::{fallback_render_size, FrameConstants, Inputs, TemporalQuality};
    use anyhow::Result;

    pub struct Runtime;

    impl Runtime {
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
        pub fn load(_device: &wgpu::Device) -> Result<Upscaler> {
            anyhow::bail!("FSR 3 upscaling is Windows-only (DirectX 12)")
        }
        pub fn upscale(&mut self, _queue: &wgpu::Queue, _inputs: &Inputs, _c: &FrameConstants) -> Result<()> {
            anyhow::bail!("FSR 3 upscaling is Windows-only (DirectX 12)")
        }
    }
}
