//! The clouds' shadow over the land.
//!
//! The sun through the cumulus layer is not one number for the whole picture: a heap a
//! kilometre and a half up throws a shadow a few hundred metres across, and from a hill
//! the land is a patchwork of lit and shaded ground that moves with the wind. The field
//! here is that patchwork: the share of the sun that reaches the cloud layer's middle at
//! every point of a square some nine kilometres wide, worked out with the sky shader's own
//! heaps (`cloud_base_shape` in sky_enhanced.wgsl, without the billows) along the sun's
//! direction through the layer. It is indexed by *cloud space* - the world plus the clouds'
//! drift - so the wind moves the shadows without the field being drawn again; only a sun
//! that has moved on, another cover or layer, or a camera that has left the middle of the
//! square makes a new one, which a helper thread works out.
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

const SHAPE_PERIOD: f32 = 13000.0;
const SIGMA: f32 = 0.035;
/// How much of the cover the billows take off the heaps' edges in the sky shader (which the
/// field leaves out): on average about this much.
pub(crate) const EROSION: f32 = 0.15;
/// The steps through the layer for one texel of the field.
const STEPS: usize = 16;

/// What a field is made from; a new one is made when this has moved on.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FieldKey {
    pub(crate) sun: Vec3,
    pub(crate) coverage: f32,
    pub(crate) bottom: f32,
    pub(crate) top: f32,
}

impl FieldKey {
    /// Whether a field made for `o` still serves: the sun's direction moves it on after about a
    /// degree (a few minutes of the day), and the threads that make a new one are two at most.
    pub(crate) fn near(&self, o: &FieldKey) -> bool {
        self.sun.dot(o.sun) > 0.999_850
            && (self.coverage - o.coverage).abs() < 0.015
            && (self.bottom - o.bottom).abs() < 40.0
            && (self.top - o.top).abs() < 80.0
    }
}

/// The cover the shadows are worked out with: the cloud density, less the billows' share.
pub(crate) fn shadow_coverage(cloud_density: f32) -> f32 {
    (0.3 + cloud_density.clamp(0.0, 1.0) * 0.55 - EROSION).clamp(0.0, 1.0)
}

/// The shape map (RGBA8, `clouds::SHAPE_SIZE / 2` a side) bilinearly at `uv`, repeating.
pub(crate) fn shape_sample(shape: &[u8], u: f32, v: f32) -> [f32; 3] {
    let size = (clouds::SHAPE_SIZE / 2) as usize;
    let texel = |x: i64, y: i64| {
        let (x, y) = (x.rem_euclid(size as i64) as usize, y.rem_euclid(size as i64) as usize);
        let i = (y * size + x) * 4;
        [shape[i] as f32 / 255.0, shape[i + 1] as f32 / 255.0, shape[i + 2] as f32 / 255.0]
    };
    let (x, y) = (u * size as f32 - 0.5, v * size as f32 - 0.5);
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (x0, y0) = (x0 as i64, y0 as i64);
    let a = texel(x0, y0);
    let b = texel(x0 + 1, y0);
    let c = texel(x0, y0 + 1);
    let d = texel(x0 + 1, y0 + 1);
    let mut o = [0.0f32; 3];
    for k in 0..3 {
        o[k] = (a[k] * (1.0 - fx) + b[k] * fx) * (1.0 - fy) + (c[k] * (1.0 - fx) + d[k] * fx) * fy;
    }
    o
}

/// The extinction (1/m) of the cumulus layer at height fraction `h` over the cloud-space
/// point `g` (m): the sky shader's heap without its billows.
pub(crate) fn layer_sigma(shape: &[u8], g: Vec2, h: f32, coverage: f32) -> f32 {
    if !(0.0..1.0).contains(&h) {
        return 0.0;
    }
    let lin = |a: f32, b: f32, v: f32| ((v - a) / (b - a)).clamp(0.0, 1.0);
    let m = shape_sample(shape, g.x / SHAPE_PERIOD, g.y / SHAPE_PERIOD);
    let lo = m[1] - 1.0;
    let n = h * h * (0.7 + m[2]) + (1.0 - h).powi(16);
    let base = (m[0] - n - lo) / (1.0 - lo) * (lin(0.0, 0.1, h) - lin(0.6, 1.0, h));
    let x = ((base + coverage - 1.0) / 0.12).clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x) * (h / 0.2).min(1.0) * SIGMA
}

/// How much of the sun gets through the layer to the point of its middle plane over the
/// cloud-space point `c`, along the sun's way.
pub(crate) fn column_transmittance(shape: &[u8], key: &FieldKey, c: Vec2) -> f32 {
    let s = key.sun;
    // (the way through the layer, bottom to top: its height fraction runs 0..1 evenly)
    let along = (key.top - key.bottom) / s.z;
    let ds = along / STEPS as f32;
    let mid = 0.5 * (key.top + key.bottom);
    let t_bottom = (key.bottom - mid) / s.z;
    let dir = Vec2::new(s.x, s.y);
    let mut od = 0.0;
    for k in 0..STEPS {
        let h = (k as f32 + 0.5) / STEPS as f32;
        let g = c + dir * (t_bottom + h * along);
        od += layer_sigma(shape, g, h, key.coverage) * ds;
    }
    (-od).exp()
}

/// The field: `FIELD_N` rows of `FIELD_N` shares of the sun, centred on the cloud-space
/// point `center`, row 0 the south edge.
pub(crate) fn compute(shape: &[u8], key: &FieldKey, center: Vec2) -> Vec<f32> {
    let n = FIELD_N as usize;
    let mut out = vec![1.0f32; n * n];
    if shape.is_empty() || key.sun.z <= 0.02 {
        return out;
    }
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).clamp(1, 2);
    let per = n.div_ceil(threads);
    std::thread::scope(|scope| {
        for (k, chunk) in out.chunks_mut(per * n).enumerate() {
            scope.spawn(move || {
                for (i, row) in chunk.chunks_mut(n).enumerate() {
                    let y = k * per + i;
                    for (x, v) in row.iter_mut().enumerate() {
                        let c = center + Vec2::new((x as f32 + 0.5) / n as f32 - 0.5, (y as f32 + 0.5) / n as f32 - 0.5) * FIELD_EXTENT;
                        *v = column_transmittance(shape, key, c);
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
    pub(crate) fn update_cloud_field(&mut self, lighting: &Lighting, cam_rel: Vec3, ro: DVec3, cloud_t: f32) -> FieldUniform {
        let s = lighting.sun_dir.normalize_or_zero();
        let off = omsi_cfg::flags::OMSI_NO_CLOUD_SHADOW_FIELD.is_set();
        if off || !self.options.cloud_shadows || !lighting.enhanced || lighting.cloud_density <= 0.0 || s.z < 0.06 || self.sky.cloud_shape_cpu.is_empty() {
            self.sky.cloud_field = FieldState::default();
            return ([0.0; 4], [0.0; 4]);
        }
        let [bottom, top] = super::enhanced::cloud_levels(lighting, cam_rel, ro);
        let mid = 0.5 * (bottom + top);
        let key = FieldKey { sun: s, coverage: shadow_coverage(lighting.cloud_density), bottom, top };
        let origin = Vec2::new(ro.x.rem_euclid(CLOUD_ORIGIN_PERIOD) as f32, ro.y.rem_euclid(CLOUD_ORIGIN_PERIOD) as f32);
        let drift = Vec2::new(lighting.cloud_offset[0], lighting.cloud_offset[1]) * 2500.0;
        // where the sun's way through the camera's own place meets the layer's middle, in
        // cloud space: what the field is to be centred on
        let here = Vec2::new(cam_rel.x, cam_rel.y) + Vec2::new(s.x, s.y) / s.z * (mid - cam_rel.z) + origin + drift;
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
            let shape = self.sky.cloud_shape_cpu.clone();
            if self.instant_exposure {
                // (a picture on its own waits for it; a window's first frame has the ground lit by the
                // camera's own number until the helper thread's field is in)
                let field = compute(&shape, &key, here);
                self.install_cloud_field(&field);
                self.sky.cloud_field.installed = Some((key, here));
            } else {
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::Builder::new()
                    .name("cloud shadows".into())
                    .spawn(move || {
                        let _ = tx.send(compute(&shape, &key, here));
                    })
                    .ok();
                self.sky.cloud_field.job = Some((key, here, rx));
            }
        }
        match &self.sky.cloud_field.installed {
            Some((_, c)) => ([c.x, c.y, 1.0 / FIELD_EXTENT, 1.0], [origin.x + drift.x, origin.y + drift.y, mid, cloud_t]),
            None => ([0.0; 4], [0.0; 4]),
        }
    }

    fn install_cloud_field(&mut self, field: &[f32]) {
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.sky.sky_lut,
                mip_level: 0,
                origin: wgpu::Origin3d { x: FIELD_X0, y: 0, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &texels(field),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(FIELD_N * 8), rows_per_image: Some(FIELD_N) },
            wgpu::Extent3d { width: FIELD_N, height: FIELD_N, depth_or_array_layers: 1 },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_field_is_shadow_under_a_heap_and_sun_between_heaps() {
        let shape = clouds::noise().shape[1].clone();
        let key = FieldKey { sun: Vec3::new(0.3, 0.2, 0.75).normalize(), coverage: shadow_coverage(0.5), bottom: 1400.0, top: 2800.0 };
        let field = compute(&shape, &key, Vec2::new(3000.0, 5000.0));
        let (lo, hi) = field.iter().fold((1.0f32, 0.0f32), |(l, h), v| (l.min(*v), h.max(*v)));
        assert!(lo < 0.25, "no shadow anywhere: darkest {lo}");
        assert!(hi > 0.95, "no sun anywhere: brightest {hi}");
        // a clear sky lets all of it through
        let clear = FieldKey { coverage: 0.0, ..key };
        assert!(compute(&shape, &clear, Vec2::ZERO).iter().all(|v| *v > 0.99));
    }

    #[test]
    fn the_field_agrees_with_the_one_point_it_replaces() {
        // the camera's own transmittance (five blurred rays) against the field's value
        // there: the same heaps, so about the same share of the sun
        let shape = clouds::noise().shape[1].clone();
        let lighting = Lighting { sun_dir: Vec3::new(0.2, 0.4, 0.8).normalize(), cloud_density: 0.5, enhanced: true, ..Lighting::default() };
        let mut sum = 0.0;
        let mut n = 0.0;
        for i in 0..12 {
            let cam = Vec3::new(i as f32 * 530.0, i as f32 * 310.0, 2.0);
            let one = super::super::enhanced::cloud_sun_transmittance(&shape, &lighting, cam, DVec3::ZERO);
            let key = FieldKey { sun: lighting.sun_dir, coverage: shadow_coverage(0.5), bottom: 1400.0, top: 2800.0 };
            let s = key.sun;
            let g = Vec2::new(cam.x, cam.y) + Vec2::new(s.x, s.y) / s.z * (2100.0 - cam.z);
            let here = column_transmittance(&shape, &key, g);
            sum += (one - here).abs();
            n += 1.0;
        }
        assert!(sum / n < 0.2, "the field and the camera's value differ by {} on average", sum / n);
    }
}
