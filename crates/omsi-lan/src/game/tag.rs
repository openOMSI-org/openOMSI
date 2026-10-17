//! What another player's name tag says (the game draws it where the player is, `name_tags`).

use super::RemoteVehicle;

/// The name tag of player `r`, `d` m away: their name, and the small line under it - line and
/// destination, how far away (beyond 25 m), what stands in for their bus, "radio" or
/// "speaking". `tr` translates into the game's language, `distance` writes a distance in the
/// player's units.
pub fn tag_text(r: &RemoteVehicle, d: f64, radio: bool, speaking: bool, tr: &dyn Fn(&str) -> String, distance: &dyn Fn(f64) -> String) -> (String, String) {
    let name = if r.name.trim().is_empty() {
        format!("player {}", r.last.id)
    } else {
        r.name.trim().to_string()
    };
    let pose = &r.last;
    let mut sub = match (pose.line.trim().is_empty(), pose.destination.trim().is_empty()) {
        (false, false) => format!("{} {}", pose.line.trim(), pose.destination.trim()),
        (true, false) => pose.destination.trim().to_string(),
        (false, true) => format!("line {}", pose.line.trim()),
        _ => String::new(),
    };
    if d > 25.0 {
        let dist = distance(d);
        sub = if sub.is_empty() { dist } else { format!("{sub} | {dist}") };
    }
    // (a box standing for their bus says how far the bus has come; another bus standing
    // for it says it is another)
    if r.stand_in || r.fallback {
        let state = tr(&super::placeholder_state(r));
        sub = if sub.is_empty() { state } else { format!("{state} | {sub}") };
    }
    if radio {
        sub = if sub.is_empty() { tr("radio") } else { format!("{} | {sub}", tr("radio")) };
    } else if speaking {
        sub = if sub.is_empty() { tr("speaking") } else { format!("{} | {sub}", tr("speaking")) };
    }
    (name, sub)
}
