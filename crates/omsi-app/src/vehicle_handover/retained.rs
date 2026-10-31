//! Re-enter the same retained vehicle, including its own timetable progress.

use crate::App;

#[derive(Default)]
pub(crate) struct SavedDuty {
    pub(super) duty: Option<crate::schedule::PlayerDuty>,
    pub(crate) reservation: Option<omsi_sim::timetable_run::DutyReservation>,
}

/// Exchange the duty before any entry point moves the corresponding vehicle bodies.
/// Reloading/replacing the driven vehicle deliberately bypasses this ownership change.
pub(crate) fn exchange_placed_duty(
    k: usize,
    player: &mut crate::app::PlayerPart,
    duty: &mut crate::app::DutyPart,
    mut schedule: Option<&mut omsi_sim::timetable_run::ScheduleSim>,
) -> bool {
    let Some(next) = player.placed.get_mut(k) else {
        return false;
    };
    let saved = next.parked_duty.take().unwrap_or_default();
    if let Some(now) = player.player.as_mut() {
        now.axes.release_all();
        now.release();
        now.parked_duty = Some(SavedDuty {
            duty: duty.duty.take(),
            reservation: schedule.as_ref().and_then(|s| s.duty_reservation()),
        });
    }
    *duty.duty = saved.duty;
    if let Some(schedule) = schedule.as_mut() {
        schedule.restore_duty_reservation(saved.reservation);
        // The target's snapshot was taken above; the outgoing bus is not placed yet.
        schedule.set_retained_reservations(
            player
                .placed
                .iter()
                .chain(player.player.iter())
                .filter_map(|p| p.parked_duty.as_ref()?.reservation.clone())
                .collect(),
        );
    }
    *duty.duty_places = false;
    *duty.journey = None;
    *player.safe_pose = None;
    *player.safe_age = 0.0;
    *player.pumping = None;
    true
}

impl App {
    pub(crate) fn sync_driven_duty_args(&mut self) {
        self.args.line = self.session.duty.as_ref().map(|d| d.line.clone());
        self.args.tour = self.session.duty.as_ref().map(|d| d.tour.clone());
    }

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
        self.take_placed(k);
        self.cam.ego = false;
        self.input.dragging = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
