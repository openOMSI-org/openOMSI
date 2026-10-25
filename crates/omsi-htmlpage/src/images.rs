//! Pictures for a page: `<img src>` and `background-image: url(...)`.
//!
//! A file is looked up and decoded the first time a page asks for it (also when a script
//! sets `img.src` later) and kept, a missing file too, so a frame never touches the disk
//! twice. A picture drawn at a size other than its own is resized once and that result is
//! kept as well (bounded by a byte budget), so a redraw is a plain copy.

use super::*;
use std::path::PathBuf;

/// Largest picture that is accepted (pixels).
const MAX_SRC_PX: u64 = 16 * 1024 * 1024;
/// Largest resized copy that is made (pixels).
const MAX_DST_PX: u64 = 4 * 1024 * 1024;
/// Memory the resized copies may hold together; the lot is dropped when it is exceeded.
const SCALED_BUDGET: usize = 64 * 1024 * 1024;

/// A decoded picture: RGBA8, straight alpha, top-left origin.
pub(crate) struct Img {
    pub(crate) w: u32,
    pub(crate) h: u32,
    pub(crate) rgba: Vec<u8>,
    /// False when every pixel is opaque (bmp, jpg, most dds): drawing can skip blending.
    pub(crate) alpha: bool,
}

#[derive(Default)]
struct Scaled {
    map: HashMap<(String, u32, u32), Arc<Img>>,
    bytes: usize,
}

pub(crate) struct ImageStore {
    dirs: Vec<PathBuf>,
    files: Mutex<HashMap<String, Option<Arc<Img>>>>,
    scaled: Mutex<Scaled>,
}

impl ImageStore {
    /// `dirs` are searched in order for a relative path (the page's folder first).
    pub(crate) fn new(dirs: Vec<PathBuf>) -> ImageStore {
        ImageStore { dirs, files: Mutex::new(HashMap::new()), scaled: Mutex::new(Scaled::default()) }
    }

    /// The picture behind `src`, `None` when there is none.
    pub(crate) fn get(&self, src: &str) -> Option<Arc<Img>> {
        if let Some(hit) = self.files.lock().unwrap().get(src) {
            return hit.clone();
        }
        let loaded = self.load(src);
        self.files.lock().unwrap().insert(src.to_string(), loaded.clone());
        loaded
    }

    pub(crate) fn dims(&self, src: &str) -> Option<(u32, u32)> {
        self.get(src).map(|i| (i.w, i.h))
    }

    fn load(&self, src: &str) -> Option<Arc<Img>> {
        let rel = src.split(['?', '#']).next().unwrap_or(src).trim();
        if rel.is_empty() || rel.contains("://") || rel.starts_with("data:") {
            return None;
        }
        let path = self.dirs.iter().map(|d| omsi_cfg::resolve_path(d, rel)).find(|p| omsi_cfg::vfs::is_file(p));
        let Some(path) = path else {
            log::debug!("htmltexture: image {rel} not found");
            return None;
        };
        let img = match omsi_sim::image_decode::decode(&path) {
            Ok(i) => i,
            Err(e) => {
                log::warn!("htmltexture: image {} cannot be read: {e}", path.display());
                return None;
            }
        };
        let px = img.width as u64 * img.height as u64;
        if px == 0 || px > MAX_SRC_PX || img.rgba.len() as u64 != px * 4 {
            log::warn!("htmltexture: image {} has an unusable size {}x{}", path.display(), img.width, img.height);
            return None;
        }
        let alpha = img.has_alpha && img.rgba.chunks_exact(4).any(|p| p[3] != 255);
        log::debug!("htmltexture: image {rel} -> {} ({}x{})", path.display(), img.width, img.height);
        Some(Arc::new(Img { w: img.width, h: img.height, rgba: img.rgba, alpha }))
    }

    /// The picture resized to `w` x `h` (the picture itself when it already has that size).
    pub(crate) fn scaled(&self, src: &str, w: u32, h: u32) -> Option<Arc<Img>> {
        let orig = self.get(src)?;
        if orig.w == w && orig.h == h {
            return Some(orig);
        }
        if w == 0 || h == 0 || w as u64 * h as u64 > MAX_DST_PX {
            return None;
        }
        let key = (src.to_string(), w, h);
        if let Some(hit) = self.scaled.lock().unwrap().map.get(&key) {
            return Some(hit.clone());
        }
        let out = Arc::new(resize(&orig, w, h));
        let mut g = self.scaled.lock().unwrap();
        if g.bytes + out.rgba.len() > SCALED_BUDGET {
            g.map.clear();
            g.bytes = 0;
        }
        g.bytes += out.rgba.len();
        g.map.insert(key, out.clone());
        Some(out)
    }
}

/// Resize with a box filter while the picture is more than twice too big, then bilinear.
fn resize(src: &Img, w: u32, h: u32) -> Img {
    let mut owned: Option<Img> = None;
    loop {
        let cur = owned.as_ref().unwrap_or(src);
        let hx = cur.w >= 2 && cur.w / 2 >= w;
        let hy = cur.h >= 2 && cur.h / 2 >= h;
        if !hx && !hy {
            break;
        }
        let next = halve(cur, hx, hy);
        owned = Some(next);
    }
    bilinear(owned.as_ref().unwrap_or(src), w, h)
}

/// Half the size along the chosen axes (2x2 or 2x1 or 1x2 average, alpha weighted).
fn halve(cur: &Img, hx: bool, hy: bool) -> Img {
    let (sx, sy) = (if hx { 2 } else { 1 }, if hy { 2 } else { 1 });
    let (nw, nh) = (cur.w / sx, cur.h / sy);
    let n = sx * sy;
    let mut out = vec![0u8; (nw as usize) * (nh as usize) * 4];
    for y in 0..nh {
        for x in 0..nw {
            let mut acc = [0u32; 4];
            for dy in 0..sy {
                for dx in 0..sx {
                    let i = (((y * sy + dy) * cur.w + x * sx + dx) * 4) as usize;
                    let p = &cur.rgba[i..i + 4];
                    if cur.alpha {
                        let a = p[3] as u32;
                        acc[0] += p[0] as u32 * a;
                        acc[1] += p[1] as u32 * a;
                        acc[2] += p[2] as u32 * a;
                        acc[3] += a;
                    } else {
                        acc[0] += p[0] as u32;
                        acc[1] += p[1] as u32;
                        acc[2] += p[2] as u32;
                    }
                }
            }
            let o = ((y * nw + x) * 4) as usize;
            if cur.alpha {
                if acc[3] > 0 {
                    out[o] = (acc[0] / acc[3]) as u8;
                    out[o + 1] = (acc[1] / acc[3]) as u8;
                    out[o + 2] = (acc[2] / acc[3]) as u8;
                    out[o + 3] = (acc[3] / n) as u8;
                }
            } else {
                out[o] = (acc[0] / n) as u8;
                out[o + 1] = (acc[1] / n) as u8;
                out[o + 2] = (acc[2] / n) as u8;
                out[o + 3] = 255;
            }
        }
    }
    Img { w: nw, h: nh, rgba: out, alpha: cur.alpha }
}

/// (first index, second index, weight of the second) of every output row/column.
fn taps(from: u32, to: u32) -> Vec<(usize, usize, f32)> {
    let ratio = from as f32 / to as f32;
    let last = from as usize - 1;
    (0..to)
        .map(|i| {
            let f = ((i as f32 + 0.5) * ratio - 0.5).max(0.0);
            let a = (f as usize).min(last);
            let b = (a + 1).min(last);
            (a, b, (f - a as f32).clamp(0.0, 1.0))
        })
        .collect()
}

fn bilinear(cur: &Img, w: u32, h: u32) -> Img {
    let xs = taps(cur.w, w);
    let ys = taps(cur.h, h);
    let cw = cur.w as usize;
    let mut out = vec![0u8; (w as usize) * (h as usize) * 4];
    let mut o = 0;
    for &(y0, y1, fy) in &ys {
        for &(x0, x1, fx) in &xs {
            let wt = [(1.0 - fx) * (1.0 - fy), fx * (1.0 - fy), (1.0 - fx) * fy, fx * fy];
            let ix = [(y0 * cw + x0) * 4, (y0 * cw + x1) * 4, (y1 * cw + x0) * 4, (y1 * cw + x1) * 4];
            if cur.alpha {
                let (mut r, mut g, mut b, mut a) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
                for k in 0..4 {
                    let p = &cur.rgba[ix[k]..ix[k] + 4];
                    let pa = p[3] as f32 * wt[k];
                    r += p[0] as f32 * pa;
                    g += p[1] as f32 * pa;
                    b += p[2] as f32 * pa;
                    a += pa;
                }
                if a > 0.0 {
                    out[o] = (r / a + 0.5) as u8;
                    out[o + 1] = (g / a + 0.5) as u8;
                    out[o + 2] = (b / a + 0.5) as u8;
                    out[o + 3] = (a + 0.5) as u8;
                }
            } else {
                let (mut r, mut g, mut b) = (0.0f32, 0.0f32, 0.0f32);
                for k in 0..4 {
                    let p = &cur.rgba[ix[k]..ix[k] + 3];
                    r += p[0] as f32 * wt[k];
                    g += p[1] as f32 * wt[k];
                    b += p[2] as f32 * wt[k];
                }
                out[o] = (r + 0.5) as u8;
                out[o + 1] = (g + 0.5) as u8;
                out[o + 2] = (b + 0.5) as u8;
                out[o + 3] = 255;
            }
            o += 4;
        }
    }
    Img { w, h, rgba: out, alpha: cur.alpha }
}