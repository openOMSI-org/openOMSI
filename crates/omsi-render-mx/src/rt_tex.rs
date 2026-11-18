//! Enhanced+: what a reflected ray sees of the textures where it hits (rt.rs, rt_reflect.wgsl).
//!
//! The rays carry no texture bindings of their own (there are thousands of textures, and no
//! bindless arrays on every device), so each texture a traced geometry names gets a small
//! copy - a tile of 30 x 30 texels inside a texel of border that repeats the opposite edge, so
//! that filtering wraps - in one atlas, made with its mean colour (rt_avg.wgsl); and each
//! traced mesh gets its triangles' texture coordinates in one buffer, copied once from its
//! page's vertices and indices when it is first traced (rt_uv.wgsl). A hit's texel is then its
//! triangle's three coordinates mixed by the hit's barycentrics, addressed as its material
//! addresses them, read from the texture's tile. Until a texture's tile or a mesh's
//! coordinates are there, a hit shows the texture's mean colour, as all hits did before.

use super::*;
use crate::rt::AVG_BUDGET;

/// A tile's size in the atlas (texels, its border included).
const TILE: u32 = 32;
const ATLAS_W: u32 = 4096;
/// Tiles per row of the atlas; texture `t` has tile `t` (a texture past the atlas's
/// 128 x 128 tiles keeps its mean colour).
const PER_ROW: u32 = ATLAS_W / TILE;
const MAX_TILES: usize = (PER_ROW * PER_ROW) as usize;
/// Meshes whose coordinates are copied per frame at most (as many as structures are built).
pub(super) const UV_BUDGET: usize = 320;
/// Bytes per triangle in the coordinates' buffer (rt_uv.wgsl).
const UV_TRI: u64 = 16;
/// A record without coordinates.
pub(super) const NO_UV: u32 = u32::MAX;

/// A mesh's coordinates in the buffer (in triangles).
struct MeshUv {
    generation: u64,
    at: u64,
    tris: u64,
    used: u64,
}

/// One mesh's copy this frame.
struct UvJob {
    vb: mx::Buffer,
    ib: mx::Buffer,
    /// rt_uv.wgsl `Job`: first vertex, first index, triangles, where they go.
    job: [u32; 4],
}

pub(super) struct TexStore {
    atlas: mx::Texture,
    /// The atlas as the reflections sample it (sRGB, filtered in linear light) and as the
    /// texture means write it (the same bytes, encoded there).
    pub(super) atlas_view: mx::TextureView,
    atlas_store: mx::TextureView,
    pub(super) uvs: mx::Buffer,
    /// The buffer the coordinates were in before it last grew: copied over at the next encode.
    uvs_old: Option<mx::Buffer>,
    uv_cap: u64,
    uv_top: u64,
    uv_free: Vec<(u64, u64)>,
    meshes: hashbrown::HashMap<MeshId, MeshUv>,
    jobs: Vec<UvJob>,
    /// Uniforms read with dynamic offsets: the copies' jobs and the texture means' tiles.
    job_buf: mx::Buffer,
    tile_buf: mx::Buffer,
    stride: u64,
    uv_pipeline: mx::ComputePipeline,
}

fn make_atlas(device: &mx::Device, h: u32) -> (mx::Texture, mx::TextureView, mx::TextureView) {
    let t = device
        .create_texture(&mx::TextureDescriptor {
            label: Some("rt texture atlas"),
            size: mx::Extent3d {
                width: ATLAS_W,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: mx::TextureDimension::D2,
            format: mx::TextureFormat::Rgba8Unorm,
            usage: mx::TextureUsages::STORAGE_BINDING
                | mx::TextureUsages::TEXTURE_BINDING
                | mx::TextureUsages::COPY_SRC
                | mx::TextureUsages::COPY_DST,
            view_formats: &[mx::TextureFormat::Rgba8UnormSrgb],
        })
        .expect("mX: a texture");
    // (sampled only: an sRGB view cannot be a storage binding)
    let sampled = t
        .create_view(&mx::TextureViewDescriptor {
            format: Some(mx::TextureFormat::Rgba8UnormSrgb),
            ..Default::default()
        })
        .expect("mX: a texture view");
    let store = t
        .create_view(&Default::default())
        .expect("mX: a texture view");
    (t, sampled, store)
}

fn make_uvs(device: &mx::Device, tris: u64) -> mx::Buffer {
    device
        .create_buffer(&mx::BufferDescriptor {
            label: Some("rt texture coordinates"),
            size: tris * UV_TRI,
            usage: mx::BufferUsages::STORAGE
                | mx::BufferUsages::COPY_DST
                | mx::BufferUsages::COPY_SRC,
        })
        .expect("mX: a buffer")
}

impl TexStore {
    pub(super) fn new(device: &mx::Device) -> TexStore {
        let (atlas, atlas_view, atlas_store) = make_atlas(device, 256);
        let stride = (device.limits().min_uniform_buffer_offset_alignment as u64).max(256);
        let uniforms = |label: &str, n: usize| {
            device
                .create_buffer(&mx::BufferDescriptor {
                    label: Some(label),
                    size: n as u64 * stride,
                    usage: mx::BufferUsages::UNIFORM | mx::BufferUsages::COPY_DST,
                })
                .expect("mX: a buffer")
        };
        let module = device
            .create_shader_module(&mx::ShaderModuleDescriptor {
                label: Some("rt texture coordinates"),
                source: &*(include_str!("rt_uv.wgsl")),
            })
            .expect("mX: a shader");
        let uv_pipeline = device
            .create_compute_pipeline(&mx::ComputePipelineDescriptor {
                label: Some("rt texture coordinates"),
                module: &module,
                entry_point: Some("cs_tri_uv"),
                constants: &[],
            })
            .expect("mX: a compute pipeline");
        let uv_cap = 1 << 18;
        TexStore {
            atlas,
            atlas_view,
            atlas_store,
            uvs: make_uvs(device, uv_cap),
            uvs_old: None,
            uv_cap,
            uv_top: 0,
            uv_free: Vec::new(),
            meshes: hashbrown::HashMap::new(),
            jobs: Vec::new(),
            job_buf: uniforms("rt texture coordinates jobs", UV_BUDGET),
            tile_buf: uniforms("rt texture tiles", AVG_BUDGET),
            stride,
            uv_pipeline,
        }
    }

    /// Where mesh `id`'s triangles' coordinates are (in triangles), queued for a copy when it
    /// has none yet - at most `UV_BUDGET` a frame; None while it has none.
    pub(super) fn mesh_uvs(&mut self, device: &mx::Device, id: MeshId, m: &GpuMesh, frame: u64) -> Option<u32> {
        if let Some(e) = self.meshes.get_mut(&id) {
            if e.generation == m.generation {
                e.used = frame;
                return Some(e.at as u32);
            }
            let e = self.meshes.remove(&id).expect("there");
            page_give(&mut self.uv_free, &mut self.uv_top, e.at, e.tris);
        }
        let tris = m.index_bytes / 12;
        let limits = device.limits();
        let max_binding = limits.max_storage_buffer_binding_size;
        if tris == 0 || self.jobs.len() >= UV_BUDGET || m.vertex_buf.size() > max_binding || m.index_buf.size() > max_binding {
            return None;
        }
        let at = match page_take(&mut self.uv_free, &mut self.uv_top, self.uv_cap, tris) {
            Some(at) => at,
            None => {
                // (grown: what is there is copied over at the next encode)
                let most = max_binding.min(limits.max_buffer_size) / UV_TRI;
                let cap = (self.uv_top + tris).next_power_of_two().min(most);
                if cap <= self.uv_cap {
                    return None;
                }
                let old = std::mem::replace(&mut self.uvs, make_uvs(device, cap));
                self.uvs_old.get_or_insert(old);
                self.uv_cap = cap;
                page_take(&mut self.uv_free, &mut self.uv_top, self.uv_cap, tris)?
            }
        };
        if at + tris > u32::MAX as u64 {
            page_give(&mut self.uv_free, &mut self.uv_top, at, tris);
            return None;
        }
        self.meshes.insert(id, MeshUv { generation: m.generation, at, tris, used: frame });
        self.jobs.push(UvJob { vb: m.vertex_buf.clone(), ib: m.index_buf.clone(), job: [m.base_vertex.max(0) as u32, m.first_index, tris as u32, at as u32] });
        Some(at as u32)
    }

    /// Let the coordinates of meshes not traced for a while go.
    pub(super) fn retain(&mut self, frame: u64) {
        let (free, top) = (&mut self.uv_free, &mut self.uv_top);
        self.meshes.retain(|_, e| {
            let keep = frame - e.used < 1200;
            if !keep {
                page_give(free, top, e.at, e.tris);
            }
            keep
        });
    }

    /// Whether the buffer was made anew since the bind groups were (then they are made again).
    pub(super) fn grown(&self) -> bool {
        self.uvs_old.is_some()
    }

    /// Copy this frame's meshes' coordinates (and what the buffer held, when it grew).
    pub(super) fn encode_uvs(&mut self, device: &mx::Device,
        queue: &mx::Queue,
        encoder: &mut mx::CommandEncoder,) {
        if let Some(old) = self.uvs_old.take() {
            encoder.copy_buffer_to_buffer(&old, 0, &self.uvs, 0, old.size().min(self.uvs.size()));
        }
        if self.jobs.is_empty() {
            return;
        }
        let mut bytes = vec![0u8; self.jobs.len() * self.stride as usize];
        for (k, j) in self.jobs.iter().enumerate() {
            let at = k * self.stride as usize;
            bytes[at..at + 16].copy_from_slice(bytemuck::cast_slice(&j.job));
        }
        queue
            .write_buffer(&self.job_buf, 0, &bytes)
            .expect("mX: a queue write");
        let groups: Vec<mx::BindGroup> = self
            .jobs
            .iter()
            .enumerate()
            .map(|(k, j)| {
                device.create_bind_group(&mx::BindGroupDescriptor {
                    label: Some("rt texture coordinates"),
                    entries: &[
                        mx::BindGroupEntry {
                            binding: 0,
                            resource: j.vb.as_entire_binding(),
                        },
                        mx::BindGroupEntry {
                            binding: 1,
                            resource: j.ib.as_entire_binding(),
                        },
                        mx::BindGroupEntry {
                            binding: 2,
                            resource: self.uvs.as_entire_binding(),
                        },
                        mx::BindGroupEntry {
                            binding: 3,
                            resource: mx::BindingResource::Buffer(mx::BufferBinding {
                                buffer: &self.job_buf,
                                offset: k as u64 * self.stride,
                                size: Some(16), }) },
                    ],
                })
            })
            .collect();
        let mut pass = encoder.begin_compute_pass(&mx::ComputePassDescriptor {
            label: Some("rt texture coordinates"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.uv_pipeline);
        for (j, bg) in self.jobs.iter().zip(&groups) {
            pass.set_bind_group(0, bg);
            // (64 triangles a group; rows of 65535 groups for a mesh of more than 4 M)
            let groups = j.job[2].div_ceil(64);
            pass.dispatch_workgroups(groups.min(65535), groups.div_ceil(65535), 1);
        }
        drop(pass);
        self.jobs.clear();
    }

    /// The atlas tall enough for textures `wanted` (grown: the tiles made so far copied over);
    /// true when it was made anew.
    pub(super) fn ensure_atlas(&mut self, device: &mx::Device,
        encoder: &mut mx::CommandEncoder, wanted: &[usize]) -> bool {
        let Some(&last) = wanted.iter().filter(|&&t| t < MAX_TILES).max() else { return false };
        let need = ((last as u32 / PER_ROW + 1) * TILE).next_power_of_two().min(ATLAS_W);
        let have = self.atlas.height();
        if need <= have {
            return false;
        }
        let (atlas, view, store) = make_atlas(device, need);
        encoder.copy_texture_to_texture(self.atlas.as_image_copy(), atlas.as_image_copy(), mx::Extent3d { width: ATLAS_W, height: have, depth_or_array_layers: 1 });
        (self.atlas, self.atlas_view, self.atlas_store) = (atlas, view, store);
        true
    }

    /// Write the tiles' corners of textures `wanted` for the texture means (rt_avg.wgsl
    /// `tile`: xy the corner, z 1 where the texture has a tile).
    pub(super) fn write_tiles(&self, queue: &mx::Queue, wanted: &[usize]) {
        let mut bytes = vec![0u8; wanted.len() * self.stride as usize];
        for (k, &t) in wanted.iter().enumerate() {
            let tile = if t < MAX_TILES { [(t as u32 % PER_ROW * TILE) as f32, (t as u32 / PER_ROW * TILE) as f32, 1.0, 0.0] } else { [0.0; 4] };
            let at = k * self.stride as usize;
            bytes[at..at + 16].copy_from_slice(bytemuck::cast_slice(&tile));
        }
        if !bytes.is_empty() {
            queue
                .write_buffer(&self.tile_buf, 0, &bytes)
                .expect("mX: a queue write");
        }
    }

    /// The texture means' bind group entries for the atlas and the `k`-th tile's uniform.
    pub(super) fn avg_bindings(&self, k: usize) -> [mx::BindGroupEntry<'_>; 2] {
        [
            mx::BindGroupEntry {
                binding: 3,
                resource: mx::BindingResource::TextureView(&self.atlas_store),
            },
            mx::BindGroupEntry {
                binding: 4,
                resource: mx::BindingResource::Buffer(mx::BufferBinding {
                    buffer: &self.tile_buf,
                    offset: k as u64 * self.stride,
                    size: Some(16),
                }),
            },
        ]
    }
}

/// How a material addresses its textures, as rt_reflect.wgsl `tile_texel` takes it.
pub(super) fn address_mode(a: TexAddressing) -> u32 {
    match a {
        TexAddressing::Wrap => 0,
        TexAddressing::Mirror => 1,
        TexAddressing::Clamp => 2,
        TexAddressing::MirrorOnce => 3,
    }
}
