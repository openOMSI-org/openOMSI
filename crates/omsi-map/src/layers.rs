//! A map's picture for the navigator, the city map and the launcher, worked out from the map
//! itself (`omsi_scene::mapgen` reads the tiles): the ground by what it is painted with, water,
//! woods and trees, every paved surface as it lies (carriageways, pavements, junctions, car
//! parks), rail and tram tracks, and the houses as footprints with their heights.
//!
//! Nothing here comes from outside the map: a map's author need not ship anything (a package,
//! `crate::package`, still adds what it has). The layers are kept on disk after the first read
//! (`~/.openomsi/cache/mapgen`), keyed by the tiles' sizes and dates, so a map is read once.
//!
//! Positions are world metres relative to `origin` (an `f32` cannot hold a big map's metres to
//! the centimetre). Areas are triangles, ready to draw; buildings are outlines (anticlockwise)
//! with their triangulated roofs.

use glam::{DVec2, Vec2, Vec3};

/// What the ground is, by the texture it is painted with.
pub const LAND_GRASS: usize = 0;
pub const LAND_FOREST: usize = 1;
pub const LAND_FIELD: usize = 2;
pub const LAND_PAVED: usize = 3;
pub const LAND_SAND: usize = 4;
pub const LAND_WATER: usize = 5;
pub const LAND_CLASSES: usize = 6;

/// What a paved surface is.
pub const PAVE_ROAD: usize = 0;
pub const PAVE_FOOT: usize = 1;
pub const PAVE_AREA: usize = 2;
/// A spline's or a junction's grass strip or island.
pub const PAVE_GREEN: usize = 3;
/// The streets' bright middle: more than a pavement's breadth from their edge.
pub const PAVE_INNER: usize = 4;
pub const PAVE_CLASSES: usize = 5;

/// What a painted marking is: lane lines, crossings, stop lines and arrows are white, a
/// no-parking or bus lane line yellow.
pub const PAINT_WHITE: usize = 0;
pub const PAINT_AMBER: usize = 1;
pub const PAINT_CLASSES: usize = 2;

/// The lines: a railway, a tram track (in a street or on its own bed), a river or stream.
pub const LINE_RAIL: u8 = 0;
pub const LINE_TRAM: u8 = 1;
pub const LINE_RIVER: u8 = 2;
/// The outlines of the carriageways and of the yards and car parks (closed rings).
pub const LINE_ROAD_EDGE: u8 = 3;
pub const LINE_AREA_EDGE: u8 = 4;

/// A house: its outline (anticlockwise, not closed), its roof's triangles (indices into the
/// outline) and how tall it is (m). `open`: a roof on posts or over a street (a canopy, a
/// bridge, a carport) with no walls under it: drawn as a roof only, so the way under it shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Building {
    pub outline: Vec<Vec2>,
    pub roof: Vec<[u16; 3]>,
    pub height: f32,
    pub open: bool,
    /// The building's model is drawn in 3D (`Layers::instances`), not a prism of the outline.
    pub has_mesh: bool,
    /// Which of `Layers::instances` it is a part of (`u32::MAX`: none): the 3D map draws the
    /// footprint instead when it has not the budget to draw the model.
    pub inst: u32,
}

/// A placed model: which of `Layers::models`, where (relative to the layers' origin) and which
/// way (compass heading, degrees).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Instance {
    pub model: u32,
    pub at: Vec2,
    pub heading: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Line {
    pub class: u8,
    pub width: f32,
    pub points: Vec<Vec2>,
}

/// The whole map's layers.
#[derive(Clone, Debug, Default)]
pub struct Layers {
    pub origin: DVec2,
    /// The tiles of the map (x0, y0, x1, y1 relative to `origin`): the ground outside them is
    /// the plain land colour.
    pub tiles: Vec<[f32; 4]>,
    /// Per land class: triangles (three points each).
    pub land: Vec<Vec<Vec2>>,
    /// Per paved class: triangles.
    pub paved: Vec<Vec<Vec2>>,
    pub buildings: Vec<Building>,
    /// The faces of the models the buildings are (x, y, z up from the model's ground) and where
    /// they stand: what the 3D map draws, untextured.
    pub models: Vec<Vec<[Vec3; 3]>>,
    pub instances: Vec<Instance>,
    /// Per marking class: triangles of the paint on the roads, as their textures lay it.
    pub paint: Vec<Vec<Vec2>>,
    /// Single trees: x, y and the crown's radius.
    pub trees: Vec<[f32; 3]>,
    pub lines: Vec<Line>,
    mapped: Option<std::sync::Arc<Mapped>>,
}

#[derive(Debug)]
struct Mapped {
    map: memmap2::Mmap,
    land: Vec<std::ops::Range<usize>>,
    paved: Vec<std::ops::Range<usize>>,
    paint: Vec<std::ops::Range<usize>>,
}

impl Mapped {
    fn points(&self, ranges: &[std::ops::Range<usize>], class: usize) -> &[Vec2] {
        ranges.get(class).map_or(&[], |r| bytemuck::cast_slice(&self.map[r.clone()]))
    }
}

const MAGIC: &[u8; 4] = b"OMML";
/// Bumped whenever the generator changes what it makes: an older cache is read again.
pub const VERSION: u32 = 19;

impl Layers {
    pub fn empty(origin: DVec2) -> Layers {
        Layers { origin, land: vec![Vec::new(); LAND_CLASSES], paved: vec![Vec::new(); PAVE_CLASSES], paint: vec![Vec::new(); PAINT_CLASSES], ..Default::default() }
    }

    pub fn land_of(&self, class: usize) -> &[Vec2] {
        match &self.mapped {
            Some(m) => m.points(&m.land, class),
            None => self.land.get(class).map_or(&[], Vec::as_slice),
        }
    }

    pub fn paved_of(&self, class: usize) -> &[Vec2] {
        match &self.mapped {
            Some(m) => m.points(&m.paved, class),
            None => self.paved.get(class).map_or(&[], Vec::as_slice),
        }
    }

    pub fn paint_of(&self, class: usize) -> &[Vec2] {
        match &self.mapped {
            Some(m) => m.points(&m.paint, class),
            None => self.paint.get(class).map_or(&[], Vec::as_slice),
        }
    }

    pub fn open(path: &std::path::Path, key: u64) -> Option<Layers> {
        let file = std::fs::File::open(path).ok()?;
        let map = unsafe { memmap2::Mmap::map(&file) }.ok()?;
        if cfg!(target_endian = "big") {
            return Layers::from_bytes(&map, key);
        }
        let mut ranges = [Vec::new(), Vec::new(), Vec::new()];
        let mut out = Layers::read(&map, key, Some(&mut ranges))?;
        let [land, paved, paint] = ranges;
        out.mapped = Some(std::sync::Arc::new(Mapped { map, land, paved, paint }));
        Some(out)
    }

    /// The extent of everything (relative), for the zoom limits.
    pub fn bounds(&self) -> Option<(Vec2, Vec2)> {
        let mut lo = Vec2::splat(f32::MAX);
        let mut hi = Vec2::splat(f32::MIN);
        for t in &self.tiles {
            lo = lo.min(Vec2::new(t[0], t[1]));
            hi = hi.max(Vec2::new(t[2], t[3]));
        }
        (lo.x <= hi.x).then_some((lo, hi))
    }

    pub fn to_bytes(&self, key: u64) -> Vec<u8> {
        let mut w = Vec::with_capacity(1 << 20);
        w.extend_from_slice(MAGIC);
        w.extend_from_slice(&VERSION.to_le_bytes());
        w.extend_from_slice(&key.to_le_bytes());
        w.extend_from_slice(&self.origin.x.to_le_bytes());
        w.extend_from_slice(&self.origin.y.to_le_bytes());
        let u32_ = |w: &mut Vec<u8>, v: usize| w.extend_from_slice(&(v as u32).to_le_bytes());
        let f = |w: &mut Vec<u8>, v: f32| w.extend_from_slice(&v.to_le_bytes());
        let pts = |w: &mut Vec<u8>, p: &[Vec2]| {
            w.extend_from_slice(&(p.len() as u32).to_le_bytes());
            for q in p {
                w.extend_from_slice(&q.x.to_le_bytes());
                w.extend_from_slice(&q.y.to_le_bytes());
            }
        };
        u32_(&mut w, self.tiles.len());
        for t in &self.tiles {
            t.iter().for_each(|v| f(&mut w, *v));
        }
        for (classes, of) in [(LAND_CLASSES, Layers::land_of as fn(&Layers, usize) -> &[Vec2]), (PAVE_CLASSES, Layers::paved_of), (PAINT_CLASSES, Layers::paint_of)] {
            u32_(&mut w, classes);
            for class in 0..classes {
                pts(&mut w, of(self, class));
            }
        }
        u32_(&mut w, self.buildings.len());
        for b in &self.buildings {
            pts(&mut w, &b.outline);
            u32_(&mut w, b.roof.len());
            for t in &b.roof {
                t.iter().for_each(|i| w.extend_from_slice(&i.to_le_bytes()));
            }
            f(&mut w, b.height);
            w.push(b.open as u8 | (b.has_mesh as u8) << 1);
            u32_(&mut w, b.inst as usize);
        }
        u32_(&mut w, self.models.len());
        for m in &self.models {
            u32_(&mut w, m.len());
            for t in m {
                for p in t {
                    f(&mut w, p.x);
                    f(&mut w, p.y);
                    f(&mut w, p.z);
                }
            }
        }
        u32_(&mut w, self.instances.len());
        for i in &self.instances {
            u32_(&mut w, i.model as usize);
            f(&mut w, i.at.x);
            f(&mut w, i.at.y);
            f(&mut w, i.heading);
        }
        u32_(&mut w, self.trees.len());
        for t in &self.trees {
            t.iter().for_each(|v| f(&mut w, *v));
        }
        u32_(&mut w, self.lines.len());
        for l in &self.lines {
            w.push(l.class);
            f(&mut w, l.width);
            pts(&mut w, &l.points);
        }
        w
    }

    /// The layers in `b` if they were made by this version for `key`; None for anything else
    /// (another version, another key, a short or damaged file).
    pub fn from_bytes(b: &[u8], key: u64) -> Option<Layers> {
        Layers::read(b, key, None)
    }

    fn read(b: &[u8], key: u64, mut ranges: Option<&mut [Vec<std::ops::Range<usize>>; 3]>) -> Option<Layers> {
        let mut r = Reader { b, at: 0 };
        if r.take(4)? != MAGIC || r.u32()? != VERSION || r.u64()? != key {
            return None;
        }
        let origin = DVec2::new(r.f64()?, r.f64()?);
        let mut out = Layers { origin, ..Default::default() };
        for _ in 0..r.count(16)? {
            out.tiles.push([r.f32()?, r.f32()?, r.f32()?, r.f32()?]);
        }
        for (k, classes) in [LAND_CLASSES, PAVE_CLASSES, PAINT_CLASSES].into_iter().enumerate() {
            let mut owned = Vec::new();
            for _ in 0..r.count(8)? {
                match ranges.as_deref_mut() {
                    Some(ranges) => ranges[k].push(r.points_at()?),
                    None => owned.push(r.points()?),
                }
            }
            owned.resize(classes, Vec::new());
            *[&mut out.land, &mut out.paved, &mut out.paint][k] = owned;
        }
        for _ in 0..r.count(17)? {
            let outline = r.points()?;
            let mut roof = Vec::new();
            for _ in 0..r.count(6)? {
                let t = [r.u16()?, r.u16()?, r.u16()?];
                if t.iter().any(|&i| i as usize >= outline.len()) {
                    return None;
                }
                roof.push(t);
            }
            let height = r.f32()?;
            let flags = r.take(1)?[0];
            let inst = r.u32()?;
            out.buildings.push(Building { outline, roof, height, open: flags & 1 != 0, has_mesh: flags & 2 != 0, inst });
        }
        for _ in 0..r.count(4)? {
            let n = r.count(36)?;
            let mut m = Vec::with_capacity(n);
            for _ in 0..n {
                let mut t = [Vec3::ZERO; 3];
                for p in t.iter_mut() {
                    *p = Vec3::new(r.f32()?, r.f32()?, r.f32()?);
                }
                m.push(t);
            }
            out.models.push(m);
        }
        for _ in 0..r.count(16)? {
            let model = r.u32()?;
            if model as usize >= out.models.len() {
                return None;
            }
            out.instances.push(Instance { model, at: Vec2::new(r.f32()?, r.f32()?), heading: r.f32()? });
        }
        for _ in 0..r.count(12)? {
            out.trees.push([r.f32()?, r.f32()?, r.f32()?]);
        }
        for _ in 0..r.count(9)? {
            let class = r.take(1)?[0];
            let width = r.f32()?;
            out.lines.push(Line { class, width, points: r.points()? });
        }
        Some(out)
    }
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let s = self.b.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(s)
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    fn f32(&mut self) -> Option<f32> {
        Some(f32::from_le_bytes(self.take(4)?.try_into().ok()?)).filter(|v| v.is_finite())
    }
    fn f64(&mut self) -> Option<f64> {
        Some(f64::from_le_bytes(self.take(8)?.try_into().ok()?)).filter(|v| v.is_finite())
    }
    /// A count of items at least `min` bytes each: never more than the bytes left allow.
    fn count(&mut self, min: usize) -> Option<usize> {
        let n = self.u32()? as usize;
        (n.checked_mul(min)? <= self.b.len() - self.at).then_some(n)
    }
    fn points(&mut self) -> Option<Vec<Vec2>> {
        let n = self.count(8)?;
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            v.push(Vec2::new(self.f32()?, self.f32()?));
        }
        Some(v)
    }
    fn points_at(&mut self) -> Option<std::ops::Range<usize>> {
        let n = self.count(8)?;
        let start = self.at;
        for _ in 0..n * 2 {
            self.f32()?;
        }
        (start % 4 == 0).then_some(start..self.at)
    }
}

/// Where the layers of a map are kept: `~/.openomsi/cache/mapgen/<name>.bin`.
pub fn cache_path(map_name: &str) -> Option<std::path::PathBuf> {
    let base = if cfg!(target_os = "ios") {
        std::path::PathBuf::from(std::env::var_os("OPENOMSI_CONTAINER")?).join("Library").join("Caches")
    } else {
        std::path::PathBuf::from(std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?).join(".openomsi").join("cache")
    };
    let safe: String = map_name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    base.is_absolute().then(|| base.join("mapgen").join(format!("{safe}.bin")))
}

// --- geometry helpers the generator and the drawing share --------------------------------

/// Twice the signed area of a polygon (positive: anticlockwise).
pub fn area2(p: &[Vec2]) -> f32 {
    let n = p.len();
    (0..n).map(|i| p[i].perp_dot(p[(i + 1) % n])).sum()
}

/// A simple polygon's triangles (ear clipping; the outline anticlockwise). Degenerate corners
/// are dropped; a polygon the clipping cannot finish (it crosses itself) gives what it has.
pub fn triangulate(p: &[Vec2]) -> Vec<[u16; 3]> {
    let n = p.len();
    if n < 3 || n > u16::MAX as usize {
        return Vec::new();
    }
    let mut idx: Vec<usize> = (0..n).collect();
    if area2(p) < 0.0 {
        idx.reverse();
    }
    let mut out = Vec::with_capacity(n - 2);
    let inside = |a: Vec2, b: Vec2, c: Vec2, q: Vec2| {
        let d1 = (b - a).perp_dot(q - a);
        let d2 = (c - b).perp_dot(q - b);
        let d3 = (a - c).perp_dot(q - c);
        d1 > 1e-7 && d2 > 1e-7 && d3 > 1e-7
    };
    let mut guard = 0;
    while idx.len() > 3 && guard < n * n {
        guard += 1;
        let m = idx.len();
        let mut clipped = false;
        for i in 0..m {
            let (ia, ib, ic) = (idx[(i + m - 1) % m], idx[i], idx[(i + 1) % m]);
            let (a, b, c) = (p[ia], p[ib], p[ic]);
            let cross = (b - a).perp_dot(c - b);
            if cross.abs() < 1e-9 {
                // (a straight corner: no triangle, just drop it)
                idx.remove(i);
                clipped = true;
                break;
            }
            if cross < 0.0 {
                continue;
            }
            if idx.iter().any(|&j| j != ia && j != ib && j != ic && inside(a, b, c, p[j])) {
                continue;
            }
            out.push([ia as u16, ib as u16, ic as u16]);
            idx.remove(i);
            clipped = true;
            break;
        }
        if !clipped {
            break;
        }
    }
    if idx.len() == 3 {
        out.push([idx[0] as u16, idx[1] as u16, idx[2] as u16]);
    }
    out
}

/// A closed outline with the corners dropped that lie within `tol` of the line through their
/// neighbours (Douglas-Peucker on a ring, split at its two farthest points).
pub fn simplify_ring(p: &[Vec2], tol: f32) -> Vec<Vec2> {
    let n = p.len();
    if n < 4 {
        return p.to_vec();
    }
    let far = (1..n).max_by(|&a, &b| p[a].distance_squared(p[0]).total_cmp(&p[b].distance_squared(p[0]))).unwrap_or(n / 2);
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[far] = true;
    let mut stack = vec![(0usize, far), (far, n)];
    while let Some((a, b)) = stack.pop() {
        let (pa, pb) = (p[a], p[b % n]);
        let ab = pb - pa;
        let len = ab.length().max(1e-6);
        let mut worst = (0.0f32, 0usize);
        for k in a + 1..b {
            let d = (p[k] - pa).perp_dot(ab).abs() / len;
            if d > worst.0 {
                worst = (d, k);
            }
        }
        if worst.0 > tol {
            keep[worst.1] = true;
            stack.push((a, worst.1));
            stack.push((worst.1, b));
        }
    }
    p.iter().zip(keep).filter(|(_, k)| *k).map(|(q, _)| *q).collect()
}

/// The outlines of the cells set in a grid (`w` x `h`, row-major, row 0 at y = 0), traced along
/// the cells' edges (each ring anticlockwise around the set cells, holes clockwise), in cell
/// units. Used for the houses' footprints and the ground's areas.
pub fn trace_cells(cells: &[bool], w: usize, h: usize) -> Vec<Vec<Vec2>> {
    let at = |x: i64, y: i64| x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h && cells[y as usize * w + x as usize];
    // directed edges with the set cell on their left: keyed by start corner
    let mut edges: hashbrown::HashMap<(i64, i64), Vec<(i64, i64)>> = hashbrown::HashMap::new();
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            if !at(x, y) {
                continue;
            }
            if !at(x, y - 1) {
                edges.entry((x, y)).or_default().push((x + 1, y));
            }
            if !at(x + 1, y) {
                edges.entry((x + 1, y)).or_default().push((x + 1, y + 1));
            }
            if !at(x, y + 1) {
                edges.entry((x + 1, y + 1)).or_default().push((x, y + 1));
            }
            if !at(x - 1, y) {
                edges.entry((x, y + 1)).or_default().push((x, y));
            }
        }
    }
    let mut rings = Vec::new();
    while let Some(&start) = edges.keys().next() {
        let mut ring = vec![start];
        let mut cur = start;
        let mut prev_dir = (0i64, 0i64);
        loop {
            let Some(outs) = edges.get_mut(&cur) else { break };
            // (at a corner two set cells touch: turn left first, which keeps rings simple)
            let pick = if outs.len() > 1 {
                let left = (-prev_dir.1, prev_dir.0);
                outs.iter().position(|o| (o.0 - cur.0, o.1 - cur.1) == left).unwrap_or(0)
            } else {
                0
            };
            let next = outs.swap_remove(pick);
            if outs.is_empty() {
                edges.remove(&cur);
            }
            prev_dir = (next.0 - cur.0, next.1 - cur.1);
            cur = next;
            if cur == start {
                break;
            }
            ring.push(cur);
        }
        // drop the corners on straight runs
        let pts: Vec<Vec2> = ring.iter().map(|&(x, y)| Vec2::new(x as f32, y as f32)).collect();
        let m = pts.len();
        let kept: Vec<Vec2> = (0..m).filter(|&i| (pts[i] - pts[(i + m - 1) % m]).perp_dot(pts[(i + 1) % m] - pts[i]).abs() > 1e-6).map(|i| pts[i]).collect();
        if kept.len() >= 3 {
            rings.push(kept);
        }
    }
    rings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_survive_the_disk() {
        let mut l = Layers::empty(DVec2::new(1000.5, -20.25));
        l.tiles.push([0.0, 0.0, 300.0, 300.0]);
        l.land[LAND_FOREST] = vec![Vec2::ZERO, Vec2::X, Vec2::Y];
        l.paved[PAVE_ROAD] = vec![Vec2::ZERO, Vec2::X * 4.0, Vec2::Y];
        l.paint[PAINT_AMBER] = vec![Vec2::ZERO, Vec2::X, Vec2::Y];
        let outline = vec![Vec2::ZERO, Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), Vec2::new(0.0, 8.0)];
        l.buildings.push(Building { roof: triangulate(&outline), outline, height: 12.5, open: true, has_mesh: true, inst: 0 });
        l.models.push(vec![[Vec3::ZERO, Vec3::X, Vec3::Z]]);
        l.instances.push(Instance { model: 0, at: Vec2::new(3.0, 4.0), heading: 90.0 });
        l.trees.push([5.0, 6.0, 3.0]);
        l.lines.push(Line { class: LINE_TRAM, width: 1.5, points: vec![Vec2::ZERO, Vec2::ONE] });
        let b = l.to_bytes(42);
        let r = Layers::from_bytes(&b, 42).unwrap();
        assert_eq!(r.buildings, l.buildings);
        assert_eq!(r.models, l.models);
        assert_eq!(r.instances, l.instances);
        assert_eq!(r.lines, l.lines);
        assert_eq!(r.land[LAND_FOREST].len(), 3);
        assert_eq!(r.paint[PAINT_AMBER].len(), 3);
        assert!(Layers::from_bytes(&b, 43).is_none());
        assert!(Layers::from_bytes(&b[..b.len() - 3], 42).is_none());
    }

    #[test]
    fn mapped_layers_read_the_triangles_from_the_file() {
        let mut l = Layers::empty(DVec2::new(5.0, 6.0));
        l.land[LAND_WATER] = vec![Vec2::ZERO, Vec2::X, Vec2::Y];
        l.paved[PAVE_INNER] = vec![Vec2::new(1.5, 2.5), Vec2::new(3.0, 4.0), Vec2::new(-7.25, 8.0), Vec2::ONE, Vec2::X, Vec2::Y];
        l.paint[PAINT_WHITE] = vec![Vec2::ONE, Vec2::NEG_ONE, Vec2::X];
        l.trees.push([1.0, 2.0, 3.0]);
        let dir = std::env::temp_dir().join(format!("omsi-layers-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("map.bin");
        let b = l.to_bytes(9);
        std::fs::write(&path, &b).unwrap();
        let m = Layers::open(&path, 9).expect("mapped");
        assert!(m.mapped.is_some() && m.land.iter().chain(&m.paved).chain(&m.paint).all(Vec::is_empty));
        for class in 0..PAVE_CLASSES {
            assert_eq!(m.paved_of(class), l.paved_of(class));
        }
        assert_eq!(m.land_of(LAND_WATER), &l.land[LAND_WATER][..]);
        assert_eq!(m.paint_of(PAINT_WHITE), &l.paint[PAINT_WHITE][..]);
        assert!(m.land_of(LAND_CLASSES).is_empty());
        assert_eq!(m.trees, l.trees);
        assert_eq!(m.to_bytes(9), b);
        assert!(Layers::open(&path, 10).is_none());
        drop(m);
        let mut bad = b.clone();
        let at = b.windows(4).position(|w| w == 1.5f32.to_le_bytes()).unwrap();
        bad[at..at + 4].copy_from_slice(&f32::NAN.to_le_bytes());
        std::fs::write(&path, &bad).unwrap();
        assert!(Layers::open(&path, 9).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_last_instances_of_a_file_without_trees_or_lines_are_read_back() {
        // (an instance is a u32 and three f32: 16 bytes, so the count check must ask for 16 each,
        // or the layers after them - none here - are what a file would have to still hold)
        let mut l = Layers::empty(DVec2::ZERO);
        l.models.push(vec![[Vec3::ZERO, Vec3::X, Vec3::Z]]);
        for i in 0..3 {
            l.instances.push(Instance { model: 0, at: Vec2::new(i as f32, 2.0), heading: 10.0 * i as f32 });
        }
        let bytes = l.to_bytes(7);
        let r = Layers::from_bytes(&bytes, 7).expect("the layers read back");
        assert_eq!(r.instances, l.instances);
        assert!(r.trees.is_empty() && r.lines.is_empty());
    }

    #[test]
    fn an_l_shaped_house_is_triangulated_inside_its_outline() {
        let p = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 4.0), Vec2::new(4.0, 4.0), Vec2::new(4.0, 10.0), Vec2::new(0.0, 10.0)];
        let t = triangulate(&p);
        assert_eq!(t.len(), 4);
        let area: f32 = t.iter().map(|t| (p[t[1] as usize] - p[t[0] as usize]).perp_dot(p[t[2] as usize] - p[t[0] as usize]) * 0.5).sum();
        assert!((area - 64.0).abs() < 1e-3, "{area}");
    }

    #[test]
    fn cells_trace_to_their_outline() {
        // an L of three cells
        let cells = [true, true, true, false];
        let rings = trace_cells(&cells, 2, 2);
        assert_eq!(rings.len(), 1);
        assert_eq!(rings[0].len(), 6);
        assert!((area2(&rings[0]) * 0.5 - 3.0).abs() < 1e-6);
    }
}
