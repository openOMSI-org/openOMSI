//! The graphics device: what the adapter can do (texture units, storage arrays, GL),
//! the bind group layouts that follow from it, waiting for the GPU, and its errors.

use crate::*;

mod fallbacks;
mod open;
mod timers;

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
pub static ADAPTER_INFO: std::sync::Mutex<Option<wgpu::AdapterInfo>> = std::sync::Mutex::new(None);

pub(crate) static CEILING: std::sync::Mutex<Option<crate::gpu_memory::Ceiling>> = std::sync::Mutex::new(None);

pub fn ceiling() -> Option<crate::gpu_memory::Ceiling> {
    *CEILING.lock().unwrap_or_else(|e| e.into_inner())
}

/// The device runs on OpenGL (set in `Renderer::new`).
pub(crate) static GL_BACKEND: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether the device draws on OpenGL (known once a renderer is made).
pub fn gl_backend() -> bool {
    GL_BACKEND.load(std::sync::atomic::Ordering::Relaxed)
}

/// The adapter is ANGLE's (OpenGL ES over Direct3D 11, Windows): its pipelines are made on
/// a thread of its own (see `angle::compile`).
pub(crate) fn is_angle(info: &wgpu::AdapterInfo) -> bool {
    cfg!(windows) && info.backend == wgpu::Backend::Gl && info.name.contains("ANGLE")
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

pub(crate) static ARRAY_PATH: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

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
    Buffer(wgpu::Buffer),
    Texture { texture: wgpu::Texture, view: wgpu::TextureView, texel: u32 },
    /// Not bound on this device (the lights under `ArrayPath::NoStorage`): takes any write.
    Unused,
}

impl GpuArray {
    /// An array of `size` bytes of `texel`-byte values (16 or 4), a texture where the
    /// vertex shader reads it on a device without vertex storage (`vertex`), or without any.
    pub(crate) fn new(device: &wgpu::Device, label: &str, size: u64, texel: u32, vertex: bool) -> GpuArray {
        let path = array_path();
        if path == ArrayPath::Storage || (!vertex && path == ArrayPath::VertexTextures) {
            return GpuArray::Buffer(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: size.max(16).next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        if !vertex && path == ArrayPath::NoStorage {
            return GpuArray::Unused;
        }
        let texels = size.div_ceil(texel as u64).max(1);
        let max_rows = device.limits().max_texture_dimension_2d;
        let rows = (texels.div_ceil(ARRAY_TEX_WIDTH as u64) as u32).clamp(1, max_rows);
        if rows == max_rows {
            log::warn!("{label}: {texels} values do not fit a texture of {ARRAY_TEX_WIDTH} x {max_rows}; the rest are not drawn");
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: ARRAY_TEX_WIDTH, height: rows, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: if texel == 16 { wgpu::TextureFormat::Rgba32Float } else { wgpu::TextureFormat::R32Uint },
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        GpuArray::Texture { texture, view, texel }
    }

    /// Its room in bytes.
    pub(crate) fn size(&self) -> u64 {
        match self {
            GpuArray::Buffer(b) => b.size(),
            GpuArray::Texture { texture, texel, .. } => texture.width() as u64 * texture.height() as u64 * *texel as u64,
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
    pub(crate) fn write(&self, queue: &wgpu::Queue, offset: u64, data: &[u8]) {
        match self {
            GpuArray::Buffer(b) => queue.write_buffer(b, offset, data),
            GpuArray::Texture { texture, texel, .. } => {
                let t = *texel as usize;
                let total = texture.width() as u64 * texture.height() as u64;
                let mut data = data;
                for (x, y, width, rows) in array_tex_spans(offset / t as u64, (data.len() / t) as u64, total) {
                    let bytes = (width * rows) as usize * t;
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo { texture, mip_level: 0, origin: wgpu::Origin3d { x, y, z: 0 }, aspect: wgpu::TextureAspect::All },
                        &data[..bytes],
                        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width * t as u32), rows_per_image: None },
                        wgpu::Extent3d { width, height: rows, depth_or_array_layers: 1 },
                    );
                    data = &data[bytes..];
                }
            }
            GpuArray::Unused => {}
        }
    }

    pub(crate) fn binding(&self) -> wgpu::BindingResource<'_> {
        match self {
            GpuArray::Buffer(b) => b.as_entire_binding(),
            GpuArray::Texture { view, .. } => wgpu::BindingResource::TextureView(view),
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
        let (width, rows) = if x == 0 && left >= w { (w, left / w) } else { ((w - x).min(left), 1) };
        out.push((x as u32, y as u32, width as u32, rows as u32));
        at += width * rows;
        left -= width * rows;
    }
    out
}

/// The bind group layout entry of a scene array read in `stage` (see `GpuArray`):
/// `float4` for vec4 values, else u32.
pub(crate) fn array_layout_entry(binding: u32, stage: wgpu::ShaderStages, float4: bool) -> wgpu::BindGroupLayoutEntry {
    array_layout_entry_on(array_path(), binding, stage, float4)
}

/// The same on a device whose arrays take `path`.
pub(crate) fn array_layout_entry_on(path: ArrayPath, binding: u32, stage: wgpu::ShaderStages, float4: bool) -> wgpu::BindGroupLayoutEntry {
    let texture = match path {
        ArrayPath::Storage => false,
        ArrayPath::VertexTextures => stage.contains(wgpu::ShaderStages::VERTEX),
        ArrayPath::NoStorage | ArrayPath::LightTextures => true,
    };
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: stage,
        ty: if texture {
            wgpu::BindingType::Texture {
                sample_type: if float4 { wgpu::TextureSampleType::Float { filterable: false } } else { wgpu::TextureSampleType::Uint },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            }
        } else {
            wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }
        },
        count: None,
    }
}

/// The camera group's textures only the enhanced path reads: the reflection probe, the sky
/// table and the sky cube.
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
    array_path() != ArrayPath::Storage && (gl_backend() || omsi_cfg::flags::OMSI_GL_TEXTURE_UNITS.is_set())
}

/// The camera group's entries on a device whose arrays take `path`, without the enhanced
/// path's textures where `omit_enhanced`: on the sixteen-texture-unit path (see
/// `sixteen_texture_units`) and whenever no enhanced pipelines or reflection probe are
/// prepared (the launcher's preview, or the vanilla GL path).
pub(crate) fn camera_layout_entries(path: ArrayPath, omit_enhanced: bool) -> Vec<wgpu::BindGroupLayoutEntry> {
    let mut camera_entries = vec![
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            array_layout_entry_on(path, 1, wgpu::ShaderStages::VERTEX, true),
            array_layout_entry_on(path, 2, wgpu::ShaderStages::VERTEX, true),
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 6,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 7,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 8,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 9,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            array_layout_entry_on(path, 10, wgpu::ShaderStages::VERTEX, false),
            // enhanced: its lighting, the reflection probe, a clamped linear sampler, the sky table
            wgpu::BindGroupLayoutEntry {
                binding: 11,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 12,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::Cube,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 13,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 14,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 17,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::Cube,
                    multisampled: false,
                },
                count: None,
            },
            // the tile light maps around the camera, and where they lie
            wgpu::BindGroupLayoutEntry {
                binding: 18,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 19,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            // the snow track field around the camera (road_snow.wgsl), and its place
            wgpu::BindGroupLayoutEntry {
                binding: 20,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 21,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
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
        camera_entries.push(array_layout_entry_on(path, 3, wgpu::ShaderStages::FRAGMENT, true));
        camera_entries.push(array_layout_entry_on(path, 4, wgpu::ShaderStages::FRAGMENT, false));
    }
    if omit_enhanced {
        camera_entries.retain(|e| !ENHANCED_CAMERA_TEXTURES.contains(&e.binding));
    }
    camera_entries
}

/// The material group's entries: its textures, its sampler and its uniform buffer.
pub(crate) fn material_layout_entries() -> Vec<wgpu::BindGroupLayoutEntry> {
    vec![
        wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 1,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 2,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 3,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 4,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 5,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 6,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 7,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 8,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        // the PBR set beside the diffuse texture: normal map, occlusion/roughness/metal
        wgpu::BindGroupLayoutEntry {
            binding: 9,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 10,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        // Tile masks/light maps clamp independently of repeating ground textures.
        wgpu::BindGroupLayoutEntry {
            binding: 11,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
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
pub fn wait_gpu(device: &wgpu::Device, submission: Option<wgpu::SubmissionIndex>) -> Result<(), wgpu::PollError> {
    if !gl_backend() {
        return device.poll(wgpu::PollType::Wait { submission_index: submission, timeout: None }).map(|_| ());
    }
    loop {
        match device.poll(wgpu::PollType::Wait { submission_index: submission.clone(), timeout: Some(GL_WAIT_SLICE) }) {
            Err(wgpu::PollError::Timeout) => std::thread::yield_now(),
            r => return r.map(|_| ()),
        }
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
    pub fn take_out_of_memory(&self) -> bool {
        let texture = crate::resources::TEXTURE_OUT_OF_MEMORY.swap(false, std::sync::atomic::Ordering::Relaxed);
        let mesh = crate::resources::MESH_OUT_OF_MEMORY.swap(false, std::sync::atomic::Ordering::Relaxed);
        self.errors.out_of_memory.swap(false, std::sync::atomic::Ordering::Relaxed) | texture | mesh
    }

    pub fn device_lost(&self) -> Option<String> {
        self.errors.device_lost.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

pub(crate) fn gpu_error_text(e: &wgpu::Error) -> String {
    match e {
        wgpu::Error::Validation { description, .. } | wgpu::Error::Internal { description, .. } => {
            description.trim().replace('\n', " ")
        }
        other => other.to_string(),
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
}
