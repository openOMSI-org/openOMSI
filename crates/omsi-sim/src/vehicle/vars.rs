//! The vehicle's script variables and triggers as the game reads and sets them, and the
//! services (fuel, wash, repair) and the odometer.

use super::*;

impl VehicleInstance {
    pub(super) fn get(&self, id: Option<omsi_script::VarId>) -> f32 {
        id.map(|i| self.state.vars[i as usize]).unwrap_or(0.0)
    }

    pub(super) fn put(&mut self, id: Option<omsi_script::VarId>, v: f32) {
        if let Some(i) = id {
            self.state.vars[i as usize] = v;
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
        if let Some(t) = ["bus_doorall", "CG_active", "C2_CP_doorall_1"].into_iter().find(|t| p.trigger(t).is_some()) {
            return Some(t);
        }
        if p.trigger("C2_CP_Tuerfreigabe_1").is_some() && p.trigger("C2_CP_Tuerfreigabe_0").is_some() {
            return Some(if self.var("C2_CP_Tuerfreigabe").is_some_and(|x| x > 0.5) { "C2_CP_Tuerfreigabe_0" } else { "C2_CP_Tuerfreigabe_1" });
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
            self.var_index.get(k).copied().ok_or_else(|| self.model_only_vars.iter().any(|n| n == k))
        });
        match slot {
            Ok(i) => self.state.vars[i as usize] = v,
            Err(true) => self.add_var(name, v),
            Err(false) => return false,
        }
        true
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
        let p = self.ty.program.clone();
        let rear_target_was_open = self.var("doorTarget_23").is_some_and(|target| target > 0.0);
        let rear_force_close_was_active = self.var("bdoor_embtn_cls").is_some_and(|close| close > 0.5);
        let rear_was_open = self.var("doorTarget_23").is_some_and(|target| target > 0.0)
            || self.var("door_2").is_some_and(|door| door > 0.05)
            || self.var("door_3").is_some_and(|door| door > 0.05);
        let coins_before = self.host.change_coins.len();
        let mut fired = self
            .vm
            .run_trigger(&p, name, &mut self.state, &mut self.host);
        if let Some(value) = changer_value(name) {
            let given = &mut self.host.change_coins[coins_before..];
            let each = value / given.len().max(1) as f32;
            for coin in given {
                coin.1 = each;
            }
        }
        // Several Volvo Wright door scripts expose the actual force-close operation as
        // `bus_dooraft1_external_CL`, while the dashboard `bus_dooraftclose` trigger only
        // sounds the button when its handbrake guard rejects the request.  Use that explicit
        // close path when the dashboard request left the rear door open.  Other buses are
        // unaffected because the fallback trigger is only present in those scripts.
        // A toggle pressed while the leaves are already closing is an open request.  The
        // fallback must not immediately undo that request just because the leaves are still
        // physically open for a few frames.
        let toggle_is_reopen_request = name.eq_ignore_ascii_case("bus_dooraft")
            && !rear_target_was_open
            && (rear_force_close_was_active || self.var("doorTarget_23").is_some_and(|target| target > 0.0));
        if (name.eq_ignore_ascii_case("bus_dooraftclose")
            || (name.eq_ignore_ascii_case("bus_dooraft") && !toggle_is_reopen_request))
            && rear_was_open
            && (self.var("doorTarget_23").is_some_and(|target| target > 0.0)
                || self.var("door_2").is_some_and(|door| door > 0.05)
                || self.var("door_3").is_some_and(|door| door > 0.05))
            && p.trigger("bus_dooraft1_external_CL").is_some()
        {
            fired |= self.vm.run_trigger(
                &p,
                "bus_dooraft1_external_CL",
                &mut self.state,
                &mut self.host,
            );
        }
        fired
    }

    /// The script variables as `names` would leave them, run one after another, with the
    /// vehicle left exactly as it was: its variables, the machine's random numbers, the
    /// sounds and messages the triggers asked for.
    pub fn trial_triggers(&mut self, names: &[&str]) -> Vec<f32> {
        let (state, vm) = (self.state.clone(), self.vm.clone());
        let (fired, fired_files, messages, time_written) = (self.host.fired_triggers.len(), self.host.fired_file_triggers.len(), self.host.messages.clone(), self.host.time_written);
        let last_message = self.host.last_message.clone();
        let fired_vars = self.host.fired_trigger_vars.len();
        let network_sounds = self.host.network_sound_triggers.len();
        for n in names {
            self.trigger(n);
        }
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

    /// Refuel until the tank stops filling. Returns the tank content the script reports.
    pub fn refuel(&mut self) -> Option<f32> {
        let mut last = f32::NEG_INFINITY;
        for _ in 0..600 {
            if !self.service("veh_tank", 1.0) {
                return None;
            }
            let now = self.var("engine_tank_content")?;
            if now <= last + 1e-3 {
                break;
            }
            last = now;
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

fn changer_value(name: &str) -> Option<f32> {
    let name = name.to_ascii_lowercase();
    if let Some(cents) = name.strip_prefix("cashdesk_change_").filter(|c| c.len() == 4) {
        return Some(cents.parse::<u32>().ok()? as f32 / 100.0);
    }
    let (units, cents) = name.strip_prefix("cashdesk_changer_")?.split_once('_').filter(|(_, c)| c.len() == 2)?;
    Some(units.parse::<u32>().ok()? as f32 + cents.parse::<u32>().ok()? as f32 / 100.0)
}

#[cfg(test)]
mod tests {
    use super::changer_value;

    #[test]
    fn a_change_key_names_its_value() {
        assert_eq!(changer_value("cashdesk_changer_0_50"), Some(0.5));
        assert_eq!(changer_value("Cashdesk_Changer_15_00"), Some(15.0));
        assert_eq!(changer_value("cashdesk_changer_0_50_off"), None);
        assert_eq!(changer_value("cashdesk_changer_0_5"), None);
        assert_eq!(changer_value("ticket_give"), None);
        assert_eq!(changer_value("cashdesk_change_0500"), Some(5.0));
        assert_eq!(changer_value("cashdesk_change_1000_off"), None);
    }
}
