//! The paint on the roads, read off the road textures: which pixels of a texture are lane
//! lines, crossings, stop lines, arrows (white) or yellow lines, so that the map can draw them
//! as shapes of their own where the road's faces carry the texture.
//!
//! A texture is a picture of asphalt with paint on it. The paint is told from the asphalt by
//! being much lighter than the darker part of the picture and (for yellow) by its hue; a
//! picture that is light all over (concrete, cobbles) has no paint, and specks are no paint.
//! Neither the name of a texture nor a list of known textures is used, so a map maker's own
//! road textures are read the same way.

use glam::Vec2;
use hashbrown::HashMap;
use omsi_map::layers;
use omsi_scenery::sli::SplineProfilePoint;
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The paint classes of a texel.
pub const TEXEL_NONE: u8 = 0;
pub const TEXEL_WHITE: u8 = 1;
pub const TEXEL_YELLOW: u8 = 2;

/// The narrowest a lane line is drawn on the map (m).
const MIN_LINE: f64 = 0.2;
/// Largest side a texture is looked at with (larger ones are sampled).
const MAX_SIDE: usize = 1024;

/// One texture's paint: a class per texel (row 0 at the top, as the picture is stored).
pub struct TexturePaint {
    pub w: usize,
    pub h: usize,
    pub cls: Vec<u8>,
    /// The paint that is a plain rectangle (see [`TexturePaint::find_bars`]).
    pub bars: Vec<Bar>,
    /// The paint that is not (arrows, symbols): the classes of `cls` with the bars taken out.
    pub rest: Vec<u8>,
}

/// A rectangle of paint in texture coordinates: u across, v along (a dash that crosses the
/// picture's edge has `v[1]` above 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bar {
    pub u: [f32; 2],
    pub v: [f32; 2],
    pub class: u8,
}

impl TexturePaint {
    /// The mask of a picture; None where it has no paint.
    pub fn read_picture(img: &omsi_texture::Image) -> Option<TexturePaint> {
        let (iw, ih) = (img.width as usize, img.height as usize);
        if iw == 0 || ih == 0 || img.rgba.len() < iw * ih * 4 {
            return None;
        }
        let step = (iw.max(ih) + MAX_SIDE - 1) / MAX_SIDE;
        let (w, h) = (iw / step.max(1), ih / step.max(1));
        let px = |x: usize, y: usize| {
            let i = ((y * step) * iw + x * step) * 4;
            [img.rgba[i] as f32 / 255.0, img.rgba[i + 1] as f32 / 255.0, img.rgba[i + 2] as f32 / 255.0]
        };
        let opaque = |x: usize, y: usize| !img.has_alpha || img.rgba[((y * step) * iw + x * step) * 4 + 3] >= 128;
        let luma = |c: [f32; 3]| 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
        // (a decal - paint on a see-through picture - has no asphalt in it: whatever is drawn is
        // the paint, and it may cover the whole of what is drawn)
        let decal = img.has_alpha && (0..w * h).step_by(((w * h) / 4096).max(1)).any(|k| !opaque(k % w, k / w));
        let mut sorted: Vec<f32> = (0..w * h).step_by(((w * h) / 4096).max(1)).filter(|k| opaque(k % w, k / w)).map(|k| luma(px(k % w, k / w))).collect();
        sorted.sort_by(|a, b| a.total_cmp(b));
        // (the asphalt: the darker quarter of the picture)
        let asphalt = if decal { 0.0 } else { sorted.get(sorted.len() / 4).copied()? };
        let mut cls = vec![TEXEL_NONE; w * h];
        let mut bright = 0usize;
        for y in 0..h {
            for x in 0..w {
                if !opaque(x, y) {
                    continue;
                }
                let c = px(x, y);
                let l = luma(c);
                // (a decal has no asphalt to be told from: it must be bright, or a light-grey
                // manhole or patch would be white paint)
                if l < asphalt + 0.26 || l < if decal { 0.72 } else { 0.5 } {
                    continue;
                }
                let (mx, mn) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2]));
                let yellow = c[0] > 0.55 && c[1] > 0.42 && c[2] < mx * 0.62 && c[0] - c[2] > 0.25;
                let white = mn > if decal { 0.62 } else { 0.45 } && (mx - mn) < 0.28;
                if yellow {
                    cls[y * w + x] = TEXEL_YELLOW;
                } else if white {
                    cls[y * w + x] = TEXEL_WHITE;
                } else {
                    continue;
                }
                bright += 1;
            }
        }
        // (a picture that is light all over has no paint on it; nor has a decal whose drawn
        // part is nearly all light: a patch)
        let drawn = if decal { (0..w * h).filter(|k| opaque(k % w, k / w)).count().max(1) } else { w * h };
        if bright == 0 || bright * 100 > drawn * if decal { 90 } else { 55 } {
            return None;
        }
        let mut mask = TexturePaint { w, h, cls, bars: Vec::new(), rest: Vec::new() };
        mask.drop_specks();
        if !mask.cls.iter().any(|c| *c != TEXEL_NONE) {
            return None;
        }
        (mask.bars, mask.rest) = mask.find_bars();
        // (only the bars and the shapes that are no bars are asked for now)
        mask.cls = Vec::new();
        if mask.rest.iter().all(|c| *c == TEXEL_NONE) {
            mask.rest = Vec::new();
        }
        Some(mask)
    }

    /// Connected texels of one class, each as (the texels, their bounding box).
    fn clumps(&self) -> Vec<(Vec<usize>, [usize; 4])> {
        let (w, h) = (self.w, self.h);
        let mut seen = vec![false; w * h];
        let mut out = Vec::new();
        for start in 0..w * h {
            if self.cls[start] == TEXEL_NONE || seen[start] {
                continue;
            }
            let class = self.cls[start];
            let mut stack = vec![start];
            let mut texels = Vec::new();
            let mut bb = [usize::MAX, usize::MAX, 0usize, 0usize];
            seen[start] = true;
            while let Some(k) = stack.pop() {
                texels.push(k);
                let (x, y) = (k % w, k / w);
                bb = [bb[0].min(x), bb[1].min(y), bb[2].max(x), bb[3].max(y)];
                let mut push = |i: usize| {
                    if !seen[i] && self.cls[i] == class {
                        seen[i] = true;
                        stack.push(i);
                    }
                };
                if x > 0 {
                    push(k - 1);
                }
                if x + 1 < w {
                    push(k + 1);
                }
                if y > 0 {
                    push(k - w);
                }
                if y + 1 < h {
                    push(k + w);
                }
            }
            out.push((texels, bb));
        }
        out
    }

    /// Paint that is only a few texels (grit in a photograph of asphalt) is no paint, nor are
    /// ragged clumps (worn, light blotches: their outline is far longer than that of the box
    /// round them, where a line, a bar or an arrow is as long as its box), and a picture with
    /// very many separate pieces of paint is dirt, not paint.
    fn drop_specks(&mut self) {
        let least = ((self.w * self.h) / 8000).max(6);
        let (w, h) = (self.w, self.h);
        let clumps = self.clumps();
        let mut kept = 0;
        for (texels, bb) in &clumps {
            let class = self.cls[texels[0]];
            let edge = texels
                .iter()
                .filter(|&&k| {
                    let (x, y) = (k % w, k / w);
                    x == 0 || y == 0 || x + 1 == w || y + 1 == h || self.cls[k - 1] != class || self.cls[k + 1] != class || self.cls[k - w] != class || self.cls[k + w] != class
                })
                .count();
            let box_edge = (2 * (bb[2] - bb[0] + bb[3] - bb[1] + 2)).saturating_sub(4).max(1);
            let ragged = edge * 10 > box_edge * 17 && texels.len() * 30 < w * h;
            if texels.len() < least || (bb[2] - bb[0] < 2 && bb[3] - bb[1] < 2) || ragged {
                for &k in texels {
                    self.cls[k] = TEXEL_NONE;
                }
            } else {
                kept += 1;
            }
        }
        if kept > 80 {
            self.cls.iter_mut().for_each(|c| *c = TEXEL_NONE);
        }
    }

    /// Whether there is paint at texture coordinates (u, v), the texture repeating.
    pub fn painted_at(&self, u: f32, v: f32) -> bool {
        let (fu, fv) = (u - u.floor(), v - v.floor());
        let in_bar = |v: f32| self.bars.iter().any(|b| fu >= b.u[0] && fu < b.u[1] && v >= b.v[0] && v < b.v[1]);
        in_bar(fv) || in_bar(fv + 1.0) || self.free_form_at(u, v) != TEXEL_NONE
    }

    /// The paint that is a plain rectangle (lane lines and their dashes, stop lines, the bars
    /// of a crossing), in texture coordinates. A dash cut by the picture's edge is joined with
    /// its other half. Paint of other shapes (arrows) is left to [`TexturePaint::free_form_at`] (and [`TexturePaint::painted_at`]).
    pub fn find_bars(&self) -> (Vec<Bar>, Vec<u8>) {
        let (w, h) = (self.w as f32, self.h as f32);
        let mut out: Vec<(Bar, [usize; 4])> = Vec::new();
        let mut rest = self.cls.clone();
        for (texels, bb) in self.clumps() {
            let area = (bb[2] - bb[0] + 1) * (bb[3] - bb[1] + 1);
            // (a plain rectangle: the texels fill nearly all of the box)
            if texels.len() * 100 < area * 85 {
                continue;
            }
            let class = self.cls[texels[0]];
            for &k in &texels {
                rest[k] = TEXEL_NONE;
            }
            let s = Bar { u: [bb[0] as f32 / w, (bb[2] + 1) as f32 / w], v: [bb[1] as f32 / h, (bb[3] + 1) as f32 / h], class };
            out.push((s, bb));
        }
        // the two halves of a dash cut by the edge of the picture
        let mut joined: Vec<Bar> = Vec::new();
        let mut used = vec![false; out.len()];
        for i in 0..out.len() {
            if used[i] {
                continue;
            }
            let (a, abb) = out[i];
            if abb[1] == 0 {
                if let Some(j) = (0..out.len()).find(|&j| j != i && !used[j] && out[j].1[3] + 1 == self.h && out[j].1[0].abs_diff(abb[0]) <= 1 && out[j].1[2].abs_diff(abb[2]) <= 1 && out[j].0.class == a.class) {
                    used[i] = true;
                    used[j] = true;
                    // (from the lower half, round the edge, on into the upper)
                    joined.push(Bar { u: a.u, v: [out[j].0.v[0], 1.0 + a.v[1]], class: a.class });
                    continue;
                }
            }
            if !used[i] {
                joined.push(a);
            }
        }
        (joined, rest)
    }

    /// The class of paint that is no plain rectangle at (u, v), the texture repeating.
    pub fn free_form_at(&self, u: f32, v: f32) -> u8 {
        if self.rest.is_empty() {
            return TEXEL_NONE;
        }
        let (fu, fv) = (u - u.floor(), v - v.floor());
        let x = ((fu * self.w as f32) as usize).min(self.w - 1);
        let y = ((fv * self.h as f32) as usize).min(self.h - 1);
        self.rest[y * self.w + x]
    }
}

/// The paint of the textures one map generation asks for, each read once: the cache lives as
/// long as the generation does (a picture's mask is up to two megabytes), not the session.
#[derive(Default)]
pub struct PaintCache {
    memo: Mutex<HashMap<std::path::PathBuf, Option<Arc<TexturePaint>>>>,
}

impl PaintCache {
    /// The paint of a texture looked up by name in `dirs`. None: no such texture, or no paint on
    /// it. The summer picture is read whatever season the game shows: snow would be white paint,
    /// and the layers are kept for good.
    pub fn get(&self, texture: &str, dirs: &[&Path]) -> Option<Arc<TexturePaint>> {
        let found = omsi_texture::find_texture_in_look(texture, dirs, None)?;
        if let Some(hit) = self.memo.lock().get(&found) {
            return hit.clone();
        }
        let mask = omsi_texture::decode_file(&found).ok().and_then(|img| TexturePaint::read_picture(&img)).map(Arc::new);
        self.memo.lock().insert(found, mask.clone());
        mask
    }
}

/// The paint of one stretch of a road section as triangles, per marking class (`layers::MARK_*`):
/// the section is the profile's segment `seg` laid along a spline of `length` metres whose
/// textures start `tex_offset` along it; `at_s(s, x)` is the place `x` across (right
/// positive, before any mirror) at distance `s` along it. The texture's rectangles of paint
/// are put where the game's mesh would show them: `u` between the segment's points across,
/// `v = v_scale * (s + tex_offset)` along (turned round on a mirrored spline).
pub fn lane_paint(paint: &TexturePaint, seg: [&SplineProfilePoint; 2], length: f64, tex_offset: f64, mirror: bool, at_s: &dyn Fn(f64, f64) -> Vec2) -> Vec<(usize, Vec<Vec2>)> {
    let (a, b) = (seg[0], seg[1]);
    let du = (b.u - a.u) as f64;
    let vs = ((a.v_scale + b.v_scale) * 0.5) as f64;
    let mut out: Vec<Vec<Vec2>> = vec![Vec::new(); layers::PAINT_CLASSES];
    if du.abs() < 1e-4 || !(vs > 1e-4 && vs < 1.0e4) || !(length > 0.0 && length < 1.0e6) {
        return Vec::new();
    }
    let (ulo, uhi) = (a.u.min(b.u) as f64, a.u.max(b.u) as f64);
    if !(ulo.is_finite() && uhi.is_finite()) || uhi - ulo > 64.0 || ulo.abs() > 1.0e6 || uhi.abs() > 1.0e6 || !tex_offset.is_finite() || !length.is_finite() {
        return Vec::new();
    }
    let x_of = |u: f64| a.x as f64 + (u - a.u as f64) / du * (b.x - a.x) as f64;
    let (w_lo, w_hi) = (vs * tex_offset, vs * (length + tex_offset));
    for st in &paint.bars {
        // (the texture repeats across: a profile may run over several of it)
        for rep in (ulo.floor() as i64)..=(uhi.floor() as i64) {
            let (u0, u1) = ((st.u[0] as f64 + rep as f64).max(ulo), (st.u[1] as f64 + rep as f64).min(uhi));
            if u1 - u0 < 1e-4 {
                continue;
            }
            let (xa, xb) = (x_of(u0), x_of(u1));
            let (xl, xr) = (xa.min(xb), xa.max(xb));
            if xr - xl < 0.03 {
                continue;
            }
            // (a painted line is a hand's breadth; on a map it is drawn a little broader, or
            // from any height it would be a hair)
            let (xl, xr) = if xr - xl < MIN_LINE { ((xl + xr - MIN_LINE) * 0.5, (xl + xr + MIN_LINE) * 0.5) } else { (xl, xr) };
            let (v0, v1) = if mirror { (1.0 - st.v[1] as f64, 1.0 - st.v[0] as f64) } else { (st.v[0] as f64, st.v[1] as f64) };
            let (k0, k1) = ((w_lo - v1).floor() as i64, (w_hi - v0).ceil() as i64);
            if k1.saturating_sub(k0) > 8000 {
                continue;
            }
            let class = (st.class as usize - 1).min(layers::PAINT_CLASSES - 1);
            for k in k0..=k1 {
                let s0 = ((k as f64 + v0) / vs - tex_offset).max(0.0);
                let s1 = ((k as f64 + v1) / vs - tex_offset).min(length);
                // (a stub of a dash cut by the spline's end goes on in the next spline; a sliver of
                // it is only noise)
                if s1 - s0 < 0.25 {
                    continue;
                }
                let n = ((s1 - s0) / 2.0).ceil().max(1.0) as usize;
                let mut prev = (at_s(s0, xl), at_s(s0, xr));
                for i in 1..=n {
                    let s = s0 + (s1 - s0) * i as f64 / n as f64;
                    let cur = (at_s(s, xl), at_s(s, xr));
                    out[class].extend([prev.0, prev.1, cur.1, prev.0, cur.1, cur.0]);
                    prev = cur;
                }
            }
        }
    }
    out.into_iter().enumerate().filter(|(_, t)| !t.is_empty()).collect()
}

/// A convex polygon (here a triangle) cut to the axis-aligned rectangle `rect` (min, max).
fn clip_to_rect(poly: &[Vec2], rect: [Vec2; 2]) -> Vec<Vec2> {
    let mut cur: Vec<Vec2> = poly.to_vec();
    // (each side in turn: keep what is on the inside, cut the edges that cross it)
    let sides: [(usize, f32, f32); 4] = [(0, rect[0].x, 1.0), (0, rect[1].x, -1.0), (1, rect[0].y, 1.0), (1, rect[1].y, -1.0)];
    for (axis, at, sign) in sides {
        let val = |p: Vec2| if axis == 0 { p.x } else { p.y };
        let inside = |p: Vec2| (val(p) - at) * sign >= 0.0;
        let mut next = Vec::with_capacity(cur.len() + 2);
        for i in 0..cur.len() {
            let (a, b) = (cur[i], cur[(i + 1) % cur.len()]);
            let (ia, ib) = (inside(a), inside(b));
            if ia {
                next.push(a);
            }
            if ia != ib {
                let t = (at - val(a)) / (val(b) - val(a));
                next.push(a.lerp(b, t));
            }
        }
        cur = next;
        if cur.len() < 3 {
            return Vec::new();
        }
    }
    cur
}

/// The paint on an object's flat faces (`faces`: triangle, its texture coordinates and which of
/// `texs` it is laid with; in the object's frame). The texture's rectangles of paint (a
/// crossing's bars, stop lines, lane lines) are cut exactly out of each face by its texture
/// coordinates; the rest (arrows, symbols) is looked at from above in cells of about 8 cm,
/// traced into outlines and cut into triangles.
pub fn face_paint(faces: &[([Vec2; 3], [Vec2; 3], usize)], texs: &[String], dirs: &[PathBuf], cache: &PaintCache) -> Vec<(usize, [Vec2; 3])> {
    let dirs: Vec<&Path> = dirs.iter().map(|d| d.as_path()).collect();
    let masks: Vec<Option<Arc<TexturePaint>>> = texs.iter().map(|t| if t.trim().is_empty() { None } else { cache.get(t, &dirs) }).collect();
    let painted: Vec<&([Vec2; 3], [Vec2; 3], usize)> = faces.iter().filter(|f| masks[f.2].is_some()).collect();
    if painted.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<(usize, [Vec2; 3])> = Vec::new();
    // the rectangles of paint, cut exactly out of each face where its texture lays them
    for (t, uv, slot) in &painted {
        let Some(mask) = masks[*slot].as_ref() else { continue };
        let det = (uv[1] - uv[0]).perp_dot(uv[2] - uv[0]);
        if det.abs() < 1e-9 {
            continue;
        }
        let (ulo, uhi) = (uv[0].min(uv[1]).min(uv[2]), uv[0].max(uv[1]).max(uv[2]));
        // (a texture coordinate that is not a number or runs over many pictures is no laying
        // of paint; checked before it is cast to a count)
        if !(ulo.is_finite() && uhi.is_finite()) || uhi.x - ulo.x > 20.0 || uhi.y - ulo.y > 20.0 || ulo.abs().max_element() > 1.0e6 || uhi.abs().max_element() > 1.0e6 {
            continue;
        }
        let (ku0, ku1, kv0, kv1) = (ulo.x.floor() as i64, uhi.x.floor() as i64, ulo.y.floor() as i64, uhi.y.floor() as i64);
        if (ku1 - ku0 + 1) * (kv1 - kv0 + 1) > 400 {
            continue;
        }
        let to_world = |p: Vec2| {
            let w1 = (p - uv[0]).perp_dot(uv[2] - uv[0]) / det;
            let w2 = (uv[1] - uv[0]).perp_dot(p - uv[0]) / det;
            t[0] * (1.0 - w1 - w2) + t[1] * w1 + t[2] * w2
        };
        for st in &mask.bars {
            for ku in ku0..=ku1 {
                for kv in kv0..=kv1 {
                    let rect = [Vec2::new(st.u[0] + ku as f32, st.v[0] + kv as f32), Vec2::new(st.u[1] + ku as f32, st.v[1] + kv as f32)];
                    let poly = clip_to_rect(&[uv[0], uv[1], uv[2]], rect);
                    for i in 1..poly.len().saturating_sub(1) {
                        out.push((st.class as usize - 1, [to_world(poly[0]), to_world(poly[i]), to_world(poly[i + 1])]));
                    }
                }
            }
        }
    }
    let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
    for (t, ..) in &painted {
        for p in t {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
    }
    let ext = hi - lo;
    if !(ext.x.is_finite() && ext.y.is_finite()) || ext.max_element() > 4000.0 {
        return out;
    }
    let cell = (ext.max_element() / 1100.0).max(0.08);
    let (w, h) = ((ext.x / cell).ceil() as usize + 3, (ext.y / cell).ceil() as usize + 3);
    let org = lo - Vec2::splat(cell * 1.5);
    let mut cls = vec![TEXEL_NONE; w * h];
    for (t, uv, slot) in painted {
        let Some(mask) = masks[*slot].as_ref() else { continue };
        let a = (t[1] - t[0]).perp_dot(t[2] - t[0]);
        if a.abs() < 1e-6 {
            continue;
        }
        let (tlo, thi) = (t[0].min(t[1]).min(t[2]), t[0].max(t[1]).max(t[2]));
        let (x0, y0) = ((((tlo.x - org.x) / cell).floor().max(0.0)) as usize, (((tlo.y - org.y) / cell).floor().max(0.0)) as usize);
        let (x1, y1) = ((((thi.x - org.x) / cell).ceil() as usize).min(w - 1), (((thi.y - org.y) / cell).ceil() as usize).min(h - 1));
        for y in y0..=y1 {
            for x in x0..=x1 {
                let q = org + Vec2::new((x as f32 + 0.5) * cell, (y as f32 + 0.5) * cell);
                let w0 = (t[1] - q).perp_dot(t[2] - q) / a;
                let w1 = (t[2] - q).perp_dot(t[0] - q) / a;
                let w2 = 1.0 - w0 - w1;
                if w0 < -0.02 || w1 < -0.02 || w2 < -0.02 {
                    continue;
                }
                let u = uv[0] * w0 + uv[1] * w1 + uv[2] * w2;
                let c = mask.free_form_at(u.x, u.y);
                if c != TEXEL_NONE {
                    cls[y * w + x] = c;
                }
            }
        }
    }
    for class in [TEXEL_WHITE, TEXEL_YELLOW] {
        let cells: Vec<bool> = cls.iter().map(|c| *c == class).collect();
        if !cells.iter().any(|c| *c) {
            continue;
        }
        let rings: Vec<Vec<Vec2>> = layers::trace_cells(&cells, w, h)
            .into_iter()
            .map(|r| layers::simplify_ring(&r, 0.6).into_iter().map(|p| org + p * cell).collect::<Vec<Vec2>>())
            .filter(|r| r.len() >= 3)
            .collect();
        let inside = |p: Vec2, ring: &[Vec2]| {
            let n = ring.len();
            let mut c = false;
            for i in 0..n {
                let (a, b) = (ring[i], ring[(i + 1) % n]);
                if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
                    c = !c;
                }
            }
            c
        };
        // (a piece of paint is a line, a bar or an arrow: a metre long or a quarter of a square
        // metre broad - what is smaller is a flake of worn paint in the texture's repeats)
        let worth = |r: &[Vec2]| {
            let (lo, hi) = r.iter().fold((Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)), |(l, h), p| (l.min(*p), h.max(*p)));
            (hi - lo).length() >= 1.0 || layers::area2(r) * 0.5 >= 0.25
        };
        for outer in rings.iter().filter(|r| layers::area2(r) * 0.5 >= 0.02 && worth(r)) {
            let mut group = vec![outer.clone()];
            group.extend(rings.iter().filter(|r| layers::area2(r) < 0.0 && inside(r[0], outer)).cloned());
            let (outline, tris) = crate::mapgen::roof_of(&group);
            for t in tris {
                out.push((class as usize - 1, [outline[t[0] as usize], outline[t[1] as usize], outline[t[2] as usize]]));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 64 x 64 picture of dark asphalt with the given light rectangles (x0, y0, x1, y1, rgb).
    fn picture(paint: &[(usize, usize, usize, usize, [u8; 3])]) -> omsi_texture::Image {
        let (w, h) = (64usize, 64usize);
        let mut rgba = vec![255u8; w * h * 4];
        for (k, px) in rgba.chunks_exact_mut(4).enumerate() {
            let g = 70 + ((k * 7) % 9) as u8;
            px[..3].copy_from_slice(&[g, g, g]);
        }
        for &(x0, y0, x1, y1, c) in paint {
            for y in y0..=y1 {
                for x in x0..=x1 {
                    rgba[(y * w + x) * 4..(y * w + x) * 4 + 3].copy_from_slice(&c);
                }
            }
        }
        omsi_texture::Image { width: w as u32, height: h as u32, rgba, has_alpha: false }
    }

    #[test]
    fn a_dash_on_asphalt_is_one_stripe() {
        let mask = TexturePaint::read_picture(&picture(&[(2, 16, 5, 47, [200, 200, 200])])).unwrap();
        let s = &mask.bars;
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].class, TEXEL_WHITE);
        assert!((s[0].u[0] - 2.0 / 64.0).abs() < 1e-6 && (s[0].u[1] - 6.0 / 64.0).abs() < 1e-6);
        assert!((s[0].v[0] - 0.25).abs() < 1e-6 && (s[0].v[1] - 0.75).abs() < 1e-6);
        assert!(mask.painted_at(3.0 / 64.0, 0.5));
        assert!(!mask.painted_at(0.5, 0.5));
    }

    #[test]
    fn yellow_is_told_from_white() {
        let mask = TexturePaint::read_picture(&picture(&[(10, 0, 13, 63, [230, 190, 40]), (40, 0, 43, 63, [235, 235, 235])])).unwrap();
        let s = &mask.bars;
        assert_eq!(s.len(), 2);
        assert!(s.iter().any(|t| t.class == TEXEL_YELLOW && t.v == [0.0, 1.0]));
        assert!(s.iter().any(|t| t.class == TEXEL_WHITE));
    }

    #[test]
    fn a_dash_over_the_edge_of_the_picture_is_joined() {
        let mask = TexturePaint::read_picture(&picture(&[(2, 0, 5, 7, [210, 210, 210]), (2, 56, 5, 63, [210, 210, 210])])).unwrap();
        let s = &mask.bars;
        assert_eq!(s.len(), 1);
        assert!((s[0].v[0] - 56.0 / 64.0).abs() < 1e-6 && (s[0].v[1] - 1.125).abs() < 1e-6, "{:?}", s[0]);
    }

    #[test]
    fn a_light_picture_and_grit_have_no_paint() {
        let light = omsi_texture::Image { width: 8, height: 8, rgba: vec![220; 8 * 8 * 4], has_alpha: false };
        assert!(TexturePaint::read_picture(&light).is_none());
        assert!(TexturePaint::read_picture(&picture(&[(20, 20, 21, 21, [230, 230, 230])])).is_none());
    }

    #[test]
    fn a_dashed_line_is_laid_along_the_spline_by_its_texture_scale() {
        // dashes over v 0.25..0.75 of a texture that repeats every 6 m (v_scale 1/6):
        // 3 m of paint every 6 m, the first from 1.5 m on
        let mask = TexturePaint::read_picture(&picture(&[(2, 16, 5, 47, [200, 200, 200])])).unwrap();
        let p = |x: f32, u: f32| SplineProfilePoint { x, z: 0.0, u, v_scale: 1.0 / 6.0 };
        let (a, b) = (p(-4.0, 0.0), p(4.0, 1.0));
        let along = |s: f64, x: f64| Vec2::new(x as f32, s as f32);
        let paint = lane_paint(&mask, [&a, &b], 20.0, 0.0, false, &along);
        assert_eq!(paint.len(), 1);
        let tris = &paint[0].1;
        let covered: f32 = tris.chunks_exact(3).map(|t| (t[1] - t[0]).perp_dot(t[2] - t[0]).abs() * 0.5).sum();
        // x from -4 + 8*2/64 to -4 + 8*6/64 (0.5 m wide); dashes at s 1.5..4.5, 7.5..10.5, 13.5..16.5, 19.5..20
        assert!((covered - 0.5 * (3.0 * 3.0 + 0.5)).abs() < 0.05, "{covered}");
        assert!(tris.iter().all(|q| q.x > -3.76 && q.x < -3.24));
        let first = tris.iter().map(|q| q.y).fold(f32::MAX, f32::min);
        assert!((first - 1.5).abs() < 1e-3, "{first}");
    }

    #[test]
    fn a_crossing_on_a_flat_face_becomes_triangles() {
        // a square of road 2 m wide whose picture has one light bar over a quarter of it
        let dir = std::env::temp_dir().join(format!("openomsi-lane-paint-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut png = image::RgbaImage::new(64, 64);
        for (i, px) in png.pixels_mut().enumerate() {
            let y = i / 64;
            *px = if y < 16 { image::Rgba([230, 230, 230, 255]) } else { image::Rgba([72, 72, 72, 255]) };
        }
        png.save(dir.join("zebra_test.png")).unwrap();
        let faces = [
            ([Vec2::ZERO, Vec2::new(2.0, 0.0), Vec2::new(2.0, 2.0)], [Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0)], 0usize),
            ([Vec2::ZERO, Vec2::new(2.0, 2.0), Vec2::new(0.0, 2.0)], [Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0), Vec2::new(0.0, 1.0)], 0usize),
        ];
        let cache = PaintCache::default();
        let paint = face_paint(&faces, &["zebra_test.png".to_string()], &[dir.clone()], &cache);
        let area: f32 = paint.iter().map(|(_, t)| (t[1] - t[0]).perp_dot(t[2] - t[0]).abs() * 0.5).sum();
        // (the bar is a rectangle of the picture: cut exactly out of the faces)
        assert!((area - 2.0 * 2.0 * 0.25).abs() < 1e-3, "{area}");
        assert!(paint.iter().all(|(c, _)| *c == layers::PAINT_WHITE));
        // texture coordinates that are not numbers or run over a million pictures lay no paint
        // (and do not run a loop from the smallest integer)
        for far in [f32::INFINITY, f32::NAN, 3.0e9] {
            let wild = [([Vec2::ZERO, Vec2::new(2.0, 0.0), Vec2::new(2.0, 2.0)], [Vec2::new(0.0, 0.0), Vec2::new(far, 0.0), Vec2::new(far, 1.0)], 0usize)];
            assert!(face_paint(&wild, &["zebra_test.png".to_string()], &[dir.clone()], &cache).is_empty());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_mirrored_spline_lays_its_dashes_the_other_way_round() {
        // a bar over the first half of the picture's length: 0..0.5 along, then 1..1.5, ...
        let mask = TexturePaint::read_picture(&picture(&[(10, 0, 13, 31, [220, 220, 220])])).unwrap();
        let p = |x: f32, u: f32| SplineProfilePoint { x, z: 0.0, u, v_scale: 1.0 };
        let (a, b) = (p(-4.0, 0.0), p(4.0, 1.0));
        let along = |s: f64, x: f64| Vec2::new(x as f32, s as f32);
        let first = |mirror: bool| {
            let paint = lane_paint(&mask, [&a, &b], 3.0, 0.0, mirror, &along);
            paint[0].1.iter().map(|q| q.y).fold(f32::MAX, f32::min)
        };
        assert!(first(false) < 0.01, "{}", first(false));
        assert!((first(true) - 0.5).abs() < 0.01, "{}", first(true));
    }

    #[test]
    fn a_light_grey_decal_is_no_paint_but_a_white_one_is() {
        let decal = |grey: u8| {
            let mut rgba = vec![0u8; 64 * 64 * 4];
            for y in 20..36 {
                for x in 20..36 {
                    let i = (y * 64 + x) * 4;
                    rgba[i..i + 4].copy_from_slice(&[grey, grey, grey, 255]);
                }
            }
            omsi_texture::Image { width: 64, height: 64, rgba, has_alpha: true }
        };
        // (a manhole cover or a patch of lighter asphalt: nearly all of what is drawn is light)
        assert!(TexturePaint::read_picture(&decal(170)).is_none());
        assert!(TexturePaint::read_picture(&decal(250)).is_none(), "a decal that is light all over is a patch");
        let mut two = decal(250);
        for y in 20..36 {
            for x in 20..28 {
                let i = (y * 64 + x) * 4;
                two.rgba[i..i + 4].copy_from_slice(&[90, 90, 90, 255]);
            }
        }
        assert!(TexturePaint::read_picture(&two).is_some(), "white over dark in a decal is paint");
    }

    #[test]
    fn a_hair_thin_line_is_drawn_broader() {
        // two texels over 8 m of 64: 0.25 m... and one texel row of 0.125 m is widened to 0.2
        let mask = TexturePaint::read_picture(&picture(&[(10, 0, 10, 63, [220, 220, 220]), (12, 0, 12, 63, [220, 220, 220]), (30, 0, 33, 63, [220, 220, 220])])).unwrap();
        let p = |x: f32, u: f32| SplineProfilePoint { x, z: 0.0, u, v_scale: 1.0 };
        let (a, b) = (p(-4.0, 0.0), p(4.0, 1.0));
        let along = |s: f64, x: f64| Vec2::new(x as f32, s as f32);
        let paint = lane_paint(&mask, [&a, &b], 3.0, 0.0, false, &along);
        let widths: Vec<f32> = paint[0].1.chunks_exact(6).map(|q| q.iter().map(|p| p.x).fold(f32::MIN, f32::max) - q.iter().map(|p| p.x).fold(f32::MAX, f32::min)).collect();
        assert!(widths.iter().all(|w| *w >= 0.199), "{widths:?}");
    }
}
