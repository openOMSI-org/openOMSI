//! The coupled cars' own variables (a tram's rear cab, whose scripts keep switches of their
//! own; see `omsi_sim::vehicle::car_state`) for the copies the others draw. Ours go on a
//! channel of their own (`omsi_net::vars::CAR_VARS_MAGIC`) as the leading vehicle's go on
//! theirs; theirs are set on the cars of our copy of their train before its frame and after,
//! as the leading vehicle's are pinned. Only for a copy whose cars have the same tables as the
//! owner's (the same train): `CarTables::hash` names them, and indices are never guessed.

use super::LanGame;
use crate::sync::var_table;
use omsi_net::LanSession;
use omsi_sim::VehicleInstance;
use std::sync::Arc;

/// What came of a car's variables, by (car index, variable id in its program).
pub(super) type Synced<T> = hashbrown::HashMap<(usize, u32), T>;

/// The tables of a train's cars that run scripts of their own, in the train's order: one
/// list of every such car's variables (each car's by its names, sorted), made again only
/// when the train's cars change.
#[derive(Default)]
pub(super) struct CarTables {
    /// (car index, its program) the tables were made for.
    key: Vec<(usize, usize)>,
    pub(super) hash: u32,
    /// (car index, variable id in its program), in the order they go on the wire.
    floats: Vec<(usize, u32)>,
    strings: Vec<(usize, u32)>,
}

/// The cars of `v` with scripts of their own: (index among its parts, its program).
fn own_cars(v: &VehicleInstance) -> impl Iterator<Item = (usize, usize)> + '_ {
    v.trailers
        .iter()
        .enumerate()
        .filter_map(|(k, t)| Some((k, Arc::as_ptr(t.own_run()?.program()) as usize)))
}

impl CarTables {
    fn of(v: &VehicleInstance) -> CarTables {
        let mut t = CarTables {
            hash: 0x811c_9dc5,
            ..Default::default()
        };
        for (k, t_run) in v.trailers.iter().enumerate() {
            let Some(run) = t_run.own_run() else { continue };
            let table = var_table(run.program());
            for b in (k as u32)
                .to_le_bytes()
                .into_iter()
                .chain(table.hash.to_le_bytes())
            {
                t.hash = (t.hash ^ b as u32).wrapping_mul(0x0100_0193);
            }
            t.key.push((k, Arc::as_ptr(run.program()) as usize));
            t.floats.extend(table.floats.iter().map(|&id| (k, id)));
            t.strings.extend(table.strings.iter().map(|&id| (k, id)));
        }
        t
    }

    /// Whether these are still the tables of `v`'s cars.
    fn fits(&self, v: &VehicleInstance) -> bool {
        own_cars(v).eq(self.key.iter().copied())
    }
}

/// Our train's cars' own variables to the others (none for a vehicle without such cars).
pub(super) fn send(lan: &mut LanSession, game: &mut LanGame, p: &VehicleInstance, dt: f32) {
    if !game.my_cars.fits(p) {
        game.my_cars = CarTables::of(p);
    }
    let t = &game.my_cars;
    if t.key.is_empty() {
        return;
    }
    let value = |&(k, id): &(usize, u32)| {
        p.trailers[k]
            .own_run()
            .and_then(|r| r.vars().get(id as usize).copied())
            .unwrap_or(0.0)
    };
    game.car_floats.clear();
    game.car_floats.extend(t.floats.iter().map(value));
    game.car_strings.resize(t.strings.len(), String::new());
    for (out, &(k, id)) in game.car_strings.iter_mut().zip(&t.strings) {
        match p.trailers[k]
            .own_run()
            .and_then(|r| r.str_vars().get(id as usize))
        {
            Some(s) => out.clone_from(s),
            None => out.clear(),
        }
    }
    lan.send_car_vars(t.hash, &game.car_floats, &game.car_strings, dt);
}

/// The others' cars' own variables as they came, kept on our copies of their trains.
pub(super) fn receive(lan: &mut LanSession, game: &mut LanGame) {
    for v in lan.take_car_vars() {
        let Some(rv) = game.remotes.get_mut(&v.id) else {
            continue;
        };
        if rv.stand_in || rv.fallback {
            continue;
        }
        take(
            &mut rv.cars,
            &rv.vehicle,
            (&mut rv.car_synced, &mut rv.car_synced_strings),
            v,
        );
    }
}

/// Keep what came (`v`) of the cars of the train `vehicle` copies, where its cars' tables
/// (`cars`, made again when they changed) are the owner's.
fn take(
    cars: &mut CarTables,
    vehicle: &VehicleInstance,
    (floats, strings): (&mut Synced<f32>, &mut Synced<String>),
    v: omsi_net::vars::VarsIn,
) {
    if !cars.fits(vehicle) {
        *cars = CarTables::of(vehicle);
        floats.clear();
        strings.clear();
    }
    if cars.key.is_empty() || cars.hash != v.table {
        return;
    }
    for (i, x) in v.floats {
        if let (Some(&at), true) = (cars.floats.get(i as usize), x.is_finite()) {
            floats.insert(at, x);
        }
    }
    for (i, text) in v.strings {
        if let Some(&at) = cars.strings.get(i as usize) {
            if omsi_net::remote_text::remote_string_ok(&text, false) {
                strings.insert(at, text);
            }
        }
    }
}

/// Set what came of the owner's cars on our copy of their train (see the module).
/// (`floats`, `strings`: a `RemoteVehicle`'s `car_synced`, `car_synced_strings`.)
pub(super) fn apply(vehicle: &mut VehicleInstance, floats: &Synced<f32>, strings: &Synced<String>) {
    for (&(k, id), &x) in floats {
        if let Some(run) = vehicle.trailers.get_mut(k).and_then(|t| t.own_run_mut()) {
            run.set_slot(id, x);
        }
    }
    for (&(k, id), text) in strings {
        if let Some(run) = vehicle.trailers.get_mut(k).and_then(|t| t.own_run_mut()) {
            run.set_str_slot(id, text);
        }
    }
}

#[cfg(test)]
#[path = "car_sync_tests.rs"]
mod tests;
