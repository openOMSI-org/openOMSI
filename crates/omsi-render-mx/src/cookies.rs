//! Light cookies (Enhanced): a lamp whose light is a picture - `[spotlight_cookie]` in a
//! model.cfg (see docs/FORMATS.md). The picture is the lamp's beam as it would show on a
//! wall square to it: how strong and in what colour the lamp shines every way within the
//! field the picture covers. The shaders look a lit point's direction from the lamp up in
//! it (`cookie_light` in enhanced.wgsl) in place of a plain cone's falloff.
//!
//! The pictures of the lamps lit in a frame are layers, at most `COOKIE_LAYERS` at once (a
//! layer is a picture and the field it covers); a lamp that finds no layer that frame is lit
//! by its plain cone, as the classic picture lights every one. The layers ride at the end of
//! the light buffer, four texels to a light's record: the scene's fragment shader reads
//! sixteen textures already (WebGPU's guaranteed number), and a buffer of their own in the
//! lamps' loop brought Apple's shader compiler down (`XPC_ERROR_CONNECTION_INTERRUPTED`,
//! the scene pipelines lost), where one more read of the light buffer does not.

use glam::Vec3;
use std::cell::RefCell;
use std::collections::HashMap;

/// How many pictures the lamps of a frame can show at once.
pub const COOKIE_LAYERS: usize = 8;
/// A layer's size (texels a side; enhanced.wgsl's `COOKIE_SIZE`): a beam pattern is soft,
/// and its picture is taken down to this with its light averaged.
pub const COOKIE_SIZE: u32 = 32;

/// A lamp's picture as the renderer knows it (`PointLight::cookie`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cookie {
    /// The picture, as registered with `Renderer::set_cookie_picture`.
    pub picture: u16,
    /// The tangents of half the angles the picture covers across and up and down: a point
    /// at the picture's right edge lies `tan[0]` across for every metre ahead.
    pub tan: [f32; 2],
}

impl Cookie {
    /// The cone round the picture (the cosine of its half-diagonal), by which the light's
    /// reach is judged and the fog's light is drawn.
    pub fn bounding_cos(&self) -> f32 {
        let t = (self.tan[0] * self.tan[0] + self.tan[1] * self.tan[1]).sqrt();
        1.0 / (1.0 + t * t).sqrt()
    }
}

/// The layer a picture with its field holds, and the frame it was last used in.
#[derive(Clone, Copy)]
struct Layer {
    picture: u16,
    tan: [f32; 2],
    used: u64,
}

/// A layer's records in the light buffer (four texels each).
pub(crate) const LAYER_RECORDS: usize = (COOKIE_SIZE * COOKIE_SIZE / 4) as usize;

pub(crate) struct CookieAtlas {
    /// The registered pictures, taken down to `COOKIE_SIZE` (linear RGB, row by row).
    pictures: RefCell<HashMap<u16, Vec<[f32; 3]>>>,
    layers: RefCell<[Option<Layer>; COOKIE_LAYERS]>,
    frame: std::cell::Cell<u64>,
    /// Where the layers start in this frame's light buffer (records), when they are in it.
    base: std::cell::Cell<Option<u32>>,
}

impl CookieAtlas {
    pub(crate) fn new() -> CookieAtlas {
        CookieAtlas { pictures: RefCell::new(HashMap::new()), layers: RefCell::new([None; COOKIE_LAYERS]), frame: std::cell::Cell::new(0), base: std::cell::Cell::new(None) }
    }

    /// Keep a picture for the lamps that name it; a layer showing an older one under the
    /// same number takes the new one.
    pub(crate) fn set_picture(&self, id: u16, img: &omsi_texture::Image) {
        self.pictures.borrow_mut().insert(id, resample(img));
    }

    /// The next frame's layers are handed out from here on.
    pub(crate) fn begin_frame(&self) {
        self.frame.set(self.frame.get() + 1);
        self.base.set(None);
    }

    /// The layer of a lamp's picture this frame: one that holds it already, else the one
    /// unused longest - none when every layer is taken by this frame's lamps, or the
    /// picture is not there.
    pub(crate) fn layer(&self, c: &Cookie) -> Option<u32> {
        let frame = self.frame.get();
        let mut layers = self.layers.borrow_mut();
        if let Some(k) = layers.iter().position(|l| l.is_some_and(|l| l.picture == c.picture && l.tan == c.tan)) {
            if let Some(l) = layers[k].as_mut() {
                l.used = frame;
            }
            return Some(k as u32);
        }
        if !self.pictures.borrow().contains_key(&c.picture) {
            return None;
        }
        let k = (0..COOKIE_LAYERS).filter(|&k| layers[k].is_none_or(|l| l.used != frame)).min_by_key(|&k| layers[k].map_or(0, |l| l.used + 1))?;
        layers[k] = Some(Layer { picture: c.picture, tan: c.tan, used: frame });
        Some(k as u32)
    }

    /// The layers this frame's lamps use, appended to the light buffer's records (`out`, four
    /// linear RGB texels a record), when any lamp uses one.
    pub(crate) fn append(&self, out: &mut Vec<super::GpuPointLight>) {
        let frame = self.frame.get();
        let layers = self.layers.borrow();
        if !layers.iter().any(|l| l.is_some_and(|l| l.used == frame)) {
            return;
        }
        let base = out.len() as u32;
        let pictures = self.pictures.borrow();
        for layer in layers.iter() {
            let texels = layer.filter(|l| l.used == frame).and_then(|l| pictures.get(&l.picture));
            for r in 0..LAYER_RECORDS {
                let t = |i: usize| texels.and_then(|t| t.get(r * 4 + i)).map_or([0.0; 4], |c| [c[0], c[1], c[2], 0.0]);
                out.push(super::GpuPointLight { pos: t(0), color: t(1), dir: t(2), extra: t(3) });
            }
        }
        self.base.set(Some(base));
    }

    /// The field every layer covers and where it lies (the camera uniform's `cookies`: x, y
    /// the tangents, z its first record in the light buffer).
    pub(crate) fn fields(&self) -> [[f32; 4]; COOKIE_LAYERS] {
        let layers = self.layers.borrow();
        let base = self.base.get().unwrap_or(0);
        std::array::from_fn(|k| {
            let at = (base as usize + k * LAYER_RECORDS) as f32;
            layers[k].map_or([1.0, 1.0, at, 0.0], |l| [l.tan[0].max(1e-3), l.tan[1].max(1e-3), at, 0.0])
        })
    }
}

/// A picture taken down (or up) to `COOKIE_SIZE` a side by averaging the texels each layer
/// texel covers, in linear light (the beam's light adds up); alpha left out.
fn resample(img: &omsi_texture::Image) -> Vec<[f32; 3]> {
    let n = COOKIE_SIZE as usize;
    let mut out = vec![[0.0; 3]; n * n];
    let (w, h) = (img.width.max(1) as usize, img.height.max(1) as usize);
    if img.rgba.len() < w * h * 4 {
        return out;
    }
    let lin = |c: u8| {
        let c = c as f32 / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    for y in 0..n {
        let (y0, y1) = (y * h / n, ((y + 1) * h / n).max(y * h / n + 1).min(h));
        for x in 0..n {
            let (x0, x1) = (x * w / n, ((x + 1) * w / n).max(x * w / n + 1).min(w));
            let mut sum = Vec3::ZERO;
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let i = (sy * w + sx) * 4;
                    sum += Vec3::new(lin(img.rgba[i]), lin(img.rgba[i + 1]), lin(img.rgba[i + 2]));
                }
            }
            out[y * n + x] = (sum / ((y1 - y0) * (x1 - x0)).max(1) as f32).to_array();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A picture's average light is kept when it is taken down to a layer.
    #[test]
    fn a_picture_keeps_its_light_when_taken_down() {
        let (w, h) = (256u32, 256u32);
        let mut rgba = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let v = if (x + y) % 2 == 0 { 255 } else { 0 };
                rgba.extend([v, v, v, 255]);
            }
        }
        let out = resample(&omsi_texture::Image { width: w, height: h, rgba, has_alpha: false });
        assert!(out.iter().all(|p| (p[0] - 0.5).abs() < 1e-3), "{:?}", &out[..2]);
    }

    /// The cone round a picture of 40 x 20 degrees reaches its corners.
    #[test]
    fn the_bounding_cone_holds_the_corners() {
        let c = Cookie { picture: 1, tan: [20f32.to_radians().tan(), 10f32.to_radians().tan()] };
        let corner = Vec3::new(c.tan[0], c.tan[1], 1.0).normalize();
        assert!((corner.z - c.bounding_cos()).abs() < 1e-5);
    }

    /// A frame's layers follow the lights in the buffer, and the fields say where; a frame
    /// without a picture lit adds nothing.
    #[test]
    fn the_layers_ride_after_the_lights() {
        let atlas = CookieAtlas::new();
        let img = omsi_texture::Image { width: 2, height: 2, rgba: vec![255; 16], has_alpha: false };
        atlas.set_picture(3, &img);
        atlas.begin_frame();
        let mut out = vec![super::super::GpuPointLight { pos: [0.0; 4], color: [0.0; 4], dir: [0.0; 4], extra: [0.0; 4] }; 5];
        atlas.append(&mut out);
        assert_eq!(out.len(), 5);
        let c = Cookie { picture: 3, tan: [0.5, 0.2] };
        assert_eq!(atlas.layer(&c), Some(0));
        assert_eq!(atlas.layer(&Cookie { picture: 9, tan: [0.5, 0.2] }), None);
        atlas.append(&mut out);
        assert_eq!(out.len(), 5 + COOKIE_LAYERS * LAYER_RECORDS);
        assert_eq!(out[5].pos[0], 1.0);
        let f = atlas.fields();
        assert_eq!((f[0][0], f[0][2], f[1][2]), (0.5, 5.0, (5 + LAYER_RECORDS) as f32));
    }
}
