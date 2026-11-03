use super::*;

pub(super) fn clock_action(app: &mut App, verb: &str, arg: &str) {
    match verb {
        // the exact time: Enter starts typing it, and sets it when typed
        "time_edit" => {
            if app.menus.menu_edit.is_some() {
                app.apply_time_edit();
            } else if app.net.lan.as_ref().is_some_and(|l| l.role == omsi_net::Role::Client) {
                app.service_msg = Some(("In a LAN session the host sets the clock".into(), 3.0));
            } else if app.real_time_locked() {
                app.service_msg = Some(("The time cannot be changed while the real-time sync is on".into(), 3.0));
            } else {
                app.menus.menu_edit = Some(String::new());
            }
        }
        "clock_ontime" => {
            if app.net.lan.as_ref().is_some_and(|l| l.role == omsi_net::Role::Client) {
                app.service_msg = Some(("In a LAN session the host sets the clock".into(), 3.0));
            } else if let Some(d) = app.player.as_ref().map(|p| p.vehicle.host.tt_delay as f64).filter(|d| d.abs() >= 1.0) {
                // (the delay as it is now, not as the button was drawn: a second click
                // would otherwise move the clock by the old amount again)
                app.shift_clock(-d);
                if let Some(p) = app.player.as_mut() {
                    p.vehicle.host.tt_delay = 0.0;
                }
            }
        }
        "clock_set" | "clock_shift" => {
            if app.net.lan.as_ref().is_some_and(|l| l.role == omsi_net::Role::Client) {
                app.service_msg = Some(("In a LAN session the host sets the clock".into(), 3.0));
            } else if let Ok(secs) = arg.trim().parse::<f64>() {
                let by = if verb == "clock_set" { secs - app.clock.time } else { secs };
                app.shift_clock(by);
            }
        }
        _ => {}
    }
}
