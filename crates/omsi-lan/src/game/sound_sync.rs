//! Owner-authored T.L / T.F events, independent of the owner's local audio listener.
use super::*;
use omsi_net::sounds::{Event, Incoming};
pub(super) fn publish_vehicle(
    lan: &mut LanSession,
    game: &mut LanGame,
    v: &mut omsi_sim::VehicleInstance,
    ai: u64,
    args: &LanConfig,
) {
    v.host.capture_sound_triggers = true;
    let events = std::mem::take(&mut v.host.network_sound_triggers);
    if events.is_empty() {
        return;
    }
    let table = sync_table(game, v);
    let bus = content_relative(&v.ty.def.path, &args.root);
    for (trigger, file, snapshot) in events {
        let mut values = Vec::new();
        if let Some(snapshot) = snapshot {
            // Transmit only the sound curves that need the value at trigger time.
            for cfg in table
                .sounds
                .iter()
                .map(|(cfg, _)| cfg)
                .chain(table.interior.iter().map(|(cfg, _)| cfg))
                .chain(table.part_sounds.iter().map(|(_, cfg, _)| cfg))
            {
                for entry in &cfg.sounds {
                    if !entry
                        .triggers
                        .iter()
                        .any(|n| n.trim().eq_ignore_ascii_case(&trigger))
                    {
                        continue;
                    }
                    for curve in &entry.vol_curves {
                        if values
                            .iter()
                            .any(|(n, _): &(String, f32)| n.eq_ignore_ascii_case(&curve.variable))
                        {
                            continue;
                        }
                        if let Some(value) = v
                            .var_slot(&curve.variable)
                            .and_then(|i| snapshot.get(i))
                            .copied()
                            .filter(|f| f.is_finite())
                        {
                            values.push((curve.variable.clone(), value));
                        }
                    }
                }
            }
        }
        lan.publish_sound(Event {
            ai,
            bus: bus.clone(),
            trigger,
            file,
            values,
        });
    }
}
pub(super) fn receive(lan: &mut LanSession, game: &mut LanGame, dt: f32) {
    for (_, age) in &mut game.pending_sounds {
        *age += dt.max(0.0);
    }
    game.pending_sounds
        .retain(|(s, age)| *age < 5.0 && lan.peers().any(|p| p.pose.id == s.owner));
    let incoming = lan.take_sounds();
    for s in incoming {
        if game.pending_sounds.len() < 1024 {
            game.pending_sounds.push((s, 0.0));
        }
    }
}
fn same_bus(a: &str, b: &str) -> bool {
    a.replace('\\', "/")
        .eq_ignore_ascii_case(&b.replace('\\', "/"))
}
fn apply(v: &mut omsi_sim::VehicleInstance, event: &Event) {
    if let Some(file) = &event.file {
        if file.len() > 260 || !omsi_net::remote_text::freetex_ok(file) {
            return;
        }
        v.host
            .fired_file_triggers
            .push((event.trigger.clone(), file.clone()));
    } else {
        v.host.fired_triggers.push(event.trigger.clone());
        if !event.values.is_empty() {
            let mut snapshot = v.state.vars.clone();
            for (name, value) in &event.values {
                if let Some(i) = v.var_slot(name) {
                    snapshot[i] = *value;
                }
            }
            v.host
                .fired_trigger_vars
                .push((event.trigger.clone(), snapshot));
        }
    }
}
pub(super) fn apply_player(pending: &mut Vec<(Incoming, f32)>, owner: u32, rv: &mut RemoteVehicle) {
    pending.retain(|(s, _)| {
        if s.owner != owner || s.event.ai != 0 {
            return true;
        }
        if !rv.stand_in && !rv.fallback && same_bus(&s.event.bus, &rv.last.bus) {
            apply(&mut rv.vehicle, &s.event);
        }
        false
    });
}

impl LanGame {
    /// Publish captured owner sounds before local audio consumes its triggers.
    pub fn publish_vehicle_sounds(
        &mut self,
        lan: &mut LanSession,
        v: &mut omsi_sim::VehicleInstance,
        ai: u64,
        args: &LanConfig,
    ) {
        publish_vehicle(lan, self, v, ai, args);
    }
    /// Called by the view only once the remote bus's sound clips have loaded.
    pub fn apply_player_sounds(&mut self, owner: u32) {
        if let Some(rv) = self.remotes.get_mut(&owner) {
            apply_player(&mut self.pending_sounds, owner, rv);
        }
    }
    /// Called for a mirrored AI bus once its sound set is ready.
    pub fn apply_ai_sounds(&mut self, v: &mut omsi_sim::VehicleInstance, ai: u64, root: &Path) {
        self.pending_sounds.retain(|(s, _)| {
            if s.event.ai != ai || ai == 0 {
                return true;
            }
            if s.owner == 1 && same_bus(&s.event.bus, &content_relative(&v.ty.def.path, root)) {
                apply(v, &s.event);
            }
            false
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bus_identity_ignores_omsi_case_and_slashes() {
        assert!(same_bus("Vehicles\\Bus\\A.bus", "vehicles/bus/a.bus"));
        assert!(!same_bus("Vehicles/Bus/A.bus", "Vehicles/Bus/B.bus"));
    }
}
