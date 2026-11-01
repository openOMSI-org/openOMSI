//! The window's picture on ANGLE (DirectX 11). ANGLE's Direct3D 11 window surfaces keep no
//! sRGB colour space (EGL_GL_COLORSPACE is taken for its D3D texture surfaces only), and
//! wgpu's GL backend presents its sRGB swapchain image by a framebuffer blit, which reads
//! the sRGB image back as linear light into that plain window: the game and the launcher
//! were shown far too dark. There the window is configured as plain RGBA8, every frame is
//! drawn into an sRGB stand-in as before, and one pass encodes it into the window.

const SHADER: &str = "
@group(0) @binding(0) var picture: texture_2d<f32>;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let c = textureLoad(picture, vec2<i32>(at.xy), 0);
    let low = c.rgb * 12.92;
    let high = 1.055 * pow(c.rgb, vec3<f32>(1.0 / 2.4)) - 0.055;
    return vec4<f32>(select(high, low, c.rgb <= vec3<f32>(0.0031308)), c.a);
}
";

pub(crate) struct SrgbEncode {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    /// The stand-in, its view and the bind group reading it, at the window's size.
    stand_in: std::cell::RefCell<Option<(wgpu::Texture, wgpu::TextureView, wgpu::BindGroup)>>,
    format: wgpu::TextureFormat,
}

impl SrgbEncode {
    /// For a window of the sRGB `format` on ANGLE, else none.
    pub(crate) fn wanted(renderer: &crate::Renderer, format: wgpu::TextureFormat) -> Option<Self> {
        if !(cfg!(windows) && crate::gl_backend() && renderer.adapter_name.contains("ANGLE") && format.is_srgb()) {
            return None;
        }
        let device = &renderer.device;
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("sRGB encode"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sRGB encode"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sRGB encode"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sRGB encode"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(format.remove_srgb_suffix().into())],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        log::info!("{}: the window keeps no sRGB colour space; each frame is encoded into it", renderer.adapter_name);
        Some(SrgbEncode { pipeline, layout, stand_in: Default::default(), format })
    }

    /// The window's own format: plain RGBA8 where the frame is encoded into it.
    pub(crate) fn window_format(&self) -> wgpu::TextureFormat {
        self.format.remove_srgb_suffix()
    }

    /// The view a frame of `size` is drawn into.
    pub(crate) fn view(&self, device: &wgpu::Device, size: wgpu::Extent3d) -> wgpu::TextureView {
        let mut stand_in = self.stand_in.borrow_mut();
        if stand_in.as_ref().is_none_or(|(t, _, _)| t.size() != size) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("sRGB stand-in"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                // (copied from as well: the glass behind the game's screens, see `SurfaceState::picture`)
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("sRGB encode"),
                layout: &self.layout,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }],
            });
            *stand_in = Some((texture, view, group));
        }
        stand_in.as_ref().map(|(_, v, _)| v.clone()).expect("stand-in")
    }

    /// The stand-in the frame is drawn into, when there is one.
    pub(crate) fn texture(&self) -> Option<wgpu::Texture> {
        self.stand_in.borrow().as_ref().map(|(t, _, _)| t.clone())
    }

    /// Encodes the stand-in into the window's `frame`.
    pub(crate) fn encode(&self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &wgpu::SurfaceTexture) {
        let stand_in = self.stand_in.borrow();
        let Some((_, _, group)) = stand_in.as_ref() else { return };
        let target = frame.texture.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("sRGB encode") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sRGB encode"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
    }
}

#[cfg(test)]
mod tests {
    /// The encoding pass is valid WGSL.
    #[test]
    fn the_shader_validates() {
        let module = wgpu::naga::front::wgsl::parse_str(super::SHADER).expect("parse");
        wgpu::naga::valid::Validator::new(wgpu::naga::valid::ValidationFlags::all(), wgpu::naga::valid::Capabilities::empty())
            .validate(&module)
            .expect("validate");
    }
}
