//! The pixel canvas and the painter that draws laid-out boxes and text into it.

use super::*;

pub(crate) struct Canvas {
    pub(crate) w: u32,
    pub(crate) h: u32,
    pub(crate) px: Vec<u8>,
    /// Only pixels inside (x0, y0, x1, y1) are drawn (`overflow: hidden`).
    pub(crate) clip: [i32; 4],
    /// Every colour's alpha is scaled by this (`opacity`).
    pub(crate) alpha: f32,
}

impl Canvas {
    pub(crate) fn new(w: u32, h: u32) -> Canvas {
        Canvas { w, h, px: vec![0; (w * h * 4) as usize], clip: [0, 0, w as i32, h as i32], alpha: 1.0 }
    }

    #[inline]
    pub(crate) fn blend(&mut self, x: i32, y: i32, c: [u8; 4], cov: f32) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 || x < self.clip[0] || y < self.clip[1] || x >= self.clip[2] || y >= self.clip[3] {
            return;
        }
        let i = ((y as u32 * self.w + x as u32) * 4) as usize;
        let cov = cov * self.alpha;
        // opaque colour over full coverage: a plain copy
        if c[3] == 255 && cov >= 0.999 {
            self.px[i..i + 4].copy_from_slice(&c);
            return;
        }
        let sa = c[3] as f32 / 255.0 * cov.clamp(0.0, 1.0);
        if sa <= 0.0 {
            return;
        }
        let d = &mut self.px[i..i + 4];
        match d[3] {
            // nothing below: the colour with the source alpha
            0 => {
                d[..3].copy_from_slice(&c[..3]);
                d[3] = (sa * 255.0).round().clamp(0.0, 255.0) as u8;
            }
            // opaque below: stays opaque, no division needed
            255 => {
                let inv = 1.0 - sa;
                for k in 0..3 {
                    d[k] = (c[k] as f32 * sa + d[k] as f32 * inv + 0.5) as u8;
                }
            }
            da8 => {
                let da = da8 as f32 / 255.0;
                let oa = sa + da * (1.0 - sa);
                for k in 0..3 {
                    let v = (c[k] as f32 * sa + d[k] as f32 * da * (1.0 - sa)) / oa;
                    d[k] = v.round().clamp(0.0, 255.0) as u8;
                }
                d[3] = (oa * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
    }

    /// Put one picture pixel `s` (RGBA, straight alpha) at an in-bounds position, `cov`
    /// scaling its alpha (the rounded corners). Opaque pixels are plain copies.
    #[inline]
    pub(crate) fn put(&mut self, x: i32, y: i32, s: &[u8], cov: f32) {
        let a = s[3];
        if a == 0 || cov <= 0.0 || x < self.clip[0] || y < self.clip[1] || x >= self.clip[2] || y >= self.clip[3] {
            return;
        }
        let i = ((y as u32 * self.w + x as u32) * 4) as usize;
        let cov = cov * self.alpha;
        if cov >= 0.999 && (a == 255 || self.px[i + 3] == 0) {
            self.px[i..i + 4].copy_from_slice(&s[..4]);
            return;
        }
        self.blend(x, y, [s[0], s[1], s[2], a], cov);
    }

    /// A horizontal run of `x0..x1` on row `y` in one colour, full coverage, in bounds.
    #[inline]
    fn span(&mut self, y: i32, x0: i32, x1: i32, c: [u8; 4]) {
        let (x0, x1) = (x0.max(self.clip[0]), x1.min(self.clip[2]));
        if x0 >= x1 || y < self.clip[1] || y >= self.clip[3] {
            return;
        }
        if c[3] == 255 && self.alpha >= 0.999 {
            let row = y as usize * self.w as usize * 4;
            for p in self.px[row + x0 as usize * 4..row + x1 as usize * 4].chunks_exact_mut(4) {
                p.copy_from_slice(&c);
            }
        } else if c[3] > 0 {
            for x in x0..x1 {
                self.blend(x, y, c, 1.0);
            }
        }
    }

    pub(crate) fn fill(&mut self, r: [f32; 4], c: [u8; 4], radius: f32) {
        if !r.iter().all(|v| v.is_finite()) || !radius.is_finite() {
            return;
        }
        let (x0, y0) = (r[0].round() as i32, r[1].round() as i32);
        let (x1, y1) = ((r[0] + r[2]).round() as i32, (r[1] + r[3]).round() as i32);
        let (x0, x1) = (x0.max(0), x1.min(self.w as i32));
        let rad = radius.min(r[2] / 2.0).min(r[3] / 2.0).max(0.0);
        let round = rad > 0.5;
        // the corner zones: only there the coverage is below 1
        let (cl, cr) = {
            let (a, b) = (r[0] + rad, r[0] + r[2] - rad);
            (a.min(b), a.max(b))
        };
        let (ct, cb) = {
            let (a, b) = (r[1] + rad, r[1] + r[3] - rad);
            (a.min(b), a.max(b))
        };
        let mid_lo = ((cl.ceil() as i32).max(x0)).min(x1);
        let mid_hi = ((cr.floor() as i32).max(mid_lo)).min(x1);
        for y in y0.max(0)..y1.min(self.h as i32) {
            let ycorner = round && ((y as f32) < ct || (y as f32 + 1.0) > cb);
            if !ycorner {
                self.span(y, x0, x1, c);
                continue;
            }
            for x in x0..mid_lo {
                self.blend(x, y, c, corner_cov(r, rad, x, y));
            }
            self.span(y, mid_lo, mid_hi, c);
            for x in mid_hi..x1 {
                self.blend(x, y, c, corner_cov(r, rad, x, y));
            }
        }
    }
}

/// A rasterised glyph: coverage bytes and where its top-left lies relative to the pen.
struct GlyphBmp {
    w: u32,
    h: u32,
    ox: i32,
    oy: i32,
    cov: Vec<u8>,
}

/// Glyphs are drawn at a quarter pixel in both axes, so a page that repaints the same text
/// every frame rasterises each glyph once.
const SUB: f32 = 4.0;

/// (glyph, size, bold, face (0 Inter, 1 the fallback), sub-pixel x, y)
type GlyphKey = (u16, u32, bool, u8, u8, u8);

thread_local! {
    static GLYPHS: std::cell::RefCell<HashMap<GlyphKey, Option<Arc<GlyphBmp>>>> = std::cell::RefCell::new(HashMap::new());
}

fn glyph_bmp(font: &FontRef<'static>, bold: bool, face: u8, id: ab_glyph::GlyphId, px: f32, sx: u8, sy: u8) -> Option<Arc<GlyphBmp>> {
    let key = (id.0, px.to_bits(), bold, face, sx, sy);
    GLYPHS.with(|g| {
        let mut g = g.borrow_mut();
        if let Some(e) = g.get(&key) {
            return e.clone();
        }
        if g.len() > 8192 {
            g.clear();
        }
        let sc = PxScale::from(px);
        let glyph = id.with_scale_and_position(sc, point(sx as f32 / SUB, sy as f32 / SUB));
        let bmp = font.outline_glyph(glyph).map(|o| {
            let b = o.px_bounds();
            let (w, h) = (b.width().ceil() as u32 + 1, b.height().ceil() as u32 + 1);
            let mut cov = vec![0u8; (w * h) as usize];
            o.draw(|gx, gy, c| {
                if gx < w && gy < h {
                    cov[(gy * w + gx) as usize] = (c.clamp(0.0, 1.0) * 255.0).round() as u8;
                }
            });
            Arc::new(GlyphBmp { w, h, ox: b.min.x.floor() as i32, oy: b.min.y.floor() as i32, cov })
        });
        g.insert(key, bmp.clone());
        bmp
    })
}

pub(crate) fn draw_text(cv: &mut Canvas, font: &FontRef<'static>, bold: bool, px: f32, x: f32, base_y: f32, color: [u8; 4], s: &str, letter: f32) {
    let sc = PxScale::from(px);
    let sf = font.as_scaled(sc);
    let mut cx = x;
    let mut prev = None;
    let fy = base_y.floor();
    let sy = (((base_y - fy) * SUB) as u8).min(SUB as u8 - 1);
    for ch in s.chars() {
        if ch == '\n' {
            continue;
        }
        face_for(font, bold, ch, |face, id, fi| {
        let sf = if fi == 0 { sf } else { face.as_scaled(sc) };
        if let Some((p, pf)) = prev {
            if pf == fi {
                cx += sf.kern(p, id);
            }
        }
        let fx = cx.floor();
        let sx = (((cx - fx) * SUB) as u8).min(SUB as u8 - 1);
        if let Some(g) = glyph_bmp(face, bold, fi, id, px, sx, sy) {
            let (bx, by) = (fx as i32 + g.ox, fy as i32 + g.oy);
            for gy in 0..g.h {
                let y = by + gy as i32;
                if y < 0 || y >= cv.h as i32 {
                    continue;
                }
                let row = &g.cov[(gy * g.w) as usize..((gy + 1) * g.w) as usize];
                for (gx, &v) in row.iter().enumerate() {
                    if v != 0 {
                        cv.blend(bx + gx as i32, y, color, v as f32 * (1.0 / 255.0));
                    }
                }
            }
        }
        cx += sf.h_advance(id) + letter;
        prev = Some((id, fi));
        });
    }
}

/// Coverage (0..1) of the pixel at (`x`, `y`) inside the rounded rectangle `r`.
fn corner_cov(r: [f32; 4], rad: f32, x: i32, y: i32) -> f32 {
    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
    let cx = px.max(r[0] + rad).min(r[0] + r[2] - rad);
    let cy = py.max(r[1] + rad).min(r[1] + r[3] - rad);
    let d = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
    (rad - d + 0.5).clamp(0.0, 1.0)
}

/// Draw `img` inside `clip` (rounded by `radius`), its top-left corner at (`ox`, `oy`),
/// tiled along the axes that repeat. Only the pixels of the clip are visited.
fn draw_tiles(cv: &mut Canvas, img: &Img, clip: [f32; 4], radius: f32, ox: i32, oy: i32, rep: (bool, bool)) {
    if !clip.iter().all(|v| v.is_finite()) || !radius.is_finite() {
        return;
    }
    let (iw, ih) = (img.w as i32, img.h as i32);
    let mut x0 = (clip[0].round() as i32).max(0);
    let mut y0 = (clip[1].round() as i32).max(0);
    let mut x1 = ((clip[0] + clip[2]).round() as i32).min(cv.w as i32);
    let mut y1 = ((clip[1] + clip[3]).round() as i32).min(cv.h as i32);
    if !rep.0 {
        x0 = x0.max(ox);
        x1 = x1.min(ox.saturating_add(iw));
    }
    if !rep.1 {
        y0 = y0.max(oy);
        y1 = y1.min(oy.saturating_add(ih));
    }
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let rad = radius.min(clip[2] / 2.0).min(clip[3] / 2.0).max(0.0);
    let round = rad > 0.5;
    let (cl, cr) = (clip[0] + rad, clip[0] + clip[2] - rad);
    let (ct, cb) = (clip[1] + rad, clip[1] + clip[3] - rad);
    let tx0 = (x0 - ox).rem_euclid(iw);
    for y in y0..y1 {
        let ty = (y - oy).rem_euclid(ih);
        let row = ty as usize * iw as usize * 4;
        let ycorner = round && ((y as f32) < ct || (y as f32 + 1.0) > cb);
        let mut tx = tx0;
        for x in x0..x1 {
            let i = row + tx as usize * 4;
            let cov = if ycorner && ((x as f32) < cl || (x as f32 + 1.0) > cr) { corner_cov(clip, rad, x, y) } else { 1.0 };
            cv.put(x, y, &img.rgba[i..i + 4], cov);
            tx += 1;
            if tx == iw {
                tx = 0;
            }
        }
    }
}

/// The `background-image` of `st` over `rect` (size, repeat and position as CSS has them).
pub(crate) fn paint_bg(cv: &mut Canvas, imgs: &ImageStore, rect: [f32; 4], radius: f32, st: &Style) {
    let Some(src) = &st.bg_img else { return };
    let (rw, rh) = (rect[2], rect[3]);
    if !rect.iter().all(|v| v.is_finite()) || rw < 1.0 || rh < 1.0 {
        return;
    }
    let Some((iw, ih)) = imgs.dims(src) else { return };
    let (iw, ih) = (iw as f32, ih as f32);
    let (tw, th) = match st.bg_size {
        BgSize::Auto => (iw, ih),
        BgSize::Cover => {
            let k = (rw / iw).max(rh / ih);
            (iw * k, ih * k)
        }
        BgSize::Contain => {
            let k = (rw / iw).min(rh / ih);
            (iw * k, ih * k)
        }
        BgSize::Dims(w, h) => match (w.map(|l| l.px(rw)), h.map(|l| l.px(rh))) {
            (Some(w), Some(h)) => (w, h),
            (Some(w), None) => (w, w * ih / iw),
            (None, Some(h)) => (h * iw / ih, h),
            (None, None) => (iw, ih),
        },
    };
    if !tw.is_finite() || !th.is_finite() {
        return;
    }
    let (tw, th) = ((tw.round() as i64).clamp(1, 16384) as u32, (th.round() as i64).clamp(1, 16384) as u32);
    let Some(tile) = imgs.scaled(src, tw, th) else { return };
    let ox = rect[0] + st.bg_pos[0].px(rw - tw as f32);
    let oy = rect[1] + st.bg_pos[1].px(rh - th as f32);
    if !ox.is_finite() || !oy.is_finite() {
        return;
    }
    draw_tiles(cv, &tile, rect, radius, ox.round() as i32, oy.round() as i32, st.bg_repeat);
}

/// An `<img>`: the picture stretched over the content box of the element.
fn paint_img(cv: &mut Canvas, lay: &Layouter, b: &LBox) {
    let n = &lay.dom.nodes[b.node];
    if n.tag != "img" || n.src.is_empty() {
        return;
    }
    let [pt, pr, pb, pl] = b.st.padding;
    let [bt, br, bb, bl] = b.st.bw;
    let area = [b.rect[0] + pl + bl, b.rect[1] + pt + bt, b.rect[2] - pl - pr - bl - br, b.rect[3] - pt - pb - bt - bb];
    if !area.iter().all(|v| v.is_finite()) || area[2] < 0.5 || area[3] < 0.5 {
        return;
    }
    let w = (area[2].round() as i64).clamp(1, 16384) as u32;
    let h = (area[3].round() as i64).clamp(1, 16384) as u32;
    let Some(pic) = lay.imgs.scaled(&n.src, w, h) else { return };
    draw_tiles(cv, &pic, area, b.st.radius, area[0].round() as i32, area[1].round() as i32, (false, false));
}

/// Coverage of the pixel (`x`, `y`) in the rounded rectangle `r`, 0 outside it.
fn rr_cov(r: [f32; 4], rad: f32, x: i32, y: i32) -> f32 {
    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
    if px < r[0] - 0.5 || py < r[1] - 0.5 || px > r[0] + r[2] + 0.5 || py > r[1] + r[3] + 0.5 {
        return 0.0;
    }
    let edge = (px - r[0]).min(r[0] + r[2] - px).min(py - r[1]).min(r[1] + r[3] - py);
    if rad <= 0.5 {
        return (edge + 0.5).clamp(0.0, 1.0);
    }
    corner_cov(r, rad, x, y).min((edge + 0.5).clamp(0.0, 1.0))
}

impl Canvas {
    /// The border of a box: four strips, or a ring where the corners are rounded and the
    /// sides alike.
    pub(crate) fn border(&mut self, r: [f32; 4], radius: f32, bw: [f32; 4], colors: [[u8; 4]; 4]) {
        if !r.iter().all(|v| v.is_finite()) || bw.iter().all(|w| *w <= 0.0) {
            return;
        }
        let [t, rt, b, l] = bw;
        let uniform = bw.iter().all(|w| (*w - t).abs() < 0.01) && colors.iter().all(|c| *c == colors[0]);
        let rad = radius.min(r[2] / 2.0).min(r[3] / 2.0).max(0.0);
        if rad > 0.5 && uniform {
            let inner = [r[0] + l, r[1] + t, (r[2] - l - rt).max(0.0), (r[3] - t - b).max(0.0)];
            let irad = (rad - t).max(0.0);
            let (x0, x1) = (r[0].floor() as i32, (r[0] + r[2]).ceil() as i32);
            let (y0, y1) = (r[1].floor() as i32, (r[1] + r[3]).ceil() as i32);
            for y in y0.max(0)..y1.min(self.h as i32) {
                for x in x0.max(0)..x1.min(self.w as i32) {
                    let outer = rr_cov(r, rad, x, y);
                    if outer <= 0.0 {
                        continue;
                    }
                    let cov = outer * (1.0 - rr_cov(inner, irad, x, y));
                    if cov > 0.0 {
                        self.blend(x, y, colors[0], cov);
                    }
                }
            }
            return;
        }
        // top and bottom run the whole width, left and right between them
        if t > 0.0 {
            self.fill([r[0], r[1], r[2], t], colors[0], 0.0);
        }
        if b > 0.0 {
            self.fill([r[0], r[1] + r[3] - b, r[2], b], colors[2], 0.0);
        }
        if l > 0.0 {
            self.fill([r[0], r[1] + t, l, (r[3] - t - b).max(0.0)], colors[3], 0.0);
        }
        if rt > 0.0 {
            self.fill([r[0] + r[2] - rt, r[1] + t, rt, (r[3] - t - b).max(0.0)], colors[1], 0.0);
        }
    }

    /// A linear gradient over `r`.
    pub(crate) fn gradient(&mut self, r: [f32; 4], radius: f32, g: &Gradient) {
        if !r.iter().all(|v| v.is_finite()) || r[2] < 1.0 || r[3] < 1.0 || g.stops.len() < 2 {
            return;
        }
        let a = g.angle.to_radians();
        let dir = (a.sin(), -a.cos());
        let len = (r[2] * dir.0.abs() + r[3] * dir.1.abs()).max(1.0);
        // the stops' positions along the line, 0..1: given ones, the rest spread between them
        let mut pos: Vec<f32> = g.stops.iter().map(|s| s.pos.map_or(f32::NAN, |l| match l {
            Len::Px(v) => v / len,
            Len::Pct(p) => p / 100.0,
            Len::Mix(v, p) => v / len + p / 100.0,
        })).collect();
        if pos[0].is_nan() {
            pos[0] = 0.0;
        }
        let last = pos.len() - 1;
        if pos[last].is_nan() {
            pos[last] = 1.0;
        }
        let mut i = 0;
        while i < pos.len() {
            if pos[i].is_nan() {
                let j = (i..pos.len()).find(|&j| !pos[j].is_nan()).unwrap_or(last);
                let (p0, p1) = (pos[i - 1], pos[j]);
                for k in i..j {
                    pos[k] = p0 + (p1 - p0) * (k - i + 1) as f32 / (j - i + 1) as f32;
                }
                i = j;
            }
            i += 1;
        }
        for k in 1..pos.len() {
            pos[k] = pos[k].max(pos[k - 1]);
        }
        let center = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
        let rad = radius.min(r[2] / 2.0).min(r[3] / 2.0).max(0.0);
        let (x0, x1) = (r[0].round() as i32, (r[0] + r[2]).round() as i32);
        let (y0, y1) = (r[1].round() as i32, (r[1] + r[3]).round() as i32);
        for y in y0.max(0)..y1.min(self.h as i32) {
            for x in x0.max(0)..x1.min(self.w as i32) {
                let t = (((x as f32 + 0.5 - center.0) * dir.0 + (y as f32 + 0.5 - center.1) * dir.1) / len + 0.5).clamp(0.0, 1.0);
                let k = pos.iter().position(|p| *p >= t).unwrap_or(last);
                let c = if k == 0 {
                    g.stops[0].color
                } else {
                    let (p0, p1) = (pos[k - 1], pos[k]);
                    let f = if p1 > p0 { (t - p0) / (p1 - p0) } else { 1.0 };
                    let (c0, c1) = (g.stops[k - 1].color, g.stops[k].color);
                    let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * f).round() as u8;
                    [mix(c0[0], c1[0]), mix(c0[1], c1[1]), mix(c0[2], c1[2]), mix(c0[3], c1[3])]
                };
                let cov = if rad > 0.5 { corner_cov(r, rad, x, y) } else { 1.0 };
                self.blend(x, y, c, cov);
            }
        }
    }

    /// The shadow of a box: layers of the box's shape growing outward, each faint.
    pub(crate) fn shadow(&mut self, r: [f32; 4], radius: f32, sh: &Shadow) {
        let a = sh.color[3] as f32 / 255.0;
        if a <= 0.0 {
            return;
        }
        let layers = (sh.blur.ceil() as i32).clamp(1, 20);
        // each layer adds a share so that the middle ends as dense as the colour says
        let per = 1.0 - (1.0 - a).powf(1.0 / layers as f32);
        let per_a = (per * 255.0).round().clamp(1.0, 255.0) as u8;
        for i in 0..layers {
            let t = i as f32 / layers as f32;
            let grow = sh.spread + sh.blur * (0.5 - t);
            let rect = [r[0] + sh.dx - grow, r[1] + sh.dy - grow, r[2] + 2.0 * grow, r[3] + 2.0 * grow];
            if rect[2] <= 0.0 || rect[3] <= 0.0 {
                continue;
            }
            self.fill(rect, [sh.color[0], sh.color[1], sh.color[2], per_a], (radius + grow).max(0.0));
        }
    }
}

pub(crate) fn paint(cv: &mut Canvas, lay: &Layouter, b: &LBox) {
    let saved_alpha = cv.alpha;
    let saved_clip = cv.clip;
    cv.alpha *= b.st.opacity;
    if !b.st.hidden {
        if let Some(sh) = &b.st.shadow {
            cv.shadow(b.rect, b.st.radius, sh);
        }
        if b.st.bg[3] > 0 {
            cv.fill(b.rect, b.st.bg, b.st.radius);
        }
        if let Some(g) = &b.st.gradient {
            cv.gradient(b.rect, b.st.radius, g);
        }
        if b.st.bg_img.is_some() {
            paint_bg(cv, lay.imgs, b.rect, b.st.radius, &b.st);
        }
        cv.border(b.rect, b.st.radius, b.st.bw, b.st.bcolor);
        paint_img(cv, lay, b);
    }
    if b.st.overflow_hidden {
        let [bt, br, bb, bl] = b.st.bw;
        let pad = [b.rect[0] + bl, b.rect[1] + bt, b.rect[0] + b.rect[2] - br, b.rect[1] + b.rect[3] - bb];
        cv.clip = [cv.clip[0].max(pad[0].round() as i32), cv.clip[1].max(pad[1].round() as i32), cv.clip[2].min(pad[2].round() as i32), cv.clip[3].min(pad[3].round() as i32)];
    }
    for it in &b.items {
        match it {
            Item::Block(c) => paint(cv, lay, c),
            Item::Line(l) => {
                if b.st.hidden {
                    continue;
                }
                for li in &l.items {
                    let font = if li.bold { lay.bold } else { lay.reg };
                    let sf = font.as_scaled(PxScale::from(li.px));
                    let base = l.y + (l.h - (sf.ascent() - sf.descent())) / 2.0 + sf.ascent();
                    draw_text(cv, font, li.bold, li.px, l.x + li.dx, base, li.color, &li.text, li.letter);
                    let thick = (li.px / 14.0).max(1.0);
                    let x = l.x + li.dx;
                    if li.underline {
                        cv.fill([x, base + li.px * 0.1, li.w, thick], li.color, 0.0);
                    }
                    if li.strike {
                        cv.fill([x, base - li.px * 0.3, li.w, thick], li.color, 0.0);
                    }
                }
            }
        }
    }
    cv.alpha = saved_alpha;
    cv.clip = saved_clip;
}
