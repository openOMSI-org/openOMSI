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

/// The enhanced clouds' noise textures (clouds.rs), made once, with their mip chains: the
/// weather map (2-D RGBA8), the detail volume (3-D R8), the base volume (3-D R8) and the
/// curl map (2-D RG8), a repeating sampler, and the CPU's copy of what the clouds' shadow is
/// worked out from. Without `enhanced_noise` (the launcher's preview, which draws no
/// enhanced sky) each is a texel, and nothing is generated.
pub(crate) struct CloudTextures {
    pub moon: mx::TextureView,
    pub weather: mx::TextureView,
    pub detail: mx::TextureView,
    pub base: mx::TextureView,
    pub curl: mx::TextureView,
    pub sampler: mx::Sampler,
    pub cpu: crate::lighting::CloudCpu,
}

/// Edge of `assets/moon.png` in texels.
const MOON_MAP_SIZE: u32 = 512;

/// The moon's face (sky_enhanced.wgsl `t_moon`) and its mip levels, level 0 first.
fn moon_map() -> Vec<Vec<u8>> {
    let png = include_bytes!("../../omsi-render/assets/moon.png");
    let face = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .expect("the moon's map")
        .into_luma8();
    assert_eq!(face.dimensions(), (MOON_MAP_SIZE, MOON_MAP_SIZE));
    let mut levels = vec![face.into_raw()];
    let mut size = MOON_MAP_SIZE as usize;
    while size > 1 {
        let above = &levels[levels.len() - 1];
        let half = size / 2;
        let level = (0..half * half)
            .map(|i| {
                let at = i / half * 2 * size + i % half * 2;
                let four = [at, at + 1, at + size, at + size + 1];
                ((four.iter().map(|&p| u32::from(above[p])).sum::<u32>() + 2) / 4) as u8
            })
            .collect();
        levels.push(level);
        size = half;
    }
    levels
}

pub(crate) fn cloud_noise_textures(
    device: &mx::Device,
    queue: &mx::Queue,
    enhanced_noise: bool,
) -> CloudTextures {
    let t0 = std::time::Instant::now();
    // (made once a process, and kept on the disk between starts: see `clouds::noise`)
    let none = clouds::Noise {
        weather: vec![vec![0, 0, 0, 0]],
        base: vec![vec![0]],
        detail: vec![vec![0]],
        curl: vec![vec![128, 128]],
    };
    let n = if enhanced_noise {
        clouds::noise()
    } else {
        &none
    };
    let make = |label: &str,
                size: u32,
                dim: mx::TextureDimension,
                format: mx::TextureFormat,
                bpp: u32,
                levels: &[Vec<u8>]| {
        let size = if enhanced_noise { size } else { 1 };
        let depth = if dim == mx::TextureDimension::D3 {
            size
        } else {
            1
        };
        let tex = device
            .create_texture(&mx::TextureDescriptor {
                label: Some(label),
                size: mx::Extent3d {
                    width: size,
                    height: size,
                    depth_or_array_layers: depth,
                },
                mip_level_count: levels.len() as u32,
                sample_count: 1,
                dimension: dim,
                format,
                usage: mx::TextureUsages::TEXTURE_BINDING | mx::TextureUsages::COPY_DST,
                view_formats: &[],
            })
            .expect("mX: a texture");
        for (m, data) in levels.iter().enumerate() {
            let e = (size >> m).max(1);
            let d = if dim == mx::TextureDimension::D3 {
                e
            } else {
                1
            };
            queue
                .write_texture(
                    mx::TexelCopyTextureInfo {
                        texture: &tex,
                        mip_level: m as u32,
                        origin: mx::Origin3d::ZERO,
                        aspect: mx::TextureAspect::All,
                    },
                    data,
                    mx::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(e * bpp),
                        rows_per_image: Some(e),
                    },
                    mx::Extent3d {
                        width: e,
                        height: e,
                        depth_or_array_layers: d,
                    },
                )
                .expect("mX: a queue write");
        }
        tex.create_view(&mx::TextureViewDescriptor::default())
            .expect("mX: a texture view")
    };
    use mx::TextureDimension::{D2, D3};
    use mx::TextureFormat::{R8Unorm, Rg8Unorm, Rgba8Unorm};
    let weather = make(
        "cloud weather",
        clouds::WEATHER_SIZE,
        D2,
        Rgba8Unorm,
        4,
        &n.weather,
    );
    let detail = make(
        "cloud detail",
        clouds::DETAIL_SIZE,
        D3,
        R8Unorm,
        1,
        &n.detail,
    );
    let base = make("cloud base", clouds::BASE_SIZE, D3, R8Unorm, 1, &n.base);
    let curl = make("cloud curl", clouds::CURL_SIZE, D2, Rg8Unorm, 2, &n.curl);
    let sampler = device
        .create_sampler(&mx::SamplerDescriptor {
            label: Some("cloud noise"),
            address_mode_u: mx::AddressMode::Repeat,
            address_mode_v: mx::AddressMode::Repeat,
            address_mode_w: mx::AddressMode::Repeat,
            mag_filter: mx::FilterMode::Linear,
            min_filter: mx::FilterMode::Linear,
            mipmap_filter: mx::MipmapFilterMode::Linear,
            ..Default::default()
        })
        .expect("mX: a sampler");
    log::info!("cloud noise made in {:.2} s", t0.elapsed().as_secs_f32());
    // (the second levels kept for the clouds' shadow, `cloud_field`)
    let cpu = if enhanced_noise {
        crate::lighting::CloudCpu::new(&n.weather[1], &n.base[1])
    } else {
        Default::default()
    };
    let moon = make(
        "moon",
        MOON_MAP_SIZE,
        D2,
        R8Unorm,
        1,
        &if enhanced_noise {
            moon_map()
        } else {
            vec![vec![0]]
        },
    );
    CloudTextures {
        moon,
        weather,
        detail,
        base,
        curl,
        sampler,
        cpu,
    }
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
        let views: Vec<&mx::TextureView> =
            textures.iter().map(|t| &scene.textures[*t].view).collect();
        let bg = HeapGroup::of(
            &self.sky.layout,
            1,
            &mx::BindGroupDescriptor {
                label: Some("sky"),
                entries: &[
                    mx::BindGroupEntry {
                        binding: 0,
                        resource: mx::BindingResource::TextureView(views[0]),
                    },
                    mx::BindGroupEntry {
                        binding: 1,
                        resource: mx::BindingResource::TextureView(views[1]),
                    },
                    mx::BindGroupEntry {
                        binding: 2,
                        resource: mx::BindingResource::TextureView(views[2]),
                    },
                    mx::BindGroupEntry {
                        binding: 3,
                        resource: mx::BindingResource::Sampler(&self.sky.sky_sampler),
                    },
                    mx::BindGroupEntry {
                        binding: 4,
                        resource: mx::BindingResource::TextureView(match clouds {
                            Some(c) => &scene.textures[c].view,
                            None => &self.black_texture.view,
                        }),
                    },
                    mx::BindGroupEntry {
                        binding: 5,
                        resource: mx::BindingResource::Sampler(&self.sampler),
                    },
                    mx::BindGroupEntry {
                        binding: 6,
                        resource: mx::BindingResource::TextureView(&self.sky.cloud_shape_view),
                    },
                    mx::BindGroupEntry {
                        binding: 7,
                        resource: mx::BindingResource::TextureView(&self.sky.cloud_detail_view),
                    },
                    mx::BindGroupEntry {
                        binding: 8,
                        resource: mx::BindingResource::Sampler(&self.sky.cloud_sampler),
                    },
                    mx::BindGroupEntry {
                        binding: 9,
                        resource: mx::BindingResource::TextureView(match vanilla_clouds {
                            Some(c) => &scene.textures[c].view,
                            None => &self.black_texture.view,
                        }),
                    },
                    mx::BindGroupEntry {
                        binding: 10,
                        resource: self.sky.vanilla_sky_buf.as_entire_binding(),
                    },
                    mx::BindGroupEntry {
                        binding: 11,
                        resource: mx::BindingResource::TextureView(match sun {
                            Some(s) => &scene.textures[s].view,
                            None => &self.black_texture.view,
                        }),
                    },
                    mx::BindGroupEntry {
                        binding: 12,
                        resource: self.sky.cloud_view_buf.as_entire_binding(),
                    },
                    mx::BindGroupEntry {
                        binding: 13,
                        resource: mx::BindingResource::TextureView(&self.sky.moon_view),
                    },
                ],
            },
        );
        scene.sky_bind_group = Some(bg);
    }
}

/// The sky: its layout, sampler, dome, the classic sky's weather, the enhanced clouds' noise, the sky table and the sky being computed.
pub(crate) struct SkyRes {
    /// The sky module's places in the heap (`SkyBase::layout`).
    pub(crate) layout: crate::shaders::HeapLayout,
    /// The enhanced sky tone-mapped into the plain pass's target: the sky of the mirrors
    /// of an Enhanced picture (sky_enhanced.wgsl `fs_enhanced_mirror`).
    pub(crate) sky_mirror_pipeline: Option<mx::RenderPipeline>,
    pub(crate) sky_sampler: mx::Sampler,
    /// The classic sky's weather (`VanillaSkyUniform`, sky bind group binding 10).
    pub(crate) vanilla_sky_buf: mx::Buffer,
    /// The enhanced clouds' noise (clouds.rs): the weather map, the detail volume and their
    /// repeating, mip-mapped sampler (sky bind group bindings 6-8).
    pub(crate) cloud_shape_view: mx::TextureView,
    pub(crate) cloud_detail_view: mx::TextureView,
    pub(crate) cloud_sampler: mx::Sampler,
    /// The moon's face (sky bind group binding 13).
    pub(crate) moon_view: mx::TextureView,
    /// Where the screen's clouds were traced from (cloud_pass.rs `CloudView`, sky bind group
    /// binding 12): the dome looks them up through it.
    pub(crate) cloud_view_buf: mx::Buffer,
    /// The CPU's copy of the clouds' weather map and base volume, for their shadow on the
    /// street, and how much of the sun the clouds let through over the camera now (smoothed).
    pub(crate) cloud_shape_cpu: crate::lighting::CloudCpu,
    pub(crate) cloud_sun: Option<f32>,
    /// The clouds' shadow over the land, held in the sky table's texture (`cloud_field`).
    pub(crate) cloud_field: FieldState,
    pub(crate) sky_mesh: (mx::Buffer, mx::Buffer, u32),
    pub(crate) sky_lut: mx::Texture,
    pub(crate) sky_lut_view: mx::TextureView,
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
