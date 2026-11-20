//! The render scale: the smaller 3D picture scaled up to the window.

use super::common::*;

pub(crate) struct Upscale {
    pub upscale_pipeline: mx::RenderPipeline,
    pub fsr: Option<Fsr1>,
    pub copy_pipeline: mx::RenderPipeline,
    pub panel_pipeline: mx::RenderPipeline,
    pub buf: mx::Buffer,
}

pub(crate) fn build(device: &mx::Device, format: mx::TextureFormat, use_fsr1: bool) -> Upscale {
    // --- render scale: the smaller 3D picture scaled up to the window
    log::info!("renderer: compiling the upscaler shaders");
    let upscale_shader = compile_shader(
        device,
        mx::ShaderModuleDescriptor {
            label: Some("upscale"),
            source: &*(include_str!("../upscale.wgsl")),
        },
    );
    let targets = [target(format, None)];
    let upscale_pipeline_for =
        |entry| no_vertex_pipeline("upscale", &upscale_shader, entry, &targets).create(device);
    let upscale_pipeline = upscale_pipeline_for("fs_main");
    let copy_pipeline = upscale_pipeline_for("fs_copy");
    // Keep the experimental shader off the default code path: some mobile/GL drivers
    // fail during shader compilation, even if a compiled pipeline is never drawn.
    let fsr = use_fsr1.then(|| build_fsr(device, format)).flatten();
    let panel_pipeline = upscale_pipeline_for("fs_panel");
    let upscale_buf = uniform_buffer(device, "upscale params", 16);
    Upscale {
        upscale_pipeline,
        fsr,
        copy_pipeline,
        panel_pipeline,
        buf: upscale_buf,
    }
}

/// FSR 1's passes for pictures of `format`: EASU to the window (no sharpening), EASU into
/// RCAS's intermediate, and RCAS.
pub(crate) struct Fsr1 {
    pub easu: mx::RenderPipeline,
    pub easu_mid: mx::RenderPipeline,
    pub rcas: mx::RenderPipeline,
    /// The scene is read through a view that is not sRGB (`shaders::fsr1_raw_input`).
    pub raw_input: bool,
    /// The intermediate's format: EASU's perceptual values as they are (RCAS reads them back
    /// without the square roots), in ten bits a channel - in eight they were rounded once
    /// there and again into the window's sRGB, a step off on a third of the pixels; the same
    /// 32 bits a pixel as the window's.
    pub mid_format: mx::TextureFormat,
}

/// FSR 1's passes (`Fsr1`) for pictures of `format`, built when the player first asks for
/// them - at the start, or later from the menu. None when the driver refuses them (a
/// validation, internal or out-of-memory error, caught here (`optional_pipeline`) rather
/// than by the device's handler, which would take it for the full renderer failing and turn
/// MSAA off): the standard upscaler stays, and the log says why.
pub(crate) fn build_fsr(device: &mx::Device, format: mx::TextureFormat) -> Option<Fsr1> {
    let raw_input = crate::shaders::fsr1_raw_input(format);
    log::info!(
        "renderer: compiling the FSR 1 shaders{}",
        if raw_input {
            " (the scene read raw)"
        } else {
            ""
        }
    );
    crate::gpu::optional_pipeline(device, "FSR 1 (the standard upscaler stays)", || {
        let shader = compile_shader(
            device,
            mx::ShaderModuleDescriptor {
                label: Some("fsr1 EASU/RCAS"),
                source: &*(crate::shaders::fsr1_source_for(format, raw_input)),
            },
        );
        let mid_format = mx::TextureFormat::Rgb10a2Unorm;
        let pass = |entry, f| {
            no_vertex_pipeline("fsr1", &shader, entry, &[target(f, None)]).create(device)
        };
        let (easu, easu_mid, rcas) = (
            pass("fs_easu", format),
            pass("fs_easu_mid", mid_format),
            pass("fs_rcas", format),
        );
        Fsr1 {
            easu,
            easu_mid,
            rcas,
            raw_input,
            mid_format,
        }
    })
}
