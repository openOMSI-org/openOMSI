//! The pieces most of the renderer's pipelines and samplers share.

use crate::DEPTH_FORMAT;
pub(crate) use crate::shaders::{HeapGroup, HeapModule, picture_wgsl};

/// A native compiler can abort before mX returns an error. Record both sides of each
/// operation so the last unfinished one identifies the module/entry point, not just "sky"
/// (at debug level: two lines a shader and a pipeline; the texts are only made when logged).
fn logged<T>(
    begin: impl FnOnce() -> String,
    end: impl FnOnce() -> String,
    op: impl FnOnce() -> T,
) -> T {
    let level = log::Level::Debug;
    if !log::log_enabled!(level) {
        return op();
    }
    let thread = std::thread::current();
    let started = std::time::Instant::now();
    log::log!(
        level,
        "renderer: {}, thread {:?} ({})",
        begin(),
        thread.id(),
        thread.name().unwrap_or("unnamed")
    );
    let done = op();
    log::log!(
        level,
        "renderer: {}, thread {:?}, {:.1} ms",
        end(),
        thread.id(),
        started.elapsed().as_secs_f64() * 1000.0
    );
    done
}

pub(crate) fn compile_shader(
    device: &mx::Device,
    descriptor: mx::ShaderModuleDescriptor<'_>,
) -> mx::ShaderModule {
    let label = descriptor.label.unwrap_or("unnamed").to_string();
    logged(
        || format!("shader begin {label}"),
        || format!("shader end {label}"),
        || {
            device
                .create_shader_module(&descriptor)
                .expect("mX: a shader")
        },
    )
}

pub(crate) fn compile_pipeline(
    device: &mx::Device,
    descriptor: &mx::RenderPipelineDescriptor<'_>,
) -> mx::RenderPipeline {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let fragment = descriptor.fragment.as_ref();
    let begin = || {
        format!(
            "pipeline begin #{id} {}, vs {}, fs {}, {}x MSAA, constants {:?}",
            descriptor.label.unwrap_or("unnamed"),
            descriptor.vertex.entry_point.unwrap_or("auto"),
            fragment.and_then(|f| f.entry_point).unwrap_or("none"),
            descriptor.multisample.count,
            fragment.map(|f| f.constants)
        )
    };
    logged(
        begin,
        || format!("pipeline end #{id}"),
        || {
            device
                .create_render_pipeline(descriptor)
                .unwrap_or_else(|e| panic!("mX: a render pipeline ({}): {e:?}", begin()))
        },
    )
}

/// A pipeline that draws without vertex buffers (a triangle over the screen, or quads the
/// vertex shader makes up): both stages from one module, triangles, none culled. By
/// default `vs_main`, no depth buffer and one sample.
pub(crate) struct NoVertexPipeline<'a> {
    label: &'a str,
    module: &'a mx::ShaderModule,
    vs: &'a str,
    fs: &'a str,
    targets: &'a [Option<mx::ColorTargetState>],
    depth_stencil: Option<mx::DepthStencilState>,
    samples: u32,
    strip: bool,
}

pub(crate) fn no_vertex_pipeline<'a>(
    label: &'a str,
    module: &'a mx::ShaderModule,
    fs: &'a str,
    targets: &'a [Option<mx::ColorTargetState>],
) -> NoVertexPipeline<'a> {
    NoVertexPipeline {
        label,
        module,
        vs: "vs_main",
        fs,
        targets,
        depth_stencil: None,
        samples: 1,
        strip: false,
    }
}

impl<'a> NoVertexPipeline<'a> {
    pub(crate) fn vs(self, vs: &'a str) -> Self {
        NoVertexPipeline { vs, ..self }
    }

    pub(crate) fn depth(self, depth_stencil: mx::DepthStencilState) -> Self {
        NoVertexPipeline {
            depth_stencil: Some(depth_stencil),
            ..self
        }
    }

    /// Quads as triangle strips of four vertices, not lists of six (the vertex shader makes
    /// them up, and runs its work once for each).
    pub(crate) fn strip(self, strip: bool) -> Self {
        NoVertexPipeline { strip, ..self }
    }

    pub(crate) fn samples(self, samples: u32) -> Self {
        NoVertexPipeline { samples, ..self }
    }

    pub(crate) fn create(self, device: &mx::Device) -> mx::RenderPipeline {
        compile_pipeline(
            device,
            &mx::RenderPipelineDescriptor {
                label: Some(self.label),
                vertex: mx::VertexState {
                    constants: &[],
                    module: self.module,
                    entry_point: Some(self.vs),
                    buffers: &[],
                },
                primitive: mx::PrimitiveState {
                    topology: if self.strip {
                        mx::PrimitiveTopology::TriangleStrip
                    } else {
                        mx::PrimitiveTopology::TriangleList
                    },
                    cull_mode: None,
                    front_face: mx::FrontFace::Ccw,
                    ..Default::default()
                },
                depth_stencil: self.depth_stencil,
                multisample: mx::MultisampleState {
                    count: self.samples,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                fragment: Some(mx::FragmentState {
                    constants: &[],
                    module: self.module,
                    entry_point: Some(self.fs),
                    targets: self.targets,
                }),
            },
        )
    }
}

/// The depth test of something drawn over the scene that writes no depth of its own.
pub(crate) fn depth_test(compare: mx::CompareFunction) -> mx::DepthStencilState {
    mx::DepthStencilState {
        format: DEPTH_FORMAT,
        depth_write_enabled: false,
        depth_compare: compare,
        stencil: Default::default(),
        bias: Default::default(),
    }
}

/// One colour target, every channel written.
pub(crate) fn target(
    format: mx::TextureFormat,
    blend: Option<mx::BlendState>,
) -> Option<mx::ColorTargetState> {
    Some(mx::ColorTargetState {
        format,
        blend,
        write_mask: mx::ColorWrites::ALL,
    })
}

/// A material texture sampler: linear, mip-mapped, anisotropic, `address` on all axes.
pub(crate) fn texture_sampler(
    device: &mx::Device,
    address: mx::AddressMode,
    anisotropy: u16,
) -> mx::Sampler {
    device
        .create_sampler(&mx::SamplerDescriptor {
            address_mode_u: address,
            address_mode_v: address,
            address_mode_w: address,
            mag_filter: mx::FilterMode::Linear,
            min_filter: mx::FilterMode::Linear,
            mipmap_filter: mx::MipmapFilterMode::Linear,
            anisotropy_clamp: anisotropy,
            ..Default::default()
        })
        .expect("mX: a sampler")
}

/// A linear sampler clamped to the edge, without mip maps.
pub(crate) fn clamped_linear_sampler(device: &mx::Device) -> mx::Sampler {
    device
        .create_sampler(&mx::SamplerDescriptor {
            address_mode_u: mx::AddressMode::ClampToEdge,
            address_mode_v: mx::AddressMode::ClampToEdge,
            mag_filter: mx::FilterMode::Linear,
            min_filter: mx::FilterMode::Linear,
            ..Default::default()
        })
        .expect("mX: a sampler")
}

/// A uniform buffer the CPU writes.
/// A uniform buffer the scene shader reads through the heap too (`frame_from_heap`).
pub(crate) fn frame_buffer(device: &mx::Device, label: &str, size: u64) -> mx::Buffer {
    device
        .create_buffer(&mx::BufferDescriptor {
            label: Some(label),
            size,
            usage: mx::BufferUsages::UNIFORM
                | mx::BufferUsages::STORAGE
                | mx::BufferUsages::COPY_DST,
        })
        .expect("mX: a buffer")
}

/// A read-only view of all of `buffer` in the heap.
pub(crate) fn heap_view(buffer: &mx::Buffer) -> mx::BufferView {
    buffer
        .create_view(&Default::default())
        .expect("mX: a buffer view")
}

/// A shader's uniform buffer (read through the heap: `groups_to_heap`).
pub(crate) fn uniform_buffer(device: &mx::Device, label: &str, size: u64) -> mx::Buffer {
    frame_buffer(device, label, size)
}
