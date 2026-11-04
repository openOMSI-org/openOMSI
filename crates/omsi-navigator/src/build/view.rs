//! Ground coverage of a perspective map, including space to move before rebuilding.

use glam::{Mat4, Vec2, Vec3};

/// The radius about the anchor enclosing all four ground corners. A circular bound also
/// covers arbitrary map headings without rebuilding on every camera rotation.
pub(crate) fn ground_reach(vp: Mat4) -> Option<f64> {
    let inv = vp.inverse();
    if !inv.is_finite() {
        return None;
    }
    let mut reach = 0.0f64;
    for x in [-1.0, 1.0] {
        for y in [-1.0, 1.0] {
            let ndc = Vec2::new(x, y);
            let a = inv.project_point3(ndc.extend(0.0));
            // A middle depth avoids loss of precision at a distant far plane.
            let b = inv.project_point3(ndc.extend(0.5));
            let d = b - a;
            if !a.is_finite() || !d.is_finite() || d.z.abs() < 1e-6 {
                return None;
            }
            let k = -a.z / d.z;
            if k <= 0.0 || !k.is_finite() {
                return None;
            }
            reach = reach.max((a + d * k).truncate().length() as f64);
        }
    }
    (reach > 0.0 && reach.is_finite()).then_some(reach)
}

pub(crate) fn padded(reach: f64) -> f64 {
    reach * 1.5 + 16.0
}

pub(crate) fn panel_reach(zoom: f64, w: f32, h: f32) -> Option<f64> {
    let pitch = super::super::PITCH.to_radians();
    let target = Vec3::new(0.0, (zoom * 0.17) as f32, 0.0);
    let eye = target
        + Vec3::new(
            0.0,
            (-zoom * pitch.cos()) as f32,
            (zoom * pitch.sin()) as f32,
        );
    let view = Mat4::look_at_rh(eye, target, Vec3::Z);
    let proj = Mat4::perspective_rh(super::super::FOV.to_radians(), w / h.max(1.0), 1.0, 20000.0);
    ground_reach(proj * view)
}

/// Cache enough ground for the entire automatic zoom range and the original driving interval.
pub(crate) fn panel_build_reach(w: f32, h: f32) -> Option<f64> {
    panel_reach(super::super::MAX_ZOOM, w, h).map(|r| r + 150.0)
}

pub(crate) fn panel_moved(built: f64, visible: f64, distance: f64) -> bool {
    distance > 150.0 || distance + visible > built
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Navigator;

    #[test]
    fn city_coverage_contains_rotated_and_tilted_view_corners_with_movement_margin() {
        for (w, h) in [(1650.0, 928.0), (500.0, 1200.0)] {
            for mpp in [0.1, 0.6, 2.5] {
                for tilt in [0.0, 0.5, 1.0] {
                    let cam = Navigator::city_cam(mpp, tilt, 0.0, w, h);
                    let corners = [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)];
                    let reach = ground_reach(cam.vp).unwrap();
                    let built = padded(reach);
                    for phi in [0.0, 0.7, 1.9, 3.14] {
                        let cam = Navigator::city_cam(mpp, tilt, phi, w, h);
                        for (x, y) in corners {
                            let at = Navigator::unproject(&cam, w, h, x, y).unwrap();
                            assert!(
                                at.length() + reach * 0.4 < built,
                                "camera rotation and movement must stay covered"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn panel_coverage_keeps_all_pitch_modes_and_drops_the_fixed_640_metre_minimum() {
        for (w, h) in [(400.0, 300.0), (1000.0, 250.0), (200.0, 800.0)] {
            let reach = panel_reach(120.0, w, h).unwrap();
            for pitch in [52.0f32, 60.0, 90.0] {
                let target = Vec3::new(0.0, 120.0 * 0.17, 0.0);
                let eye = target
                    + Vec3::new(
                        0.0,
                        -120.0 * pitch.to_radians().cos(),
                        120.0 * pitch.to_radians().sin(),
                    );
                let up = if pitch == 90.0 { Vec3::Y } else { Vec3::Z };
                let vp = Mat4::perspective_rh(40f32.to_radians(), w / h, 1.0, 20000.0)
                    * Mat4::look_at_rh(eye, target, up);
                assert!(ground_reach(vp).unwrap() <= reach * 1.001);
            }
        }
        assert!(padded(panel_reach(120.0, 400.0, 300.0).unwrap()) < 300.0);
        assert!(ground_reach(Mat4::ZERO).is_none());
    }
    #[test]
    fn stop_start_zoom_cycles_keep_the_original_150_metre_rebuild_interval() {
        for (w, h) in [(400.0, 300.0), (1000.0, 250.0), (200.0, 800.0)] {
            let built = panel_build_reach(w, h).unwrap();
            for cycle in 0..4 {
                for zoom in (150..=320).chain((150..320).rev()) {
                    let visible = panel_reach(zoom as f64, w, h).unwrap();
                    assert!(
                        !panel_moved(built, visible, 149.0),
                        "stop/start cycle {cycle}, zoom {zoom}"
                    );
                    assert!(panel_moved(built, visible, 151.0));
                }
            }
            let smaller = panel_build_reach(w * 0.5, h).unwrap();
            assert!(
                panel_moved(smaller, panel_reach(320.0, w * 100.0, h).unwrap(), 0.0),
                "resize must rebuild uncovered ground"
            );
        }
    }
}
