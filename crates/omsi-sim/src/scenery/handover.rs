//! How placed scenery objects reach the simulation's list, and what the player and the
//! traffic do to them between steps (switches, signals, mouse events, pages).
//!
//! The tile loader places objects while the simulation may be stepping elsewhere: it hands
//! each new object (and each tile that goes) to a [`SceneryInbox`], and the list takes
//! them in that order at the start of its step and before anything else reads it.

use super::SceneryInstance;
use super::state::{SceneryObject, SceneryState};
use std::sync::atomic::{AtomicU64, Ordering};

/// One change of the loaded scenery, in the order the loader made them.
pub enum SceneryChange {
    /// A placed object (its key already given, see [`SceneryInbox::next_key`]).
    Add(SceneryObject),
    /// The objects of a tile go.
    DropTile((i32, i32)),
    /// A particle emitter reads the variables of objects of this map id.
    Watch(i64),
    /// A placed traffic light lamp's script (its key given by [`SceneryInbox::next_key`]).
    Lamp(super::LampScript),
}

/// The loader's side of the handover: shared by the world (which writes) and the
/// simulation's list (which takes).
pub struct SceneryInbox {
    /// Which world this is: a list that follows another inbox starts empty.
    id: u64,
    next_key: AtomicU64,
    changes: crate::handover::Inbox<SceneryChange>,
}

static INBOXES: AtomicU64 = AtomicU64::new(1);

impl Default for SceneryInbox {
    fn default() -> Self {
        SceneryInbox {
            id: INBOXES.fetch_add(1, Ordering::Relaxed),
            next_key: AtomicU64::new(1),
            changes: Default::default(),
        }
    }
}

impl SceneryInbox {
    /// A new object's key (counting up: the list stays in key order).
    pub fn next_key(&self) -> u64 {
        self.next_key.fetch_add(1, Ordering::Relaxed)
    }

    pub fn push(&self, change: SceneryChange) {
        self.changes.push(change);
    }
}

const SWITCH_VARS: &[&str] = &[
    "Switch",
    "Weiche",
    "weiche_dir",
    "weiche_pos",
    "Weichenstellung",
    "Weichenlage",
    "switch_pos",
    "active_path",
];

fn switch_var_value(inst: &SceneryInstance) -> Option<f32> {
    for &name in SWITCH_VARS {
        if let Some(val) = inst.var(name) {
            return Some(val);
        }
    }
    None
}

/// Give the switch variables of points `o` the value `value`: whether one changed.
fn set_switch_vars(o: &mut SceneryObject, value: f32) -> bool {
    let vars = o.switch_vars.get_or_insert_with(|| {
        SWITCH_VARS
            .iter()
            .filter_map(|&name| o.inst.program.var(name))
            .collect()
    });
    let mut changed = false;
    for &var_id in vars.iter() {
        if let Some(val) = o.inst.state.vars.get_mut(var_id as usize) {
            if *val != value {
                *val = value;
                changed = true;
            }
        }
    }
    changed
}

/// The value points `o` are set to.
fn switch_value(o: &SceneryObject) -> Option<f32> {
    match &o.switch_vars {
        Some(vars) => vars
            .first()
            .and_then(|&i| o.inst.state.vars.get(i as usize).copied()),
        None => switch_var_value(&o.inst),
    }
}

impl SceneryState {
    /// Take the changes `inbox` has (in order). A list that followed another inbox (another
    /// map) is emptied first.
    pub fn take_inbox(&mut self, inbox: &SceneryInbox) {
        if self.inbox_id != inbox.id {
            *self = SceneryState::default();
            self.inbox_id = inbox.id;
        }
        if !inbox.changes.is_pending() {
            return;
        }
        for c in inbox.changes.take() {
            match c {
                SceneryChange::Add(o) => {
                    self.push(o);
                }
                SceneryChange::DropTile(key) => {
                    self.drop_tile(key, |_, _| {});
                    self.lamps.drop_tile(key);
                }
                SceneryChange::Watch(id) => self.watch(id),
                SceneryChange::Lamp(lamp) => self.lamps.push(lamp),
            }
        }
    }

    /// Whether switch object `id` is set to its path `path` (None: unknown switch or path).
    pub fn switch_set_to(&self, id: i64, path: u16) -> Option<bool> {
        let o = self.get(self.find(id)?);
        let direction = *o.switch_dirs.get(path as usize)?;
        let val = switch_value(o)?;
        if let Some(d) = direction {
            Some((val - d as f32).abs() < 0.5)
        } else if o.switch_dirs.iter().any(|d| d.is_some()) {
            let any_other_active = o
                .switch_dirs
                .iter()
                .filter_map(|d| *d)
                .any(|d| (val - d as f32).abs() < 0.5);
            Some(!any_other_active)
        } else {
            Some((val - path as f32).abs() < 0.5)
        }
    }

    /// Throw the points the trains need (`Traffic::switch_requests`): a switch object gets
    /// that value in its switch variables, and its blades turn with it.
    pub fn set_switches(&mut self, requests: &[(i64, u16)]) {
        for &(id, path) in requests {
            let Some(o) = self.find_mut(id) else { continue };
            let Some(&direction) = o.switch_dirs.get(path as usize) else {
                continue;
            };
            let target_d = if let Some(d) = direction {
                d as f32
            } else if o.switch_dirs.contains(&Some(0)) {
                // Zero selects an explicit branch here; the script has no known default value.
                continue;
            } else if o.switch_dirs.iter().any(|d| d.is_some()) {
                0.0
            } else {
                path as f32
            };
            if set_switch_vars(o, target_d) && omsi_cfg::flags::OMSI_DEBUG_SWITCHES.is_set() {
                let path = o
                    .sco_path
                    .as_deref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                log::info!(
                    "switch {} ({}) at ({:.0}, {:.0}) thrown to {target_d} for a train",
                    id,
                    path,
                    o.pos.x,
                    o.pos.y
                );
            }
        }
    }

    /// The points within `radius` of `at` and the value each is set to: (map id, the key it
    /// was placed with, value). What a LAN host tells a player's game, whose points then lie
    /// as the host's do (a key that changed: the tile came again, its points as the map has
    /// them).
    pub fn switch_values_near(&self, at: glam::DVec3, radius: f64) -> Vec<(i64, u64, f32)> {
        let r2 = radius * radius;
        self.objects()
            .iter()
            .filter(|o| !o.switch_dirs.is_empty() && (o.pos - at).truncate().length_squared() <= r2)
            .filter_map(|o| Some((o.map_id, o.key, switch_value(o)?)))
            .collect()
    }

    /// Set points `id` as another game has them (`switch_values_near` there); false where
    /// there are no such points here.
    pub fn set_switch_value(&mut self, id: i64, value: f32) -> bool {
        match self.find_mut(id) {
            Some(o) if !o.switch_dirs.is_empty() && value.is_finite() => {
                if set_switch_vars(o, value) && omsi_cfg::flags::OMSI_DEBUG_SWITCHES.is_set() {
                    log::info!("switch {id} set to {value} as another game has it");
                }
                true
            }
            _ => false,
        }
    }

    /// Set the railway signals: `aspects` gives each signal object's `Signal` (0 stop, 1 go,
    /// 2 go at the route's speed limit) as the traffic worked it out, and every signal
    /// learns what the next one shows (`NextSignal`) - a distant signal what its main
    /// signal shows. `ids`: the objects `routes` names (gathered once when the map opened).
    pub fn set_signals(
        &mut self,
        routes: &[omsi_map::ailists::SignalRoute],
        ids: &hashbrown::HashSet<i64>,
        aspects: &hashbrown::HashMap<i64, f32>,
    ) {
        if routes.is_empty() {
            return;
        }
        let mut next: hashbrown::HashMap<i64, f32> = hashbrown::HashMap::new();
        for r in routes {
            let own = aspects.get(&r.signal.0).copied().unwrap_or(0.0);
            if let Some(d) = r.dist_signal {
                let e = next.entry(d).or_insert(0.0);
                *e = e.max(own);
            }
            if let Some(n) = r.next_signal {
                let e = next.entry(r.signal.0).or_insert(0.0);
                *e = e.max(aspects.get(&n).copied().unwrap_or(0.0));
            }
        }
        // (found anew only when objects came or went, not by a walk of the list each frame)
        if self.signals.0 != self.generation() {
            let found: Vec<u32> = self
                .objects()
                .iter()
                .enumerate()
                .filter(|(_, o)| ids.contains(&o.map_id))
                .map(|(i, _)| i as u32)
                .collect();
            self.signals = (self.generation(), found);
        }
        let signals = std::mem::take(&mut self.signals.1);
        for &i in &signals {
            let o = &mut self.objects_mut()[i as usize];
            let a = aspects.get(&o.map_id).copied().unwrap_or(0.0);
            if o.inst.var("Signal") != Some(a) {
                o.inst.set_var("Signal", a);
                if omsi_cfg::flags::OMSI_DEBUG_SIGNALS.is_set() {
                    log::info!(
                        "signal {} at ({:.0}, {:.0}) shows {a}",
                        o.map_id,
                        o.pos.x,
                        o.pos.y
                    );
                }
            }
            o.inst
                .set_var("NextSignal", next.get(&o.map_id).copied().unwrap_or(0.0));
        }
        self.signals.1 = signals;
    }

    /// A press, release or move on page `page` of object `map_id`: what the page does
    /// (`omsi.setVar`, `omsi.trigger`) reaches the object's script.
    pub fn html_pointer(
        &mut self,
        map_id: i64,
        page: usize,
        u: f32,
        v: f32,
        kind: crate::htmltex::PointerKind,
    ) -> bool {
        match self.find_mut(map_id) {
            Some(o) => o.inst.html_pointer(page, u, v, kind),
            None => false,
        }
    }

    /// Click down on a scenery object's `[mouseevent]` switch or button.
    pub fn object_click(&mut self, map_id: i64, event: &str) -> bool {
        let Some(o) = self.find_mut(map_id) else {
            return false;
        };
        log::info!("scenery mouse event {event} on object {map_id}");
        let ok = o.inst.trigger(event);
        let drag = format!("{event}_drag");
        let low = event.to_ascii_lowercase();
        if (low.contains("taste") || low.contains("button") || low.contains("click"))
            && o.inst.program.trigger(&drag).is_some()
        {
            o.inst.host.mouse = (0.0, 0.0);
            o.inst.trigger(&drag);
            o.inst.host.mouse = (0.0, 0.0);
        }
        ok
    }

    /// Mouse dragged while holding down a scenery object switch.
    pub fn object_drag(&mut self, map_id: i64, event: &str, dx: f32, dy: f32) -> bool {
        let Some(o) = self.find_mut(map_id) else {
            return false;
        };
        let drag = format!("{event}_drag");
        o.inst.host.mouse = (dx, dy);
        let ok = o.inst.trigger(&drag);
        o.inst.host.mouse = (0.0, 0.0);
        ok
    }

    /// Mouse button released from a scenery object switch (`<event>_off`).
    pub fn object_release(&mut self, map_id: i64, event: &str) -> bool {
        let Some(o) = self.find_mut(map_id) else {
            return false;
        };
        let off = format!("{event}_off");
        o.inst.trigger(&off)
    }

    /// Mouse wheel notch over a scenery object switch.
    pub fn object_wheel(&mut self, map_id: i64, event: &str, amount: f32) -> bool {
        let Some(o) = self.find_mut(map_id) else {
            return false;
        };
        o.inst.host.mouse = (0.0, amount);
        let _ = o.inst.trigger(&format!("{event}_drag"));
        o.inst.host.mouse = (0.0, 0.0);
        o.inst.trigger(&format!("{event}_off"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenery::SceneryInstance;
    use glam::DVec3;
    use std::sync::Arc;

    fn object(inbox: &SceneryInbox, map_id: i64, tile: (i32, i32)) -> SceneryObject {
        let mut p = omsi_script::Program::default();
        p.declare_var("Switch");
        let inst = SceneryInstance::new(Arc::new(p), &[], crate::SimClock::default(), &[]);
        let mut o = SceneryObject::new(inbox.next_key(), map_id, DVec3::ZERO, tile, inst, 0);
        o.switch_dirs = vec![None, Some(1)];
        o
    }

    /// The list takes what was placed and taken away in that order, and starts anew for
    /// another map's inbox.
    #[test]
    fn the_list_follows_its_inbox_in_order() {
        let inbox = SceneryInbox::default();
        let mut s = SceneryState::default();
        inbox.push(SceneryChange::Add(object(&inbox, 1, (0, 0))));
        inbox.push(SceneryChange::Add(object(&inbox, 2, (1, 0))));
        inbox.push(SceneryChange::DropTile((0, 0)));
        inbox.push(SceneryChange::Add(object(&inbox, 1, (2, 0))));
        s.take_inbox(&inbox);
        let ids: Vec<i64> = s.objects().iter().map(|o| o.map_id).collect();
        assert_eq!(ids, vec![2, 1]);
        assert!(s.objects().windows(2).all(|w| w[0].key < w[1].key));
        // points thrown through the list, read back
        assert_eq!(s.switch_set_to(1, 1), Some(false));
        assert_eq!(s.switch_set_to(1, 0), Some(true));
        s.set_switches(&[(1, 1)]);
        assert_eq!(s.switch_set_to(1, 1), Some(true));
        assert_eq!(s.switch_set_to(1, 0), Some(false));
        let other = SceneryInbox::default();
        s.take_inbox(&other);
        assert!(s.is_empty());
    }

    /// A LAN player's game takes the points as the host has them, by value, and reads back
    /// only the points near it.
    #[test]
    fn points_go_by_value_to_another_game() {
        let inbox = SceneryInbox::default();
        let mut host = SceneryState::default();
        let mut near = object(&inbox, 1, (0, 0));
        near.pos = DVec3::new(10.0, 0.0, 0.0);
        let mut far = object(&inbox, 2, (0, 0));
        far.pos = DVec3::new(900.0, 0.0, 0.0);
        inbox.push(SceneryChange::Add(near));
        inbox.push(SceneryChange::Add(far));
        host.take_inbox(&inbox);
        host.set_switches(&[(1, 1), (2, 1)]);
        let sent = host.switch_values_near(DVec3::ZERO, 500.0);
        assert_eq!(sent.len(), 1);
        assert_eq!((sent[0].0, sent[0].2), (1, 1.0));

        let other = SceneryInbox::default();
        let mut player = SceneryState::default();
        other.push(SceneryChange::Add(object(&other, 1, (0, 0))));
        player.take_inbox(&other);
        assert_eq!(player.switch_set_to(1, 1), Some(false));
        for (id, _, v) in sent {
            assert!(player.set_switch_value(id, v));
        }
        assert_eq!(player.switch_set_to(1, 1), Some(true));
        assert!(
            !player.set_switch_value(2, 1.0),
            "points this game has not loaded"
        );
        assert!(!player.set_switch_value(1, f32::NAN));
    }

    #[test]
    fn invalid_switch_path_is_ignored() {
        let inbox = SceneryInbox::default();
        let mut s = SceneryState::default();
        inbox.push(SceneryChange::Add(object(&inbox, 1, (0, 0))));
        s.take_inbox(&inbox);
        s.set_switches(&[(1, 1)]);
        assert_eq!(s.switch_set_to(1, 2), None);
        s.set_switches(&[(1, 2)]);
        assert_eq!(s.switch_set_to(1, 1), Some(true));
    }

    #[test]
    fn untagged_switch_path_does_not_select_an_explicit_zero_direction() {
        let inbox = SceneryInbox::default();
        let mut s = SceneryState::default();
        let mut switch = object(&inbox, 1, (0, 0));
        switch.switch_dirs = vec![None, Some(0), Some(1)];
        inbox.push(SceneryChange::Add(switch));
        s.take_inbox(&inbox);
        s.set_switches(&[(1, 2), (1, 0)]);
        assert_eq!(s.switch_set_to(1, 1), Some(false));
        assert_eq!(s.switch_set_to(1, 2), Some(true));
    }

    #[test]
    fn switch_with_german_variable_and_untagged_paths_is_thrown_and_read_back() {
        let inbox = SceneryInbox::default();
        let mut s = SceneryState::default();
        let mut p = omsi_script::Program::default();
        p.declare_var("Weiche");
        let inst = SceneryInstance::new(Arc::new(p), &[], crate::SimClock::default(), &[]);
        let mut switch = SceneryObject::new(inbox.next_key(), 10, DVec3::ZERO, (0, 0), inst, 0);
        switch.switch_dirs = vec![None, None];
        inbox.push(SceneryChange::Add(switch));
        s.take_inbox(&inbox);
        assert_eq!(s.switch_set_to(10, 0), Some(true));
        assert_eq!(s.switch_set_to(10, 1), Some(false));
        s.set_switches(&[(10, 1)]);
        assert_eq!(s.switch_set_to(10, 1), Some(true));
        assert_eq!(s.switch_set_to(10, 0), Some(false));
        s.set_switches(&[(10, 0)]);
        assert_eq!(s.switch_set_to(10, 0), Some(true));
        assert_eq!(s.switch_set_to(10, 1), Some(false));
    }

    /// A lamp's script is found by its key, whatever order the loader's threads handed the
    /// lamps over in, and goes with its tile.
    #[test]
    fn lamps_by_key_and_tile() {
        let inbox = SceneryInbox::default();
        let mut s = SceneryState::default();
        let lamp = |key: u64, tile: (i32, i32)| {
            let inst = SceneryInstance::new(
                Arc::new(omsi_script::Program::default()),
                &[],
                crate::SimClock::default(),
                &[],
            );
            SceneryChange::Lamp(crate::scenery::LampScript { key, tile, inst })
        };
        let (a, b, c) = (inbox.next_key(), inbox.next_key(), inbox.next_key());
        inbox.push(lamp(b, (0, 0)));
        inbox.push(lamp(a, (1, 0)));
        inbox.push(lamp(c, (0, 0)));
        s.take_inbox(&inbox);
        assert_eq!(s.lamps.len(), 3);
        assert!(s.lamps.get(a).is_some() && s.lamps.get(b).is_some() && s.lamps.get(c).is_some());
        inbox.push(SceneryChange::DropTile((0, 0)));
        s.take_inbox(&inbox);
        assert_eq!(s.lamps.len(), 1);
        assert!(s.lamps.get(a).is_some() && s.lamps.get(b).is_none());
    }
}
