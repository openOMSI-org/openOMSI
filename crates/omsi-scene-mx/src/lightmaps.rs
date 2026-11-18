//! Baking the `[maplight]` light maps of a tile.
use super::*;

/// A `[maplight]` as a light map bakes it: where it stands (world x, y), its height over its
/// object, colour and core radius.
#[derive(Debug, Clone, Copy)]
pub(super) struct BakeLamp {
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) height: f32,
    pub(super) color: [f32; 3],
    pub(super) radius: f32,
}

/// A light map as Omsi.exe bakes one (0x7903e0): 256 x 256 texels over the tile at `origin`
/// and its eight neighbours, north at the top row. A texel, on the ground at its south-west
/// corner, takes each lamp's colour x min(1, (core / distance)^2), added up and held at 1,
/// and its bytes truncated (0x404c88). The distance is in three dimensions from the lamp's
/// height over its own object (the `[maplight]`'s z: where the object stands does not count),
/// and a lamp further than 15.96 cores along either axis adds nothing (less than a step).
pub(super) fn bake_light_map(lamps: &[BakeLamp], origin: DVec3) -> omsi_texture::Image {
    const SIDE: usize = 256;
    let ts = tile_size() as f32;
    let reach = 15.96f32;
    let mut sum = vec![[0f32; 3]; SIDE * SIDE];
    // texel column c lies at x = (3c / 256 - 1) ts, row r at y = (2 - 3r / 256) ts
    let texel = 3.0 * ts / SIDE as f32;
    for l in lamps {
        let (lx, ly) = ((l.x - origin.x) as f32, (l.y - origin.y) as f32);
        let box_ = reach * l.radius;
        let c0 = (((lx - box_) / texel + SIDE as f32 / 3.0).floor().max(0.0)) as usize;
        let c1 = (((lx + box_) / texel + SIDE as f32 / 3.0).ceil().max(0.0) as usize).min(SIDE - 1);
        let r0 = (((2.0 * ts - (ly + box_)) / texel).floor().max(0.0)) as usize;
        let r1 = (((2.0 * ts - (ly - box_)) / texel).ceil().max(0.0) as usize).min(SIDE - 1);
        for r in r0..=r1 {
            let py = (2.0 - r as f32 / SIDE as f32 * 3.0) * ts;
            let dy = ly - py;
            if dy.abs() > box_ {
                continue;
            }
            for c in c0..=c1 {
                let px = (c as f32 / SIDE as f32 * 3.0 - 1.0) * ts;
                let dx = lx - px;
                if dx.abs() > box_ {
                    continue;
                }
                let d = (dx * dx + l.height * l.height + dy * dy).sqrt();
                let f = (l.radius / d).powi(2).min(1.0);
                let t = &mut sum[r * SIDE + c];
                for k in 0..3 {
                    t[k] = (t[k] + l.color[k] * f).min(1.0);
                }
            }
        }
    }
    let mut rgba = vec![255u8; SIDE * SIDE * 4];
    for (t, px) in sum.iter().zip(rgba.chunks_exact_mut(4)) {
        for k in 0..3 {
            px[k] = (t[k] * 255.0) as u8;
        }
    }
    omsi_texture::Image { width: SIDE as u32, height: SIDE as u32, rgba, has_alpha: false }
}

/// The part of a `.map.LM.bmp` that covers its own tile, resampled to the picture's full
/// size. The editor bakes each light map over the tile and its eight neighbours, north at the
/// top row: the tile is the middle third. Neighbouring light maps are the same picture shifted
/// by a third (85 texels between two tiles, 171 between every other one, on all stock maps).
/// Laid over the tile whole, every pool of light came out three times as large and away from
/// its lamp - a filling station's blue light lay on a garden 390 m off.
pub(super) fn own_tile_of_light_map(img: &omsi_texture::Image) -> omsi_texture::Image {
    let (w, h) = (img.width as usize, img.height as usize);
    if w < 3 || h < 3 {
        return img.clone();
    }
    let texel = |x: usize, y: usize, c: usize| img.rgba[(y * w + x) * 4 + c] as f32;
    let mut rgba = vec![0u8; w * h * 4];
    for y in 0..h {
        // (bilinear, the texel centres of the output spread evenly over the middle third)
        let sy = (h as f32 / 3.0 + (y as f32 + 0.5) / 3.0 - 0.5).clamp(0.0, (h - 1) as f32);
        let (y0, fy) = (sy.floor() as usize, sy.fract());
        let y1 = (y0 + 1).min(h - 1);
        for x in 0..w {
            let sx = (w as f32 / 3.0 + (x as f32 + 0.5) / 3.0 - 0.5).clamp(0.0, (w - 1) as f32);
            let (x0, fx) = (sx.floor() as usize, sx.fract());
            let x1 = (x0 + 1).min(w - 1);
            for c in 0..4 {
                let top = texel(x0, y0, c) * (1.0 - fx) + texel(x1, y0, c) * fx;
                let bottom = texel(x0, y1, c) * (1.0 - fx) + texel(x1, y1, c) * fx;
                rgba[(y * w + x) * 4 + c] = (top * (1.0 - fy) + bottom * fy).round() as u8;
            }
        }
    }
    omsi_texture::Image { width: img.width, height: img.height, rgba, has_alpha: img.has_alpha }
}

/// Give the lamps of a tile the hue its light map (the tile's own part, see
/// [`own_tile_of_light_map`]; north at the top row) shows under them, where the map is lit
/// there at all; their brightness stays.
pub(super) fn tint_lights_from_light_map(lights: &mut [omsi_render::PointLight], img: &omsi_texture::Image, origin: DVec3) {
    let ts = omsi_map::tile_size();
    let (w, h) = (img.width as i64, img.height as i64);
    if w == 0 || h == 0 {
        return;
    }
    for l in lights.iter_mut() {
        let u = (l.position.x - origin.x) / ts;
        let v = (l.position.y - origin.y) / ts;
        if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
            continue;
        }
        let (cx, cy) = ((u * w as f64) as i64, ((1.0 - v) * h as f64) as i64);
        // the brightest texel within a few metres (the lamp stands over its pool's edge)
        let reach = ((6.0 / ts) * w as f64).ceil().max(1.0) as i64;
        let mut best = [0u8; 3];
        for y in (cy - reach).max(0)..=(cy + reach).min(h - 1) {
            for x in (cx - reach).max(0)..=(cx + reach).min(w - 1) {
                let i = ((y * w + x) * 4) as usize;
                let c = [img.rgba[i], img.rgba[i + 1], img.rgba[i + 2]];
                if c.iter().map(|&v| v as u32).sum::<u32>() > best.iter().map(|&v| v as u32).sum::<u32>() {
                    best = c;
                }
            }
        }
        let peak = *best.iter().max().unwrap() as f32;
        if peak < 40.0 {
            continue;
        }
        let hue = [best[0] as f32 / peak, best[1] as f32 / peak, best[2] as f32 / peak];
        let bright = l.color.iter().cloned().fold(0.0f32, f32::max);
        l.color = [hue[0] * bright, hue[1] * bright, hue[2] * bright];
    }
}
