//! Geometric planes for depth offsets, independent of authored lighting normals.
use crate::MeshData;
use glam::{DVec3, Mat4, Vec2, Vec2Swizzles, Vec3};

pub(crate) const NO_PLANE: [f32; 2] = [2.0, 2.0];
const PLANE_GAP: f64 = 5e-5;

/// Encode the inverse-transpose transformed normal in two unused affine row entries.
/// Degenerate transforms and nonplanar slots keep the existing shading-normal fallback.
pub(crate) fn encode(normal_matrix: Mat4, normal: Vec3) -> [f32; 2] {
    let Some(n) = normal_matrix.transform_vector3(normal).try_normalize() else {
        return NO_PLANE;
    };
    if !n.is_finite() {
        return NO_PLANE;
    }
    let n = n / n.abs().element_sum();
    let p = if n.z < 0.0 {
        (Vec2::ONE - n.truncate().abs().yx())
            * Vec2::new(
                if n.x < 0.0 { -1.0 } else { 1.0 },
                if n.y < 0.0 { -1.0 } else { 1.0 },
            )
    } else {
        n.truncate()
    };
    p.to_array()
}

/// Use each slot's largest triangle as its candidate plane, then check every referenced
/// vertex, including small faces omitted from the coplanar ordering's bounded summary.
/// Double precision avoids losing micrometre offsets in meshes far from their local origin.
pub(crate) fn normals(data: &MeshData) -> Vec<Vec3> {
    let slots = data
        .ranges
        .iter()
        .map(|r| r.2 as usize + 1)
        .max()
        .unwrap_or(0);
    let mut planes = vec![(DVec3::ZERO, DVec3::ZERO, 0.0f64); slots];
    for &(first, count, slot) in &data.ranges {
        let end = first.saturating_add(count).min(data.indices.len() as u32);
        let Some(indices) = data.indices.get(first as usize..end as usize) else {
            continue;
        };
        for t in indices.chunks_exact(3) {
            let Some(a) = data.positions.get(t[0] as usize) else {
                continue;
            };
            let Some(b) = data.positions.get(t[1] as usize) else {
                continue;
            };
            let Some(c) = data.positions.get(t[2] as usize) else {
                continue;
            };
            let cross = (b.as_dvec3() - a.as_dvec3()).cross(c.as_dvec3() - a.as_dvec3());
            let area = cross.length();
            if area.is_finite() && area > planes[slot as usize].2 && area > 2e-8 {
                planes[slot as usize] = (a.as_dvec3(), cross / area, area);
            }
        }
    }
    let mut valid: Vec<bool> = planes.iter().map(|p| p.2 > 0.0).collect();
    for &(first, count, slot) in &data.ranges {
        let slot = slot as usize;
        let (point, normal, _) = planes[slot];
        let end = first.saturating_add(count).min(data.indices.len() as u32);
        let Some(indices) = data.indices.get(first as usize..end as usize) else {
            valid[slot] = false;
            continue;
        };
        for &index in indices {
            let Some(p) = data.positions.get(index as usize) else {
                valid[slot] = false;
                break;
            };
            let gap = normal.dot(p.as_dvec3() - point).abs();
            if !gap.is_finite() || gap > PLANE_GAP {
                valid[slot] = false;
                break;
            }
        }
    }
    planes
        .into_iter()
        .zip(valid)
        .map(|((_, n, _), valid)| if valid { n.as_vec3() } else { Vec3::ZERO })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(encoded: [f32; 2]) -> Vec3 {
        let p = Vec2::from_array(encoded);
        let mut n = Vec3::new(p.x, p.y, 1.0 - p.abs().element_sum());
        let t = (-n.z).clamp(0.0, 1.0);
        n.x += if n.x >= 0.0 { -t } else { t };
        n.y += if n.y >= 0.0 { -t } else { t };
        n.normalize()
    }

    #[test]
    fn octahedral_normals_round_trip_both_hemispheres() {
        for n in [
            Vec3::X,
            Vec3::Y,
            Vec3::Z,
            -Vec3::X,
            -Vec3::Y,
            -Vec3::Z,
            Vec3::new(0.3, -0.7, -0.6).normalize(),
        ] {
            assert!(decode(encode(Mat4::IDENTITY, n)).distance(n) < 1e-6);
        }
        assert_eq!(encode(Mat4::IDENTITY, Vec3::ZERO), NO_PLANE);
        assert_eq!(encode(Mat4::NAN, Vec3::Z), NO_PLANE);
    }

    #[test]
    fn nonuniform_scale_uses_the_inverse_transpose() {
        let transform = Mat4::from_scale(Vec3::new(2.0, 1.0, 1.0));
        let n = decode(encode(
            transform.inverse().transpose(),
            Vec3::new(1.0, 0.0, 1.0).normalize(),
        ));
        assert!(n.distance(Vec3::new(0.5, 0.0, 1.0).normalize()) < 1e-6);
    }

    #[test]
    fn every_indexed_vertex_must_lie_in_the_plane() {
        let mut mesh = MeshData {
            positions: vec![Vec3::ZERO, Vec3::X, Vec3::Y],
            indices: vec![0, 1, 2],
            ranges: vec![(0, 3, 0)],
            ..Default::default()
        };
        assert_eq!(normals(&mesh), vec![Vec3::Z]);
        // A smaller folded face can share the centre while its edges leave the plane.
        mesh.positions.extend([
            Vec3::ZERO,
            Vec3::new(0.1, 0.0, 0.00175),
            Vec3::new(0.0, 0.1, 0.0),
        ]);
        mesh.indices.extend([3, 4, 5]);
        mesh.ranges[0].1 = 6;
        assert_eq!(normals(&mesh), vec![Vec3::ZERO]);
        // Separate slots do not invalidate the planar base.
        mesh.ranges = vec![(0, 3, 0), (3, 3, 2)];
        let n = normals(&mesh);
        assert_eq!(&n[..2], &[Vec3::Z, Vec3::ZERO]);
        assert!(n[2].length() > 0.99);
        mesh.indices[0] = 99;
        assert_eq!(normals(&mesh)[0], Vec3::ZERO);
    }
}
