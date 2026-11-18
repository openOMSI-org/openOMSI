//! The game's side of the player's depot: a depot bus starts and ends with its own wear.

use omsi_launcher_lib::depot;
use omsi_sim::VehicleInstance;

/// Puts back the wear bus `id` was left with (a first outing keeps the bus as it comes).
pub(crate) fn restore(v: &mut VehicleInstance, id: &str) {
    let d = match depot::load() {
        Ok(d) => d,
        Err(e) => return log::warn!("depot: {e:#}"),
    };
    let Some(b) = d.get(id) else {
        return log::warn!("depot bus {id} not found in {}", depot::path().display());
    };
    if !is_this_bus(v, &b.bus) {
        return log::warn!("depot bus {} is a {}, not this vehicle: its wear is left alone", b.name, b.bus);
    }
    if b.wear.is_empty() {
        return log::info!("depot bus {} ({id}): first outing", b.name);
    }
    if let Some(km) = b.odometer() {
        v.set_odometer_km(km);
    }
    let kept: Vec<(String, f32)> = b.wear.iter().filter(|(n, x)| depot::keeps(n, *x)).cloned().collect();
    let (n, _) = v.restore_script_state(&kept, &[]);
    log::info!("depot bus {} ({id}): {n} of {} wear variables restored", b.name, b.wear.len());
}

/// The vehicle's wear variables as they are now.
pub(crate) fn wear(v: &VehicleInstance) -> Vec<(String, f32)> {
    let mut out: Vec<(String, f32)> = v
        .ty
        .program
        .var_names
        .iter()
        .enumerate()
        .filter(|(_, n)| depot::is_wear_var(n) && !n.eq_ignore_ascii_case(depot::DIRT))
        .map(|(i, n)| (n.clone(), v.state.vars.get(i).copied().unwrap_or(0.0)))
        .filter(|(n, x)| depot::keeps(n, *x))
        .collect();
    for k in ["kmcounter_km", "kmcounter_m"] {
        if !out.iter().any(|(n, _)| n.eq_ignore_ascii_case(k)) {
            out.extend(v.var(k).map(|x| (k.to_string(), x)));
        }
    }
    out.push((depot::DIRT.into(), v.dirt));
    out
}

pub(crate) struct DepotSwitch {
    pub(crate) prev: (Option<String>, Option<String>),
    pub(crate) old_uid: u64,
    pub(crate) record: Option<(String, depot::Outing, Vec<(String, f32)>)>,
    pub(crate) base: depot::Outing,
}

pub(crate) fn record_wear(id: &str, outing: &depot::Outing, wear: Vec<(String, f32)>) {
    match depot::record(id, outing, wear) {
        Ok(true) => log::info!("depot bus {id}: {:.1} km and its wear recorded", outing.metres / 1000.0),
        Ok(false) => log::warn!("depot bus {id} is no longer in the depot"),
        Err(e) => log::warn!("depot: {e:#}"),
    }
}

/// Adds the session to bus `id`, if the player still drives it.
pub(crate) fn record(v: &VehicleInstance, id: &str, outing: &depot::Outing) {
    let bus = depot::load().ok().and_then(|d| d.get(id).map(|b| b.bus.clone()));
    match bus {
        Some(b) if !is_this_bus(v, &b) => log::info!("depot bus {id}: the session ended in another vehicle, not recorded"),
        _ => record_wear(id, outing, wear(v)),
    }
}

pub(crate) fn is_this_bus(v: &VehicleInstance, bus: &str) -> bool {
    same_file(&v.ty.def.path.to_string_lossy(), bus)
}

fn same_file(path: &str, bus: &str) -> bool {
    let norm = |s: &str| s.replace('\\', "/").to_ascii_lowercase();
    norm(path).ends_with(&norm(bus.trim_start_matches(['/', '\\'])))
}

pub(crate) fn bus_of(id: &str) -> Option<String> {
    depot::load().ok().and_then(|d| d.get(id).map(|b| b.bus.clone()))
}

pub(crate) fn drives(vehicle_path: &str, depot_bus: Option<&str>) -> bool {
    depot_bus.is_some_and(|b| same_file(vehicle_path, b))
}

pub(crate) fn outing_since(c: &crate::career::Career, args: &crate::cli::Args, base: &depot::Outing) -> depot::Outing {
    depot::Outing {
        map: args.map.clone(),
        line: args.line.clone().unwrap_or_default(),
        tour: args.tour.clone().unwrap_or_default(),
        metres: c.metres - base.metres,
        seconds: c.seconds - base.seconds,
        stops: c.stops[0] - base.stops,
        tickets: c.tickets.0 - base.tickets,
        takings: c.tickets.1 - base.takings,
        crashes: c.crashes[0] - base.crashes,
    }
}

pub(crate) fn shift_base(base: &mut depot::Outing, old: &crate::career::Career, new: &crate::career::Career) {
    base.metres += new.metres - old.metres;
    base.seconds += new.seconds - old.seconds;
    base.stops += new.stops[0] - old.stops[0];
    base.tickets += new.tickets.0 - old.tickets.0;
    base.takings += new.tickets.1 - old.tickets.1;
    base.crashes += new.crashes[0] - old.crashes[0];
}

pub(crate) fn switch_landed(loading: bool, old: Option<u64>, now: Option<(u64, bool)>) -> Option<bool> {
    if loading {
        return None;
    }
    Some(matches!(now, Some((uid, new_bus)) if Some(uid) != old && new_bus))
}

pub(crate) fn listed() -> Vec<depot::DepotBus> {
    depot::load().map(|d| d.buses).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::career::Career;

    #[test]
    fn the_outing_goes_on_across_a_driver_change() {
        let mut args = <crate::cli::Args as clap::Parser>::parse_from(["openomsi"]);
        args.map = "maps/Grundorf".into();
        let mut old = Career::default();
        (old.metres, old.seconds, old.stops[0], old.tickets, old.crashes[0]) = (5000.0, 600.0, 4, (3, 6.0), 1);
        let mut base = depot::Outing { metres: 1000.0, seconds: 100.0, stops: 1, tickets: 1, takings: 2.0, crashes: 0, ..Default::default() };
        let mut next = Career::default();
        next.seconds = old.seconds;
        next.go_on_from(&mut old);
        shift_base(&mut base, &old, &next);
        next.metres += 2000.0;
        next.stops[0] += 2;
        let o = outing_since(&next, &args, &base);
        assert_eq!((o.metres, o.seconds, o.stops, o.tickets, o.takings, o.crashes), (6000.0, 500.0, 5, 2, 4.0, 1));
        assert_eq!(o.map, "maps/Grundorf");
    }

    #[test]
    fn the_old_depot_bus_is_recorded_only_from_itself() {
        assert!(drives("/omsi/Vehicles/MAN_NL202/MAN_NL202.bus", Some("Vehicles\\MAN_NL202\\MAN_NL202.bus")));
        assert!(!drives("/omsi/Vehicles/MAN_SD200/MAN_SD80.bus", Some("Vehicles/MAN_NL202/MAN_NL202.bus")), "another bus taken meanwhile");
        assert!(!drives("/omsi/Vehicles/MAN_NL202/MAN_NL202.bus", None), "no longer in the depot");
    }

    #[test]
    fn a_depot_bus_counts_as_taken_only_once_it_is_driven() {
        assert_eq!(switch_landed(true, Some(1), Some((1, false))), None, "still loading");
        assert_eq!(switch_landed(false, Some(1), Some((2, true))), Some(true));
        assert_eq!(switch_landed(false, Some(1), Some((1, false))), Some(false), "the load failed");
        assert_eq!(switch_landed(false, Some(1), Some((3, false))), Some(false), "another vehicle taken meanwhile");
        assert_eq!(switch_landed(false, None, None), Some(false), "on foot, nothing driven");
        assert_eq!(switch_landed(false, None, Some((2, true))), Some(true));
    }
}
