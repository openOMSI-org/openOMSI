//! Taking the wheel: of a placed vehicle, of the own bus, of another cab of the own train.

use super::AVATAR_KEY;

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

/// The driver of `p` takes the wheel in the cab of coupled part `cab` (None: the leading
/// vehicle's): its driver's camera is the one chosen (its `[set_camera_std]`) and the keys
/// work its scripts (`VehicleInstance::active_cab`). Back in the leading vehicle from another
/// cab, its standard camera again.
pub(crate) fn seat_in_cab(p: &mut crate::player::Player, cab: Option<usize>) {
    if cab.is_some_and(|k| p.vehicle.cab_seat(k).is_none()) {
        return;
    }
    let was = p.vehicle.active_cab;
    p.vehicle.active_cab = cab;
    let def = &p.vehicle.ty.def;
    match cab.map(|k| p.vehicle.cab_camera_slot(k)) {
        Some(Some(slot)) => {
            let total = p.driver_camera_count().max(1);
            // (the choice counts from the front's standard camera, see `driver_camera_slot`)
            p.cam_choice.0 =
                (slot + total - def.camera_std % def.cameras_driver.len().max(1)) % total;
        }
        Some(None) => {}
        None if was.is_some() => p.cam_choice.0 = 0,
        None => {}
    }
}

/// From the outside view, the driver goes to the train's next driving end (the one before,
/// `back`); what it is, for the screen (None: the train has one driving end).
pub(crate) fn change_driving_end(p: &mut crate::player::Player, back: bool) -> Option<String> {
    let (next, number, n) = p.vehicle.next_driving_end(back);
    if n < 2 {
        return None;
    }
    seat_in_cab(p, next);
    Some(omsi_ui::i18n::tr_fill(
        "Driving end {number} of {count}: {name}",
        &[
            ("{number}", &number),
            ("{count}", &n),
            ("{name}", &p.vehicle.part_name(next)),
        ],
    ))
}

/// Back at the wheel of the own bus.
pub(crate) fn sit_at_the_wheel(
    people: &mut crate::app::PeoplePart,
    view: &mut crate::app::ViewPart,
    ui: &mut crate::app::UiPart,
    player: &crate::app::PlayerPart,
) {
    let Some(f) = people.on_foot.take() else {
        return;
    };
    if let Some(h) = people.humans.as_mut() {
        h.avatar_remove(AVATAR_KEY);
    }
    *view.view = if f.view_before == "outside" || f.view_before == "driver" {
        f.view_before
    } else {
        "driver".into()
    };
    // (the eyes glide from where the walker's were into the cab camera)
    if *view.view == "driver" {
        view.cam.cam_blend.entering = true;
    }
    crate::app_impl::sync_view_look(view.cam, view.view, player.player.as_ref());
    view.cam.look = (0.0, 0.0);
    // (the walking keys' help goes with the walking: it stood over the cab view)
    if ui
        .service_msg
        .as_ref()
        .is_some_and(|m| m.0.starts_with("On foot"))
    {
        *ui.service_msg = None;
    }
}
