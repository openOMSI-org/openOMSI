//! A continuous ground surface beneath aligned splines, with constrained profile seams.
use super::*;
use spade::{ConstrainedDelaunayTriangulation, Point2, Triangulation};

const STEP: f64 = 0.5;
const END_BLEND: f64 = 1.0;
const EPS: f64 = 1e-8;
const NORMAL_STEP: f64 = 0.01;

// Bound optional fitting work; exceeding this restores the caller's legacy cuts.
const MAX_PROBES: usize = 160_000;
const MAX_SEGMENTS: usize = 80_000;

#[derive(Debug)]
pub enum TerrainFitError {
    Insertion(spade::InsertionError),
    Complexity,
    Panicked,
}
impl std::fmt::Display for TerrainFitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Insertion(e) => write!(f, "{e}"),
            Self::Complexity => f.write_str("terrain fitting work budget exceeded"),
            Self::Panicked => f.write_str("terrain fitting failed internally"),
        }
    }
}
impl From<spade::InsertionError> for TerrainFitError {
    fn from(e: spade::InsertionError) -> Self {
        Self::Insertion(e)
    }
}

pub(super) type Cdt = ConstrainedDelaunayTriangulation<Point2<f64>>;

/// Solid spline backing with explicit terminal planes, in the mesh's coordinates.
/// Side shoulders follow the terrain cell width; end caps return to saved ground locally.
#[derive(Default)]
pub struct SplineTerrainProfile {
    pub mesh: MeshData,
    ends: Option<[End; 2]>,
}

#[derive(Clone, Copy)]
struct End {
    point: DVec2,
    outward: DVec2,
    width: f64,
}

impl SplineTerrainProfile {
    pub(super) fn new(mesh: MeshData, curve: &SplineCurve) -> Self {
        let ends = (!mesh.positions.is_empty()).then(|| {
            [
                -SplineCurve::dir(curve.heading_at(0.0)),
                SplineCurve::dir(curve.heading_at(curve.length)),
            ]
            .map(|outward| {
                // Include the authored terrain profile's end extensions and skew.
                let extent = mesh
                    .positions
                    .iter()
                    .map(|p| p.truncate().as_dvec2().dot(outward))
                    .fold(f64::NEG_INFINITY, f64::max);
                End {
                    point: outward * extent,
                    outward,
                    width: END_BLEND,
                }
            })
        });
        Self { mesh, ends }
    }
}

struct Rim {
    points: [DVec3; 2],
    ends: Option<[End; 2]>,
    gradient: DVec2,
    width: f64,
}

struct Field {
    triangles: Vec<[DVec3; 3]>,
    edges: Vec<Rim>,
    faces: Vec<Vec<usize>>,
    rims: Vec<Vec<usize>>,
    bins: usize,
    cap_rays: Vec<[DVec2; 2]>,
    cap_constraints: Vec<[DVec2; 2]>,
    roads: Option<Box<Field>>,
}

impl Field {
    fn new(bins: usize) -> Self {
        Self {
            triangles: Vec::new(),
            edges: Vec::new(),
            faces: vec![Vec::new(); bins * bins],
            rims: vec![Vec::new(); bins * bins],
            bins,
            cap_rays: Vec::new(),
            cap_constraints: Vec::new(),
            roads: None,
        }
    }

    fn bin(&self, p: DVec2) -> (usize, usize) {
        let p = (p / tile_size() * self.bins as f64)
            .floor()
            .clamp(DVec2::ZERO, DVec2::splat((self.bins - 1) as f64));
        (p.x as usize, p.y as usize)
    }

    fn bounds(&self, points: &[DVec3], pad: f64) -> Option<[usize; 4]> {
        let lo = points
            .iter()
            .fold(DVec2::splat(f64::INFINITY), |a, p| a.min(p.truncate()))
            - DVec2::splat(pad);
        let hi = points
            .iter()
            .fold(DVec2::splat(f64::NEG_INFINITY), |a, p| a.max(p.truncate()))
            + DVec2::splat(pad);
        if hi.x < 0.0 || hi.y < 0.0 || lo.x > tile_size() || lo.y > tile_size() {
            return None;
        }
        let (x0, y0) = self.bin(lo);
        let (x1, y1) = self.bin(hi);
        Some([x0, y0, x1, y1])
    }

    /// Conservative overlap with the spline's existing spatial index. Include a
    /// halo for road blending and normal probes; do not refine unrelated roads.
    fn affects(&self, points: &[DVec3], pad: f64) -> bool {
        let Some([x0, y0, x1, y1]) = self.bounds(points, pad) else {
            return false;
        };
        (y0..=y1).any(|y| {
            (x0..=x1).any(|x| {
                let i = y * self.bins + x;
                !self.faces[i].is_empty() || !self.rims[i].is_empty()
            })
        })
    }

    fn add_triangle(&mut self, p: [DVec3; 3]) {
        if p.iter().any(|p| !p.is_finite())
            || (p[1] - p[0])
                .truncate()
                .perp_dot((p[2] - p[0]).truncate())
                .abs()
                < EPS
        {
            return;
        }
        let Some([x0, y0, x1, y1]) = self.bounds(&p, NORMAL_STEP * 2.0) else {
            return;
        };
        let i = self.triangles.len();
        self.triangles.push(p);
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.faces[y * self.bins + x].push(i);
            }
        }
    }

    fn add_rim(&mut self, p: [DVec3; 2], ends: Option<[End; 2]>, gradient: DVec2, width: f64) {
        if p.iter().any(|p| !p.is_finite()) || (p[1] - p[0]).truncate().length_squared() < EPS {
            return;
        }
        let Some([x0, y0, x1, y1]) = self.bounds(&p, width) else {
            return;
        };
        let i = self.edges.len();
        self.edges.push(Rim {
            points: p,
            ends,
            gradient,
            width,
        });
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.rims[y * self.bins + x].push(i);
            }
        }
    }

    fn height(&self, p: DVec2, base: f64, terrain_at: &impl Fn(DVec2) -> f64) -> f64 {
        let h = self.raw_height(p, base, terrain_at);
        // Limit the new fill, without excavating pre-existing terrain over an
        // underpass or tunnel. The small clearance avoids coplanar road flicker.
        self.roads
            .as_ref()
            .map_or(h, |roads| h.min(roads.road_limit(p, h).max(base - 0.003)))
    }

    fn road_limit(&self, p: DVec2, h: f64) -> f64 {
        if let Some(z) = self.surface(p) {
            return h.min(z - 0.003);
        }
        let (x, y) = self.bin(p);
        let mut result = h;
        for &i in &self.rims[y * self.bins + x] {
            let rim = &self.edges[i];
            let [a, b] = rim.points;
            let e = (b - a).truncate();
            let t = ((p - a.truncate()).dot(e) / e.length_squared()).clamp(0.0, 1.0);
            let q = a.lerp(b, t);
            let d = p.distance(q.truncate());
            if d < rim.width {
                let s = 1.0 - d / rim.width;
                let fade = s.powi(3) * (10.0 - 15.0 * s + 6.0 * s * s);
                let z = q.z + rim.gradient.dot(p - q.truncate()) - 0.003;
                result = result.min(h - (h - z).max(0.0) * fade);
            }
        }
        result
    }

    fn surface(&self, p: DVec2) -> Option<f64> {
        let (x, y) = self.bin(p);
        let mut surface: Option<f64> = None;
        for &i in &self.faces[y * self.bins + x] {
            let [a, b, c] = self.triangles[i];
            let (u, v, d) = ((b - a).truncate(), (c - a).truncate(), p - a.truncate());
            let det = u.perp_dot(v);
            let (s, t) = (d.perp_dot(v) / det, u.perp_dot(d) / det);
            if s >= -EPS && t >= -EPS && s + t <= 1.0 + EPS {
                let z = a.z + s * (b.z - a.z) + t * (c.z - a.z);
                surface = Some(surface.map_or(z, |h| h.min(z)));
            }
        }
        surface
    }

    fn raw_height(&self, p: DVec2, base: f64, terrain_at: &impl Fn(DVec2) -> f64) -> f64 {
        if let Some(h) = self.surface(p) {
            return h;
        }
        let (x, y) = self.bin(p);
        let (mut sum, mut weights) = (0.0, 0.0);
        for &i in &self.rims[y * self.bins + x] {
            let rim = &self.edges[i];
            let [a, b] = rim.points;
            let (mut probe, mut beyond) = (p, 0.0f64);
            if let Some(ends) = rim.ends {
                for end in ends {
                    let d = (p - end.point).dot(end.outward).max(0.0);
                    probe -= end.outward * d;
                    beyond = beyond.max(d / end.width);
                }
            }
            let e = (b - a).truncate();
            let t = ((probe - a.truncate()).dot(e) / e.length_squared()).clamp(0.0, 1.0);
            let q = a.lerp(b, t);
            let d = q.truncate().distance(probe);
            if d < rim.width && beyond < 1.0 {
                let lateral = 1.0 - d / rim.width;
                let terminal = 1.0 - beyond;
                // Blend a displacement of the saved ground, not an absolute plane:
                // a lip's tangent must not flatten a nearby cliff or a curved hill.
                // Match the lip derivative locally, fading that correction within
                // one sampling step rather than extrapolating it across the bank.
                let at = q.truncate();
                let delta = p - at;
                let tangent_fade = (1.0 - delta.length() / STEP).max(0.0).powi(3);
                let mut correction = q.z - terrain_at(at);
                if tangent_fade > 0.0 {
                    let ground_gradient = DVec2::new(
                        terrain_at(at + DVec2::X * NORMAL_STEP)
                            - terrain_at(at - DVec2::X * NORMAL_STEP),
                        terrain_at(at + DVec2::Y * NORMAL_STEP)
                            - terrain_at(at - DVec2::Y * NORMAL_STEP),
                    ) / (2.0 * NORMAL_STEP);
                    correction += (rim.gradient - ground_gradient).dot(delta) * tangent_fade;
                }
                // Independent smooth fades keep the bank's shape through the cap.
                let smooth = |t: f64| t.powi(3) * (10.0 - 15.0 * t + 6.0 * t * t);
                let fade = smooth(lateral) * smooth(terminal);
                let strength = lateral * terminal;
                let weight = strength.powi(4) / d.powi(2).max(1e-16);
                sum += correction * fade * weight;
                weights += weight;
            }
        }
        if weights == 0.0 {
            return base;
        }
        base + sum / weights
    }
}

fn insert(
    cdt: &mut Cdt,
    p: DVec2,
) -> Result<spade::handles::FixedVertexHandle, spade::InsertionError> {
    let p = super::terrain_constraints::snap(p);
    cdt.insert(Point2::new(p.x, p.y))
}

fn segment(a: DVec2, b: DVec2) -> Option<[DVec2; 2]> {
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    for (a, d) in [(a.x, b.x - a.x), (a.y, b.y - a.y)] {
        if d.abs() < EPS {
            if a < 0.0 || a > tile_size() {
                return None;
            }
        } else {
            let (s, t) = (-a / d, (tile_size() - a) / d);
            lo = lo.max(s.min(t));
            hi = hi.min(s.max(t));
        }
    }
    (hi > lo).then(|| {
        [
            a.lerp(b, lo).clamp(DVec2::ZERO, DVec2::splat(tile_size())),
            a.lerp(b, hi).clamp(DVec2::ZERO, DVec2::splat(tile_size())),
        ]
    })
}

/// Fit the ground to complete spline troughs, preserving the authored terrain shape.
/// Positions in each profile mesh are offset into this tile's local coordinates.
/// Authored terrain slope changes and trough creases constrain the
/// tessellation: narrow ditches cannot disappear between five-metre terrain points.
/// Explicit object/tunnel holes remain handled separately by the cut mask.
pub fn build_fitted_terrain_mesh(
    terrain: &Terrain,
    profiles: &[(&SplineTerrainProfile, DVec3)],
) -> MeshData {
    build_fitted_terrain_mesh_with_neighbors(terrain, profiles, &[])
}

/// Use the saved ground of neighboring tiles for rims across a tile border.
/// Neighbor origins are expressed relative to this tile.
pub fn build_fitted_terrain_mesh_with_neighbors(
    terrain: &Terrain,
    profiles: &[(&SplineTerrainProfile, DVec3)],
    neighbors: &[(&Terrain, DVec2)],
) -> MeshData {
    build_fitted_terrain_mesh_with_roads(terrain, profiles, neighbors, &[])
}

/// Keep the fitted ground beneath the actual graphical road surfaces. Road
/// boundaries and slope changes constrain the triangulation, retaining every road rim.
pub fn build_fitted_terrain_mesh_with_roads(
    terrain: &Terrain,
    profiles: &[(&SplineTerrainProfile, DVec3)],
    neighbors: &[(&Terrain, DVec2)],
    roads: &[(&MeshData, DVec3)],
) -> MeshData {
    try_build_fitted_terrain_mesh_with_roads(terrain, profiles, neighbors, roads).unwrap_or_else(
        |e| {
            log::warn!("cannot fit spline terrain: {e}; keeping the saved ground");
            build_terrain_mesh(terrain)
        },
    )
}

/// Saved ground height at `p` (this tile's coordinates) on this tile or a neighbor;
/// `None` further out, where nothing is loaded.
fn saved_ground(terrain: &Terrain, neighbors: &[(&Terrain, DVec2)], p: DVec2) -> Option<f64> {
    let side = tile_size();
    let inside = |q: DVec2| q.x >= 0.0 && q.y >= 0.0 && q.x <= side && q.y <= side;
    if inside(p) {
        return Some(super::terrain_walls::height(terrain, p, side));
    }
    neighbors
        .iter()
        .find(|&&(_, origin)| inside(p - origin))
        .map(|&(t, origin)| super::terrain_walls::height(t, p - origin, side))
}

/// How far the saved ground may stand over a profile's rim before the spline is a cutting
/// (or a tunnel approach) rather than a trough: the hill is then left to the projected hole.
const CUTTING_DEPTH: f64 = 1.0;

/// Does the saved ground stand more than `CUTTING_DEPTH` above the rim of this profile
/// anywhere it is known? Such a spline keeps Omsi.exe's hole and soil walls instead of
/// pulling the hillside down to it.
pub fn profile_in_cutting(
    profile: &SplineTerrainProfile,
    offset: DVec3,
    terrain: &Terrain,
    neighbors: &[(&Terrain, DVec2)],
) -> bool {
    super::terrain_fit_surface::boundary(&profile.mesh, offset)
        .iter()
        .flat_map(|(rim, _)| rim.iter())
        .any(|p| {
            saved_ground(terrain, neighbors, p.truncate()).is_some_and(|z| z - p.z > CUTTING_DEPTH)
        })
}

/// Fit terrain while allowing the caller to restore legacy spline cuts on failure.
pub fn try_build_fitted_terrain_mesh_with_roads(
    terrain: &Terrain,
    profiles: &[(&SplineTerrainProfile, DVec3)],
    neighbors: &[(&Terrain, DVec2)],
    roads: &[(&MeshData, DVec3)],
) -> Result<MeshData, TerrainFitError> {
    omsi_cfg::panic_guard::catch(|| fit_profiles(terrain, profiles, neighbors, roads))
        .unwrap_or(Err(TerrainFitError::Panicked))
}

fn fit_profiles(
    terrain: &Terrain,
    profiles: &[(&SplineTerrainProfile, DVec3)],
    neighbors: &[(&Terrain, DVec2)],
    roads: &[(&MeshData, DVec3)],
) -> Result<MeshData, TerrainFitError> {
    if profiles
        .iter()
        .map(|(p, _)| p.mesh.indices.len())
        .sum::<usize>()
        > 600_000
    {
        return Err(TerrainFitError::Complexity);
    }
    if profiles
        .iter()
        .all(|(profile, _)| profile.mesh.indices.is_empty())
    {
        return Ok(build_terrain_mesh(terrain));
    }
    let terrain_at = |p: DVec2| {
        saved_ground(terrain, neighbors, p).unwrap_or_else(|| {
            super::terrain_walls::height(terrain, p, tile_size())
        })
    };
    // Side shoulders need a full ground cell even if the lip itself is already
    // aligned: the saved hill can rise steeply immediately beside that lip.
    // Terminal widths remain independent, so ends still close locally.
    let blend = (tile_size() / terrain.cells as f64).max(STEP);
    let mut field = Field::new(terrain.cells.clamp(1, 128));
    let mut profiles = profiles.to_vec();
    // The same inputs must choose the same CDT diagonals regardless of tile source order.
    profiles.sort_by(|(a, oa), (b, ob)| {
        a.mesh
            .positions
            .iter()
            .flat_map(|p| (p.as_dvec3() + *oa).to_array().map(f64::to_bits))
            .cmp(
                b.mesh
                    .positions
                    .iter()
                    .flat_map(|p| (p.as_dvec3() + *ob).to_array().map(f64::to_bits)),
            )
            .then(a.mesh.indices.cmp(&b.mesh.indices))
    });
    for (profile, offset) in profiles {
        let mesh = &profile.mesh;
        let ends = profile.ends.map(|ends| {
            ends.map(|e| {
                let mut end = End {
                    point: e.point + offset.truncate(),
                    ..e
                };
                for v in &mesh.positions {
                    let p = v.as_dvec3() + offset;
                    if (-1e-5..=1.0).contains(&(end.point - p.truncate()).dot(end.outward)) {
                        end.width = end.width.max(2.0 * (p.z - terrain_at(p.truncate())).abs());
                    }
                }
                end
            })
        });
        if let Some(ends) = ends {
            for end in ends {
                if !end.width.is_finite() || end.width > tile_size() {
                    return Err(TerrainFitError::Complexity);
                }
                let across = DVec2::new(-end.outward.y, end.outward.x);
                let mut span = [f64::INFINITY, f64::NEG_INFINITY];
                for v in &mesh.positions {
                    let p = v.truncate().as_dvec2() + offset.truncate();
                    let behind = (end.point - p).dot(end.outward);
                    if (-1e-5..=blend).contains(&behind) {
                        let t = p.dot(across);
                        span[0] = span[0].min(t);
                        span[1] = span[1].max(t);
                    }
                    if (-1e-5..=1.0).contains(&behind) {
                        let ray = [p, p + end.outward * (behind.max(0.0) + end.width)];
                        if let Some(ray) = segment(ray[0], ray[1]) {
                            field.cap_rays.push(ray);
                            field.cap_constraints.push(ray);
                        }
                    }
                }
                if span[0].is_finite() {
                    let anchor = end.outward * end.point.dot(end.outward);
                    let steps = (end.width / (STEP * 0.25)).ceil() as usize;
                    for i in 0..=steps {
                        let p = anchor + end.outward * (end.width * i as f64 / steps as f64);
                        if let Some(ray) = segment(
                            p + across * (span[0] - blend),
                            p + across * (span[1] + blend),
                        ) {
                            field.cap_rays.push(ray);
                        }
                        if field.cap_rays.len() > MAX_SEGMENTS {
                            return Err(TerrainFitError::Complexity);
                        }
                    }
                }
            }
        }
        for tri in mesh.indices.chunks_exact(3) {
            field.add_triangle(
                [0, 1, 2].map(|i| mesh.positions[tri[i] as usize].as_dvec3() + offset),
            );
        }
        for (points, gradient) in super::terrain_fit_surface::boundary(mesh, offset) {
            let mismatch = points
                .iter()
                .map(|p| (p.z - terrain_at(p.truncate())).abs())
                .fold(0.0, f64::max);
            field.add_rim(points, ends, gradient, (2.0 * mismatch).max(blend));
        }
    }
    // (a profile that ends a few metres outside the tile has no triangle in it, but its
    // shoulder still moves the border: the neighbor, which holds the triangles, fits it)
    if field.triangles.is_empty() && field.edges.is_empty() {
        return Ok(build_terrain_mesh(terrain));
    }
    // Retain the complete road height field, including its clearance. Only CDT
    // constraints and dense samples are culled below, avoiding artificial seams.
    if !roads.is_empty() {
        let mut ceiling = Field::new(field.bins);
        for &(mesh, offset) in roads {
            for tri in mesh.indices.chunks_exact(3) {
                ceiling.add_triangle(
                    [0, 1, 2].map(|i| mesh.positions[tri[i] as usize].as_dvec3() + offset),
                );
            }
            for (points, gradient) in super::terrain_fit_surface::boundary(mesh, offset) {
                ceiling.add_rim(points, None, gradient, STEP);
            }
        }
        field.roads = Some(Box::new(ceiling));
    }
    fit(terrain, &field, &terrain_at)
}

fn fit(
    terrain: &Terrain,
    field: &Field,
    terrain_at: &impl Fn(DVec2) -> f64,
) -> Result<MeshData, TerrainFitError> {
    let mut cdt = Cdt::new();
    let mut constraints = Vec::new();
    let mut probes = Vec::new();
    let side = tile_size();
    let cell = side / terrain.cells as f64;
    // Enclose every possible spline displacement in a frame of saved-grid
    // edges. Outside it, flat grid interiors can be omitted without allowing a
    // fitted triangle to leak into unchanged terrain. Preserve every real saved
    // slope change and every tile-border station, including inside the frame.
    let n = terrain.cells;
    let mut frame = [n, n, 0usize, 0usize];
    for y in 0..n {
        for x in 0..n {
            let a = DVec3::new(x as f64 * cell, y as f64 * cell, 0.0);
            let b = a + DVec3::new(cell, cell, 0.0);
            if field.affects(&[a, b], 2.0 * NORMAL_STEP) {
                frame[0] = frame[0].min(x.saturating_sub(1));
                frame[1] = frame[1].min(y.saturating_sub(1));
                frame[2] = frame[2].max((x + 2).min(n));
                frame[3] = frame[3].max((y + 2).min(n));
            }
        }
    }
    // The old clearance under a low, distant road is part of the height field too.
    if let Some(roads) = &field.roads {
        for y in 0..=n {
            for x in 0..=n {
                let p = DVec2::new(x as f64 * cell, y as f64 * cell);
                let base = terrain.height_at(x, y) as f64;
                if roads.affects(&[p.extend(0.0)], 2.0 * NORMAL_STEP)
                    && (field.height(p, base, terrain_at) - base).abs() > 1e-8
                {
                    frame[0] = frame[0].min(x.saturating_sub(1));
                    frame[1] = frame[1].min(y.saturating_sub(1));
                    frame[2] = frame[2].max((x + 1).min(n));
                    frame[3] = frame[3].max((y + 1).min(n));
                }
            }
        }
    }
    let gradients: Vec<_> = (0..n)
        .flat_map(|y| {
            (0..n).map(move |x| {
                let a = terrain.height_at(x, y) as f64;
                let b = terrain.height_at(x + 1, y) as f64;
                let c = terrain.height_at(x, y + 1) as f64;
                let d = terrain.height_at(x + 1, y + 1) as f64;
                [
                    DVec2::new(b - a, d - b) / cell,
                    DVec2::new(d - c, c - a) / cell,
                ]
            })
        })
        .collect();
    let crease = |a: DVec2, b: DVec2| {
        !a.is_finite() || !b.is_finite() || (a - b).abs().max_element() > 1e-10
    };
    for y in 0..=n {
        for x in 0..=n {
            let a = DVec2::new(x as f64 * cell, y as f64 * cell);
            if x >= frame[0] && x <= frame[2] && y >= frame[1] && y <= frame[3] {
                insert(&mut cdt, a)?;
            }
            let horizontal_frame =
                (y == frame[1] || y == frame[3]) && x >= frame[0] && x < frame[2];
            let vertical_frame = (x == frame[0] || x == frame[2]) && y >= frame[1] && y < frame[3];
            if x < n
                && (y == 0
                    || y == n
                    || horizontal_frame
                    || crease(gradients[y * n + x][0], gradients[(y - 1) * n + x][1]))
            {
                constraints.push([a, a + DVec2::X * cell]);
            }
            if y < n
                && (x == 0
                    || x == n
                    || vertical_frame
                    || crease(gradients[y * n + x][1], gradients[y * n + x - 1][0]))
            {
                constraints.push([a, a + DVec2::Y * cell]);
            }
            if x < n && y < n && crease(gradients[y * n + x][0], gradients[y * n + x][1]) {
                constraints.push([a, a + DVec2::splat(cell)]);
            }
        }
    }
    // Coplanar source triangles do not introduce a height crease. Keep their
    // boundary and true slope changes instead of cutting the ground at every
    // road/spline station and diagonal. The full height field is unchanged.
    let creases = sampling::creases(
        field.triangles.iter().chain(
            field
                .roads
                .iter()
                .flat_map(|r| &r.triangles)
                .filter(|tri| field.affects(&tri[..], STEP + 2.0 * NORMAL_STEP)),
        ),
    );
    for [a, b] in creases {
        if let Some(segment) = segment(a, b) {
            constraints.push(segment);
        }
    }
    // Join only straight sampling runs. The original rims, height field and all
    // profile/road crease constraints remain unchanged.
    let runs = sampling::runs(
        field
            .edges
            .iter()
            .chain(
                field
                    .roads
                    .iter()
                    .flat_map(|r| &r.edges)
                    .filter(|edge| field.affects(&edge.points, STEP + 2.0 * NORMAL_STEP)),
            )
            .map(|edge| (edge.points, edge.width))
            .collect(),
    );
    for (points, width) in runs {
        let [a, b] = points.map(|p| p.truncate());
        let normal = DVec2::new(-(b - a).y, (b - a).x).normalize();
        if !width.is_finite() || width > tile_size() {
            return Err(TerrainFitError::Complexity);
        }
        let steps = (width / STEP).ceil() as i32;
        let distances = (-steps..=steps)
            .map(|step| width * step as f64 / steps as f64)
            .chain([-0.25, -0.125, -0.06, -0.03, 0.03, 0.06, 0.125, 0.25]);
        for distance in distances {
            let Some([a, b]) = segment(a + normal * distance, b + normal * distance) else {
                continue;
            };
            let n = (a.distance(b) / STEP).ceil().max(1.0) as usize;
            if probes.len() + n + 1 > MAX_PROBES {
                return Err(TerrainFitError::Complexity);
            }
            probes.extend((0..=n).map(|i| a.lerp(b, i as f64 / n as f64)));
        }
    }
    // Keep the longitudinal profile-knot constraints: otherwise triangles from
    // a side shoulder can cross a narrow end cap and introduce a second dip.
    for &[a, b] in &field.cap_constraints {
        if let Some(ray) = segment(a, b) {
            constraints.push(ray);
        }
    }
    // All original fine cap probes still bound the height error; the transverse
    // probe lines do not need to become permanent triangulation constraints.
    for &[a, b] in &field.cap_rays {
        if let Some([a, b]) = segment(a, b) {
            let n = (a.distance(b) / (STEP * 0.25)).ceil().max(1.0) as usize;
            if probes.len() + n + 1 > MAX_PROBES {
                return Err(TerrainFitError::Complexity);
            }
            probes.extend((0..=n).map(|i| a.lerp(b, i as f64 / n as f64)));
        }
    }
    if constraints.len() > MAX_SEGMENTS {
        return Err(TerrainFitError::Complexity);
    }
    super::terrain_constraints::add(&mut cdt, &constraints, side, terrain.cells.clamp(1, 128))?;
    sampling::refine(
        &mut cdt,
        probes,
        |p| field.height(p, terrain_at(p), terrain_at),
        |p, h| {
            if (h - terrain_at(p)).abs() <= 1e-10 {
                1e-9
            } else {
                0.001
            }
        },
    )?;
    let mut mesh = MeshData::default();
    for v in cdt.vertices() {
        let p = v.position();
        let p = DVec2::new(p.x, p.y);
        let base = super::terrain_walls::height(terrain, p, side);
        mesh.positions
            .push(p.extend(field.height(p, base, terrain_at)).as_vec3());
        mesh.uvs.push((p / side).as_vec2());
        mesh.normals.push(Vec3::Z);
    }
    for face in cdt.inner_faces() {
        let [a, b, c] = face.vertices().map(|v| v.fix().index() as u32);
        mesh.indices.extend([a, b, c]);
    }
    mesh.ranges.push((0, mesh.indices.len() as u32, 0));
    for (p, normal) in mesh.positions.iter().zip(&mut mesh.normals) {
        let p = p.truncate().as_dvec2();
        let h = |q: DVec2| field.height(q, terrain_at(q), terrain_at);
        let e = NORMAL_STEP;
        let dx = (h(p + DVec2::X * e) - h(p - DVec2::X * e)) / (2.0 * e);
        let dy = (h(p + DVec2::Y * e) - h(p - DVec2::Y * e)) / (2.0 * e);
        *normal = DVec3::new(-dx, -dy, 1.0).normalize().as_vec3();
    }
    Ok(mesh)
}

#[path = "terrain_fit_sampling.rs"]
mod sampling;

#[cfg(test)]
#[path = "terrain_fit_tests.rs"]
mod tests;
