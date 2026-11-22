//! A coupled car's own script state (see `car_run`) outside the car: what a situation saves
//! of it and restores, and what the copies other players see are given of it. A
//! `[scriptshare]` part has none of its own (its variables are the leading vehicle's).

use super::{CarRun, TrailerPart, VehicleInstance};
use omsi_content::situation::SituationCar;
use omsi_script::{Program, VarId};
use std::sync::Arc;

impl CarRun {
    /// The car's own program, whose variables [`CarRun::vars`] and [`CarRun::str_vars`] hold.
    pub fn program(&self) -> &Arc<Program> {
        &self.prog
    }

    /// The values of its program's variables, by their ids in it.
    pub fn vars(&self) -> &[f32] {
        &self.state.vars[..self.prog.var_names.len().min(self.state.vars.len())]
    }

    /// The values of its program's string variables, by their ids in it.
    pub fn str_vars(&self) -> &[String] {
        &self.state.str_vars
    }

    /// Set its program's variable `id` as another game has it: the car's own from then on,
    /// not refreshed from the leading vehicle at its next frame (its scripts and its moving
    /// parts see that game's value). False where it has none.
    pub fn set_slot(&mut self, id: VarId, value: f32) -> bool {
        let i = id as usize;
        let ok = i < self.prog.var_names.len() && i < self.state.vars.len();
        if ok {
            self.state.vars[i] = value;
            if self.local.get(i) == Some(&false) {
                self.local[i] = true;
                self.lead_len = None;
            }
        }
        ok
    }

    /// Set its program's string variable `id`; false where it has none.
    pub fn set_str_slot(&mut self, id: VarId, value: &str) -> bool {
        match self.state.str_vars.get_mut(id as usize) {
            Some(s) => {
                if s != value {
                    *s = value.to_string();
                }
                true
            }
            None => false,
        }
    }

    /// Its own variables by name, as a situation keeps them (`car`: its place among the
    /// parts): those its scripts or its keys stored, not those refreshed from the leading
    /// vehicle, which has them itself.
    fn saved(&self, car: usize) -> SituationCar {
        let own = |slot: usize| {
            self.shared
                .binary_search_by_key(&(slot as VarId), |p| p.0)
                .is_err()
        };
        SituationCar {
            car,
            vars: (self.prog.var_names.iter().zip(self.vars()).enumerate())
                .filter(|(slot, _)| own(*slot))
                .map(|(_, (n, x))| (n.clone(), *x as f64))
                .collect(),
            string_vars: self
                .prog
                .str_var_names
                .iter()
                .cloned()
                .zip(self.str_vars().iter().cloned())
                .collect(),
        }
    }

    /// Take back what [`CarRun::saved`] kept, by name, as the car's own again (what a key
    /// stored is not refreshed from the leading vehicle after the load either): how many
    /// values it took.
    fn restore(&mut self, saved: &SituationCar) -> usize {
        let mut n = 0;
        for (name, x) in &saved.vars {
            n += usize::from(self.set_own(name, *x as f32));
        }
        for (name, x) in &saved.string_vars {
            if let Some(id) = self.prog.str_var(name) {
                n += usize::from(self.set_str_slot(id, x));
            }
        }
        n
    }
}

impl TrailerPart {
    /// Its scripts' own variables, where it runs scripts of its own (not `[scriptshare]`).
    pub fn own_run(&self) -> Option<&CarRun> {
        self.run.as_deref().filter(|r| !r.script_share)
    }

    /// [`TrailerPart::own_run`], to set its values.
    pub fn own_run_mut(&mut self) -> Option<&mut CarRun> {
        self.run.as_deref_mut().filter(|r| !r.script_share)
    }
}

impl VehicleInstance {
    /// The own variables of each coupled car that has them, for a situation: a tram's rear
    /// cab keeps its switches when the game is saved and loaded.
    pub fn car_states(&self) -> Vec<SituationCar> {
        self.trailers
            .iter()
            .enumerate()
            .filter_map(|(k, t)| Some(t.own_run()?.saved(k)))
            .collect()
    }

    /// Take back [`VehicleInstance::car_states`] from a situation: a car whose scripts have
    /// not started yet takes them when they do. How many values were taken now.
    pub fn restore_car_states(&mut self, cars: &[SituationCar]) -> usize {
        let mut n = 0;
        for c in cars {
            let Some(part) = self.trailers.get_mut(c.car) else {
                continue;
            };
            match part.run.as_deref_mut() {
                Some(run) if !run.script_share => n += run.restore(c),
                Some(_) => {}
                None => part.saved = Some(c.clone()),
            }
        }
        n
    }
}

/// See `TrailerPart::saved`: what a car whose scripts start now takes from a situation.
pub(super) fn take_saved(part_saved: &mut Option<SituationCar>, run: &mut CarRun) {
    if let Some(c) = part_saved.take() {
        if !run.script_share {
            run.restore(&c);
        }
    }
}
