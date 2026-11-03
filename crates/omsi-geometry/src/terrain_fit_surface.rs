//! Terrain backing uses the rendered cross-section and its tangents, not a four-point V.
use super::*;
use std::collections::HashMap;

// The road shoulder may be steep, but an almost vertical spline face must not
// extrapolate its tangent into kilometer-high terrain spikes.
const MAX_SHOULDER_GRADIENT: f64 = 8.0;

pub(super) fn profiles(def: &Spline) -> Vec<Vec<[f32; 3]>> {
    if !def.terrain_hole_profiles.is_empty() {
        return def.terrain_hole_profiles.clone();
    }
    let parts: Vec<Vec<[f32; 2]>> = def
        .profiles
        .iter()
        .map(|p| p.points.iter().map(|p| [p.x, p.z]).collect())
        .collect();
    let mut used = vec![false; parts.len()];
    let mut out = Vec::new();
    while let Some(first) = used.iter().position(|u| !u) {
        used[first] = true;
        let mut chain = parts[first].clone();
        while let Some(last) = chain.last().copied() {
            let next = parts.iter().enumerate().position(|(i, p)| {
                !used[i]
                    && p.first().is_some_and(|q| {
                        (q[0] - last[0]).abs() < 0.01 && (q[1] - last[1]).abs() < 0.01
                    })
            });
            let Some(next) = next else {
                break;
            };
            used[next] = true;
            chain.extend(parts[next].iter().copied());
        }
        chain.dedup();
        if chain.len() < 2 {
            continue;
        }
        // A projected height field cannot represent an overhanging cross-section.
        // Preserve the existing authored/fallback cutter for those unusual splines.
        if chain.windows(2).any(|p| p[1][0] < p[0][0]) {
            return terrain_hole_profiles(def);
        }
        let (left, right) = (chain[0][0] + 0.03, chain.last().unwrap()[0] - 0.03);
        if left >= right {
            continue;
        }
        let at = |x: f32| {
            chain.windows(2).find_map(|p| {
                let d = p[1][0] - p[0][0];
                (d > 1e-6 && x >= p[0][0] && x <= p[1][0])
                    .then(|| [x, p[0][1] + (p[1][1] - p[0][1]) * (x - p[0][0]) / d])
            })
        };
        let (Some(a), Some(b)) = (at(left), at(right)) else {
            continue;
        };
        let mut section = vec![a];
        section.extend(chain.into_iter().filter(|p| p[0] > left && p[0] < right));
        section.push(b);
        let depth =
            |p: &[f32; 2]| (a[1] + (b[1] - a[1]) * (p[0] - left) / (right - left) - p[1]).max(0.0);
        let deepest = section.iter().map(depth).fold(0.0, f32::max);
        out.push(
            section
                .iter()
                .map(|p| {
                    [
                        p[0],
                        p[1] - 0.003,
                        if deepest > 1e-6 {
                            -0.5 * depth(p) / deepest
                        } else {
                            0.0
                        },
                    ]
                })
                .collect(),
        );
    }
    out
}

/// Each boundary edge carries the adjacent surface's horizontal gradient. This
/// lets the exterior blend match both the rim elevation and the slope at that rim.
pub(super) fn boundary(mesh: &MeshData, offset: DVec3) -> Vec<([DVec3; 2], DVec2)> {
    let mut edges = HashMap::<(u32, u32), (usize, [DVec3; 2], DVec2)>::new();
    for t in mesh.indices.chunks_exact(3) {
        let [a, b, c] = [0, 1, 2].map(|i| mesh.positions[t[i] as usize].as_dvec3() + offset);
        let (d, e) = (b - a, c - a);
        let det = d.truncate().perp_dot(e.truncate());
        if det.abs() < 1e-8 {
            continue;
        }
        let gradient = DVec2::new(d.z * e.y - e.z * d.y, d.x * e.z - e.x * d.z) / det;
        let gradient = if gradient.is_finite() {
            gradient.clamp_length_max(MAX_SHOULDER_GRADIENT)
        } else {
            DVec2::ZERO
        };
        for j in 0..3 {
            let (u, v) = (t[j], t[(j + 1) % 3]);
            let entry = edges.entry((u.min(v), u.max(v))).or_insert((
                0,
                [
                    mesh.positions[u as usize].as_dvec3() + offset,
                    mesh.positions[v as usize].as_dvec3() + offset,
                ],
                gradient,
            ));
            entry.0 += 1;
        }
    }
    let mut edges: Vec<_> = edges.into_iter().filter(|(_, v)| v.0 == 1).collect();
    edges.sort_by_key(|(k, _)| *k);
    edges
        .into_iter()
        .map(|(_, (_, points, gradient))| (points, gradient))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steep_narrow_spline_face_has_a_bounded_shoulder_tangent() {
        // The XY projection is a 1 mm sliver, but the surface climbs 1 km.
        // Extrapolating that gradient into the adjacent terrain creates spikes.
        let mesh = MeshData {
            positions: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(0.001, 0.0, 1000.0),
                Vec3::new(0.0, 0.001, 0.0),
            ],
            indices: vec![0, 1, 2],
            ..Default::default()
        };
        let edges = boundary(&mesh, DVec3::ZERO);
        assert_eq!(edges.len(), 3);
        for (_, tangent) in edges {
            assert!(tangent.is_finite());
            assert!(tangent.length() <= MAX_SHOULDER_GRADIENT + 1e-6);
        }
    }

    #[test]
    fn ordinary_road_slope_keeps_its_tangent() {
        let mesh = MeshData {
            positions: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.5),
                Vec3::new(0.0, 1.0, 0.0),
            ],
            indices: vec![0, 1, 2],
            ..Default::default()
        };
        let edges = boundary(&mesh, DVec3::ZERO);
        assert_eq!(edges.len(), 3);
        for (_, tangent) in edges {
            assert!((tangent.length() - 0.5).abs() < 1e-6);
        }
    }
}

[executed on device: Gaming PC (5e20e136-803f-4de7-b45c-6cae00d89d56)]