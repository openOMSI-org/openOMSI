//! BC1/BC2/BC3 (DXT1/3/5) block encoding, and the mip chains of textures that go to the GPU
//! compressed.
//!
//! D3DX loads a DXT file as it is and, when the file has no mip chain, filters the levels in
//! RGBA and compresses them again; a BMP or TGA stays uncompressed there. Here the uncompressed
//! pictures can be compressed too (see [`crate::GpuOptions`]): a texture on the GPU as BC1 takes
//! an eighth of its RGBA size, as BC3 a quarter. The encoder fits each block along its
//! principal colour axis and refines the end points by least squares (the approach of
//! stb_dxt and libsquish's range fit); flat blocks get exact end points from tables, alpha
//! blocks try both of BC3's interpolation modes.

use crate::dds::decode_block;

/// Colour error weights (Rec. 601 luma): a green error shows more than a blue one.
const WEIGHTS: [f32; 3] = [0.299, 0.587, 0.114];

#[inline]
fn expand5(v: u32) -> u32 {
    (v << 3) | (v >> 2)
}

#[inline]
fn expand6(v: u32) -> u32 {
    (v << 2) | (v >> 4)
}

#[inline]
fn to565(c: [f32; 3]) -> u16 {
    let r = (c[0].clamp(0.0, 255.0) * 31.0 / 255.0 + 0.5) as u16;
    let g = (c[1].clamp(0.0, 255.0) * 63.0 / 255.0 + 0.5) as u16;
    let b = (c[2].clamp(0.0, 255.0) * 31.0 / 255.0 + 0.5) as u16;
    (r << 11) | (g << 5) | b
}

#[inline]
fn from565(v: u16) -> [f32; 3] {
    let v = v as u32;
    [
        expand5((v >> 11) & 31) as f32,
        expand6((v >> 5) & 63) as f32,
        expand5(v & 31) as f32,
    ]
}

/// For every 8-bit value, the 5- and 6-bit end point pairs whose 2:1 mix (palette index 2)
/// comes closest: a flat block is then exact instead of off by the quantisation step.
struct SingleColour {
    five: [(u8, u8); 256],
    six: [(u8, u8); 256],
}

fn single_colour() -> &'static SingleColour {
    static T: std::sync::OnceLock<SingleColour> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let table = |bits: u32| -> [(u8, u8); 256] {
            let n = 1u32 << bits;
            let expand = |v: u32| if bits == 5 { expand5(v) } else { expand6(v) };
            let mut out = [(0u8, 0u8); 256];
            for (want, slot) in out.iter_mut().enumerate() {
                let mut best = (u32::MAX, 0u32, 0u32);
                for a in 0..n {
                    for b in 0..n {
                        let mix = (2 * expand(a) + expand(b)) / 3;
                        let e = (mix as i32 - want as i32).unsigned_abs();
                        // a pair whose ends are close keeps the other indices near too
                        let spread = (expand(a) as i32 - expand(b) as i32).unsigned_abs();
                        let score = e * 1000 + spread;
                        if score < best.0 {
                            best = (score, a, b);
                        }
                    }
                }
                *slot = (best.1 as u8, best.2 as u8);
            }
            out
        };
        SingleColour {
            five: table(5),
            six: table(6),
        }
    })
}

/// How the colour part of a block is interpreted.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ColourMode {
    /// Always four colours (BC2/BC3, and opaque BC1 blocks).
    Four,
    /// BC1 with 1-bit alpha: pixels whose alpha is below 128 become index 3 of the
    /// three-colour mode (transparent black).
    Punch,
}

#[inline]
fn werr(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    d[0] * d[0] * WEIGHTS[0] + d[1] * d[1] * WEIGHTS[1] + d[2] * d[2] * WEIGHTS[2]
}

/// Palette of two quantised end points in four- or three-colour mode.
#[inline]
fn palette(c0: u16, c1: u16, four: bool) -> [[f32; 3]; 4] {
    let p0 = from565(c0);
    let p1 = from565(c1);
    if four {
        let m = |a: f32, b: f32| ((2.0 * a + b) / 3.0).floor();
        [
            p0,
            p1,
            [m(p0[0], p1[0]), m(p0[1], p1[1]), m(p0[2], p1[2])],
            [m(p1[0], p0[0]), m(p1[1], p0[1]), m(p1[2], p0[2])],
        ]
    } else {
        let m = |a: f32, b: f32| ((a + b) / 2.0).floor();
        [
            p0,
            p1,
            [m(p0[0], p1[0]), m(p0[1], p1[1]), m(p0[2], p1[2])],
            [0.0; 3],
        ]
    }
}

/// Indices of the pixels against a palette; returns the total error.
#[inline]
fn assign(
    px: &[[f32; 3]; 16],
    use_px: u16,
    pal: &[[f32; 3]; 4],
    n: usize,
    idx: &mut [u8; 16],
) -> f32 {
    let mut total = 0.0;
    for i in 0..16 {
        if use_px & (1 << i) == 0 {
            idx[i] = 3;
            continue;
        }
        let p = px[i];
        let mut e = [f32::MAX; 4];
        for k in 0..n {
            e[k] = werr(p, pal[k]);
        }
        // the first of equal errors wins, as before
        let mut k = 0usize;
        for j in 1..4 {
            if e[j] < e[k] {
                k = j;
            }
        }
        idx[i] = k as u8;
        total += e[k];
    }
    total
}

/// Least-squares end points for fixed indices (`n` = 4 or 3 colours).
fn least_squares(
    px: &[[f32; 3]; 16],
    use_px: u16,
    idx: &[u8; 16],
    n: usize,
) -> Option<([f32; 3], [f32; 3])> {
    let (mut aa, mut bb, mut ab) = (0.0f32, 0.0f32, 0.0f32);
    let mut ax = [0.0f32; 3];
    let mut bx = [0.0f32; 3];
    for i in 0..16 {
        if use_px & (1 << i) == 0 {
            continue;
        }
        const FOUR: [(f32, f32); 4] = [
            (1.0, 0.0),
            (0.0, 1.0),
            (2.0 / 3.0, 1.0 / 3.0),
            (1.0 / 3.0, 2.0 / 3.0),
        ];
        const THREE: [(f32, f32); 4] = [(1.0, 0.0), (0.0, 1.0), (0.5, 0.5), (0.5, 0.5)];
        let (a, b) = if n == 4 {
            FOUR[idx[i] as usize & 3]
        } else {
            THREE[idx[i] as usize & 3]
        };
        aa += a * a;
        bb += b * b;
        ab += a * b;
        for c in 0..3 {
            ax[c] += a * px[i][c];
            bx[c] += b * px[i][c];
        }
    }
    let det = aa * bb - ab * ab;
    if det.abs() < 1e-6 {
        return None;
    }
    let f = 1.0 / det;
    let mut e0 = [0.0; 3];
    let mut e1 = [0.0; 3];
    for c in 0..3 {
        e0[c] = (ax[c] * bb - bx[c] * ab) * f;
        e1[c] = (bx[c] * aa - ax[c] * ab) * f;
    }
    Some((e0, e1))
}

/// End points and index for one colour of a four-colour block: the 2:1 mix (palette index 2,
/// c0 > c1) of two table end points, swapped ends making it index 3 - exact where a plain
/// 565 colour is off by its quantisation step.
fn flat_ends(c: [u8; 3]) -> (u16, u16, u8) {
    let t = single_colour();
    let (r0, r1) = t.five[c[0] as usize];
    let (g0, g1) = t.six[c[1] as usize];
    let (b0, b1) = t.five[c[2] as usize];
    let mut c0 = ((r0 as u16) << 11) | ((g0 as u16) << 5) | b0 as u16;
    let mut c1 = ((r1 as u16) << 11) | ((g1 as u16) << 5) | b1 as u16;
    let mut k = 2;
    if c0 < c1 {
        std::mem::swap(&mut c0, &mut c1);
        k = 3;
    }
    if c0 == c1 {
        k = 0;
    }
    (c0, c1, k)
}

/// How much a shift of a block's mean colour counts against an end point pair, per pixel
/// and squared channel level, besides the per-pixel error: a block that fits its own noise
/// along a tinted axis (a smooth dark grey picks up green or blue - 565 steps by 8 in red
/// and blue, and the luma weights make a blue error cheap) shows as a patch of another
/// colour beside its neighbours, the grain of a dark dashboard (#845), where the pixel
/// error alone hardly changes.
const DC_PENALTY: f32 = 0.5;

/// Encode the colour half of a block. `px` are the 16 pixels (RGBA), `mode` how the block
/// will be read.
fn encode_colour(px: &[[u8; 4]; 16], mode: ColourMode) -> ([u8; 8], f32) {
    let mut use_px: u16 = 0xFFFF;
    if mode == ColourMode::Punch {
        use_px = 0;
        for (i, p) in px.iter().enumerate() {
            if p[3] >= 128 {
                use_px |= 1 << i;
            }
        }
    }
    let transparent = use_px != 0xFFFF;
    let mut fpx = [[0.0f32; 3]; 16];
    for i in 0..16 {
        fpx[i] = [px[i][0] as f32, px[i][1] as f32, px[i][2] as f32];
    }
    let write = |c0: u16, c1: u16, idx: &[u8; 16]| -> [u8; 8] {
        let mut bits = 0u32;
        for (i, &k) in idx.iter().enumerate() {
            bits |= (k as u32 & 3) << (2 * i);
        }
        let mut out = [0u8; 8];
        out[..2].copy_from_slice(&c0.to_le_bytes());
        out[2..4].copy_from_slice(&c1.to_le_bytes());
        out[4..].copy_from_slice(&bits.to_le_bytes());
        out
    };
    if use_px == 0 {
        // all transparent: three-colour mode, every index 3
        return (write(0, 0, &[3; 16]), 0.0);
    }
    // flat block (all used pixels one colour): exact end points from the tables
    let first = (0..16).find(|i| use_px & (1 << i) != 0).unwrap();
    let flat = (0..16).all(|i| use_px & (1 << i) == 0 || px[i][..3] == px[first][..3]);
    if flat {
        let c = px[first];
        let q = to565([c[0] as f32, c[1] as f32, c[2] as f32]);
        let exact = from565(q) == [c[0] as f32, c[1] as f32, c[2] as f32];
        let (c0, c1, k) = if !exact && !transparent {
            flat_ends([c[0], c[1], c[2]])
        } else {
            (q, q, 0u8)
        };
        // (with transparent pixels the block is in three-colour mode, c0 == c1 = q)
        let mut idx = [k; 16];
        for (i, v) in idx.iter_mut().enumerate() {
            if use_px & (1 << i) == 0 {
                *v = 3;
            }
        }
        let pal = palette(c0, c1, c0 > c1);
        let err = (0..16)
            .filter(|i| use_px & (1 << i) != 0)
            .map(|i| werr(fpx[i], pal[idx[i] as usize]))
            .sum();
        return (write(c0, c1, &idx), err);
    }
    // the used pixels' mean colour, and what a pair costs for moving it (see DC_PENALTY)
    let mut mean_u = [0.0f32; 3];
    let used = (0..16).filter(|i| use_px & (1 << i) != 0).count() as f32;
    for i in (0..16).filter(|i| use_px & (1 << i) != 0) {
        for c in 0..3 {
            mean_u[c] += fpx[i][c] / used;
        }
    }
    let dc_cost = |c0: u16, c1: u16, four: bool, idx: &[u8; 16]| -> f32 {
        let pal = palette(c0, c1, four);
        let mut m = [0.0f32; 3];
        for i in (0..16).filter(|i| use_px & (1 << i) != 0) {
            for c in 0..3 {
                m[c] += pal[idx[i] as usize][c] / used;
            }
        }
        DC_PENALTY
            * used
            * (0..3)
                .map(|c| (m[c] - mean_u[c]) * (m[c] - mean_u[c]))
                .sum::<f32>()
    };
    // principal axis of the used pixels (weighted space)
    let sw = [WEIGHTS[0].sqrt(), WEIGHTS[1].sqrt(), WEIGHTS[2].sqrt()];
    let mut mean = [0.0f32; 3];
    let mut n = 0.0f32;
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for i in 0..16 {
        if use_px & (1 << i) == 0 {
            continue;
        }
        for c in 0..3 {
            let v = fpx[i][c] * sw[c];
            mean[c] += v;
            lo[c] = lo[c].min(v);
            hi[c] = hi[c].max(v);
        }
        n += 1.0;
    }
    for m in mean.iter_mut() {
        *m /= n;
    }
    let mut cov = [0.0f32; 6];
    for i in 0..16 {
        if use_px & (1 << i) == 0 {
            continue;
        }
        let d = [
            fpx[i][0] * sw[0] - mean[0],
            fpx[i][1] * sw[1] - mean[1],
            fpx[i][2] * sw[2] - mean[2],
        ];
        cov[0] += d[0] * d[0];
        cov[1] += d[0] * d[1];
        cov[2] += d[0] * d[2];
        cov[3] += d[1] * d[1];
        cov[4] += d[1] * d[2];
        cov[5] += d[2] * d[2];
    }
    let mut axis = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    for _ in 0..3 {
        let v = [
            cov[0] * axis[0] + cov[1] * axis[1] + cov[2] * axis[2],
            cov[1] * axis[0] + cov[3] * axis[1] + cov[4] * axis[2],
            cov[2] * axis[0] + cov[4] * axis[1] + cov[5] * axis[2],
        ];
        let len = v[0].abs().max(v[1].abs()).max(v[2].abs());
        if len < 1e-6 {
            break;
        }
        axis = [v[0] / len, v[1] / len, v[2] / len];
    }
    let alen = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    if alen < 1e-6 {
        axis = [1.0, 1.0, 1.0];
    }
    let (mut tmin, mut tmax) = (f32::MAX, f32::MIN);
    for i in 0..16 {
        if use_px & (1 << i) == 0 {
            continue;
        }
        let t = (0..3)
            .map(|c| (fpx[i][c] * sw[c] - mean[c]) * axis[c])
            .sum::<f32>();
        tmin = tmin.min(t);
        tmax = tmax.max(t);
    }
    let a2 = axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2];
    let point = |t: f32| -> [f32; 3] {
        let s = t / a2.max(1e-12);
        [
            (mean[0] + axis[0] * s) / sw[0],
            (mean[1] + axis[1] * s) / sw[1],
            (mean[2] + axis[2] * s) / sw[2],
        ]
    };
    let mut e0 = point(tmax);
    let mut e1 = point(tmin);
    // try the colour count the block allows, keep the best of the refinements
    let n_col = if transparent { 3 } else { 4 };
    let mut best: Option<(f32, u16, u16, [u8; 16])> = None;
    const ROUNDS: usize = 3;
    let mut last: Option<(u16, u16)> = None;
    for round in 0..ROUNDS {
        let mut c0 = to565(e0);
        let mut c1 = to565(e1);
        let four = n_col == 4;
        if four && c0 < c1 {
            std::mem::swap(&mut c0, &mut c1);
        }
        if !four && c0 > c1 {
            std::mem::swap(&mut c0, &mut c1);
        }
        // the same ends as the last round: the same indices, nothing more to find
        if last == Some((c0, c1)) {
            break;
        }
        last = Some((c0, c1));
        let mut idx = [0u8; 16];
        let err = if four && c0 == c1 {
            // equal ends would read as three colours: one colour it is
            let pal = palette(c0, c1, true);
            let mut e = 0.0;
            for i in 0..16 {
                idx[i] = 0;
                e += werr(fpx[i], pal[0]);
            }
            e
        } else {
            let pal = palette(c0, c1, four);
            assign(&fpx, use_px, &pal, n_col, &mut idx)
        };
        let score = err + dc_cost(c0, c1, four, &idx);
        if best.as_ref().map(|b| score < b.0).unwrap_or(true) {
            best = Some((score, c0, c1, idx));
        }
        if err <= 0.0 || (four && c0 == c1) || round + 1 == ROUNDS {
            break;
        }
        // refine: least squares on the indices just found (in the palette's own order)
        match least_squares(&fpx, use_px, &idx, n_col) {
            Some((a, b)) => {
                e0 = a;
                e1 = b;
            }
            None => break,
        }
    }
    // a smooth block may keep its mean colour better with the flat fit of that mean
    if !transparent {
        let m = [0, 1, 2].map(|c| (mean_u[c] + 0.5).clamp(0.0, 255.0) as u8);
        let (c0, c1, k) = flat_ends(m);
        let idx = [k; 16];
        let pal = palette(c0, c1, true);
        let err: f32 = (0..16).map(|i| werr(fpx[i], pal[k as usize])).sum();
        let score = err + dc_cost(c0, c1, true, &idx);
        if best.as_ref().map(|b| score < b.0).unwrap_or(true) {
            best = Some((score, c0, c1, idx));
        }
    }
    let (_, c0, c1, idx) = best.unwrap();
    let pal = palette(c0, c1, c0 > c1 || (n_col == 4 && c0 == c1));
    let err = (0..16)
        .filter(|i| use_px & (1 << i) != 0)
        .map(|i| werr(fpx[i], pal[idx[i] as usize]))
        .sum();
    (write(c0, c1, &idx), err)
}

/// BC3 alpha block (and the error it leaves).
fn encode_alpha_bc3(px: &[[u8; 4]; 16]) -> ([u8; 8], f32) {
    let mut lo = 255u8;
    let mut hi = 0u8;
    let mut lo_in = 255u8;
    let mut hi_in = 0u8;
    for p in px {
        lo = lo.min(p[3]);
        hi = hi.max(p[3]);
        if p[3] != 0 && p[3] != 255 {
            lo_in = lo_in.min(p[3]);
            hi_in = hi_in.max(p[3]);
        }
    }
    let pack = |a0: u8, a1: u8, idx: &[u8; 16]| -> [u8; 8] {
        let mut bits = 0u64;
        for (i, &k) in idx.iter().enumerate() {
            bits |= (k as u64 & 7) << (3 * i);
        }
        let mut out = [0u8; 8];
        out[0] = a0;
        out[1] = a1;
        out[2..].copy_from_slice(&bits.to_le_bytes()[..6]);
        out
    };
    if lo == hi {
        return (pack(lo, lo, &[0; 16]), 0.0);
    }
    let table = |a0: u8, a1: u8| -> [i32; 8] {
        let (a0, a1) = (a0 as i32, a1 as i32);
        let mut t = [0i32; 8];
        t[0] = a0;
        t[1] = a1;
        if a0 > a1 {
            for i in 1..7 {
                t[i + 1] = ((7 - i as i32) * a0 + i as i32 * a1) / 7;
            }
        } else {
            for i in 1..5 {
                t[i + 1] = ((5 - i as i32) * a0 + i as i32 * a1) / 5;
            }
            t[6] = 0;
            t[7] = 255;
        }
        t
    };
    let fit = |a0: u8, a1: u8| -> (u32, [u8; 16]) {
        let t = table(a0, a1);
        let mut idx = [0u8; 16];
        let mut err = 0u32;
        for (i, p) in px.iter().enumerate() {
            let a = p[3] as i32;
            let mut best = (u32::MAX, 0u8);
            for (k, v) in t.iter().enumerate() {
                let d = (a - v).unsigned_abs();
                if d < best.0 {
                    best = (d, k as u8);
                }
            }
            idx[i] = best.1;
            err += best.0 * best.0;
        }
        (err, idx)
    };
    // only fully clear and fully opaque pixels: the six-value mode has both exactly
    if lo_in > hi_in {
        let (e, idx) = fit(0, 0);
        return (pack(0, 0, &idx), e as f32);
    }
    // eight-value mode over the full range, and the six-value mode with explicit 0 and 255
    // over the values between (cut-outs with soft edges)
    let mut best = {
        let (e, idx) = fit(hi, lo);
        (e, hi, lo, idx)
    };
    if lo == 0 || hi == 255 {
        let (e, idx) = fit(lo_in, hi_in);
        if e < best.0 {
            best = (e, lo_in, hi_in, idx);
        }
    }
    // a little end point search where it is not exact yet
    if best.0 > 16 {
        let (h0, l0, six) = (best.1, best.2, best.1 <= best.2);
        for d0 in [-1i32, 1, 0] {
            for d1 in [-1i32, 0, 1] {
                let (a0, a1) = (
                    (h0 as i32 + d0).clamp(0, 255) as u8,
                    (l0 as i32 + d1).clamp(0, 255) as u8,
                );
                if (a0 <= a1) != six || (d0 == 0 && d1 == 0) {
                    continue;
                }
                let (e, idx) = fit(a0, a1);
                if e < best.0 {
                    best = (e, a0, a1, idx);
                }
            }
        }
    }
    (pack(best.1, best.2, &best.3), best.0 as f32)
}

/// BC2 (explicit 4-bit) alpha block.
fn encode_alpha_bc2(px: &[[u8; 4]; 16]) -> ([u8; 8], f32) {
    let mut bits = 0u64;
    let mut err = 0.0;
    for (i, p) in px.iter().enumerate() {
        let q = ((p[3] as u32 * 15 + 127) / 255) as u64;
        bits |= q << (4 * i);
        let d = p[3] as f32 - (q * 17) as f32;
        err += d * d;
    }
    (bits.to_le_bytes(), err)
}

/// Block formats this module writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bc {
    /// DXT1; `punch` = 1-bit alpha (pixels below alpha 128 become transparent).
    Bc1 {
        punch: bool,
    },
    Bc2,
    Bc3,
}

impl Bc {
    pub fn block_bytes(self) -> usize {
        match self {
            Bc::Bc1 { .. } => 8,
            _ => 16,
        }
    }
}

/// The 16 pixels of block (bx, by), edge pixels repeated for a picture smaller than 4.
#[inline]
fn gather(rgba: &[u8], w: usize, h: usize, bx: usize, by: usize) -> [[u8; 4]; 16] {
    let mut px = [[0u8; 4]; 16];
    for y in 0..4 {
        let sy = (by * 4 + y).min(h - 1);
        for x in 0..4 {
            let sx = (bx * 4 + x).min(w - 1);
            let o = (sy * w + sx) * 4;
            px[y * 4 + x].copy_from_slice(&rgba[o..o + 4]);
        }
    }
    px
}

/// Encode an RGBA picture. Returns the blocks and the summed weighted squared error of
/// the colour (per pixel, 0..65025 scale) and alpha channels.
pub fn encode(rgba: &[u8], width: u32, height: u32, format: Bc) -> (Vec<u8>, f64, f64) {
    let (w, h) = (width.max(1) as usize, height.max(1) as usize);
    let bw = w.div_ceil(4);
    let bh = h.div_ceil(4);
    let bb = format.block_bytes();
    let mut out = vec![0u8; bw * bh * bb];
    let row = |by: usize, dst: &mut [u8]| -> (f64, f64) {
        let (mut ec, mut ea) = (0.0f64, 0.0f64);
        for bx in 0..bw {
            let px = gather(rgba, w, h, bx, by);
            let blk = &mut dst[bx * bb..(bx + 1) * bb];
            match format {
                Bc::Bc1 { punch } => {
                    let has_clear = punch && px.iter().any(|p| p[3] < 128);
                    let (c, e) = encode_colour(
                        &px,
                        if has_clear {
                            ColourMode::Punch
                        } else {
                            ColourMode::Four
                        },
                    );
                    blk.copy_from_slice(&c);
                    ec += e as f64;
                }
                Bc::Bc2 | Bc::Bc3 => {
                    let (a, e_a) = if format == Bc::Bc2 {
                        encode_alpha_bc2(&px)
                    } else {
                        encode_alpha_bc3(&px)
                    };
                    let (c, e) = encode_colour(&px, ColourMode::Four);
                    blk[..8].copy_from_slice(&a);
                    blk[8..].copy_from_slice(&c);
                    ec += e as f64;
                    ea += e_a as f64;
                }
            }
        }
        (ec, ea)
    };
    let (mut ec, mut ea) = (0.0, 0.0);
    // big pictures row by row on the worker pool
    if bw * bh >= 4096 {
        use rayon::prelude::*;
        let sums: Vec<(f64, f64)> = out
            .par_chunks_mut(bw * bb)
            .enumerate()
            .map(|(by, dst)| row(by, dst))
            .collect();
        for (c, a) in sums {
            ec += c;
            ea += a;
        }
    } else {
        for (by, dst) in out.chunks_mut(bw * bb).enumerate() {
            let (c, a) = row(by, dst);
            ec += c;
            ea += a;
        }
    }
    (out, ec, ea)
}

fn decode_kind(format: Bc) -> u8 {
    match format {
        Bc::Bc1 { .. } => 1,
        Bc::Bc2 => 3,
        Bc::Bc3 => 5,
    }
}

/// Decode blocks back to RGBA (`kind` 1, 3 or 5 as in DXT1/3/5).
pub fn decode(blocks: &[u8], width: u32, height: u32, format: Bc) -> Vec<u8> {
    let (w, h) = (width.max(1) as usize, height.max(1) as usize);
    let bw = w.div_ceil(4);
    let bb = format.block_bytes();
    let kind = decode_kind(format);
    let mut rgba = vec![0u8; w * h * 4];
    for (i, blk) in blocks.chunks_exact(bb).enumerate() {
        let (bx, by) = (i % bw, i / bw);
        let px = decode_block(blk, kind);
        for y in 0..4 {
            for x in 0..4 {
                let (ix, iy) = (bx * 4 + x, by * 4 + y);
                if ix < w && iy < h {
                    rgba[(iy * w + ix) * 4..][..4].copy_from_slice(&px[y * 4 + x]);
                }
            }
        }
    }
    rgba
}

/// The largest colour shift (luma-weighted squared error of the mean, 0..65025 scale) of a
/// flat patch - texels of one exact colour, two or more in a block - in a block that holds
/// three or more such patches over at least 12 of its 16 texels. `src` is the picture,
/// `dec` its compressed blocks decoded. Such a block is a row of small colour swatches (an
/// atlas whose handrails or buttons take their colour from two or three texels); four
/// colours along one line are all a block can hold, so one swatch may turn into another
/// colour altogether while the picture as a whole still measures well.
pub fn worst_patch_shift(src: &[u8], dec: &[u8], width: u32, height: u32) -> f64 {
    let (w, h) = (width as usize, height as usize);
    worst_shift(src, w, h, |bx, by| gather(dec, w, h, bx, by))
}

pub(crate) fn worst_block_patch_shift(
    src: &[u8],
    blocks: &[u8],
    width: u32,
    height: u32,
    format: Bc,
) -> f64 {
    let (w, h) = (width as usize, height as usize);
    let (bw, bb, kind) = (w.div_ceil(4), format.block_bytes(), decode_kind(format));
    worst_shift(src, w, h, |bx, by| {
        decode_block(&blocks[(by * bw + bx) * bb..][..bb], kind)
    })
}

fn worst_shift(src: &[u8], w: usize, h: usize, dec: impl Fn(usize, usize) -> [[u8; 4]; 16]) -> f64 {
    let mut worst = 0.0f64;
    for by in 0..h / 4 {
        for bx in 0..w / 4 {
            let mut keys = [0u32; 16];
            for (y, row) in keys.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let o = ((by * 4 + y) * w + bx * 4) * 4;
                for (k, p) in row.iter_mut().zip(src[o..o + 16].as_chunks::<4>().0) {
                    *k = u32::from_le_bytes(*p) & 0x00FF_FFFF;
                }
            }
            if let Some(counts) = swatch_counts(&keys) {
                worst = worst.max(keys_shift(&keys, &counts, &dec(bx, by)));
            }
        }
    }
    worst
}

fn block_patch_shift(src: &[[u8; 4]; 16], dec: &[[u8; 4]; 16]) -> f64 {
    let keys = src.map(|p| u32::from_le_bytes([p[0], p[1], p[2], 0]));
    swatch_counts(&keys).map_or(0.0, |counts| keys_shift(&keys, &counts, dec))
}

fn swatch_counts(keys: &[u32; 16]) -> Option<[u8; 16]> {
    if keys.iter().all(|&k| k == keys[0]) {
        return None;
    }
    let counts: [u8; 16] =
        std::array::from_fn(|i| keys.iter().map(|&k| (k == keys[i]) as u8).sum());
    if counts.iter().filter(|&&n| n >= 2).count() < 12 {
        return None;
    }
    let patches = (0..16)
        .filter(|&i| counts[i] >= 2 && !keys[..i].contains(&keys[i]))
        .count();
    (patches >= 3).then_some(counts)
}

fn keys_shift(keys: &[u32; 16], counts: &[u8; 16], dec: &[[u8; 4]; 16]) -> f64 {
    let mut block_worst = 0.0f64;
    for i in 0..16 {
        if counts[i] < 2 || keys[..i].contains(&keys[i]) {
            continue;
        }
        let (n, mut sum) = (counts[i] as u32, [0u32; 3]);
        for j in i..16 {
            if keys[j] == keys[i] {
                for c in 0..3 {
                    sum[c] += dec[j][c] as u32;
                }
            }
        }
        let colour = keys[i].to_le_bytes();
        let e: f64 = (0..3)
            .map(|c| {
                let d = sum[c] as f64 / n as f64 - colour[c] as f64;
                d * d * WEIGHTS[c] as f64
            })
            .sum();
        block_worst = block_worst.max(e);
    }
    block_worst
}

fn quantised_ends(c: [f32; 3]) -> [u16; 3] {
    let q = |v: f32, max: f32, up: f32| {
        ((v.clamp(0.0, 255.0) * max / 255.0) + up)
            .floor()
            .clamp(0.0, max) as u16
    };
    let at = |up: f32| (q(c[0], 31.0, up) << 11) | (q(c[1], 63.0, up) << 5) | q(c[2], 31.0, up);
    [at(0.5), at(0.0), at(0.999)]
}

fn swatch_block(px: &[[u8; 4]; 16], four_only: bool, max_shift_sq: f64) -> Option<([u8; 8], f32)> {
    let fpx: [[f32; 3]; 16] = px.map(|p| [p[0] as f32, p[1] as f32, p[2] as f32]);
    let mut colours: Vec<[f32; 3]> = Vec::with_capacity(16);
    for p in &fpx {
        if !colours.contains(p) {
            colours.push(*p);
        }
    }
    let mut patch = [false; 16];
    for i in 0..16 {
        patch[i] = (0..16).filter(|&j| px[j][..3] == px[i][..3]).count() >= 2;
    }
    const FOUR: [f32; 4] = [1.0, 0.0, 2.0 / 3.0, 1.0 / 3.0];
    const THREE: [f32; 3] = [1.0, 0.0, 0.5];
    let mut best: Option<(f32, u16, u16, [u8; 16])> = None;
    let mut try_pair = |lo: u16, hi: u16| {
        let mut modes: [Option<(u16, u16, bool)>; 2] = [None, None];
        if hi > lo {
            modes[0] = Some((hi, lo, true));
        }
        if !four_only {
            modes[1] = Some((lo.min(hi), lo.max(hi), false));
        }
        for (c0, c1, four) in modes.into_iter().flatten() {
            let pal = palette(c0, c1, four);
            let mut idx = [0u8; 16];
            let err = assign(&fpx, 0xFFFF, &pal, if four { 4 } else { 3 }, &mut idx);
            if best.as_ref().is_some_and(|b| err >= b.0) {
                continue;
            }
            let fits = (0..16)
                .all(|i| !patch[i] || (werr(fpx[i], pal[idx[i] as usize]) as f64) <= max_shift_sq);
            if fits {
                best = Some((err, c0, c1, idx));
            }
        }
    };
    let weights: Vec<f32> = if four_only {
        FOUR.to_vec()
    } else {
        FOUR.iter().chain(THREE.iter()).copied().collect()
    };
    for (ai, a) in colours.iter().enumerate() {
        for b in &colours[ai + 1..] {
            for &u in &weights {
                for &v in &weights {
                    if u == v {
                        continue;
                    }
                    let d = [0, 1, 2].map(|c| (a[c] - b[c]) / (u - v));
                    let e1 = [0, 1, 2].map(|c| a[c] - u * d[c]);
                    let e0 = [0, 1, 2].map(|c| e1[c] + d[c]);
                    for q0 in quantised_ends(e0) {
                        for q1 in quantised_ends(e1) {
                            try_pair(q0.min(q1), q0.max(q1));
                        }
                    }
                }
            }
        }
    }
    let (err, c0, c1, idx) = best?;
    let mut bits = 0u32;
    for (i, &k) in idx.iter().enumerate() {
        bits |= (k as u32 & 3) << (2 * i);
    }
    let mut out = [0u8; 8];
    out[..2].copy_from_slice(&c0.to_le_bytes());
    out[2..4].copy_from_slice(&c1.to_le_bytes());
    out[4..].copy_from_slice(&bits.to_le_bytes());
    Some((out, err))
}

pub fn repair_swatches(
    src: &[u8],
    blocks: &mut [u8],
    width: u32,
    height: u32,
    format: Bc,
    max_shift_sq: f64,
) -> Option<f64> {
    let (w, h) = (width as usize, height as usize);
    let bw = w.div_ceil(4);
    let bb = format.block_bytes();
    let (kind, colour_at) = match format {
        Bc::Bc1 { .. } => (1u8, 0usize),
        Bc::Bc2 => (3, 8),
        Bc::Bc3 => (5, 8),
    };
    let mut delta = 0.0f64;
    for by in 0..h / 4 {
        for bx in 0..w / 4 {
            let px = gather(src, w, h, bx, by);
            let blk = &mut blocks[(by * bw + bx) * bb..][..bb];
            let dec = decode_block(blk, kind);
            if block_patch_shift(&px, &dec) <= max_shift_sq {
                continue;
            }
            if matches!(format, Bc::Bc1 { punch: true }) && px.iter().any(|p| p[3] < 128) {
                return None;
            }
            let (colour, err) =
                swatch_block(&px, format != Bc::Bc1 { punch: false }, max_shift_sq)?;
            let before: f32 = (0..16)
                .map(|i| {
                    werr(
                        [px[i][0] as f32, px[i][1] as f32, px[i][2] as f32],
                        [dec[i][0] as f32, dec[i][1] as f32, dec[i][2] as f32],
                    )
                })
                .sum();
            blk[colour_at..colour_at + 8].copy_from_slice(&colour);
            delta += (err - before) as f64;
        }
    }
    Some(delta)
}

/// Whether a DXT1 block has transparent pixels (three-colour mode with index 3 in use).
pub fn bc1_block_has_alpha(blk: &[u8]) -> bool {
    let c0 = u16::from_le_bytes([blk[0], blk[1]]);
    let c1 = u16::from_le_bytes([blk[2], blk[3]]);
    if c0 > c1 {
        return false;
    }
    let bits = u32::from_le_bytes([blk[4], blk[5], blk[6], blk[7]]);
    (0..16).any(|i| (bits >> (2 * i)) & 3 == 3)
}

fn srgb_to_linear_table() -> &'static [f32; 256] {
    static T: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut t = [0.0f32; 256];
        for (i, v) in t.iter_mut().enumerate() {
            let c = i as f32 / 255.0;
            *v = if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            };
        }
        t
    })
}

fn linear_to_srgb_table() -> &'static [u8; 4097] {
    static T: std::sync::OnceLock<[u8; 4097]> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut t = [0u8; 4097];
        for (i, v) in t.iter_mut().enumerate() {
            let l = i as f32 / 4096.0;
            let s = if l <= 0.003_130_8 {
                l * 12.92
            } else {
                1.055 * l.powf(1.0 / 2.4) - 0.055
            };
            *v = (s * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
        }
        t
    })
}

/// The next mip level of an sRGB picture: a 2x2 box in linear light (what the GPU's
/// filtered blit into an sRGB target gives), alpha averaged as it is.
pub fn downsample(rgba: &[u8], w: u32, h: u32) -> (Vec<u8>, u32, u32) {
    downsample_filtered(rgba, w, h, false)
}

/// Colour mips use coverage-weighted RGB, like the GPU mip shader, so white RGB in
/// clear texels cannot bleach the visible silhouette. With no covered texel, keep
/// the plain RGB: opaque materials may use zero alpha only as a reflection mask.
pub fn downsample_alpha_weighted(rgba: &[u8], w: u32, h: u32) -> (Vec<u8>, u32, u32) {
    downsample_filtered(rgba, w, h, true)
}

fn downsample_filtered(rgba: &[u8], w: u32, h: u32, weighted: bool) -> (Vec<u8>, u32, u32) {
    let (w, h) = (w as usize, h as usize);
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let lin = srgb_to_linear_table();
    let back = linear_to_srgb_table();
    let mut out = vec![0u8; nw * nh * 4];
    let fill_row = |y: usize, dst: &mut [u8]| {
        for x in 0..nw {
            let mut c = [0.0f32; 3];
            let mut covered = [0.0f32; 3];
            let mut a = 0u32;
            for dy in 0..2 {
                let sy = (y * 2 + dy).min(h - 1);
                for dx in 0..2 {
                    let sx = (x * 2 + dx).min(w - 1);
                    let o = (sy * w + sx) * 4;
                    let alpha = rgba[o + 3] as u32;
                    for k in 0..3 {
                        let v = lin[rgba[o + k] as usize];
                        c[k] += v;
                        if weighted {
                            covered[k] += v * alpha as f32;
                        }
                    }
                    a += alpha;
                }
            }
            let d = &mut dst[x * 4..x * 4 + 4];
            for k in 0..3 {
                let v = if weighted && a > 0 {
                    covered[k] / a as f32
                } else {
                    c[k] * 0.25
                };
                d[k] = back[((v * 4096.0 + 0.5) as usize).min(4096)];
            }
            d[3] = ((a + 2) / 4) as u8;
        }
    };
    if nw * nh >= 65536 {
        use rayon::prelude::*;
        out.par_chunks_mut(nw * 4)
            .enumerate()
            .for_each(|(y, dst)| fill_row(y, dst));
    } else {
        for (y, dst) in out.chunks_mut(nw * 4).enumerate() {
            fill_row(y, dst);
        }
    }
    (out, nw as u32, nh as u32)
}

/// Resample an RGBA picture to `nw` x `nh` with a Catmull-Rom filter (the texel centres
/// keep their places in texture space, so a picture resized by a texel or two looks the
/// same where it is drawn; the filter keeps its sharpness).
pub fn resize(rgba: &[u8], w: u32, h: u32, nw: u32, nh: u32) -> Vec<u8> {
    let (w, h, nw, nh) = (w as usize, h as usize, nw as usize, nh as usize);
    if (w, h) == (nw, nh) {
        return rgba.to_vec();
    }
    // taps and weights along one axis: for every output texel the four source texels
    let taps = |from: usize, to: usize| -> Vec<([usize; 4], [f32; 4])> {
        (0..to)
            .map(|o| {
                let src = (o as f32 + 0.5) * from as f32 / to as f32 - 0.5;
                let i = src.floor();
                let t = src - i;
                let i = i as isize;
                let (t2, t3) = (t * t, t * t * t);
                let wts = [
                    (-t3 + 2.0 * t2 - t) * 0.5,
                    (3.0 * t3 - 5.0 * t2 + 2.0) * 0.5,
                    (-3.0 * t3 + 4.0 * t2 + t) * 0.5,
                    (t3 - t2) * 0.5,
                ];
                let idx = [i - 1, i, i + 1, i + 2].map(|k| k.clamp(0, from as isize - 1) as usize);
                (idx, wts)
            })
            .collect()
    };
    let tx = taps(w, nw);
    let ty = taps(h, nh);
    // Rows are resampled across once and kept while the output rows below still need them
    // (four at a time), so a 2550² picture needs a few rows of floats, not a copy of itself.
    let across = |y: usize, row: &mut [f32]| {
        let src = &rgba[y * w * 4..(y + 1) * w * 4];
        for (x, (idx, wts)) in tx.iter().enumerate() {
            for c in 0..4 {
                row[x * 4 + c] = (0..4).map(|k| src[idx[k] * 4 + c] as f32 * wts[k]).sum();
            }
        }
    };
    let mut out = vec![0u8; nw * nh * 4];
    use rayon::prelude::*;
    let band = 64usize;
    out.par_chunks_mut(nw * 4 * band)
        .enumerate()
        .for_each(|(b, rows)| {
            let mut cache: Vec<(usize, Vec<f32>)> = Vec::with_capacity(6);
            for (r, row) in rows.chunks_mut(nw * 4).enumerate() {
                let (idx, wts) = &ty[b * band + r];
                for &sy in idx {
                    if !cache.iter().any(|(k, _)| *k == sy) {
                        let mut v = if cache.len() >= 6 {
                            cache.remove(0).1
                        } else {
                            vec![0f32; nw * 4]
                        };
                        across(sy, &mut v);
                        cache.push((sy, v));
                    }
                }
                let rows4: Vec<&Vec<f32>> = idx
                    .iter()
                    .map(|sy| &cache.iter().find(|(k, _)| k == sy).unwrap().1)
                    .collect();
                for x in 0..nw {
                    for c in 0..4 {
                        let v: f32 = (0..4).map(|k| rows4[k][x * 4 + c] * wts[k]).sum();
                        row[x * 4 + c] = (v + 0.5).clamp(0.0, 255.0) as u8;
                    }
                }
            }
        });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn psnr(a: &[u8], b: &[u8], channels: &[usize]) -> f64 {
        let mut se = 0.0f64;
        let mut n = 0usize;
        for (pa, pb) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
            for &c in channels {
                let d = pa[c] as f64 - pb[c] as f64;
                se += d * d;
                n += 1;
            }
        }
        if se == 0.0 {
            return 99.0;
        }
        10.0 * (255.0f64 * 255.0 / (se / n as f64)).log10()
    }

    fn gradient(w: u32, h: u32) -> Vec<u8> {
        let mut v = Vec::new();
        for y in 0..h {
            for x in 0..w {
                v.extend_from_slice(&[
                    (x * 255 / (w - 1)) as u8,
                    (y * 255 / (h - 1)) as u8,
                    ((x + y) * 127 / (w + h - 2)) as u8,
                    ((x ^ y) & 0xFF) as u8,
                ]);
            }
        }
        v
    }

    /// A smooth dark grey with a little noise (a dashboard's plastic, #845) keeps each
    /// block's mean colour: fitted along the noise, blocks went green or blue by up to five
    /// levels and the panel looked grainy.
    #[test]
    fn smooth_dark_blocks_keep_their_colour() {
        let (w, h) = (64u32, 64u32);
        let mut s = 12345u32;
        let mut rgba = Vec::new();
        for _ in 0..w * h {
            s = s.wrapping_mul(1664525).wrapping_add(1013904223);
            let v = (31 + (s >> 24) % 7) as u8;
            rgba.extend_from_slice(&[v, v, v + 1, 255]);
        }
        let (blk, _, _) = encode(&rgba, w, h, Bc::Bc1 { punch: false });
        let back = decode(&blk, w, h, Bc::Bc1 { punch: false });
        let mut shifted = 0;
        for by in 0..(h / 4) as usize {
            for bx in 0..(w / 4) as usize {
                let (mut a, mut b) = ([0i32; 3], [0i32; 3]);
                for y in 0..4 {
                    for x in 0..4 {
                        let o = ((by * 4 + y) * w as usize + bx * 4 + x) * 4;
                        for c in 0..3 {
                            a[c] += rgba[o + c] as i32;
                            b[c] += back[o + c] as i32;
                        }
                    }
                }
                if (0..3).any(|c| (a[c] - b[c]).abs() > 16 * 5 / 2) {
                    shifted += 1;
                }
            }
        }
        assert!(shifted <= 4, "{shifted} of 256 blocks changed colour");
        assert!(psnr(&rgba, &back, &[0, 1, 2]) > 40.0);
    }

    #[test]
    fn flat_blocks_are_exact_or_nearly() {
        for c in [
            [0u8, 0, 0],
            [255, 255, 255],
            [47, 74, 83],
            [128, 64, 200],
            [13, 250, 7],
        ] {
            let rgba: Vec<u8> = (0..16).flat_map(|_| [c[0], c[1], c[2], 255]).collect();
            let (blk, _, _) = encode(&rgba, 4, 4, Bc::Bc1 { punch: false });
            let back = decode(&blk, 4, 4, Bc::Bc1 { punch: false });
            for p in back.chunks_exact(4) {
                for k in 0..3 {
                    assert!((p[k] as i32 - c[k] as i32).abs() <= 1, "{c:?} -> {p:?}");
                }
                assert_eq!(p[3], 255);
            }
        }
    }

    #[test]
    fn gradients_keep_their_quality() {
        let rgba = gradient(64, 64);
        let (blk, _, _) = encode(&rgba, 64, 64, Bc::Bc1 { punch: false });
        let back = decode(&blk, 64, 64, Bc::Bc1 { punch: false });
        let p = psnr(&rgba, &back, &[0, 1, 2]);
        assert!(p > 36.0, "BC1 gradient PSNR {p:.1}");
        let (blk, _, _) = encode(&rgba, 64, 64, Bc::Bc3);
        let back = decode(&blk, 64, 64, Bc::Bc3);
        let pa = psnr(&rgba, &back, &[3]);
        assert!(psnr(&rgba, &back, &[0, 1, 2]) > 36.0);
        assert!(pa > 20.0, "BC3 noisy alpha PSNR {pa:.1}");
    }

    #[test]
    fn punch_through_alpha_survives() {
        let mut rgba = gradient(8, 8);
        for (i, p) in rgba.chunks_exact_mut(4).enumerate() {
            p[3] = if i % 3 == 0 { 0 } else { 255 };
            if p[3] == 0 {
                p[0] = 0;
                p[1] = 0;
                p[2] = 0;
            }
        }
        let (blk, _, _) = encode(&rgba, 8, 8, Bc::Bc1 { punch: true });
        let back = decode(&blk, 8, 8, Bc::Bc1 { punch: true });
        for (a, b) in rgba.chunks_exact(4).zip(back.chunks_exact(4)) {
            assert_eq!(a[3] == 0, b[3] == 0);
        }
        assert!(blk.chunks_exact(8).any(bc1_block_has_alpha));
    }

    #[test]
    fn cutout_alpha_is_kept_by_bc3() {
        let mut rgba = gradient(16, 16);
        for (i, p) in rgba.chunks_exact_mut(4).enumerate() {
            p[3] = match i % 5 {
                0 => 0,
                1 => 255,
                2 => 128,
                3 => 255,
                _ => 0,
            };
        }
        let (blk, _, _) = encode(&rgba, 16, 16, Bc::Bc3);
        let back = decode(&blk, 16, 16, Bc::Bc3);
        for (a, b) in rgba.chunks_exact(4).zip(back.chunks_exact(4)) {
            assert!(
                (a[3] as i32 - b[3] as i32).abs() <= 4,
                "{} -> {}",
                a[3],
                b[3]
            );
        }
    }

    #[test]
    fn small_levels_encode() {
        for (w, h) in [(1u32, 1u32), (2, 1), (2, 2), (3, 5)] {
            let rgba = vec![200u8; (w * h * 4) as usize];
            let (blk, _, _) = encode(&rgba, w, h, Bc::Bc3);
            assert_eq!(blk.len(), (w.div_ceil(4) * h.div_ceil(4) * 16) as usize);
            let back = decode(&blk, w, h, Bc::Bc3);
            assert!(back.iter().all(|v| (*v as i32 - 200).abs() <= 2));
        }
    }

    #[test]
    fn downsample_is_linear_light() {
        // black and white average to sRGB 188 in linear light, not 128
        let rgba = [
            0u8, 0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 255, 255, 255, 255, 255,
        ];
        let (d, w, h) = downsample(&rgba, 2, 2);
        assert_eq!((w, h), (1, 1));
        assert!((d[0] as i32 - 188).abs() <= 1, "{}", d[0]);
        assert_eq!(d[3], 255);
    }

    #[test]
    fn resize_by_a_texel_keeps_the_picture() {
        // a smooth picture 2 texels wider: the texels land where they were
        let (w, h) = (130u32, 64u32);
        let src: Vec<u8> = (0..w * h)
            .flat_map(|i| {
                [
                    ((i % w) * 255 / (w - 1)) as u8,
                    ((i / w) * 4) as u8,
                    90,
                    255,
                ]
            })
            .collect();
        let out = resize(&src, w, h, 132, 64);
        assert_eq!(out.len(), 132 * 64 * 4);
        for y in [0usize, 30, 63] {
            for x in [0usize, 40, 131] {
                let u = (x as f32 + 0.5) / 132.0 * w as f32 - 0.5;
                let want = (u.clamp(0.0, (w - 1) as f32) * 255.0 / (w - 1) as f32).round();
                let got = out[(y * 132 + x) * 4] as f32;
                assert!((got - want).abs() <= 2.0, "({x}, {y}): {got} vs {want}");
                assert_eq!(out[(y * 132 + x) * 4 + 3], 255);
            }
        }
        // the same size is a copy
        assert_eq!(resize(&src, w, h, w, h), src);
    }

    fn old_block_patch_shift(src: &[[u8; 4]; 16], dec: &[[u8; 4]; 16]) -> f64 {
        let (mut seen, mut patches, mut covered, mut block_worst) = (0u16, 0, 0, 0.0f64);
        for i in 0..16 {
            if seen & (1 << i) != 0 {
                continue;
            }
            let (mut n, mut sum) = (0u32, [0u32; 3]);
            for j in i..16 {
                if src[j][..3] == src[i][..3] {
                    seen |= 1 << j;
                    n += 1;
                    for c in 0..3 {
                        sum[c] += dec[j][c] as u32;
                    }
                }
            }
            if n >= 2 {
                patches += 1;
                covered += n;
                let e: f64 = (0..3)
                    .map(|c| {
                        let d = sum[c] as f64 / n as f64 - src[i][c] as f64;
                        d * d * WEIGHTS[c] as f64
                    })
                    .sum();
                block_worst = block_worst.max(e);
            }
        }
        if patches >= 3 && covered >= 12 {
            block_worst
        } else {
            0.0
        }
    }

    fn old_worst_patch_shift(src: &[u8], dec: &[u8], width: u32, height: u32) -> f64 {
        let (w, h) = (width as usize, height as usize);
        let mut worst = 0.0f64;
        for by in 0..h / 4 {
            for bx in 0..w / 4 {
                worst = worst.max(old_block_patch_shift(
                    &gather(src, w, h, bx, by),
                    &gather(dec, w, h, bx, by),
                ));
            }
        }
        worst
    }

    struct Rng(u32);

    impl Rng {
        fn next(&mut self) -> u32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 17;
            self.0 ^= self.0 << 5;
            self.0
        }
    }

    fn swatch_picture(rng: &mut Rng, w: u32, h: u32) -> Vec<u8> {
        let palette: Vec<[u8; 4]> = (0..24)
            .map(|_| rng.next().to_le_bytes())
            .map(|c| [c[0], c[1], c[2], if c[3] > 40 { 255 } else { c[3] }])
            .collect();
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for by in 0..h / 4 {
            for bx in 0..w / 4 {
                let style = rng.next() % 6;
                let colours = 1 + rng.next() as usize % 6;
                let base = rng.next() as usize % palette.len();
                for i in 0..16u32 {
                    let (x, y) = (bx * 4 + i % 4, by * 4 + i / 4);
                    let p = match style {
                        0 => [x as u8, y as u8, (x ^ y) as u8, 255],
                        1 => rng.next().to_le_bytes(),
                        2 => palette[base],
                        3 => palette[(base + (i % 4) as usize % colours) % palette.len()],
                        4 => palette[(base + rng.next() as usize % colours) % palette.len()],
                        _ => {
                            let mut c =
                                palette[(base + (i / 4) as usize % colours) % palette.len()];
                            c[0] = c[0].wrapping_add(rng.next().is_multiple_of(3) as u8);
                            c
                        }
                    };
                    rgba[((y * w + x) * 4) as usize..][..4].copy_from_slice(&p);
                }
            }
        }
        rgba
    }

    #[test]
    fn swatch_check_is_unchanged() {
        let mut rng = Rng(0x9E37_79B9);
        let mut swatch_blocks = 0;
        for _ in 0..200_000 {
            let colours = 1 + rng.next() as usize % 7;
            let palette: Vec<[u8; 4]> = (0..colours).map(|_| rng.next().to_le_bytes()).collect();
            let src: [[u8; 4]; 16] = std::array::from_fn(|_| {
                let mut c = palette[rng.next() as usize % colours];
                c[3] = rng.next() as u8;
                c
            });
            let noisy = rng.next().is_multiple_of(2);
            let dec: [[u8; 4]; 16] = std::array::from_fn(|i| {
                if noisy {
                    rng.next().to_le_bytes()
                } else {
                    src[i].map(|v| v.saturating_add(rng.next() as u8 % 9))
                }
            });
            let old = old_block_patch_shift(&src, &dec);
            assert_eq!(
                block_patch_shift(&src, &dec).to_bits(),
                old.to_bits(),
                "{src:?} {dec:?}"
            );
            swatch_blocks += (old > 0.0) as u32;
        }
        assert!(swatch_blocks > 10_000, "{swatch_blocks}");
        for (w, h) in [(64u32, 64u32), (256, 128), (36, 20), (6, 9)] {
            let src = swatch_picture(&mut rng, w, h);
            for format in [
                Bc::Bc1 { punch: false },
                Bc::Bc1 { punch: true },
                Bc::Bc3,
                Bc::Bc2,
            ] {
                let (blocks, _, _) = encode(&src, w, h, format);
                let dec = decode(&blocks, w, h, format);
                let old = old_worst_patch_shift(&src, &dec, w, h);
                assert_eq!(worst_patch_shift(&src, &dec, w, h).to_bits(), old.to_bits());
                assert_eq!(
                    worst_block_patch_shift(&src, &blocks, w, h, format).to_bits(),
                    old.to_bits()
                );
                if w >= 64 {
                    assert!(old > 0.0);
                }
            }
        }
    }

    #[test]
    fn swatch_check_speed() {
        let (w, h) = (1024u32, 1024u32);
        let src = swatch_picture(&mut Rng(12345), w, h);
        let (blocks, _, _) = encode(&src, w, h, Bc::Bc1 { punch: false });
        let t = std::time::Instant::now();
        let dec = decode(&blocks, w, h, Bc::Bc1 { punch: false });
        let old = old_worst_patch_shift(&src, &dec, w, h);
        let before = t.elapsed().as_secs_f64();
        let t = std::time::Instant::now();
        let new = worst_block_patch_shift(&src, &blocks, w, h, Bc::Bc1 { punch: false });
        let after = t.elapsed().as_secs_f64();
        assert_eq!(old.to_bits(), new.to_bits());
        eprintln!(
            "swatch check of a 1024x1024 BC1 picture: decode + check {:.1} ms before, {:.1} ms now",
            before * 1e3,
            after * 1e3
        );
    }

    #[test]
    fn encoder_speed() {
        // a noisy 1024² picture: about a tenth of a second per core is the budget
        let (w, h) = (1024u32, 1024u32);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        let mut s = 12345u32;
        for (i, p) in rgba.chunks_exact_mut(4).enumerate() {
            s = s.wrapping_mul(1664525).wrapping_add(1013904223);
            let x = (i as u32 % w) as u8;
            p.copy_from_slice(&[x, (s >> 24) as u8 / 4 + x / 2, 90, 255]);
        }
        let t = std::time::Instant::now();
        let (blk, _, _) = encode(&rgba, w, h, Bc::Bc1 { punch: false });
        let secs = t.elapsed().as_secs_f64();
        let back = decode(&blk, w, h, Bc::Bc1 { punch: false });
        eprintln!(
            "BC1 1024x1024 in {:.3} s (all cores), PSNR {:.1}",
            secs,
            psnr(&rgba, &back, &[0, 1, 2])
        );
    }
}
