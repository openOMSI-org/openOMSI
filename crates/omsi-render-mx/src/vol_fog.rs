//! Enhanced graphics: the volumetric fog (volumetric_fog.wgsl, the settings'
//! `volumetric_fog`). The air between the camera and the depth prepass is marched at half
//! size each frame, blended into the frames before, and exchanged over the full picture for
//! the analytic air the main pass laid on. Off - or in Vanilla, or where the pipelines could
//! not be made - the main pass's analytic fog is all there is, as before.

use crate::passes::{FrameCtx, PassTimers};
use crate::pipelines::common::*;
use crate::*;

/// The side of the density noise's tiling cube (texels).
const NOISE_SIZE: u32 = 64;
/// The noise's span in the world (m) - the shader's `VF_BANKS` and `VF_EDDIES`: the
/// offsets are passed within these periods (a float of the world's coordinates far from
/// the map's origin would not keep the fog still).
const BANKS: [f64; 3] = [260.0, 260.0, 70.0];
const EDDIES: [f64; 3] = [52.0, 52.0, 26.0];
/// Steps along a ray: the settings' cloud quality (`Lighting::low_clouds`) halves them.
const STEPS: f32 = 24.0;
const STEPS_LOW: f32 = 12.0;
/// How far the air is marched (m): past it the analytic fog goes on. Through a broken sky
/// the sun's beams are followed farther (they stand kilometres long under the clouds).
const REACH: f32 = 1200.0;
const REACH_GAPS: f32 = 3000.0;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct VolFogUniform {
    pub(crate) inv_view_proj: [[f32; 4]; 4],
    pub(crate) prev_view_proj: [[f32; 4]; 4],
    pub(crate) size: [f32; 4],
    pub(crate) march: [f32; 4],
    pub(crate) prev_cam: [f32; 4],
    pub(crate) detail: [f32; 4],
}

/// The half-size results of one picture size: each frame marches into one pair and reads
/// the other (the last frame's), and the composite reads the one just marched.
struct Targets {
    size: (u32, u32),
    depth: mx::Texture,
    air: [mx::TextureView; 2],
    dist: [mx::TextureView; 2],
    march: [HeapGroup; 2],
    composite: [HeapGroup; 2],
}

/// What the next frame follows its history by.
#[derive(Clone, Copy)]
struct Last {
    view_proj: Mat4,
    ro: DVec3,
    cam_rel: Vec3,
    exposure: f32,
    size: (u32, u32),
}

pub(crate) struct VolFog {
    /// the module's places in the heap (`groups_to_heap`)
    pub(crate) layout: crate::shaders::HeapLayout,
    buf: mx::Buffer,
    march: mx::RenderPipeline,
    composite: mx::RenderPipeline,
    noise: mx::TextureView,
    sampler: mx::Sampler,
    targets: Option<Targets>,
    frame: u32,
    last: Option<Last>,
    /// How far the wind has carried the banks and the eddies since the start (m, world).
    drift: [DVec3; 2],
}

/// The shader module: the lamps-in-the-fog module (its camera, its shadow look-ups and
/// enhanced_common.wgsl) with the volumetric fog's entry points added.
pub(crate) fn shader_source() -> String {
    format!(
        "{}\n{}",
        fog_lamps_shader_source(),
        crate::shaders::hot_wgsl(include_str!("volumetric_fog.wgsl"), "volumetric_fog.wgsl")
    )
}

impl VolFog {
    /// The pipelines and the noise; `None` where the lamps-in-the-fog pass cannot be made
    /// either (OpenGL, no storage buffers) or the driver fails on them.
    pub(crate) fn new(
        device: &mx::Device,
        queue: &mx::Queue,
        hdr_format: mx::TextureFormat,
        gl: bool,
    ) -> Option<VolFog> {
        if gl || array_path().no_storage() || basic_pipelines() {
            return None;
        }
        optional_pipeline(device, "the volumetric fog", || {
            log::info!("renderer: compiling the volumetric fog shaders");
            let module = compile_shader(
                device,
                mx::ShaderModuleDescriptor {
                    label: Some("volumetric fog"),
                    source: &crate::shaders::groups_to_heap(&shader_source()),
                },
            );
            let march_targets = [
                target(mx::TextureFormat::Rgba16Float, None),
                target(mx::TextureFormat::R16Float, None),
            ];
            let march = no_vertex_pipeline(
                "volumetric fog march",
                &module,
                "fs_vol_march",
                &march_targets,
            )
            .vs("vs_vol_fog")
            .create(device);
            // the picture times the source's alpha, plus the source: the analytic air the
            // picture holds exchanged for the marched one
            let exchange = mx::BlendState {
                color: mx::BlendComponent {
                    src_factor: mx::BlendFactor::One,
                    dst_factor: mx::BlendFactor::SrcAlpha,
                    operation: mx::BlendOperation::Add,
                },
                alpha: mx::BlendComponent {
                    src_factor: mx::BlendFactor::Zero,
                    dst_factor: mx::BlendFactor::One,
                    operation: mx::BlendOperation::Add,
                },
            };
            let composite = no_vertex_pipeline(
                "volumetric fog composite",
                &module,
                "fs_vol_composite",
                &[target(hdr_format, Some(exchange))],
            )
            .vs("vs_vol_fog")
            .create(device);
            let noise = noise_texture(device, queue);
            let sampler = device
                .create_sampler(&mx::SamplerDescriptor {
                    label: Some("volumetric fog noise"),
                    address_mode_u: mx::AddressMode::Repeat,
                    address_mode_v: mx::AddressMode::Repeat,
                    address_mode_w: mx::AddressMode::Repeat,
                    mag_filter: mx::FilterMode::Linear,
                    min_filter: mx::FilterMode::Linear,
                    ..Default::default()
                })
                .expect("mX: a sampler");
            let buf = uniform_buffer(
                device,
                "volumetric fog params",
                std::mem::size_of::<VolFogUniform>() as u64,
            );
            VolFog {
                layout: crate::shaders::layout_of(&shader_source()),
                buf,
                march,
                composite,
                noise,
                sampler,
                targets: None,
                frame: 0,
                last: None,
                drift: [DVec3::ZERO; 2],
            }
        })
    }

    /// The half-size targets and bind groups for this picture size and depth prepass.
    fn ensure_targets(
        &mut self,
        device: &mx::Device,
        w: u32,
        h: u32,
        depth: &mx::Texture,
        depth_view: &mx::TextureView,
    ) {
        if self
            .targets
            .as_ref()
            .is_some_and(|t| t.size == (w, h) && t.depth == *depth)
        {
            return;
        }
        let half = mx::Extent3d {
            width: w.div_ceil(2),
            height: h.div_ceil(2),
            depth_or_array_layers: 1,
        };
        let make = |label: &str, format: mx::TextureFormat| {
            device
                .create_texture(&mx::TextureDescriptor {
                    label: Some(label),
                    size: half,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: mx::TextureDimension::D2,
                    format,
                    usage: mx::TextureUsages::RENDER_ATTACHMENT
                        | mx::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .expect("mX: a texture")
                .create_view(&Default::default())
                .expect("mX: a texture view")
        };
        let air = [
            make("volumetric fog", mx::TextureFormat::Rgba16Float),
            make("volumetric fog", mx::TextureFormat::Rgba16Float),
        ];
        let dist = [
            make("volumetric fog distance", mx::TextureFormat::R16Float),
            make("volumetric fog distance", mx::TextureFormat::R16Float),
        ];
        let bg = |k: usize| {
            HeapGroup::of(
                &self.layout,
                2,
                &mx::BindGroupDescriptor {
                    label: Some("volumetric fog"),
                    entries: &[
                        mx::BindGroupEntry {
                            binding: 0,
                            resource: self.buf.as_entire_binding(),
                        },
                        mx::BindGroupEntry {
                            binding: 1,
                            resource: mx::BindingResource::TextureView(depth_view),
                        },
                        mx::BindGroupEntry {
                            binding: 2,
                            resource: mx::BindingResource::TextureView(&air[k]),
                        },
                        mx::BindGroupEntry {
                            binding: 3,
                            resource: mx::BindingResource::TextureView(&dist[k]),
                        },
                        mx::BindGroupEntry {
                            binding: 4,
                            resource: mx::BindingResource::TextureView(&self.noise),
                        },
                        mx::BindGroupEntry {
                            binding: 5,
                            resource: mx::BindingResource::Sampler(&self.sampler),
                        },
                    ],
                },
            )
        };
        // (the march into k reads the other pair; the composite reads k)
        let march = [bg(1), bg(0)];
        let composite = [bg(0), bg(1)];
        self.targets = Some(Targets {
            size: (w, h),
            depth: depth.clone(),
            air,
            dist,
            march,
            composite,
        });
        self.last = None;
    }
}

impl Renderer {
    /// Whether this frame's picture gets the volumetric fog in place of the analytic one:
    /// the option, the pipelines and the Enhanced picture (`enh.vol_fog.x`).
    pub(crate) fn vol_fog_enabled(&self, lighting: &Lighting) -> bool {
        self.options.volumetric_fog && lighting.enhanced && self.lights.vol_fog.is_some()
    }

    /// The volumetric fog over this frame's picture, before the lamps' light in the air is
    /// added to it (`encode_fog_lamps`); true when it was drawn (the shafts are its then).
    /// `view`: the picture to draw over (`None`: nothing to draw this frame).
    pub(crate) fn encode_vol_fog(
        &mut self,
        encoder: &mut mx::CommandEncoder,
        scene: &Scene,
        f: &FrameCtx,
        vp_mat: Mat4,
        view: Option<&mx::TextureView>,
        timers: &mut PassTimers,
    ) -> bool {
        let Some(mut vf) = self.lights.vol_fog.take() else {
            return false;
        };
        let drawn = match view {
            Some(view) if self.vol_fog_enabled(f.lighting) => {
                self.record_vol_fog(&mut vf, encoder, scene, f, vp_mat, view, timers)
            }
            _ => false,
        };
        if !drawn {
            vf.last = None;
        }
        self.lights.vol_fog = Some(vf);
        drawn
    }

    #[allow(clippy::too_many_arguments)]
    fn record_vol_fog(
        &self,
        vf: &mut VolFog,
        encoder: &mut mx::CommandEncoder,
        scene: &Scene,
        f: &FrameCtx,
        vp_mat: Mat4,
        view: &mx::TextureView,
        timers: &mut PassTimers,
    ) -> bool {
        let (Some(ao), Some(cam_bg)) = (
            self.targets.ao.as_ref(),
            scene.camera_heaps.as_ref().and_then(|c| c.vol.as_ref()),
        ) else {
            return false;
        };
        let (w, h) = (f.width, f.height);
        vf.ensure_targets(&self.device, w, h, &ao.depth_tex, &ao.depth_view);
        let exposure = self.tone.enh_last.exposure[0].max(1e-12);
        let cam_rel = f.cam_rel;
        // the last frame's camera taken to this frame's render origin
        let last = vf
            .last
            .filter(|l| l.size == (w, h) && !f.xr_view && f.dt < 1.0);
        let (prev_vp, prev_cam, ratio) = match last {
            Some(l) => (
                l.view_proj * Mat4::from_translation((f.ro - l.ro).as_vec3()),
                l.cam_rel + (l.ro - f.ro).as_vec3(),
                exposure / l.exposure.max(1e-12),
            ),
            None => (vp_mat, cam_rel, 1.0),
        };
        vf.frame = vf.frame.wrapping_add(1);
        let k = (vf.frame & 1) as usize;
        // (the 2 x 2 block's pixels in a crossed order, and the steps' start by the golden ratio)
        let sub = [0.0, 3.0, 1.0, 2.0][(vf.frame % 4) as usize];
        let detail = wrapped(f.ro - vf.drift[1], EDDIES);
        let half = (w.div_ceil(2), h.div_ceil(2));
        let u = VolFogUniform {
            inv_view_proj: vp_mat.inverse().to_cols_array_2d(),
            prev_view_proj: prev_vp.to_cols_array_2d(),
            size: [w as f32, h as f32, half.0 as f32, half.1 as f32],
            march: [
                if f.lighting.low_clouds {
                    STEPS_LOW
                } else {
                    STEPS
                },
                (vf.frame as f32 * 0.618_034).fract(),
                if last.is_some() { 1.0 } else { 0.0 },
                ratio,
            ],
            prev_cam: [prev_cam.x, prev_cam.y, prev_cam.z, sub],
            detail: [detail.x, detail.y, detail.z, 0.0],
        };
        self.queue
            .write_buffer(&vf.buf, 0, bytemuck::bytes_of(&u))
            .expect("mX: a queue write");
        let t = vf.targets.as_ref().unwrap();
        {
            let mut pass = encoder.begin_render_pass(&mx::RenderPassDescriptor {
                label: Some("volumetric fog march"),
                color_attachments: &[
                    Some(mx::RenderPassColorAttachment {
                        view: &t.air[k],
                        resolve_target: None,
                        ops: mx::Operations {
                            load: mx::LoadOp::Clear(mx::Color::TRANSPARENT),
                            store: mx::StoreOp::Store,
                        },
                    }),
                    Some(mx::RenderPassColorAttachment {
                        view: &t.dist[k],
                        resolve_target: None,
                        ops: mx::Operations {
                            load: mx::LoadOp::Clear(mx::Color::TRANSPARENT),
                            store: mx::StoreOp::Store,
                        },
                    }),
                ],
                depth_stencil_attachment: None,
                timestamp_writes: pass_timer(
                    timers.set.as_ref(),
                    &mut timers.timed,
                    "volumetric fog",
                ),
                occlusion_query_set: None,
            });
            pass.set_pipeline(&vf.march);
            cam_bg.set(&mut pass);
            t.march[k].set(&mut pass);
            pass.draw(0..3, 0..1);
        }
        {
            let mut pass = encoder.begin_render_pass(&mx::RenderPassDescriptor {
                label: Some("volumetric fog composite"),
                color_attachments: &[Some(mx::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: mx::Operations {
                        load: mx::LoadOp::Load,
                        store: mx::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&vf.composite);
            cam_bg.set(&mut pass);
            t.composite[k].set(&mut pass);
            pass.draw(0..3, 0..1);
        }
        vf.last = Some(Last {
            view_proj: vp_mat,
            ro: f.ro,
            cam_rel,
            exposure,
            size: (w, h),
        });
        true
    }
}

/// The enhanced uniform's slots (enhanced_common.wgsl `vol_fog`, `vol_fog2`) for the volumetric
/// fog `vf` (`None`: off this frame), and the wind's drift of the fog carried on by `dt`: x 1
/// with the volumetric fog on, y how much the fog thickens and thins in banks (0 even), z how
/// far the air is marched (m); xyz of the second the banks' offset in the noise (m, within its
/// period).
pub(crate) fn slots(
    vf: Option<&mut VolFog>,
    lighting: &Lighting,
    ro: DVec3,
    weather_fog: f32,
    dt: f32,
    cloud_shadows: bool,
) -> ([f32; 4], [f32; 4]) {
    let Some(vf) = vf else {
        return ([0.0; 4], [0.0; 4]);
    };
    // The banks go with the wind; in a calm they still creep (a radiation fog is never
    // quite still), and the small eddies run a little faster and rise and sink as they go.
    let wind = lighting.wind.as_dvec3();
    let calm = DVec3::new(0.35, 0.2, 0.0);
    let dt = dt.clamp(0.0, 0.5) as f64;
    vf.drift[0] += (wind + calm) * dt;
    vf.drift[1] += (wind * 1.4 + calm + DVec3::new(0.0, 0.0, 0.12)) * dt;
    let banks = wrapped(ro - vf.drift[0], BANKS);
    // the droplets' share of the weather's extinction: a mist or a fog lies in banks, the
    // haze of rain or falling snow is even
    let rain = if lighting.rain > 0.0 && lighting.snowfall <= 0.0 {
        2.3 / (2500.0 - 1800.0 * lighting.rain.clamp(0.0, 1.0))
    } else {
        0.0
    };
    let snow = snowfall_extinction(lighting.snowfall);
    let droplets = if weather_fog > 0.0 {
        ((weather_fog - rain - snow) / weather_fog).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let reach = march_reach(weather_fog, lighting.cloud_density > 0.0 && cloud_shadows);
    (
        [1.0, 0.85 * droplets, reach, ground_haze(lighting)],
        [banks.x, banks.y, banks.z, 0.0],
    )
}

/// The low haze over the ground the volumetric fog marches besides the weather's own fog
/// (1/m at the ground, thinning over some 40 m - volumetric_fog.wgsl `HAZE_HEIGHT`): the
/// moisture and dust of the lowest air, thickest round sunrise and sunset (dew and the
/// evening's cooling), faint at noon, swept away by wind and washed out by rain. It is what
/// shows the sun's shafts through the crowns and between the houses on a clear morning, a
/// low sun's beams through a street's trees - the weather's fog alone is zero then, and
/// the air was as clear as a vacuum to them.
pub(crate) fn ground_haze(lighting: &Lighting) -> f32 {
    let alt = lighting.sun_dir.normalize_or_zero().z.clamp(-1.0, 1.0).asin().to_degrees();
    let low_sun = (-((alt - 3.0) / 10.0).powi(2)).exp();
    let night = crate::atmosphere::smoothstep(-2.0, -10.0, alt);
    let base = HAZE_NOON + (HAZE_LOW_SUN - HAZE_NOON) * low_sun;
    let base = base + (HAZE_NIGHT - base) * night;
    let wind = lighting.wind.truncate().length();
    let swept = 1.0 / (1.0 + wind / 5.0);
    let washed = 1.0 - 0.6 * lighting.rain.clamp(0.0, 1.0);
    base * swept * washed
}

/// The ground haze's density round a low sun, at noon and at night (1/m).
const HAZE_LOW_SUN: f32 = 0.0022;
const HAZE_NOON: f32 = 0.00045;
const HAZE_NIGHT: f32 = 0.0009;

/// How far the air is marched (m): in a thick fog only as far as anything shows through it
/// (all the steps where they are seen), in clear air or a mist as far as `REACH`, under a
/// broken sky whose beams are drawn as far as `REACH_GAPS`.
pub(crate) fn march_reach(weather_fog: f32, gaps: bool) -> f32 {
    let most = if gaps { REACH_GAPS } else { REACH };
    if weather_fog > 1e-5 {
        (6.0 / weather_fog).clamp(220.0, most)
    } else {
        most
    }
}

/// `p` within the noise's periods (each axis in [0, period)).
fn wrapped(p: DVec3, period: [f64; 3]) -> Vec3 {
    Vec3::new(
        p.x.rem_euclid(period[0]) as f32,
        p.y.rem_euclid(period[1]) as f32,
        p.z.rem_euclid(period[2]) as f32,
    )
}

/// The fog's density noise: a cube of `NOISE_SIZE` texels that tiles on every side, a sum of
/// three octaves of smooth value noise, its values spread over 0..1.
fn density_noise() -> Vec<u8> {
    let n = NOISE_SIZE as usize;
    let hash = |x: u32, y: u32, z: u32, seed: u32| {
        let mut h = x.wrapping_mul(0x8da6_b343)
            ^ y.wrapping_mul(0xd816_3841)
            ^ z.wrapping_mul(0xcb1a_b31f)
            ^ seed.wrapping_mul(0x9e37_79b9);
        h ^= h >> 15;
        h = h.wrapping_mul(0x2c1b_3c6d);
        h ^= h >> 12;
        h = h.wrapping_mul(0x297a_2d39);
        h ^= h >> 15;
        (h & 0xffff) as f32 / 65535.0
    };
    let smooth = |t: f32| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    // (cells per side of each octave: the noise wraps at every one of them)
    let octaves = [(4u32, 0.55f32, 1u32), (8, 0.3, 2), (16, 0.15, 3)];
    let mut v = vec![0.0f32; n * n * n];
    for z in 0..n {
        for y in 0..n {
            for x in 0..n {
                let mut sum = 0.0;
                for &(cells, weight, seed) in &octaves {
                    let s = cells as f32 / n as f32;
                    let (fx, fy, fz) = (x as f32 * s, y as f32 * s, z as f32 * s);
                    let (ix, iy, iz) = (fx.floor() as u32, fy.floor() as u32, fz.floor() as u32);
                    let (tx, ty, tz) = (smooth(fx.fract()), smooth(fy.fract()), smooth(fz.fract()));
                    let at = |dx: u32, dy: u32, dz: u32| {
                        hash(
                            (ix + dx) % cells,
                            (iy + dy) % cells,
                            (iz + dz) % cells,
                            seed,
                        )
                    };
                    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
                    let x0 = lerp(
                        lerp(at(0, 0, 0), at(1, 0, 0), tx),
                        lerp(at(0, 1, 0), at(1, 1, 0), tx),
                        ty,
                    );
                    let x1 = lerp(
                        lerp(at(0, 0, 1), at(1, 0, 1), tx),
                        lerp(at(0, 1, 1), at(1, 1, 1), tx),
                        ty,
                    );
                    sum += weight * lerp(x0, x1, tz);
                }
                v[(z * n + y) * n + x] = sum;
            }
        }
    }
    let (lo, hi) = v
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), &x| (lo.min(x), hi.max(x)));
    v.iter()
        .map(|&x| (((x - lo) / (hi - lo).max(1e-6)) * 255.0).round() as u8)
        .collect()
}

fn noise_texture(device: &mx::Device, queue: &mx::Queue) -> mx::TextureView {
    let size = mx::Extent3d {
        width: NOISE_SIZE,
        height: NOISE_SIZE,
        depth_or_array_layers: NOISE_SIZE,
    };
    let tex = device
        .create_texture(&mx::TextureDescriptor {
            label: Some("volumetric fog noise"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: mx::TextureDimension::D3,
            format: mx::TextureFormat::R8Unorm,
            usage: mx::TextureUsages::TEXTURE_BINDING | mx::TextureUsages::COPY_DST,
            view_formats: &[],
        })
        .expect("mX: a texture");
    queue
        .write_texture(
            mx::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: mx::Origin3d::ZERO,
                aspect: mx::TextureAspect::All,
            },
            &density_noise(),
            mx::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(NOISE_SIZE),
                rows_per_image: Some(NOISE_SIZE),
            },
            size,
        )
        .expect("mX: a queue write");
    tex.create_view(&Default::default())
        .expect("mX: a texture view")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_density_noise_tiles_and_spans_its_range() {
        let v = density_noise();
        let n = NOISE_SIZE as usize;
        assert_eq!(v.len(), n * n * n);
        assert_eq!(
            (*v.iter().min().unwrap(), *v.iter().max().unwrap()),
            (0, 255)
        );
        // (the last texel and the first of the next tile differ as little as neighbours do)
        let at = |x: usize, y: usize, z: usize| v[(z * n + y) * n + x] as i32;
        let mut worst = 0;
        for a in 0..n {
            for b in 0..n {
                worst = worst
                    .max((at(n - 1, a, b) - at(0, a, b)).abs())
                    .max((at(a, n - 1, b) - at(a, 0, b)).abs())
                    .max((at(a, b, n - 1) - at(a, b, 0)).abs());
            }
        }
        assert!(worst < 40, "seam of {worst}");
        let mean = v.iter().map(|&x| x as f64).sum::<f64>() / v.len() as f64;
        assert!((mean - 127.5).abs() < 40.0, "mean {mean}");
    }

    #[test]
    fn the_march_reaches_as_far_as_the_fog_lets_anything_show() {
        assert_eq!(march_reach(0.0, false), REACH);
        assert_eq!(march_reach(0.0, true), REACH_GAPS);
        // a fog of 100 m visibility: its first couple of hundred metres
        assert_eq!(march_reach(0.03, false), 220.0);
        assert!((march_reach(0.01, false) - 600.0).abs() < 1.0);
    }

    #[test]
    fn the_offsets_stay_within_their_periods() {
        let w = wrapped(DVec3::new(-1.0, 123_456.7, 70.0), BANKS);
        assert!((w.x - 259.0).abs() < 1e-3 && w.y >= 0.0 && w.y < 260.0 && w.z.abs() < 1e-3);
    }
}
