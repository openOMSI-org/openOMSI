//! Join straight sampling runs without modifying the underlying height field.
use glam::{DVec2, DVec3};
use std::collections::BTreeMap;

type Key = [u64; 3];
fn key(p: DVec3) -> Key {
    p.to_array().map(|v| if v == 0.0 { 0 } else { v.to_bits() })
}

/// Only degree-two, straight joins with identical sample widths may be removed.
/// Branches, corners, different shoulder widths and disconnected rims stay separate.
pub(super) fn runs(edges: Vec<([DVec3; 2], f64)>) -> Vec<([DVec3; 2], f64)> {
    let mut nodes = BTreeMap::<Key, Vec<usize>>::new();
    for (i, (points, _)) in edges.iter().enumerate() {
        for &p in points {
            nodes.entry(key(p)).or_default().push(i);
        }
    }
    let mut visited = vec![false; edges.len()];
    let mut result = Vec::new();
    for (i, &(mut points, width)) in edges.iter().enumerate() {
        if visited[i] {
            continue;
        }
        visited[i] = true;
        for end in 0..2 {
            loop {
                let adjacent = &nodes[&key(points[end])];
                if adjacent.len() != 2 {
                    break;
                }
                let Some(&next) = adjacent.iter().find(|&&j| !visited[j]) else {
                    break;
                };
                let (candidate, other_width) = edges[next];
                if width != other_width {
                    break;
                }
                let other = if key(candidate[0]) == key(points[end]) {
                    candidate[1]
                } else {
                    candidate[0]
                };
                let a = points[1 - end] - points[end];
                let b = other - points[end];
                if a.dot(b) >= 0.0 || a.cross(b).length() > 1e-12 * a.length() * b.length() {
                    break;
                }
                visited[next] = true;
                points[end] = other;
            }
        }
        result.push((points, width));
    }
    result
}

/// Insert a dense probe only when the current piecewise-linear surface misses its
/// height budget. Recheck after edge flips; a bounded dense fallback preserves
/// correctness if refinement does not converge within sixteen passes.
pub(super) fn refine(
    cdt: &mut super::Cdt,
    mut points: Vec<DVec2>,
    height: impl Fn(DVec2) -> f64,
    max_error: impl Fn(DVec2, f64) -> f64,
) -> Result<(), spade::InsertionError> {
    use spade::Triangulation;
    for p in &mut points {
        *p = super::super::terrain_constraints::snap(*p);
    }
    points.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    points.dedup();
    let probes: Vec<_> = points
        .into_iter()
        .map(|p| {
            let h = height(p);
            (p, h, max_error(p, h))
        })
        .collect();
    // Neighboring tiles must keep the same clipped seam stations independently
    // of their interior refinement and edge flips.
    let side = super::super::tile_size();
    for &(p, _, _) in &probes {
        if p.x == 0.0 || p.y == 0.0 || p.x == side || p.y == side {
            super::insert(cdt, p)?;
        }
    }
    let mut cache = Vec::<Option<f64>>::new();
    let mut weights = Vec::with_capacity(3);
    for _ in 0..8 {
        // Pick the largest error per current face before mutating the mesh.
        // Greedy insertion from an empty fit over-refines straight shoulders.
        let mut selected = BTreeMap::<[usize; 3], (DVec2, f64, f64)>::new();
        for &(p, h, error) in &probes {
            let actual = interpolate(cdt, p, &height, &mut cache, &mut weights);
            let deviation = actual.map_or(f64::INFINITY, |v| (v - h).abs());
            if deviation > error {
                let mut face = [usize::MAX; 3];
                for (i, (v, _)) in weights.iter().enumerate() {
                    face[i] = v.index();
                }
                face.sort_unstable();
                let candidate = selected.entry(face).or_insert((p, h, deviation));
                if deviation > candidate.2 {
                    *candidate = (p, h, deviation);
                }
            }
        }
        if selected.is_empty() {
            return Ok(());
        }
        for (_, (p, h, _)) in selected {
            let v = super::insert(cdt, p)?;
            cache.resize(cdt.num_vertices(), None);
            cache[v.index()] = Some(h);
        }
    }
    // Once the coarse fit is established, resolve small remaining errors in one
    // sweep rather than subdividing an almost-flat face once per entire pass.
    for _ in 0..8 {
        let mut inserted = false;
        for &(p, h, error) in &probes {
            let actual = interpolate(cdt, p, &height, &mut cache, &mut weights);
            if actual.is_none_or(|v| (v - h).abs() > error) {
                let v = super::insert(cdt, p)?;
                cache.resize(cdt.num_vertices(), None);
                cache[v.index()] = Some(h);
                inserted = true;
            }
        }
        if !inserted {
            return Ok(());
        }
    }
    for (p, _, _) in probes {
        super::insert(cdt, p)?;
    }
    Ok(())
}

fn interpolate(
    cdt: &super::Cdt,
    p: DVec2,
    height: &impl Fn(DVec2) -> f64,
    cache: &mut Vec<Option<f64>>,
    weights: &mut Vec<(spade::handles::FixedVertexHandle, f64)>,
) -> Option<f64> {
    use spade::{FloatTriangulation, Point2, Triangulation};
    cache.resize(cdt.num_vertices(), None);
    cdt.barycentric()
        .get_weights(Point2::new(p.x, p.y), weights);
    if weights.is_empty() {
        return None;
    }
    Some(
        weights
            .iter()
            .map(|(v, w)| {
                let h = cache[v.index()].get_or_insert_with(|| {
                    let q = cdt.vertex(*v).position();
                    height(DVec2::new(q.x, q.y))
                });
                *h * *w
            })
            .sum(),
    )
}

/// A triangulated planar strip needs its boundary, not every station's diagonal.
/// Keep all true slope changes and ambiguous/non-manifold joins. This does not
/// simplify the source surfaces used for height or normal evaluation.
pub(super) fn creases<'a>(
    triangles: impl Iterator<Item = &'a [glam::DVec3; 3]>,
) -> Vec<[DVec2; 2]> {
    type Point = [u64; 3];
    type Edge = (usize, [DVec3; 2], DVec2, bool, bool);
    let mut edges = BTreeMap::<[Point; 2], Edge>::new();
    for &[a, b, c] in triangles {
        let (d, e) = (b - a, c - a);
        let det = d.truncate().perp_dot(e.truncate());
        if det.abs() < 1e-12 {
            continue;
        }
        let gradient = DVec2::new(d.z * e.y - e.z * d.y, d.x * e.z - e.x * d.z) / det;
        for [u, v, w] in [[a, b, c], [b, c, a], [c, a, b]] {
            let key = |p: glam::DVec3| p.to_array().map(|v| if v == 0.0 { 0 } else { v.to_bits() });
            let mut pair = [key(u), key(v)];
            let side = (v - u).truncate().perp_dot((w - u).truncate())
                * if pair[0] <= pair[1] { 1.0 } else { -1.0 };
            pair.sort_unstable();
            let edge = edges
                .entry(pair)
                .or_insert((0, [u, v], gradient, false, side > 0.0));
            // Duplicate/overlapping faces occupy the same side of an edge;
            // their common boundary must not be mistaken for an interior seam.
            if edge.0 > 0 {
                edge.3 |= edge.4 == (side > 0.0);
            }
            edge.0 += 1;
            edge.3 |= (edge.2 - gradient).abs().max_element() > 1e-10;
        }
    }
    runs(
        edges
            .into_values()
            .filter(|e| e.0 != 2 || e.3)
            .map(|e| (e.1, 0.0))
            .collect(),
    )
    .into_iter()
    .map(|(p, _)| p.map(|p| p.truncate()))
    .collect()
}
