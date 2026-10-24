//! CPU geometry: spline curves, spline surface extrusion, terrain meshes, placement.
//!
//! World frame: x east, y north, z up, metres. Headings in degrees, 0 = +y, clockwise.

use glam::{DVec2, DVec3, Mat4, Quat, Vec2, Vec3};
use omsi_map::{tile_size, MapSpline, Terrain};
use omsi_scenery::Spline;

mod hole_rims;
mod ground_sound;
pub use ground_sound::{GroundSound, LineKind, SoundLine, SoundSpot};
mod terrain_walls;
pub use terrain_walls::{terrain_hole_walls, terrain_hole_walls_with_roads};
mod terrain_fit;
mod terrain_fit_surface;
mod terrain_constraints;
pub use terrain_fit::{
    profile_in_cutting,
    TerrainFitError, SplineTerrainProfile, build_fitted_terrain_mesh, build_fitted_terrain_mesh_with_neighbors,
    try_build_fitted_terrain_mesh_with_roads,
    build_fitted_terrain_mesh_with_roads,
};

/// A renderable triangle mesh with one texture per material slot.
#[derive(Debug, Clone, Default)]
pub struct MeshData {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub uvs: Vec<Vec2>,
    /// (first index, index count, material slot)
    pub ranges: Vec<(u32, u32, u32)>,
    pub indices: Vec<u32>,
    /// A mesh from a content file (o3d / .x): drawn like Direct3D draws it, only from the
    /// side its triangles face (clockwise in the file's frame). Generated geometry
    /// (terrain, splines, tree cards) leaves this off and is seen from both sides.
    pub one_sided: bool,
}

impl MeshData {
    /// Join static meshes with the same ordered material slots and winding. Each range
    /// keeps its place in the material order, but draws all segments in one call.
    /// Positions must already be expressed in the same coordinate frame.
    pub fn merge_static(meshes: &[&MeshData]) -> MeshData {
        let Some(first) = meshes.first() else { return MeshData::default() };
        let mut out = MeshData { one_sided: first.one_sided, ..Default::default() };
        let mut ranges = vec![Vec::new(); first.ranges.len()];
        for mesh in meshes {
            assert_eq!(mesh.one_sided, first.one_sided);
            assert_eq!(mesh.ranges.len(), first.ranges.len());
            let base = out.positions.len() as u32;
            out.positions.extend_from_slice(&mesh.positions);
            out.normals.extend_from_slice(&mesh.normals);
            out.uvs.extend_from_slice(&mesh.uvs);
            for (i, &(start, count, slot)) in mesh.ranges.iter().enumerate() {
                assert_eq!(slot, first.ranges[i].2);
                ranges[i].extend(mesh.indices[start as usize..(start + count) as usize].iter().map(|v| base + v));
            }
        }
        for (indices, &(_, _, slot)) in ranges.into_iter().zip(&first.ranges) {
            let start = out.indices.len() as u32;
            let count = indices.len() as u32;
            out.indices.extend(indices);
            // Adjacent profiles using the same slot have identical draw parameters.
            match out.ranges.last_mut() {
                Some((_, n, s)) if *s == slot => *n += count,
                _ => out.ranges.push((start, count, slot)),
            }
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Bytes the mesh holds on the heap.
    pub fn heap_bytes(&self) -> usize {
        self.positions.capacity() * 12 + self.normals.capacity() * 12 + self.uvs.capacity() * 8 + self.indices.capacity() * 4 + self.ranges.capacity() * 12
    }
}

/// Analytic spline curve (one map segment).
#[derive(Debug, Clone, Copy)]
pub struct SplineCurve {
    pub start: DVec3,
    pub heading_deg: f64,
    pub length: f64,
    pub radius: f64,
    pub grad_start: f64,
    pub grad_end: f64,
    /// `[spline_h]`: the height gained over the length; the height is then a cubic between
    /// the two gradients instead of the gradient parabola of a plain `[spline]`.
    pub delta_h: Option<f64>,
    /// Cant in percent: the rise across per metre.
    pub cant_start: f64,
    pub cant_end: f64,
    /// The map's skew at the start and the end: a cross-section at lateral `x` is shifted
    /// along the spline by `x * skew`, the skew running linearly from start to end
    /// (Omsi.exe sub_5aaef4). Two splines meeting at a corner mitre there (a fence turning
    /// 90 degrees ends at skew 1 and the next starts at -1).
    pub skew_start: f64,
    pub skew_end: f64,
    /// Where the textures start along the spline (the map's [`MapSpline::tex_offset`]).
    pub tex_offset: f64,
    /// The spline's IDCode: the seed of its `[patchwork_chain]` order.
    pub seed: u32,
    /// How far out from the centre line the cant lifts (`[halfcantwidth]` of the .sli;
    /// beyond it the profile keeps the height it has there). See [`half_cant_width`].
    pub half_cant_width: f64,
}

/// The half cant width OMSI uses for a spline type: the .sli's `[halfcantwidth]`, else as
/// far as its `[heightprofile]`s reach either side (Omsi.exe 0x5adb3c: the width starts at
/// -1 and becomes the larger of -min(0, x0) and max(0, x1) over the height profiles, 0x5ac727
/// / 0x5ac78d) - not the drawn profile. A type without height profiles (the grass and
/// pavement pieces of a crossing kit) has none at all, and a cant the map gives it does
/// nothing: taken over the drawn width, Westcountry's crossings' `-70` tipped their pieces
/// metres into the ground and the sky, pointed spikes at every corner.
pub fn half_cant_width(def: &Spline) -> f64 {
    if let Some(w) = def.half_cant_width.filter(|w| w.is_finite() && *w >= 0.0) {
        return w as f64;
    }
    let (lo, hi) = def.height_profiles.iter().fold((0.0f64, 0.0f64), |(lo, hi), h| (lo.min(h.x0 as f64), hi.max(h.x1 as f64)));
    (-lo).max(hi).max(-1.0)
}

/// The half cant width of a spline without a .sli object (OMSI: 10 m).
pub const DEFAULT_HALF_CANT_WIDTH: f64 = 10.0;

static HALF_CANT_WIDTHS: std::sync::OnceLock<std::sync::RwLock<std::collections::HashMap<String, f64>>> = std::sync::OnceLock::new();

/// Remember a spline type's half cant width by its file name (as a tile names it), for
/// what is placed on its splines without the type at hand (objects in rows).
pub fn register_half_cant_width(file: &str, def: &Spline) {
    let key = file.trim().to_ascii_lowercase().replace('\\', "/");
    HALF_CANT_WIDTHS.get_or_init(Default::default).write().unwrap_or_else(|e| e.into_inner()).insert(key, half_cant_width(def));
}

/// The half cant width of the spline type `file` names, as registered (else the default).
pub fn half_cant_width_of(file: &str) -> f64 {
    let key = file.trim().to_ascii_lowercase().replace('\\', "/");
    HALF_CANT_WIDTHS.get().and_then(|m| m.read().ok().and_then(|m| m.get(&key).copied())).unwrap_or(DEFAULT_HALF_CANT_WIDTH)
}

impl SplineCurve {
    pub fn from_map(s: &MapSpline, tile_origin: DVec2) -> SplineCurve {
        SplineCurve {
            start: DVec3::new(s.pos[0] + tile_origin.x, s.pos[1] + tile_origin.y, s.pos[2]),
            heading_deg: s.heading,
            length: s.length,
            radius: s.radius,
            grad_start: s.grad_start,
            grad_end: s.grad_end,
            delta_h: s.delta_h,
            cant_start: s.cant_start,
            cant_end: s.cant_end,
            skew_start: s.skew_start,
            skew_end: s.skew_end,
            tex_offset: s.tex_offset,
            seed: s.id as u32,
            half_cant_width: DEFAULT_HALF_CANT_WIDTH,
        }
    }

    /// The same curve with its spline type's half cant width.
    pub fn with_sli(self, def: &Spline) -> SplineCurve {
        SplineCurve { half_cant_width: half_cant_width(def), ..self }
    }

    /// Direction vector (unit, horizontal) for a heading.
    #[inline]
    pub fn dir(heading_deg: f64) -> DVec2 {
        let h = heading_deg.to_radians();
        DVec2::new(h.sin(), h.cos())
    }

    /// Heading at distance `s`.
    pub fn heading_at(&self, s: f64) -> f64 {
        if self.radius == 0.0 {
            self.heading_deg
        } else {
            let turn = self.radius.signum();
            self.heading_deg + turn * (s / self.radius.abs()).to_degrees()
        }
    }

    /// Height at distance `s`: a plain spline's gradient (percent) changes linearly along
    /// it; a `[spline_h]` runs as the cubic that leaves with the start gradient, arrives with
    /// the end gradient and climbs exactly `delta_h`.
    pub fn height_at(&self, s: f64) -> f64 {
        let l = self.length.max(1e-6);
        let t = (s / l).clamp(0.0, 1.0);
        match self.delta_h {
            Some(dh) => {
                let (m0, m1) = (self.grad_start / 100.0 * l, self.grad_end / 100.0 * l);
                let (t2, t3) = (t * t, t * t * t);
                self.start.z + (t3 - 2.0 * t2 + t) * m0 + (3.0 * t2 - 2.0 * t3) * dh + (t3 - t2) * m1
            }
            None => {
                let g = self.grad_start + (self.grad_end - self.grad_start) * t * 0.5;
                self.start.z + s * g / 100.0
            }
        }
    }

    /// Rise per metre at distance `s`.
    pub fn slope_at(&self, s: f64) -> f64 {
        let l = self.length.max(1e-6);
        let t = (s / l).clamp(0.0, 1.0);
        match self.delta_h {
            Some(dh) => {
                let (m0, m1) = (self.grad_start / 100.0 * l, self.grad_end / 100.0 * l);
                ((3.0 * t * t - 4.0 * t + 1.0) * m0 + (6.0 * t - 6.0 * t * t) * dh + (3.0 * t * t - 2.0 * t) * m1) / l
            }
            None => (self.grad_start + (self.grad_end - self.grad_start) * t) / 100.0,
        }
    }

    pub fn cant_at(&self, s: f64) -> f64 {
        let l = self.length.max(1e-6);
        let t = (s / l).clamp(0.0, 1.0);
        self.cant_start + (self.cant_end - self.cant_start) * t
    }

    /// Centre-line point at distance `s`.
    pub fn point_at(&self, s: f64) -> DVec3 {
        let xy = if self.radius == 0.0 {
            self.start.truncate() + Self::dir(self.heading_deg) * s
        } else {
            let r = self.radius.abs();
            let turn = self.radius.signum();
            let d = Self::dir(self.heading_deg);
            let right = DVec2::new(d.y, -d.x);
            let centre = self.start.truncate() + right * r * turn;
            let ang = -turn * s / r; // clockwise = negative angle in an x-east/y-north frame
            let p = self.start.truncate() - centre;
            let (sa, ca) = ang.sin_cos();
            centre + DVec2::new(p.x * ca - p.y * sa, p.x * sa + p.y * ca)
        };
        DVec3::new(xy.x, xy.y, self.height_at(s))
    }

    /// Point at distance `s` and lateral offset `x` (right positive) and height `z`.
    pub fn offset_point(&self, s: f64, x: f64, z: f64) -> DVec3 {
        let c = self.point_at(s);
        let d = Self::dir(self.heading_at(s));
        let right = DVec2::new(d.y, -d.x);
        // (the original: the cant is a percentage and lifts only within the half
        // cant width; read as degrees it banked every curve 1.75 times too steeply)
        let hcw = self.half_cant_width.max(0.0);
        let dz = -x.clamp(-hcw, hcw) * self.cant_at(s) / 100.0;
        DVec3::new(c.x + right.x * x, c.y + right.y * x, c.z + z + dz)
    }

    pub fn end_point(&self) -> DVec3 {
        self.point_at(self.length)
    }
}

/// How many cross-sections a spline is drawn with, as Omsi.exe's TSplineSegment.Generate
/// (sub_5b1e14) and the mesh builder (sub_5aee00) count them: one every half degree of
/// turn, one every half degree the gradient changes, at least one per 10 m - and never
/// more than the spline type's buffers hold (2580 vertices, 3870 indices), which is what
/// keeps a road of many profiles to a few dozen.
pub fn spline_station_count(def: &Spline, curve: &SplineCurve) -> usize {
    const HALF_DEGREE: f64 = 0.008726647;
    let l = curve.length.max(0.0);
    // (the turn: Generate's curvature times length)
    let turn = if curve.radius != 0.0 { (l / curve.radius).abs() } else { 0.0 };
    let horizontal = (turn / HALF_DEGREE) as usize + 1;
    // the height polynomial g0 s + a s^2 + b s^3 (sub_5ab108)
    let (g0, g1) = (curve.grad_start / 100.0, curve.grad_end / 100.0);
    let (a, b) = match curve.delta_h {
        Some(dh) if l > 1e-6 => ((3.0 * (dh - g0 * l) - (g1 - g0) * l) / (l * l), ((g1 - g0) * l - 2.0 * (dh - g0 * l)) / (l * l * l)),
        _ if l > 1e-6 => ((g1 - g0) / (2.0 * l), 0.0),
        _ => (0.0, 0.0),
    };
    let vertical = if a != 0.0 || b != 0.0 {
        let bend = (2.0 * a + 3.0 * b * l).abs() + (2.0 * a).abs();
        (l * bend / HALF_DEGREE).min(1e6) as usize + 1
    } else {
        1
    };
    let n = horizontal.max(vertical.max((l / 10.0) as usize));
    n.min(station_cap(def)).max(1)
}

/// The most cross-sections a spline type's mesh may have (Omsi.exe 0x5adbf0: 2580 vertices
/// and 3870 indices over all its profiles).
fn station_cap(def: &Spline) -> usize {
    let points: usize = def.profiles.iter().map(|p| p.points.len()).sum();
    let segments: usize = def.profiles.iter().map(|p| p.points.len().saturating_sub(1)).sum();
    if points == 0 || segments == 0 {
        return usize::MAX;
    }
    (2580 / points).saturating_sub(1).min(3870 / (6 * segments))
}

/// The `[patchwork_chain]` of a spline: which part of the texture each stretch shows.
struct Patchwork {
    /// The texture slot it belongs to.
    texture: usize,
    /// Parts the texture is cut into.
    parts: usize,
    /// Cross-sections per stretch.
    per: usize,
    /// Per stretch the part, from 1 (negative: laid backwards).
    order: Vec<i32>,
}

/// OMSI's patchwork order (Omsi.exe sub_5aee00): the spline is cut into
/// `trunc(length / segment_length) + 1` stretches; each takes a part whose first letter is the
/// last letter of the part before (or, if invertible, a part laid backwards), drawn by weight
/// with the map's own random numbers: seeded by the spline's IDCode plus 100 per stretch and
/// stirred `(id + stretch) mod 7 + 1` times by `x = (x * 31415926 + 1) mod 10^8`. The last
/// stretch has to end on the chain's first letter, so the next spline joins on.
/// `stations` is made a multiple of the stretches.
fn patchwork(def: &Spline, curve: &SplineCurve, stations: &mut usize) -> Option<Patchwork> {
    // (OMSI keeps one chain per spline type, the last one defined)
    let (texture, pc) = def.textures.iter().enumerate().rev().find_map(|(i, t)| t.patchwork.as_ref().filter(|p| p.valid()).map(|p| (i, p)))?;
    let chain = pc.chain.as_bytes();
    let n_parts = pc.weights.len();
    let weight = |k: usize| pc.weights.as_bytes()[k].wrapping_sub(b'0').min(9) as usize;
    let invertible = |k: usize| pc.invertable.as_bytes()[k] == b'1';
    let seg = pc.segment_length as f64;
    if !(seg > 0.0) {
        return None;
    }
    let stretches = ((curve.length / seg).max(0.0).min(1e6) as usize) + 1;
    let cap = station_cap(def);
    let mut count = (*stations).min(cap);
    let mut per = count / stretches + 1;
    count = stretches * per;
    if count > cap {
        count = cap.min(stretches).max(cap / stretches * stretches);
        per = count / stretches;
    }
    let per = per.max(1);
    *stations = stretches * per;
    let mut state = chain[0];
    let mut order = Vec::with_capacity(stretches);
    for k in 0..stretches {
        let mut cands: Vec<i32> = Vec::new();
        if k + 1 == stretches {
            for p in 0..n_parts {
                if chain[p] == state && chain[p + 1] == chain[0] {
                    cands.extend(std::iter::repeat(p as i32 + 1).take(weight(p)));
                }
            }
            for p in 0..n_parts {
                if invertible(p) && chain[p + 1] == state && chain[p] == chain[0] {
                    cands.extend(std::iter::repeat(-(p as i32 + 1)).take(weight(p)));
                }
            }
        }
        if cands.is_empty() {
            for q in -(n_parts as i32)..=n_parts as i32 {
                let p = q.unsigned_abs() as usize;
                let fits = match q.signum() {
                    -1 => invertible(p - 1) && chain[p] == state,
                    1 => chain[p - 1] == state,
                    _ => false,
                };
                if fits {
                    cands.extend(std::iter::repeat(q).take(weight(p - 1)));
                }
            }
        }
        let mut x: i64 = curve.seed as i64 + 100 * k as i64;
        for _ in 0..=((curve.seed as i64 + k as i64) % 7) {
            x = (x * 31_415_926 + 1) % 100_000_000;
        }
        let r = x as f64 / 1e8;
        let part = if cands.is_empty() {
            // (a chain that cannot go on: OMSI logs it; take any part)
            ((r * n_parts as f64) as i32 + 1).min(n_parts as i32)
        } else {
            cands[((cands.len() as f64 * r) as usize).min(cands.len() - 1)]
        };
        state = if part < 0 { chain[(-part - 1) as usize] } else { chain[part as usize] };
        order.push(part);
    }
    Some(Patchwork { texture, parts: n_parts, per, order })
}

/// The point of a spline mesh at distance `s` along it, `x` across (right positive, already
/// mirrored) and `z` up: the cross-section shifted along the spline by the skew (Omsi.exe
/// sub_5aaef4: `x * (skew_start (1 - s/L) + skew_end s/L)`), and that shifted distance.
pub fn skewed_point(curve: &SplineCurve, s: f64, x: f64, z: f64) -> (DVec3, f64) {
    let t = if curve.length > 0.0 { s / curve.length } else { 0.0 };
    let skew = curve.skew_start * (1.0 - t) + curve.skew_end * t;
    let s = if skew.is_finite() { s + x * skew.clamp(-32.0, 32.0) } else { s };
    (curve.offset_point(s, x, z), s)
}

/// Extrude a spline definition along a map curve into a mesh (one material slot per texture),
/// as Omsi.exe's sub_5aee00 does: [`spline_station_count`] cross-sections, each shifted by the
/// skew; `v = v_scale * (s + tex_offset)` so a chain's textures run on across its joints
/// (`[scaleTexByLength]`: from 0 to `v_scale` over the whole spline; a `[patchwork_chain]`:
/// part by part); a mirrored spline is flipped across and its v turned round.
/// Positions are relative to `origin` (f64 subtraction keeps precision on large maps).
pub fn build_spline_mesh(def: &Spline, curve: &SplineCurve, mirror: bool, origin: DVec3) -> MeshData {
    let mut mesh = MeshData::default();
    if curve.length <= 0.0 {
        return mesh;
    }
    let mut n = spline_station_count(def, curve);
    let patch = patchwork(def, curve, &mut n);
    let mirror_sign = if mirror { -1.0 } else { 1.0 };
    let curve = &curve.with_sli(def);
    let station = |i: usize| curve.length * i as f64 / n as f64;
    for profile in &def.profiles {
        if profile.points.len() < 2 {
            continue;
        }
        let first_index = mesh.indices.len() as u32;
        let base = mesh.positions.len() as u32;
        let pn = profile.points.len() as u32;
        let tex = def.textures.get(profile.texture);
        let quad = |mesh: &mut MeshData, a: u32, b: u32, c: u32, d: u32| {
            // counter-clockwise seen from above, so the computed normals point up
            // (profile points run left to right, stations forward along the curve)
            if mirror {
                mesh.indices.extend_from_slice(&[a, c, b, b, c, d]);
            } else {
                mesh.indices.extend_from_slice(&[a, b, c, b, d, c]);
            }
        };
        match patch.as_ref().filter(|p| p.texture == profile.texture) {
            Some(pw) => {
                // every stretch between two cross-sections has vertices of its own: the
                // part changes where one stretch of the chain ends and the next begins
                for i in 0..n {
                    let block = i / pw.per;
                    let part = pw.order.get(block).copied().unwrap_or(1);
                    let p0 = (part.unsigned_abs() as usize).saturating_sub(1) as f64 / pw.parts as f64;
                    let q = base + (i as u32) * 2 * pn;
                    for e in 0..2 {
                        let r = (i + e - block * pw.per) as f64;
                        let r = if part < 0 { pw.per as f64 - r } else { r };
                        let v = r / (pw.per * pw.parts) as f64 + p0;
                        for pt in &profile.points {
                            let (p, _) = skewed_point(curve, station(i + e), pt.x as f64 * mirror_sign, pt.z as f64);
                            mesh.positions.push((p - origin).as_vec3());
                            mesh.normals.push(Vec3::Z);
                            mesh.uvs.push(Vec2::new(pt.u, (v * mirror_sign) as f32));
                        }
                    }
                    for j in 0..pn - 1 {
                        quad(&mut mesh, q + j, q + j + 1, q + pn + j, q + pn + j + 1);
                    }
                }
            }
            None => {
                let by_length = tex.is_some_and(|t| t.scale_by_length);
                // (v can run to thousands on a long chain: drop whole repeats, which change
                // nothing, where the profile's points share one scale)
                let shared = profile.points.iter().all(|p| p.v_scale == profile.points[0].v_scale);
                let whole = if shared && !by_length { (profile.points[0].v_scale as f64 * curve.tex_offset * mirror_sign).floor() } else { 0.0 };
                for i in 0..=n {
                    for pt in &profile.points {
                        let (p, s) = skewed_point(curve, station(i), pt.x as f64 * mirror_sign, pt.z as f64);
                        mesh.positions.push((p - origin).as_vec3());
                        mesh.normals.push(Vec3::Z);
                        let v = if by_length {
                            i as f64 * pt.v_scale as f64 / n as f64 * mirror_sign
                        } else {
                            pt.v_scale as f64 * (s + curve.tex_offset) * mirror_sign - whole
                        };
                        mesh.uvs.push(Vec2::new(pt.u, v as f32));
                    }
                }
                for i in 0..n as u32 {
                    for j in 0..pn - 1 {
                        let a = base + i * pn + j;
                        quad(&mut mesh, a, a + 1, a + pn, a + pn + 1);
                    }
                }
            }
        }
        let count = mesh.indices.len() as u32 - first_index;
        mesh.ranges.push((first_index, count, profile.texture as u32));
    }
    compute_normals(&mut mesh);
    // Drawn from the side a profile faces only, as OMSI draws splines: the makers orient
    // every face - 7560 of 7630 level faces in the stock and two add-on maps' splines face
    // up, the other 70 are the undersides of bridges, roofs and tunnel ceilings - and a
    // fence or guard rail is two faces a couple of centimetres apart, one per side, which
    // drawn from both sides fought in the depth buffer and flickered as the camera moved.
    // (The winding is turned to the content meshes' clockwise front; the normals, made
    // above, stay as they are.)
    for t in mesh.indices.chunks_exact_mut(3) {
        t.swap(1, 2);
    }
    mesh.one_sided = true;
    mesh
}

/// The profiles of the hole a spline laid with `[spline_terrain_align]` cuts into the ground,
/// as points (x across, height, how far that end of the hole stands off the spline's end):
/// the `.sli`'s own `[terrainholeprofile]`s, or else those Omsi.exe makes from the drawn
/// profiles when it loads the spline (0x5ab908, from 0x5adc52): the profiles are strung
/// together where one begins within a centimetre of where the last ended, and each string
/// gives a trough from its left end to its right end, 3 cm in from both and 3 mm below them,
/// whose bottom lies 10 cm under the lowest point of the string and reaches half a metre
/// past both ends of the spline. (The bottom's corners are where the lines from the edges
/// through the points below them come down to that depth; with nothing below an edge, a
/// quarter of the way across. The left edge is always the first profile's first point:
/// Omsi.exe compares every other profile's start with it but then takes the first one's
/// again.)
pub fn terrain_hole_profiles(def: &Spline) -> Vec<Vec<[f32; 3]>> {
    if !def.terrain_hole_profiles.is_empty() {
        return def.terrain_hole_profiles.clone();
    }
    const JOIN: f32 = 0.01;
    const INSET: f32 = 0.03;
    let profiles: Vec<Vec<(f32, f32)>> = def.profiles.iter().map(|p| p.points.iter().map(|q| (q.x, q.z)).collect()).collect();
    let mut used = vec![false; profiles.len()];
    let mut out = Vec::new();
    while used.iter().any(|u| !u) {
        let mut chain: Vec<usize> = Vec::new();
        loop {
            let next = (0..profiles.len()).find(|&j| {
                !used[j]
                    && match chain.last() {
                        None => true,
                        Some(&l) => match (profiles[l].last(), profiles[j].first()) {
                            (Some(a), Some(b)) => (a.0 - b.0).abs() < JOIN && (a.1 - b.1).abs() < JOIN,
                            _ => false,
                        },
                    }
            });
            match next {
                Some(j) => {
                    used[j] = true;
                    chain.push(j);
                }
                None => break,
            }
        }
        let Some((&(xl, zl), _)) = profiles[chain[0]].first().zip(profiles[chain[0]].last()) else {
            continue;
        };
        let (mut xr, mut zr) = *profiles[chain[0]].last().unwrap();
        for &j in &chain[1..] {
            if let Some(&(x, z)) = profiles[j].last() {
                if x > xr {
                    (xr, zr) = (x, z);
                }
            }
        }
        // An automatic trough needs positive width after both insets. Thin overhead
        // wires otherwise produce an inverted cut with terrain walls up to the wire.
        // Explicit terrainholeprofiles returned above keep their authored dimensions.
        if xl + INSET >= xr - INSET {
            continue;
        }
        let points = || chain.iter().flat_map(|&j| profiles[j].iter().copied());
        let low = points().fold(zl.min(zr), |m, (_, z)| m.min(z));
        let bottom_left = if low < zl {
            points().filter(|p| zl > p.1).fold(xr, |m, (x, z)| m.min((x - xl) / (zl - z) * (zl - low) + xl))
        } else {
            (xr - xl) / 4.0 + xl
        };
        let bottom_right = if low < zr {
            points().filter(|p| zr > p.1).fold(xl, |m, (x, z)| m.max(xr - (xr - x) / (zr - z) * (zr - low)))
        } else {
            (xr - xl) * 3.0 / 4.0 + xl
        };
        let low = low - 0.1;
        out.push(vec![[xl + INSET, zl - 0.003, 0.0], [bottom_left, low, -0.5], [bottom_right, low, -0.5], [xr - INSET, zr - 0.003, 0.0]]);
    }
    out
}

/// The outlines (world x, y) a spline laid with `[spline_terrain_align]` cuts out of the
/// ground, one per [`terrain_hole_profiles`] profile, as Omsi.exe's TSplineSegment.Generate
/// makes them (0x5b1178, from 0x5b13b1): along the right edge (the profile's last point)
/// at the inner cross-sections, across the far end (the profile backwards), back along the
/// left edge (its first point) and across the near end. An end lies where each point's
/// third value puts it (the hole reaching past the spline's end) unless the map's
/// `[spline_terrain_align_2]` says otherwise: `mode` 2 and 4 keep the far end at the end,
/// 3 and 4 the near end at the start. A mirrored spline takes the profile backwards and
/// turned across.
pub fn spline_hole_outlines(def: &Spline, curve: &SplineCurve, mirror: bool, mode: u8) -> Vec<Vec<DVec2>> {
    spline_hole_rims(def, curve, mirror, mode).into_iter()
        .map(|ring| ring.into_iter().map(DVec3::truncate).collect()).collect()
}

/// The same boundary as [`spline_hole_outlines`], retaining each profile point's height
/// (including gradient, cant and skew) for the ground's connection to the spline.
pub fn spline_hole_rims(def: &Spline, curve: &SplineCurve, mirror: bool, mode: u8) -> Vec<Vec<DVec3>> {
    if mode == 0 || curve.length <= 0.0 {
        return Vec::new();
    }
    let mut n = spline_station_count(def, curve);
    let _ = patchwork(def, curve, &mut n);
    let n = n.max(1);
    let l = curve.length;
    let curve = &curve.with_sli(def);
    let at = |q: &[f32; 3], s: f64| skewed_point(curve, s, q[0] as f64, q[1] as f64).0;
    terrain_hole_profiles(def)
        .into_iter()
        .filter(|p| !p.is_empty())
        .map(|p| {
            let p: Vec<[f32; 3]> = if mirror { p.iter().rev().map(|q| [-q[0], q[1], q[2]]).collect() } else { p };
            let k = p.len();
            let mut ring = Vec::with_capacity(2 * (k + n - 1));
            for i in 1..n {
                ring.push(at(&p[k - 1], i as f64 * l / n as f64));
            }
            for q in p.iter().rev() {
                ring.push(at(q, if mode & 1 == 0 { l } else { l - q[2] as f64 }));
            }
            for i in (1..n).rev() {
                ring.push(at(&p[0], i as f64 * l / n as f64));
            }
            for q in &p {
                ring.push(at(q, if mode > 2 { 0.0 } else { q[2] as f64 }));
            }
            ring
        })
        .collect()
}

/// The solid ground backing of an aligned spline. Unlike a projected hole, this
/// retains the trough's floor and its end extensions, even where no road is drawn.
/// `rims` are the spline's `spline_hole_rims`, one per profile, in world space.
pub fn build_spline_terrain_profile(
    def: &Spline,
    curve: &SplineCurve,
    mirror: bool,
    mode: u8,
    rims: &[Vec<DVec3>],
    origin: DVec3,
) -> SplineTerrainProfile {
    let mut mesh = MeshData::default();
    if mode == 0 || curve.length <= 0.0 {
        return SplineTerrainProfile::default();
    }
    let mut n = spline_station_count(def, curve);
    let _ = patchwork(def, curve, &mut n);
    let n = n.max(1);
    let curve = curve.with_sli(def);
    for (profile, rim) in terrain_fit_surface::profiles(def).into_iter().zip(rims) {
        if profile.len() < 2
            || outline_crosses_itself(&rim.iter().map(|p| p.truncate()).collect::<Vec<_>>())
        {
            continue;
        }
        let p: Vec<_> = if mirror {
            profile.iter().rev().map(|p| [-p[0], p[1], p[2]]).collect()
        } else {
            profile
        };
        let first = mesh.positions.len() as u32;
        let k = p.len();
        for i in 0..=n {
            for q in &p {
                let s = if i == 0 {
                    if mode > 2 { 0.0 } else { q[2] as f64 }
                } else if i == n {
                    if mode & 1 == 0 {
                        curve.length
                    } else {
                        curve.length - q[2] as f64
                    }
                } else {
                    i as f64 * curve.length / n as f64
                };
                mesh.positions
                    .push((skewed_point(&curve, s, q[0] as f64, q[1] as f64).0 - origin).as_vec3());
            }
        }
        for i in 0..n {
            for j in 0..k - 1 {
                let a = first + (i * k + j) as u32;
                let b = a + 1;
                let c = a + k as u32;
                let d = c + 1;
                mesh.indices.extend([a, b, d, a, d, c]);
            }
        }
    }
    SplineTerrainProfile::new(mesh, &curve)
}

/// Does a closed outline cross itself? Omsi.exe cuts no hole along one that does (0x5748bc
/// and the terrain's "Terrain hole cutting" error): a tight curve folds the inner edge over.
pub fn outline_crosses_itself(ring: &[DVec2]) -> bool {
    let n = ring.len();
    if n < 4 {
        return n < 3;
    }
    let cross = |a: DVec2, b: DVec2, c: DVec2| (b - a).perp_dot(c - a);
    let hits = |p1: DVec2, p2: DVec2, q1: DVec2, q2: DVec2| {
        let (d1, d2) = (cross(q1, q2, p1), cross(q1, q2, p2));
        let (d3, d4) = (cross(p1, p2, q1), cross(p1, p2, q2));
        ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    };
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        for j in i + 2..n {
            if (j + 1) % n == i {
                continue;
            }
            if hits(a, b, ring[j], ring[(j + 1) % n]) {
                return true;
            }
        }
    }
    false
}

/// The outlines of a `[terrainhole]` cutter, seen from above (world x, y): its open rims,
/// the edges only one face uses, chained into closed rings. OMSI 2 cuts the ground along an
/// object's cutter as exactly as along a spline's outline, so a junction's ground ends at its
/// kerb, not a texel short of it. Rims that meet at a vertex are separated using their
/// incident faces. A closed cutter (no rim) gives no ring.
/// Positions are the mesh's after `transform`, relative to `origin`.
pub fn hole_mesh_outlines(mesh: &MeshData, transform: &Mat4, origin: DVec3) -> Vec<Vec<DVec2>> {
    hole_mesh_rims(mesh, transform, origin).into_iter()
        .map(|ring| ring.into_iter().map(DVec3::truncate).collect()).collect()
}

/// The open rims of an object cutter, retaining their transformed world heights.
/// Touching rims follow the mesh's face adjacency, not the angle of their projection:
/// a vertical profile can share a corner with a road without joining their boundaries.
/// Closed meshes still have no rim; no wall height is invented for them.
pub fn hole_mesh_rims(mesh: &MeshData, transform: &Mat4, origin: DVec3) -> Vec<Vec<DVec3>> {
    use std::collections::HashMap;
    // (a model repeats a vertex for every face and UV seam: the corners by place, to the mm)
    let key = |v: Vec3| ((v.x * 1000.0).round() as i64, (v.y * 1000.0).round() as i64, (v.z * 1000.0).round() as i64);
    let mut place: HashMap<(i64, i64, i64), DVec3> = HashMap::new();
    let mut edges: HashMap<((i64, i64, i64), (i64, i64, i64)), Vec<usize>> = HashMap::new();
    let mut faces = Vec::new();
    for t in mesh.indices.chunks_exact(3) {
        let k = [0, 1, 2].map(|i| {
            let v = mesh.positions[t[i] as usize];
            let kv = key(v);
            place.entry(kv).or_insert_with(|| origin + transform.transform_point3(v).as_dvec3());
            kv
        });
        if k[0] == k[1] || k[1] == k[2] || k[2] == k[0] {
            continue;
        }
        let face = faces.len();
        faces.push(k);
        for i in 0..3 {
            let (a, b) = (k[i], k[(i + 1) % 3]);
            edges.entry(if a < b { (a, b) } else { (b, a) }).or_default().push(face);
        }
    }
    let mut next: HashMap<(i64, i64, i64), Vec<(i64, i64, i64)>> = HashMap::new();
    for ((a, b), n) in &edges {
        if n.len() == 1 {
            next.entry(*a).or_default().push(*b);
            next.entry(*b).or_default().push(*a);
        }
    }
    if next.values().any(|v| v.len() != 2) {
        return hole_rims::trace_faces(&faces, &edges, &place);
    }
    let mut starts: Vec<_> = next.keys().copied().collect();
    starts.sort_unstable();
    let mut used = std::collections::HashSet::new();
    let mut rings = Vec::new();
    for s in starts {
        if !used.insert(s) {
            continue;
        }
        let (mut prev, mut cur) = (s, next[&s][0]);
        let mut ring = vec![place[&s]];
        while cur != s && used.insert(cur) {
            ring.push(place[&cur]);
            let n = &next[&cur];
            let step = if n[0] == prev { n[1] } else { n[0] };
            (prev, cur) = (cur, step);
        }
        if cur == s && ring.len() >= 3 {
            rings.push(ring);
        }
    }
    rings
}

/// The surface a spline's `[heightprofile]` segments describe, extruded along the curve like
/// the drawn profile (the same cross-sections, the same skew): what vehicles and wheels stand
/// on. OMSI keeps it apart from the graphics - a railway's third rail or a tunnel's walls are
/// drawn but not driven on, a fence or the Berlin Wall has no height profile at all, and a
/// street's lies exactly on its carriageway and pavements. Positions are relative to `origin`.
pub fn build_height_profile_mesh(def: &Spline, curve: &SplineCurve, mirror: bool, origin: DVec3) -> MeshData {
    let mut mesh = MeshData::default();
    if curve.length <= 0.0 || def.height_profiles.is_empty() {
        return mesh;
    }
    let n = spline_station_count(def, curve);
    let sign = if mirror { -1.0 } else { 1.0 };
    let curve = &curve.with_sli(def);
    // the ordinary profiles first (material 0), then the wall tops (material 1)
    let ridge = |hp: &&omsi_scenery::sli::HeightProfile| is_wall_top(hp.x0, hp.x1, hp.z0, hp.z1);
    let (flat, tops): (Vec<_>, Vec<_>) = def.height_profiles.iter().partition(|hp| !ridge(hp));
    let mut flat_end = 0u32;
    for (pass, list) in [flat, tops].into_iter().enumerate() {
        for hp in list {
            if (hp.x1 - hp.x0).abs() < 1e-3 {
                continue;
            }
            let base = mesh.positions.len() as u32;
            let (z0, z1) = match drawn_height(def, hp.x0.min(hp.x1), hp.x0.max(hp.x1)) {
                Some(d) if hp.z0.min(hp.z1) > d + PHANTOM_LIFT => (d, d),
                _ => (hp.z0, hp.z1),
            };
            for i in 0..=n {
                let s = curve.length * i as f64 / n as f64;
                for (x, z) in [(hp.x0, z0), (hp.x1, z1)] {
                    let (p, _) = skewed_point(curve, s, x as f64 * sign, z as f64);
                    mesh.positions.push((p - origin).as_vec3());
                    mesh.normals.push(Vec3::Z);
                    mesh.uvs.push(Vec2::ZERO);
                }
            }
            for i in 0..n as u32 {
                let (a, b) = (base + i * 2, base + i * 2 + 1);
                let (c, d) = (a + 2, b + 2);
                mesh.indices.extend_from_slice(&[a, b, c, b, d, c]);
            }
        }
        if pass == 0 {
            flat_end = mesh.indices.len() as u32;
        }
    }
    mesh.ranges.push((0, flat_end, 0));
    if mesh.indices.len() as u32 > flat_end {
        mesh.ranges.push((flat_end, mesh.indices.len() as u32 - flat_end, 1));
    }
    mesh
}

/// How far a height profile may lie over everything the spline draws across it before it
/// counts as a slip of its maker and is brought down to the drawn surface.
const PHANTOM_LIFT: f32 = 0.25;

/// The highest point the spline's drawn profiles reach between `xa` and `xb` (none: nothing
/// is drawn there - an invisible footway, which keeps its height profile as it is).
/// Westcountry's yellow surface marking draws its paint 10 cm up and says 50 cm in its
/// `[heightprofile]`: every wheel met it as a 40 cm wall across the carriageway.
fn drawn_height(def: &Spline, xa: f32, xb: f32) -> Option<f32> {
    let mut best: Option<f32> = None;
    for p in &def.profiles {
        for w in p.points.windows(2) {
            let (a, b) = (&w[0], &w[1]);
            let (lo, hi) = (a.x.min(b.x), a.x.max(b.x));
            if hi < xa - 0.05 || lo > xb + 0.05 {
                continue;
            }
            // (a vertical face, a kerb's edge, counts with its top)
            let z = a.z.max(b.z);
            best = Some(best.map_or(z, |m: f32| m.max(z)));
        }
    }
    best
}

/// A height profile that is the top of a wall: a strip narrower than a wheel could stand on
/// (under 0.6 m), 0.3 m and more over the spline. UK maps give their stone and brick walls
/// one (Westcountry's `cb_wall03`: 30 cm wide, 1.6 m up); taken as road, a wheel rolled
/// onto the wall where its top met the road and rode along it as the road fell away.
pub fn is_wall_top(x0: f32, x1: f32, z0: f32, z1: f32) -> bool {
    let w = (x1 - x0).abs();
    w >= 1e-3 && w < 0.6 && z0.min(z1) >= 0.3
}

/// Recompute smooth vertex normals from triangles.
pub fn compute_normals(mesh: &mut MeshData) {
    let mut acc = vec![Vec3::ZERO; mesh.positions.len()];
    for tri in mesh.indices.chunks_exact(3) {
        let (a, b, c) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        let n = (mesh.positions[b] - mesh.positions[a]).cross(mesh.positions[c] - mesh.positions[a]);
        acc[a] += n;
        acc[b] += n;
        acc[c] += n;
    }
    mesh.normals = acc.into_iter().map(|n| if n.length_squared() > 0.0 { n.normalize() } else { Vec3::Z }).collect();
}

/// Smooth vertex normals from the faces as D3DXComputeNormals makes them for a mesh read
/// from a file: (v1 - v0) x (v2 - v0) in the file's Direct3D frame, which the y/z swap of
/// `mesh_from_o3d` mirrors, hence (v2 - v0) x (v1 - v0) here. Omsi.exe rebuilds the normals
/// of every mesh of an object with `[crossing_heightdeformation]` this way. Each face's unit
/// normal counts with the face's angle at the vertex, D3DX's default weighting (neither
/// D3DXTANGENT_WEIGHT_BY_AREA nor _EQUAL), not with its area.
pub fn compute_normals_d3d(mesh: &mut MeshData) {
    let mut acc = vec![Vec3::ZERO; mesh.positions.len()];
    for tri in mesh.indices.chunks_exact(3) {
        let i = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
        let p = i.map(|k| mesh.positions[k]);
        let n = (p[2] - p[0]).cross(p[1] - p[0]).normalize_or_zero();
        for k in 0..3 {
            let e1 = (p[(k + 1) % 3] - p[k]).normalize_or_zero();
            let e2 = (p[(k + 2) % 3] - p[k]).normalize_or_zero();
            acc[i[k]] += n * e1.dot(e2).clamp(-1.0, 1.0).acos();
        }
    }
    for (n, a) in mesh.normals.iter_mut().zip(acc) {
        if a.length_squared() > 0.0 {
            *n = a.normalize();
        }
    }
}

/// Terrain mesh for one tile in tile-local coordinates (0..300). UVs are tile space (0..1);
/// the material scales them for the ground texture.
pub fn build_terrain_mesh(t: &Terrain) -> MeshData {
    let n = t.samples();
    let cell = tile_size() as f32 / t.cells as f32;
    let mut mesh = MeshData::default();
    mesh.positions.reserve(n * n);
    for j in 0..n {
        for i in 0..n {
            let x = i as f32 * cell;
            let y = j as f32 * cell;
            mesh.positions.push(Vec3::new(x, y, t.height_at(i, j)));
            mesh.uvs.push(Vec2::new(x / tile_size() as f32, y / tile_size() as f32));
            mesh.normals.push(Vec3::Z);
        }
    }
    for j in 0..n - 1 {
        for i in 0..n - 1 {
            let a = (j * n + i) as u32;
            let b = a + 1;
            let c = a + n as u32;
            let d = c + 1;
            // split along the diagonal from the cell's (i, j) corner to its (i+1, j+1)
            // corner, as OMSI does (its terrain vertex cache, `.map.terrain_0.rdy`,
            // lists every cell as (i,j)-(i,j+1)-(i+1,j+1) and (i,j)-(i+1,j+1)-(i+1,j)).
            // The maps' ground is shaped against those triangles: split the other way,
            // wherever the ground steps down beside a kerb half a cell of it stood up
            // over the road as a V-shaped wedge.
            mesh.indices.extend_from_slice(&[a, d, c, a, b, d]);
        }
    }
    mesh.ranges.push((0, mesh.indices.len() as u32, 0));
    compute_normals(&mut mesh);
    mesh
}

/// Height of the terrain *as drawn* at tile-local metres: the two triangles of each cell
/// that [`build_terrain_mesh`] makes (diagonal from the cell's (i, j) corner to its
/// (i+1, j+1) corner, as OMSI splits it), not the bilinear patch `Terrain::sample`
/// interpolates. On a curved slope the
/// two differ by centimetres, and a wheel standing on the bilinear one sank into the grass
/// or floated over it.
pub fn terrain_height(t: &Terrain, x: f32, y: f32) -> f32 {
    t.sample(x, y)
}

/// Rotation of a map object: heading (Z), pitch (X), bank (Y). The position is kept apart
/// as the instance origin (f64). Pitched first, then banked, then turned, as Omsi.exe puts
/// an object down (sub_79cb18: RotationX(pitch) · RotationZ(bank) · RotationY(heading)) - a
/// rock both pitched and banked a good deal leant otherwise with the bank taken first.
pub fn object_rotation(rot_deg: [f64; 3]) -> Mat4 {
    let heading = (-rot_deg[0]).to_radians() as f32; // clockwise heading → counter-clockwise rotation about +z
    let pitch = rot_deg[1].to_radians() as f32;
    let bank = rot_deg[2].to_radians() as f32;
    let q = Quat::from_rotation_z(heading) * Quat::from_rotation_y(bank) * Quat::from_rotation_x(pitch);
    Mat4::from_quat(q)
}

/// The same angles in the order D3DX's `QuaternionRotationYawPitchRoll` applies them (the
/// bank first, then the pitch, then the heading), which is how Omsi.exe turns an attached
/// object by its own angles (0x79d547).
pub fn object_rotation_ypr(rot_deg: [f64; 3]) -> Mat4 {
    let heading = (-rot_deg[0]).to_radians() as f32;
    let pitch = rot_deg[1].to_radians() as f32;
    let bank = rot_deg[2].to_radians() as f32;
    Mat4::from_quat(Quat::from_rotation_z(heading) * Quat::from_rotation_x(pitch) * Quat::from_rotation_y(bank))
}

/// A map file's heading, pitch and bank (`[object]`, `[attachObj]`, `[splineAttachement]`)
/// in the terms [`object_rotation`] takes. The file keeps the angles of Direct3D's
/// left-handed frame, where a positive pitch lowers the nose and a positive bank raises the
/// right side; going over to the right-handed world turns every sense of rotation round.
/// `object_rotation` already turns the heading, and pitch and bank have to turn as well: taken
/// as they stand, TH_Wald's rocks tipped the other way and stood as boxes with a grass lid
/// beside the road instead of a rock face. (Pitch and bank that openOMSI works out itself -
/// a parked car on a slope, an object tilted with its spline - are world angles already.)
pub fn map_rotation(rot_deg: [f64; 3]) -> [f64; 3] {
    [rot_deg[0], -rot_deg[1], -rot_deg[2]]
}

/// Convert an `.o3d`/`.x` mesh to [`MeshData`] (one range per material).
pub fn mesh_from_o3d(m: &omsi_o3d::Mesh) -> MeshData {
    mesh_from_o3d_turning(m, true)
}

pub fn mesh_from_o3d_turning(m: &omsi_o3d::Mesh, may_turn: bool) -> MeshData {
    // Vertices are stored in the parent (object/vehicle) frame already; the matrix in the
    // file is the mesh's pivot frame used by `origin_from_mesh` animations, not a transform
    // to apply. Mesh files use Direct3D's frame (x right, y up, z forward); the world uses
    // x right, y forward, z up, so y and z are swapped.
    let swap = |v: Vec3| Vec3::new(v.x, v.z, v.y);
    let mut out = MeshData { one_sided: true, ..Default::default() };
    out.positions = m.vertices.iter().map(|v| swap(v.position)).collect();
    out.normals = m.vertices.iter().map(|v| swap(v.normal).normalize_or_zero()).collect();
    out.uvs = m.vertices.iter().map(|v| v.uv).collect();
    let turn = may_turn && turns_round(m);
    // group triangles by material, preserving material index as slot
    let mat_count = m.materials.len().max(1);
    let mut buckets: Vec<Vec<u32>> = vec![Vec::new(); mat_count];
    for t in &m.triangles {
        let slot = (t.material as usize).min(mat_count - 1);
        if turn {
            buckets[slot].extend_from_slice(&[t.indices[0], t.indices[2], t.indices[1]]);
        } else {
            buckets[slot].extend_from_slice(&t.indices);
        }
    }
    for (slot, idx) in buckets.into_iter().enumerate() {
        if idx.is_empty() {
            continue;
        }
        let first = out.indices.len() as u32;
        out.indices.extend_from_slice(&idx);
        // Omsi.exe draws per material, so a material-less o3d (bone dummies, collision
        // meshes) is invisible; its triangles stay for whatever reads them.
        if !m.materials.is_empty() {
            out.ranges.push((first, idx.len() as u32, slot as u32));
        }
    }
    out
}


pub fn turns_round(m: &omsi_o3d::Mesh) -> bool {
    // A mesh whose faces all turn their backs on their own normals was mirrored in the
    // modeller (the winding flips, the normals are recomputed): drawn one-sided as it stands,
    // the front shows nothing - the LiAZ 5292's right mirror housing and two dashboard
    // screens were holes. Such a mesh is turned round to face where its normals face. (Only
    // a mesh that is backwards nearly throughout: smoothed normals disagree with a few faces
    // of any ordinary mesh, and two-sided parts are two faces facing apart.)
    // The normals turned by the file's matrix are counted as well: an exporter that left
    // them in the object's own frame (the Solaris Urbino's lamps and dashboard screens: a
    // half turn about x) wrote faces that are wound the right way round.
    let rot = glam::Mat3::from_mat4(m.transform.transpose());
    let (mut against, mut against_turned, mut counted) = (0usize, 0usize, 0usize);
    for t in &m.triangles {
        let v = t.indices.map(|i| &m.vertices[i as usize]);
        let g = (v[1].position - v[0].position).cross(v[2].position - v[0].position);
        let n = v[0].normal + v[1].normal + v[2].normal;
        if g.length_squared() > 1e-12 && n.length_squared() > 1e-12 {
            counted += 1;
            if g.dot(n) < 0.0 {
                against += 1;
            }
            if g.dot(rot * n) < 0.0 {
                against_turned += 1;
            }
        }
    }
    // …but only when the file's own matrix says the object was mirrored (a positive
    // determinant where the exporter writes -1 for an ordinary object: the LiAZ's right
    // mirror housing, the Solaris' display planes). OMSI culls by winding alone
    // (D3DCULL_CCW, which only its stencil-shadow pass changes, the original), so a mesh
    // with an ordinary matrix is drawn as wound, whatever its normals say: the Procity's
    // dashboard screens face the driver by their winding and their normals away, and
    // turning them round hid the pressure and trip displays.
    // Turned round, the Urbino's headlamps faced into the bus and the body showed through
    // the holes in their place.
    // (An identity, or no matrix at all - every `.x`, an `.o3d` of the oldest exporters -
    // says nothing of a mirror: it is what an exporter writes that never mirrors. Taken
    // for one, a house whose exporter wrote its normals inward was turned inside out, its
    // walls seen from within (#874), and the road crossings of Buildings_Alex, wound to
    // face up with their normals down, faced the ground and left holes in the streets.)
    let linear = glam::Mat3::from_mat4(m.transform);
    let identity = linear.abs_diff_eq(glam::Mat3::IDENTITY, 1e-4);
    // Some exporters keep authored winding with a positive non-uniform scale or a small
    // rotation. Those transforms are not mirrors, even when the stored normals face the
    // other way. Ignore scale when checking whether the basis still follows the object axes.
    let axis_aligned = {
        let x = linear.x_axis.normalize_or_zero();
        let y = linear.y_axis.normalize_or_zero();
        let z = linear.z_axis.normalize_or_zero();
        x.dot(Vec3::X) > 0.9 && y.dot(Vec3::Y) > 0.9 && z.dot(Vec3::Z) > 0.9
    };
    let mirrored =
        m.has_transform && !identity && !axis_aligned && m.transform.determinant() > 0.0;
    let explained = against_turned * 10 <= counted;
    mirrored && !explained && counted >= 2 && against * 10 >= counted * 9
}

pub fn positive_det_faces_forward(m: &omsi_o3d::Mesh) -> Option<bool> {
    if m.transform.determinant() <= 0.0 {
        return None;
    }
    let (mut against, mut counted) = (0usize, 0usize);
    for t in &m.triangles {
        let v = t.indices.map(|i| &m.vertices[i as usize]);
        let g = (v[1].position - v[0].position).cross(v[2].position - v[0].position);
        let n = v[0].normal + v[1].normal + v[2].normal;
        if g.length_squared() > 1e-12 && n.length_squared() > 1e-12 {
            counted += 1;
            if g.dot(n) < 0.0 {
                against += 1;
            }
        }
    }
    match counted {
        0..=1 => None,
        _ if against * 10 <= counted => Some(true),
        _ if against * 10 >= counted * 9 => Some(false),
        _ => None,
    }
}

pub fn reverse_winding(data: &mut MeshData) {
    for t in data.indices.chunks_exact_mut(3) {
        t.swap(1, 2);
    }
}

#[cfg(test)]
mod tests;

mod raycast;
pub use raycast::*;
mod drive_grid;
pub use drive_grid::*;
mod tile_surface;
pub use tile_surface::*;


#[cfg(test)]
mod cant_tests {
    use super::*;

    #[test]
    fn cant_is_a_percentage_within_the_half_cant_width() {
        let c = SplineCurve { start: DVec3::ZERO, heading_deg: 0.0, length: 10.0, radius: 0.0, grad_start: 0.0, grad_end: 0.0, delta_h: None, cant_start: 5.0, cant_end: 5.0, skew_start: 0.0, skew_end: 0.0, tex_offset: 0.0, seed: 0, half_cant_width: 3.0 };
        // 2 m right at 5 %: 10 cm down
        assert!((c.offset_point(5.0, 2.0, 0.0).z + 0.10).abs() < 1e-9);
        // beyond the half cant width the height stays what it is at its edge
        assert!((c.offset_point(5.0, 6.0, 0.0).z + 0.15).abs() < 1e-9);
        assert!((c.offset_point(5.0, -6.0, 0.0).z - 0.15).abs() < 1e-9);
    }
}

#[cfg(test)]
mod ray_hit_tests {
    use super::*;

    /// A 2 x 2 quad in the x/z plane at y = 0 (x -1..1, z -1..1), two triangles, two material slots;
    /// u runs with x, v runs down (v = 0 at z = 1).
    fn quad() -> MeshData {
        MeshData {
            positions: vec![Vec3::new(-1.0, 0.0, 1.0), Vec3::new(1.0, 0.0, 1.0), Vec3::new(1.0, 0.0, -1.0), Vec3::new(-1.0, 0.0, -1.0)],
            normals: vec![Vec3::Y; 4],
            uvs: vec![Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0), Vec2::new(0.0, 1.0)],
            ranges: vec![(0, 3, 0), (3, 3, 1)],
            indices: vec![0, 1, 2, 0, 2, 3],
            one_sided: false,
        }
    }

    #[test]
    fn a_ray_gives_the_texture_coordinates_it_hit() {
        let m = quad();
        let id = Mat4::IDENTITY;
        let h = ray_mesh_hit(Vec3::new(0.5, -5.0, 0.5), Vec3::Y, &m, &id).unwrap();
        assert!((h.t - 5.0).abs() < 1e-4);
        assert!((h.uv - Vec2::new(0.75, 0.25)).length() < 1e-4, "{:?}", h.uv);
        assert_eq!(m.slot_of(h.index), 0, "the upper right triangle is slot 0");
        let h = ray_mesh_hit(Vec3::new(-0.5, -5.0, -0.5), Vec3::Y, &m, &id).unwrap();
        assert!((h.uv - Vec2::new(0.25, 0.75)).length() < 1e-4, "{:?}", h.uv);
        assert_eq!(m.slot_of(h.index), 1);
        assert!(ray_mesh_hit(Vec3::new(2.0, -5.0, 0.0), Vec3::Y, &m, &id).is_none());
    }

    #[test]
    fn the_mesh_transform_is_undone() {
        let m = quad();
        // the quad moved 10 m east and doubled in size: its middle is at x = 10, z = 0
        let xf = Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0)) * Mat4::from_scale(Vec3::splat(2.0));
        let h = ray_mesh_hit(Vec3::new(11.0, -3.0, 0.5), Vec3::Y, &m, &xf).unwrap();
        assert!((h.t - 3.0).abs() < 1e-4);
        assert!((h.uv - Vec2::new(0.75, 0.375)).length() < 1e-4, "{:?}", h.uv);
        assert_eq!(ray_mesh_hit(Vec3::new(11.0, -3.0, 0.5), Vec3::Y, &m, &xf).map(|h| h.t), ray_mesh(Vec3::new(11.0, -3.0, 0.5), Vec3::Y, &m, &xf));
    }
}


#[cfg(test)]
mod winding_transform_tests {
    use super::*;

    #[test]
    fn positive_near_identity_scale_keeps_authored_winding() {
        let v = |x: f32, y: f32| omsi_o3d::Vertex {
            position: Vec3::new(x, y, 1.0),
            normal: Vec3::new(0.0, 0.0, 1.0),
            uv: Vec2::ZERO,
        };
        let mesh = omsi_o3d::Mesh {
            vertices: vec![v(0.0, 0.0), v(0.0, 1.0), v(1.0, 0.0), v(1.0, 1.0)],
            triangles: vec![
                omsi_o3d::Triangle {
                    indices: [0, 1, 2],
                    material: 0,
                },
                omsi_o3d::Triangle {
                    indices: [2, 1, 3],
                    material: 0,
                },
            ],
            materials: vec![omsi_o3d::Material::default()],
            transform: glam::Mat4::from_scale(Vec3::new(0.918_948, 0.951_776, 1.0)),
            has_transform: true,
            ..Default::default()
        };
        assert_eq!(positive_det_faces_forward(&mesh), Some(false));
        assert!(!turns_round(&mesh));
        assert_eq!(mesh_from_o3d(&mesh).indices[..3], [0, 1, 2]);
    }
}
