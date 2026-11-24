use super::{DeviceCfg, find_device_cfg, latch_release, names_match};
use crate::actions::is_game_action;

struct Held {
    device: String,
    button: usize,
    actions: Vec<String>,
    latching: bool,
    continuous: bool,
    fresh: bool,
    out: bool,
    /// A latching switch put in while the menu was open: its press is still to come, when
    /// the menu closes (`resume`).
    pending: bool,
}

/// Button-down actions remember their original mapping until release. Editing a binding
/// must release that action, rather than sending an unrelated new action's key-up.
#[derive(Default)]
pub(super) struct HeldButtons(Vec<Held>);

impl HeldButtons {
    pub(super) fn event(
        &mut self,
        cfg: &[DeviceCfg],
        name: &str,
        button: usize,
        down: bool,
        actions: &mut Vec<(String, bool)>,
    ) {
        if down {
            if self
                .0
                .iter()
                .any(|h| names_match(&h.device, name) && h.button == button)
            {
                return;
            }
            let Some(d) = find_device_cfg(cfg, name) else {
                return;
            };
            let bound = d.actions_of(button);
            if bound.is_empty() {
                return;
            }
            actions.extend(bound.iter().map(|a| (a.clone(), true)));
            self.0.push(Held {
                device: name.to_string(),
                button,
                actions: bound,
                latching: d.latching.contains(&button),
                continuous: d.continuous(button),
                fresh: true,
                out: false,
                pending: false,
            });
        } else if let Some(i) = self
            .0
            .iter()
            .position(|h| names_match(&h.device, name) && h.button == button)
        {
            let h = self.0.remove(i);
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

    pub(super) fn release(&mut self, actions: &mut Vec<(String, bool)>) {
        for h in self.0.drain(..) {
            // Do not toggle a latching switch just because its configuration changed.
            // (a pending one never sent its press: there is nothing to let go of)
            if !h.pending {
                actions.extend(h.actions.into_iter().map(|a| (a, false)));
            }
        }
    }

    /// Let go of the buttons held down, but not of the latching switches: a switch stays
    /// where it is while the pause menu is open or the window is in the background, and
    /// when it comes out later its release still reaches the bus. Let go of with the rest,
    /// the switch went out in the game at ESC or alt-tab and only came back once it was
    /// switched off and on again by hand (#1876).
    pub(super) fn release_momentary(&mut self, actions: &mut Vec<(String, bool)>) {
        self.0.retain(|h| {
            if !h.latching {
                actions.extend(h.actions.iter().map(|a| (a.clone(), false)));
            }
            h.latching
        });
    }

    /// A button moved while the menu is open. A latching switch keeps the bus's state
    /// with its own, so either way it moves counts once the menu closes: one switched out
    /// goes out then, and one switched in goes in then. Only the first was kept: a switch
    /// put in during the pause was forgotten, stood in while what it works stayed off,
    /// and from then on worked the wrong way round (#3861).
    pub(super) fn while_editing(
        &mut self,
        cfg: &[DeviceCfg],
        name: &str,
        button: usize,
        down: bool,
    ) {
        if let Some(i) = self
            .0
            .iter()
            .position(|h| names_match(&h.device, name) && h.button == button)
        {
            if self.0[i].pending && !down {
                // in and out again within the pause: nothing happened
                self.0.remove(i);
            } else {
                self.0[i].out = !down;
            }
            return;
        }
        if !down {
            return;
        }
        let Some(d) = find_device_cfg(cfg, name) else {
            return;
        };
        let bound = d.actions_of(button);
        if bound.is_empty() || !d.latching.contains(&button) {
            return;
        }
        self.0.push(Held {
            device: name.to_string(),
            button,
            actions: bound,
            latching: true,
            continuous: d.continuous(button),
            fresh: true,
            out: false,
            pending: true,
        });
    }

    pub(super) fn resume(&mut self, cfg: &[DeviceCfg], actions: &mut Vec<(String, bool)>) {
        for h in self.0.iter_mut().filter(|h| h.pending) {
            h.pending = false;
            h.fresh = true;
            actions.extend(h.actions.iter().map(|a| (a.clone(), true)));
        }
        let out: Vec<(String, usize)> = self
            .0
            .iter()
            .filter(|h| h.out)
            .map(|h| (h.device.clone(), h.button))
            .collect();
        for (name, button) in out {
            self.event(cfg, &name, button, false, actions);
        }
    }

    pub(super) fn repeating(&mut self) -> Vec<String> {
        let mut due = Vec::new();
        for h in self
            .0
            .iter_mut()
            .filter(|h| h.continuous && !h.out && !h.pending)
        {
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
        let mut d = DeviceCfg {
            name: "Wheel".into(),
            ..Default::default()
        };
        d.buttons = vec![
            ("bus_doorfront0".into(), "0".into()),
            ("retarder_up".into(), "1".into()),
            ("gear_up".into(), "1".into()),
        ];
        d.more = vec![
            (0, "bus_doorfront1".into()),
            (0, "bus_doorrear0".into()),
            (2, "sim_pause".into()),
        ];
        vec![d]
    }

    #[test]
    fn one_button_works_every_action_it_is_given() {
        let (cfg, mut held, mut actions) = (wheel(), HeldButtons::default(), Vec::new());
        held.event(&cfg, "Wheel", 0, true, &mut actions);
        held.event(&cfg, "Wheel", 0, false, &mut actions);
        let names: Vec<(&str, bool)> = actions.iter().map(|(a, d)| (a.as_str(), *d)).collect();
        assert_eq!(
            names,
            [
                ("bus_doorfront0", true),
                ("bus_doorfront1", true),
                ("bus_doorrear0", true),
                ("bus_doorfront0", false),
                ("bus_doorfront1", false),
                ("bus_doorrear0", false)
            ]
        );
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
        assert_eq!(
            actions,
            [
                ("gear_up".to_string(), true),
                ("sim_pause".to_string(), true)
            ]
        );
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
        held.while_editing(&cfg, "Wheel", 1, false);
        assert!(held.repeating().is_empty());
        actions.clear();
        held.resume(&cfg, &mut actions);
        assert_eq!(actions.first(), Some(&("retarder_up".to_string(), false)));
        assert!(held.repeating().is_empty());
        held.event(&cfg, "Wheel", 1, true, &mut actions);
        held.repeating();
        assert_eq!(held.repeating(), ["retarder_up"]);
    }

    /// #3861: a latching switch put in while the menu is open goes in when it closes, so
    /// the bus follows it and its next way out switches back.
    #[test]
    fn a_latching_switch_switched_on_in_the_menu_goes_in_when_it_closes() {
        let mut cfg = wheel();
        cfg[0].latching = vec![0];
        let (mut held, mut actions) = (HeldButtons::default(), Vec::new());
        held.while_editing(&cfg, "Wheel", 0, true);
        assert!(actions.is_empty());
        assert!(held.repeating().is_empty());
        held.resume(&cfg, &mut actions);
        let names: Vec<(&str, bool)> = actions.iter().map(|(a, d)| (a.as_str(), *d)).collect();
        assert_eq!(
            names,
            [
                ("bus_doorfront0", true),
                ("bus_doorfront1", true),
                ("bus_doorrear0", true)
            ]
        );
        actions.clear();
        held.event(&cfg, "Wheel", 0, false, &mut actions);
        // let go, then the latch's way back: each action once more
        assert_eq!(actions.len(), 3 + 3 * 2);
        assert_eq!(actions[3], ("bus_doorfront0".to_string(), false));
        assert_eq!(actions[4], ("bus_doorfront0".to_string(), true));
    }

    /// In and out again during the pause, or a button that is no latch: nothing at all.
    #[test]
    fn a_switch_moved_and_back_in_the_menu_or_a_plain_button_does_nothing() {
        let mut cfg = wheel();
        cfg[0].latching = vec![0];
        let (mut held, mut actions) = (HeldButtons::default(), Vec::new());
        held.while_editing(&cfg, "Wheel", 0, true);
        held.while_editing(&cfg, "Wheel", 0, false);
        held.while_editing(&cfg, "Wheel", 2, true);
        held.resume(&cfg, &mut actions);
        held.release(&mut actions);
        assert!(actions.is_empty());
    }
}
