use super::{Flat, ObjectRun, flats::FLATS_PER_SLOT, normal_key};
use crate::AlphaMode;
use std::sync::Arc;

impl ObjectRun {
    /// One blended material can repeat several stepped surfaces in the same mesh, such as
    /// the cover over two angled camera monitors. Give it a step only if every plane exactly
    /// matches a stepped surface. Slots that may have discarded planes are ineligible.
    pub(super) fn repeats_stepped_planes(
        &self,
        flats: &[&Flat],
        first_flat: usize,
        alpha: Option<&AlphaMode>,
    ) -> bool {
        alpha == Some(&AlphaMode::Blend)
            && (2..FLATS_PER_SLOT).contains(&flats.len())
            && flats.iter().all(|&&flat| {
                self.flats[first_flat..].iter().any(|&(base, level)| {
                    level > 0
                        && Flat {
                            slot: base.slot,
                            ..flat
                        } == base
                })
            })
    }

    pub(super) fn remember_unstepped_flats(&mut self, flats: &[&Flat], mesh: usize) {
        for flat in flats {
            self.by_normal
                .entry(normal_key(flat.normal))
                .or_default()
                .push(self.flats.len());
            self.flats.push((**flat, 0));
            self.flat_caps.push(Arc::from([]));
            self.flat_meshes.push(mesh);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Mat4, Vec3};

    #[test]
    fn blended_cover_can_step_over_both_angled_screens() {
        let left = Flat {
            slot: 0,
            normal: Vec3::Z,
            orientation: 1.0,
            offset: 0.0,
            lo: Vec3::ZERO,
            hi: Vec3::new(1.0, 1.0, 0.0),
            axis: Vec3::Y,
            half: [0.5; 2],
        };
        let right = Flat {
            slot: 1,
            normal: Vec3::X,
            offset: 2.0,
            lo: Vec3::new(2.0, 0.0, 0.0),
            hi: Vec3::new(2.0, 1.0, 1.0),
            ..left
        };
        for (cover, alpha, expected) in [
            (right, AlphaMode::Blend, 2),
            (right, AlphaMode::Opaque, 0),
            (
                Flat {
                    offset: 2.01,
                    lo: right.lo + Vec3::X * 0.01,
                    hi: right.hi + Vec3::X * 0.01,
                    ..right
                },
                AlphaMode::Blend,
                0,
            ),
        ] {
            let flats = [
                left,
                right,
                Flat { slot: 2, ..left },
                Flat { slot: 3, ..right },
                Flat { slot: 4, ..left },
                Flat { slot: 4, ..cover },
            ];
            let mut materials = [AlphaMode::Opaque; 5];
            materials[4] = alpha;
            let mut run = ObjectRun::default();
            run.begin(None);
            let (levels, _, _) = run.levels_for_materials_with_depth_planes(
                [0; 3],
                0,
                0,
                Mat4::IDENTITY,
                &flats,
                5,
                &materials,
                &[Vec3::Z, Vec3::X, Vec3::Z, Vec3::X, Vec3::ZERO],
            );
            assert_eq!(levels, [0, 0, 1, 1, expected]);
        }
    }
}
