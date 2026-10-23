//! Planarize terrain constraints before giving them to the triangulator. In particular,
//! never ask Spade's incremental constraint splitter to resolve nearly touching roads.
use super::terrain_fit::{Cdt, TerrainFitError};
use glam::DVec2;
use spade::{Point2, Triangulation, handles::FixedVertexHandle};
use std::collections::{HashMap, HashSet};

const SCALE: f64 = 10_000.0; // 0.1 mm stays distinct in tile-local f32 coordinates.
type Key = (i64, i64);

fn key(p: DVec2) -> Key {
    ((p.x * SCALE).round() as i64, (p.y * SCALE).round() as i64)
}
fn point(p: Key) -> DVec2 {
    // Triangulate exactly the XY coordinates the renderer will receive. Rounding
    // a narrow double-precision triangle only on output can collapse or invert it.
    DVec2::new(p.0 as f64 / SCALE, p.1 as f64 / SCALE)
        .as_vec2()
        .as_dvec2()
}

pub(super) fn snap(p: DVec2) -> DVec2 {
    point(key(p))
}

fn crossings(a: [DVec2; 2], b: [DVec2; 2]) -> Vec<Key> {
    let (d, e, offset) = (a[1] - a[0], b[1] - b[0], b[0] - a[0]);
    let cross = d.perp_dot(e);
    if cross.abs() > 1e-12 * d.length() * e.length() {
        let (t, u) = (offset.perp_dot(e) / cross, offset.perp_dot(d) / cross);
        if (-1e-10..=1.0 + 1e-10).contains(&t) && (-1e-10..=1.0 + 1e-10).contains(&u) {
            return vec![key(a[0] + d * t.clamp(0.0, 1.0))];
        }
    } else if offset.perp_dot(d).abs() <= 1e-10 * d.length() {
        // Shared endpoints and collinear overlaps must get the same split vertices.
        return a
            .into_iter()
            .chain(b)
            .filter(|p| {
                let (t, u) = (
                    (*p - a[0]).dot(d) / d.length_squared(),
                    (*p - b[0]).dot(e) / e.length_squared(),
                );
                (-1e-10..=1.0 + 1e-10).contains(&t) && (-1e-10..=1.0 + 1e-10).contains(&u)
            })
            .map(key)
            .collect();
    }
    Vec::new()
}

fn vertex(
    cdt: &mut Cdt,
    map: &mut HashMap<Key, FixedVertexHandle>,
    p: Key,
) -> Result<FixedVertexHandle, TerrainFitError> {
    if let Some(&v) = map.get(&p) {
        return Ok(v);
    }
    if map.len() >= 65_536 {
        return Err(TerrainFitError::Complexity);
    }
    let q = point(p);
    let v = cdt.insert(Point2::new(q.x, q.y))?;
    map.insert(p, v);
    Ok(v)
}

/// Split crossings in a tile-local broad phase, inserting all vertices before edges.
/// Rounded intersections are shared by both segments, rather than independently
/// computed while the triangulator is mutating a neighboring constraint region.
pub(super) fn add(
    cdt: &mut Cdt,
    segments: &[[DVec2; 2]],
    side: f64,
    bins: usize,
) -> Result<(), TerrainFitError> {
    let mut unique = HashSet::new();
    let segments: Vec<_> = segments
        .iter()
        .filter_map(|&[a, b]| {
            let (a, b) = (key(a), key(b));
            (a != b && unique.insert((a.min(b), a.max(b)))).then(|| [point(a), point(b)])
        })
        .collect();
    let mut splits: Vec<Vec<Key>> = segments.iter().map(|p| p.map(key).to_vec()).collect();
    let mut spatial = vec![Vec::<usize>::new(); bins * bins];
    let mut visits = 0usize;
    for (i, &[a, b]) in segments.iter().enumerate() {
        let covered = covered_bins([a, b], side, bins);
        let mut neighbors = Vec::new();
        for &cell in &covered {
            let entries = &spatial[cell];
            visits += entries.len();
            if visits > 8_000_000 {
                return Err(TerrainFitError::Complexity);
            }
            neighbors.extend_from_slice(entries);
        }
        neighbors.sort_unstable();
        neighbors.dedup();
        for j in neighbors {
            let points = crossings([a, b], segments[j]);
            splits[i].extend_from_slice(&points);
            splits[j].extend(points);
        }
        for cell in covered {
            spatial[cell].push(i);
        }
    }
    let mut vertices = HashMap::new();
    // All crossings are present before the first constraint: no automatic edge
    // splitting or callbacks into a half-modified CDT are needed.
    for split in &splits {
        for &p in split {
            vertex(cdt, &mut vertices, p)?;
        }
    }
    let mut rejected = 0usize;
    for (split, [a, b]) in splits.iter_mut().zip(segments) {
        let d = b - a;
        split.sort_by(|&u, &v| (point(u) - a).dot(d).total_cmp(&(point(v) - a).dot(d)));
        split.dedup();
        for pair in split.windows(2) {
            let (a, b) = (vertices[&pair[0]], vertices[&pair[1]]);
            if a != b && cdt.try_add_constraint(a, b).is_empty() {
                // Weld rounding can turn sub-pixel segments into a crossing. Retain
                // their vertices and the solid CDT surface, never invoke the unsafe
                // splitter and never discard the tile because of a tiny crease.
                rejected += 1;
            }
        }
    }
    if rejected > 0 {
        log::debug!("terrain fitting: {rejected} sub-pixel constraints retained as vertices");
    }
    Ok(())
}

/// Conservative supercover: include both cells at a grid boundary. Visiting
/// each crossed column avoids the quadratic bounding-box area of diagonal roads.
pub(super) fn covered_bins([a, b]: [DVec2; 2], side: f64, bins: usize) -> Vec<usize> {
    let cell = side / bins as f64;
    let d = b - a;
    let index = |v: f64| (v / cell).floor().clamp(0.0, (bins - 1) as f64) as usize;
    // The original rectangle is already optimal for grid edges and short
    // constraints. Avoid extra boundary halos and per-column arithmetic there.
    let (x0, x1) = (index(a.x.min(b.x)), index(a.x.max(b.x)));
    let (y0, y1) = (index(a.y.min(b.y)), index(a.y.max(b.y)));
    if x0 == x1 || y0 == y1 || (x1 - x0 + 1) * (y1 - y0 + 1) <= 8 {
        return (x0..=x1).flat_map(|x| (y0..=y1).map(move |y| y * bins + x)).collect();
    }
    let epsilon = 1e-8;
    let x0 = index(a.x.min(b.x) - epsilon);
    let x1 = index(a.x.max(b.x) + epsilon);
    let mut covered = Vec::new();
    for x in x0..=x1 {
        let (lo, hi) = if d.x.abs() < 1e-12 {
            (0.0, 1.0)
        } else {
            let u = (x as f64 * cell - epsilon - a.x) / d.x;
            let v = ((x + 1) as f64 * cell + epsilon - a.x) / d.x;
            (u.min(v).max(0.0), u.max(v).min(1.0))
        };
        if lo > hi {
            continue;
        }
        let ya = a.y + d.y * lo;
        let yb = a.y + d.y * hi;
        for y in index(ya.min(yb) - epsilon)..=index(ya.max(yb) + epsilon) {
            covered.push(y * bins + x);
        }
    }
    covered
}
