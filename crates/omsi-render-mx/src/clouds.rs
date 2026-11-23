//! The noise the enhanced clouds are built from, made once at start-up the way Guerrilla's
//! Horizon Zero Dawn and Nubis clouds (and Frostbite's TileableVolumeNoise) build theirs:
//!
//! * the **weather map** (2-D, RGBA8): R where the clouds stand - a Perlin-Worley field,
//!   equalised so that cutting it at `1 - cover` leaves that share of the sky under cloud;
//!   G how the cloud type varies (heaps or a flatter sheet); B how tall a heap grows; A a finer
//!   Worley field of small heaps between the big ones;
//! * the **base volume** (3-D, R8, 128³): Perlin-Worley noise eroded by three octaves of
//!   Worley - the billowing body of every cloud;
//! * the **detail volume** (3-D, R8, 32³): Worley octaves that eat the edges into wisps
//!   (under a cloud) and cauliflower (on its top);
//! * the **curl map** (2-D, RG8): the curl of a Perlin field, which twists the detail so
//!   that the edges look torn by the wind rather than stamped from a grid.
//!
//! Everything repeats seamlessly (every lattice is taken modulo its frequency) and comes
//! with a mip chain (a box filter) so that far clouds do not shimmer.

/// Edge of the weather map in texels.
pub const WEATHER_SIZE: u32 = 512;
/// Edge of the base volume in texels.
pub const BASE_SIZE: u32 = 128;
/// Edge of the detail volume in texels.
pub const DETAIL_SIZE: u32 = 32;
/// Edge of the curl map in texels.
pub const CURL_SIZE: u32 = 128;

/// An integer hash (lowbias32) of a lattice point and a seed, 0..1.
fn hash(x: i32, y: i32, z: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343)
        ^ (y as u32).wrapping_mul(0xd816_3841)
        ^ (z as u32).wrapping_mul(0xcb1a_b31f)
        ^ seed.wrapping_mul(0x9e37_79b9);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846c_a68b);
    h ^= h >> 16;
    (h & 0xff_ffff) as f32 / 16_777_216.0
}

fn wrap(i: i32, n: i32) -> i32 {
    i.rem_euclid(n)
}

fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Gradient (Perlin) noise on a lattice of `n` cells a side over the unit cube, about -1..1.
fn perlin(p: [f32; 3], n: i32, seed: u32) -> f32 {
    let q = [p[0] * n as f32, p[1] * n as f32, p[2] * n as f32];
    let i = [
        q[0].floor() as i32,
        q[1].floor() as i32,
        q[2].floor() as i32,
    ];
    let f = [
        q[0] - q[0].floor(),
        q[1] - q[1].floor(),
        q[2] - q[2].floor(),
    ];
    let grad = |dx: i32, dy: i32, dz: i32| {
        let (x, y, z) = (wrap(i[0] + dx, n), wrap(i[1] + dy, n), wrap(i[2] + dz, n));
        // one of the twelve edges of a cube
        const G: [[f32; 3]; 12] = [
            [1.0, 1.0, 0.0],
            [-1.0, 1.0, 0.0],
            [1.0, -1.0, 0.0],
            [-1.0, -1.0, 0.0],
            [1.0, 0.0, 1.0],
            [-1.0, 0.0, 1.0],
            [1.0, 0.0, -1.0],
            [-1.0, 0.0, -1.0],
            [0.0, 1.0, 1.0],
            [0.0, -1.0, 1.0],
            [0.0, 1.0, -1.0],
            [0.0, -1.0, -1.0],
        ];
        let g = G[(hash(x, y, z, seed) * 12.0) as usize % 12];
        g[0] * (f[0] - dx as f32) + g[1] * (f[1] - dy as f32) + g[2] * (f[2] - dz as f32)
    };
    let u = [fade(f[0]), fade(f[1]), fade(f[2])];
    let x00 = lerp(grad(0, 0, 0), grad(1, 0, 0), u[0]);
    let x10 = lerp(grad(0, 1, 0), grad(1, 1, 0), u[0]);
    let x01 = lerp(grad(0, 0, 1), grad(1, 0, 1), u[0]);
    let x11 = lerp(grad(0, 1, 1), grad(1, 1, 1), u[0]);
    lerp(lerp(x00, x10, u[1]), lerp(x01, x11, u[1]), u[2])
}

/// Perlin octaves from `n` cells up, mapped to 0..1.
fn perlin_fbm(p: [f32; 3], n: i32, octaves: u32, seed: u32) -> f32 {
    let (mut sum, mut amp, mut w, mut k) = (0.0, 1.0, 0.0, n);
    for o in 0..octaves {
        sum += amp * perlin(p, k, seed + o);
        w += amp;
        amp *= 0.5;
        k *= 2;
    }
    (0.5 + 0.5 * sum / w * 1.6).clamp(0.0, 1.0)
}

/// Worley (cellular) noise: 1 at a cell's point, falling off with the distance to the
/// nearest (in cells), on `n` cells a side that repeat; `flat` for a 2-D field (z ignored).
fn worley(p: [f32; 3], n: i32, seed: u32, flat: bool) -> f32 {
    let q = [p[0] * n as f32, p[1] * n as f32, p[2] * n as f32];
    let i = [
        q[0].floor() as i32,
        q[1].floor() as i32,
        q[2].floor() as i32,
    ];
    let f = [
        q[0] - q[0].floor(),
        q[1] - q[1].floor(),
        q[2] - q[2].floor(),
    ];
    let mut best = 9.0f32;
    let zr = if flat { 0..=0 } else { -1..=1 };
    for dz in zr {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (x, y, z) = (wrap(i[0] + dx, n), wrap(i[1] + dy, n), wrap(i[2] + dz, n));
                let o = [
                    hash(x, y, z, seed),
                    hash(x, y, z, seed + 101),
                    if flat {
                        f[2]
                    } else {
                        hash(x, y, z, seed + 202)
                    },
                ];
                let r = [
                    dx as f32 + o[0] - f[0],
                    dy as f32 + o[1] - f[1],
                    dz as f32 + o[2] - f[2],
                ];
                best = best.min(r[0] * r[0] + r[1] * r[1] + r[2] * r[2]);
            }
        }
    }
    (1.0 - best.sqrt()).clamp(0.0, 1.0)
}

/// Three octaves of Worley from `n` cells up (0.625, 0.25, 0.125).
fn worley_fbm(p: [f32; 3], n: i32, seed: u32, flat: bool) -> f32 {
    worley(p, n, seed, flat) * 0.625
        + worley(p, n * 2, seed + 7, flat) * 0.25
        + worley(p, n * 4, seed + 13, flat) * 0.125
}

fn remap(v: f32, lo: f32, hi: f32, a: f32, b: f32) -> f32 {
    a + (v - lo) / (hi - lo).max(1e-6) * (b - a)
}

/// The base volume's texel: Perlin-Worley (the Perlin billows with the Worley cells' rounded
/// tops), cut by the low-frequency Worley octaves.
fn base_texel(p: [f32; 3]) -> f32 {
    // (one Worley field per frequency, each octave sum taken from them: the cells' fbm from 4
    // cells up, and the erosion's three fbms from 4, 8 and 16 up)
    let w = [4, 8, 16, 32, 64].map(|n| worley(p, n, 11 + n as u32, false));
    let fbm = |k: usize| w[k] * 0.625 + w[k + 1] * 0.25 + w[k + 2] * 0.125;
    let perlin = perlin_fbm(p, 4, 4, 1);
    let pw = remap(perlin, 0.0, 1.0, fbm(0), 1.0);
    let low = fbm(0) * 0.625 + fbm(1) * 0.25 + fbm(2) * 0.125;
    remap(pw, low - 1.0, 1.0, 0.0, 1.0).clamp(0.0, 1.0)
}

fn detail_texel(p: [f32; 3]) -> f32 {
    let w = [2, 4, 8, 16, 32].map(|n| worley(p, n, 51 + n as u32, false));
    let fbm = |k: usize| w[k] * 0.625 + w[k + 1] * 0.25 + w[k + 2] * 0.125;
    fbm(0) * 0.625 + fbm(1) * 0.25 + fbm(2) * 0.125
}

fn weather_texel(u: f32, v: f32) -> [f32; 4] {
    let p = [u, v, 0.5];
    let perlin = perlin_fbm(p, 3, 6, 81);
    let cells = worley_fbm(p, 6, 91, true);
    let field = remap(perlin, 0.0, 1.0, cells, 1.0);
    let kind = perlin_fbm(p, 2, 3, 101);
    let tall = perlin_fbm(p, 5, 3, 111);
    let small = worley_fbm(p, 24, 121, true);
    [field, kind, tall, small]
}

/// Stretch values so that their 1st and 99th percentiles span 0..1.
fn stretch(v: &mut [f32]) {
    let mut s: Vec<f32> = v.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    let (lo, hi) = (s[s.len() / 100], s[s.len() * 99 / 100]);
    for x in v.iter_mut() {
        *x = ((*x - lo) / (hi - lo).max(1e-6)).clamp(0.0, 1.0);
    }
}

/// Replace values by their rank (0..1): an even spread, so that a threshold at `1 - c`
/// keeps a share `c`.
fn equalise(v: &mut [f32]) {
    let mut idx: Vec<usize> = (0..v.len()).collect();
    idx.sort_by(|a, b| v[*a].total_cmp(&v[*b]));
    let n = v.len() as f32;
    for (rank, i) in idx.into_iter().enumerate() {
        v[i] = (rank as f32 + 0.5) / n;
    }
}

fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// `n` values computed on as many threads as the machine has.
fn parallel<T: Send + Copy + Default>(n: usize, f: impl Fn(usize) -> T + Sync) -> Vec<T> {
    let mut out = vec![T::default(); n];
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .max(1);
    let per = n.div_ceil(threads);
    std::thread::scope(|s| {
        for (k, chunk) in out.chunks_mut(per).enumerate() {
            let f = &f;
            s.spawn(move || {
                for (i, v) in chunk.iter_mut().enumerate() {
                    *v = f(k * per + i);
                }
            });
        }
    });
    out
}

/// A box-filtered mip chain of a square (`dims` 2) or cubic (3) picture of `bpp` channels.
fn mips(base: Vec<u8>, size: usize, dims: u32, bpp: usize) -> Vec<Vec<u8>> {
    let mut levels = vec![base];
    let mut size = size;
    while size > 1 {
        let prev = levels.last().expect("level");
        let half = size / 2;
        let depth = if dims == 3 { half } else { 1 };
        let mut next = vec![0u8; half * half * depth * bpp];
        for z in 0..depth {
            for y in 0..half {
                for x in 0..half {
                    for c in 0..bpp {
                        let mut s = 0u32;
                        let taps = if dims == 3 { 8 } else { 4 };
                        for d in 0..taps {
                            let (dx, dy, dz) = (d & 1, (d >> 1) & 1, d >> 2);
                            s += prev[((((2 * z + dz) * size + 2 * y + dy) * size) + 2 * x + dx)
                                * bpp
                                + c] as u32;
                        }
                        next[(((z * half + y) * half) + x) * bpp + c] =
                            ((s + taps as u32 / 2) / taps as u32) as u8;
                    }
                }
            }
        }
        levels.push(next);
        size = half;
    }
    levels
}

/// The weather map's mip chain, RGBA8, level 0 first.
pub fn weather_map() -> Vec<Vec<u8>> {
    let n = WEATHER_SIZE as usize;
    let texels = parallel(n * n, |t| {
        weather_texel(
            ((t % n) as f32 + 0.5) / n as f32,
            ((t / n) as f32 + 0.5) / n as f32,
        )
    });
    let mut chans: Vec<Vec<f32>> = (0..4)
        .map(|c| texels.iter().map(|t| t[c]).collect())
        .collect();
    equalise(&mut chans[0]);
    for c in chans.iter_mut().skip(1) {
        stretch(c);
    }
    let mut base = vec![0u8; n * n * 4];
    for i in 0..n * n {
        for c in 0..4 {
            base[i * 4 + c] = to_u8(chans[c][i]);
        }
    }
    mips(base, n, 2, 4)
}

fn volume(n: usize, f: fn([f32; 3]) -> f32) -> Vec<Vec<u8>> {
    let mut v = parallel(n * n * n, |i| {
        let (x, y, z) = (i % n, (i / n) % n, i / (n * n));
        f([
            (x as f32 + 0.5) / n as f32,
            (y as f32 + 0.5) / n as f32,
            (z as f32 + 0.5) / n as f32,
        ])
    });
    stretch(&mut v);
    mips(v.into_iter().map(to_u8).collect(), n, 3, 1)
}

/// The base volume's mip chain, R8, level 0 first (z slices of y rows of x).
pub fn base_volume() -> Vec<Vec<u8>> {
    volume(BASE_SIZE as usize, base_texel)
}

/// The detail volume's mip chain, R8, level 0 first.
pub fn detail_volume() -> Vec<Vec<u8>> {
    volume(DETAIL_SIZE as usize, detail_texel)
}

/// The curl map's mip chain, RG8 (0.5 is no twist), level 0 first.
pub fn curl_map() -> Vec<Vec<u8>> {
    let n = CURL_SIZE as usize;
    let field = parallel(n * n, |i| {
        perlin_fbm(
            [
                ((i % n) as f32 + 0.5) / n as f32,
                ((i / n) as f32 + 0.5) / n as f32,
                0.5,
            ],
            4,
            3,
            131,
        )
    });
    let at = |x: usize, y: usize| field[(y % n) * n + x % n];
    let mut v = vec![[0.0f32; 2]; n * n];
    let mut peak = 1e-6f32;
    for y in 0..n {
        for x in 0..n {
            let dx = at(x + 1, y) - at(x + n - 1, y);
            let dy = at(x, y + 1) - at(x, y + n - 1);
            v[y * n + x] = [dy, -dx];
            peak = peak.max(dx.abs()).max(dy.abs());
        }
    }
    let base = v
        .iter()
        .flat_map(|c| {
            [
                to_u8(0.5 + 0.5 * c[0] / peak),
                to_u8(0.5 + 0.5 * c[1] / peak),
            ]
        })
        .collect();
    mips(base, n, 2, 2)
}

/// Deterministic cloud data, shared by renderer rebuilds in this process. The disk copy
/// also avoids regenerating millions of Worley texels on the next start.
pub struct Noise {
    pub weather: Vec<Vec<u8>>,
    pub base: Vec<Vec<u8>>,
    pub detail: Vec<Vec<u8>>,
    pub curl: Vec<Vec<u8>>,
}

const CACHE_HEADER: &[u8; 8] = b"OMSCN002"; // bump when the noise algorithm changes

fn levels_size(size: u32, dimensions: u32, bpp: usize) -> usize {
    (0..=size.trailing_zeros())
        .map(|m| ((size >> m) as usize).pow(dimensions) * bpp)
        .sum()
}

/// (size, dimensions, bytes a texel) of the weather, base, detail and curl pictures.
const LAYOUT: [(u32, u32, usize); 4] = [
    (WEATHER_SIZE, 2, 4),
    (BASE_SIZE, 3, 1),
    (DETAIL_SIZE, 3, 1),
    (CURL_SIZE, 2, 2),
];

fn cache_size() -> usize {
    16 + LAYOUT
        .iter()
        .map(|(s, d, b)| levels_size(*s, *d, *b))
        .sum::<usize>()
}

fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, b| {
        (h ^ *b as u64).wrapping_mul(0x100000001b3)
    })
}

fn decode_cache(bytes: &[u8]) -> Option<Noise> {
    if bytes.len() != cache_size() || &bytes[..8] != CACHE_HEADER {
        return None;
    }
    let expected = u64::from_le_bytes(bytes[8..16].try_into().ok()?);
    if checksum(&bytes[16..]) != expected {
        return None;
    }
    let mut at = 16;
    let mut read = |(size, dimensions, bpp): (u32, u32, usize)| {
        (0..=size.trailing_zeros())
            .map(|m| {
                let len = ((size >> m) as usize).pow(dimensions) * bpp;
                let level = bytes[at..at + len].to_vec();
                at += len;
                level
            })
            .collect()
    };
    Some(Noise {
        weather: read(LAYOUT[0]),
        base: read(LAYOUT[1]),
        detail: read(LAYOUT[2]),
        curl: read(LAYOUT[3]),
    })
}

fn encode_cache(noise: &Noise) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(cache_size());
    bytes.extend_from_slice(CACHE_HEADER);
    bytes.extend_from_slice(&[0; 8]);
    for level in noise
        .weather
        .iter()
        .chain(&noise.base)
        .chain(&noise.detail)
        .chain(&noise.curl)
    {
        bytes.extend_from_slice(level);
    }
    let hash = checksum(&bytes[16..]);
    bytes[8..16].copy_from_slice(&hash.to_le_bytes());
    bytes
}

fn cache_path() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    let home = std::path::PathBuf::from(home);
    // An unavailable home or cache is harmless; do not write into the game/mod folders.
    home.is_absolute()
        .then(|| home.join(".openbus/cache/cloud-noise-v2.bin"))
}

fn generate() -> Noise {
    let (weather, base, detail, curl) = std::thread::scope(|s| {
        let a = s.spawn(weather_map);
        let b = s.spawn(base_volume);
        let c = s.spawn(detail_volume);
        let d = s.spawn(curl_map);
        (
            a.join().expect("cloud weather"),
            b.join().expect("cloud base"),
            c.join().expect("cloud detail"),
            d.join().expect("cloud curl"),
        )
    });
    Noise {
        weather,
        base,
        detail,
        curl,
    }
}

fn load_noise(path: Option<&std::path::Path>) -> Noise {
    if let Some(path) = path {
        // Bound reads before allocating: an unrelated/corrupt file cannot exhaust memory.
        if std::fs::metadata(path)
            .ok()
            .is_some_and(|m| m.len() == cache_size() as u64)
        {
            if let Some(noise) = std::fs::read(path).ok().and_then(|b| decode_cache(&b)) {
                return noise;
            }
        }
        let _ = std::fs::remove_file(path); // a bad copy must be replaceable on Windows too
    }
    let noise = generate();
    if let Some(path) = path {
        save_noise(path, &noise);
    }
    noise
}

fn save_noise(path: &std::path::Path, noise: &Noise) {
    use std::io::Write;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let Some(parent) = path.parent() else { return };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let temp = path.with_extension(format!(
        "{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let result = (|| -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(&encode_cache(noise))?;
        drop(file);
        // A competing launcher/game may have filled it first. Either complete copy works.
        std::fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
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
        let generated = noise();
        let bytes = encode_cache(generated);
        assert_eq!(bytes.len(), cache_size());
        let loaded = decode_cache(&bytes).unwrap();
        assert_eq!(loaded.weather, generated.weather);
        assert_eq!(loaded.base, generated.base);
        assert_eq!(loaded.curl, generated.curl);
        assert!(decode_cache(&bytes[..bytes.len() - 1]).is_none());
        let mut bad = bytes.clone();
        bad[7] ^= 1;
        assert!(decode_cache(&bad).is_none());
        let mut bad = bytes.clone();
        bad[100] ^= 1;
        assert!(decode_cache(&bad).is_none());
        let path =
            std::env::temp_dir().join(format!("omsi-cloud-cache-{}.bin", std::process::id()));
        save_noise(&path, generated);
        let disk = load_noise(Some(&path));
        assert_eq!(disk.base, generated.base);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn noise_tiles_and_spans_its_range() {
        // the lattices repeat: one period on, the same value
        let a = worley([0.3, 0.7, 0.5], 8, 3, false);
        let b = worley([1.3, 0.7, 0.5], 8, 3, false);
        assert!((a - b).abs() < 1e-4);
        let a = perlin([0.3, 0.2, 0.9], 4, 5);
        let b = perlin([0.3, 1.2, 0.9], 4, 5);
        assert!((a - b).abs() < 1e-4);
        let n = noise();
        assert_eq!(n.weather.len(), WEATHER_SIZE.trailing_zeros() as usize + 1);
        assert_eq!(n.base[0].len(), (BASE_SIZE as usize).pow(3));
        // the coverage field is even: a cut at 0.7 keeps about 30 %
        let above = n.weather[0]
            .chunks(4)
            .filter(|p| p[0] as f32 / 255.0 > 0.7)
            .count() as f32
            / (WEATHER_SIZE * WEATHER_SIZE) as f32;
        assert!((above - 0.3).abs() < 0.02, "{above}");
        let (lo, hi) = (
            n.base[0].iter().min().copied().unwrap_or(0),
            n.base[0].iter().max().copied().unwrap_or(0),
        );
        assert!(hi - lo > 200, "base spans {lo}..{hi}");
    }
}
