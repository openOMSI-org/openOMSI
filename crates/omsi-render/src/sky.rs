//! The sky's own resources: the classic sky's textures and weather, and the enhanced
//! clouds' noise.

use crate::*;

/// The weather of the classic sky, the values Omsi.exe draws its haze and clouds from
/// (its THimmel render 0x5d8e98 and the cloud layer 0x754e44).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VanillaSky {
    /// The weather's `[clouds]` height H (m over the map's zero): the apex of the cloud
    /// cone, and the measure of the haze over the horizon - also with no clouds (-1).
    pub cloud_height: f32,
    /// The weather's `[fog]` range as the file gives it (m): the haze over the horizon.
    pub fog_range: f32,
    /// How far one sees (m: the fog range, shortened by rain and snow): the far clouds go
    /// over into the fog colour.
    pub visibility: f32,
    /// The cloud type's size in `Weather/clouds.cfg` (m of ground a tile of its texture
    /// covers); 0: no clouds.
    pub cloud_size: f32,
    /// The cloud type is an `ovc` one (its texture an opaque deck).
    pub overcast: bool,
    /// How far the wind has carried the clouds (m, east and north; modulo
    /// `VANILLA_CLOUD_PERIOD`).
    pub cloud_offset: [f32; 2],
}

/// The period the classic clouds' drift is kept in (m): a whole number of tiles of the stock
/// cloud types (1000 and 2000 m), so that the wrap does not move them.
pub const VANILLA_CLOUD_PERIOD: f32 = 10000.0;

/// The classic sky's uniform (sky.wgsl `VanillaSkyUniform`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct VanillaSkyUniform {
    /// cloud height, cloud size (0: none), drift east, drift north
    pub(crate) cloud: [f32; 4],
    /// fog range, visibility, 1 for an overcast type, the render origin's height
    pub(crate) haze: [f32; 4],
}

/// The enhanced clouds' noise textures (clouds.rs), made once: the shape map (2-D RGBA8)
/// and the detail volume (3-D R8), both with their mip chains, and a repeating sampler.
/// Without `enhanced_noise` (the launcher's preview, which draws no enhanced sky) both are
/// a texel each, and nothing is generated.
pub(crate) fn cloud_noise_textures(device: &wgpu::Device, queue: &wgpu::Queue, enhanced_noise: bool) -> (wgpu::TextureView, wgpu::TextureView, wgpu::Sampler, Vec<u8>) {
    let t0 = std::time::Instant::now();
    // (made once a process, and kept on the disk between starts: see `clouds::noise`)
    let none = (vec![vec![0, 0, 0, 255]], vec![vec![0]]);
    let (shape, detail) = match enhanced_noise.then(clouds::noise) {
        Some(n) => (&n.shape, &n.detail),
        None => (&none.0, &none.1),
    };
    let make = |label: &str, size: u32, dim: wgpu::TextureDimension, format: wgpu::TextureFormat, bpp: u32, levels: &[Vec<u8>]| {
        let depth = if dim == wgpu::TextureDimension::D3 { size } else { 1 };
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: depth },
            mip_level_count: levels.len() as u32,
            sample_count: 1,
            dimension: dim,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (m, data) in levels.iter().enumerate() {
            let e = (size >> m).max(1);
            let d = if dim == wgpu::TextureDimension::D3 { e } else { 1 };
            queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: m as u32, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                data,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(e * bpp), rows_per_image: Some(e) },
                wgpu::Extent3d { width: e, height: e, depth_or_array_layers: d },
            );
        }
        tex.create_view(&wgpu::TextureViewDescriptor::default())
    };
    let (shape_size, detail_size) = if enhanced_noise { (clouds::SHAPE_SIZE, clouds::DETAIL_SIZE) } else { (1, 1) };
    let shape_view = make("cloud shape", shape_size, wgpu::TextureDimension::D2, wgpu::TextureFormat::Rgba8Unorm, 4, shape);
    let detail_view = make("cloud detail", detail_size, wgpu::TextureDimension::D3, wgpu::TextureFormat::R8Unorm, 1, detail);
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("cloud noise"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..Default::default()
    });
    log::info!("cloud noise made in {:.2} s", t0.elapsed().as_secs_f32());
    // (the shape map's second level kept for the clouds' shadow, `cloud_sun_transmittance`)
    let cpu = shape.get(1).cloned().unwrap_or_default();
    (shape_view, detail_view, sampler, cpu)
}

impl Renderer {
    /// Sky gradient textures (day, twilight, night); without them the sky is the clear colour.
    pub fn set_sky_textures(&self, scene: &mut Scene, textures: [TextureId; 3]) {
        self.set_sky_textures_clouds(scene, textures, None)
    }

    /// Sky gradients plus an optional tiling cloud texture (`Texture\clouds.tga`).
    pub fn set_sky_textures_clouds(
        &self,
        scene: &mut Scene,
        textures: [TextureId; 3],
        clouds: Option<TextureId>,
    ) {
        self.set_sky_textures_vanilla(scene, textures, clouds, None, None)
    }

    /// Sky gradients, the cloud field (`clouds`, the enhanced sky's and its weather's
    /// picture) and the weather's cloud type texture as it is (`vanilla_clouds`: the
    /// classic sky's cloud layer, see `VanillaSky`).
    pub fn set_sky_textures_vanilla(
        &self,
        scene: &mut Scene,
        textures: [TextureId; 3],
        clouds: Option<TextureId>,
        vanilla_clouds: Option<TextureId>,
        sun: Option<TextureId>,
    ) {
        let views: Vec<&wgpu::TextureView> =
            textures.iter().map(|t| &scene.textures[*t].view).collect();
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sky"),
            layout: &self.sky.sky_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(views[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(views[1]),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(views[2]),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sky.sky_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(match clouds {
                        Some(c) => &scene.textures[c].view,
                        None => &self.black_texture.view,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&self.sky.cloud_shape_view),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&self.sky.cloud_detail_view),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::Sampler(&self.sky.cloud_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: wgpu::BindingResource::TextureView(match vanilla_clouds {
                        Some(c) => &scene.textures[c].view,
                        None => &self.black_texture.view,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 10,
                    resource: self.sky.vanilla_sky_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 11,
                    resource: wgpu::BindingResource::TextureView(match sun {
                        Some(s) => &scene.textures[s].view,
                        None => &self.black_texture.view,
                    }),
                },
            ],
        });
        scene.sky_bind_group = Some(bg);
    }
}

/// The sky: its layout, sampler, dome, the classic sky's weather, the enhanced clouds' noise, the sky table and the sky being computed.
pub(crate) struct SkyRes {
    /// The enhanced sky tone-mapped into the plain pass's target: the sky of the mirrors
    /// of an Enhanced picture (sky_enhanced.wgsl `fs_enhanced_mirror`).
    pub(crate) sky_mirror_pipeline: Option<wgpu::RenderPipeline>,
    pub(crate) sky_layout: wgpu::BindGroupLayout,
    pub(crate) sky_sampler: wgpu::Sampler,
    /// The classic sky's weather (`VanillaSkyUniform`, sky bind group binding 10).
    pub(crate) vanilla_sky_buf: wgpu::Buffer,
    /// The enhanced clouds' noise (clouds.rs): the shape map, the detail volume and their
    /// repeating, mip-mapped sampler (sky bind group bindings 6-8).
    pub(crate) cloud_shape_view: wgpu::TextureView,
    pub(crate) cloud_detail_view: wgpu::TextureView,
    pub(crate) cloud_sampler: wgpu::Sampler,
    /// The shape map's second level (RGBA8), for the clouds' shadow on the street, and how
    /// much of the sun the clouds let through over the camera now (smoothed).
    pub(crate) cloud_shape_cpu: Vec<u8>,
    pub(crate) cloud_sun: Option<f32>,
    /// The clouds' shadow over the land, held in the sky table's texture (`cloud_field`).
    pub(crate) cloud_field: FieldState,
    pub(crate) sky_mesh: (wgpu::Buffer, wgpu::Buffer, u32),
    pub(crate) sky_lut: wgpu::Texture,
    pub(crate) sky_lut_view: wgpu::TextureView,
    pub(crate) sky_state: Option<atmosphere::SkyState>,
    /// How much the lamps round the camera light the night sky, relative to a city's
    /// (`lamp_sky_glow`), as the sky is computed with it.
    pub(crate) city_glow: Option<f32>,
    /// A sky being computed on a helper thread, for this input.
    pub(crate) sky_job: Option<(
        atmosphere::SkyInput,
        std::sync::mpsc::Receiver<atmosphere::SkyState>,
    )>,
}
