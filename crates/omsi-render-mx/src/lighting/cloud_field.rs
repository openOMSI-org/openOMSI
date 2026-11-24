//! The clouds' shadow over the land.
//!
//! The sun through the cloud layer is not one number for the whole picture: a heap a
//! kilometre and a half up throws a shadow a few hundred metres across, and from a hill
//! the land is a patchwork of lit and shaded ground that moves with the wind. The field
//! here is that patchwork: the share of the sun that reaches the cloud layer's middle at
//! every point of a square some nine kilometres wide, worked out with the sky shader's own
//! clouds (`cloud_density` in sky_enhanced.wgsl: the weather map, the cloud type's height
//! profile and the base volume, with the detail's erosion taken on average) along the sun's
//! direction through the layer. It is indexed by *cloud space* - the world plus the clouds'
//! drift - so the wind moves the shadows without the field being drawn again; only a sun
//! that has moved on, another cover or layer, the base volume having drifted on against the
//! weather map (the clouds' slow change of shape), or a camera that has left the middle of
//! the square makes a new one, which a helper thread works out.
//!
//! The field lives in the sky table's texture (right of the table, `FIELD_X0`): the
//! enhanced shaders all have that texture, and the scene's camera group has no room left
//! for another one (sixteen on a Metal fragment stage). The shaders look it up through
//! `cloud_shadow_ratio` in enhanced_common.wgsl: the main pass lights the ground with it,
//! the fog pass takes the sun's shafts through the gaps of a broken sky from it.

use crate::*;
use glam::{Vec2, Vec3};

/// Texels along a side of the field.
pub(crate) const FIELD_N: u32 = 192;
/// Its width (m).
pub(crate) const FIELD_EXTENT: f32 = 9600.0;
/// Where the field starts in the sky table's texture, one texel clear of the table so that
/// the table's bilinear taps never reach it.
pub(crate) const FIELD_X0: u32 = atmosphere::SKY_LUT_W + 1;
/// The sky table's texture: the table in its corner, the field beside it.
pub(crate) const SKY_TEX_W: u32 = FIELD_X0 + FIELD_N;
pub(crate) const SKY_TEX_H: u32 = FIELD_N;

// The cloud model's numbers, as sky_enhanced.wgsl has them.
const WEATHER_PERIOD: f32 = 35000.0;
const BASE_PERIOD: f32 = 5000.0;
/// How much faster than the weather map the base volume drifts (the clouds change shape).
pub(crate) const BASE_DRIFT: f32 = 1.12;
const CLOUD_SIGMA: f32 = 0.05;
const DETAIL_ERODE: f32 = 0.32;
/// The detail volume's mean, which the field takes off every heap in place of the detail.
const DETAIL_MEAN: f32 = 0.5;
const COVER_SPREAD: f32 = 1.1;
/// The steps through the layer for one texel of the field.
const STEPS: usize = 16;

/// The weather's clouds as a whole, as `cloud_sky` in sky_enhanced.wgsl works them out:
/// the cover, how much of it is a closed deck, the storm, the cloud type (1 heaps, ~0.3 a
/// sheet), how much the detail erodes and how dense they are.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct CloudParams {
    pub(crate) cover: f32,
    pub(crate) deck: f32,
    pub(crate) storm: f32,
    pub(crate) kind: f32,
    pub(crate) erode: f32,
    pub(crate) dens: f32,
}

impl CloudParams {
    /// From the cover (`Lighting::cloud_density`), the sky's overcast (`SkyInput::overcast`,
    /// enh.weather.w), the rain (enh.weather.z) and the storm (enh.layers.w).
    pub(crate) fn new(cover: f32, overcast: f32, rain: f32, storm: f32) -> CloudParams {
        let ss = atmosphere::smoothstep;
        let cover = cover.clamp(0.0, 1.0);
        let deck = ss(0.8, 1.0, cover).max(ss(0.55, 1.0, overcast));
        let kind = (1.0 + (0.3 + 0.35 * rain - 1.0) * deck).max(storm);
        CloudParams {
            cover,
            deck,
            storm,
            kind,
            erode: DETAIL_ERODE * (1.0 - 0.3 * deck),
            dens: 1.0 + 1.2 * (deck * (0.4 + 0.6 * rain)).max(storm),
        }
    }

    /// The clouds of this light (the same numbers the shaders get).
    pub(crate) fn of(lighting: &Lighting) -> CloudParams {
        let wet_cover = (lighting.rain * 1.5).clamp(0.0, 1.0);
        let storm = forced_storm()
            .map_or(lighting.storm, |f| f.0)
            .clamp(0.0, 1.0);
        CloudParams::new(
            lighting.cloud_density,
            lighting.overcast.clamp(0.0, 1.0).max(wet_cover),
            lighting.rain.clamp(0.0, 1.0),
            storm,
        )
    }
}

/// The CPU's copy of the clouds' weather map (RGBA8, `clouds::WEATHER_SIZE / 2` a side) and
/// base volume (R8, `clouds::BASE_SIZE / 2` a side): their second levels.
#[derive(Clone, Default)]
pub(crate) struct CloudCpu {
    weather: std::sync::Arc<Vec<u8>>,
    base: std::sync::Arc<Vec<u8>>,
}

impl CloudCpu {
    pub(crate) fn new(weather: &[u8], base: &[u8]) -> CloudCpu {
        CloudCpu {
            weather: std::sync::Arc::new(weather.to_vec()),
            base: std::sync::Arc::new(base.to_vec()),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.weather.len() < (clouds::WEATHER_SIZE as usize / 2).pow(2) * 4
            || self.base.len() < (clouds::BASE_SIZE as usize / 2).pow(3)
    }

    /// The weather map bilinearly at `uv`, repeating.
    fn weather(&self, u: f32, v: f32) -> [f32; 4] {
        let n = (clouds::WEATHER_SIZE / 2) as i64;
        let (x, y) = (u * n as f32 - 0.5, v * n as f32 - 0.5);
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let texel = |x: i64, y: i64| {
            let i = ((y.rem_euclid(n) * n + x.rem_euclid(n)) * 4) as usize;
            [0, 1, 2, 3].map(|c| self.weather[i + c] as f32 / 255.0)
        };
        let (a, b, c, d) = (
            texel(x0, y0),
            texel(x0 + 1, y0),
            texel(x0, y0 + 1),
            texel(x0 + 1, y0 + 1),
        );
        [0, 1, 2, 3].map(|k| {
            (a[k] * (1.0 - fx) + b[k] * fx) * (1.0 - fy) + (c[k] * (1.0 - fx) + d[k] * fx) * fy
        })
    }

    /// The base volume trilinearly at `p` (in its periods), repeating.
    fn base(&self, p: Vec3) -> f32 {
        let n = (clouds::BASE_SIZE / 2) as i64;
        let q = p * n as f32 - Vec3::splat(0.5);
        let i = q.floor();
        let f = q - i;
        let (x0, y0, z0) = (i.x as i64, i.y as i64, i.z as i64);
        let texel = |x: i64, y: i64, z: i64| {
            self.base[((z.rem_euclid(n) * n + y.rem_euclid(n)) * n + x.rem_euclid(n)) as usize]
                as f32
                / 255.0
        };
        let mut sum = 0.0;
        for k in 0..8 {
            let (dx, dy, dz) = (k & 1, (k >> 1) & 1, k >> 2);
            let w = if dx == 1 { f.x } else { 1.0 - f.x }
                * if dy == 1 { f.y } else { 1.0 - f.y }
                * if dz == 1 { f.z } else { 1.0 - f.z };
            sum += w * texel(x0 + dx, y0 + dy, z0 + dz);
        }
        sum
    }
}

fn linearstep(a: f32, b: f32, v: f32) -> f32 {
    ((v - a) / (b - a)).clamp(0.0, 1.0)
}

/// How much of a cloud type's layer its clouds fill at height fraction `h` (sky_enhanced.wgsl
/// `cloud_profile`): a sheet low in the layer with a flat top, heaps with a flat base and
/// a rounded top, as tall as `tall` lets them grow.
pub(crate) fn cloud_profile(h: f32, kind: f32, tall: f32) -> f32 {
    let top = (0.16 + 0.84 * kind) * (0.6 + 0.4 * tall);
    let bottom = linearstep(0.0, 0.05 + 0.05 * kind, h);
    bottom * (1.0 - linearstep(top * (0.75 - 0.4 * kind), top, h))
}

/// The extinction (1/m) of the cloud layer at height fraction `h` over the cloud-space point
/// `g` (m), `depth` the layer's depth (m) and `base_shift` how far the base volume has drifted
/// against the weather map: the sky shader's clouds with the detail's erosion on average.
pub(crate) fn layer_sigma(
    cpu: &CloudCpu,
    g: Vec2,
    h: f32,
    depth: f32,
    base_shift: Vec2,
    c: &CloudParams,
) -> f32 {
    if !(0.0..1.0).contains(&h) || c.cover <= 0.001 {
        return 0.0;
    }
    let w = cpu.weather(g.x / WEATHER_PERIOD, g.y / WEATHER_PERIOD);
    let f = w[0] * 0.7 + w[3] * 0.3;
    let cov = (c.cover + (f - 0.5) * COVER_SPREAD).clamp(0.0, 1.0);
    let cov = cov + (1.0 - cov) * c.deck;
    if cov <= 0.001 {
        return 0.0;
    }
    let kind = (c.kind + (w[1] - 0.5) * 0.5 * (1.0 - c.deck)).clamp(0.0, 1.0);
    let prof = cloud_profile(h, kind, w[2].max(c.storm));
    if prof <= 0.0 {
        return 0.0;
    }
    let b = cpu.base(Vec3::new(g.x + base_shift.x, g.y + base_shift.y, h * depth) / BASE_PERIOD);
    let base = b + (0.5 + 0.5 * b - b) * c.deck;
    let m = ((base * prof - (1.0 - cov)) / cov.max(1e-3)).clamp(0.0, 1.0) * cov;
    let e = DETAIL_MEAN * c.erode;
    ((m - e) / (1.0 - e)).clamp(0.0, 1.0) * CLOUD_SIGMA * c.dens
}

/// What a field is made from; a new one is made when this has moved on.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FieldKey {
    pub(crate) sun: Vec3,
    pub(crate) clouds: CloudParams,
    pub(crate) bottom: f32,
    pub(crate) top: f32,
    pub(crate) base_shift: Vec2,
}

impl FieldKey {
    /// Whether a field made for `o` still serves: the sun's direction moves it on after about a
    /// degree (a few minutes of the day), the base volume after some 60 m of drift against
    /// the weather map, and the threads that make a new one are two at most.
    pub(crate) fn near(&self, o: &FieldKey) -> bool {
        let (a, b) = (&self.clouds, &o.clouds);
        self.sun.dot(o.sun) > 0.999_850
            && (a.cover - b.cover).abs() < 0.015
            && (a.deck - b.deck).abs() < 0.02
            && (a.storm - b.storm).abs() < 0.02
            && (a.kind - b.kind).abs() < 0.02
            && (self.bottom - o.bottom).abs() < 40.0
            && (self.top - o.top).abs() < 80.0
            && (self.base_shift - o.base_shift).abs().max_element() < 60.0
    }
}

/// How much of the sun gets through the layer to the point of its middle plane over the
/// cloud-space point `c`, along the sun's way.
pub(crate) fn column_transmittance(cpu: &CloudCpu, key: &FieldKey, c: Vec2) -> f32 {
    let s = key.sun;
    // (the way through the layer, bottom to top: its height fraction runs 0..1 evenly)
    let depth = key.top - key.bottom;
    let along = depth / s.z;
    let ds = along / STEPS as f32;
    let mid = 0.5 * (key.top + key.bottom);
    let t_bottom = (key.bottom - mid) / s.z;
    let dir = Vec2::new(s.x, s.y);
    let mut od = 0.0;
    for k in 0..STEPS {
        let h = (k as f32 + 0.5) / STEPS as f32;
        let g = c + dir * (t_bottom + h * along);
        od += layer_sigma(cpu, g, h, depth, key.base_shift, &key.clouds) * ds;
    }
    (-od).exp()
}

/// The field: `FIELD_N` rows of `FIELD_N` shares of the sun, centred on the cloud-space
/// point `center`, row 0 the south edge.
pub(crate) fn compute(cpu: &CloudCpu, key: &FieldKey, center: Vec2) -> Vec<f32> {
    let n = FIELD_N as usize;
    let mut out = vec![1.0f32; n * n];
    if cpu.is_empty() || key.sun.z <= 0.02 {
        return out;
    }
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .clamp(1, 2);
    let per = n.div_ceil(threads);
    std::thread::scope(|scope| {
        for (k, chunk) in out.chunks_mut(per * n).enumerate() {
            scope.spawn(move || {
                for (i, row) in chunk.chunks_mut(n).enumerate() {
                    let y = k * per + i;
                    for (x, v) in row.iter_mut().enumerate() {
                        let c = center
                            + Vec2::new(
                                (x as f32 + 0.5) / n as f32 - 0.5,
                                (y as f32 + 0.5) / n as f32 - 0.5,
                            ) * FIELD_EXTENT;
                        *v = column_transmittance(cpu, key, c);
                    }
                }
            });
        }
    });
    out
}

/// The field as the texture holds it: RGBA half floats, the share in the red.
pub(crate) fn texels(field: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(field.len() * 8);
    for v in field {
        for c in [*v, 0.0, 0.0, 1.0] {
            bytes.extend_from_slice(&atmosphere::f16_bits(c).to_le_bytes());
        }
    }
    bytes
}

/// The field being made or in use.
#[derive(Default)]
pub(crate) struct FieldState {
    installed: Option<(FieldKey, Vec2)>,
    job: Option<(FieldKey, Vec2, std::sync::mpsc::Receiver<Vec<f32>>)>,
}

/// What the shaders are told of the field (`Enhanced::cloud_f1`, `cloud_f2`): zeros where
/// there is none.
pub(crate) type FieldUniform = ([f32; 4], [f32; 4]);

impl Renderer {
    /// Keep the clouds' shadow field up to date for this frame and say where it is. `cloud_t`
    /// is the share of the sun over the camera that the main pass's sun already has taken
    /// off (the shaders divide it out again, see `cloud_shadow_ratio`).
    pub(crate) fn update_cloud_field(
        &mut self,
        lighting: &Lighting,
        cam_rel: Vec3,
        ro: DVec3,
        cloud_t: f32,
    ) -> FieldUniform {
        let s = lighting.sun_dir.normalize_or_zero();
        let off = omsi_cfg::flags::OMSI_NO_CLOUD_SHADOW_FIELD.is_set();
        if off
            || !self.options.cloud_shadows
            || !lighting.enhanced
            || lighting.cloud_density <= 0.0
            || s.z < 0.06
            || self.sky.cloud_shape_cpu.is_empty()
        {
            self.sky.cloud_field = FieldState::default();
            return ([0.0; 4], [0.0; 4]);
        }
        let [bottom, top] = super::enhanced::cloud_levels(lighting, cam_rel, ro);
        let mid = 0.5 * (bottom + top);
        let origin = Vec2::new(
            ro.x.rem_euclid(CLOUD_ORIGIN_PERIOD) as f32,
            ro.y.rem_euclid(CLOUD_ORIGIN_PERIOD) as f32,
        );
        let drift = Vec2::new(lighting.cloud_offset[0], lighting.cloud_offset[1]) * 2500.0;
        let key = FieldKey {
            sun: s,
            clouds: CloudParams::of(lighting),
            bottom,
            top,
            base_shift: drift * (BASE_DRIFT - 1.0),
        };
        // where the sun's way through the camera's own place meets the layer's middle, in
        // cloud space: what the field is to be centred on
        let here = Vec2::new(cam_rel.x, cam_rel.y)
            + Vec2::new(s.x, s.y) / s.z * (mid - cam_rel.z)
            + origin
            + drift;
        // (a field made for another sun or cover waits for the new one to be ready: the old
        // goes on being right to within a few per cent for the minute that takes)
        if let Some((k, c, rx)) = &self.sky.cloud_field.job {
            match rx.try_recv() {
                Ok(field) => {
                    let (k, c) = (*k, *c);
                    self.sky.cloud_field.job = None;
                    self.install_cloud_field(&field);
                    self.sky.cloud_field.installed = Some((k, c));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.sky.cloud_field.job = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    let _ = (k, c);
                }
            }
        }
        let stale = match &self.sky.cloud_field.installed {
            None => true,
            Some((k, c)) => !k.near(&key) || (here - *c).abs().max_element() > FIELD_EXTENT * 0.18,
        };
        if stale && self.sky.cloud_field.job.is_none() {
            let cpu = self.sky.cloud_shape_cpu.clone();
            if self.instant_exposure {
                // (a picture on its own waits for it; a window's first frame has the ground lit by the
                // camera's own number until the helper thread's field is in)
                let field = compute(&cpu, &key, here);
                self.install_cloud_field(&field);
                self.sky.cloud_field.installed = Some((key, here));
            } else {
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::Builder::new()
                    .name("cloud shadows".into())
                    .spawn(move || {
                        let _ = tx.send(compute(&cpu, &key, here));
                    })
                    .ok();
                self.sky.cloud_field.job = Some((key, here, rx));
            }
        }
        match &self.sky.cloud_field.installed {
            Some((_, c)) => (
                [c.x, c.y, 1.0 / FIELD_EXTENT, 1.0],
                [origin.x + drift.x, origin.y + drift.y, mid, cloud_t],
            ),
            None => ([0.0; 4], [0.0; 4]),
        }
    }

    fn install_cloud_field(&mut self, field: &[f32]) {
        self.queue
            .write_texture(
                mx::TexelCopyTextureInfo {
                    texture: &self.sky.sky_lut,
                    mip_level: 0,
                    origin: mx::Origin3d {
                        x: FIELD_X0,
                        y: 0,
                        z: 0,
                    },
                    aspect: mx::TextureAspect::All,
                },
                &texels(field),
                mx::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(FIELD_N * 8),
                    rows_per_image: Some(FIELD_N),
                },
                mx::Extent3d {
                    width: FIELD_N,
                    height: FIELD_N,
                    depth_or_array_layers: 1,
                },
            )
            .expect("mX: a queue write");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cpu() -> CloudCpu {
        let n = clouds::noise();
        CloudCpu::new(&n.weather[1], &n.base[1])
    }

    #[test]
    fn the_field_is_shadow_under_a_heap_and_sun_between_heaps() {
        let cpu = cpu();
        let key = FieldKey {
            sun: Vec3::new(0.3, 0.2, 0.75).normalize(),
            clouds: CloudParams::new(0.5, 0.0, 0.0, 0.0),
            bottom: 1400.0,
            top: 2800.0,
            base_shift: Vec2::ZERO,
        };
        let field = compute(&cpu, &key, Vec2::new(3000.0, 5000.0));
        let (lo, hi) = field
            .iter()
            .fold((1.0f32, 0.0f32), |(l, h), v| (l.min(*v), h.max(*v)));
        assert!(lo < 0.25, "no shadow anywhere: darkest {lo}");
        assert!(hi > 0.95, "no sun anywhere: brightest {hi}");
        // a clear sky lets all of it through
        let clear = FieldKey {
            clouds: CloudParams::new(0.0, 0.0, 0.0, 0.0),
            ..key
        };
        assert!(compute(&cpu, &clear, Vec2::ZERO).iter().all(|v| *v > 0.99));
        // and a closed deck next to nothing
        let deck = FieldKey {
            clouds: CloudParams::new(1.0, 1.0, 0.5, 0.0),
            ..key
        };
        let mean =
            compute(&cpu, &deck, Vec2::ZERO).iter().sum::<f32>() / (FIELD_N * FIELD_N) as f32;
        assert!(mean < 0.1, "a deck lets {mean} through");
    }

    #[test]
    fn the_field_agrees_with_the_one_point_it_replaces() {
        // the camera's own transmittance (five blurred rays) against the field's value
        // there: the same clouds, so about the same share of the sun
        let cpu = cpu();
        let lighting = Lighting {
            sun_dir: Vec3::new(0.2, 0.4, 0.8).normalize(),
            cloud_density: 0.5,
            enhanced: true,
            ..Lighting::default()
        };
        let mut sum = 0.0;
        let mut n = 0.0;
        for i in 0..12 {
            let cam = Vec3::new(i as f32 * 530.0, i as f32 * 310.0, 2.0);
            let one =
                super::super::enhanced::cloud_sun_transmittance(&cpu, &lighting, cam, DVec3::ZERO);
            let key = FieldKey {
                sun: lighting.sun_dir,
                clouds: CloudParams::of(&lighting),
                bottom: 1400.0,
                top: 2800.0,
                base_shift: Vec2::ZERO,
            };
            let s = key.sun;
            let g = Vec2::new(cam.x, cam.y) + Vec2::new(s.x, s.y) / s.z * (2100.0 - cam.z);
            let here = column_transmittance(&cpu, &key, g);
            sum += (one - here).abs();
            n += 1.0;
        }
        assert!(
            sum / n < 0.2,
            "the field and the camera's value differ by {} on average",
            sum / n
        );
    }
}
