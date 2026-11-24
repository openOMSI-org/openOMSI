//! What a wheel stands on: the upward faces of a tile on a coarse grid (DriveGrid), and the
//! `.surf` maps that make a flat road face shake the bus.

use super::*;

/// How far outside a road face a wheel's point may lie and still stand on it (m). Two spline
/// segments meeting end to end each work out their seam's vertices for themselves, and the
/// two edges come out a fraction of a millimetre apart: a point that fell into that sliver met
/// neither face and dropped through to whatever lay under the road (a car's wheel fell 18 cm
/// onto the ground for a frame where Spandau's Falkenseer Chaussee joins its next segment, and
/// the cars bounced at the seam). Omsi.exe's own meshes are drawn without such gaps showing;
/// a few millimetres closes them and is lost in a tyre's footprint.
pub const SEAM_TOLERANCE: f32 = 0.005;

/// The barycentric weights of (x, y) in the plan view of triangle `a b c`, when the point
/// lies inside it or no farther than `tol` metres from the finite triangle. Outside
/// points use the nearest point on its boundary, so heights and UVs are not extrapolated.
fn plan_weights(a: Vec3, b: Vec3, c: Vec3, x: f32, y: f32, tol: f32) -> Option<(f32, f32, f32)> {
    let [a, b, c] = [a, b, c].map(|v| v.truncate().as_dvec2());
    let p = DVec2::new(x as f64, y as f64);
    let d = (b - a).perp_dot(c - a);
    if d.abs() < 1e-9 {
        return None;
    }
    let l2 = (p - a).perp_dot(c - a) / d;
    let l3 = (b - a).perp_dot(p - a) / d;
    let l1 = 1.0 - l2 - l3;
    if l1 >= 0.0 && l2 >= 0.0 && l3 >= 0.0 {
        return Some((l1 as f32, l2 as f32, l3 as f32));
    }

    // Offsetting infinite edge lines creates a long wedge beyond an acute tip.
    // Measure against the three finite segments instead, retaining the 5 mm seam.
    let mut best_distance = f64::MAX;
    let mut weights = (0.0, 0.0, 0.0);
    for (u, v, edge) in [(a, b, 0), (b, c, 1), (c, a, 2)] {
        let e = v - u;
        let t = ((p - u).dot(e) / e.length_squared()).clamp(0.0, 1.0);
        let distance = p.distance_squared(u + e * t);
        if distance < best_distance {
            best_distance = distance;
            weights = match edge {
                0 => (1.0 - t, t, 0.0),
                1 => (0.0, 1.0 - t, t),
                _ => (t, 0.0, 1.0 - t),
            };
        }
    }
    (best_distance <= (tol as f64).powi(2)).then_some((
        weights.0 as f32,
        weights.1 as f32,
        weights.2 as f32,
    ))
}

/// The box outside which [`plan_weights`] accepts no point of `abc`, matching the
/// seam-expanded bounds used to bucket the triangle in [`DriveGrid::build`].
fn plan_reach(a: Vec3, b: Vec3, c: Vec3, tol: f32) -> [f32; 4] {
    let (lo, hi) = (a.min(b).min(c), a.max(b).max(c));
    [lo.x - tol, lo.y - tol, hi.x + tol, hi.y + tol]
}

/// A `.surf` map: a picture beside a road texture (`str_kopfgr01.bmp.surf`) whose red
/// channel lies over that texture's own coordinates. Where a wheel stands on a face drawn
/// with the texture, OMSI 2 moves the ground by [`HeightMap::AMPLITUDE`] × (2·red − 1):
/// cobbles, slabs and broken asphalt shake the bus over a face that is flat (#886).
#[derive(Debug, Clone, Default)]
pub struct HeightMap {
    pub width: usize,
    pub height: usize,
    /// The red channel, row by row from the top (v = 0).
    pub red: Vec<u8>,
}

impl HeightMap {
    /// How far the ground moves at full red or black (m).
    pub const AMPLITUDE: f32 = 0.02;

    /// From an RGBA picture (row by row from the top).
    pub fn from_rgba(width: usize, height: usize, rgba: &[u8]) -> Option<HeightMap> {
        (width > 0 && height > 0 && rgba.len() >= width * height * 4).then(|| HeightMap {
            width,
            height,
            red: rgba
                .chunks_exact(4)
                .take(width * height)
                .map(|p| p[0])
                .collect(),
        })
    }

    /// The ground's lift (m) at texture coordinates `uv`, tiled and filtered between four
    /// texels as OMSI 2 reads it: no wrap at the picture's edge - the position stops at the
    /// last texel, and there the second last is read (a strip one texel wide, level).
    pub fn lift(&self, uv: Vec2) -> f32 {
        let at = |t: f32, n: usize| {
            let p = (t.rem_euclid(1.0) * n as f32).min(n as f32 - 1.0);
            let f = p - p.floor();
            let i = (p as usize).min(n.saturating_sub(2));
            (i, (i + 1).min(n - 1), f)
        };
        let (x0, x1, fx) = at(uv.x, self.width);
        let (y0, y1, fy) = at(uv.y, self.height);
        let r = |x: usize, y: usize| self.red[y * self.width + x] as f32 / 255.0;
        let top = r(x0, y0) * (1.0 - fx) + r(x1, y0) * fx;
        let bottom = r(x0, y1) * (1.0 - fx) + r(x1, y1) * fx;
        Self::AMPLITUDE * (2.0 * (top * (1.0 - fy) + bottom * fy) - 1.0)
    }
}

/// The `.surf` maps a mesh's faces are drawn with: its texture coordinates and material
/// ranges (as in its [`MeshData`]) and the map of each material slot's texture (none past
/// the end). Kept beside a mesh whose own ranges mean something else (a spline's drive mesh).
#[derive(Debug, Clone, Default)]
pub struct SurfFaces {
    pub uvs: Vec<Vec2>,
    pub ranges: Vec<(u32, u32, u32)>,
    pub maps: Vec<Option<std::sync::Arc<HeightMap>>>,
    /// OMSI's `[surface]` id of each material slot's texture (asphalt past the end): what
    /// the tyres roll on there (see [`DriveGrid::surface_at`]).
    pub ids: Vec<u8>,
}

impl SurfFaces {
    /// Only where a texture has a map, and the mesh texture coordinates to lay it on.
    pub fn of(mesh: &MeshData, maps: &[Option<std::sync::Arc<HeightMap>>]) -> Option<SurfFaces> {
        (maps.iter().any(Option::is_some) && mesh.uvs.len() == mesh.positions.len()).then(|| {
            SurfFaces {
                uvs: mesh.uvs.clone(),
                ranges: mesh.ranges.clone(),
                maps: maps.to_vec(),
                ids: Vec::new(),
            }
        })
    }

    /// [`SurfFaces::of`] with the `[surface]` ids of the slots' textures: kept where a
    /// texture has a map or a surface other than asphalt.
    pub fn tagged(
        mesh: &MeshData,
        maps: &[Option<std::sync::Arc<HeightMap>>],
        ids: &[u8],
    ) -> Option<SurfFaces> {
        if !ids.iter().any(|i| *i != 0) {
            return Self::of(mesh, maps);
        }
        let uvs = if mesh.uvs.len() == mesh.positions.len() {
            mesh.uvs.clone()
        } else {
            Vec::new()
        };
        let maps = if uvs.is_empty() {
            Vec::new()
        } else {
            maps.to_vec()
        };
        Some(SurfFaces {
            uvs,
            ranges: mesh.ranges.clone(),
            maps,
            ids: ids.to_vec(),
        })
    }

    /// The `[surface]` id of face `j`.
    pub(crate) fn id(&self, j: usize) -> u8 {
        if self.ids.is_empty() {
            return 0;
        }
        let first = (j * 3) as u32;
        self.ranges
            .iter()
            .find(|r| first >= r.0 && first < r.0 + r.1)
            .and_then(|r| self.ids.get(r.2 as usize))
            .copied()
            .unwrap_or(0)
    }

    /// The map and corner texture coordinates of face `j` (indices `tri`).
    pub(crate) fn face(
        &self,
        j: usize,
        tri: &[u32],
    ) -> Option<(&std::sync::Arc<HeightMap>, [Vec2; 3])> {
        let first = (j * 3) as u32;
        let r = self
            .ranges
            .iter()
            .find(|r| first >= r.0 && first < r.0 + r.1)?;
        let m = self.maps.get(r.2 as usize)?.as_ref()?;
        let uv = [0, 1, 2].map(|k| self.uvs.get(tri[k] as usize).copied());
        Some((m, [uv[0]?, uv[1]?, uv[2]?]))
    }

    /// Bytes held on the heap (the maps are shared).
    pub fn heap_bytes(&self) -> usize {
        self.uvs.capacity() * 8
            + self.ranges.capacity() * 12
            + self.maps.capacity() * 8
            + self.ids.capacity()
    }
}

/// A triangle without a `.surf` map in [`DriveGrid`].
const NO_BUMP: u32 = u32::MAX;

/// Upward-facing triangles of one tile (tile-local x/y in metres, absolute z), bucketed on a
/// coarse grid so that a wheel asks only the few faces around it.
#[derive(Debug, Clone, Default)]
pub struct DriveGrid {
    pub tris: Vec<[Vec3; 3]>,
    /// Per triangle: a wall top ([`is_wall_top`]), never stood on - a wall where it stands
    /// over the ground (see [`DriveGrid::probe_walls`]).
    pub ridge: Vec<bool>,
    /// Per triangle: its entry in `bumps`, or `NO_BUMP` where its texture has no `.surf`.
    bump_of: Vec<u32>,
    /// The `.surf` map (index into `maps`) and the texture coordinates of the corners.
    bumps: Vec<(u32, [Vec2; 3])>,
    maps: Vec<std::sync::Arc<HeightMap>>,
    /// Per triangle, the `[surface]` id of its texture (empty while every face is asphalt).
    surface: Vec<u8>,
    pub(crate) cells: usize,
    pub(crate) cell: f32,
    /// Per cell, the range of `items` that lists its triangles (`cells² + 1` offsets).
    pub(crate) start: Vec<u32>,
    pub(crate) items: Vec<u32>,
    /// Per triangle, the box (min x, min y, max x, max y) outside which [`plan_weights`] never accepts a point.
    reach: Vec<[f32; 4]>,
}

impl DriveGrid {
    /// Edge of one bucket (m).
    pub const CELL: f32 = 4.0;

    /// Bytes the grid holds on the heap.
    pub fn heap_bytes(&self) -> usize {
        self.tris.capacity() * std::mem::size_of::<[Vec3; 3]>()
            + self.ridge.capacity()
            + self.bump_of.capacity() * 4
            + self.bumps.capacity() * std::mem::size_of::<(u32, [Vec2; 3])>()
            + self.start.capacity() * 4
            + self.items.capacity() * 4
            + self.reach.capacity() * 16
            + self.surface.capacity()
    }

    /// Add a triangle; walls (faces steeper than about 70°) are left out, they are nothing
    /// to stand on.
    pub fn push(&mut self, p: [Vec3; 3]) {
        self.push_kind(p, false);
    }

    /// Add a triangle, a wall top or not.
    pub fn push_kind(&mut self, p: [Vec3; 3], ridge: bool) -> bool {
        let nrm = (p[1] - p[0]).cross(p[2] - p[0]);
        let len = nrm.length();
        if len < 1e-6 || nrm.z.abs() / len < 0.3 {
            return false;
        }
        self.tris.push(p);
        self.ridge.push(ridge);
        self.bump_of.push(NO_BUMP);
        true
    }

    /// Add a triangle drawn with a texture that has a `.surf` map, with the texture
    /// coordinates of its corners.
    pub fn push_surf(
        &mut self,
        p: [Vec3; 3],
        uv: [Vec2; 3],
        map: &std::sync::Arc<HeightMap>,
    ) -> bool {
        if !self.push_kind(p, false) {
            return false;
        }
        let m = match self
            .maps
            .iter()
            .position(|m| std::sync::Arc::ptr_eq(m, map))
        {
            Some(m) => m,
            None => {
                self.maps.push(map.clone());
                self.maps.len() - 1
            }
        };
        if let Some(b) = self.bump_of.last_mut() {
            *b = self.bumps.len() as u32;
        }
        self.bumps.push((m as u32, uv));
        true
    }

    /// Per `[surface]` id among the faces: how many, and the middle of one of them (for
    /// `OMSI_DEBUG_SURFACES`: where to find a cobbled road).
    pub fn surface_examples(&self) -> Vec<(u8, usize, Vec3)> {
        let mut out: Vec<(u8, usize, Vec3)> = Vec::new();
        for (i, t) in self.tris.iter().enumerate() {
            let id = self.surface.get(i).copied().unwrap_or(0);
            let mid = (t[0] + t[1] + t[2]) / 3.0;
            match out.iter_mut().find(|e| e.0 == id) {
                Some(e) => e.1 += 1,
                None => out.push((id, 1, mid)),
            }
        }
        out.sort_by_key(|e| e.0);
        out
    }

    /// Give the face added last the `[surface]` id `id`.
    pub fn tag_last(&mut self, id: u8) {
        if id == 0 || self.tris.is_empty() {
            return;
        }
        self.surface.resize(self.tris.len(), 0);
        if let Some(s) = self.surface.last_mut() {
            *s = id;
        }
    }

    /// The highest face (not a wall top) at or below `top` over tile-local (x, y): its height
    /// and `[surface]` id - what the tyre standing there rolls on.
    pub fn surface_at(&self, x: f32, y: f32, top: f32) -> Option<(f32, u8)> {
        if self.cells == 0 || x < 0.0 || y < 0.0 {
            return None;
        }
        let (cx, cy) = ((x / self.cell) as usize, (y / self.cell) as usize);
        if cx >= self.cells || cy >= self.cells {
            return None;
        }
        let k = cy * self.cells + cx;
        let mut best: Option<(f32, u8)> = None;
        for &i in &self.items[self.start[k] as usize..self.start[k + 1] as usize] {
            if self.ridge.get(i as usize).copied().unwrap_or(false) || !self.reaches(i, x, y) {
                continue;
            }
            let [a, b, c] = self.tris[i as usize];
            let Some((l1, l2, l3)) = plan_weights(a, b, c, x, y, SEAM_TOLERANCE) else {
                continue;
            };
            let z = l1 * a.z + l2 * b.z + l3 * c.z;
            if z <= top && best.is_none_or(|(old, _)| z > old) {
                best = Some((z, self.surface.get(i as usize).copied().unwrap_or(0)));
            }
        }
        best
    }

    /// Bucket the triangles of a tile `tile` metres wide; those entirely outside are dropped.
    pub fn build(&mut self, tile: f32) {
        let n = ((tile / Self::CELL).ceil() as usize).max(1);
        self.cells = n;
        self.cell = tile / n as f32;
        let mut ranges: Vec<(usize, usize, usize, usize)> = Vec::with_capacity(self.tris.len());
        let mut keep = Vec::with_capacity(self.tris.len());
        let mut keep_ridge = Vec::with_capacity(self.tris.len());
        let mut keep_bump = Vec::with_capacity(self.tris.len());
        let tagged = !self.surface.is_empty();
        let mut keep_surface = Vec::with_capacity(if tagged { self.tris.len() } else { 0 });
        self.ridge.resize(self.tris.len(), false);
        self.bump_of.resize(self.tris.len(), NO_BUMP);
        if tagged {
            self.surface.resize(self.tris.len(), 0);
        }
        for (k, ((t, r), b)) in self
            .tris
            .iter()
            .zip(self.ridge.iter())
            .zip(self.bump_of.iter())
            .enumerate()
        {
            // (bucketed with the seam tolerance round it: a point that close is on it)
            let e = SEAM_TOLERANCE;
            let (lo_x, hi_x) = (
                t[0].x.min(t[1].x).min(t[2].x) - e,
                t[0].x.max(t[1].x).max(t[2].x) + e,
            );
            let (lo_y, hi_y) = (
                t[0].y.min(t[1].y).min(t[2].y) - e,
                t[0].y.max(t[1].y).max(t[2].y) + e,
            );
            if hi_x < 0.0 || hi_y < 0.0 || lo_x > tile || lo_y > tile {
                continue;
            }
            let c = |v: f32| ((v / self.cell).floor().max(0.0) as usize).min(n - 1);
            ranges.push((c(lo_x), c(hi_x), c(lo_y), c(hi_y)));
            keep.push(*t);
            keep_ridge.push(*r);
            keep_bump.push(*b);
            if tagged {
                keep_surface.push(self.surface[k]);
            }
        }
        self.reach = keep
            .iter()
            .map(|t| plan_reach(t[0], t[1], t[2], SEAM_TOLERANCE))
            .collect();
        self.tris = keep;
        self.ridge = keep_ridge;
        self.bump_of = keep_bump;
        self.surface = keep_surface;
        let mut count = vec![0u32; n * n + 1];
        for &(x0, x1, y0, y1) in &ranges {
            for y in y0..=y1 {
                for x in x0..=x1 {
                    count[y * n + x] += 1;
                }
            }
        }
        let mut start = vec![0u32; n * n + 1];
        for k in 0..n * n {
            start[k + 1] = start[k] + count[k];
        }
        let mut fill = start.clone();
        let mut items = vec![0u32; start[n * n] as usize];
        for (i, &(x0, x1, y0, y1)) in ranges.iter().enumerate() {
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let k = y * n + x;
                    items[fill[k] as usize] = i as u32;
                    fill[k] += 1;
                }
            }
        }
        self.start = start;
        self.items = items;
    }

    /// The faces over tile-local (x, y): the highest one not above `z_top` and the lowest one
    /// above it.
    pub fn probe(&self, x: f32, y: f32, z_top: f32) -> Probe {
        self.probe_kind(x, y, z_top, false)
    }

    /// Highest road face below the point, with its upward geometric normal.
    /// Reflections need the actual plane rather than a raster texel's height.
    pub fn surface_below(&self, x: f32, y: f32, top: f32) -> Option<(f32, Vec3)> {
        if self.cells == 0 || x < 0.0 || y < 0.0 {
            return None;
        }
        let (cx, cy) = ((x / self.cell) as usize, (y / self.cell) as usize);
        if cx >= self.cells || cy >= self.cells {
            return None;
        }
        let k = cy * self.cells + cx;
        let mut best: Option<(f32, Vec3)> = None;
        for &i in &self.items[self.start[k] as usize..self.start[k + 1] as usize] {
            if self.ridge.get(i as usize).copied().unwrap_or(false) || !self.reaches(i, x, y) {
                continue;
            }
            let [a, b, c] = self.tris[i as usize];
            let Some((l1, l2, l3)) = plan_weights(a, b, c, x, y, SEAM_TOLERANCE) else {
                continue;
            };
            let z = l1 * a.z + l2 * b.z + l3 * c.z;
            if z <= top && best.is_none_or(|(old, _)| z > old) {
                let n = (b - a).cross(c - a).normalize();
                best = Some((z, if n.z < 0.0 { -n } else { n }));
            }
        }
        best
    }

    /// The wall tops over tile-local (x, y) alone, as [`DriveGrid::probe`] gives the rest.
    pub fn probe_walls(&self, x: f32, y: f32, z_top: f32) -> Probe {
        self.probe_kind(x, y, z_top, true)
    }

    /// The road heights over (x, y) into `road` (their count), the highest wall top into `walls`.
    pub fn heights(&self, x: f32, y: f32, road: &mut [f32], walls: &mut Option<f32>) -> usize {
        if self.cells == 0 || x < 0.0 || y < 0.0 {
            return 0;
        }
        let (cx, cy) = ((x / self.cell) as usize, (y / self.cell) as usize);
        if cx >= self.cells || cy >= self.cells {
            return 0;
        }
        let k = cy * self.cells + cx;
        let mut n = 0;
        for &i in &self.items[self.start[k] as usize..self.start[k + 1] as usize] {
            if !self.reaches(i, x, y) {
                continue;
            }
            let [a, b, c] = self.tris[i as usize];
            let Some((l1, l2, l3)) = plan_weights(a, b, c, x, y, SEAM_TOLERANCE) else {
                continue;
            };
            let mut z = l1 * a.z + l2 * b.z + l3 * c.z;
            if let Some(&(m, uv)) = self
                .bump_of
                .get(i as usize)
                .and_then(|&b| self.bumps.get(b as usize))
            {
                z += self.maps[m as usize].lift(uv[0] * l1 + uv[1] * l2 + uv[2] * l3);
            }
            if self.ridge.get(i as usize).copied().unwrap_or(false) {
                *walls = Probe {
                    below: *walls,
                    above: None,
                }
                .merge(Probe::of(z, f32::MAX))
                .below;
            } else {
                if let Some(slot) = road.get_mut(n) {
                    *slot = z;
                }
                n += 1;
            }
        }
        n
    }

    #[inline]
    fn reaches(&self, i: u32, x: f32, y: f32) -> bool {
        self.reach
            .get(i as usize)
            .is_none_or(|r| x >= r[0] && y >= r[1] && x <= r[2] && y <= r[3])
    }

    fn probe_kind(&self, x: f32, y: f32, z_top: f32, ridges: bool) -> Probe {
        let mut out = Probe::default();
        if self.cells == 0 || x < 0.0 || y < 0.0 {
            return out;
        }
        let (cx, cy) = ((x / self.cell) as usize, (y / self.cell) as usize);
        if cx >= self.cells || cy >= self.cells {
            return out;
        }
        let k = cy * self.cells + cx;
        for &i in &self.items[self.start[k] as usize..self.start[k + 1] as usize] {
            if self.ridge.get(i as usize).copied().unwrap_or(false) != ridges
                || !self.reaches(i, x, y)
            {
                continue;
            }
            let [a, b, c] = self.tris[i as usize];
            let Some((l1, l2, l3)) = plan_weights(a, b, c, x, y, SEAM_TOLERANCE) else {
                continue;
            };
            let mut z = l1 * a.z + l2 * b.z + l3 * c.z;
            if let Some(&(m, uv)) = self
                .bump_of
                .get(i as usize)
                .and_then(|&b| self.bumps.get(b as usize))
            {
                z += self.maps[m as usize].lift(uv[0] * l1 + uv[1] * l2 + uv[2] * l3);
            }
            out = out.merge(Probe::of(z, z_top));
        }
        out
    }
}
