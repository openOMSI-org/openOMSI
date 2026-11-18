//! What the lens does to each picture of a photo before the pictures are added up: the
//! pictures taken through the points of the aperture show what is out of focus as so many
//! copies side by side, and each is softened by as much as lies between two of them - the
//! blur circle of the point at that pixel (from the picture's depth) over the root of the
//! number of pictures - so that the copies run together into one smooth blur whatever the
//! count. A tilted lens's plane of focus is laid by moving each picture over the frame.

use rayon::prelude::*;

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

/// A picture (RGBA, sRGB) in linear light (RGB): what is burnt out in it (its brightest
/// value near the white) taken as up to `1 + 8 * highlights` times brighter - a street
/// lamp is far brighter than the white it was cut to, and out of focus its light spreads
/// into a bright disc rather than a grey smear.
pub fn linear(rgba: &[u8], w: u32, h: u32, highlights: f32) -> Vec<f32> {
    let lut = lut();
    let n = (w * h) as usize;
    let mut out = vec![0.0f32; n * 3];
    out.par_chunks_mut(w as usize * 3).zip(rgba.par_chunks(w as usize * 4)).for_each(|(o, px)| {
        for (o, p) in o.chunks_exact_mut(3).zip(px.chunks_exact(4)) {
            let c = [lut[p[0] as usize], lut[p[1] as usize], lut[p[2] as usize]];
            let k = if highlights > 0.0 {
                let m = c[0].max(c[1]).max(c[2]);
                let over = ((m - 0.85) / 0.15).clamp(0.0, 1.0);
                1.0 + highlights * 8.0 * over * over
            } else {
                1.0
            };
            o[0] = c[0] * k;
            o[1] = c[1] * k;
            o[2] = c[2] * k;
        }
    });
    out
}

/// Each row's pixels the mean of the row round them, `radius[x]` pixels to either side
/// (between two whole widths for a fraction); in place, RGB.
fn rows(lin: &mut [f32], w: usize, radius: &[f32]) {
    lin.par_chunks_mut(w * 3).zip(radius.par_chunks(w)).for_each(|(row, rad)| {
        if rad.iter().all(|r| *r < 0.02) {
            return;
        }
        let mut pre = vec![0.0f64; (w + 1) * 3];
        for x in 0..w {
            for c in 0..3 {
                pre[(x + 1) * 3 + c] = pre[x * 3 + c] + row[x * 3 + c] as f64;
            }
        }
        let boxed = |x: usize, r: usize, c: usize| {
            let (a, b) = (x.saturating_sub(r), (x + r + 1).min(w));
            ((pre[b * 3 + c] - pre[a * 3 + c]) / (b - a) as f64) as f32
        };
        for x in 0..w {
            let r = rad[x].clamp(0.0, 96.0);
            if r < 0.02 {
                continue;
            }
            let r0 = r.floor() as usize;
            let t = r - r0 as f32;
            for c in 0..3 {
                row[x * 3 + c] = boxed(x, r0, c) * (1.0 - t) + boxed(x, r0 + 1, c) * t;
            }
        }
    });
}

/// `src` (`w` x `h`, `k` values a pixel) with rows and columns swapped.
fn transpose(src: &[f32], w: usize, h: usize, k: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; src.len()];
    out.par_chunks_mut(h * k).enumerate().for_each(|(x, col)| {
        for y in 0..h {
            col[y * k..y * k + k].copy_from_slice(&src[(y * w + x) * k..(y * w + x) * k + k]);
        }
    });
    out
}

/// A picture in linear light softened pixel by pixel: a box `radius[i]` pixels to either
/// side, along the rows and then the columns.
pub fn soften(lin: &mut Vec<f32>, w: u32, h: u32, radius: &[f32]) {
    let (w, h) = (w as usize, h as usize);
    if radius.len() < w * h || radius.iter().all(|r| *r < 0.02) {
        return;
    }
    rows(lin, w, radius);
    let mut t = transpose(lin, w, h, 3);
    let rt = transpose(radius, w, h, 1);
    rows(&mut t, h, &rt);
    *lin = transpose(&t, h, w, 3);
}

/// A picture (RGBA) and its depth moved over the frame: each pixel takes what lies
/// `by(x, y)` pixels before it (bilinear for the colour, the nearest for the depth).
pub fn warp(rgba: &[u8], depth: &[f32], w: u32, h: u32, by: impl Fn(f32, f32) -> (f32, f32) + Sync) -> (Vec<u8>, Vec<f32>) {
    let (wu, hu) = (w as usize, h as usize);
    let mut px = vec![0u8; wu * hu * 4];
    let mut dp = vec![0.0f32; wu * hu];
    px.par_chunks_mut(wu * 4).zip(dp.par_chunks_mut(wu)).enumerate().for_each(|(y, (row, drow))| {
        for x in 0..wu {
            let (dx, dy) = by(x as f32, y as f32);
            let (sx, sy) = ((x as f32 - dx).clamp(0.0, w as f32 - 1.0), (y as f32 - dy).clamp(0.0, h as f32 - 1.0));
            let (x0, y0) = (sx.floor() as usize, sy.floor() as usize);
            let (x1, y1) = ((x0 + 1).min(wu - 1), (y0 + 1).min(hu - 1));
            let (tx, ty) = (sx - x0 as f32, sy - y0 as f32);
            for c in 0..4 {
                let at = |xx: usize, yy: usize| rgba[(yy * wu + xx) * 4 + c] as f32;
                let top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * tx;
                let bot = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * tx;
                row[x * 4 + c] = (top + (bot - top) * ty + 0.5) as u8;
            }
            drow[x] = depth.get((sy.round() as usize) * wu + sx.round() as usize).copied().unwrap_or(f32::MAX);
        }
    });
    (px, dp)
}

/// How far apart the copies of a point with a blur circle of radius 1 lie when `n` pictures
/// are spread over the aperture, and so how wide (half) each one is softened.
pub fn gap(n: u32) -> f32 {
    0.62 * (std::f32::consts::PI / n.max(1) as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zero_radius_leaves_the_picture() {
        let mut lin: Vec<f32> = (0..4 * 3 * 3).map(|i| i as f32 / 36.0).collect();
        let before = lin.clone();
        soften(&mut lin, 4, 3, &[0.0; 12]);
        assert_eq!(lin, before);
    }

    #[test]
    fn softening_keeps_the_light_and_spreads_a_point() {
        let (w, h) = (21u32, 21u32);
        let mut lin = vec![0.0f32; (w * h * 3) as usize];
        let mid = (10 * w + 10) as usize * 3;
        lin[mid] = 1.0;
        soften(&mut lin, w, h, &vec![2.0; (w * h) as usize]);
        let total: f32 = lin.iter().step_by(3).sum();
        assert!((total - 1.0).abs() < 1e-3, "{total}");
        // a 5 x 5 box: the point's light over 25 pixels
        assert!((lin[mid] - 1.0 / 25.0).abs() < 1e-4, "{}", lin[mid]);
        assert_eq!(lin[((10 * w + 13) * 3) as usize], 0.0);
    }

    #[test]
    fn burnt_out_lights_are_brighter_than_white() {
        let px = [255u8, 255, 255, 255, 128, 128, 128, 255];
        let lin = linear(&px, 2, 1, 1.0);
        assert!(lin[0] > 8.0, "{}", lin[0]);
        assert!((lin[3] - lut()[128]).abs() < 1e-6);
    }

    #[test]
    fn a_warp_moves_the_picture() {
        let px: Vec<u8> = (0..4u8).flat_map(|i| [i * 60, 0, 0, 255]).collect();
        let (out, d) = warp(&px, &[1.0, 2.0, 3.0, 4.0], 4, 1, |_, _| (1.0, 0.0));
        assert_eq!(out[4], 0);
        assert_eq!(out[8], 60);
        assert_eq!(d[2], 2.0);
    }
}
