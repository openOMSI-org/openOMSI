//! F4 selects the first visible vehicle body, never a bus through another vehicle.

use crate::App;
use omsi_sim::ai_traffic::model::vehicle_bodies;
use omsi_sim::vehicle_transfer::picking::ray_box;


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
