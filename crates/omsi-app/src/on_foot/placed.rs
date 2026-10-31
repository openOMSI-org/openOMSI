//! Taking a retained vehicle through the existing pedestrian and change-vehicle paths.

use super::*;

impl App {
    /// Take the wheel of placed vehicle `k` (the one driven now, if any, stays placed).
    pub(crate) fn take_placed(&mut self, k: usize) {
        if k >= self.session.placed.len() {
            return;
        }
        self.join_sim_thread();
        if SeatCx::of(&mut self.parts()).take_placed(k) {
            self.sync_driven_duty_args();
            self.sync_passenger_ownership();
        }
    }
}

crate::app::stage_ctx!(
    /// Taking the wheel of a placed vehicle: its part of the game.
    SeatCx { reads: [sound], writes: [people, player, ui, view, duty, traffic] }
);

impl SeatCx<'_, '_> {
    fn take_placed(self, k: usize) -> bool {
        if !crate::vehicle_handover::exchange_placed_duty(
            k,
            self.player,
            self.duty,
            self.traffic.schedule.as_mut().map(|s| &mut s.sim),
        ) {
            return false;
        }
        take_placed(
            k,
            self.player,
            self.people,
            self.view,
            self.ui,
            self.sound,
            self.common.settings,
        );
        true
    }
}

/// Take the wheel of placed vehicle `k` (the one driven now, if any, stays placed).
#[allow(clippy::too_many_arguments)]
pub(crate) fn take_placed(
    k: usize,
    player: &mut crate::app::PlayerPart,
    people: &mut crate::app::PeoplePart,
    view: &mut crate::app::ViewPart,
    ui: &mut crate::app::UiPart,
    sound: &crate::app::SoundState,
    settings: &crate::settings::Settings,
) {
    if k >= player.placed.len() {
        return;
    }
    let mut next = player.placed.remove(k);
    if let Some(a) = sound.audio.as_ref() {
        next.load_sounds(a);
    }
    next.vehicle.host.auto_clutch = if settings.auto_clutch { 1.0 } else { 0.0 };
    if let Some(now) = player.player.take() {
        let mut now = now;
        if let (Some(a), Some(mut ss)) = (sound.audio.as_ref(), now.sounds.take()) {
            ss.stop_all(a);
        }
        if let Some(h) = people.humans.as_mut() {
            h.player_bus_swapped(now.uid, next.uid, &mut next.vehicle);
        }
        player.placed.push(now);
    } else if let Some(h) = people.humans.as_mut() {
        // (no bus driven before: whoever rides in this one is the player's bus's now)
        h.player_bus_swapped(0, next.uid, &mut next.vehicle);
    }
    let name = format!(
        "{} {}",
        next.vehicle.ty.def.manufacturer, next.vehicle.ty.def.type_name
    );
    *player.player = Some(next);
    if let Some(f) = people.on_foot.take() {
        if let Some(h) = people.humans.as_mut() {
            h.avatar_remove(AVATAR_KEY);
        }
        let _ = f;
    }
    *view.view = "driver".into();
    crate::app_impl::sync_view_look(view.cam, view.view, player.player.as_ref());
    view.cam.look = (0.0, 0.0);
    if let (Some(cam), Some(p)) = (view.camera.as_ref(), player.player.as_ref()) {
        *view.camera = Some(p.camera("driver", cam));
    }
    *ui.service_msg = Some((
        omsi_ui::i18n::tr_fill("Now driving: {name}", &[("{name}", &name.trim())]),
        4.0,
    ));
}
