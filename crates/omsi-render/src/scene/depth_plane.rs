//! Geometric planes used to keep depth offsets independent of lighting normals.
use crate::MeshData;
use glam::{DVec3, Mat4, Vec2, Vec2Swizzles, Vec3};

pub(crate) const NO_PLANE: [f32; 2] = [2.0, 2.0];

pub(crate) fn encode(normal_matrix: Mat4, normal: Vec3) -> [f32; 2] {
    let Some(n) = normal_matrix.transform_vector3(normal).try_normalize() else {
        return NO_PLANE;
    };
    if !n.is_finite() {
        return NO_PLANE;
    }
    let n = n / n.abs().element_sum();
    let packed = if n.z < 0.0 {
        (Vec2::ONE - n.truncate().abs().yx())
            * Vec2::new(if n.x < 0.0 { -1.0 } else { 1.0 }, if n.y < 0.0 { -1.0 } else { 1.0 })
    } else {
        n.truncate()
    };
    packed.to_array()
}

pub(crate) fn normals(data: &MeshData) -> Vec<Vec3> {
    let slots = data.ranges.iter().map(|range| range.2 as usize + 1).max().unwrap_or(0);
    let mut planes = vec![(DVec3::ZERO, DVec3::ZERO, 0.0f64); slots];
    for &(first, count, slot) in &data.ranges {
        let end = first.saturating_add(count).min(data.indices.len() as u32);
        let Some(indices) = data.indices.get(first as usize..end as usize) else { continue };
        for triangle in indices.chunks_exact(3) {
            let (Some(a), Some(b), Some(c)) = (
                data.positions.get(triangle[0] as usize),
                data.positions.get(triangle[1] as usize),
                data.positions.get(triangle[2] as usize),
            ) else { continue };
            let cross = (b.as_dvec3() - a.as_dvec3()).cross(c.as_dvec3() - a.as_dvec3());
            let area = cross.length();
            let plane = &mut planes[slot as usize];
            if area.is_finite() && area > plane.2 && area > 2e-8 {
                *plane = (a.as_dvec3(), cross / area, area);
            }
        }
    }

    let mut valid: Vec<bool> = planes.iter().map(|plane| plane.2 > 0.0).collect();
    for &(first, count, slot) in &data.ranges {
        let slot = slot as usize;
        let end = first.saturating_add(count).min(data.indices.len() as u32);
        let Some(indices) = data.indices.get(first as usize..end as usize) else {
            valid[slot] = false;
            continue;
        };
        let reference = planes[slot].1;
        for triangle in indices.chunks_exact(3) {
            let (Some(a), Some(b), Some(c)) = (
                data.positions.get(triangle[0] as usize),
                data.positions.get(triangle[1] as usize),
                data.positions.get(triangle[2] as usize),
            ) else {
                valid[slot] = false;
                break;
            };
            let cross = (b.as_dvec3() - a.as_dvec3()).cross(c.as_dvec3() - a.as_dvec3());
            let area = cross.length();
            if !area.is_finite() || area <= 2e-8 || reference.dot(cross / area).abs() < 0.9995 {
                valid[slot] = false;
                break;
            }
        }
    }

    planes.into_iter().zip(valid).map(|((_, normal, _), valid)| {
        if valid { normal.as_vec3() } else { Vec3::ZERO }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(encoded: [f32; 2]) -> Vec3 {
        let p = Vec2::from_array(encoded);
        let mut n = Vec3::new(p.x, p.y, 1.0 - p.abs().element_sum());
        let fold = (-n.z).clamp(0.0, 1.0);
        n.x += if n.x >= 0.0 { -fold } else { fold };
        n.y += if n.y >= 0.0 { -fold } else { fold };
        n.normalize()
    }

    #[test]
    fn packed_normals_round_trip_both_hemispheres() {
        for normal in [Vec3::X, Vec3::Y, Vec3::Z, -Vec3::X, -Vec3::Y, -Vec3::Z, Vec3::new(0.3, -0.7, -0.6).normalize()] {
            assert!(decode(encode(Mat4::IDENTITY, normal)).distance(normal) < 1e-6);
        }
        assert_eq!(encode(Mat4::IDENTITY, Vec3::ZERO), NO_PLANE);
        assert_eq!(encode(Mat4::NAN, Vec3::Z), NO_PLANE);
    }

    #[test]
    fn only_planar_slots_receive_depth_normals() {
        let mesh = MeshData {
            positions: vec![Vec3::ZERO, Vec3::X, Vec3::Y],
            indices: vec![0, 1, 2],
            ranges: vec![(0, 3, 0)],
            ..Default::default()
        };
        assert_eq!(normals(&mesh), vec![Vec3::Z]);
    }

    #[test]
    fn parallel_layers_in_one_slot_share_a_geometric_depth_normal() {
        let mesh = MeshData {
            positions: vec![
                Vec3::ZERO,
                Vec3::X,
                Vec3::Y,
                Vec3::new(0.0, 0.0, 0.1),
                Vec3::new(1.0, 0.0, 0.1),
                Vec3::new(0.0, 1.0, 0.1),
            ],
            indices: vec![0, 1, 2, 3, 4, 5],
            ranges: vec![(0, 6, 0)],
            ..Default::default()
        };
        assert_eq!(normals(&mesh), vec![Vec3::Z]);
    }

    #[test]
    fn nonparallel_layers_do_not_share_a_geometric_depth_normal() {
        let mesh = MeshData {
            positions: vec![
                Vec3::ZERO,
                Vec3::X,
                Vec3::Y,
                Vec3::ZERO,
                Vec3::X,
                Vec3::new(0.0, 1.0, 0.2),
            ],
            indices: vec![0, 1, 2, 3, 4, 5],
            ranges: vec![(0, 6, 0)],
            ..Default::default()
        };
        assert_eq!(normals(&mesh), vec![Vec3::ZERO]);
    }
}
