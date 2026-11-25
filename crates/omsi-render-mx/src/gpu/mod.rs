//! The graphics device: what the adapter can do (texture units, storage arrays, GL),
//! the bind group layouts that follow from it, waiting for the GPU, and its errors.

pub(crate) mod schema;

use crate::*;

mod auxiliary;
mod fallbacks;
mod open;
mod timers;
mod upscaler_device;
pub use auxiliary::with_auxiliary_renderer;
pub(crate) use auxiliary::{
    AdapterBool, auxiliary_context, update_ceiling, with_auxiliary_context,
};

#[cfg(test)]
mod isolation_tests;

pub use fallbacks::*;
pub use open::*;
pub(crate) use timers::*;

/// Texture memory (MB) the adapter is taken to have room for (0 = no adapter yet), see
/// `Renderer::new`.
pub static ADAPTER_TEXTURE_MB: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// The discrete card's own memory (MB) where the system tells it (0 = not known, or not a
/// discrete card), see `Renderer::new`.
pub static ADAPTER_VRAM_MB: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// The adapter the device was opened on (its name, ids, backend and driver), see `Renderer::new`.
pub static ADAPTER_INFO: std::sync::Mutex<Option<mx::AdapterInfo>> = std::sync::Mutex::new(None);

pub(crate) static CEILING: std::sync::Mutex<Option<crate::gpu_memory::Ceiling>> =
    std::sync::Mutex::new(None);

pub fn ceiling() -> Option<crate::gpu_memory::Ceiling> {
    update_ceiling(|c| *c)
}

/// The device runs on OpenGL (set in `Renderer::new`).
pub(crate) static GL_BACKEND: auxiliary::AdapterBool = auxiliary::AdapterBool::new(0);
pub(crate) static AMD_VULKAN: auxiliary::AdapterBool = auxiliary::AdapterBool::new(1);
pub(crate) static MULTISAMPLED_SHADING: auxiliary::AdapterBool = auxiliary::AdapterBool::new(2);

/// Whether the device draws on OpenGL (known once a renderer is made).
pub fn gl_backend() -> bool {
    GL_BACKEND.load(std::sync::atomic::Ordering::Relaxed)
}

/// The adapter is ANGLE's (OpenGL ES over Direct3D 11, Windows): its pipelines are made on
/// a thread of its own (see `angle::compile`).
pub(crate) fn is_angle(info: &mx::AdapterInfo) -> bool {
    cfg!(windows) && info.backend == mx::Backend::Gl && info.name.contains("ANGLE")
}

/// How the per-draw arrays (the model matrices, the instance parameters, the draw list) and
/// the point lights reach the scene shader on this device (set in `Renderer::new_on`).
/// Older OpenGL chips cannot read a storage buffer in a vertex shader - an Intel HD 2500, a
/// Mali on GLES ("Downlevel flags VERTEX_STORAGE are required but not supported", the
/// renderer was never made, #770, #316) - or have no storage buffers at all (OpenGL below
/// 4.3, GLES 3.0): for them the arrays are textures the vertex shader reads texel by texel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum ArrayPath {
    /// Storage buffers everywhere (every Metal, Vulkan and DirectX 12 device).
    Storage,
    /// The vertex shader's arrays are textures; the lights stay storage buffers.
    VertexTextures,
    /// No storage buffers at all: the arrays are textures, and the point lights (the street
    /// lamps, headlights and interior lamps lit per pixel) are left out - the sixteen
    /// textures a fragment shader may read here are all taken.
    NoStorage,
    LightTextures,
}

impl ArrayPath {
    pub(crate) fn no_storage(self) -> bool {
        matches!(self, ArrayPath::NoStorage | ArrayPath::LightTextures)
    }
}

pub(crate) static ARRAY_PATH: auxiliary::AdapterArrays = auxiliary::AdapterArrays::new();

pub(crate) fn array_path() -> ArrayPath {
    match ARRAY_PATH.load(std::sync::atomic::Ordering::Relaxed) {
        1 => ArrayPath::VertexTextures,
        2 => ArrayPath::NoStorage,
        3 => ArrayPath::LightTextures,
        _ => ArrayPath::Storage,
    }
}

/// Texels per row of an array kept as a texture (within every device's 2048).
pub(crate) const ARRAY_TEX_WIDTH: u32 = 2048;

/// A read-only array of the scene shader: a storage buffer, or where the device cannot
/// read one (see `ArrayPath`) a texture of one value per texel - four floats
/// (`Rgba32Float`, 16 bytes) or one `u32` (`R32Uint`) - in rows of `ARRAY_TEX_WIDTH`.
pub(crate) enum GpuArray {
    Buffer(mx::Buffer),
    Texture {
        texture: mx::Texture,
        view: mx::TextureView,
        texel: u32,
    },
    /// Not bound on this device (the lights under `ArrayPath::NoStorage`): takes any write.
    Unused,
}

impl GpuArray {
    /// An array of `size` bytes of `texel`-byte values (16 or 4), a texture where the
    /// vertex shader reads it on a device without vertex storage (`vertex`), or without any.
    pub(crate) fn new(
        device: &mx::Device,
        label: &str,
        size: u64,
        texel: u32,
        vertex: bool,
    ) -> GpuArray {
        let path = array_path();
        if path == ArrayPath::Storage || (!vertex && path == ArrayPath::VertexTextures) {
            return GpuArray::Buffer(
                device
                    .create_buffer(&mx::BufferDescriptor {
                        label: Some(label),
                        size: size.max(16).next_multiple_of(mx::COPY_BUFFER_ALIGNMENT),
                        // (copied from too: temporal upscaling keeps last frame's matrices)
                        usage: mx::BufferUsages::STORAGE
                            | mx::BufferUsages::COPY_DST
                            | mx::BufferUsages::COPY_SRC,
                    })
                    .expect("mX: a buffer"),
            );
        }
        if !vertex && path == ArrayPath::NoStorage {
            return GpuArray::Unused;
        }
        let texels = size.div_ceil(texel as u64).max(1);
        let max_rows = device.limits().max_texture_dimension_2d;
        let rows = (texels.div_ceil(ARRAY_TEX_WIDTH as u64) as u32).clamp(1, max_rows);
        if rows == max_rows {
            log::warn!(
                "{label}: {texels} values do not fit a texture of {ARRAY_TEX_WIDTH} x {max_rows}; the rest are not drawn"
            );
        }
        let texture = device
            .create_texture(&mx::TextureDescriptor {
                label: Some(label),
                size: mx::Extent3d {
                    width: ARRAY_TEX_WIDTH,
                    height: rows,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: mx::TextureDimension::D2,
                format: if texel == 16 {
                    mx::TextureFormat::Rgba32Float
                } else {
                    mx::TextureFormat::R32Uint
                },
                usage: mx::TextureUsages::TEXTURE_BINDING | mx::TextureUsages::COPY_DST,
                view_formats: &[],
            })
            .expect("mX: a texture");
        let view = texture
            .create_view(&Default::default())
            .expect("mX: a texture view");
        GpuArray::Texture {
            texture,
            view,
            texel,
        }
    }

    /// Its room in bytes.
    pub(crate) fn size(&self) -> u64 {
        match self {
            GpuArray::Buffer(b) => b.size(),
            GpuArray::Texture { texture, texel, .. } => {
                texture.width() as u64 * texture.height() as u64 * *texel as u64
            }
            GpuArray::Unused => u64::MAX,
        }
    }

    /// Bytes it takes on the GPU (for the memory summary).
    pub(crate) fn gpu_bytes(&self) -> u64 {
        match self {
            GpuArray::Unused => 0,
            a => a.size(),
        }
    }

    /// Write `data` at byte `offset` (both whole values); what runs past its end is dropped.
    pub(crate) fn write(&self, queue: &mx::Queue, offset: u64, data: &[u8]) {
        match self {
            GpuArray::Buffer(b) => queue
                .write_buffer(b, offset, data)
                .expect("mX: a queue write"),
            GpuArray::Texture { texture, texel, .. } => {
                let t = *texel as usize;
                let total = texture.width() as u64 * texture.height() as u64;
                let mut data = data;
                for (x, y, width, rows) in
                    array_tex_spans(offset / t as u64, (data.len() / t) as u64, total)
                {
                    let bytes = (width * rows) as usize * t;
                    queue
                        .write_texture(
                            mx::TexelCopyTextureInfo {
                                texture,
                                mip_level: 0,
                                origin: mx::Origin3d { x, y, z: 0 },
                                aspect: mx::TextureAspect::All,
                            },
                            &data[..bytes],
                            mx::TexelCopyBufferLayout {
                                offset: 0,
                                bytes_per_row: Some(width * t as u32),
                                rows_per_image: None,
                            },
                            mx::Extent3d {
                                width,
                                height: rows,
                                depth_or_array_layers: 1,
                            },
                        )
                        .expect("mX: a queue write");
                    data = &data[bytes..];
                }
            }
            GpuArray::Unused => {}
        }
    }

    /// Its place in the heap (`frame_from_heap`): a buffer's view, kept in `views`, or the
    /// texture's own.
    pub(crate) fn heap_index(&self, views: &mut Vec<mx::BufferView>) -> u32 {
        match self {
            GpuArray::Buffer(b) => {
                let view = crate::pipelines::common::heap_view(b);
                let index = view.index();
                views.push(view);
                index
            }
            GpuArray::Texture { view, .. } => view.index(),
            GpuArray::Unused => 0,
        }
    }

    pub(crate) fn binding(&self) -> mx::BindingResource<'_> {
        match self {
            GpuArray::Buffer(b) => b.as_entire_binding(),
            GpuArray::Texture { view, .. } => mx::BindingResource::TextureView(view),
            GpuArray::Unused => unreachable!("an unused array is not bound"),
        }
    }
}

/// The rectangles (x, y, width, rows) of an array texture `ARRAY_TEX_WIDTH` wide and
/// `total` texels big that `n` values from value `at` on fill, in order: the rest of a row,
/// whole rows, the start of the last row. What runs past the end is left out.
pub(crate) fn array_tex_spans(mut at: u64, n: u64, total: u64) -> Vec<(u32, u32, u32, u32)> {
    let w = ARRAY_TEX_WIDTH as u64;
    let mut left = n.min(total.saturating_sub(at));
    let mut out = Vec::new();
    while left > 0 {
        let (x, y) = (at % w, at / w);
        let (width, rows) = if x == 0 && left >= w {
            (w, left / w)
        } else {
            ((w - x).min(left), 1)
        };
        out.push((x as u32, y as u32, width as u32, rows as u32));
        at += width * rows;
        left -= width * rows;
    }
    out
}

/// The bind group layout entry of a scene array read in `stage` (see `GpuArray`):
/// `float4` for vec4 values, else u32.
pub(crate) fn array_layout_entry(
    binding: u32,
    stage: crate::gpu::schema::ShaderStages,
    float4: bool,
) -> crate::gpu::schema::BindGroupLayoutEntry {
    array_layout_entry_on(array_path(), binding, stage, float4)
}

/// The same on a device whose arrays take `path`.
pub(crate) fn array_layout_entry_on(
    path: ArrayPath,
    binding: u32,
    stage: crate::gpu::schema::ShaderStages,
    float4: bool,
) -> crate::gpu::schema::BindGroupLayoutEntry {
    let texture = match path {
        ArrayPath::Storage => false,
        ArrayPath::VertexTextures => stage.contains(crate::gpu::schema::ShaderStages::VERTEX),
        ArrayPath::NoStorage | ArrayPath::LightTextures => true,
    };
    crate::gpu::schema::BindGroupLayoutEntry {
        binding,
        visibility: stage,
        ty: if texture {
            crate::gpu::schema::BindingType::Texture {
                sample_type: if float4 {
                    mx::TextureSampleType::Float { filterable: false }
                } else {
                    mx::TextureSampleType::Uint
                },
                view_dimension: mx::TextureViewDimension::D2,
                multisampled: false,
            }
        } else {
            crate::gpu::schema::BindingType::Buffer {
                ty: crate::gpu::schema::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            }
        },
        count: None,
    }
}

/// The camera group's textures only the enhanced path reads: the reflection probe, the sky
/// table and the window's clouds.
pub(crate) const ENHANCED_CAMERA_TEXTURES: [u32; 3] = [12, 14, 17];

/// wgpu's OpenGL backend has sixteen texture units for a pipeline - for all its groups and
/// both its shaders together (wgpu-hal's `MAX_TEXTURE_SLOTS`, whatever the chip has) - while
/// it lets through sixteen for each shader alone. With the per-draw arrays as textures (see
/// `ArrayPath`) the scene's camera and material groups held nineteen, and the renderer went
/// down making its first pipeline ("index out of bounds: the len is 16 but the index is 16"
/// in wgpu-hal's gles/device.rs, on the phones of #1133, #1154, #1162, #1176: OpenGL was
/// passed over for Vulkan and the panic reported as the game's end). There the camera group
/// leaves out the enhanced path's three textures, and the enhanced path is not made
/// (vanilla and vanilla+ draw as everywhere). OMSI_GL_TEXTURE_UNITS=1 takes this layout on
/// any device (with OMSI_GPU_ARRAYS, to try it without such a chip).
pub(crate) fn sixteen_texture_units() -> bool {
    array_path() != ArrayPath::Storage
        && (gl_backend() || omsi_cfg::flags::OMSI_GL_TEXTURE_UNITS.is_set())
}

/// The camera group's entries on a device whose arrays take `path`, without the enhanced
/// path's textures where `omit_enhanced`: on the sixteen-texture-unit path (see
/// `sixteen_texture_units`) and whenever no enhanced pipelines or reflection probe are
/// prepared (the launcher's preview, or the vanilla GL path).
pub(crate) fn camera_layout_entries(
    path: ArrayPath,
    omit_enhanced: bool,
) -> Vec<crate::gpu::schema::BindGroupLayoutEntry> {
    let mut camera_entries = vec![
        crate::gpu::schema::BindGroupLayoutEntry {
            binding: 0,
            visibility: crate::gpu::schema::ShaderStages::VERTEX_FRAGMENT,
            ty: crate::gpu::schema::BindingType::Buffer {
                ty: crate::gpu::schema::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
        array_layout_entry_on(path, 1, crate::gpu::schema::ShaderStages::VERTEX, true),
        array_layout_entry_on(path, 2, crate::gpu::schema::ShaderStages::VERTEX, true),
        crate::gpu::schema::BindGroupLayoutEntry {
            binding: 5,
            visibility: crate::gpu::schema::ShaderStages::FRAGMENT,
            ty: crate::gpu::schema::BindingType::Texture {
                sample_type: mx::TextureSampleType::Depth,
                view_dimension: mx::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        crate::gpu::schema::BindGroupLayoutEntry {
            binding: 6,
            visibility: crate::gpu::schema::ShaderStages::FRAGMENT,
            ty: crate::gpu::schema::BindingType::Sampler(
                crate::gpu::schema::SamplerBindingType::Comparison,
            ),
            count: None,
        },
        crate::gpu::schema::BindGroupLayoutEntry {
            binding: 7,
            visibility: crate::gpu::schema::ShaderStages::FRAGMENT,
            ty: crate::gpu::schema::BindingType::Texture {
                sample_type: mx::TextureSampleType::Depth,
                view_dimension: mx::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        crate::gpu::schema::BindGroupLayoutEntry {
            binding: 8,
            visibility: crate::gpu::schema::ShaderStages::FRAGMENT,
            ty: crate::gpu::schema::BindingType::Texture {
                sample_type: mx::TextureSampleType::Float { filterable: true },
                view_dimension: mx::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        crate::gpu::schema::BindGroupLayoutEntry {
            binding: 9,
            visibility: crate::gpu::schema::ShaderStages::FRAGMENT,
            ty: crate::gpu::schema::BindingType::Sampler(
                crate::gpu::schema::SamplerBindingType::Filtering,
            ),
            count: None,
        },
        array_layout_entry_on(path, 10, crate::gpu::schema::ShaderStages::VERTEX, false),
        // enhanced: its lighting, the reflection probe, a clamped linear sampler, the sky table
        crate::gpu::schema::BindGroupLayoutEntry {
            binding: 11,
            visibility: crate::gpu::schema::ShaderStages::VERTEX_FRAGMENT,
            ty: crate::gpu::schema::BindingType::Buffer {
                ty: crate::gpu::schema::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
        crate::gpu::schema::BindGroupLayoutEntry {
            binding: 12,
            visibility: crate::gpu::schema::ShaderStages::FRAGMENT,
            ty: crate::gpu::schema::BindingType::Texture {
                sample_type: mx::TextureSampleType::Float { filterable: true },
                view_dimension: mx::TextureViewDimension::Cube,
                multisampled: false,
            },
            count: None,
        },
        crate::gpu::schema::BindGroupLayoutEntry {
            binding: 13,
            visibility: crate::gpu::schema::ShaderStages::FRAGMENT,
            ty: crate::gpu::schema::BindingType::Sampler(
                crate::gpu::schema::SamplerBindingType::Filtering,
            ),
            count: None,
        },
        crate::gpu::schema::BindGroupLayoutEntry {
            binding: 14,
            visibility: crate::gpu::schema::ShaderStages::FRAGMENT,
            ty: crate::gpu::schema::BindingType::Texture {
                sample_type: mx::TextureSampleType::Float { filterable: true },
                view_dimension: mx::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        crate::gpu::schema::BindGroupLayoutEntry {
            binding: 17,
            visibility: crate::gpu::schema::ShaderStages::FRAGMENT,
            ty: crate::gpu::schema::BindingType::Texture {
                sample_type: mx::TextureSampleType::Float { filterable: true },
                view_dimension: mx::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        // the tile light maps around the camera, and where they lie
        crate::gpu::schema::BindGroupLayoutEntry {
            binding: 18,
            visibility: crate::gpu::schema::ShaderStages::FRAGMENT,
            ty: crate::gpu::schema::BindingType::Texture {
                sample_type: mx::TextureSampleType::Float { filterable: true },
                view_dimension: mx::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        crate::gpu::schema::BindGroupLayoutEntry {
            binding: 19,
            visibility: crate::gpu::schema::ShaderStages::FRAGMENT,
            ty: crate::gpu::schema::BindingType::Buffer {
                ty: crate::gpu::schema::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
        // the snow track field around the camera (road_snow.wgsl), and its place
        crate::gpu::schema::BindGroupLayoutEntry {
            binding: 20,
            visibility: crate::gpu::schema::ShaderStages::FRAGMENT,
            ty: crate::gpu::schema::BindingType::Buffer {
                ty: crate::gpu::schema::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
        crate::gpu::schema::BindGroupLayoutEntry {
            binding: 21,
            visibility: crate::gpu::schema::ShaderStages::FRAGMENT,
            ty: crate::gpu::schema::BindingType::Buffer {
                ty: crate::gpu::schema::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
    ];
    if !road_snow_supported() {
        camera_entries.retain(|e| e.binding != 20);
    }
    // the point lights and their grid (see `ArrayPath::NoStorage`)
    if path != ArrayPath::NoStorage {
        camera_entries.push(array_layout_entry_on(
            path,
            3,
            crate::gpu::schema::ShaderStages::FRAGMENT,
            true,
        ));
        camera_entries.push(array_layout_entry_on(
            path,
            4,
            crate::gpu::schema::ShaderStages::FRAGMENT,
            false,
        ));
    }
    if omit_enhanced {
        camera_entries.retain(|e| !ENHANCED_CAMERA_TEXTURES.contains(&e.binding));
    }
    camera_entries
}

/// The material group's entries: its textures, its sampler and its uniform buffer.
pub(crate) fn material_layout_entries() -> Vec<crate::gpu::schema::BindGroupLayoutEntry> {
    vec![
        crate::gpu::schema::BindGroupLayoutEntry {
            binding: 0,
            visibility: crate::gpu::schema::ShaderStages::FRAGMENT,
            ty: crate::gpu::schema::BindingType::Texture {
                sample_type: mx::TextureSampleType::Float { filterable: true },
                view_dimension: mx::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        crate::gpu::schema::BindGroupLayoutEntry {
            binding: 1,
            visibility: crate::gpu::schema::ShaderStages::FRAGMENT,
            ty: crate::gpu::schema::BindingType::Sampler(
                crate::gpu::schema::SamplerBindingType::Filtering,
            ),
            count: None,
        },
        crate::gpu::schema::BindGroupLayoutEntry {
            binding: 2,
            visibility: crate::gpu::schema::ShaderStages::VERTEX_FRAGMENT,
            ty: crate::gpu::schema::BindingType::Buffer {
                ty: crate::gpu::schema::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
        crate::gpu::schema::BindGroupLayoutEntry {
            binding: 3,
            visibility: crate::gpu::schema::ShaderStages::FRAGMENT,
            ty: crate::gpu::schema::BindingType::Texture {
                sample_type: mx::TextureSampleType::Float { filterable: true },
                view_dimension: mx::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        crate::gpu::schema::BindGroupLayoutEntry {
            binding: 4,
            visibility: crate::gpu::schema::ShaderStages::FRAGMENT,
            ty: crate::gpu::schema::BindingType::Texture {
                sample_type: mx::TextureSampleType::Float { filterable: true },
                view_dimension: mx::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        crate::gpu::schema::BindGroupLayoutEntry {
            binding: 5,
            visibility: crate::gpu::schema::ShaderStages::FRAGMENT,
            ty: crate::gpu::schema::BindingType::Texture {
                sample_type: mx::TextureSampleType::Float { filterable: true },
                view_dimension: mx::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        crate::gpu::schema::BindGroupLayoutEntry {
            binding: 6,
            visibility: crate::gpu::schema::ShaderStages::FRAGMENT,
            ty: crate::gpu::schema::BindingType::Texture {
                sample_type: mx::TextureSampleType::Float { filterable: true },
                view_dimension: mx::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        crate::gpu::schema::BindGroupLayoutEntry {
            binding: 7,
            visibility: crate::gpu::schema::ShaderStages::FRAGMENT,
            ty: crate::gpu::schema::BindingType::Texture {
                sample_type: mx::TextureSampleType::Float { filterable: true },
                view_dimension: mx::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        crate::gpu::schema::BindGroupLayoutEntry {
            binding: 8,
            visibility: crate::gpu::schema::ShaderStages::FRAGMENT,
            ty: crate::gpu::schema::BindingType::Texture {
                sample_type: mx::TextureSampleType::Float { filterable: true },
                view_dimension: mx::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        // the PBR set beside the diffuse texture: normal map, occlusion/roughness/metal
        crate::gpu::schema::BindGroupLayoutEntry {
            binding: 9,
            visibility: crate::gpu::schema::ShaderStages::FRAGMENT,
            ty: crate::gpu::schema::BindingType::Texture {
                sample_type: mx::TextureSampleType::Float { filterable: true },
                view_dimension: mx::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        crate::gpu::schema::BindGroupLayoutEntry {
            binding: 10,
            visibility: crate::gpu::schema::ShaderStages::FRAGMENT,
            ty: crate::gpu::schema::BindingType::Texture {
                sample_type: mx::TextureSampleType::Float { filterable: true },
                view_dimension: mx::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        // Tile masks/light maps clamp independently of repeating ground textures.
        crate::gpu::schema::BindGroupLayoutEntry {
            binding: 11,
            visibility: crate::gpu::schema::ShaderStages::FRAGMENT,
            ty: crate::gpu::schema::BindingType::Sampler(
                crate::gpu::schema::SamplerBindingType::Filtering,
            ),
            count: None,
        },
    ]
}

/// Wait until the GPU has done `submission` (None: everything submitted so far).
///
/// On OpenGL wgpu holds the one GL context for the whole of a wait, and every other thread
/// that wants it meanwhile (a worker making a bus's textures) gives up after
/// a second with a panic - "Could not lock adapter context. This is most-likely a deadlock."
/// (wgpu-hal's WGL lock; #843: a slow chip took longer than that for a frame). There the
/// wait is made of short ones, and the context is free between them.
pub fn wait_gpu(
    device: &mx::Device,
    submission: Option<mx::SubmissionIndex>,
) -> Result<(), mx::Error> {
    if !gl_backend() {
        return match submission {
            Some(i) => device.wait(i),
            None => device.wait_idle(),
        };
    }
    let Some(i) = submission else {
        return device.wait_idle();
    };
    loop {
        device.poll()?;
        if device.is_done(i) {
            return Ok(());
        }
        std::thread::sleep(GL_WAIT_SLICE);
    }
}

/// The longest a single wait for the GPU holds the GL context (see [`wait_gpu`]).
pub(crate) const GL_WAIT_SLICE: std::time::Duration = std::time::Duration::from_millis(20);

/// On OpenGL, the GPU work of worker threads (textures and meshes of a bus made while the
/// world loads) goes one thread at a time: a dozen of them queueing for the GL context left
/// the last one waiting past wgpu's one second (#843). Elsewhere the device takes them all.
pub(crate) fn gl_worker_turn() -> Option<std::sync::MutexGuard<'static, ()>> {
    static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());
    gl_backend().then(|| TURN.lock().unwrap_or_else(|e| e.into_inner()))
}

/// A GPU error with what the validation layer said, not just its kind.
impl Renderer {
    /// Why the graphics device was lost, if it was: nothing can be drawn any more.
    /// Lay the sphere maps of `[matl_envmap]` out by this heading instead of the view
    /// (None: by the view, as Omsi.exe does). A sphere map turns with the view it is drawn
    /// for: in the headset every turn of the player's head turned every reflection of the
    /// bus with it, the two eyes each their own way - the reflections swam about.
    pub fn set_env_heading(&self, heading: Option<f32>) {
        self.env_heading.set(heading);
    }

    /// Whether the card ran out of memory since the last call (the game then keeps fewer
    /// textures, before the driver gives up the device).
    /// Send the uploads written so far to the GPU and let go of their staging copies once it
    /// has them (see the wgpu renderer's `flush_uploads`: a loading stage that uploads
    /// without drawing held them all until the first frame).
    pub fn flush_uploads(&self) {
        self.queue.submit(Vec::<mx::CommandBuffer>::new());
        if let Err(e) = self.device.poll() {
            (self.errors.report)(&e);
        }
    }

    pub fn take_out_of_memory(&self) -> bool {
        let texture = crate::resources::TEXTURE_OUT_OF_MEMORY
            .swap(false, std::sync::atomic::Ordering::Relaxed);
        let mesh =
            crate::resources::MESH_OUT_OF_MEMORY.swap(false, std::sync::atomic::Ordering::Relaxed);
        self.errors
            .out_of_memory
            .swap(false, std::sync::atomic::Ordering::Relaxed)
            | texture
            | mesh
    }

    pub fn device_lost(&self) -> Option<String> {
        self.errors
            .device_lost
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

pub(crate) fn gpu_error_text(e: &mx::Error) -> String {
    e.to_string().trim().replace('\n', " ")
}

/// Run `make`, which creates GPU objects, and return why it failed if the device refused one:
/// the builders take mX's `Result`s with `.expect`, and a refusal ends the build here, where the
/// renderer falls back (less multisampling, fewer pipelines) as it did on wgpu's error scopes.
/// Other panics are bugs and go on.
pub(crate) fn gpu_attempt<T>(make: impl FnOnce() -> T) -> Result<T, String> {
    // (caught quietly: the game's panic hook reports no end of the game for it)
    match omsi_cfg::panic_guard::catch_unwind(make) {
        Ok(made) => Ok(made),
        Err(panic) => {
            let text = panic
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| panic.downcast_ref::<&str>().copied())
                .unwrap_or("")
                .to_string();
            if text.starts_with("mX: ") {
                Err(text)
            } else {
                // (not mX refusing an object: a bug, the end of the game as before; the hook
                // only logged it as caught)
                log::error!("the game stopped on an error: {text}");
                std::panic::resume_unwind(panic)
            }
        }
    }
}

/// The error behind a `gpu_attempt` that failed ("mX: what: Error"), for the error report.
pub(crate) fn refused_error(text: String) -> mx::Error {
    // (a call on a removed device panics with the call's own error - DXGI_ERROR_DEVICE_REMOVED,
    // 0x887A0005 - not mX's DeviceLost: taken for an object refused, every frame was dropped
    // and the window stood grey. The backend's own report says whether the device is gone.)
    let lost_code = [
        "0x887A0005",
        "0x887A0006",
        "0x887A0007",
        "DEVICE_REMOVED",
        "DEVICE_HUNG",
        "ERROR_DEVICE_LOST",
    ]
    .iter()
    .any(|c| text.contains(c));
    if let Some(report) = mx::lost_report() {
        mx::Error::DeviceLost(format!("{text}\n{report}"))
    } else if text.contains("DeviceLost(") || lost_code {
        mx::Error::DeviceLost(text)
    } else if text.contains("OutOfMemory(") {
        mx::Error::OutOfMemory(text)
    } else {
        mx::Error::Device(text)
    }
}

/// What the device's error handlers report: errors with multisampling or ray tracing on, out of memory, the device lost.
pub(crate) struct GpuErrors {
    /// Set by the device's error handler when something failed with multisampling on.
    pub(crate) gpu_error: Arc<std::sync::atomic::AtomicBool>,
    /// A GPU error while Enhanced+ traces rays: its ray tracing is left out from the next
    /// frame on (a frame whose commands the device refused shows nothing at all - left to
    /// go on, the picture stood still on the loading screen while the game ran behind it).
    pub(crate) rt_error: Arc<std::sync::atomic::AtomicBool>,
    /// The card ran out of memory since the last `take_out_of_memory`.
    pub(crate) out_of_memory: Arc<std::sync::atomic::AtomicBool>,
    /// Why the graphics device was lost (a driver reset, the card removed), once it was.
    pub(crate) device_lost: Arc<std::sync::Mutex<Option<String>>>,
    /// Where GPU errors go (see `pipelines::errors::Report`).
    pub(crate) report: crate::pipelines::errors::Report,
    pub(crate) log: Arc<std::sync::Mutex<crate::pipelines::errors::ErrorLog>>,
}

/// The GPU errors from one moment on (`Renderer::watch_errors`): what wgpu's error scopes
/// told tests and checks. mX returns most errors where they happen (a `Result`, or a panic
/// for misuse); the rest - a frame mX could not draw, an error a poll found - go to the
/// renderer's error report, which this reads.
pub struct ErrorWatch {
    from: u64,
    log: Arc<std::sync::Mutex<crate::pipelines::errors::ErrorLog>>,
    device: mx::Device,
    report: crate::pipelines::errors::Report,
}

impl ErrorWatch {
    /// The first error reported since the watch began (the device polled first), if any.
    pub fn pop(self) -> Option<String> {
        if let Err(e) = self.device.poll() {
            (self.report)(&e);
        }
        let log = self.log.lock().unwrap_or_else(|p| p.into_inner());
        log.entries
            .iter()
            .find(|(n, _)| *n >= self.from)
            .map(|(_, t)| t.clone())
    }
}

impl Renderer {
    /// Watch for GPU errors from now on (see [`ErrorWatch`]).
    pub fn watch_errors(&self) -> ErrorWatch {
        let from = self
            .errors
            .log
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .next;
        ErrorWatch {
            from,
            log: self.errors.log.clone(),
            device: self.device.clone(),
            report: self.errors.report.clone(),
        }
    }
}
