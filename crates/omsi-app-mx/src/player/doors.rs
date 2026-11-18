//! The bus's doors: which keys open which leaves, and the triggers the scripts' doors answer.

use super::*;

pub(crate) use omsi_sim::door_keys::{door_keys, door_trigger_target, trigger_leaves};

/// Which triggers of a door key's group to fire so that its leaves end up together: when
/// any is open (by its target), only the open ones (to close them), else all. Toggling
/// every leaf of a group made a closed leaf open while an open one closed.
pub(crate) fn door_group_to_fire(v: &mut omsi_sim::VehicleInstance, group: &[String]) -> Vec<String> {
    let fire = door_group_plan(v, group);
    if fire.len() < 2 {
        return fire;
    }
    // Triggers that undo each other are an open and a close key, not two leaves: Road-hog123's
    // door script (the UK buses' - the London Citybus 400, the Enviro400s) opens both leaves
    // on `bus_doorfront0` and closes both on `bus_doorfront1`, and firing the pair left the
    // doors shut: the passengers queued at the closed door for good. Tried on the scripts
    // first (the vehicle left as it was): when the doors' targets end where they began, only
    // the trigger that moves them now is fired.
    let mut targets: Vec<omsi_script::VarId> = group.iter().filter_map(|n| door_trigger_target(&v.ty.program, n.split('|').next().unwrap_or(n))).collect();
    targets.sort_unstable();
    targets.dedup();
    if targets.is_empty() {
        return fire;
    }
    let at = |vars: &[f32]| -> Vec<bool> { targets.iter().map(|&t| vars.get(t as usize).is_some_and(|x| *x > 0.5)).collect() };
    let base = at(&v.state.vars);
    let names: Vec<&str> = fire.iter().map(|s| s.as_str()).collect();
    if at(&v.trial_triggers(&names)) != base {
        return fire;
    }
    match fire.iter().find(|n| at(&v.trial_triggers(&[n.as_str()])) != base) {
        Some(one) => vec![one.clone()],
        None => fire,
    }
}

/// Whether the leaf door trigger `name` works is open now: what the trigger does to the
/// doors' targets when tried (a target it turns down was up), else the target its script
/// names first. The first one named can be a branch not taken: the SD202's
/// `bus_doorfront1` names the rear doors' target (`doorTarget_23`, for a bus whose rear
/// door the front buttons work) before its own leaf's, so with both front leaves open
/// the second Shift+1 shut only one of them.
pub(crate) fn door_trigger_open(v: &mut omsi_sim::VehicleInstance, name: &str) -> Option<bool> {
    let targets: Vec<usize> = v
        .ty
        .program
        .var_names
        .iter()
        .enumerate()
        .filter(|(_, n)| {
            let n = n.to_ascii_lowercase();
            n.contains("target") || n.contains("soll")
        })
        .map(|(k, _)| k)
        .collect();
    if !targets.is_empty() {
        let tried = v.trial_triggers(&[name]);
        let now = &v.state.vars;
        let changed = targets.iter().find(|&&k| (tried.get(k).copied().unwrap_or(0.0) > 0.5) != (now.get(k).copied().unwrap_or(0.0) > 0.5));
        if let Some(&k) = changed {
            return Some(now.get(k).copied().unwrap_or(0.0) > 0.5);
        }
    }
    door_trigger_target(&v.ty.program, name).and_then(|id| v.state.vars.get(id as usize).copied()).map(|x| x > 0.5)
}

/// Which triggers of a door key's group the doors' targets ask for (see
/// `door_group_to_fire`).
fn door_group_plan(v: &mut omsi_sim::VehicleInstance, group: &[String]) -> Vec<String> {
    // an open and close pair (`open|close`): whichever fits the leaf now
    let group: Vec<String> = group
        .iter()
        .map(|n| match n.split_once('|') {
            Some((open, close)) => {
                let leaf = trigger_leaves(&v.ty.program, open);
                let is_open = leaf.iter().any(|l| v.var(l).is_some_and(|x| x > 0.5));
                if is_open { close.to_string() } else { open.to_string() }
            }
            None => n.clone(),
        })
        .collect();
    let group = &group[..];
    let states: Vec<Option<bool>> = group
        .iter()
        .map(|n| door_trigger_open(v, n))
        .collect();
    if group.len() < 2 || states.iter().any(|s| s.is_none()) {
        return group.to_vec();
    }
    let any_open = states.iter().any(|s| *s == Some(true));
    group.iter().zip(&states).filter(|(_, s)| !any_open || **s == Some(true)).map(|(n, _)| n.clone()).collect()
}

/// `Digit1`..`Digit9` as 1..9 (`Digit0` and the numpad digits are left for whatever
/// `Inputs/keyboard.cfg` already puts on them).
pub(crate) fn digit_of(code: KeyCode) -> Option<usize> {
    Some(match code {
        KeyCode::Digit1 => 1,
        KeyCode::Digit2 => 2,
        KeyCode::Digit3 => 3,
        KeyCode::Digit4 => 4,
        KeyCode::Digit5 => 5,
        KeyCode::Digit6 => 6,
        KeyCode::Digit7 => 7,
        KeyCode::Digit8 => 8,
        KeyCode::Digit9 => 9,
        _ => return None,
    })
}

// (the game's own door actions by name live with the controllers: `omsi-input`)
pub(crate) use omsi_input::actions::door_action;
