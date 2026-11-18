//! The noise the enhanced clouds are built from, made once at start-up: the method of
//! Guerrilla's Horizon Zero Dawn clouds as bevy-volumetric-clouds (MIT, evroon) and
//! Frostbite's TileableVolumeNoise build it.
//!
//! * the **shape map** (2-D, tiling): R a Perlin fbm cut by Worley cells - where the heaps
//!   stand and how dense they are; G the coverage the heap needs before it appears there
//!   (three octaves of Worley, stored + 1); B how much its top is rounded off;
//! * the **detail volume** (3-D, tiling): Worley noise of three frequencies that eats the
//!   heaps' edges into billows.
//!
//! Both repeat seamlessly (every lattice is taken modulo its frequency), and both come with
//! a mip chain (a box filter) so that far clouds do not shimmer.

/// Edge of the shape map in texels.
pub const SHAPE_SIZE: u32 = 256;
/// Edge of the detail volume in texels.
pub const DETAIL_SIZE: u32 = 64;

fn fract(x: f32) -> f32 {
    x - x.floor()
}

fn modf(a: f32, b: f32) -> f32 {
    a - b * (a / b).floor()
}

/// Hash without sine (Dave Hoskins), 3 → 1.
fn hash13(p: [f32; 3], k: f32) -> f32 {
    let mut p3 = [fract(p[0] * k), fract(p[1] * k), fract(p[2] * k)];
    let d = p3[0] * (p3[1] + 19.19) + p3[1] * (p3[2] + 19.19) + p3[2] * (p3[0] + 19.19);
    p3 = [p3[0] + d, p3[1] + d, p3[2] + d];
    fract((p3[0] + p3[1]) * p3[2])
}

/// Value noise on a lattice that repeats every `tile` cells.
fn value_noise(x: [f32; 3], tile: f32) -> f32 {
    let p = [x[0].floor(), x[1].floor(), x[2].floor()];
    let f = [fract(x[0]), fract(x[1]), fract(x[2])];
    let u = f.map(|f| f * f * (3.0 - 2.0 * f));
    let h = |dx: f32, dy: f32, dz: f32| {
        hash13([modf(p[0] + dx, tile), modf(p[1] + dy, tile), modf(p[2] + dz, tile)], 0.1031)
    };
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let x00 = lerp(h(0.0, 0.0, 0.0), h(1.0, 0.0, 0.0), u[0]);
    let x10 = lerp(h(0.0, 1.0, 0.0), h(1.0, 1.0, 0.0), u[0]);
    let x01 = lerp(h(0.0, 0.0, 1.0), h(1.0, 0.0, 1.0), u[0]);
    let x11 = lerp(h(0.0, 1.0, 1.0), h(1.0, 1.0, 1.0), u[0]);
    lerp(lerp(x00, x10, u[1]), lerp(x01, x11, u[1]), u[2])
}

/// Worley (cellular) noise, 1 at a cell's point falling off with the squared distance,
/// on cells that repeat every `tile`.
fn worley(x: [f32; 3], tile: f32) -> f32 {
    let p = [x[0].floor(), x[1].floor(), x[2].floor()];
    let f = [fract(x[0]), fract(x[1]), fract(x[2])];
    let mut best = 100.0f32;
    for k in -1..=1 {
        for j in -1..=1 {
            for i in -1..=1 {
                let b = [i as f32, j as f32, k as f32];
                let c = [modf(p[0] + b[0], tile), modf(p[1] + b[1], tile), modf(p[2] + b[2], tile)];
                // the cell's point, one hash per axis
                let o = [hash13(c, 1031.1031), hash13([c[1] + 7.3, c[2], c[0]], 1031.1031), hash13([c[2] + 3.1, c[0], c[1]], 1031.1031)];
                let r = [b[0] - f[0] + o[0], b[1] - f[1] + o[1], b[2] - f[2] + o[2]];
                best = best.min(r[0] * r[0] + r[1] * r[1] + r[2] * r[2]);
            }
        }
    }
    1.0 - best
}

fn fbm(p: [f32; 3], octaves: u32, freq: f32, noise: fn([f32; 3], f32) -> f32) -> f32 {
    let (mut f, mut amp, mut sum, mut w) = (freq, 1.0, 0.0, 0.0);
    for _ in 0..octaves {
        sum += amp * noise([p[0] * f, p[1] * f, p[2] * f], f);
        w += amp;
        f *= 2.0;
        amp *= 0.5;
    }
    sum / w
}

fn shape_texel(u: f32, v: f32) -> [f32; 3] {
    let c = [u, v, 0.5];
    let perlin = fbm(c, 7, 4.0, value_noise);
    let cells = fbm(c, 4, 8.0, worley);
    let r = (1.0 + (perlin - 1.0) * 0.9) * (1.0 + (cells - 1.0) * 0.7);
    let g = 0.625 * fbm(c, 3, 15.0, worley) + 0.25 * fbm(c, 3, 19.0, worley) + 0.125 * fbm(c, 3, 23.0, worley) - 1.0;
    let b = 1.0 - fbm([c[0] + 0.5, c[1] + 0.5, c[2] + 0.5], 4, 9.0, worley);
    [r, g + 1.0, b]
}

fn detail_texel(x: f32, y: f32, z: f32) -> f32 {
    let c = [x, y, z];
    let r = fbm(c, 3, 3.0, worley);
    let g = fbm(c, 3, 8.0, worley);
    let b = fbm(c, 3, 16.0, worley);
    (1.0 - (r + g * 0.5 + b * 0.25) / 1.75).max(0.0)
}

fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// Rows of `n` computed on as many threads as the machine has.
fn parallel_rows<T: Send + Clone + Default>(n: usize, row_len: usize, f: impl Fn(usize, &mut [T]) + Sync) -> Vec<T> {
    let mut out = vec![T::default(); n * row_len];
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).max(1);
    let per = n.div_ceil(threads);
    std::thread::scope(|s| {
        for (k, chunk) in out.chunks_mut(per * row_len).enumerate() {
            let f = &f;
            s.spawn(move || {
                for (i, row) in chunk.chunks_mut(row_len).enumerate() {
                    f(k * per + i, row);
                }
            });
        }
    });
    out
}

/// The shape map's mip chain, RGBA8, level 0 first.
pub fn shape_map() -> Vec<Vec<u8>> {
    let n = SHAPE_SIZE as usize;
    let base = parallel_rows::<u8>(n, n * 4, |y, row| {
        for x in 0..n {
            let t = shape_texel((x as f32 + 0.5) / n as f32, (y as f32 + 0.5) / n as f32);
            row[x * 4..x * 4 + 4].copy_from_slice(&[to_u8(t[0]), to_u8(t[1]), to_u8(t[2]), 255]);
        }
    });
    let mut levels = vec![base];
    let mut size = n;
    while size > 1 {
        let prev = levels.last().expect("level");
        let half = size / 2;
        let mut next = vec![0u8; half * half * 4];
        for y in 0..half {
            for x in 0..half {
                for c in 0..4 {
                    let s: u32 = [(0, 0), (1, 0), (0, 1), (1, 1)]
                        .iter()
                        .map(|(dx, dy)| prev[((2 * y + dy) * size + 2 * x + dx) * 4 + c] as u32)
                        .sum();
                    next[(y * half + x) * 4 + c] = ((s + 2) / 4) as u8;
                }
            }
        }
        levels.push(next);
        size = half;
    }
    levels
}

/// The detail volume's mip chain, R8, level 0 first (z slices of y rows of x).
pub fn detail_volume() -> Vec<Vec<u8>> {
    let n = DETAIL_SIZE as usize;
    let base = parallel_rows::<u8>(n * n, n, |zy, row| {
        let (z, y) = (zy / n, zy % n);
        for (x, v) in row.iter_mut().enumerate() {
            *v = to_u8(detail_texel((x as f32 + 0.5) / n as f32, (y as f32 + 0.5) / n as f32, (z as f32 + 0.5) / n as f32));
        }
    });
    let mut levels = vec![base];
    let mut size = n;
    while size > 1 {
        let prev = levels.last().expect("level");
        let half = size / 2;
        let mut next = vec![0u8; half * half * half];
        for z in 0..half {
            for y in 0..half {
                for x in 0..half {
                    let mut s = 0u32;
                    for d in 0..8 {
                        let (dx, dy, dz) = (d & 1, (d >> 1) & 1, d >> 2);
                        s += prev[((2 * z + dz) * size + 2 * y + dy) * size + 2 * x + dx] as u32;
                    }
                    next[(z * half + y) * half + x] = ((s + 4) / 8) as u8;
                }
            }
        }
        levels.push(next);
        size = half;
    }
    levels
}

/// Deterministic cloud data, shared by renderer rebuilds in this process. The disk copy
/// also avoids regenerating hundreds of thousands of Worley texels on the next start.
pub struct Noise {
    pub shape: Vec<Vec<u8>>,
    pub detail: Vec<Vec<u8>>,
}

const CACHE_HEADER: &[u8; 8] = b"OMSCN001"; // bump when the noise algorithm changes

fn levels_size(size: u32, dimensions: u32, bpp: usize) -> usize {
    (0..=size.trailing_zeros()).map(|m| ((size >> m) as usize).pow(dimensions) * bpp).sum()
}

fn cache_size() -> usize {
    16 + levels_size(SHAPE_SIZE, 2, 4) + levels_size(DETAIL_SIZE, 3, 1)
}

fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, b| (h ^ *b as u64).wrapping_mul(0x100000001b3))
}

fn decode_cache(bytes: &[u8]) -> Option<Noise> {
    if bytes.len() != cache_size() || &bytes[..8] != CACHE_HEADER {
        return None;
    }
    let expected = u64::from_le_bytes(bytes[8..16].try_into().ok()?);
    if checksum(&bytes[16..]) != expected { return None; }
    let mut at = 16;
    let mut read = |size: u32, dimensions: u32, bpp: usize| {
        (0..=size.trailing_zeros()).map(|m| {
            let len = ((size >> m) as usize).pow(dimensions) * bpp;
            let level = bytes[at..at + len].to_vec();
            at += len;
            level
        }).collect()
    };
    Some(Noise { shape: read(SHAPE_SIZE, 2, 4), detail: read(DETAIL_SIZE, 3, 1) })
}

fn encode_cache(noise: &Noise) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(cache_size());
    bytes.extend_from_slice(CACHE_HEADER);
    bytes.extend_from_slice(&[0; 8]);
    for level in noise.shape.iter().chain(&noise.detail) { bytes.extend_from_slice(level); }
    let hash = checksum(&bytes[16..]);
    bytes[8..16].copy_from_slice(&hash.to_le_bytes());
    bytes
}

fn cache_path() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    let home = std::path::PathBuf::from(home);
    // An unavailable home or cache is harmless; do not write into the game/mod folders.
    home.is_absolute().then(|| home.join(".openomsi/cache/cloud-noise-v1.bin"))
}

fn load_noise(path: Option<&std::path::Path>) -> Noise {
    if let Some(path) = path {
        // Bound reads before allocating: an unrelated/corrupt file cannot exhaust memory.
        if std::fs::metadata(path).ok().is_some_and(|m| m.len() == cache_size() as u64) {
            if let Some(noise) = std::fs::read(path).ok().and_then(|b| decode_cache(&b)) {
                return noise;
            }
        }
        let _ = std::fs::remove_file(path); // a bad copy must be replaceable on Windows too
    }
    let (shape, detail) = std::thread::scope(|s| {
        let a = s.spawn(shape_map);
        let b = s.spawn(detail_volume);
        (a.join().expect("cloud shape"), b.join().expect("cloud detail"))
    });
    let noise = Noise { shape, detail };
    if let Some(path) = path { save_noise(path, &noise); }
    noise
}

fn save_noise(path: &std::path::Path, noise: &Noise) {
    use std::io::Write;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let Some(parent) = path.parent() else { return };
    if std::fs::create_dir_all(parent).is_err() { return; }
    let temp = path.with_extension(format!("{}-{}.tmp", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    let result = (|| -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&temp)?;
        file.write_all(&encode_cache(noise))?;
        drop(file);
        // A competing launcher/game may have filled it first. Either complete copy works.
        std::fs::rename(&temp, path)
    })();
    if result.is_err() { let _ = std::fs::remove_file(temp); }
}

pub fn noise() -> &'static Noise {
    static NOISE: std::sync::OnceLock<Noise> = std::sync::OnceLock::new();
    NOISE.get_or_init(|| load_noise(cache_path().as_deref()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_round_trip_rejects_stale_truncated_and_corrupt_data() {
        let generated = load_noise(None);
        let bytes = encode_cache(&generated);
        assert_eq!(bytes.len(), cache_size());
        let loaded = decode_cache(&bytes).unwrap();
        assert_eq!(loaded.shape, generated.shape);
        assert_eq!(loaded.detail, generated.detail);
        assert!(decode_cache(&bytes[..bytes.len() - 1]).is_none());
        let mut bad = bytes.clone();
        bad[7] ^= 1;
        assert!(decode_cache(&bad).is_none());
        let mut bad = bytes.clone();
        bad[100] ^= 1;
        assert!(decode_cache(&bad).is_none());
        let path = std::env::temp_dir().join(format!("omsi-cloud-cache-{}.bin", std::process::id()));
        save_noise(&path, &generated);
        let disk = load_noise(Some(&path));
        assert_eq!(disk.shape, generated.shape);
        assert_eq!(disk.detail, generated.detail);
        std::fs::write(&path, b"interrupted write").unwrap();
        let recovered = load_noise(Some(&path));
        assert_eq!(recovered.shape, generated.shape);
        assert_eq!(recovered.detail, generated.detail);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn noise_tiles_and_spans_its_range() {
        // the lattices repeat: one period on, the same value
        let a = worley([0.3 * 8.0, 0.7 * 8.0, 4.0], 8.0);
        let b = worley([0.3 * 8.0 + 8.0, 0.7 * 8.0, 4.0], 8.0);
        assert!((a - b).abs() < 1e-5);
        let a = value_noise([1.3, 2.7, 0.5], 4.0);
        let b = value_noise([5.3, 2.7, 0.5], 4.0);
        assert!((a - b).abs() < 1e-5);
        let shape = shape_map();
        assert_eq!(shape.len(), SHAPE_SIZE.trailing_zeros() as usize + 1);
        let r: Vec<u8> = shape[0].chunks(4).map(|p| p[0]).collect();
        let (lo, hi) = (r.iter().min().copied().unwrap_or(0), r.iter().max().copied().unwrap_or(0));
        assert!(hi - lo > 120, "shape R spans {lo}..{hi}");
    }
}
