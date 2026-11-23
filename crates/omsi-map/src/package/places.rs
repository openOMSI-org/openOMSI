//! What a package says about places: the icons on the map, the regions (municipalities) and the names,
//! and the named streets with their speed limits, to tell which street and which municipality one is in.
//! Read from `markers.cfg`, `regions.cfg` and the lines and labels of `surface.oms`.

use super::{Package, Rd, LINE_STREET, LINE_STREET_LIMITS};

/// A name on the map: where, what kind (0 district, 1 terminal, 2 town, 3 landmark, 4 water), how important
/// (the author's number, high shows from far away) and the text.
#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    pub at: (f64, f64),
    pub kind: u8,
    pub rank: u8,
    pub name: String,
}

/// An icon on the map: where (world metres), the picture's path under the package folder and how wide it is
/// drawn (pixels at interface size 1). The file's last column, a name, is for people reading it.
#[derive(Clone, Debug, PartialEq)]
pub struct Marker {
    pub at: (f64, f64),
    pub icon: String,
    pub width_px: f32,
    pub name: String,
}

/// A region of the map (a municipality): its outline, to say where the player is, and the box its name fits in
/// (`box_w` 0: no room for a readable name, so none is drawn).
#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    pub name: String,
    pub label: (f64, f64),
    pub box_w: f64,
    pub box_h: f64,
    pub outline: Vec<(f64, f64)>,
}

/// A street found by place: its name and its speed limits (km/h, 0 = not said).
#[derive(Clone, Debug, PartialEq)]
pub struct Street {
    pub name: String,
    pub speed: u32,
    /// For heavy vehicles, when it is not the same.
    pub heavy: u32,
}

/// Whether `rel` names a file inside the package folder: a relative path of plain names, with `/` (or `\`) between them. Not one that climbs
/// out (`..`), and not an absolute or rooted one (`/tmp/a.png`, `\a.png`, `C:\a.png`, `\\server\share\a.png`): `Path::join` takes such a path
/// instead of the folder, so the icon would be read from wherever the author's machine has it. A colon is refused anywhere for the same reason:
/// a package that works only on its author's computer is no package.
pub fn is_inside(rel: &str) -> bool {
    let rel = rel.replace('\\', "/");
    !rel.is_empty() && !rel.contains(':') && !rel.starts_with('/') && rel.split('/').all(|c| !c.is_empty() && c != "." && c != "..")
}

impl Package {
    /// Reads `regions.cfg` and finds out whether the surface has named streets (called once, by `open`).
    pub(super) fn load_places(&mut self) {
        self.regions = self.read_regions();
        self.has_streets = self.index.iter().any(|e| self.data.get(e.at..e.at + e.len).is_some_and(chunk_has_streets));
    }

    /// A place of the author's grid (metres, as `markers.cfg` and `regions.cfg` write them) as world metres.
    pub fn author_to_world(&self, ax: f64, ay: f64) -> (f64, f64) {
        let (tx, ty) = ((ax / self.grid).floor(), (ay / self.grid).floor());
        crate::tile_local_to_world(tx as i32, ty as i32, ax - tx * self.grid, ay - ty * self.grid)
    }

    /// World metres as metres of the author's grid (the inverse of `author_to_world`).
    fn world_to_author(&self, p: (f64, f64)) -> (f64, f64) {
        let ((tx, ty), (lx, ly)) = crate::world_to_tile_local(p.0, p.1);
        (tx as f64 * self.grid + lx, ty as f64 * self.grid + ly)
    }

    fn name(&self, i: u32) -> String {
        self.names.get(i as usize).cloned().unwrap_or_default()
    }

    /// Every name of the map, once (the file is small: a few hundred).
    pub fn labels(&self) -> Vec<Label> {
        let mut out = Vec::new();
        for e in &self.index {
            if let Some(body) = self.data.get(e.at..e.at + e.len) {
                let _ = self.chunk_labels(body, &mut out);
            }
        }
        out
    }

    /// The labels at the end of a chunk: after its fills and its lines.
    fn chunk_labels(&self, body: &[u8], out: &mut Vec<Label>) -> Option<()> {
        let mut r = Rd { b: body, i: 0 };
        for _ in 0..r.u32()? {
            r.u8()?;
            let n = r.u32()? as usize;
            r.take(n.checked_mul(24)?)?;
        }
        for _ in 0..r.u32()? {
            r.u8()?;
            r.u8()?;
            r.f32()?;
            r.u32()?;
            let n = r.u32()? as usize;
            r.take(n.checked_mul(8)?)?;
        }
        for _ in 0..r.u32()? {
            let (kind, rank, name) = (r.u8()?, r.u8()?, r.u32()?);
            let at = self.to_world(r.f32()?, r.f32()?);
            out.push(Label { at, kind, rank, name: self.name(name) });
        }
        Some(())
    }

    /// `markers.cfg`: `marker = icon | width px | x | y | name`, x and y in metres of the author's grid. A line that
    /// cannot be read is left out.
    pub fn markers(&self) -> Vec<Marker> {
        let Ok(bytes) = omsi_cfg::vfs::read(&self.dir.join("markers.cfg")) else {
            return Vec::new();
        };
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let mut out = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            if line.starts_with('#') || !k.trim().eq_ignore_ascii_case("marker") {
                continue;
            }
            let f: Vec<&str> = v.split('|').map(str::trim).collect();
            if f.len() < 4 || !is_inside(f[0]) {
                continue;
            }
            let (Ok(width), Ok(x), Ok(y)) = (f[1].parse::<f32>(), f[2].parse::<f64>(), f[3].parse::<f64>()) else {
                continue;
            };
            if !(width > 0.0 && width < 512.0) || !x.is_finite() || !y.is_finite() {
                continue;
            }
            out.push(Marker { at: self.author_to_world(x, y), icon: f[0].to_string(), width_px: width, name: f.get(4).copied().unwrap_or("").to_string() });
        }
        out
    }

    /// A file of the package folder (an icon), when its path lies inside it (see [`is_inside`]).
    pub fn picture(&self, rel: &str) -> Option<Vec<u8>> {
        if !is_inside(rel) {
            return None;
        }
        omsi_cfg::vfs::read(&self.dir.join(rel.replace('\\', "/"))).ok()
    }

    /// `regions.cfg`: `region = name | label x | label y | box width | box height | x,y x,y ...`, metres of the
    /// author's grid. A line that cannot be read is left out.
    fn read_regions(&self) -> Vec<Region> {
        let Ok(bytes) = omsi_cfg::vfs::read(&self.dir.join("regions.cfg")) else {
            return Vec::new();
        };
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let mut out = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            if line.starts_with('#') || !k.trim().eq_ignore_ascii_case("region") {
                continue;
            }
            let f: Vec<&str> = v.split('|').map(str::trim).collect();
            if f.len() < 6 || f[0].is_empty() {
                continue;
            }
            let num = |i: usize| f[i].parse::<f64>().ok().filter(|v| v.is_finite());
            let (Some(x), Some(y), Some(w), Some(h)) = (num(1), num(2), num(3), num(4)) else {
                continue;
            };
            let outline: Vec<(f64, f64)> = f[5]
                .split_whitespace()
                .filter_map(|p| {
                    let (a, b) = p.split_once(',')?;
                    Some(self.author_to_world(a.parse().ok()?, b.parse().ok()?))
                })
                .collect();
            if outline.len() < 3 {
                continue;
            }
            out.push(Region { name: f[0].to_string(), label: self.author_to_world(x, y), box_w: w.max(0.0), box_h: h.max(0.0), outline });
        }
        out
    }

    pub fn regions(&self) -> &[Region] {
        &self.regions
    }

    /// The package can say where the player is (municipality outlines or named streets).
    pub fn has_location(&self) -> bool {
        !self.regions.is_empty() || self.has_streets
    }

    /// The municipality and the street at a world position: the region whose outline holds it, and the named
    /// street within 30 m (read from the chunks around, nothing kept).
    pub fn locate(&self, pos: (f64, f64)) -> (Option<String>, Option<String>) {
        let region = self.regions.iter().find(|r| point_in_poly(&r.outline, pos)).map(|r| r.name.clone());
        (region, self.street_info(pos, 30.0).map(|s| s.name))
    }

    /// The named street within `reach` metres of a world position, with its limits (read from the chunks around,
    /// nothing kept).
    pub fn street_info(&self, pos: (f64, f64), reach: f64) -> Option<Street> {
        if !self.has_streets {
            return None;
        }
        let a = self.world_to_author(pos);
        let a = (a.0 - self.origin.0, a.1 - self.origin.1);
        let (cx, cy) = ((a.0 / self.chunk).floor() as i32, (a.1 / self.chunk).floor() as i32);
        let mut best: Option<(f64, u32, u32, u32)> = None;
        for dx in -1..=1 {
            for dy in -1..=1 {
                let Some(&i) = self.by_key.get(&(cx + dx, cy + dy)) else {
                    continue;
                };
                let e = &self.index[i];
                if let Some(body) = self.data.get(e.at..e.at + e.len) {
                    let _ = street_in_chunk(body, a, &mut best);
                }
            }
        }
        best.filter(|b| b.0 <= reach)
            .map(|b| Street { name: self.name(b.1), speed: b.2, heavy: if b.3 == b.2 { 0 } else { b.3 } })
            .filter(|s| !s.name.is_empty())
    }
}

fn point_in_poly(poly: &[(f64, f64)], p: (f64, f64)) -> bool {
    if poly.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0 {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn dist_to_segment(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let ab = (b.0 - a.0, b.1 - a.1);
    let l2 = ab.0 * ab.0 + ab.1 * ab.1;
    let t = if l2 > 1e-9 { (((p.0 - a.0) * ab.0 + (p.1 - a.1) * ab.1) / l2).clamp(0.0, 1.0) } else { 0.0 };
    let (dx, dy) = (a.0 + ab.0 * t - p.0, a.1 + ab.1 * t - p.1);
    (dx * dx + dy * dy).sqrt()
}

/// Whether a chunk has named streets (class 3 or 4 lines).
fn chunk_has_streets(body: &[u8]) -> bool {
    let mut r = Rd { b: body, i: 0 };
    let scan = |r: &mut Rd| -> Option<bool> {
        for _ in 0..r.u32()? {
            r.u8()?;
            let n = r.u32()? as usize;
            r.take(n.checked_mul(24)?)?;
        }
        for _ in 0..r.u32()? {
            let class = r.u8()?;
            r.u8()?;
            r.f32()?;
            r.u32()?;
            let n = r.u32()? as usize;
            if class == LINE_STREET || class == LINE_STREET_LIMITS {
                return Some(true);
            }
            r.take(n.checked_mul(8)?)?;
        }
        Some(false)
    };
    scan(&mut r).unwrap_or(false)
}

/// The nearest street piece of a chunk to `p` (metres of the author's grid from the file's origin): `best` is
/// (distance, name index, limit, heavy limit), kept when nearer than what it holds.
fn street_in_chunk(body: &[u8], p: (f64, f64), best: &mut Option<(f64, u32, u32, u32)>) -> Option<()> {
    let mut r = Rd { b: body, i: 0 };
    for _ in 0..r.u32()? {
        r.u8()?;
        let n = r.u32()? as usize;
        r.take(n.checked_mul(24)?)?;
    }
    for _ in 0..r.u32()? {
        let class = r.u8()?;
        let flags = r.u8()?;
        let width = r.f32()?;
        let name = r.u32()?;
        let n = r.u32()? as usize;
        let (speed, heavy) = if class == LINE_STREET_LIMITS { (width.max(0.0).round() as u32, flags as u32) } else { (0, 0) };
        if class != LINE_STREET && class != LINE_STREET_LIMITS {
            r.take(n.checked_mul(8)?)?;
            continue;
        }
        let mut prev: Option<(f64, f64)> = None;
        for _ in 0..n {
            let q = (r.f32()? as f64, r.f32()? as f64);
            if let Some(a) = prev {
                let d = dist_to_segment(p, a, q);
                if best.is_none_or(|b| d < b.0) {
                    *best = Some((d, name, speed, heavy));
                }
            }
            prev = Some(q);
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_is_in_a_polygon_or_not() {
        let sq = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        assert!(point_in_poly(&sq, (5.0, 5.0)));
        assert!(!point_in_poly(&sq, (15.0, 5.0)));
        assert!(!point_in_poly(&sq[..2], (5.0, 5.0)));
    }

    #[test]
    fn distance_to_a_segment_stops_at_its_ends() {
        assert!((dist_to_segment((5.0, 3.0), (0.0, 0.0), (10.0, 0.0)) - 3.0).abs() < 1e-9);
        assert!((dist_to_segment((13.0, 4.0), (0.0, 0.0), (10.0, 0.0)) - 5.0).abs() < 1e-9);
        assert!((dist_to_segment((3.0, 4.0), (0.0, 0.0), (0.0, 0.0)) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn only_a_relative_path_inside_the_package_is_an_icon() {
        for good in ["icons/stop.png", "icons\\stop.png", "stop.png", "a/b/c.png"] {
            assert!(super::is_inside(good), "{good}");
        }
        for bad in ["", ".", "..", "../a.png", "icons/../../a.png", "icons\\..\\a.png", "/a.png", "/tmp/icon.png", "\\a.png", "\\\\server\\share\\a.png", "C:\\a.png", "C:/a.png", "C:a.png", "icons//a.png", "./a.png"] {
            assert!(!super::is_inside(bad), "{bad}");
        }
    }

    #[test]
    fn the_example_names_its_places() {
        // docs/examples/openbus-realmap, in the tile grid of the tutorial map: a street through (494, 730)
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/examples");
        let p = Package::open(&dir).expect("docs/examples/openbus-realmap is a package");
        assert_eq!(p.markers().len(), 2);
        assert!(p.picture("icons/stop.png").is_some_and(|b| b.len() > 4 && &b[1..4] == b"PNG"));
        assert!(p.picture("../manifest.cfg").is_none(), "a path cannot climb out of the folder");
        for bad in ["/tmp/icon.png", "\\icon.png", "C:/icons/stop.png", "C:\\icons\\stop.png", "C:stop.png", "\\\\server\\share\\stop.png", "icons/../../manifest.cfg"] {
            assert!(p.picture(bad).is_none(), "{bad} is not inside the package folder");
        }
        assert_eq!(p.regions().len(), 1);
        assert!(p.has_location());
        let mut names: Vec<String> = p.labels().into_iter().map(|l| l.name).collect();
        names.sort();
        assert_eq!(names, ["Exampleton", "Lakeside", "Parkfield"]);
        let (city, street) = p.locate((494.0, 730.0));
        assert_eq!((city.as_deref(), street.as_deref()), (Some("Exampleton"), Some("Example Street")));
        let s = p.street_info((494.0, 730.0), 30.0).expect("a street within 30 m");
        assert_eq!((s.speed, s.heavy), (30, 0));
        // far from every street and outside the region: nothing
        assert_eq!(p.locate((5000.0, 5000.0)), (None, None));
        assert!(p.street_info((494.0, 1100.0), 30.0).is_none());
    }
}
