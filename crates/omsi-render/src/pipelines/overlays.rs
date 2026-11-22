//! The HUD's overlay quads, and the VR interface drawn with the same layout.

use super::common::*;

const PREMUL: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent::OVER,
};

pub(crate) struct Overlays {
    pub layout: wgpu::BindGroupLayout,
    shader: wgpu::ShaderModule,
    pl: wgpu::PipelineLayout,
    pub pipeline: wgpu::RenderPipeline,
}

impl Overlays {
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat, msaa: u32) -> Overlays {
        // HUD overlay quads
        let overlay_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("overlay"),
            entries: &[
                uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT),
                float_texture_entry(1),
                sampler_entry(2),
            ],
        });
        log::info!("renderer: compiling the overlays shaders");
        let overlay_shader = compile_shader(
            device,
            wgpu::ShaderModuleDescriptor {
                label: Some("overlay"),
                source: wgpu::ShaderSource::Wgsl(
                    crate::shaders::hot_wgsl(include_str!("../overlay.wgsl"), "overlay.wgsl")
                        .into(),
                ),
            },
        );
        let overlay_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("overlay"),
            bind_group_layouts: &[Some(&overlay_layout)],
            immediate_size: 0,
        });
        let overlay_pipeline = no_vertex_pipeline(
            "overlay",
            &overlay_pl,
            &overlay_shader,
            "fs_main",
            &[target(format, Some(PREMUL))],
        )
        .depth(depth_test(wgpu::CompareFunction::Always))
        .samples(msaa)
        .create(device);
        Overlays {
            layout: overlay_layout,
            shader: overlay_shader,
            pl: overlay_pl,
            pipeline: overlay_pipeline,
        }
    }

    /// The overlay pipeline without multisampling or depth, and the VR interface's.
    pub(crate) fn single_sampled(
        &self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> (wgpu::RenderPipeline, wgpu::RenderPipeline) {
        let targets = [target(format, Some(PREMUL))];
        let overlay_pipeline_1x =
            no_vertex_pipeline("overlay 1x", &self.pl, &self.shader, "fs_main", &targets)
                .create(device);
        log::info!("renderer: compiling the VR interface shaders");
        let xr_ui_shader = compile_shader(
            device,
            wgpu::ShaderModuleDescriptor {
                label: Some("OpenXR spatial UI"),
                source: wgpu::ShaderSource::Wgsl(
                    crate::shaders::hot_wgsl(include_str!("../xr_ui.wgsl"), "xr_ui.wgsl").into(),
                ),
            },
        );
        let xr_ui_pipeline = no_vertex_pipeline(
            "OpenXR spatial UI",
            &self.pl,
            &xr_ui_shader,
            "fs_main",
            &targets,
        )
        .create(device);
        (overlay_pipeline_1x, xr_ui_pipeline)
    }
}
