//! Finding a handed-over bus changes only the F4 camera, never its ownership.

use crate::App;
use glam::DVec3;
use omsi_ui::tr;
use omsi_navigator::handed_over_camera_pose;

impl App {
    pub(crate) fn choose_handed_over_bus(
        &mut self,
        action: &str,
    ) -> Option<crate::game_lists::ListKind> {
        let id = action.strip_prefix("find_ai ")?.parse::<u64>().ok()?;
        (!self.find_handed_over_bus(id)).then_some(crate::game_lists::ListKind::HandedOverBuses)
    }

    pub(crate) fn handed_over_items(&self) -> Vec<(String, String)> {
        self.session
            .handed_over
            .entries()
            .map(|(id, name)| {
                let exists = self.handed_over_pose(id).is_some();
                let state = tr(if exists { "In traffic" } else { "Left traffic" });
                (format!("#{id} {name} | {state}"), format!("find_ai {id}"))
            })
            .collect()
    }

    fn handed_over_pose(&self, id: u64) -> Option<(DVec3, f64)> {
        if !self.session.handed_over.contains(id) {
            return None;
        }
        super::tracking::locate(
            id,
            self.session
                .traffic
                .as_ref()?
                .cars
                .iter()
                .map(|car| (car.id, (car.vehicle.position, car.vehicle.heading))),
        )
    }

    pub(crate) fn open_handed_over_buses(&mut self) {
        self.join_sim_thread();
        if self.net.lan.is_some() {
            self.service_msg = Some((
                tr("Vehicle handover is only available in single-player").into_owned(),
                5.0,
            ));
        } else if self.session.handed_over.is_empty() {
            self.service_msg = Some((
                tr("No buses have been handed to AI in this session").into_owned(),
                5.0,
            ));
        } else {
            self.open_list(crate::game_lists::ListKind::HandedOverBuses);
        }
    }

    pub(crate) fn find_handed_over_bus(&mut self, id: u64) -> bool {
        self.join_sim_thread();
        if self.net.lan.is_some() {
            return false;
        }
        let Some((position, heading)) = self
            .handed_over_pose(id)
            .and_then(|(p, h)| handed_over_camera_pose(p, h))
        else {
            self.service_msg = Some((
                tr("This bus has left traffic and can no longer be located").into_owned(),
                5.0,
            ));
            return false;
        };
        let Some(camera) = self.camera.as_mut() else {
            self.service_msg = Some((tr("The camera is not ready").into_owned(), 5.0));
            return false;
        };
        camera.position = position;
        camera.yaw = heading;
        camera.pitch = -45.0;
        self.view = "free".into();
        self.cam.ego = false;
        self.input.dragging = false;
        self.close_game_menu();
        self.service_msg = Some((
            tr("Located the bus. Click it in F4 free view to take over").into_owned(),
            7.0,
        ));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_bus_is_reported_without_changing_player_or_camera_mode() {
        let args = <crate::cli::Args as clap::Parser>::parse_from(["openomsi"]);
        let mut app = crate::assemble_app(args, Default::default());
        app.session.handed_over.record(7, "Test bus".into());
        let view = app.view.clone();
        let ego = app.cam.ego;
        assert_eq!(app.handed_over_items().len(), 1);
        assert!(!app.find_handed_over_bus(7));
        assert_eq!(app.view, view);
        assert_eq!(app.cam.ego, ego);
        assert!(app.player.is_none());
        assert!(app.service_msg.is_some());
    }
}
