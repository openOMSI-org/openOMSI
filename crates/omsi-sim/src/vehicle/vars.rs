//! The vehicle's script variables and triggers as the game reads and sets them, and the
//! services (fuel, wash, repair) and the odometer.

use super::*;

/// The control cables' `write` variables of a vehicle (`p`, `def`) named after variable
/// `name` that its scripts store in one run of stores with it (see
/// `VehicleInstance::set_var_with_cables`).
fn cable_partners(
    p: &omsi_script::Program,
    def: &omsi_vehicle::Vehicle,
    name: &str,
) -> Vec<omsi_script::VarId> {
    let Some(id) = p.var(name) else {
        return Vec::new();
    };
    let low = name.trim().to_ascii_lowercase();
    let writes: Vec<omsi_script::VarId> = def
        .control_cable_front
        .iter()
        .chain(&def.control_cable_back)
        .filter_map(|c| c.lines.get(3))
        .filter(|w| w.to_ascii_lowercase().contains(&low))
        .filter_map(|w| p.var(w))
        .collect();
    if writes.is_empty() {
        return Vec::new();
    }
    let mut partners = Vec::new();
    for block in &p.blocks {
        // (a run of stores one after another: they all take the value on the stack)
        for run in block
            .ops
            .split(|op| !matches!(op, omsi_script::Op::Store(_)))
        {
            if run
                .iter()
                .any(|op| matches!(op, omsi_script::Op::Store(s) if *s == id))
            {
                partners.extend(run.iter().filter_map(|op| match op {
                    omsi_script::Op::Store(s) if *s != id && writes.contains(s) => Some(*s),
                    _ => None,
                }));
            }
        }
    }
    partners
}

impl VehicleInstance {
    /// How many of the train's `[add_camera_reflexion]` cameras come before part `part`'s.
    pub fn mirror_base(&self, part: usize) -> usize {
        self.ty.def.cameras_reflexion.len()
            + self
                .trailers
                .iter()
                .take(part)
                .map(|t| t.ty.def.cameras_reflexion.len())
                .sum::<usize>()
    }

    pub(super) fn get(&self, id: Option<omsi_script::VarId>) -> f32 {
        id.map(|i| self.state.vars[i as usize]).unwrap_or(0.0)
    }

    pub(super) fn put(&mut self, id: Option<omsi_script::VarId>, v: f32) {
        if let Some(i) = id {
            self.state.vars[i as usize] = v;
        }
    }

    pub(super) fn put_turn(&mut self, id: Option<omsi_script::VarId>, axle: usize, side: usize) {
        let Some(w) = self
            .physics
            .wheels
            .get_mut(axle)
            .and_then(|a| a.get_mut(side))
        else {
            return;
        };
        let step = w.turned - w.turned_put;
        w.turned_put = w.turned;
        if let Some(i) = id {
            self.state.vars[i as usize] += step;
        }
    }

    /// The bus's total mileage, as Omsi.exe 0x64207E writes it into a situation:
    /// the starting reading plus the kilometres driven since it was loaded.
    pub fn odometer_km(&self) -> f64 {
        self.host.km_base + self.driven_km
    }

    /// Continue a saved reading without giving the bus another random service history.
    /// Omsi.exe clears driven km at 0x643DBB and restores the total into the starting
    /// reading at 0x643DF3; subtract our already driven distance to keep the same total
    /// even when the caller has ticked the vehicle before restoring its situation.
    pub fn set_odometer_km(&mut self, km: f64) {
        self.host.km_base = km - self.driven_km;
        self.km_started = true;
    }

    /// Where variable `name` sits among the script's variables (`State::vars`).
    pub fn var_slot(&self, name: &str) -> Option<usize> {
        omsi_script::compile::with_lower(name, |k| self.var_index.get(k).map(|&i| i as usize))
    }

    pub fn var(&self, name: &str) -> Option<f32> {
        omsi_script::compile::with_lower(name, |k| self.var_index.get(k).copied())
            .map(|i| self.state.vars[i as usize])
    }

    /// The variables' names (lower case) and slots in `state.vars`, shared (the view reads a
    /// copy of the values by them: `crate::view_events::CarLooks`).
    pub fn var_index(&self) -> &std::sync::Arc<HashMap<String, omsi_script::VarId>> {
        &self.var_index
    }

    /// Whether variable `name` was declared in the vehicle's script set (as opposed to built-in host variables).
    pub fn has_script_var(&self, name: &str) -> bool {
        self.ty.program.has_script_var(name)
    }

    /// Whether any of the vehicle's scripts assigns variable `name`: a variable that is only
    /// declared (a shared varlist) keeps its starting zero and means nothing.
    pub fn script_stores(&self, name: &str) -> bool {
        let p = &self.ty.program;
        p.var(name).is_some_and(|id| p.stores(id))
    }

    /// The door release to fire now: the bus's own, the C2's switch turned over, else `bus_dooraft`.
    pub fn door_release_trigger(&self) -> Option<&'static str> {
        let p = &self.ty.program;
        if let Some(t) = ["bus_doorall", "CG_active", "C2_CP_doorall_1"]
            .into_iter()
            .find(|t| p.trigger(t).is_some())
        {
            return Some(t);
        }
        if p.trigger("C2_CP_Tuerfreigabe_1").is_some()
            && p.trigger("C2_CP_Tuerfreigabe_0").is_some()
        {
            return Some(if self.var("C2_CP_Tuerfreigabe").is_some_and(|x| x > 0.5) {
                "C2_CP_Tuerfreigabe_0"
            } else {
                "C2_CP_Tuerfreigabe_1"
            });
        }
        p.trigger("bus_dooraft").is_some().then_some("bus_dooraft")
    }

    /// The name of script variable `index` (lower case), for diagnostics and key helpers.
    pub fn var_name(&self, index: usize) -> Option<&str> {
        self.var_index
            .iter()
            .find(|&(_, &i)| i as usize == index)
            .map(|(n, _)| n.as_str())
    }

    pub fn set_var(&mut self, name: &str, v: f32) -> bool {
        let slot = omsi_script::compile::with_lower(name, |k| {
            self.var_index
                .get(k)
                .copied()
                .ok_or_else(|| self.model_only_vars.iter().any(|n| n == k))
        });
        match slot {
            Ok(i) => self.state.vars[i as usize] = v,
            Err(true) => {
                self.add_var(name, v);
                // a variable only the model names (#3887): its animations move from now on
                let id = (self.state.vars.len() - 1) as omsi_script::VarId;
                for a in &mut self.animators {
                    a.bind_missing(name, id);
                }
            }
            Err(false) => return false,
        }
        true
    }

    /// [`Self::set_var`], and the control cables' `write` variables the scripts store the same
    /// value into in one go (`(S.L.IBIS_Linie_Complex) (S.L.SLW_IBIS_Linie_Complex_A)`): what
    /// the game sets by hand on the IBIS reaches the train's other cab as the IBIS's own keys
    /// would send it. Only cables named after the variable count (`SLW_IBIS_Linie_Complex_A`):
    /// the NF6D's IBIS reset zeroes all its cables in one run of stores.
    ///
    /// The cars of a train that run their own scripts and work out the variable themselves (the
    /// NF6D's rear cab has an IBIS of its own) are set the same way.
    pub fn set_var_with_cables(&mut self, name: &str, v: f32) -> bool {
        let set = self.set_var(name, v);
        for s in cable_partners(&self.ty.program, &self.ty.def, name) {
            self.state.vars[s as usize] = v;
        }
        for t in &mut self.trailers {
            let ty = t.ty.clone();
            let Some(run) = t.run.as_deref_mut() else {
                continue;
            };
            if !ty.program.var(name).is_some_and(|id| ty.program.stores(id)) {
                continue;
            }
            run.set_own(name, v);
            for s in cable_partners(&ty.program, &ty.def, name) {
                if let Some(w) = ty.program.var_names.get(s as usize) {
                    run.set_own(w, v);
                }
            }
        }
        set
    }

    /// Variables the engine feeds the model directly (`Dirt_Norm`, `AI`, …): OMSI's
    /// varlists rarely declare them, and an undeclared `[alphascale]` variable used to
    /// read as 1.0 - every bus wore its dirt film at full strength from the first frame.
    pub fn set_engine_var(&mut self, name: &str, v: f32) {
        if !self.set_var(name, v) {
            self.add_var(name, v);
        }
    }

    fn add_var(&mut self, name: &str, v: f32) {
        let id = self.state.vars.len() as omsi_script::VarId;
        self.state.vars.push(v);
        std::sync::Arc::make_mut(&mut self.var_index).insert(name.to_ascii_lowercase(), id);
        self.pax_vars = std::sync::OnceLock::new();
        self.engine_ids = std::sync::OnceLock::new();
    }

    pub fn trigger(&mut self, name: &str) -> bool {
        if let Some(part) = self.active_cab {
            if self.trigger_part(part, name) {
                return true;
            }
            // (a control the other cab has under its own letter: `cp_venta` / `cp_ventb`)
            if let Some(twin) = self.cab_twin_trigger(part, name) {
                return self.trigger_part(part, &twin);
            }
        }
        let p = self.ty.program.clone();
        self.vm
            .run_trigger(&p, name, &mut self.state, &mut self.host)
    }

    pub fn has_trigger(&self, name: &str) -> bool {
        self.ty.program.trigger(name).is_some()
    }

    /// Whether the own scripts of coupled part `part` have one of the triggers `names`.
    pub fn part_has_trigger(&self, part: usize, names: &[&str]) -> bool {
        self.trailers
            .get(part)
            .and_then(|t| t.run.as_deref())
            .is_some_and(|r| names.iter().any(|n| r.has_trigger(n)))
    }

    /// Set variable `name` in the own scripts of coupled part `part`.
    pub fn set_part_var(&mut self, part: usize, name: &str, v: f32) {
        if let Some(run) = self
            .trailers
            .get_mut(part)
            .and_then(|t| t.run.as_deref_mut())
        {
            run.set_own(name, v);
        }
    }

    /// Set string variable `name` in the own scripts of coupled part `part`.
    pub fn set_part_str(&mut self, part: usize, name: &str, text: &str) {
        if let Some(run) = self
            .trailers
            .get_mut(part)
            .and_then(|t| t.run.as_deref_mut())
        {
            run.set_own_str(name, text);
        }
    }

    /// The trigger of coupled part `part`'s cab that is the leading vehicle's `name` under the
    /// other cab's letter: `name` with one of its `a`s made a `b` (`cp_venta` -> `cp_ventb`,
    /// `spiegel_al_init` -> `spiegel_bl_init`), where exactly one such the part has and the
    /// leading vehicle has `name` itself.
    pub fn cab_twin_trigger(&self, part: usize, name: &str) -> Option<String> {
        self.ty.program.trigger(name)?;
        let run = self.trailers.get(part)?.run.as_deref()?;
        let low = name.to_ascii_lowercase();
        let mut twins = low
            .char_indices()
            .filter(|&(_, c)| c == 'a')
            .map(|(i, _)| format!("{}b{}", &low[..i], &low[i + 1..]))
            .filter(|t| run.has_trigger(t));
        let twin = twins.next()?;
        twins.next().is_none().then_some(twin)
    }

    /// Fire trigger `name` in the own scripts of coupled part `part` (one that runs them, see
    /// `car_run`). False when it runs none or its scripts have no such trigger: the caller
    /// then gives it to the leading vehicle's.
    pub fn trigger_part(&mut self, part: usize, name: &str) -> bool {
        if !self
            .trailers
            .get(part)
            .and_then(|t| t.run.as_deref())
            .is_some_and(|r| r.has_trigger(name))
        {
            return false;
        }
        let mut trailers = std::mem::take(&mut self.trailers);
        let t = &mut trailers[part];
        let fired = t
            .run
            .as_deref_mut()
            .is_some_and(|r| r.trigger(self, name, &mut t.script_textures));
        self.trailers = trailers;
        fired
    }

    /// The script variables as `names` would leave them, run one after another, with the
    /// vehicle left exactly as it was: its variables, the machine's random numbers, the
    /// sounds and messages the triggers asked for.
    pub fn trial_triggers(&mut self, names: &[&str]) -> Vec<f32> {
        let (state, vm) = (self.state.clone(), self.vm.clone());
        let (fired, fired_files, messages, time_written) = (
            self.host.fired_triggers.len(),
            self.host.fired_file_triggers.len(),
            self.host.messages.clone(),
            self.host.time_written,
        );
        let last_message = self.host.last_message.clone();
        let fired_vars = self.host.fired_trigger_vars.len();
        let network_sounds = self.host.network_sound_triggers.len();
        // (the leading vehicle's alone: a coupled part's run is not put back afterwards)
        let cab = self.active_cab.take();
        for n in names {
            self.trigger(n);
        }
        self.active_cab = cab;
        let out = self.state.vars.clone();
        self.state = state;
        self.vm = vm;
        self.host.fired_triggers.truncate(fired);
        self.host.fired_trigger_vars.truncate(fired_vars);
        self.host.network_sound_triggers.truncate(network_sounds);
        self.host.fired_file_triggers.truncate(fired_files);
        self.host.messages = messages;
        self.host.last_message = last_message;
        self.host.time_written = time_written;
        out
    }

    /// Fire a trigger and read the number the script leaves on the stack. OMSI asks the
    /// bus how long its repairs take this way (`malfunction_gettime`): every damaged system
    /// adds its minutes to the value passing through the chain of macros.
    pub fn trigger_value(&mut self, name: &str) -> Option<f32> {
        let p = self.ty.program.clone();
        self.vm.stacks = Default::default();
        if !self
            .vm
            .run_trigger(&p, name, &mut self.state, &mut self.host)
        {
            return None;
        }
        Some(self.vm.stacks.st[0])
    }

    /// Run one of the engine's service triggers with `secs` on the clock: OMSI holds
    /// `veh_tank` / `veh_wash` down while the pump or the wash runs and the bus script
    /// decides what a second of it is worth (the SD202 takes 3 litres and caps at 250).
    /// One step of a depot service: its trigger (`veh_tank`, `veh_wash`) with `secs` of
    /// the script's clock. False when the bus has no such handling.
    pub fn service(&mut self, name: &str, secs: f32) -> bool {
        let keep = self.host.clock.timegap;
        self.host.clock.timegap = secs;
        let ok = self.trigger(name);
        self.host.clock.timegap = keep;
        ok
    }

    /// Refuel until the pump stops changing anything. Returns the tank content the script
    /// reports.
    ///
    /// `veh_tank` is the bus's whole "fill up" handling, not only the diesel tank's: an
    /// electric bus charges its battery there, and its (often vestigial) diesel tank may
    /// already be full. So the pump runs until no variable of the script moves any more,
    /// not until `engine_tank_content` stops rising, which had cut the charge off after
    /// two seconds (#3784).
    pub fn refuel(&mut self) -> Option<f32> {
        let mut last = self.state.vars.clone();
        for _ in 0..600 {
            if !self.service("veh_tank", 1.0) {
                return None;
            }
            let settled = self
                .state
                .vars
                .iter()
                .zip(&last)
                .all(|(now, was)| (now - was).abs() <= 1e-3 || (now.is_nan() && was.is_nan()));
            if settled {
                break;
            }
            last.clone_from(&self.state.vars);
        }
        self.var("engine_tank_content")
    }

    /// Run the bus through the wash until it stops getting cleaner.
    pub fn wash(&mut self) -> Option<f32> {
        self.dirt = 0.0;
        self.set_engine_var("Dirt_Norm", 0.0);
        let mut last = f32::INFINITY;
        for _ in 0..600 {
            if !self.service("veh_wash", 1.0) {
                return None;
            }
            let now = self.var("Dirt_Wiped")?;
            if now >= last - 1e-4 {
                break;
            }
            last = now;
        }
        self.var("Dirt_Wiped")
    }

    /// Minutes the workshop needs for the damage the bus has right now.
    pub fn repair_minutes(&mut self) -> Option<f32> {
        self.trigger_value("malfunction_gettime")
    }

    /// Repair everything (the engine fires this once the driver accepts the waiting time).
    pub fn repair(&mut self) -> bool {
        self.trigger("malfunction_reset")
    }
}

#[cfg(test)]
mod tests {
    /// A cash desk key hands out what its script asks for with `GiveChangeCoin`: coin
    /// number n of the map's money system, as often as the script calls it, whatever value
    /// the key's name speaks of (a DM desk on a euro map, a 0.20 key on a map with 0.25s;
    /// #4191). OMSI 2 does not read the key's name.
    #[test]
    fn a_change_key_gives_the_coins_its_script_names() {
        let osc = "{trigger:cashdesk_change_0500}\n5 (M.V.GiveChangeCoin)\n{end}\n\
                   {trigger:cashdesk_changer_0_20}\n3 (M.V.GiveChangeCoin)\n{end}\n\
                   {trigger:cashdesk_changer_0_30}\n0 (M.V.GiveChangeCoin)\n0 (M.V.GiveChangeCoin)\n0 (M.V.GiveChangeCoin)\n{end}\n";
        let mut v = crate::timetable_run::tests::script_test_vehicle(osc, "", "");
        v.trigger("cashdesk_change_0500");
        assert_eq!(std::mem::take(&mut v.host.change_coins), [5]);
        v.trigger("cashdesk_changer_0_20");
        assert_eq!(std::mem::take(&mut v.host.change_coins), [3]);
        v.trigger("cashdesk_changer_0_30");
        assert_eq!(std::mem::take(&mut v.host.change_coins), [0, 0, 0]);
    }
}
#[cfg(test)]
mod turn_tests {
    #[test]
    fn a_wheel_turn_counts_on_by_its_whole_step_and_from_a_script_s_reset() {
        let mut v =
            crate::timetable_run::tests::script_test_vehicle("", "Wheel_Rotation_0_L\n", "");
        let id = v.ty.program.var("Wheel_Rotation_0_L");
        let w = crate::physics::WheelState {
            long: 0.0,
            lat: 0.0,
            radius: 0.5,
            driven: true,
            rotation_deg: 0.0,
            turned: 0.0,
            turned_put: 0.0,
            rpm: 0.0,
            suspension: 0.0,
        };
        v.physics.wheels = vec![[w.clone(), w]];
        v.physics.wheels[0][0].turned += 3.3;
        v.put_turn(id, 0, 0);
        assert!(
            (v.get(id) - 3.3).abs() < 1e-5,
            "a step over half a turn counts forward: {}",
            v.get(id)
        );
        v.put(id, 0.0);
        v.physics.wheels[0][0].turned += 0.5;
        v.put_turn(id, 0, 0);
        assert!(
            (v.get(id) - 0.5).abs() < 1e-5,
            "a script's reset counts on from 0: {}",
            v.get(id)
        );
    }
}
