//! The enhanced precipitation's pictures, drawn at start-up (nothing is loaded): an atlas
//! of snow crystals and flakes for the snowfall (snow.wgsl `flake_alpha_e`), and what the
//! enhanced shaders are told of the weather on the ground (`Enhanced::precip_ext`).
//!
//! The atlas holds eight cells of 64 texels, four a row: single crystals in the first row
//! (a stellar dendrite, a fernlike dendrite, a sectored plate, a plain hexagonal plate, as
//! Nakaya and Libbrecht photographed them), aggregates of them in the second (clumps of a
//! few crystals each, and a rimed lump). Each shape is built of capsules and hexagons and
//! drawn by its distance, antialiased to a texel; R holds how much of a texel the ice
//! covers, G how bright it is there (the ridges and rims of a crystal catch more light than
//! its thin plate). The mip chain is a box filter, and every cell keeps its own texels down
//! to the last level, four a side.

use crate::view::Lighting;

/// Texels a cell of the atlas has a side.
pub(crate) const FLAKE_CELL: u32 = 64;
/// The atlas's cells a row and its rows.
pub(crate) const FLAKE_COLS: u32 = 4;
pub(crate) const FLAKE_ROWS: u32 = 2;
/// Its mip levels (64 down to 4 texels a cell).
pub(crate) const FLAKE_LEVELS: u32 = 5;

/// One stroke of a crystal: a capsule from `a` to `b` tapering from `r0` to `r1`, or a
/// hexagon round `a` of apothem `r0`, turned by `b.x` (filled to `fill`, its rim `r1` wide
/// drawn brighter). Coordinates in the cell, -1..1.
#[derive(Clone, Copy)]
enum Stroke {
    Capsule {
        a: [f32; 2],
        b: [f32; 2],
        r0: f32,
        r1: f32,
        bright: f32,
    },
    Hex {
        c: [f32; 2],
        turn: f32,
        r: f32,
        fill: f32,
        rim: f32,
    },
}

/// A small, fixed pseudo-random sequence (the atlas is the same on every start).
struct Lcg(u32);

impl Lcg {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.0 >> 8) as f32 / 16777216.0
    }
}

fn rot(p: [f32; 2], a: f32) -> [f32; 2] {
    let (s, c) = a.sin_cos();
    [p[0] * c - p[1] * s, p[0] * s + p[1] * c]
}

fn add(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] + b[0], a[1] + b[1]]
}

fn scale(a: [f32; 2], k: f32) -> [f32; 2] {
    [a[0] * k, a[1] * k]
}

/// A six-armed crystal round `c` of radius `size`, turned by `turn`: arms with side
/// branches at 60 degrees (`branches` a side, each shorter towards the tip), a small
/// hexagonal core; `fern` thins the arms and packs the branches.
fn dendrite(
    out: &mut Vec<Stroke>,
    c: [f32; 2],
    size: f32,
    turn: f32,
    branches: u32,
    fern: bool,
    rng: &mut Lcg,
) {
    let width = if fern { 0.022 } else { 0.03 } * size;
    out.push(Stroke::Hex {
        c,
        turn,
        r: 0.14 * size,
        fill: 0.8,
        rim: 0.03 * size,
    });
    // (the six arms grow alike, as the crystal's arms do in the same air: one set of
    // branch lengths for all of them)
    let lens: Vec<f32> = (0..branches).map(|_| 0.75 + 0.5 * rng.next()).collect();
    for arm in 0..6 {
        let a = turn + arm as f32 * std::f32::consts::FRAC_PI_3;
        let dir = rot([1.0, 0.0], a);
        let tip = scale(dir, size * 0.95);
        out.push(Stroke::Capsule {
            a: c,
            b: add(c, tip),
            r0: width,
            r1: width * 0.55,
            bright: 1.0,
        });
        for (i, len) in lens.iter().enumerate() {
            let t = 0.28 + 0.62 * (i as f32 + 0.5) / branches as f32;
            let at = add(c, scale(dir, size * 0.95 * t));
            let l = size * (1.0 - t) * if fern { 0.5 } else { 0.62 } * len;
            for side in [-1.0f32, 1.0] {
                let bd = rot(dir, side * std::f32::consts::FRAC_PI_3);
                out.push(Stroke::Capsule {
                    a: at,
                    b: add(at, scale(bd, l)),
                    r0: width * 0.8,
                    r1: width * 0.45,
                    bright: 0.9,
                });
            }
        }
    }
}

/// A plate: a hexagon with a brighter rim; `sectored` adds the ridges from the centre to
/// its corners and an inner hexagon, as a sectored plate has.
fn plate(out: &mut Vec<Stroke>, c: [f32; 2], size: f32, turn: f32, sectored: bool) {
    out.push(Stroke::Hex {
        c,
        turn,
        r: size * 0.82,
        fill: if sectored { 0.5 } else { 0.62 },
        rim: 0.05 * size,
    });
    out.push(Stroke::Hex {
        c,
        turn,
        r: size * 0.45,
        fill: 0.15,
        rim: 0.035 * size,
    });
    if sectored {
        for k in 0..6 {
            // (to the hexagon's corners, on its x axis and every 60 degrees on: its flat
            // sides face y)
            let a = turn + k as f32 * std::f32::consts::FRAC_PI_3;
            let tip = add(c, scale(rot([1.0, 0.0], a), size * 0.9));
            out.push(Stroke::Capsule {
                a: c,
                b: tip,
                r0: 0.03 * size,
                r1: 0.02 * size,
                bright: 1.0,
            });
        }
    }
}

/// The strokes of each of the eight cells.
fn cells() -> Vec<Vec<Stroke>> {
    let mut rng = Lcg(0x5eed_f1a6);
    let mut cells = Vec::new();
    // the single crystals
    let mut s = Vec::new();
    dendrite(&mut s, [0.0, 0.0], 0.92, 0.1, 4, false, &mut rng);
    cells.push(s);
    let mut s = Vec::new();
    dendrite(&mut s, [0.0, 0.0], 0.95, 0.4, 9, true, &mut rng);
    cells.push(s);
    let mut s = Vec::new();
    plate(&mut s, [0.0, 0.0], 0.95, 0.2, true);
    cells.push(s);
    let mut s = Vec::new();
    plate(&mut s, [0.0, 0.0], 0.75, 0.0, false);
    cells.push(s);
    // the aggregates: a few crystals of several kinds caught together, spread over the
    // cell (a crystal seen at a slant is foreshortened: drawn smaller is enough here)
    for (count, lump) in [(4u32, false), (6, false), (9, false), (7, true)] {
        let mut s = Vec::new();
        for i in 0..count {
            let r = 0.18 + 0.42 * rng.next().sqrt();
            let a = i as f32 * 2.399 + rng.next();
            let c = scale(rot([1.0, 0.0], a), r * 0.75);
            let size = 0.32 + 0.26 * rng.next();
            let turn = rng.next() * 6.283;
            if lump {
                // rimed: the crystals buried under frozen droplets, a knobbly lump
                s.push(Stroke::Hex {
                    c,
                    turn,
                    r: size * 0.7,
                    fill: 0.85,
                    rim: 0.0,
                });
                for _ in 0..5 {
                    let o = add(c, scale(rot([1.0, 0.0], rng.next() * 6.283), size * 0.7));
                    s.push(Stroke::Capsule {
                        a: o,
                        b: o,
                        r0: 0.05 + 0.06 * rng.next(),
                        r1: 0.0,
                        bright: 1.0,
                    });
                }
            } else if rng.next() < 0.7 {
                dendrite(&mut s, c, size, turn, 3, rng.next() < 0.4, &mut rng);
            } else {
                plate(&mut s, c, size * 0.8, turn, rng.next() < 0.5);
            }
        }
        cells.push(s);
    }
    cells
}

/// The signed distance to a stroke (negative inside), and how much ice it holds inside and
/// how bright that is.
fn stroke_at(s: &Stroke, p: [f32; 2]) -> (f32, f32, f32) {
    match *s {
        Stroke::Capsule {
            a,
            b,
            r0,
            r1,
            bright,
        } => {
            let ab = [b[0] - a[0], b[1] - a[1]];
            let ap = [p[0] - a[0], p[1] - a[1]];
            let l2 = ab[0] * ab[0] + ab[1] * ab[1];
            let t = if l2 > 0.0 {
                ((ap[0] * ab[0] + ap[1] * ab[1]) / l2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let d = [ap[0] - ab[0] * t, ap[1] - ab[1] * t];
            let r = if l2 > 0.0 { r0 + (r1 - r0) * t } else { r0 };
            ((d[0] * d[0] + d[1] * d[1]).sqrt() - r, 1.0, bright)
        }
        Stroke::Hex {
            c,
            turn,
            r,
            fill,
            rim,
        } => {
            let q = rot([p[0] - c[0], p[1] - c[1]], -turn);
            // (the distance to a regular hexagon with flat sides at +-r on y)
            let k = [-0.866_025_4f32, 0.5f32, 0.577_350_3f32];
            let mut q = [q[0].abs(), q[1].abs()];
            let dk = (k[0] * q[0] + k[1] * q[1]).min(0.0);
            q = [q[0] - 2.0 * dk * k[0], q[1] - 2.0 * dk * k[1]];
            let qx = q[0] - q[0].clamp(-k[2] * r, k[2] * r);
            let qy = q[1] - r;
            let d = (qx * qx + qy * qy).sqrt() * qy.signum();
            // a plate is thin: the ice of its body is fainter than its rim
            let on_rim = rim > 0.0 && d > -rim;
            (
                d,
                if on_rim { 1.0 } else { fill },
                if on_rim { 1.0 } else { 0.7 },
            )
        }
    }
}

/// The atlas's levels, RG8 each: level 0 is `FLAKE_COLS * FLAKE_CELL` by
/// `FLAKE_ROWS * FLAKE_CELL` texels.
pub(crate) fn flake_atlas() -> Vec<Vec<u8>> {
    let w = (FLAKE_COLS * FLAKE_CELL) as usize;
    let h = (FLAKE_ROWS * FLAKE_CELL) as usize;
    let texel = 2.0 / FLAKE_CELL as f32;
    let mut top = vec![0u8; w * h * 2];
    for (i, strokes) in cells().iter().enumerate() {
        let (cx, cy) = (
            (i as u32 % FLAKE_COLS) as usize,
            (i as u32 / FLAKE_COLS) as usize,
        );
        for y in 0..FLAKE_CELL as usize {
            for x in 0..FLAKE_CELL as usize {
                let p = [
                    (x as f32 + 0.5) * texel - 1.0,
                    (y as f32 + 0.5) * texel - 1.0,
                ];
                // the strokes laid over each other as thin sheets of ice: what one lets
                // through the next can still cover
                let mut through = 1.0f32;
                let mut light = 0.0f32;
                for s in strokes {
                    let (d, fill, bright) = stroke_at(s, p);
                    let cover = (0.5 - d / texel).clamp(0.0, 1.0) * fill;
                    if cover > 0.0 {
                        light += through * cover * bright;
                        through *= 1.0 - cover;
                    }
                }
                // (and none outside the cell's middle, so that its levels stay its own)
                let r = (p[0] * p[0] + p[1] * p[1]).sqrt();
                let keep = ((1.02 - r) / 0.06).clamp(0.0, 1.0);
                let cover = (1.0 - through) * keep;
                let bright = if cover > 0.0 {
                    (light * keep / cover).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let o = ((cy * FLAKE_CELL as usize + y) * w + cx * FLAKE_CELL as usize + x) * 2;
                top[o] = (cover * 255.0).round() as u8;
                top[o + 1] = (bright * 255.0).round() as u8;
            }
        }
    }
    let mut levels = vec![top];
    for l in 1..FLAKE_LEVELS as usize {
        let (pw, ph) = (w >> (l - 1), h >> (l - 1));
        let (nw, nh) = (pw / 2, ph / 2);
        let prev = &levels[l - 1];
        let mut next = vec![0u8; nw * nh * 2];
        for y in 0..nh {
            for x in 0..nw {
                for ch in 0..2 {
                    let at = |xx: usize, yy: usize| prev[(yy * pw + xx) * 2 + ch] as u32;
                    let s = at(2 * x, 2 * y)
                        + at(2 * x + 1, 2 * y)
                        + at(2 * x, 2 * y + 1)
                        + at(2 * x + 1, 2 * y + 1);
                    next[(y * nw + x) * 2 + ch] = s.div_ceil(4) as u8;
                }
            }
        }
        levels.push(next);
    }
    levels
}

/// The atlas on the card and its sampler (linear, clamped, with its mip chain).
pub(crate) struct FlakeAtlas {
    pub(crate) view: wgpu::TextureView,
    pub(crate) sampler: wgpu::Sampler,
}

impl FlakeAtlas {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> FlakeAtlas {
        let levels = flake_atlas();
        let (w, h) = (FLAKE_COLS * FLAKE_CELL, FLAKE_ROWS * FLAKE_CELL);
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("snow flake atlas"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: FLAKE_LEVELS,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rg8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (m, data) in levels.iter().enumerate() {
            let (lw, lh) = (w >> m, h >> m);
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &tex,
                    mip_level: m as u32,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(lw * 2),
                    rows_per_image: Some(lh),
                },
                wgpu::Extent3d {
                    width: lw,
                    height: lh,
                    depth_or_array_layers: 1,
                },
            );
        }
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("snow flake atlas"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        FlakeAtlas {
            view: tex.create_view(&wgpu::TextureViewDescriptor::default()),
            sampler,
        }
    }
}

/// The enhanced uniform's precipitation slot (`Enhanced::precip_ext`): x how hard rain (not
/// snow) is falling on the ground (0..1: the rings on the puddles), y how far the snow on
/// the roads is thawing to slush (0..1: rain or a wet road on it while no fresh snow falls),
/// z how hard it snows (0..1), w the `puddles` setting (0 none .. 2 high).
pub(crate) fn precip_ext(lighting: &Lighting) -> [f32; 4] {
    let snowing = lighting.snowfall.clamp(0.0, 1.0);
    let fresh = (snowing * 4.0).min(1.0);
    let rain = lighting.rain.clamp(0.0, 1.0) * (1.0 - fresh);
    let thaw = lighting.rain.max(lighting.wetness * 0.7).clamp(0.0, 1.0) * (1.0 - fresh);
    [rain, thaw, snowing, lighting.puddles.min(2) as f32]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flake_atlas_has_its_levels_and_shapes() {
        let levels = flake_atlas();
        assert_eq!(levels.len(), FLAKE_LEVELS as usize);
        let (w, h) = (
            (FLAKE_COLS * FLAKE_CELL) as usize,
            (FLAKE_ROWS * FLAKE_CELL) as usize,
        );
        for (m, l) in levels.iter().enumerate() {
            assert_eq!(l.len(), (w >> m) * (h >> m) * 2);
        }
        // every cell holds ice at its middle's surroundings, and none at its corners
        let top = &levels[0];
        for i in 0..(FLAKE_COLS * FLAKE_ROWS) as usize {
            let (cx, cy) = (i % FLAKE_COLS as usize, i / FLAKE_COLS as usize);
            let cover: u32 = (0..FLAKE_CELL as usize)
                .flat_map(|y| (0..FLAKE_CELL as usize).map(move |x| (x, y)))
                .map(|(x, y)| {
                    top[((cy * FLAKE_CELL as usize + y) * w + cx * FLAKE_CELL as usize + x) * 2]
                        as u32
                })
                .sum();
            assert!(cover > 255 * 200, "cell {i} is nearly empty: {cover}");
            let corner = ((cy * FLAKE_CELL as usize) * w + cx * FLAKE_CELL as usize) * 2;
            assert_eq!(top[corner], 0, "cell {i} reaches its corner");
        }
    }
}
