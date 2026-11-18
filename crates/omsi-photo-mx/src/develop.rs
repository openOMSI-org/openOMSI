//! Developing a photo: the pictures of one shot added up in linear light (each one taken
//! through another point of the lens and at another moment of the shutter - that is what
//! makes the depth of field and the motion blur), then the darkroom: exposure, white
//! balance, tone, colour, a film look, vignetting, grain, the lens's colour fringes and
//! sharpening, into the 8-bit sRGB picture shown and saved.

use rayon::prelude::*;

/// The film looks of the photo mode (the "Filter" list).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Filter {
    #[default]
    None,
    Vivid,
    Warm,
    Cool,
    Vintage,
    Sepia,
    Mono,
    Noir,
    BleachBypass,
    TealOrange,
    Faded,
}

impl Filter {
    pub const ALL: [Filter; 11] = [Filter::None, Filter::Vivid, Filter::Warm, Filter::Cool, Filter::Vintage, Filter::Sepia, Filter::Mono, Filter::Noir, Filter::BleachBypass, Filter::TealOrange, Filter::Faded];

    pub fn name(self) -> &'static str {
        match self {
            Filter::None => "None",
            Filter::Vivid => "Vivid",
            Filter::Warm => "Warm",
            Filter::Cool => "Cool",
            Filter::Vintage => "Vintage",
            Filter::Sepia => "Sepia",
            Filter::Mono => "Black and white",
            Filter::Noir => "Noir",
            Filter::BleachBypass => "Bleach bypass",
            Filter::TealOrange => "Teal and orange",
            Filter::Faded => "Faded film",
        }
    }
}

/// The darkroom's settings. Every value is 0 when it leaves the picture as it is; the
/// signed ones run from -1 to 1, the others from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Grade {
    /// Stops of light (-3 .. 3).
    pub exposure: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub saturation: f32,
    pub vibrance: f32,
    /// Warmer (positive) or cooler.
    pub temperature: f32,
    /// Magenta (positive) or green.
    pub tint: f32,
    pub filter: Filter,
    pub filter_strength: f32,
    pub vignette: f32,
    pub grain: f32,
    pub aberration: f32,
    pub sharpen: f32,
    /// The camera's exposure (stops against the meter, the advanced mode's): the sensor
    /// takes that much more light and burns out at its white; the darkroom's exposure works
    /// on what it took.
    pub camera_ev: f32,
    /// The sensor's sensitivity (ISO; 0: no noise): its noise, the light's own grain and the
    /// sensor's floor, grows with it.
    pub iso: f32,
    /// The white balance in kelvin (0: as rendered).
    pub kelvin: f32,
    /// Local contrast (-1 .. 1).
    pub clarity: f32,
    /// Split toning: the hues (0 .. 1) laid into the shadows and the highlights, how much,
    /// and where the two meet (-1 .. 1).
    pub split_shadows: f32,
    pub split_highlights: f32,
    pub split_amount: f32,
    pub split_balance: f32,
    /// The lens as it comes: its distortion (k1, barrel below 0) and how much its corners
    /// darken (the light falling off over the frame, more wide open).
    pub distortion: f32,
    pub lens_vignette: f32,
    /// The diffraction's blur (a Gaussian's sigma in pixels).
    pub diffraction: f32,
    pub border: Border,
}

/// A border round the photo, inside its frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Border {
    #[default]
    None,
    White,
    Black,
    /// Wide at the bottom, as an instant print.
    Instant,
    /// A thin black line inside a wide white mat.
    Gallery,
}

impl Border {
    pub const ALL: [Border; 5] = [Border::None, Border::White, Border::Black, Border::Instant, Border::Gallery];

    pub fn name(self) -> &'static str {
        match self {
            Border::None => "None",
            Border::White => "White",
            Border::Black => "Black",
            Border::Instant => "Instant print",
            Border::Gallery => "Gallery mat",
        }
    }

    /// The colour of the frame at (x, y) inside a picture `w` x `h` (None: the photo).
    fn at(self, x: u32, y: u32, w: u32, h: u32) -> Option<[f32; 3]> {
        let s = w.min(h) as f32;
        let (x, y, w, h) = (x as f32, y as f32, w as f32, h as f32);
        let inside = |l: f32, t: f32, r: f32, b: f32| x >= l && x < w - r && y >= t && y < h - b;
        match self {
            Border::None => None,
            Border::White => (!inside(s * 0.03, s * 0.03, s * 0.03, s * 0.03)).then_some([1.0; 3]),
            Border::Black => (!inside(s * 0.03, s * 0.03, s * 0.03, s * 0.03)).then_some([0.0; 3]),
            Border::Instant => (!inside(s * 0.05, s * 0.05, s * 0.05, s * 0.2)).then_some([0.96, 0.95, 0.92]),
            Border::Gallery => {
                let m = s * 0.08;
                if !inside(m, m, m, m) {
                    Some([0.97; 3])
                } else if !inside(m + 2.0, m + 2.0, m + 2.0, m + 2.0) {
                    Some([0.05; 3])
                } else {
                    None
                }
            }
        }
    }
}

impl Grade {
    /// Settings that leave the picture as it was taken.
    pub fn neutral() -> Grade {
        Grade { filter_strength: 1.0, ..Default::default() }
    }
}

/// The pictures of one shot added up, in linear light.
pub struct Accum {
    pub w: u32,
    pub h: u32,
    sum: Vec<f32>,
    pub n: u32,
}

/// sRGB byte to linear light.
fn lut() -> &'static [f32; 256] {
    static LUT: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
    LUT.get_or_init(|| {
        let mut t = [0.0f32; 256];
        for (i, v) in t.iter_mut().enumerate() {
            let c = i as f32 / 255.0;
            *v = if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
        }
        t
    })
}

/// Linear light (0..1) to sRGB (0..1).
fn to_srgb(l: f32) -> f32 {
    let l = l.clamp(0.0, 1.0);
    if l <= 0.003_130_8 {
        l * 12.92
    } else {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    }
}

impl Accum {
    pub fn new(w: u32, h: u32) -> Accum {
        Accum { w, h, sum: vec![0.0; (w * h * 3) as usize], n: 0 }
    }

    /// One more picture of the shot, already in linear light (RGB, `w` x `h`).
    pub fn add_linear(&mut self, lin: &[f32]) {
        if lin.len() < self.sum.len() {
            return;
        }
        self.sum.par_iter_mut().zip(lin.par_iter()).for_each(|(s, l)| *s += *l);
        self.n += 1;
    }

    /// The mean picture so far, linear RGB.
    pub fn mean(&self) -> Vec<f32> {
        let inv = 1.0 / self.n.max(1) as f32;
        self.sum.par_iter().map(|s| s * inv).collect()
    }

    /// The mean picture's luminance (linear), for the meter.
    pub fn luma(&self) -> Vec<f32> {
        let inv = 1.0 / self.n.max(1) as f32;
        self.sum.par_chunks(3).map(|c| (0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]) * inv).collect()
    }

    /// A copy whose picture went through `f` (linear RGB mean in, out), one picture's worth.
    fn through(&self, f: impl FnOnce(Vec<f32>) -> Vec<f32>) -> Accum {
        let sum = f(self.mean());
        Accum { w: self.w, h: self.h, sum, n: 1 }
    }

    /// One more picture of the shot (RGBA, sRGB, `w` x `h`).
    pub fn add(&mut self, rgba: &[u8]) {
        if self.w == 0 || self.h == 0 {
            return;
        }
        let lut = lut();
        if rgba.len() < (self.w * self.h * 4) as usize {
            return;
        }
        self.sum.par_chunks_mut(self.w as usize * 3).zip(rgba.par_chunks(self.w as usize * 4)).for_each(|(s, px)| {
            for (o, p) in s.chunks_exact_mut(3).zip(px.chunks_exact(4)) {
                o[0] += lut[p[0] as usize];
                o[1] += lut[p[1] as usize];
                o[2] += lut[p[2] as usize];
            }
        });
        self.n += 1;
    }

    /// The mean of the pictures at pixel (x, y), linear RGB.
    fn at(&self, x: i64, y: i64) -> [f32; 3] {
        let x = x.clamp(0, self.w as i64 - 1) as usize;
        let y = y.clamp(0, self.h as i64 - 1) as usize;
        let k = (y * self.w as usize + x) * 3;
        let inv = 1.0 / self.n.max(1) as f32;
        [self.sum[k] * inv, self.sum[k + 1] * inv, self.sum[k + 2] * inv]
    }

    /// The mean between pixels (bilinear), at pixel coordinates `fx`, `fy`.
    fn sample(&self, fx: f32, fy: f32) -> [f32; 3] {
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - x0, fy - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let a = self.at(x0, y0);
        let b = self.at(x0 + 1, y0);
        let c = self.at(x0, y0 + 1);
        let d = self.at(x0 + 1, y0 + 1);
        let mut o = [0.0; 3];
        for i in 0..3 {
            let top = a[i] + (b[i] - a[i]) * tx;
            let bot = c[i] + (d[i] - c[i]) * tx;
            o[i] = top + (bot - top) * ty;
        }
        o
    }
}

/// The part of a `w` x `h` picture kept by the aspect ratio `aspect` (width / height; None:
/// the whole): x, y, width, height.
pub fn crop_rect(w: u32, h: u32, aspect: Option<f32>) -> (u32, u32, u32, u32) {
    let Some(a) = aspect.filter(|a| *a > 0.0) else { return (0, 0, w, h) };
    let (fw, fh) = (w as f32, h as f32);
    if fw / fh > a {
        let cw = (fh * a).round().clamp(1.0, fw) as u32;
        ((w - cw) / 2, 0, cw, h)
    } else {
        let ch = (fw / a).round().clamp(1.0, fh) as u32;
        (0, (h - ch) / 2, w, ch)
    }
}

fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The white balance's gains for a warmer/cooler and a magenta/green setting: a shift along
/// the daylight locus (blue-amber) and across it (green-magenta), the brightness kept.
fn white_balance(temperature: f32, tint: f32) -> [f32; 3] {
    let t = temperature * 0.30;
    let g = tint * 0.20;
    let m = [1.0 + t, 1.0 - g, 1.0 - t];
    let l = luma(m);
    [m[0] / l, m[1] / l, m[2] / l]
}

/// A film look, in display values (0..1).
fn film(c: [f32; 3], f: Filter) -> [f32; 3] {
    let l = luma(c);
    let sat = |c: [f32; 3], s: f32| mix3([luma(c); 3], c, s);
    let curve = |x: f32, k: f32| {
        // an S around the middle grey, k = how steep
        let s = x - 0.5;
        (0.5 + s * (1.0 + k) / (1.0 + k * 2.0 * s.abs())).clamp(0.0, 1.0)
    };
    match f {
        Filter::None => c,
        Filter::Vivid => {
            let c = sat(c, 1.35);
            [curve(c[0], 0.25), curve(c[1], 0.25), curve(c[2], 0.25)]
        }
        Filter::Warm => sat([c[0] * 1.07, c[1] * 1.01, c[2] * 0.86], 1.05),
        Filter::Cool => sat([c[0] * 0.90, c[1] * 0.99, c[2] * 1.10], 0.95),
        Filter::Vintage => {
            let c = sat(c, 0.72);
            [0.07 + c[0] * 0.90 * 1.04, 0.06 + c[1] * 0.88, 0.05 + c[2] * 0.80]
        }
        Filter::Sepia => [l * 1.07 + 0.03, l * 0.92 + 0.02, l * 0.72],
        Filter::Mono => {
            // (a black-and-white film sees red and green brighter than the eye's luma)
            let m = 0.30 * c[0] + 0.59 * c[1] + 0.11 * c[2];
            [m; 3]
        }
        Filter::Noir => {
            let m = curve(0.30 * c[0] + 0.59 * c[1] + 0.11 * c[2], 0.9);
            [m; 3]
        }
        Filter::BleachBypass => {
            // the silver left in: the luma laid over the colour, half the colour gone
            let over = |a: f32, b: f32| if a < 0.5 { 2.0 * a * b } else { 1.0 - 2.0 * (1.0 - a) * (1.0 - b) };
            let o = [over(l, c[0]), over(l, c[1]), over(l, c[2])];
            sat(mix3(c, o, 0.75), 0.55)
        }
        Filter::TealOrange => {
            // the shadows to teal, the light (skin, lamps, sunlit paint) to orange
            let w = smoothstep(0.15, 0.85, l);
            let shade = mix3(c, [c[0] * 0.80, c[1] * 1.02, c[2] * 1.12], 1.0 - w);
            let lit = mix3(shade, [shade[0] * 1.12, shade[1] * 1.00, shade[2] * 0.84], w);
            sat(lit, 1.1)
        }
        Filter::Faded => {
            let c = sat(c, 0.82);
            [0.08 + c[0] * 0.84, 0.08 + c[1] * 0.84, 0.10 + c[2] * 0.82]
        }
    }
}

/// A grain value at a pixel: noise from -0.5 to 0.5, the same picture after picture.
fn grain_at(x: u32, y: u32) -> f32 {
    let mut h = x.wrapping_mul(0x8da6_b343) ^ y.wrapping_mul(0xd816_3841) ^ 0x9e37_79b9;
    h ^= h >> 13;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 16;
    (h & 0xffff) as f32 / 65535.0 - 0.5
}

/// The developed picture (RGBA, sRGB, `acc.w` x `acc.h`); outside `crop` (x, y, w, h) the
/// bars of the frame.
pub fn develop(acc: &Accum, g: &Grade, crop: Option<(u32, u32, u32, u32)>) -> Vec<u8> {
    let mut out = vec![0u8; acc.w as usize * acc.h as usize * 4];
    develop_into(acc, g, crop, &mut out, |v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8, 255);
    out
}

/// The developed picture with 16 bits a channel (RGBA), for a photo to be worked on further.
pub fn develop16(acc: &Accum, g: &Grade, crop: Option<(u32, u32, u32, u32)>) -> Vec<u16> {
    let mut out = vec![0u16; acc.w as usize * acc.h as usize * 4];
    develop_into(acc, g, crop, &mut out, |v| (v.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16, 65535);
    out
}

/// A normal-distributed number (about) from the pixel and the channel: the same picture
/// after picture.
fn noise_at(x: u32, y: u32, c: u32) -> f32 {
    let a = grain_at(x.wrapping_mul(3).wrapping_add(c * 7919), y ^ (c * 104_729));
    let b = grain_at(y.wrapping_add(c * 31), x.wrapping_mul(5) ^ 0x5bd1);
    let d = grain_at(x ^ 0x2545_f491, y.wrapping_mul(7).wrapping_add(c));
    // (three uniforms: near enough a bell, sigma 1)
    (a + b + d) * 2.0
}

/// A hue (0 .. 1) as a colour of the luma of a middle grey.
fn hue_rgb(h: f32) -> [f32; 3] {
    let f = |n: f32| {
        let k = (n + h * 6.0) % 6.0;
        1.0 - (k.min(4.0 - k).clamp(0.0, 1.0))
    };
    [f(5.0), f(3.0), f(1.0)]
}

fn develop_into<T: Copy + Send + Sync>(acc: &Accum, g: &Grade, crop: Option<(u32, u32, u32, u32)>, out: &mut [T], conv: impl Fn(f32) -> T + Sync, one: T) {
    let (w, h) = (acc.w as usize, acc.h as usize);
    if acc.n == 0 || w == 0 || h == 0 {
        return;
    }
    // the lens's diffraction, before anything else (it is in the light the sensor takes)
    let diffracted;
    let acc = if g.diffraction > 0.3 {
        diffracted = acc.through(|mut lin| {
            let r = vec![g.diffraction * 1.6; w * h];
            crate::optics::soften(&mut lin, w as u32, h as u32, &r);
            lin
        });
        &diffracted
    } else {
        acc
    };
    // the local contrast: the brightness round each pixel (some 2 per cent of the frame)
    let local: Option<Vec<f32>> = (g.clarity != 0.0).then(|| {
        let mut l: Vec<f32> = acc.luma().iter().flat_map(|v| [*v, *v, *v]).collect();
        let r = vec![h as f32 * 0.025; w * h];
        crate::optics::soften(&mut l, w as u32, h as u32, &r);
        crate::optics::soften(&mut l, w as u32, h as u32, &r);
        l.chunks(3).map(|c| c[0]).collect()
    });
    let sensor = 2f32.powf(g.camera_ev);
    let clip = g.camera_ev != 0.0 || g.iso > 0.0;
    let gain = 2f32.powf(g.exposure);
    let wb = {
        let a = white_balance(g.temperature, g.tint);
        let k = crate::exposure::kelvin_gains(g.kelvin);
        [a[0] * k[0], a[1] * k[1], a[2] * k[2]]
    };
    // the sensor's noise: the light's own grain (its variance grows with the light) and the
    // read-out's floor, both raised with the sensitivity
    let shot_k = 5e-5 * g.iso / 100.0;
    let read = 2.5e-4 * (g.iso / 100.0).powf(0.8);
    let (split_s, split_h) = (hue_rgb(g.split_shadows), hue_rgb(g.split_highlights));
    let (cx, cy) = (w as f32 * 0.5, h as f32 * 0.5);
    let diag = (cx * cx + cy * cy).sqrt().max(1.0);
    let ab = g.aberration * 0.006;
    let (kx, ky, kw, kh) = crop.unwrap_or((0, 0, w as u32, h as u32));
    let zero = conv(0.0);
    out.par_chunks_mut(w * 4).enumerate().for_each(|(y, row)| {
        for x in 0..w {
            let px = &mut row[x * 4..x * 4 + 4];
            px[3] = one;
            if (x as u32) < kx || (x as u32) >= kx + kw || (y as u32) < ky || (y as u32) >= ky + kh {
                px[0] = zero;
                px[1] = zero;
                px[2] = zero;
                continue;
            }
            if let Some(b) = g.border.at(x as u32 - kx, y as u32 - ky, kw, kh) {
                px[0] = conv(b[0]);
                px[1] = conv(b[1]);
                px[2] = conv(b[2]);
                continue;
            }
            let (dx, dy) = (x as f32 - cx, y as f32 - cy);
            let r = (dx * dx + dy * dy).sqrt() / diag;
            // the lens's distortion: where this pixel's light came from
            let s = 1.0 + g.distortion * r * r;
            // the lens: red drawn out a little, blue drawn in, towards the corners
            let mut c = if ab > 0.0 || g.distortion != 0.0 {
                let k = ab * r * r;
                let red = acc.sample(cx + dx * s * (1.0 + k), cy + dy * s * (1.0 + k))[0];
                let blue = acc.sample(cx + dx * s * (1.0 - k), cy + dy * s * (1.0 - k))[2];
                let mid = acc.sample(cx + dx * s, cy + dy * s);
                [red, mid[1], blue]
            } else {
                acc.at(x as i64, y as i64)
            };
            // the light falling off towards the corners
            if g.lens_vignette > 0.0 {
                let v = 1.0 - g.lens_vignette * smoothstep(0.15, 1.1, r).powf(1.4);
                c = [c[0] * v, c[1] * v, c[2] * v];
            }
            if let Some(local) = local.as_ref() {
                let l = luma(c).max(1e-5);
                let around = local[y * w + x].max(1e-5);
                let k = (l / around).powf(g.clarity * 0.55);
                c = [c[0] * k, c[1] * k, c[2] * k];
            }
            // the sensor: the camera's exposure, burnt out at its white, and its noise
            if clip {
                for v in c.iter_mut() {
                    *v = (*v * sensor).min(1.0);
                }
                if g.iso > 0.0 {
                    let l = luma(c).max(0.0);
                    let sd = (l * shot_k + read * read).sqrt();
                    let lum = noise_at(x as u32, y as u32, 3) * sd * 0.7;
                    for (i, v) in c.iter_mut().enumerate() {
                        *v = (*v + lum + noise_at(x as u32, y as u32, i as u32) * sd * 0.55).max(0.0);
                    }
                }
            }
            if g.sharpen > 0.0 {
                let (xi, yi) = (x as i64, y as i64);
                let n = [acc.at(xi - 1, yi), acc.at(xi + 1, yi), acc.at(xi, yi - 1), acc.at(xi, yi + 1)];
                for i in 0..3 {
                    let avg = (n[0][i] + n[1][i] + n[2][i] + n[3][i]) * 0.25;
                    c[i] = (c[i] + (c[i] - avg) * g.sharpen * 1.5).max(0.0);
                }
            }
            // exposure and white balance in linear light; pushed over the white, a soft
            // shoulder rolls it off instead of cutting it (a picture left as it was keeps its
            // values)
            for i in 0..3 {
                let v = c[i] * gain * wb[i];
                c[i] = if v <= 0.8 || gain * wb[i] <= 1.0 { v } else { 0.8 + 0.2 * (1.0 - (-(v - 0.8) / 0.2).exp()) / (1.0 - (-(gain * wb[i] - 0.8) / 0.2).exp()) };
            }
            let mut d = [to_srgb(c[0]), to_srgb(c[1]), to_srgb(c[2])];
            // shadows and highlights: the brightness moved where it is dark or light
            let l = luma(d);
            if g.shadows != 0.0 || g.highlights != 0.0 {
                let lift = g.shadows * 0.35 * (1.0 - l) * (1.0 - l) * (1.0 - smoothstep(0.0, 0.6, l) * 0.5);
                let roll = g.highlights * 0.30 * l * l;
                let to = (l + lift + roll).clamp(0.0, 1.0);
                let k = if l > 1e-4 { to / l } else { 1.0 };
                d = [d[0] * k, d[1] * k, d[2] * k];
            }
            // contrast round the middle grey
            if g.contrast != 0.0 {
                let k = g.contrast;
                for v in d.iter_mut() {
                    let s = *v - 0.5;
                    *v = if k > 0.0 { 0.5 + s * (1.0 + k) / (1.0 + k * 2.0 * s.abs()) } else { 0.5 + s * (1.0 + k * 0.7) };
                }
            }
            // saturation, and vibrance (more where the colour is weak)
            if g.saturation != 0.0 || g.vibrance != 0.0 {
                let l = luma(d);
                let mx = d[0].max(d[1]).max(d[2]);
                let mn = d[0].min(d[1]).min(d[2]);
                let s_now = if mx > 1e-4 { (mx - mn) / mx } else { 0.0 };
                let s = 1.0 + g.saturation + g.vibrance * (1.0 - s_now) * 0.8;
                d = mix3([l; 3], d, s.max(0.0));
            }
            if g.filter != Filter::None && g.filter_strength > 0.0 {
                d = mix3(d, film(d, g.filter), g.filter_strength);
            }
            if g.split_amount > 0.0 {
                let l = luma(d).clamp(0.0, 1.0);
                let mid = 0.5 + g.split_balance * 0.3;
                let ws = 1.0 - smoothstep(0.0, mid, l);
                let wh = smoothstep(mid, 1.0, l);
                for i in 0..3 {
                    d[i] += ((split_s[i] - luma(split_s)) * ws + (split_h[i] - luma(split_h)) * wh) * g.split_amount * 0.22;
                }
            }
            if g.vignette > 0.0 {
                let v = 1.0 - g.vignette * 0.85 * smoothstep(0.30, 1.05, r);
                d = [d[0] * v, d[1] * v, d[2] * v];
            }
            if g.grain > 0.0 {
                let l = luma(d).clamp(0.0, 1.0);
                // (strongest in the mid-tones, as film's)
                let n = grain_at(x as u32, y as u32) * g.grain * 0.16 * (1.0 - (2.0 * l - 1.0).abs() * 0.6);
                d = [d[0] + n, d[1] + n, d[2] + n];
            }
            px[0] = conv(d[0]);
            px[1] = conv(d[1]);
            px[2] = conv(d[2]);
        }
    });
}

/// The developed picture's histogram in `crop` (x, y, w, h): 64 bins of red, green, blue
/// and the luma.
pub fn histogram(rgba: &[u8], w: u32, crop: (u32, u32, u32, u32)) -> [[u32; 64]; 4] {
    let mut hist = [[0u32; 64]; 4];
    let (x0, y0, cw, ch) = crop;
    for y in (y0..y0 + ch).step_by(2) {
        for x in (x0..x0 + cw).step_by(2) {
            let k = ((y * w + x) * 4) as usize;
            let Some(p) = rgba.get(k..k + 3) else { continue };
            for c in 0..3 {
                hist[c][(p[c] >> 2) as usize] += 1;
            }
            let l = (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) as u8;
            hist[3][(l >> 2) as usize] += 1;
        }
    }
    hist
}

/// The live view's aids over a developed picture (not saved): stripes where it burns out
/// (red) or goes black (blue), and the edges in focus lit (`focus`: per pixel, the blur
/// circle's radius in pixels).
pub fn overlays(rgba: &mut [u8], w: u32, h: u32, zebra: bool, focus: Option<&[f32]>) {
    if w == 0 || h == 0 {
        return;
    }
    let (wu, hu) = (w as usize, h as usize);
    let src = focus.map(|_| rgba.to_vec());
    rgba.par_chunks_mut(wu * 4).enumerate().for_each(|(y, row)| {
        for x in 0..wu {
            let p = &mut row[x * 4..x * 4 + 4];
            if zebra && ((x + y) / 5) % 2 == 0 {
                if p[0].max(p[1]).max(p[2]) >= 252 {
                    p[0] = 255;
                    p[1] = 30;
                    p[2] = 60;
                    continue;
                }
                if p[0].max(p[1]).max(p[2]) <= 2 {
                    p[0] = 40;
                    p[1] = 90;
                    p[2] = 255;
                    continue;
                }
            }
            if let (Some(f), Some(s)) = (focus, src.as_ref()) {
                if x == 0 || y == 0 || x + 1 >= wu || y + 1 >= hu || f.get(y * wu + x).is_none_or(|r| *r > 0.7) {
                    continue;
                }
                let l = |xx: usize, yy: usize| {
                    let k = (yy * wu + xx) * 4;
                    0.3 * s[k] as f32 + 0.59 * s[k + 1] as f32 + 0.11 * s[k + 2] as f32
                };
                let gx = l(x + 1, y) - l(x - 1, y);
                let gy = l(x, y + 1) - l(x, y - 1);
                if gx * gx + gy * gy > 40.0 * 40.0 {
                    p[0] = 255;
                    p[1] = 40;
                    p[2] = 40;
                }
            }
        }
    });
}

/// The developed picture cut to its frame (RGBA).
pub fn cropped(rgba: &[u8], w: u32, crop: (u32, u32, u32, u32)) -> Vec<u8> {
    let (x, y, cw, ch) = crop;
    let mut out = Vec::with_capacity((cw * ch * 4) as usize);
    for row in y..y + ch {
        let a = ((row * w + x) * 4) as usize;
        out.extend_from_slice(&rgba[a..a + (cw * 4) as usize]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_sized_photos_never_split_into_zero_sized_chunks() {
        let mut zero_width = Accum::new(0, 2);
        zero_width.add(&[]);
        assert_eq!(zero_width.n, 0);
        let mut zero_height = Accum::new(2, 0);
        zero_height.add(&[]);
        assert_eq!(zero_height.n, 0);
        overlays(&mut [], 0, 2, true, None);
        overlays(&mut [], 2, 0, true, None);
    }

    fn flat(w: u32, h: u32, c: [u8; 3]) -> Vec<u8> {
        (0..w * h).flat_map(|_| [c[0], c[1], c[2], 255]).collect()
    }

    #[test]
    fn a_neutral_development_gives_the_picture_back() {
        let mut a = Accum::new(4, 3);
        let px: Vec<u8> = (0..12u32).flat_map(|i| [(i * 20) as u8, (255 - i * 20) as u8, 128, 255]).collect();
        a.add(&px);
        let out = develop(&a, &Grade::neutral(), None);
        for (o, p) in out.chunks(4).zip(px.chunks(4)) {
            for i in 0..3 {
                assert!((o[i] as i32 - p[i] as i32).abs() <= 1, "{o:?} vs {p:?}");
            }
        }
    }

    #[test]
    fn the_pictures_of_a_shot_average_in_linear_light() {
        // black and white averaged make the linear half: sRGB 188, not 128
        let mut a = Accum::new(2, 2);
        a.add(&flat(2, 2, [0, 0, 0]));
        a.add(&flat(2, 2, [255, 255, 255]));
        let out = develop(&a, &Grade::neutral(), None);
        assert!((out[0] as i32 - 188).abs() <= 1, "{}", out[0]);
    }

    #[test]
    fn a_stop_more_doubles_the_light() {
        let mut a = Accum::new(1, 1);
        a.add(&flat(1, 1, [100, 100, 100]));
        let base = lut()[100];
        let out = develop(&a, &Grade { exposure: 1.0, ..Grade::neutral() }, None);
        let back = lut()[out[0] as usize];
        assert!((back - base * 2.0).abs() < 0.01, "{back} vs {}", base * 2.0);
    }

    #[test]
    fn black_and_white_has_no_colour_left() {
        let mut a = Accum::new(1, 1);
        a.add(&flat(1, 1, [200, 60, 30]));
        for f in [Filter::Mono, Filter::Noir] {
            let out = develop(&a, &Grade { filter: f, ..Grade::neutral() }, None);
            assert!(out[0] == out[1] && out[1] == out[2], "{f:?}: {:?}", &out[..3]);
        }
    }

    #[test]
    fn the_vignette_darkens_the_corners_and_leaves_the_middle() {
        let mut a = Accum::new(64, 40);
        a.add(&flat(64, 40, [180, 180, 180]));
        let out = develop(&a, &Grade { vignette: 1.0, ..Grade::neutral() }, None);
        let at = |x: usize, y: usize| out[(y * 64 + x) * 4];
        assert!((at(32, 20) as i32 - 180).abs() <= 1);
        assert!(at(0, 0) < 90, "{}", at(0, 0));
    }

    #[test]
    fn warmer_is_redder_and_less_blue() {
        let mut a = Accum::new(1, 1);
        a.add(&flat(1, 1, [128, 128, 128]));
        let out = develop(&a, &Grade { temperature: 1.0, ..Grade::neutral() }, None);
        assert!(out[0] > 128 && out[2] < 128, "{:?}", &out[..3]);
    }

    #[test]
    fn a_frame_crops_to_its_aspect_ratio() {
        assert_eq!(crop_rect(1920, 1080, None), (0, 0, 1920, 1080));
        assert_eq!(crop_rect(1920, 1080, Some(1.0)), (420, 0, 1080, 1080));
        let (x, y, w, h) = crop_rect(1920, 1080, Some(21.0 / 9.0));
        assert_eq!((x, w), (0, 1920));
        assert_eq!(h, 823);
        assert_eq!(y, (1080 - 823) / 2);
        let px = develop(&{ let mut a = Accum::new(4, 2); a.add(&flat(4, 2, [255, 255, 255])); a }, &Grade::neutral(), Some((1, 0, 2, 2)));
        assert_eq!(px[0], 0, "outside the frame: the bar");
        assert_eq!(px[4], 255);
        assert_eq!(cropped(&px, 4, (1, 0, 2, 2)).len(), 2 * 2 * 4);
    }

    #[test]
    fn a_sensor_burns_out_and_a_high_iso_is_noisy() {
        let mut a = Accum::new(32, 32);
        a.add(&flat(32, 32, [150, 150, 150]));
        // two stops over: white, and no darkroom brings it back
        let over = develop(&a, &Grade { camera_ev: 2.0, exposure: -2.0, ..Grade::neutral() }, None);
        assert!(over[0] < 200 && over[0] > 120, "{}", over[0]);
        let spread = |px: &[u8]| {
            let v: Vec<f32> = px.chunks(4).map(|p| p[1] as f32).collect();
            let m = v.iter().sum::<f32>() / v.len() as f32;
            (v.iter().map(|x| (x - m) * (x - m)).sum::<f32>() / v.len() as f32).sqrt()
        };
        let low = develop(&a, &Grade { iso: 100.0, ..Grade::neutral() }, None);
        let high = develop(&a, &Grade { iso: 12800.0, ..Grade::neutral() }, None);
        assert!(spread(&high) > 4.0 * spread(&low).max(0.5), "{} {}", spread(&low), spread(&high));
    }

    #[test]
    fn a_border_frames_the_photo() {
        let mut a = Accum::new(100, 80);
        a.add(&flat(100, 80, [20, 20, 20]));
        let px = develop(&a, &Grade { border: Border::White, ..Grade::neutral() }, None);
        assert_eq!(px[0], 255);
        assert!(px[((40 * 100 + 50) * 4) as usize] < 30);
        let px16 = develop16(&a, &Grade::neutral(), None);
        assert!((px16[0] as i32 / 257 - 20).abs() <= 1);
    }

    #[test]
    fn the_histogram_counts_the_frame() {
        let px = flat(8, 8, [255, 0, 0]);
        let h = histogram(&px, 8, (0, 0, 8, 8));
        assert_eq!(h[0][63], 16);
        assert_eq!(h[1][0], 16);
    }
}
