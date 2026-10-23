//! Planarize terrain constraints before giving them to the triangulator. In particular,
//! never ask Spade's incremental constraint splitter to resolve nearly touching roads.
use super::terrain_fit::Cdt;
use glam::DVec2;
use spade::{InsertionError, Point2, Triangulation, handles::FixedVertexHandle};
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
) -> Result<FixedVertexHandle, InsertionError> {
    if let Some(&v) = map.get(&p) {
        return Ok(v);
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
) -> Result<(), InsertionError> {
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
    let bin = |p: DVec2| {
        let p = (p / side * bins as f64)
            .floor()
            .clamp(DVec2::ZERO, DVec2::splat((bins - 1) as f64));
        (p.x as usize, p.y as usize)
    };
    for (i, &[a, b]) in segments.iter().enumerate() {
        let (lo, hi) = (a.min(b), a.max(b));
        let ((x0, y0), (x1, y1)) = (bin(lo), bin(hi));
        let mut neighbors = Vec::new();
        for y in y0..=y1 {
            for x in x0..=x1 {
                neighbors.extend_from_slice(&spatial[y * bins + x]);
            }
        }
        neighbors.sort_unstable();
        neighbors.dedup();
        for j in neighbors {
            let points = crossings([a, b], segments[j]);
            splits[i].extend_from_slice(&points);
            splits[j].extend(points);
        }
        for y in y0..=y1 {
            for x in x0..=x1 {
                spatial[y * bins + x].push(i);
            }
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
