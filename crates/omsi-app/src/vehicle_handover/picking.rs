//! F4 selects the first visible vehicle body, never a bus through another vehicle.

use crate::App;
use glam::DVec3;
use omsi_sim::ai_traffic::model::vehicle_bodies;
use omsi_sim::collision::Obb;

fn ray_box(origin: DVec3, direction: DVec3, b: &Obb) -> Option<f64> {
    let [right, forward] = b.axes();
    let relative = origin.truncate() - b.center;
    let o = [relative.dot(right), relative.dot(forward), origin.z];
    let d = [
        direction.truncate().dot(right),
        direction.truncate().dot(forward),
        direction.z,
    ];
    let lo = [-b.half.x, -b.half.y, b.z0];
    let hi = [b.half.x, b.half.y, b.z1];
    let (mut near, mut far) = (0.0f64, 2000.0f64);
    for i in 0..3 {
        if d[i].abs() < 1e-9 {
            if o[i] < lo[i] || o[i] > hi[i] {
                return None;
            }
        } else {
            let (a, z) = ((lo[i] - o[i]) / d[i], (hi[i] - o[i]) / d[i]);
            near = near.max(a.min(z));
            far = far.min(a.max(z));
            if near > far {
                return None;
            }
        }
    }
    Some(near)
}

impl App {
    pub(crate) fn pick_ai_bus(&mut self) -> bool {
        use winit::keyboard::KeyCode;
        if [
            KeyCode::ControlLeft,
            KeyCode::ControlRight,
            KeyCode::ShiftLeft,
            KeyCode::ShiftRight,
            KeyCode::AltLeft,
            KeyCode::AltRight,
        ]
        .iter()
        .any(|key| self.input.keys.contains(key))
        {
            return false;
        }
        if self.view != "free"
            || self.cam.ego
            || self.net.lan.is_some()
            || self.menus.game_menu.is_some()
            || self.menus.chooser.is_some()
            || self.menus.game_panels.any()
            || self.photo.is_some()
            || self.menus.placing.is_some()
        {
            return false;
        }
        let Some((origin, direction, _)) = self.cursor_ray_now() else {
            return false;
        };
        let direction = direction.as_dvec3();
        let Some(traffic) = self.session.traffic.as_ref() else {
            return false;
        };
        let hits = traffic
            .cars
            .iter()
            .filter_map(|c| {
                let distance = vehicle_bodies(&c.vehicle)
                    .iter()
                    .filter_map(|b| ray_box(origin, direction, b))
                    .min_by(f64::total_cmp)?;
                Some((
                    Some(super::selection::Target::Ai(c.id)),
                    distance,
                    super::road_bus(&c.vehicle.ty),
                ))
            })
            .chain(self.session.placed.iter().filter_map(|p| {
                let distance = vehicle_bodies(&p.vehicle)
                    .iter()
                    .filter_map(|b| ray_box(origin, direction, b))
                    .min_by(f64::total_cmp)?;
                Some((
                    Some(super::selection::Target::Placed(p.uid)),
                    distance,
                    super::road_bus(&p.vehicle.ty),
                ))
            }))
            .chain(self.player.iter().filter_map(|p| {
                let distance = vehicle_bodies(&p.vehicle)
                    .iter()
                    .filter_map(|b| ray_box(origin, direction, b))
                    .min_by(f64::total_cmp)?;
                Some((None, distance, false))
            }));
        let Some((target, distance)) = super::selection::nearest(hits) else {
            return false;
        };
        let Some(world) = self.world.clone() else {
            return false;
        };
        let end = origin + direction * (distance - 0.1).max(0.0);
        if world
            .data
            .peek()
            .lists
            .collision
            .ray_blocked(origin, end, 0.0, 0.0)
        {
            return false;
        }
        if crate::placing::ground_hit(&world, origin, direction, distance).is_some() {
            return false;
        }
        self.menus.game_panels.handover = Some(super::Request {
            target: Some(target),
            world,
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_ray_hits_the_nearest_face_and_respects_height() {
        let b = Obb::from_box(
            [2.5, 12.0, 3.0, 0.0, 0.0, 1.5],
            DVec3::new(0.0, 20.0, 0.0),
            0.0,
        );
        assert_eq!(ray_box(DVec3::new(0.0, 0.0, 1.0), DVec3::Y, &b), Some(14.0));
        assert!(ray_box(DVec3::new(0.0, 0.0, 5.0), DVec3::Y, &b).is_none());
        assert!(ray_box(DVec3::new(0.0, 0.0, 1.0), -DVec3::Y, &b).is_none());
    }

    #[test]
    fn takeover_is_not_selected_outside_f4() {
        let args = <crate::cli::Args as clap::Parser>::parse_from(["openomsi"]);
        let mut app = crate::assemble_app(args, Default::default());
        for view in ["driver", "pax", "outside", "foot"] {
            app.view = view.into();
            assert!(!app.pick_ai_bus());
            assert!(app.menus.game_panels.handover.is_none());
        }
    }
}
