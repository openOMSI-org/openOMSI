//! Re-enter the same retained vehicle, including its own timetable progress.

use crate::App;

#[derive(Default)]
pub(crate) struct SavedDuty {
    pub(super) duty: Option<crate::schedule::PlayerDuty>,
    pub(crate) reservation: Option<omsi_sim::timetable_run::DutyReservation>,
}

impl App {
    pub(super) fn sync_retained_reservations(&mut self) {
        if let Some(s) = self.session.schedule.as_mut() {
            s.set_retained_reservations(
                self.session
                    .placed
                    .iter()
                    .filter_map(|p| p.parked_duty.as_ref()?.reservation.clone())
                    .collect(),
            );
        }
    }

    pub(super) fn take_retained_bus(&mut self, uid: u64) -> Result<(), String> {
        if self.view != "free" || self.cam.ego {
            return Err("Parked buses can only be selected in F4 free view".into());
        }
        if self.net.lan.is_some() {
            return Err("Vehicle handover is only available in single-player".into());
        }
        // Resolve the stable identity again: the placed list may change during confirmation.
        let k = self
            .session
            .placed
            .iter()
            .position(|p| p.uid == uid)
            .ok_or("The selected parked bus is no longer available")?;
        if !super::road_bus(&self.session.placed[k].vehicle.ty) {
            return Err("This parked vehicle cannot be driven by a player".into());
        }
        let saved = self.session.placed[k]
            .parked_duty
            .take()
            .unwrap_or_default();
        if let Some(p) = self.player.as_mut() {
            p.axes.release_all();
            p.release();
            p.parked_duty = Some(SavedDuty {
                duty: self.session.duty.take(),
                reservation: self
                    .session
                    .schedule
                    .as_ref()
                    .and_then(|s| s.duty_reservation()),
            });
        }
        self.take_placed(k);
        self.session.duty = saved.duty;
        if let Some(s) = self.session.schedule.as_mut() {
            s.restore_duty_reservation(saved.reservation);
        }
        self.sync_retained_reservations();
        self.args.line = self.session.duty.as_ref().map(|d| d.line.clone());
        self.args.tour = self.session.duty.as_ref().map(|d| d.tour.clone());
        self.session.duty_places = false;
        self.session.journey = None;
        self.session.safe_pose = None;
        self.session.safe_age = 0.0;
        self.session.pumping = None;
        self.cam.ego = false;
        self.input.dragging = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player() -> crate::player::Player {
        let mut vehicle = crate::schedule::tests::script_test_vehicle("", "", "");
        let ty = std::sync::Arc::make_mut(&mut vehicle.ty);
        ty.def.path = "retained.bus".into();
        ty.def.cameras_driver.push(omsi_vehicle::Camera {
            pos: [0.0, 0.0, 1.8],
            dist: 0.0,
            fov: 60.0,
            yaw: 0.0,
            pitch: 0.0,
            extra: None,
            fixed: false,
        });
        let ty = vehicle.ty.clone();
        let render = crate::scene::VehicleRender {
            window_wipers: None,
            instances: vec![],
            far: vec![],
            own_materials: vec![],
            set: None,
            variants: vec![],
            text_textures: vec![],
            script_textures: vec![],
            shared_script: false,
            displays_far: false,
            display_tick: 0,
            skinned: vec![],
            damaged: vec![],
            glass_cracks: vec![],
            hidden: false,
            interior_lamps: Default::default(),
            interior_blocks: Default::default(),
        };
        crate::spawn::new_player(vehicle, render, vec![], vec![], false, &ty)
    }

    fn app() -> App {
        let args = <crate::cli::Args as clap::Parser>::parse_from(["openomsi"]);
        let mut app = crate::assemble_app(args, Default::default());
        app.view = "free".into();
        app
    }

    fn duty(line: &str) -> crate::schedule::PlayerDuty {
        crate::schedule::PlayerDuty::new(line.into(), "tour".into(), vec![], 0, 0, true)
    }

    #[test]
    fn original_and_taken_bus_can_be_swapped_repeatedly_without_losing_their_duties() {
        let mut app = app();
        let mut original = player();
        let original_id = original.uid;
        original.parked_duty = Some(SavedDuty {
            duty: Some(duty("original")),
            ..Default::default()
        });
        let taken = player();
        let taken_id = taken.uid;
        app.session.placed.push(original);
        app.player = Some(taken);
        app.session.duty = Some(duty("taken"));
        for (uid, line) in [
            (original_id, "original"),
            (taken_id, "taken"),
            (original_id, "original"),
        ] {
            app.view = "free".into();
            app.take_retained_bus(uid).unwrap();
            assert_eq!(app.player.as_ref().unwrap().uid, uid);
            assert_eq!(app.session.placed.len(), 1);
            assert_ne!(app.session.placed[0].uid, uid);
            assert_eq!(app.session.duty.as_ref().unwrap().line, line);
            assert_eq!(app.args.line.as_deref(), Some(line));
            assert_eq!(app.view, "driver");
        }
    }

    #[test]
    fn confirmation_resolves_uid_after_reordering_and_cancels_a_removed_bus() {
        let mut app = app();
        let original = player();
        let uid = original.uid;
        app.session.placed.extend([original, player()]);
        app.session.placed.swap(0, 1);
        app.take_retained_bus(uid).unwrap();
        assert_eq!(app.player.as_ref().unwrap().uid, uid);
        app.view = "free".into();
        assert!(app.take_retained_bus(uid).is_err());
        assert_eq!(app.player.as_ref().unwrap().uid, uid);
        assert_eq!(app.session.placed.len(), 1);
    }

    #[test]
    fn a_bus_without_its_own_duty_does_not_inherit_the_other_bus_timetable() {
        let mut app = app();
        let parked = player();
        let uid = parked.uid;
        app.session.placed.push(parked);
        app.player = Some(player());
        app.session.duty = Some(duty("taken"));
        app.take_retained_bus(uid).unwrap();
        assert!(app.session.duty.is_none());
        assert!(app.args.line.is_none());
        assert_eq!(
            app.session.placed[0]
                .parked_duty
                .as_ref()
                .unwrap()
                .duty
                .as_ref()
                .unwrap()
                .line,
            "taken"
        );
    }

    #[test]
    fn confirmation_rechecks_free_view_before_changing_ownership() {
        let mut app = app();
        let parked = player();
        let uid = parked.uid;
        app.session.placed.push(parked);
        for view in ["driver", "outside", "pax", "foot"] {
            app.view = view.into();
            assert!(app.take_retained_bus(uid).is_err());
            assert!(app.player.is_none());
            assert_eq!(app.session.placed.len(), 1);
        }
        app.view = "free".into();
        app.cam.ego = true;
        assert!(app.take_retained_bus(uid).is_err());
    }
}
