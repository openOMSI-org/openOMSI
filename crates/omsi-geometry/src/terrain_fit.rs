//! A continuous ground surface beneath aligned splines, with constrained profile seams.
use super::*;
use spade::{ConstrainedDelaunayTriangulation, Point2, Triangulation};

const STEP: f64 = 0.5;
const END_BLEND: f64 = 1.0;
const EPS: f64 = 1e-8;
const NORMAL_STEP: f64 = 0.01;

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
                let strength = (1.0 - d / rim.width) * (1.0 - beyond);
                // Blend a displacement of the saved ground, not an absolute plane:
                // a lip's tangent must not flatten a nearby cliff or a curved hill.
                // Match the lip derivative locally, fading that correction within
                // one sampling step rather than extrapolating it across the bank.
                let at = q.truncate();
                let ground_gradient = DVec2::new(
                    terrain_at(at + DVec2::X * NORMAL_STEP)
                        - terrain_at(at - DVec2::X * NORMAL_STEP),
                    terrain_at(at + DVec2::Y * NORMAL_STEP)
                        - terrain_at(at - DVec2::Y * NORMAL_STEP),
                ) / (2.0 * NORMAL_STEP);
                let delta = p - at;
                let tangent_fade = (1.0 - delta.length() / STEP).max(0.0).powi(3);
                let correction = q.z - terrain_at(at)
                    + (rim.gradient - ground_gradient).dot(delta) * tangent_fade;
                let fade = strength.powi(3) * (10.0 - 15.0 * strength + 6.0 * strength * strength);
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

/// Clip a surface triangle to the tile. Interpolate the actual 3D surface at seams.
fn clip(mut p: Vec<DVec3>) -> Vec<DVec3> {
    for (axis, bound, sign) in [
        (0, 0.0, 1.0),
        (0, tile_size(), -1.0),
        (1, 0.0, 1.0),
        (1, tile_size(), -1.0),
    ] {
        let mut out = Vec::new();
        for i in 0..p.len() {
            let (a, b) = (p[i], p[(i + 1) % p.len()]);
            let (da, db) = ((a[axis] - bound) * sign, (b[axis] - bound) * sign);
            if da >= 0.0 {
                out.push(a);
            }
            if (da >= 0.0) != (db >= 0.0) {
                let mut q = a.lerp(b, da / (da - db));
                q[axis] = bound;
                out.push(q);
            }
        }
        p = out;
    }
    p
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

/// Fit the ground to complete spline troughs, retaining every terrain triangle.
/// Positions in each profile mesh are offset into this tile's local coordinates.
/// Both the old terrain diagonals and the trough's triangle edges constrain the
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

/// Keep the fitted ground beneath the actual graphical road surfaces. Road triangle
/// edges constrain the same triangulation, so no triangle can cut across a road rim.
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

/// Fit terrain while allowing the caller to restore legacy spline cuts on failure.
pub fn try_build_fitted_terrain_mesh_with_roads(
    terrain: &Terrain,
    profiles: &[(&SplineTerrainProfile, DVec3)],
    neighbors: &[(&Terrain, DVec2)],
    roads: &[(&MeshData, DVec3)],
) -> Result<MeshData, spade::InsertionError> {
    if profiles
        .iter()
        .all(|(profile, _)| profile.mesh.indices.is_empty())
    {
        return Ok(build_terrain_mesh(terrain));
    }
    let terrain_at = |p: DVec2| {
        let side = tile_size();
        if p.x < 0.0 || p.y < 0.0 || p.x > side || p.y > side {
            for &(t, origin) in neighbors {
                let q = p - origin;
                if q.x >= 0.0 && q.y >= 0.0 && q.x <= side && q.y <= side {
                    return super::terrain_walls::height(t, q, side);
                }
            }
        }
        super::terrain_walls::height(terrain, p, side)
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
                        field
                            .cap_rays
                            .push([p, p + end.outward * (behind.max(0.0) + end.width)]);
                    }
                }
                if span[0].is_finite() {
                    let anchor = end.outward * end.point.dot(end.outward);
                    let steps = (end.width / (STEP * 0.25)).ceil() as usize;
                    for i in 0..=steps {
                        let p = anchor + end.outward * (end.width * i as f64 / steps as f64);
                        field.cap_rays.push([
                            p + across * (span[0] - blend),
                            p + across * (span[1] + blend),
                        ]);
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
    if field.triangles.is_empty() {
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
) -> Result<MeshData, spade::InsertionError> {
    let mut cdt = Cdt::new();
    let mut constraints = Vec::new();
    let side = tile_size();
    let cell = side / terrain.cells as f64;
    // Constrain the original grid and diagonal, preserving the authored surface
    // exactly outside the local transition. Every tile shares the same seam grid.
    for y in 0..=terrain.cells {
        for x in 0..=terrain.cells {
            let a = DVec2::new(x as f64 * cell, y as f64 * cell);
            insert(&mut cdt, a)?;
            if x < terrain.cells {
                constraints.push([a, a + DVec2::X * cell]);
            }
            if y < terrain.cells {
                constraints.push([a, a + DVec2::Y * cell]);
            }
            if x < terrain.cells && y < terrain.cells {
                constraints.push([a, a + DVec2::splat(cell)]);
            }
        }
    }
    for tri in field.triangles.iter().chain(
        field
            .roads
            .iter()
            .flat_map(|r| &r.triangles)
            .filter(|tri| field.affects(&tri[..], STEP + 2.0 * NORMAL_STEP)),
    ) {
        let p = clip(tri.to_vec());
        for i in 0..p.len() {
            constraints.push([p[i].truncate(), p[(i + 1) % p.len()].truncate()]);
        }
    }
    // Put the blend's samples only beside a rim, not over the entire tile. Inserting
    // into the constrained mesh also splits existing edges instead of making cracks.
    for edge in field.edges.iter().chain(
        field
            .roads
            .iter()
            .flat_map(|r| &r.edges)
            .filter(|edge| field.affects(&edge.points, STEP + 2.0 * NORMAL_STEP)),
    ) {
        let [a, b] = edge.points;
        let d = (b - a).truncate();
        let normal = DVec2::new(-d.y, d.x).normalize();
        let steps = (edge.width / STEP).ceil() as i32;
        let distances = (-steps..=steps)
            .map(|step| edge.width * step as f64 / steps as f64)
            .chain([-0.25, -0.125, -0.06, -0.03, 0.03, 0.06, 0.125, 0.25]);
        for distance in distances {
            let Some([a, b]) = segment(
                a.truncate() + normal * distance,
                b.truncate() + normal * distance,
            ) else {
                continue;
            };
            // Retain the clipped endpoints on tile seams too. The blend is a smooth
            // field, not a crease: constraining its offset lines would introduce
            // nearly coincident intersections with the actual profile constraints.
            insert(&mut cdt, a)?;
            insert(&mut cdt, b)?;
            let n = (a.distance(b) / STEP).ceil().max(1.0) as usize;
            for i in 0..=n {
                insert(&mut cdt, a.lerp(b, i as f64 / n as f64))?;
            }
        }
    }
    // Resolve the short cap along its profile knots, rather than letting sparse
    // side-shoulder samples triangulate a second dip after the visible ditch ends.
    for &[a, b] in &field.cap_rays {
        if let Some([a, b]) = segment(a, b) {
            constraints.push([a, b]);
            let n = (a.distance(b) / (STEP * 0.25)).ceil().max(1.0) as usize;
            for i in 0..=n {
                insert(&mut cdt, a.lerp(b, i as f64 / n as f64))?;
            }
        }
    }
    super::terrain_constraints::add(&mut cdt, &constraints, side, terrain.cells.clamp(1, 128))?;
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

#[cfg(test)]
#[path = "terrain_fit_tests.rs"]
mod tests;
