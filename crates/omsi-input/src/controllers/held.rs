use super::{find_device_cfg, latch_release, names_match, DeviceCfg};
use crate::actions::is_game_action;

/// A press that has not chosen short or long yet. Kept off `Held`, so a button that fires
/// on the way down does not carry a timer.
struct Pending {
    device: String,
    button: usize,
    at: std::time::Instant,
    ms: u32,
    long: String,
    short: Vec<String>,
    latch: bool,
}

struct Held {
    device: String,
    button: usize,
    actions: Vec<String>,
    latching: bool,
    continuous: bool,
    fresh: bool,
    out: bool,
}

/// Button-down actions remember their original mapping until release. Editing a binding
/// must release that action, rather than sending an unrelated new action's key-up.
pub(super) struct HeldButtons {
    long_press: bool,
    buttons: Vec<Held>,
    /// Empty unless a button with a second action is down and has not fired. The frame
    /// path looks at nothing else.
    pending: Vec<Pending>,
}

impl Default for HeldButtons {
    fn default() -> Self {
        Self { long_press: false, buttons: Vec::new(), pending: Vec::new() }
    }
}

impl HeldButtons {
    pub(super) fn set_long_press(&mut self, on: bool) {
        if self.long_press != on {
            self.long_press = on;
        }
    }

    /// True only while a second action is waiting. The frame skips the timer otherwise.
    #[inline]
    pub(super) fn waiting(&self) -> bool {
        !self.pending.is_empty()
    }

    pub(super) fn event(&mut self, cfg: &[DeviceCfg], name: &str, button: usize, down: bool, actions: &mut Vec<(String, bool)>) {
        if down {
            if self.buttons.iter().any(|h| names_match(&h.device, name) && h.button == button) || self.pending.iter().any(|p| names_match(&p.device, name) && p.button == button) {
                return;
            }
            let Some(d) = find_device_cfg(cfg, name) else { return };
            let bound = d.actions_of(button);
            // A second action is read only while the setting is on. Off, this is the press path as it was.
            if self.long_press {
                if let Some((long, ms)) = d.second_of(button) {
                    self.pending.push(Pending { device: name.to_string(), button, at: std::time::Instant::now(), ms, long, short: bound, latch: d.latching.contains(&button) });
                    return;
                }
            }
            if bound.is_empty() {
                return;
            }
            actions.extend(bound.iter().map(|a| (a.clone(), true)));
            self.buttons.push(Held { device: name.to_string(), button, actions: bound, latching: d.latching.contains(&button), continuous: d.continuous(button), fresh: true, out: false });
        } else if let Some(i) = self.pending.iter().position(|p| names_match(&p.device, name) && p.button == button) {
            let w = self.pending.remove(i);
            if self.long_press {
                self.finish_wait(w, actions);
            }
        } else if let Some(i) = self.buttons.iter().position(|h| names_match(&h.device, name) && h.button == button) {
            let h = self.buttons.remove(i);
            actions.extend(h.actions.iter().map(|a| (a.clone(), false)));
            if h.latching {
                for a in &h.actions {
                    let back = latch_release(a);
                    actions.push((back.clone(), true));
                    actions.push((back, false));
                }
            }
        }
    }

    fn finish_wait(&self, w: Pending, actions: &mut Vec<(String, bool)>) {
        // The button is already up. Past the time, the second action is one down and one up.
        // Before it, the original action is.
        if w.at.elapsed().as_millis() as u32 >= w.ms {
            actions.push((w.long.clone(), true));
            actions.push((w.long, false));
            return;
        }
        for a in &w.short {
            actions.push((a.clone(), true));
            actions.push((a.clone(), false));
        }
        if w.latch {
            for a in &w.short {
                let back = latch_release(a);
                actions.push((back.clone(), true));
                actions.push((back, false));
            }
        }
    }

    /// Fire a second action whose time has been reached. Returns at once when nothing is waiting.
    /// With long press off, a wait that has not fired is dropped.
    pub(super) fn advance(&mut self, actions: &mut Vec<(String, bool)>) {
        if self.pending.is_empty() {
            return;
        }
        if !self.long_press {
            self.pending.clear();
            return;
        }
        let mut i = 0;
        while i < self.pending.len() {
            if self.pending[i].at.elapsed().as_millis() as u32 >= self.pending[i].ms {
                let w = self.pending.remove(i);
                let name = w.long.clone();
                actions.push((name.clone(), true));
                self.buttons.push(Held { device: w.device, button: w.button, actions: vec![name], latching: false, continuous: false, fresh: true, out: false });
            } else {
                i += 1;
            }
        }
    }

    pub(super) fn release(&mut self, actions: &mut Vec<(String, bool)>) {
        // A wait that never fired is not a press.
        self.pending.clear();
        for h in self.buttons.drain(..) {
            // Do not toggle a latching switch just because its configuration changed.
            actions.extend(h.actions.into_iter().map(|a| (a, false)));
        }
    }

    /// Let go of the buttons held down, but not of the latching switches: a switch stays
    /// where it is while the pause menu is open or the window is in the background, and
    /// when it comes out later its release still reaches the bus. Let go of with the rest,
    /// the switch went out in the game at ESC or alt-tab and only came back once it was
    /// switched off and on again by hand (#1876).
    pub(super) fn release_momentary(&mut self, actions: &mut Vec<(String, bool)>) {
        self.pending.clear();
        self.buttons.retain(|h| {
            if !h.latching {
                actions.extend(h.actions.iter().map(|a| (a.clone(), false)));
            }
            h.latching
        });
    }

    pub(super) fn while_editing(&mut self, name: &str, button: usize, down: bool) {
        if let Some(h) = self.buttons.iter_mut().find(|h| names_match(&h.device, name) && h.button == button) {
            h.out = !down;
        }
    }

    pub(super) fn resume(&mut self, cfg: &[DeviceCfg], actions: &mut Vec<(String, bool)>) {
        let out: Vec<(String, usize)> = self.buttons.iter().filter(|h| h.out).map(|h| (h.device.clone(), h.button)).collect();
        for (name, button) in out {
            self.event(cfg, &name, button, false, actions);
        }
    }

    pub(super) fn repeating(&mut self) -> Vec<String> {
        let mut due = Vec::new();
        for h in self.buttons.iter_mut().filter(|h| h.continuous && !h.out) {
            if !std::mem::take(&mut h.fresh) {
                due.extend(h.actions.iter().filter(|a| !is_game_action(a)).cloned());
            }
        }
        due
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wheel() -> Vec<DeviceCfg> {
        let mut d = DeviceCfg { name: "Wheel".into(), ..Default::default() };
        d.buttons = vec![("bus_doorfront0".into(), "0".into()), ("retarder_up".into(), "1".into()), ("gear_up".into(), "1".into())];
        d.more = vec![(0, "bus_doorfront1".into()), (0, "bus_doorrear0".into()), (2, "sim_pause".into())];
        vec![d]
    }

    #[test]
    fn one_button_works_every_action_it_is_given() {
        let (cfg, mut held, mut actions) = (wheel(), HeldButtons::default(), Vec::new());
        held.event(&cfg, "Wheel", 0, true, &mut actions);
        held.event(&cfg, "Wheel", 0, false, &mut actions);
        let names: Vec<(&str, bool)> = actions.iter().map(|(a, d)| (a.as_str(), *d)).collect();
        assert_eq!(names, [("bus_doorfront0", true), ("bus_doorfront1", true), ("bus_doorrear0", true), ("bus_doorfront0", false), ("bus_doorfront1", false), ("bus_doorrear0", false)]);
    }

    #[test]
    fn a_continuous_button_fires_again_every_frame_it_is_held() {
        let (cfg, mut held, mut actions) = (wheel(), HeldButtons::default(), Vec::new());
        held.event(&cfg, "Wheel", 1, true, &mut actions);
        held.event(&cfg, "Wheel", 0, true, &mut actions);
        assert!(held.repeating().is_empty());
        assert_eq!(held.repeating(), ["retarder_up"]);
        assert_eq!(held.repeating(), ["retarder_up"]);
        held.event(&cfg, "Wheel", 1, false, &mut actions);
        assert!(held.repeating().is_empty());
    }

    #[test]
    fn the_gears_repeat_but_the_game_switches_do_not() {
        let (cfg, mut held, mut actions) = (wheel(), HeldButtons::default(), Vec::new());
        held.event(&cfg, "Wheel", 2, true, &mut actions);
        assert_eq!(actions, [("gear_up".to_string(), true), ("sim_pause".to_string(), true)]);
        assert!(held.repeating().is_empty());
        assert_eq!(held.repeating(), ["gear_up"]);
    }

    #[test]
    fn a_latching_switch_switched_off_in_the_menu_goes_out_when_it_closes() {
        let mut cfg = wheel();
        cfg[0].latching = vec![1];
        let (mut held, mut actions) = (HeldButtons::default(), Vec::new());
        held.event(&cfg, "Wheel", 1, true, &mut actions);
        held.repeating();
        held.release_momentary(&mut actions);
        held.while_editing("Wheel", 1, false);
        assert!(held.repeating().is_empty());
        actions.clear();
        held.resume(&cfg, &mut actions);
        assert_eq!(actions.first(), Some(&("retarder_up".to_string(), false)));
        assert!(held.repeating().is_empty());
        held.event(&cfg, "Wheel", 1, true, &mut actions);
        held.repeating();
        assert_eq!(held.repeating(), ["retarder_up"]);
    }

    fn with_second(button: usize, action: &str, ms: Option<u32>) -> Vec<DeviceCfg> {
        let mut cfg = wheel();
        cfg[0].set_second(button, action);
        if let Some(ms) = ms {
            cfg[0].set_hold_ms(button, ms);
        }
        cfg
    }

    fn age(held: &mut HeldButtons, ms: u32) {
        let back = std::time::Duration::from_millis(ms as u64);
        for w in &mut held.pending {
            w.at = w.at.checked_sub(back).unwrap_or(w.at);
        }
    }

    #[test]
    fn long_press_off_still_fires_the_press_action() {
        let (cfg, mut held, mut actions) = (with_second(0, "bus_wiper", None), HeldButtons::default(), Vec::new());
        held.event(&cfg, "Wheel", 0, true, &mut actions);
        assert_eq!(actions, [("bus_doorfront0".into(), true), ("bus_doorfront1".into(), true), ("bus_doorrear0".into(), true)]);
        age(&mut held, 500);
        held.advance(&mut actions);
        assert!(actions.iter().all(|(a, _)| a != "bus_wiper"));
    }

    #[test]
    fn long_press_on_without_a_second_action_fires_on_the_way_down() {
        let (cfg, mut held, mut actions) = (wheel(), HeldButtons::default(), Vec::new());
        held.set_long_press(true);
        held.event(&cfg, "Wheel", 0, true, &mut actions);
        assert_eq!(actions.first(), Some(&("bus_doorfront0".into(), true)));
    }

    #[test]
    fn a_tap_fires_the_press_action_down_then_up() {
        let (cfg, mut held, mut actions) = (with_second(0, "bus_wiper", None), HeldButtons::default(), Vec::new());
        held.set_long_press(true);
        held.event(&cfg, "Wheel", 0, true, &mut actions);
        assert!(actions.is_empty());
        age(&mut held, 100);
        held.event(&cfg, "Wheel", 0, false, &mut actions);
        let names: Vec<&str> = actions.iter().map(|(a, _)| a.as_str()).collect();
        assert!(names.contains(&"bus_doorfront0") && names.contains(&"bus_doorfront1"));
        assert!(!names.contains(&"bus_wiper"));
        assert_eq!(actions.iter().filter(|(a, _)| a == "bus_doorfront0").map(|(_, d)| *d).collect::<Vec<_>>(), vec![true, false]);
    }

    #[test]
    fn a_hold_fires_only_the_second_action() {
        let (cfg, mut held, mut actions) = (with_second(0, "bus_wiper", None), HeldButtons::default(), Vec::new());
        held.set_long_press(true);
        held.event(&cfg, "Wheel", 0, true, &mut actions);
        age(&mut held, 200);
        held.advance(&mut actions);
        assert_eq!(actions, [("bus_wiper".into(), true)]);
        actions.clear();
        held.event(&cfg, "Wheel", 0, false, &mut actions);
        assert_eq!(actions, [("bus_wiper".into(), false)]);
    }

    #[test]
    fn letting_go_of_focus_during_the_wait_fires_nothing() {
        let (cfg, mut held, mut actions) = (with_second(0, "bus_wiper", None), HeldButtons::default(), Vec::new());
        held.set_long_press(true);
        held.event(&cfg, "Wheel", 0, true, &mut actions);
        held.release_momentary(&mut actions);
        assert!(actions.is_empty());
        held.advance(&mut actions);
        assert!(actions.is_empty());
        let _ = cfg;
    }
}
