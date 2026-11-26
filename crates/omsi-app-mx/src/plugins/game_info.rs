use super::*;

/// What the game is doing, for `omsi.info()`.
pub(crate) fn game_info(app: &crate::App) -> Vec<(&'static str, InfoValue)> {
    use InfoValue::{Bool, Num, Text};
    let mut v: Vec<(&'static str, InfoValue)> = Vec::new();
    v.push((
        "map",
        Text(
            app.world
                .as_ref()
                .map(|w| w.global.name.clone())
                .unwrap_or_default(),
        ),
    ));
    v.push(("clock", Num(app.clock.time)));
    v.push(("day", Num(app.clock.day_of_year as f64)));
    v.push(("year", Num(app.clock.year as f64)));
    v.push(("view", Text(app.view.clone())));
    v.push(("paused", Bool(app.paused)));
    v.push(("on_foot", Bool(app.session.on_foot.is_some())));
    v.push(("multiplayer", Bool(app.net.lan.is_some())));
    // the situation the game started from (the launcher's "continue": `laststn.osn`)
    // (relative to the OMSI folder, `/`-separated, whether the launcher passed it absolute or not)
    if let Some(s) = app.args.situation.as_ref() {
        let p = std::path::Path::new(s);
        let rel = p.strip_prefix(&app.args.root).unwrap_or(p);
        v.push(("situation", Text(rel.to_string_lossy().replace('\\', "/"))));
    }
    // this session's, as the personnel file counts them
    v.push(("crashes", Num(app.session.career.crashes[0] as f64)));
    v.push(("heavy_crashes", Num(app.session.career.crashes[3] as f64)));
    v.push(("pedestrians_hit", Num(app.session.career.crashes[1] as f64)));
    if let Some(t) = app.session.traffic.as_ref() {
        v.push(("traffic", Num(t.cars.len() as f64)));
    }
    if let Some(w) = app.world.as_ref() {
        v.push((
            "map_path",
            Text(w.global.path.to_string_lossy().into_owned()),
        ));
    }
    v.push(("version", Text(crate::startup::VERSION.to_string())));
    if let Some(p) = app.player.as_ref() {
        let veh = &p.vehicle;
        v.push(("speed", Num(veh.physics.velocity_kmh().abs() as f64)));
        v.push(("delay", Num(veh.host.tt_delay as f64)));
        // Tile coordinates from global.cfg and metres within the tile (x east, y north).
        let ((tx, ty), (lx, ly)) = omsi_map::world_to_tile_local(veh.position.x, veh.position.y);
        v.push(("tile_x", Num(tx as f64)));
        v.push(("tile_y", Num(ty as f64)));
        v.push(("tile_pos_x", Num(lx)));
        v.push(("tile_pos_y", Num(ly)));
        v.push(("heading", Num(veh.heading.rem_euclid(360.0))));
        v.push((
            "vehicle_manufacturer",
            Text(veh.ty.def.manufacturer.trim().to_string()),
        ));
        v.push((
            "vehicle_model",
            Text(veh.ty.def.type_name.trim().to_string()),
        ));
        // the terminus the bus shows (the hof entry its scripts chose; none for an
        // `[addterminus_allexit]` one), which is not always the timetable's
        let shown = match (veh.var("target_index_int"), veh.host.hof.as_ref()) {
            (Some(i), Some(hof)) if i.is_finite() && i >= 0.0 => hof
                .termini
                .get(i.round() as usize)
                .filter(|t| !t.all_exit)
                .map(|t| t.texture_id.trim().to_string()),
            _ => None,
        };
        v.push(("destination", Text(shown.unwrap_or_default())));
        v.push((
            "passengers",
            Num(app.session.humans.as_ref().map(|h| h.riding()).unwrap_or(0) as f64),
        ));
    }
    if let Some(d) = app.session.duty.as_ref() {
        v.push(("line", Text(d.line.trim().to_string())));
        v.push(("tour", Text(d.tour.trim().to_string())));
        if let Some(trip) = d.trips.get(d.trip_index) {
            v.push(("trip", Num(d.trip_index as f64 + 1.0)));
            v.push(("trips", Num(d.trips.len() as f64)));
            v.push(("terminus", Text(trip.terminus.trim().to_string())));
            v.push(("trip_name", Text(trip.name.trim().to_string())));
            v.push(("stops", Num(trip.stops.len() as f64)));
            // the bus reached the trip's last stop: the trip is over, though the duty moves on to
            // the next one only a minute before it leaves
            v.push(("trip_done", Bool(d.trip_done())));
            let bus = app.player.as_ref().map(|p| p.vehicle.position);
            let next_calling = omsi_sim::timetable_run::next_calling_stop(&trip.stops, d.next_stop);
            stop_info(
                &mut v,
                &trip.stops,
                next_calling,
                d.at_stop() && next_calling == d.next_stop,
                bus,
            );
            if let Some(stop) = trip.stops.get(next_calling) {
                if let Some(distance) = app
                    .menus
                    .navigator
                    .as_ref()
                    .and_then(|n| n.next_stop_distance_for(stop.object_id))
                {
                    v.push(("next_stop_distance_nav", Num(distance)));
                }
            }
        }
    }
    v
}

/// `omsi.info()`'s keys of the next stop and the one before it: `stops` the trip's, `next`
/// the index of the next one, `bus` where the player's bus is.
pub(super) fn stop_info(
    v: &mut Vec<(&'static str, InfoValue)>,
    stops: &[crate::schedule::PlannedStop],
    next: usize,
    at_stop: bool,
    bus: Option<glam::DVec3>,
) {
    use InfoValue::{Bool, Num, Text};
    // straight-line metres from the bus, where the stop's place is known
    let distance = |s: &crate::schedule::PlannedStop| {
        let (bus, stop) = (bus?, s.position?);
        Some((stop.x - bus.x).hypot(stop.y - bus.y))
    };
    if let Some(s) = stops.get(next) {
        v.push(("next_stop", Text(s.name.trim().to_string())));
        v.push(("next_stop_number", Num(next as f64 + 1.0)));
        v.push(("next_stop_arrival", Num(s.arr)));
        v.push(("next_stop_departure", Num(s.dep)));
        // the map's object ID of the stop, as the timetable and the map files name it:
        // outside tools match the stop by it, the name can occur twice
        v.push(("next_stop_id", Num(s.object_id as f64)));
        v.push(("at_stop", Bool(at_stop)));
        if let Some(m) = distance(s) {
            v.push(("next_stop_distance", Num(m)));
        }
    }
    // the last stop before the next one the trip calls at (passing stations left out)
    if let Some(s) = stops.iter().take(next).rev().find(|s| s.calls()) {
        v.push(("previous_stop", Text(s.name.trim().to_string())));
        v.push(("previous_stop_id", Num(s.object_id as f64)));
        if let Some(m) = distance(s) {
            v.push(("previous_stop_distance", Num(m)));
        }
    }
}
