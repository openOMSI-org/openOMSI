use super::{PLANE_GAP, canonical, normal_key};
use crate::MeshData;
use glam::Vec3;

/// Large planes retained even when they are not the local representative.
pub(super) const FLATS_PER_SLOT: usize = 32;
/// Local planes retained per normal-and-position cell before the slot-wide bound.
const FLATS_PER_CELL: usize = 16;
const LOCAL_FLATS_PER_SLOT: usize = 256;
const FLAT_CELL_SIZE: f32 = 0.5;

/// A flat surface of a mesh's material slot: its plane and the box its triangles lie in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Flat {
    pub(crate) slot: u32,
    pub(crate) normal: Vec3,
    pub(super) orientation: f32,
    pub(crate) offset: f32,
    pub(crate) lo: Vec3,
    pub(crate) hi: Vec3,
    pub(crate) axis: Vec3,
    pub(crate) half: [f32; 2],
}

/// Flat surfaces grouped by plane, retaining large planes and bounded local representatives.
pub(crate) fn mesh_flats(data: &MeshData) -> Vec<Flat> {
    let mut out = Vec::new();
    for &(first, count, slot) in &data.ranges {
        let end = (first + count).min(data.indices.len() as u32);
        let mut planes: hashbrown::HashMap<(i32, i32, i32, i32), (Flat, f32, [f32; 4])> =
            hashbrown::HashMap::with_capacity((end.saturating_sub(first) / 3) as usize);
        let mut k = first as usize;
        let at = |i: u32| data.positions.get(i as usize).copied();
        while k + 2 < end as usize {
            let idx = [data.indices[k], data.indices[k + 1], data.indices[k + 2]];
            k += 3;
            let (Some(a), Some(b), Some(c)) = (at(idx[0]), at(idx[1]), at(idx[2])) else {
                continue;
            };
            let points = [a, b, c];
            let cross = (b - a).cross(c - a);
            let area = cross.length() * 0.5;
            if area < 1e-8 {
                continue;
            }
            let source_normal = cross / (2.0 * area);
            let (normal, offset) = canonical(source_normal, source_normal.dot(a));
            let key = (
                (normal.x * 1000.0).round() as i32,
                (normal.y * 1000.0).round() as i32,
                (normal.z * 1000.0).round() as i32,
                (offset / PLANE_GAP).round() as i32,
            );
            let lo = a.min(b).min(c);
            let hi = a.max(b).max(c);
            let entry = planes.entry(key).or_insert_with(|| {
                let mut edges = [b - a, c - b, a - c];
                edges.sort_by(|left, right| {
                    left.length_squared().total_cmp(&right.length_squared())
                });
                let edge = edges[1];
                let axis = (edge - normal * normal.dot(edge))
                    .try_normalize()
                    .unwrap_or_else(|| normal.any_orthonormal_vector());
                (
                    Flat {
                        slot,
                        normal,
                        orientation: if normal.dot(source_normal) < 0.0 {
                            -1.0
                        } else {
                            1.0
                        },
                        offset,
                        lo,
                        hi,
                        axis,
                        half: [0.0; 2],
                    },
                    0.0,
                    [f32::MAX, f32::MIN, f32::MAX, f32::MIN],
                )
            });
            entry.0.lo = entry.0.lo.min(lo);
            entry.0.hi = entry.0.hi.max(hi);
            entry.1 += area;
            let across = normal.cross(entry.0.axis);
            for point in points {
                let along = point.dot(entry.0.axis);
                let across = point.dot(across);
                entry.2 = [
                    entry.2[0].min(along),
                    entry.2[1].max(along),
                    entry.2[2].min(across),
                    entry.2[3].max(across),
                ];
            }
        }
        let mut flats: Vec<(Flat, f32)> = planes
            .into_values()
            .map(|(mut flat, area, extents)| {
                flat.half = [
                    (extents[1] - extents[0]) * 0.5,
                    (extents[3] - extents[2]) * 0.5,
                ];
                (flat, area)
            })
            .collect();
        if flats.len() > FLATS_PER_SLOT {
            let mut local = hashbrown::HashMap::<_, Vec<(Flat, f32)>>::new();
            for &(flat, area) in &flats {
                local
                    .entry(flat_cell_key(&flat))
                    .or_default()
                    .push((flat, area));
            }
            let mut selected = Vec::new();
            for mut bucket in local.into_values() {
                if bucket.len() > FLATS_PER_CELL {
                    bucket.select_nth_unstable_by(FLATS_PER_CELL, flat_area_order);
                    bucket.truncate(FLATS_PER_CELL);
                }
                selected.extend(bucket);
            }
            if selected.len() > LOCAL_FLATS_PER_SLOT {
                selected.select_nth_unstable_by(LOCAL_FLATS_PER_SLOT, flat_area_order);
                selected.truncate(LOCAL_FLATS_PER_SLOT);
            }
            let mut seen: hashbrown::HashSet<_> = selected
                .iter()
                .map(|(flat, _)| flat_selection_key(flat))
                .collect();
            let mut largest = flats.clone();
            largest.select_nth_unstable_by(FLATS_PER_SLOT, flat_area_order);
            largest.truncate(FLATS_PER_SLOT);
            selected.extend(
                largest
                    .into_iter()
                    .filter(|(flat, _)| seen.insert(flat_selection_key(flat))),
            );
            flats = selected;
        }
        flats.sort_by(flat_area_order);
        out.extend(flats.into_iter().map(|(flat, _)| flat));
    }
    out
}

fn flat_area_order(left: &(Flat, f32), right: &(Flat, f32)) -> std::cmp::Ordering {
    right
        .1
        .total_cmp(&left.1)
        .then_with(|| flat_selection_key(&left.0).cmp(&flat_selection_key(&right.0)))
}

fn flat_cell_key(flat: &Flat) -> (i32, i32, i32, i32, i32, i32) {
    let normal = normal_key(flat.normal);
    let center = (flat.lo + flat.hi) * 0.5;
    (
        normal.0,
        normal.1,
        normal.2,
        (center.x / FLAT_CELL_SIZE).floor() as i32,
        (center.y / FLAT_CELL_SIZE).floor() as i32,
        (center.z / FLAT_CELL_SIZE).floor() as i32,
    )
}

fn flat_selection_key(flat: &Flat) -> (i32, i32, i32, i32, i32, i32, i32) {
    let center = (flat.lo + flat.hi) * 0.5;
    (
        (flat.normal.x * 1000.0).round() as i32,
        (flat.normal.y * 1000.0).round() as i32,
        (flat.normal.z * 1000.0).round() as i32,
        (flat.offset / PLANE_GAP).round() as i32,
        (center.x / FLAT_CELL_SIZE).floor() as i32,
        (center.y / FLAT_CELL_SIZE).floor() as i32,
        (center.z / FLAT_CELL_SIZE).floor() as i32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::overlay::ObjectRun;
    use glam::{Mat4, Vec2};

    fn quad(z: f32, half: f32, cut_other_way: bool) -> MeshData {
        let positions = vec![
            Vec3::new(-half, -half, z),
            Vec3::new(half, -half, z),
            Vec3::new(half, half, z),
            Vec3::new(-half, half, z),
        ];
        let indices = if cut_other_way {
            vec![0, 1, 3, 1, 2, 3]
        } else {
            vec![0, 1, 2, 0, 2, 3]
        };
        MeshData {
            wire_wind: Vec::new(),
            positions,
            normals: vec![Vec3::Z; 4],
            uvs: vec![Vec2::ZERO; 4],
            ranges: vec![(0, 6, 0)],
            indices,
            one_sided: false,
        }
    }

    fn moved(mut mesh: MeshData, offset: Vec3) -> MeshData {
        for position in &mut mesh.positions {
            *position += offset;
        }
        mesh
    }

    #[test]
    fn a_small_support_plane_in_a_complex_material_still_orders_a_later_label() {
        let mut positions = Vec::new();
        let mut indices = Vec::new();
        for part in (0..33)
            .map(|i| {
                moved(
                    quad(i as f32 * 0.01 + 1.0, 1.0, false),
                    Vec3::new(10.0 + i as f32 * 3.0, 0.0, 0.0),
                )
            })
            .chain([quad(0.0, 0.2, false)])
        {
            let base = positions.len() as u32;
            positions.extend(part.positions);
            indices.extend(part.indices.into_iter().map(|index| index + base));
        }
        let (vertex_count, index_count) = (positions.len(), indices.len());
        let support = MeshData {
            wire_wind: Vec::new(),
            positions,
            indices,
            normals: vec![Vec3::Z; vertex_count],
            uvs: vec![Vec2::ZERO; vertex_count],
            ranges: vec![(0, index_count as u32, 0)],
            one_sided: false,
        };
        let label = quad(0.0, 0.1, true);
        let mut run = ObjectRun::default();
        run.begin(None);
        run.levels([0; 3], 0, 0, Mat4::IDENTITY, &mesh_flats(&support), 1);
        assert_eq!(
            run.levels([0; 3], 1, 1, Mat4::IDENTITY, &mesh_flats(&label), 1),
            vec![1]
        );
    }

    #[test]
    fn retained_flat_candidates_are_bounded_and_deterministic() {
        let mut positions = Vec::new();
        let mut indices = Vec::new();
        for index in 0..400 {
            let part = quad(index as f32, 0.1, false);
            let base = positions.len() as u32;
            positions.extend(part.positions);
            indices.extend(part.indices.into_iter().map(|index| index + base));
        }
        let vertex_count = positions.len();
        let mesh = MeshData {
            wire_wind: Vec::new(),
            positions,
            ranges: vec![(0, indices.len() as u32, 0)],
            indices,
            normals: vec![Vec3::Z; vertex_count],
            uvs: vec![Vec2::ZERO; vertex_count],
            one_sided: false,
        };

        let flats = mesh_flats(&mesh);
        assert!(flats.len() <= FLATS_PER_SLOT + LOCAL_FLATS_PER_SLOT);
        assert_eq!(flats, mesh_flats(&mesh));
    }
}
