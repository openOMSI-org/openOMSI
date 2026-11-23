//! The HUD's overlay quads, and the VR interface drawn with the same layout.

use super::common::*;

const PREMUL: mx::BlendState = mx::BlendState {
    color: mx::BlendComponent {
        src_factor: mx::BlendFactor::One,
        dst_factor: mx::BlendFactor::OneMinusSrcAlpha,
        operation: mx::BlendOperation::Add,
    },
    alpha: mx::BlendComponent::OVER,
};

pub(crate) struct Overlays {
    shader: mx::ShaderModule,
    pub pipeline: mx::RenderPipeline,
}

impl Overlays {
    pub(crate) fn new(device: &mx::Device, format: mx::TextureFormat, msaa: u32) -> Overlays {
        // HUD overlay quads
        log::info!("renderer: compiling the overlays shaders");
        let overlay_shader = compile_shader(
            device,
            mx::ShaderModuleDescriptor {
                label: Some("overlay"),
                source: &*(crate::shaders::hot_wgsl(
                    include_str!("../overlay.wgsl"),
                    "overlay.wgsl",
                )),
            },
        );
        let overlay_pipeline = no_vertex_pipeline(
            "overlay",
            &overlay_shader,
            "fs_main",
            &[target(format, Some(PREMUL))],
        )
        .depth(depth_test(mx::CompareFunction::Always))
        .samples(msaa)
        .create(device);
        Overlays {
            shader: overlay_shader,
            pipeline: overlay_pipeline,
        }
    }

    /// The overlay pipeline without multisampling or depth, and the VR interface's.
    pub(crate) fn single_sampled(
        &self,
        device: &mx::Device,
        format: mx::TextureFormat,
    ) -> (mx::RenderPipeline, mx::RenderPipeline) {
        let targets = [target(format, Some(PREMUL))];
        let overlay_pipeline_1x =
            no_vertex_pipeline("overlay 1x", &self.shader, "fs_main", &targets).create(device);
        log::info!("renderer: compiling the VR interface shaders");
        let xr_ui_shader = compile_shader(
            device,
            mx::ShaderModuleDescriptor {
                label: Some("OpenXR spatial UI"),
                source: &*(crate::shaders::hot_wgsl(include_str!("../xr_ui.wgsl"), "xr_ui.wgsl")),
            },
        );
        let xr_ui_pipeline =
            no_vertex_pipeline("OpenXR spatial UI", &xr_ui_shader, "fs_main", &targets)
                .create(device);
        (overlay_pipeline_1x, xr_ui_pipeline)
    }
}
