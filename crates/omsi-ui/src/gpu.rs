//! The wgpu side: one pipeline for every [`Vertex`](crate::Vertex) list, drawn in layers
//! (each its own camera, viewport and rounded clip) into a window or a texture, MSAA
//! resolved, premultiplied alpha.

use glam::Mat4;
use std::ops::Range;

use crate::atlas::Atlas;
use crate::paint::Vertex;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Packed {
    pos: [f32; 3],
    ext: [u16; 2],
    width: [u16; 2],
    uv: [f32; 2],
    color: [u8; 4],
    mode: [u16; 2],
}

fn half(v: [f32; 2]) -> [u16; 2] {
    v.map(|x| half::f16::from_f32(x.clamp(-65504.0, 65504.0)).to_bits())
}

#[cfg(test)]
const PACK_CHUNK: usize = 16384;

fn pack(v: &Vertex) -> Packed {
    Packed {
        pos: v.pos,
        ext: half(v.ext),
        width: half(v.width),
        uv: v.uv,
        color: v.color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8),
        mode: half(v.mode),
    }
}

/// How one group of draws is seen.
#[derive(Clone, Copy, Debug)]
pub struct Layer {
    pub view_proj: Mat4,
    /// Target pixels the world is drawn into (x, y, w, h); pixel vertices ignore it.
    pub viewport: [f32; 4],
    /// Everything outside this box (x0, y0, x1, y1) with rounded corners fades out.
    pub clip: [f32; 4],
    pub radius: f32,
    pub opacity: f32,
    /// Metres per pixel per metre of depth (`2 tan(fov/2) / viewport height`).
    pub px_scale: f32,
    /// Drawn depth-tested and writing depth (a [`Gpu::with_depth`] only): solid 3D shapes
    /// (the navigator's houses) hide what is behind them whatever order they come in.
    pub depth: bool,
    /// Shadows on the ground (a [`Gpu::with_depth`] only): each pixel darkened once however
    /// many shadow triangles cover it (the stencil counts them).
    pub shadow: bool,
    /// How tall the solid shapes stand (their heights and shadows scaled by it: 1 as built,
    /// 0 flat), so that a view tilting into 3D raises its houses without building them again.
    pub height: f32,
}

impl Layer {
    /// Pixels only, clipped to `clip`.
    pub fn flat(clip: [f32; 4], radius: f32, opacity: f32) -> Layer {
        Layer { view_proj: Mat4::IDENTITY, viewport: [0.0; 4], clip, radius, opacity, px_scale: 0.0, depth: false, shadow: false, height: 1.0 }
    }
    /// A perspective view of a world drawn into `viewport`.
    pub fn world(view: Mat4, fov_y: f32, viewport: [f32; 4], clip: [f32; 4], radius: f32, opacity: f32) -> Layer {
        let aspect = viewport[2] / viewport[3].max(1.0);
        let proj = Mat4::perspective_rh(fov_y, aspect, 1.0, 20000.0);
        Layer { view_proj: proj * view, viewport, clip, radius, opacity, px_scale: 2.0 * (fov_y * 0.5).tan() / viewport[3].max(1.0), depth: false, shadow: false, height: 1.0 }
    }
    /// The same layer drawn depth-tested (see `depth`).
    pub fn with_depth(mut self) -> Layer {
        self.depth = true;
        self
    }
    /// The same layer drawing shadows (see `shadow`).
    pub fn with_shadow(mut self) -> Layer {
        self.shadow = true;
        self.depth = false;
        self
    }
    /// The same layer with its solid shapes `k` times as tall (see `height`).
    pub fn with_height(mut self, k: f32) -> Layer {
        self.height = k;
        self
    }
}

/// Vertices `range` of buffer `buffer`, seen as layer `layer`, with texture `texture`
/// (0 = the atlas).
#[derive(Clone, Debug)]
pub struct Draw {
    pub buffer: usize,
    pub range: Range<u32>,
    pub layer: usize,
    pub texture: usize,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniform {
    view_proj: [[f32; 4]; 4],
    viewport: [f32; 4],
    target: [f32; 4],
    clip: [f32; 4],
    params: [f32; 4],
}

const SLOT: u64 = 256;
const DEPTH_FORMAT: mx::TextureFormat = mx::TextureFormat::Depth24PlusStencil8;
const MAX_LAYERS: u64 = 256;

struct Tex {
    texture: Option<mx::Texture>,
    group: mx::BindGroup,
    size: (u32, u32),
}

pub struct Vertices(Option<mx::Buffer>, u64);

/// For a second target a `Gpu` draws into beside its main one: kept by whoever draws it, so
/// that neither replaces the other's at every switch of size, and freed with its holder.
#[derive(Default)]
pub struct Attachments {
    msaa: Option<(mx::TextureView, u32, u32)>,
    depth: Option<(mx::TextureView, u32, u32)>,
}

pub struct Gpu {
    pipeline: mx::RenderPipeline,
    /// With a depth buffer: the pipeline of the depth-tested layers, and the buffer.
    pipeline_depth: Option<mx::RenderPipeline>,
    pipeline_shadow: Option<mx::RenderPipeline>,
    depth: Option<(mx::TextureView, u32, u32)>,
    uniform: mx::Buffer,
    /// One bind group per layer, each on its slot of `uniform` (made when first drawn).
    uniform_groups: Vec<mx::BindGroup>,
    sampler: mx::Sampler,
    /// The vertex buffers and their capacity (None: making it failed; tried again next upload).
    buffers: Vec<(Option<mx::Buffer>, u64)>,
    packed: Vec<Packed>,
    textures: Vec<Option<Tex>>,
    msaa: Option<(mx::TextureView, u32, u32)>,
    failed_target: Option<(u32, u32)>,
    samples: u32,
    format: mx::TextureFormat,
    atlas_size: u32,
    /// (to make the atlas texture anew when the atlas grows)
    device: mx::Device,
}

impl Gpu {
    /// A pipeline drawing into `format` with `samples` MSAA samples, and an atlas texture of
    /// `atlas_size` pixels square (texture 0).
    pub fn new(device: &mx::Device,
        format: mx::TextureFormat, samples: u32, atlas_size: u32) -> Gpu {
        Self::make(device, format, samples, atlas_size, false)
    }

    /// The same with a depth buffer: layers marked `depth` are depth-tested (see [`Layer::depth`]).
    pub fn with_depth(device: &mx::Device,
        format: mx::TextureFormat, samples: u32, atlas_size: u32) -> Gpu {
        Self::make(device, format, samples, atlas_size, true)
    }

    fn make(device: &mx::Device,
        format: mx::TextureFormat, samples: u32, atlas_size: u32, with_depth: bool) -> Gpu {
        let shader = device
            .create_shader_module(&mx::ShaderModuleDescriptor {
                label: Some("omsi-ui"),
                source: &*(include_str!("ui.wgsl")),
            })
            .expect("mX: a shader");
        let attrs = mx::vertex_attr_array![0 => Float32x3, 1 => Float16x2, 2 => Float16x2, 3 => Float32x2, 4 => Unorm8x4, 5 => Float16x2];
        let make_pipeline = |depth: Option<mx::DepthStencilState>| {
            device
                .create_render_pipeline(&mx::RenderPipelineDescriptor {
                    label: Some("omsi-ui"),
                    vertex: mx::VertexState {
                        constants: &[],
                        module: &shader,
                        entry_point: Some("vs_main"),
                        buffers: &[mx::VertexBufferLayout {
                            array_stride: std::mem::size_of::<Packed>() as u64,
                            step_mode: mx::VertexStepMode::Vertex,
                            attributes: &attrs,
                        }],
                    },
                    primitive: mx::PrimitiveState {
                        topology: mx::PrimitiveTopology::TriangleList,
                        cull_mode: None,
                        ..Default::default()
                    },
                    depth_stencil: depth,
                    multisample: mx::MultisampleState {
                        count: samples,
                        mask: !0,
                        alpha_to_coverage_enabled: false,
                    },
                    fragment: Some(mx::FragmentState {
                        constants: &[],
                        module: &shader,
                        entry_point: Some("fs_main"),
                        targets: &[Some(mx::ColorTargetState {
                            format,
                            blend: Some(mx::BlendState {
                                color: mx::BlendComponent {
                                    src_factor: mx::BlendFactor::One,
                                    dst_factor: mx::BlendFactor::OneMinusSrcAlpha,
                                    operation: mx::BlendOperation::Add,
                                },
                                alpha: mx::BlendComponent {
                                    src_factor: mx::BlendFactor::One,
                                    dst_factor: mx::BlendFactor::OneMinusSrcAlpha,
                                    operation: mx::BlendOperation::Add,
                                },
                            }),
                            write_mask: mx::ColorWrites::ALL,
                        })],
                    }),
                })
                .expect("mX: a render pipeline")
        };
        let depth_state = |test: bool, stencil: mx::StencilState| mx::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: test,
            depth_compare: if test {
                mx::CompareFunction::Less
            } else {
                mx::CompareFunction::Always
            },
            stencil,
            bias: Default::default(),
        };
        // (a shadow pixel is drawn where the stencil is still 0, and counts it up)
        let once = mx::StencilFaceState {
            compare: mx::CompareFunction::Equal,
            fail_op: mx::StencilOperation::Keep,
            depth_fail_op: mx::StencilOperation::Keep,
            pass_op: mx::StencilOperation::IncrementClamp,
        };
        let shadow_stencil = mx::StencilState { front: once, back: once, read_mask: 0xff, write_mask: 0xff };
        let pipeline = make_pipeline(with_depth.then(|| depth_state(false, Default::default())));
        let pipeline_depth = with_depth.then(|| make_pipeline(Some(depth_state(true, Default::default()))));
        let pipeline_shadow = with_depth.then(|| make_pipeline(Some(depth_state(false, shadow_stencil))));
        let uniform = device
            .create_buffer(&mx::BufferDescriptor {
                label: Some("omsi-ui uniform"),
                size: SLOT * MAX_LAYERS,
                usage: mx::BufferUsages::UNIFORM | mx::BufferUsages::COPY_DST,
            })
            .expect("mX: a buffer");
        let sampler = device
            .create_sampler(&mx::SamplerDescriptor {
                label: Some("omsi-ui"),
                address_mode_u: mx::AddressMode::ClampToEdge,
                address_mode_v: mx::AddressMode::ClampToEdge,
                mag_filter: mx::FilterMode::Linear,
                min_filter: mx::FilterMode::Linear,
                mipmap_filter: mx::MipmapFilterMode::Linear,
                ..Default::default()
            })
            .expect("mX: a sampler");
        let mut g = Gpu { pipeline, pipeline_depth, pipeline_shadow, depth: None, uniform,
            uniform_groups: Vec::new(), sampler, buffers: Vec::new(), packed: Vec::new(), textures: Vec::new(), msaa: None, failed_target: None, samples, format, atlas_size: atlas_size.max(1), device: device.clone() };
        let atlas = vec![0u8; (atlas_size * atlas_size * 4) as usize];
        g.add_texture_internal(device, None, atlas_size, atlas_size, &atlas);
        g
    }

    fn add_texture_internal(&mut self, device: &mx::Device,
        queue: Option<&mx::Queue>, w: u32, h: u32, rgba: &[u8]) -> usize {
        let texture = device
            .create_texture(&mx::TextureDescriptor {
                label: Some("omsi-ui image"),
                size: mx::Extent3d {
                    width: w.max(1),
                    height: h.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: mx::TextureDimension::D2,
                format: mx::TextureFormat::Rgba8UnormSrgb,
                usage: mx::TextureUsages::TEXTURE_BINDING | mx::TextureUsages::COPY_DST,
                view_formats: &[],
            })
            .expect("mX: a texture");
        if let Some(q) = queue {
            Self::write(q, &texture, w, [0, 0, w, h], rgba);
        }
        let view = texture
            .create_view(&Default::default())
            .expect("mX: a texture view");
        let group = self.group(device, &view);
        self.textures.push(Some(Tex { texture: Some(texture), group, size: (w, h) }));
        self.textures.len() - 1
    }

    fn group(&self, device: &mx::Device, view: &mx::TextureView) -> mx::BindGroup {
        device.create_bind_group(&mx::BindGroupDescriptor {
            label: Some("omsi-ui texture"),
            entries: &[
                mx::BindGroupEntry {
                    binding: 0,
                    resource: mx::BindingResource::TextureView(view),
                },
                mx::BindGroupEntry {
                    binding: 1,
                    resource: mx::BindingResource::Sampler(&self.sampler), }],
        })
    }

    fn write(queue: &mx::Queue, t: &mx::Texture, stride_w: u32, r: [u32; 4], rgba: &[u8]) {
        if r[2] == 0 || r[3] == 0 {
            return;
        }
        let offset = ((r[1] * stride_w + r[0]) * 4) as usize;
        queue
            .write_texture(
                mx::TexelCopyTextureInfo {
                    texture: t,
                    mip_level: 0,
                    origin: mx::Origin3d {
                        x: r[0],
                        y: r[1],
                        z: 0,
                    },
                    aspect: mx::TextureAspect::All,
                },
                &rgba[offset..],
                mx::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride_w * 4),
                    rows_per_image: None,
                },
                mx::Extent3d {
                    width: r[2],
                    height: r[3],
                    depth_or_array_layers: 1,
                },
            )
            .expect("mX: a queue write");
    }

    /// An RGBA picture (straight alpha, sRGB) to draw with; returns its texture index.
    pub fn add_image(&mut self, device: &mx::Device,
        queue: &mx::Queue, w: u32, h: u32, rgba: &[u8]) -> usize {
        self.add_texture_internal(device, Some(queue), w, h, rgba)
    }

    /// Replace the pixels of a picture added by `add_image` (same size; else it is made
    /// anew under the same index).
    pub fn update_image(&mut self, device: &mx::Device,
        queue: &mx::Queue, id: usize, w: u32, h: u32, rgba: &[u8]) {
        let same = matches!(self.textures.get(id), Some(Some(t)) if t.size == (w, h) && t.texture.is_some());
        if same {
            let t = self.textures[id].as_ref().unwrap().texture.as_ref().unwrap();
            Self::write(queue, t, w, [0, 0, w, h], rgba);
            return;
        }
        let k = self.add_texture_internal(device, Some(queue), w, h, rgba);
        let t = self.textures.pop().flatten();
        debug_assert_eq!(k, self.textures.len());
        if id < self.textures.len() {
            self.textures[id] = t;
        } else {
            self.textures.push(t);
        }
    }

    /// A texture drawn elsewhere (a render target) to draw with; returns its index.
    pub fn add_view(&mut self, device: &mx::Device,
        view: &mx::TextureView, size: (u32, u32)) -> usize {
        let group = self.group(device, view);
        self.textures.push(Some(Tex { texture: None, group, size }));
        self.textures.len() - 1
    }

    /// Point `id` at another view (a render target made anew at another size).
    pub fn set_view(&mut self, device: &mx::Device,
        id: usize,
        view: &mx::TextureView, size: (u32, u32)) {
        let group = self.group(device, view);
        if let Some(slot) = self.textures.get_mut(id) {
            *slot = Some(Tex { texture: None, group, size });
        }
    }

    pub fn free(&mut self, id: usize) {
        if id > 0 {
            if let Some(t) = self.textures.get_mut(id) {
                *t = None;
            }
        }
    }

    /// Send the atlas's changed region to the GPU.
    pub fn upload_atlas(&mut self, queue: &mx::Queue, atlas: &mut Atlas) {
        if atlas.size != self.atlas_size {
            // the atlas grew (see `Atlas::begin_frame`): its texture is made anew at its size
            let device = self.device.clone();
            let size = atlas.size;
            let texture = device
                .create_texture(&mx::TextureDescriptor {
                    label: Some("omsi-ui atlas"),
                    size: mx::Extent3d {
                        width: size,
                        height: size,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: mx::TextureDimension::D2,
                    format: mx::TextureFormat::Rgba8UnormSrgb,
                    usage: mx::TextureUsages::TEXTURE_BINDING | mx::TextureUsages::COPY_DST,
                    view_formats: &[],
                })
                .expect("mX: a texture");
            let view = texture
                .create_view(&Default::default())
                .expect("mX: a texture view");
            let group = self.group(&device, &view);
            if let Some(slot) = self.textures.first_mut() {
                *slot = Some(Tex { texture: Some(texture), group, size: (size, size) });
            }
            self.atlas_size = size;
            atlas.mark_all_dirty();
        }
        if let Some(r) = atlas.take_dirty() {
            if let Some(Some(Tex { texture: Some(t), .. })) = self.textures.first() {
                Self::write(queue, t, atlas.size, r, &atlas.rgba);
            }
        }
    }

    /// Put `verts` into vertex buffer `id` (grown as needed).
    pub fn upload(&mut self, device: &mx::Device, queue: &mx::Queue, id: usize, verts: &[Vertex]) {
        self.packed.clear();
        self.packed.extend(verts.iter().map(pack));
        let bytes = std::mem::size_of_val(self.packed.as_slice()) as u64;
        while self.buffers.len() <= id {
            self.buffers.push((None, 0));
        }
        if bytes > self.buffers[id].1 || self.buffers[id].0.is_none() {
            let cap = bytes.next_power_of_two().max(4096);
            self.buffers[id] = (Self::vertex_buffer(device, cap), cap);
        }
        if let (Some(buf), true) = (&self.buffers[id].0, bytes > 0) {
            queue
                .write_buffer(buf, 0, bytemuck::cast_slice(&self.packed))
                .expect("mX: a queue write");
        }
    }

    pub fn vertices(device: &mx::Device, verts: &[Vertex]) -> Vertices {
        let bytes = (verts.len() * std::mem::size_of::<Packed>()) as u64;
        if bytes == 0 {
            return Vertices(None, 0);
        }
        let packed: Vec<Packed> = verts.iter().map(pack).collect();
        let usage = mx::BufferUsages::VERTEX | mx::BufferUsages::COPY_DST;
        let buf = match device.create_buffer_init(&mx::BufferInitDescriptor {
            label: Some("omsi-ui vertices"),
            contents: bytemuck::cast_slice(&packed),
            usage,
        }) {
            Ok(b) => Some(b),
            Err(e) => {
                log::warn!("omsi-ui: a vertex buffer of {bytes} bytes could not be made: {e}");
                None
            }
        };
        Vertices(buf, bytes)
    }

    pub fn drop_vertices(&mut self, id: usize) {
        if let Some(b) = self.buffers.get_mut(id) {
            *b = (None, 0);
        }
    }

    pub fn drop_targets(&mut self) {
        self.msaa = None;
        self.depth = None;
    }

    pub fn put(&mut self, id: usize, v: Vertices) {
        while self.buffers.len() <= id {
            self.buffers.push((None, 0));
        }
        self.buffers[id] = (v.0, v.1);
    }

    /// A vertex buffer of `size` bytes, made where its failure can be seen: a card out of
    /// memory hands out an invalid buffer, and writing to that every frame flooded the log
    /// with thousands of errors and took the frame rate down with it (#217). A failed one
    /// is None: nothing draws from it, and the next upload tries again.
    fn vertex_buffer(device: &mx::Device, size: u64) -> Option<mx::Buffer> {
        match device.create_buffer(&mx::BufferDescriptor {
            label: Some("omsi-ui vertices"),
            size,
            usage: mx::BufferUsages::VERTEX | mx::BufferUsages::COPY_DST,
        }) {
            Ok(buf) => Some(buf),
            Err(e) => {
                log::warn!("omsi-ui: a vertex buffer of {size} bytes could not be made: {e}");
                None
            }
        }
    }

    fn attachment(&mut self, device: &mx::Device,
        label: &str,
        (w, h): (u32, u32),
        format: mx::TextureFormat,
    ) -> Option<mx::TextureView> {
        let made = device.create_texture(&mx::TextureDescriptor {
            label: Some(label),
            size: mx::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: self.samples,
            dimension: mx::TextureDimension::D2,
            format,
            usage: mx::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        match made.and_then(|t| t.create_view(&Default::default())) {
            Ok(view) => {
                self.failed_target = None;
                Some(view)
            }
            Err(e) => {
                if self.failed_target.replace((w, h)) != Some((w, h)) {
                    log::warn!("omsi-ui: {label} ({w}x{h}) could not be made: {e}");
                }
                None
            }
        }
    }

    /// [`Self::render`] through `attachments` instead of the `Gpu`'s own.
    #[allow(clippy::too_many_arguments)]
    pub fn render_with(&mut self, attachments: &mut Attachments, device: &mx::Device,
        queue: &mx::Queue,
        encoder: &mut mx::CommandEncoder,
        target: &mx::TextureView,
        size: (u32, u32),
        clear: Option<mx::Color>, layers: &[Layer], draws: &[Draw]) {
        std::mem::swap(&mut self.msaa, &mut attachments.msaa);
        std::mem::swap(&mut self.depth, &mut attachments.depth);
        self.render(device, queue, encoder, target, size, clear, layers, draws);
        std::mem::swap(&mut self.msaa, &mut attachments.msaa);
        std::mem::swap(&mut self.depth, &mut attachments.depth);
    }

    /// Draw into `target` (`size` pixels): cleared to `clear` first when given.
    #[allow(clippy::too_many_arguments)]
    pub fn render(&mut self, device: &mx::Device,
        queue: &mx::Queue,
        encoder: &mut mx::CommandEncoder,
        target: &mx::TextureView,
        size: (u32, u32),
        clear: Option<mx::Color>, layers: &[Layer], draws: &[Draw]) {
        let (w, h) = (size.0.max(1), size.1.max(1));
        // the layers' uniforms
        let mut data = vec![0u8; (SLOT * MAX_LAYERS) as usize];
        for (k, l) in layers.iter().take(MAX_LAYERS as usize).enumerate() {
            let vp = if l.viewport[2] > 0.0 { l.viewport } else { [0.0, 0.0, w as f32, h as f32] };
            let u = Uniform { view_proj: l.view_proj.to_cols_array_2d(), viewport: vp, target: [w as f32, h as f32, l.height, 0.0], clip: l.clip, params: [l.radius, l.opacity, l.px_scale, 0.0] };
            let at = k * SLOT as usize;
            data[at..at + std::mem::size_of::<Uniform>()].copy_from_slice(bytemuck::bytes_of(&u));
        }
        queue
            .write_buffer(
                &self.uniform,
                0,
                &data[..(SLOT as usize * layers.len().clamp(1, MAX_LAYERS as usize))],
            )
            .expect("mX: a queue write");
        while self.uniform_groups.len() < layers.len().min(MAX_LAYERS as usize) {
            let at = self.uniform_groups.len() as u64 * SLOT;
            self.uniform_groups
                .push(device.create_bind_group(&mx::BindGroupDescriptor {
                    label: Some("omsi-ui uniform"),
                    entries: &[mx::BindGroupEntry {
                        binding: 0,
                        resource: mx::BindingResource::Buffer(mx::BufferBinding {
                            buffer: &self.uniform,
                            offset: at,
                            size: Some(std::mem::size_of::<Uniform>() as u64),
                        }),
                    }],
                }));
        }
        if self.samples > 1 && self.msaa.as_ref().map(|m| (m.1, m.2) != (w, h)).unwrap_or(true) {
            self.msaa = None;
            let Some(view) = self.attachment(device, "omsi-ui msaa", (w, h), self.format) else { return };
            self.msaa = Some((view, w, h));
        }
        if self.pipeline_depth.is_some() && self.depth.as_ref().map(|d| (d.1, d.2) != (w, h)).unwrap_or(true) {
            self.depth = None;
            let Some(view) = self.attachment(device, "omsi-ui depth", (w, h), DEPTH_FORMAT) else { return };
            self.depth = Some((view, w, h));
        }
        let (view, resolve) = match (&self.msaa, self.samples > 1) {
            (Some(m), true) => (&m.0, Some(target)),
            _ => (target, None),
        };
        let depth_view = self.pipeline_depth.as_ref().and(self.depth.as_ref()).map(|d| &d.0);
        let mut pass = encoder.begin_render_pass(&mx::RenderPassDescriptor {
            label: Some("omsi-ui"),
            color_attachments: &[Some(mx::RenderPassColorAttachment {
                view,
                resolve_target: resolve,
                ops: mx::Operations {
                    load: match (clear, resolve.is_some()) {
                        (Some(c), _) => mx::LoadOp::Clear(c),
                        (None, true) => mx::LoadOp::Clear(mx::Color::TRANSPARENT),
                        (None, false) => mx::LoadOp::Load,
                    },
                    store: if resolve.is_some() {
                        mx::StoreOp::Discard
                    } else {
                        mx::StoreOp::Store } },
            })],
            depth_stencil_attachment: depth_view.map(|view| mx::RenderPassDepthStencilAttachment {
                view,
                depth_ops: Some(mx::Operations {
                    load: mx::LoadOp::Clear(1.0),
                    store: mx::StoreOp::Discard,
                }),
                stencil_ops: Some(mx::Operations {
                    load: mx::LoadOp::Clear(0),
                    store: mx::StoreOp::Discard,
                }),
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        let mut depth_on: Option<u8> = None;
        let mut state: Option<(usize, usize, usize)> = None;
        for d in draws {
            let (Some((Some(buf), _)), Some(l)) = (self.buffers.get(d.buffer), layers.get(d.layer)) else { continue };
            // (a texture that is not there - one freed, or a number kept from another device -
            // draws nothing: taken for the atlas, a picture came out as the interface's words)
            let Some(Some(tex)) = self.textures.get(d.texture) else { continue };
            if d.range.is_empty() || d.layer as usize >= self.uniform_groups.len() {
                continue;
            }
            let want = match (&self.pipeline_depth, &self.pipeline_shadow) {
                (Some(_), Some(_)) if l.shadow => 2u8,
                (Some(_), _) if l.depth => 1,
                _ => 0,
            };
            if depth_on != Some(want) {
                pass.set_pipeline(match want {
                    2 => self.pipeline_shadow.as_ref().unwrap_or(&self.pipeline),
                    1 => self.pipeline_depth.as_ref().unwrap_or(&self.pipeline),
                    _ => &self.pipeline,
                });
                depth_on = Some(want);
            }
            if state == Some((d.layer, d.texture, d.buffer)) {
                pass.draw(d.range.clone(), 0..1);
                continue;
            }
            state = None;
            pass.set_bind_group(0, &self.uniform_groups[d.layer as usize]);
            pass.set_bind_group(1, &tex.group);
            // A flat layer's `viewport` is the size its pixels are laid out in (an interface in
            // logical pixels): drawn over the whole target, scaled to it. As the rasteriser's
            // viewport it squeezed an interface zoomed by 1.1 into the top-left 90 % of the
            // window while the clips stayed full size (cut-off text, a black band below).
            let flat = l.px_scale == 0.0 && l.view_proj == Mat4::IDENTITY;
            let vp = if l.viewport[2] > 0.0 && !flat { l.viewport } else { [0.0, 0.0, w as f32, h as f32] };
            let vx = vp[0].clamp(0.0, w as f32 - 1.0);
            let vy = vp[1].clamp(0.0, h as f32 - 1.0);
            pass.set_viewport(vx, vy, vp[2].min(w as f32 - vx).max(1.0), vp[3].min(h as f32 - vy).max(1.0), 0.0, 1.0);
            let x0 = l.clip[0].floor().clamp(0.0, w as f32) as u32;
            let y0 = l.clip[1].floor().clamp(0.0, h as f32) as u32;
            let x1 = l.clip[2].ceil().clamp(0.0, w as f32) as u32;
            let y1 = l.clip[3].ceil().clamp(0.0, h as f32) as u32;
            if x1 <= x0 || y1 <= y0 {
                continue;
            }
            pass.set_scissor_rect(x0, y0, x1 - x0, y1 - y0);
            pass.set_vertex_buffer(0, &buf, 0);
            state = Some((d.layer, d.texture, d.buffer));
            pass.draw(d.range.clone(), 0..1);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn packed_vertices_keep_their_values_in_36_bytes() {
        let v = crate::Vertex { pos: [1234.5, -987.25, 3.5], ext: [-0.7071, 1.0], width: [3.4, 2.5], uv: [0.123456, 0.987654], color: [0.2, 0.5, 1.0, 0.02], mode: [3.0, 1.0] };
        let p = super::pack(&v);
        assert_eq!(std::mem::size_of::<super::Packed>(), 36);
        use std::mem::offset_of;
        use super::Packed;
        assert_eq!([offset_of!(Packed, ext), offset_of!(Packed, width), offset_of!(Packed, uv), offset_of!(Packed, color), offset_of!(Packed, mode)], [12, 16, 20, 28, 32]);
        let f = |b: [u16; 2]| b.map(|x| half::f16::from_bits(x).to_f32());
        assert_eq!(p.pos, v.pos);
        assert_eq!(p.uv, v.uv);
        assert_eq!(f(p.mode), v.mode);
        for (a, b) in f(p.ext).iter().chain(&f(p.width)).zip(v.ext.iter().chain(&v.width)) {
            assert!((a - b).abs() <= b.abs() * 1e-3, "{a} {b}");
        }
        assert_eq!(p.color, [51, 128, 255, 5]);
    }

    #[test]
    fn vertices_longer_than_a_chunk_fill_their_buffer() {
        let instance = mx::Instance::new();
        let Ok(adapter) = instance.default_adapter() else {
            eprintln!("no graphics adapter: skipped");
            return;
        };
        let (device, _queue) = adapter
            .request_device(&mx::DeviceDescriptor::default())
            .unwrap();
        let verts = vec![crate::Vertex::default(); super::PACK_CHUNK * 2 + 7];
        let v = super::Gpu::vertices(&device, &verts);
        assert!(v.0.is_some());
        assert_eq!(v.1, (verts.len() * std::mem::size_of::<super::Packed>()) as u64);
    }

    #[test]
    fn shader_validates() {
        let m = naga::front::wgsl::parse_str(include_str!("ui.wgsl")).expect("parse");
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all()).validate(&m).expect("valid");
    }

    #[test]
    fn a_target_that_cannot_be_made_is_skipped_and_made_again_later() {
        let instance = mx::Instance::new();
        let Ok(adapter) = instance.default_adapter() else {
            eprintln!("no graphics adapter: skipped");
            return;
        };
        let (device, queue) = adapter
            .request_device(&mx::DeviceDescriptor::default())
            .unwrap();
        let format = mx::TextureFormat::Rgba8UnormSrgb;
        let mut gpu = super::Gpu::new(&device, format, 4, 256);
        let target = device
            .create_texture(&mx::TextureDescriptor {
                label: None,
                size: mx::Extent3d {
                    width: 64,
                    height: 64,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: mx::TextureDimension::D2,
                format,
                usage: mx::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .expect("mX: a texture");
        let view = target
            .create_view(&Default::default())
            .expect("mX: a texture view");
        let mut encoder = device.create_command_encoder(None);
        let huge = device.limits().max_texture_dimension_2d + 1;
        gpu.render(&device, &queue, &mut encoder, &view, (huge, 64), None, &[], &[]);
        assert!(gpu.msaa.is_none());
        gpu.render(&device, &queue, &mut encoder, &view, (64, 64), None, &[], &[]);
        assert!(gpu.msaa.is_some());
        let done = queue.submit([encoder.finish()]);
        device.wait(done).expect("the frames ran");
    }

    #[test]
    fn a_second_target_keeps_its_attachments_apart_from_the_gpus_own() {
        let instance = mx::Instance::new();
        let Ok(adapter) = instance.default_adapter() else {
            eprintln!("no graphics adapter: skipped");
            return;
        };
        let (device, queue) = adapter
            .request_device(&mx::DeviceDescriptor::default())
            .unwrap();
        let format = mx::TextureFormat::Rgba8UnormSrgb;
        let mut gpu = super::Gpu::with_depth(&device, format, 4, 256);
        let target = |side: u32| {
            device.create_texture(&mx::TextureDescriptor {
                    label: None,
                    size: mx::Extent3d { width: side, height: side, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: mx::TextureDimension::D2,
                    format,
                    usage: mx::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                })
                .expect("mX: a texture")
                .create_view(&Default::default())
                .expect("mX: a texture view")
        };
        let (panel, screen) = (target(64), target(32));
        let mut screens = super::Attachments::default();
        let mut encoder = device.create_command_encoder(None);
        gpu.render(&device, &queue, &mut encoder, &panel, (64, 64), None, &[], &[]);
        gpu.render_with(&mut screens, &device, &queue, &mut encoder, &screen, (32, 32), None, &[], &[]);
        assert_eq!(gpu.msaa.as_ref().map(|m| (m.1, m.2)), Some((64, 64)), "the panel's stay");
        assert_eq!(gpu.depth.as_ref().map(|d| (d.1, d.2)), Some((64, 64)));
        assert_eq!(screens.msaa.as_ref().map(|m| (m.1, m.2)), Some((32, 32)), "the screen's are its holder's");
        assert_eq!(screens.depth.as_ref().map(|d| (d.1, d.2)), Some((32, 32)));
        queue.submit([encoder.finish()]);
    }
}
