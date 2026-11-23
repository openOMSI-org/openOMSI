//! Foot support in the same frame as PoseInput::origin; never move simulation positions.
use crate::scene::World;
use glam::{DVec2, DVec3, Vec3};
use omsi_sim::people::cabin::{BusNow, Cabin};

pub(super) fn sample(world: &World, bus: Option<&BusNow>, at: DVec2, level: f64) -> Option<f64> {
    let Some(bus) = bus else {
        return world.walk_height_near(at.x, at.y, level);
    };
    if at.x.abs() > bus.half.x - 0.05 {
        // Intersect along cabin-local z, keeping the foot's local x/y fixed. An inverse
        // world-vertical projection changes x/y on a tilted floor and is not this support.
        return ground_in_frame(
            level,
            |z| bus.world(Vec3::new(at.x as f32, at.y as f32, z as f32)),
            |p| world.walk_height_near(p.x, p.y, p.z),
        );
    }
    cabin_floor(&bus.cabin, at, level)
}

fn ground_in_frame(
    level: f64,
    project: impl Fn(f64) -> DVec3,
    ground: impl Fn(DVec3) -> Option<f64>,
) -> Option<f64> {
    let base = project(level);
    // world() includes the articulated section/blend at this fixed local x/y.
    let dz = (project(level + 1.0) - base).z;
    if !dz.is_finite() || dz <= 1e-5 {
        return None;
    }
    let mut z = level;
    // Flat ground converges immediately; repeat at the changed world x/y for a ramp.
    // Bounded work, only for supports beside doors, never a whole-mesh scan.
    for _ in 0..4 {
        let p = project(z);
        let error = ground(p)? - p.z;
        if !error.is_finite() {
            return None;
        }
        z += error / dz;
        if error.abs() < 1e-5 {
            break;
        }
    }
    Some(z)
}

fn cabin_floor(cabin: &Cabin, at: DVec2, level: f64) -> Option<f64> {
    support(
        &cabin.graph.points,
        &cabin.links,
        cabin.seats.iter().map(|s| s.floor),
        at,
        level,
    )
}

// Restore the earlier procedural sampler: interpolate path links (stairs/ramps),
// include place floors, and prefer the nearby deck instead of an upper landing.
fn support(
    points: &[Vec3],
    links: &[(i32, i32, bool)],
    floors: impl Iterator<Item = Vec3>,
    at: DVec2,
    level: f64,
) -> Option<f64> {
    let mut best = None;
    let mut consider = |xy: DVec2, z: f64| {
        let dz = (z - level).abs();
        if !z.is_finite() || dz >= 1.0 {
            return;
        }
        let score = (xy - at).length() + 0.5 * dz;
        if best.is_none_or(|(s, _)| score < s) {
            best = Some((score, z));
        }
    };
    let point = |i| usize::try_from(i).ok().and_then(|i| points.get(i));
    for &(a, b, _) in links {
        let (Some(a), Some(b)) = (point(a), point(b)) else {
            continue;
        };
        let axy = a.truncate().as_dvec2();
        let ab = (b - a).truncate().as_dvec2();
        let t = if ab.length_squared() > 1e-8 {
            ((at - axy).dot(ab) / ab.length_squared()).clamp(0.0, 1.0)
        } else {
            0.0
        };
        consider(axy + ab * t, a.z as f64 + (b.z - a.z) as f64 * t);
    }
    for p in points.iter().copied().chain(floors) {
        consider(p.truncate().as_dvec2(), p.z as f64);
    }
    best.map(|(_, z)| z)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tilted_cabin_support_keeps_local_xy_on_flat_and_sloped_pavement() {
        let rot = glam::DMat3::from_rotation_y(0.35) * glam::DMat3::from_rotation_x(-0.15);
        let project = |z| DVec3::new(10000.0, -20000.0, 0.8) + rot * DVec3::new(1.4, -6.0, z);
        for slope in [0.0, 0.08] {
            let ground = |p: DVec3| Some(0.25 + slope * (p.x - 10000.0));
            let z = ground_in_frame(0.45, project, ground).unwrap();
            let p = project(z);
            assert!((p.z - ground(p).unwrap()).abs() < 1e-5);
            let local = rot.inverse() * (p - DVec3::new(10000.0, -20000.0, 0.8));
            assert!(local.truncate().abs_diff_eq(DVec2::new(1.4, -6.0), 1e-8));
        }
        assert_eq!(ground_in_frame(0.0, |_| DVec3::ZERO, |_| Some(0.0)), None);
    }
    #[test]
    fn stairs_interpolate_and_decks_do_not_cross() {
        let p = [
            Vec3::new(0.0, 0.0, 0.4),
            Vec3::new(0.0, 1.0, 0.8),
            Vec3::new(0.0, 0.5, 2.3),
        ];
        assert!(
            (support(
                &p,
                &[(0, 1, false)],
                std::iter::empty(),
                DVec2::new(0.0, 0.5),
                0.6
            )
            .unwrap()
                - 0.6)
                .abs()
                < 1e-6
        );
        assert_eq!(
            support(&p, &[], std::iter::empty(), DVec2::new(0.0, 0.5), 2.3),
            Some(p[2].z as f64)
        );
        assert_eq!(
            support(
                &[],
                &[(-1, 99, false)],
                std::iter::empty(),
                DVec2::ZERO,
                0.0
            ),
            None
        );
    }
    #[test]
    fn seat_platform_is_a_floor_without_path_links() {
        assert_eq!(
            support(
                &[],
                &[],
                [Vec3::new(0.0, 0.0, 0.7)].into_iter(),
                DVec2::ZERO,
                0.7
            ),
            Some(0.7_f32 as f64)
        );
    }
}
