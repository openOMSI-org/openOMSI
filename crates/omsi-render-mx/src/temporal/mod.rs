//! Temporal upscaling: NVIDIA DLSS, AMD FSR 3, Intel XeSS or Apple MetalFX draw the picture
//! smaller, take the last frames' pixels along by each pixel's motion and bring it up to the
//! window; frame generation adds pictures between the drawn ones. The vendors' code is driven by
//! mX's `mx-upscale` on mX's own device, queue and textures; this module holds the game's side:
//! the settings' words, the jitter, the motion vectors, the textures and the HUD
//! (`frame.rs`).
//!
//! (The `upscaler` setting's other choices, Catmull-Rom and FSR 1, work on one frame and need
//! none of this: `encode_upscale`.)
//!
//! No vendor's code is linked or in the repository. The Windows x64 package carries AMD's
//! DLLs; NVIDIA's and Intel's come from the launcher's optional installer; MetalFX is part of
//! macOS 13 and newer. Without them the game neither offers the choice nor changes anything
//! (`omsi_cfg::upscaler_files`).

mod frame;
#[cfg(test)]
mod tests;

pub(crate) use frame::*;
pub(crate) use mx_upscale::{Constants as FrameConstants, Inputs};

/// How the game runs the vendors: their libraries from the game's folder.
pub(crate) fn upscale_options() -> mx_upscale::Options {
    let show = match omsi_cfg::flags::OMSI_FG_SHOW
        .var()
        .map(|v| v.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("generated") => mx_upscale::Show::Generated,
        Some("drawn") => mx_upscale::Show::Drawn,
        _ => mx_upscale::Show::Both,
    };
    mx_upscale::Options {
        library_dir: omsi_cfg::upscaler_files::program_dir(),
        show,
        ..Default::default()
    }
}

/// What makes the in-between frames (the `frame_generation` setting).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameGenerationKind {
    /// Apple MetalFX, on a Mac.
    MetalFx,
    /// NVIDIA DLSS Frame Generation: 1 to 3 pictures between two drawn ones.
    Dlss(u32),
    /// AMD FSR 3.
    Fsr3,
    /// Intel XeSS (DirectX 12).
    Xess,
}

impl FrameGenerationKind {
    pub fn parse(s: &str) -> Option<FrameGenerationKind> {
        match omsi_cfg::upscaler_files::frame_generation_word(s) {
            "metalfx" => Some(FrameGenerationKind::MetalFx),
            "dlss2x" => Some(FrameGenerationKind::Dlss(1)),
            "dlss3x" => Some(FrameGenerationKind::Dlss(2)),
            "dlss4x" => Some(FrameGenerationKind::Dlss(3)),
            "fsr3" => Some(FrameGenerationKind::Fsr3),
            "xess" => Some(FrameGenerationKind::Xess),
            _ => None,
        }
    }

    /// The upscaler it takes its depth and motion vectors from, and cannot run without.
    pub fn upscaler(self) -> TemporalUpscaler {
        match self {
            FrameGenerationKind::MetalFx => TemporalUpscaler::MetalFx,
            FrameGenerationKind::Dlss(_) => TemporalUpscaler::Dlss,
            FrameGenerationKind::Fsr3 => TemporalUpscaler::Fsr3,
            FrameGenerationKind::Xess => TemporalUpscaler::Xess,
        }
    }

    pub(crate) fn method(self) -> mx_upscale::FrameGenMethod {
        match self {
            FrameGenerationKind::MetalFx => mx_upscale::FrameGenMethod::MetalFx,
            FrameGenerationKind::Dlss(generated) => mx_upscale::FrameGenMethod::Dlss { generated },
            FrameGenerationKind::Fsr3 => mx_upscale::FrameGenMethod::Fsr3,
            FrameGenerationKind::Xess => mx_upscale::FrameGenMethod::Xess,
        }
    }
}

/// Frame generation's handle: the renderer hands it each upscaled frame, the window's present
/// takes it from there.
#[derive(Clone)]
pub struct FrameGeneration(Handle);

#[derive(Clone)]
enum Handle {
    Generator(std::sync::Arc<std::sync::Mutex<Generation>>),
    /// Apple's interpolator alone, which the program drives by hand (tests).
    #[cfg(target_os = "macos")]
    Interpolator(mx_upscale::MetalFxInterpolator),
}

pub(crate) struct Generation {
    generator: mx_upscale::FrameGenerator,
    /// A frame was handed over since the last present.
    prepared: bool,
}

impl FrameGeneration {
    /// Apple's interpolator, frames handed to it and taken from it by hand.
    #[cfg(target_os = "macos")]
    pub fn interpolator(i: mx_upscale::MetalFxInterpolator) -> FrameGeneration {
        FrameGeneration(Handle::Interpolator(i))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Generation> {
        match &self.0 {
            Handle::Generator(g) => g.lock().unwrap_or_else(|e| e.into_inner()),
            #[cfg(target_os = "macos")]
            Handle::Interpolator(_) => unreachable!("a window's frame generation is a generator"),
        }
    }

    /// False once it gave up: the window then draws without it.
    pub(crate) fn working(&self) -> bool {
        match &self.0 {
            Handle::Generator(_) => self.lock().generator.working(),
            #[cfg(target_os = "macos")]
            Handle::Interpolator(i) => i.working(),
        }
    }

    /// The in-between frames made so far.
    pub(crate) fn generated(&self) -> u64 {
        match &self.0 {
            Handle::Generator(_) => self.lock().generator.generated(),
            #[cfg(target_os = "macos")]
            Handle::Interpolator(i) => i.generated(),
        }
    }

    /// An upscaled frame, for the window's present of it.
    pub(crate) fn prepare(&self, queue: &mx::Queue, inputs: mx_upscale::FrameInputs) {
        match &self.0 {
            Handle::Generator(_) => {
                let mut g = self.lock();
                if let Err(e) = g.generator.prepare(queue, inputs) {
                    log::debug!("frame generation: {e:#}");
                }
                g.prepared = true;
            }
            #[cfg(target_os = "macos")]
            Handle::Interpolator(i) => i.prepare(inputs),
        }
    }

    /// The GPU done with the frames handed over: before the textures they read are let go of.
    pub(crate) fn wait(&self) {
        match &self.0 {
            Handle::Generator(_) => self.lock().generator.wait(),
            #[cfg(target_os = "macos")]
            Handle::Interpolator(i) => i.wait(),
        }
    }

    /// Whether the vendor takes the last frame's picture too (MetalFX's interpolator).
    pub(crate) fn wants_last_picture(&self) -> bool {
        match &self.0 {
            Handle::Generator(_) => {
                self.lock().generator.method() == mx_upscale::FrameGenMethod::MetalFx
            }
            #[cfg(target_os = "macos")]
            Handle::Interpolator(_) => true,
        }
    }
}

/// The window's side of frame generation.
pub struct GenerationWindow {
    handle: FrameGeneration,
}

impl GenerationWindow {
    /// For a window of this format and size, on the renderer's device. A vendor that makes the
    /// window's swapchain needs the surface configured again (`SurfaceState` does).
    pub(crate) fn new(
        device: &mx::Device,
        queue: &mx::Queue,
        surface: &mx::Surface<'_>,
        kind: FrameGenerationKind,
        format: mx::TextureFormat,
        size: (u32, u32),
    ) -> anyhow::Result<GenerationWindow> {
        let generator = mx_upscale::FrameGenerator::new(
            device,
            queue,
            surface,
            kind.method(),
            format,
            size,
            &upscale_options(),
        )?;
        Ok(GenerationWindow {
            handle: FrameGeneration(Handle::Generator(std::sync::Arc::new(
                std::sync::Mutex::new(Generation {
                    generator,
                    prepared: false,
                }),
            ))),
        })
    }

    pub(crate) fn handle(&self) -> FrameGeneration {
        self.handle.clone()
    }

    pub(crate) fn working(&self) -> bool {
        self.handle.working()
    }

    /// Whether the vendor made the window's swapchain (the surface is configured again when it
    /// starts and stops).
    pub(crate) fn owns_swapchain(&self) -> bool {
        self.handle.lock().generator.method().owns_swapchain()
    }

    /// The window changed its size.
    pub(crate) fn resize(&mut self, device: &mx::Device, queue: &mx::Queue, size: (u32, u32)) {
        self.handle.lock().generator.resize(device, queue, size);
    }

    /// What the game draws the window's picture into (MetalFX); None: the window itself.
    pub(crate) fn view(&self) -> Option<mx::TextureView> {
        self.handle.lock().generator.window_view()
    }

    /// The same, to be copied from.
    pub(crate) fn texture(&self) -> Option<mx::Texture> {
        self.handle.lock().generator.window_texture()
    }

    /// Shows the frame: with the made pictures before it, or alone where none was handed over
    /// (a menu, the loading screen).
    pub(crate) fn present(
        &self,
        surface: &mx::Surface<'_>,
        device: &mx::Device,
        queue: &mx::Queue,
        frame: mx::SurfaceTexture,
    ) {
        let mut g = self.handle.lock();
        if !std::mem::take(&mut g.prepared) {
            g.generator.skip();
        }
        g.generator.present(surface, device, queue, frame);
    }

    /// Turned off: the window's own swapchain again where the vendor made it.
    pub(crate) fn stop(self, surface: &mx::Surface<'_>) {
        self.handle.wait();
        let Handle::Generator(g) = self.handle.0 else {
            return;
        };
        match std::sync::Arc::try_unwrap(g) {
            Ok(g) => g
                .into_inner()
                .unwrap_or_else(|e| e.into_inner())
                .generator
                .stop(surface),
            Err(_) => log::warn!("frame generation: still held by the renderer as it stops"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemporalUpscaler {
    Dlss,
    Fsr3,
    MetalFx,
    Xess,
}

impl TemporalUpscaler {
    pub fn parse(s: &str) -> Option<TemporalUpscaler> {
        match s.trim().to_ascii_lowercase().as_str() {
            "dlss" => Some(TemporalUpscaler::Dlss),
            "fsr3" => Some(TemporalUpscaler::Fsr3),
            "metalfx" => Some(TemporalUpscaler::MetalFx),
            "xess" => Some(TemporalUpscaler::Xess),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            TemporalUpscaler::Dlss => "dlss",
            TemporalUpscaler::Fsr3 => "fsr3",
            TemporalUpscaler::MetalFx => "metalfx",
            TemporalUpscaler::Xess => "xess",
        }
    }

    pub(crate) fn method(self) -> mx_upscale::Method {
        match self {
            TemporalUpscaler::Dlss => mx_upscale::Method::Dlss,
            TemporalUpscaler::Fsr3 => mx_upscale::Method::Fsr3,
            TemporalUpscaler::MetalFx => mx_upscale::Method::MetalFxTemporal,
            TemporalUpscaler::Xess => mx_upscale::Method::Xess,
        }
    }

    /// FSR 3 gives up the history faster than DLSS for the same mask: its panes take a weaker one.
    /// MetalFX and XeSS read their masks as FSR 3 does (1 is "no history at all"), and take FSR 3's.
    pub(crate) fn glass_mask_entry(self) -> &'static str {
        match self {
            TemporalUpscaler::Dlss => "fs_glass_mask",
            TemporalUpscaler::Fsr3 | TemporalUpscaler::MetalFx | TemporalUpscaler::Xess => {
                "fs_glass_mask_fsr"
            }
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

    pub(crate) fn mx(self) -> mx_upscale::Quality {
        match self {
            TemporalQuality::Native => mx_upscale::Quality::Native,
            TemporalQuality::Quality => mx_upscale::Quality::Quality,
            TemporalQuality::Balanced => mx_upscale::Quality::Balanced,
            TemporalQuality::Performance => mx_upscale::Quality::Performance,
            TemporalQuality::UltraPerformance => mx_upscale::Quality::UltraPerformance,
        }
    }

    pub fn scale(self) -> f32 {
        self.mx().scale()
    }
}

pub fn fallback_render_size(quality: TemporalQuality, out: (u32, u32)) -> (u32, u32) {
    mx_upscale::render_size(quality.mx(), out)
}

/// The size MetalFX draws at, and whether it is larger than the quality asked for: Apple's
/// scaler goes up to `max_scale` times a side (`supportedInputContentMaxScaleForDevice`), which
/// is less than Ultra performance's three on most Macs.
pub fn metalfx_render_size(
    quality: TemporalQuality,
    out: (u32, u32),
    max_scale: f32,
) -> ((u32, u32), bool) {
    mx_upscale::metalfx_render_size(quality.mx(), out, max_scale)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Temporal {
    pub upscaler: TemporalUpscaler,
    pub quality: TemporalQuality,
}

/// The device extensions DLSS needs of a Vulkan device (Streamline started for it, before the
/// device is made).
pub(crate) fn dlss_vulkan_extensions() -> anyhow::Result<Vec<&'static std::ffi::CStr>> {
    mx_upscale::Upscaler::dlss_vulkan_extensions(&upscale_options())
}

pub(crate) fn dlss_vulkan_abandon() {
    mx_upscale::Upscaler::dlss_vulkan_abandon()
}
