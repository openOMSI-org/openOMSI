//! The duty, the timetable and the map for the plugin API (`duty.*`, `timetable.*`,
//! `map.*`).

use crate::App;
use omsi_plugin as op;

fn stop(s: &crate::schedule::PlannedStop) -> op::Stop {
    op::Stop { id: s.object_id, name: s.name.trim().to_string(), arr: s.arr, dep: s.dep, pos: s.position.map(|p| [p.x, p.y, p.z]), stops: s.stops, closed: s.closed.is_some() }
}

fn trip(t: &omsi_sim::timetable_run::PlannedTrip) -> op::Trip {
    op::Trip { name: t.name.trim().to_string(), line: t.line.trim().to_string(), terminus: t.terminus.trim().to_string(), departure: t.departure, end: t.end, stops: t.stops.iter().map(stop).collect() }
}

pub(crate) fn duty(app: &App) -> Option<op::Duty> {
    let d = app.session.duty.as_ref()?;
    let t = d.trips.get(d.trip_index)?;
    Some(op::Duty {
        line: d.line.trim().to_string(),
        tour: d.tour.trim().to_string(),
        trip: d.trip_index,
        trips: d.trips.len(),
        next: d.next_stop,
        at_stop: d.at_stop(),
        trip_done: d.trip_done(),
        delay: d.delay(app.clock.time),
        current: trip(t),
    })
}

pub(crate) fn trips(app: &App) -> Vec<op::Trip> {
    app.session.duty.as_ref().map(|d| d.trips.iter().map(trip).collect()).unwrap_or_default()
}

/// The duty gives up its next stop (the game menu's "Skip the next stop").
pub(crate) fn skip_next(app: &mut App) -> Option<String> {
    let d = app.session.duty.as_mut()?;
    let name = d.skip_next()?;
    log::info!("duty: stop '{name}' skipped by a plugin, next stop {}", d.next_stop);
    if let Some(p) = app.player.as_mut() {
        let (trip, k) = d.trip_for_ibis();
        p.ibis_to_stop(trip, k);
    }
    Some(name.trim().to_string())
}

/// Close stops of the duty (a diversion): the navigator strikes them through, and the duty
/// goes on past them.
pub(crate) fn set_closed_stops(app: &mut App, stops: &[String], via: &str) -> bool {
    let Some(d) = app.session.duty.as_mut() else { return false };
    let closed = |d: &crate::schedule::PlayerDuty| d.trips.iter().flat_map(|t| t.stops.iter().map(|s| s.closed.clone())).collect::<Vec<_>>();
    let before = closed(d);
    let n = d.set_closed_stops(stops, via);
    // (a plugin may say the same every frame: the log hears of a change only)
    if closed(d) != before {
        log::info!("duty: {n} stop(s) closed by a plugin ({})", stops.join(", "));
    }
    true
}

/// Make a stop of the trip the next one: handed to the duty as a bus page hands it.
pub(crate) fn skip_to(app: &mut App, index: usize) -> bool {
    let n = app.session.duty.as_ref().and_then(|d| d.trips.get(d.trip_index)).map_or(0, |t| t.stops.len());
    match app.player.as_mut() {
        Some(p) if index < n => {
            p.html_next_stop = Some(index);
            true
        }
        _ => false,
    }
}

pub(crate) fn start(app: &mut App, line: &str, tour: &str, trip: usize, stop: usize) -> Result<(), String> {
    if app.net.lan.as_ref().is_some_and(|l| l.role == omsi_net::Role::Client) {
        return Err("in a LAN session the host gives the duties".into());
    }
    let sch = crate::duty_book::timetable(app).ok_or("the map has no timetable")?;
    let known = sch.data.lines.iter().find(|l| l.name.trim().eq_ignore_ascii_case(line.trim())).and_then(|l| l.tours.iter().find(|t| t.number.trim().eq_ignore_ascii_case(tour.trim())));
    if known.is_none() {
        return Err(format!("no tour {tour} of line {line}"));
    }
    if trip >= sch.tour_trip_count(line, tour).max(1) {
        return Err(format!("the tour has {} trips", sch.tour_trip_count(line, tour)));
    }
    crate::game_lists::start_duty_at(app, line, tour, trip, stop);
    match app.session.duty.as_ref() {
        Some(d) if d.line.trim().eq_ignore_ascii_case(line.trim()) && d.tour.trim().eq_ignore_ascii_case(tour.trim()) => Ok(()),
        _ => Err(app.service_msg.as_ref().map(|m| m.0.clone()).unwrap_or_else(|| "the duty could not be taken".into())),
    }
}

/// Plugin `owner` leaves the player's bus's IBIS to the driver, or gives it back: the game
/// types the duty's trips by itself while no plugin holds it (`Player::ibis_holds`).
pub(crate) fn set_auto_ibis(app: &mut App, owner: u64, on: bool) -> bool {
    let Some(p) = app.player.as_mut() else { return false };
    if on {
        p.ibis_holds.remove(&owner);
    } else {
        p.ibis_holds.insert(owner);
    }
    true
}

/// The trip under way typed into the player's bus's IBIS now, as Shift+U types it; the
/// duty's next trips follow by themselves unless the IBIS is left to the driver.
pub(crate) fn type_ibis(app: &mut App) -> bool {
    let (Some(d), Some(p)) = (app.session.duty.as_ref(), app.player.as_mut()) else { return false };
    let (trip, stop) = d.trip_for_ibis();
    let holds = std::mem::take(&mut p.ibis_holds);
    p.set_duty_destination(trip, stop);
    p.ibis_holds = holds;
    true
}

pub(crate) fn finish(app: &mut App) -> bool {
    if app.session.duty.is_none() {
        return false;
    }
    crate::game_lists::end_duty(app);
    true
}

pub(crate) fn lines(app: &App) -> Vec<op::Line> {
    let Some(sch) = crate::duty_book::timetable(app) else { return Vec::new() };
    sch.data
        .lines
        .iter()
        .map(|l| op::Line {
            name: l.name.trim().to_string(),
            user_allowed: l.user_allowed,
            tours: l.tours.iter().map(|t| op::Tour { number: t.number.trim().to_string(), today: sch.tour_available(t), trips: sch.tour_trip_count(&l.name, &t.number) }).collect(),
        })
        .collect()
}

pub(crate) fn tour_stops(app: &App, line: &str, tour: &str, trip: Option<usize>) -> Vec<(usize, usize, String, f64)> {
    let Some(sch) = crate::duty_book::timetable(app) else { return Vec::new() };
    match trip {
        Some(t) => sch.tour_trip_stops(line, tour, t),
        None => sch.tour_stops(line, tour),
    }
}

pub(crate) fn ai_buses(app: &App) -> Vec<op::AiBus> {
    let (Some(s), Some(t)) = (app.session.schedule.as_ref(), app.session.traffic.as_ref()) else { return Vec::new() };
    s.ai_bus_rows(&t.sim)
        .into_iter()
        .map(|r| op::AiBus { id: r.id, line: r.line, tour: r.tour, trip: r.trip, terminus: r.terminus, departure: r.depart, next_stop_id: r.next_stop_id, at_stop: r.at_stop, trip_done: r.trip_done, delay: r.delay_s, x: r.x, y: r.y, number: r.number })
        .collect()
}

// --- the map -----------------------------------------------------------------------------

pub(crate) fn map_info(app: &App) -> Option<op::MapInfo> {
    let w = app.world.as_ref()?;
    Some(op::MapInfo {
        name: w.global.name.clone(),
        friendly_name: w.global.friendly_name.clone(),
        path: w.global.path.to_string_lossy().into_owned(),
        left_hand_traffic: w.global.left_hand_traffic,
        tile_size: omsi_map::tile_size(),
    })
}

pub(crate) fn tiles(app: &App) -> Vec<op::Tile> {
    let Some(w) = app.world.as_ref() else { return Vec::new() };
    let loaded = w.loaded_tiles();
    w.map_tiles()
        .into_iter()
        .map(|(_, x, y, file)| op::Tile { x, y, file: file.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default(), loaded: loaded.contains(&(x, y)) })
        .collect()
}

pub(crate) fn stops(app: &App) -> Vec<op::MapStop> {
    let Some(w) = app.world.as_ref() else { return Vec::new() };
    let list = w.data.peek().lists.bus_stops.clone();
    list.into_iter().map(|(id, p, h, name)| op::MapStop { id, name, pos: [p.x, p.y, p.z, h] }).collect()
}

pub(crate) fn object(app: &App, id: i64) -> Option<[f64; 4]> {
    let w = app.world.as_ref()?;
    let (p, r) = *w.data.peek().objects.get(&id)?;
    Some([p.x, p.y, p.z, r[0]])
}

pub(crate) fn objects_near(app: &App, x: f64, y: f64, r: f64) -> Vec<(i64, [f64; 4])> {
    let Some(w) = app.world.as_ref() else { return Vec::new() };
    let data = w.data.peek();
    let all = &data.objects;
    all.iter().filter(|(_, (p, _))| (p.x - x).hypot(p.y - y) <= r).map(|(id, (p, rot))| (*id, [p.x, p.y, p.z, rot[0]])).collect()
}

pub(crate) fn entrypoints(app: &App) -> Vec<op::Entry> {
    let Some(w) = app.world.as_ref() else { return Vec::new() };
    w.global
        .entry_points
        .iter()
        .enumerate()
        .map(|(i, ep)| op::Entry { index: i, name: ep.name.trim().to_string(), pos: w.entry_point_place(ep).map(|(p, r)| [p.x, p.y, p.z, r[0]]) })
        .collect()
}

/// Whether this game may move the player's bus (not a LAN client, a bus to move).
fn may_move(app: &App) -> bool {
    app.player.is_some() && !app.net.lan.as_ref().is_some_and(|l| l.role == omsi_net::Role::Client)
}

pub(crate) fn teleport(app: &mut App, at: [f64; 3], heading: f64) -> bool {
    if !may_move(app) || !at.iter().all(|v| v.is_finite()) {
        return false;
    }
    crate::admin::teleport(app, glam::DVec3::new(at[0], at[1], at[2]), heading);
    app.service_event("teleport", "plugin", None);
    true
}

pub(crate) fn teleport_entry(app: &mut App, i: usize) -> bool {
    if !may_move(app) {
        return false;
    }
    let Some(w) = app.world.clone() else { return false };
    let Some(ep) = w.global.entry_points.get(i) else { return false };
    // (a start point on a tile not read yet: the map's index places it)
    let place = w.entry_point_place(ep).or_else(|| {
        w.index();
        w.entry_point_place(ep)
    });
    let Some((pos, rot)) = place else { return false };
    teleport(app, [pos.x, pos.y, pos.z], rot[0])
}

pub(crate) fn place_on_road(app: &mut App, x: f64, y: f64) -> bool {
    if !may_move(app) {
        return false;
    }
    let before = app.player.as_ref().map(|p| p.vehicle.position);
    app.place_bus_at(glam::DVec2::new(x, y));
    app.player.as_ref().map(|p| p.vehicle.position) != before
}

pub(crate) fn lane(app: &App, x: f64, y: f64) -> Option<op::Lane> {
    let t = app.session.traffic.as_ref()?;
    let z = app.world.as_ref().and_then(|w| w.ground_height(x, y)).unwrap_or(0.0);
    let (i, _, dist) = t.net.nearest_lane(glam::DVec3::new(x, y, z), omsi_sim::traffic::LaneKind::Street)?;
    let l = &t.net.lanes[i];
    Some(op::Lane { index: i, distance: dist, speed_limit_kmh: l.speed_limit_kmh, name: l.name.clone(), has_light: l.traffic_light.is_some() })
}

/// The complete source tour, aligned with the active duty even when only one trip was picked.
pub(crate) fn tour(
    app: &App,
    line: &str,
    tour: &str,
    selected: bool,
) -> Result<op::api::Value, String> {
    use op::api::Value as V;
    fn rec(p: Vec<(&str, V)>) -> V {
        V::Map(p.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }
    let sch = crate::duty_book::timetable(app).ok_or("no timetable")?;
    let w = app.world.as_ref().ok_or("no world")?.data();
    let plan = sch.tour_plan(w.as_ref(), line, tour)?;
    let offset = if selected {
        tour_time_offset(&plan, app.session.duty.as_ref())
    } else {
        0.0
    };
    let entries = plan
        .trips
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let t = &p.trip;
            let stops = t
                .stops
                .iter()
                .map(|s| {
                    let mut fields = vec![
                        ("id", V::Int(s.object_id)),
                        ("name", s.name.clone().into()),
                        ("arrival", (s.arr + offset).into()),
                        ("departure", (s.dep + offset).into()),
                        ("stops", s.stops.into()),
                    ];
                    if let Some(p) = s.position {
                        fields.extend([("x", p.x.into()), ("y", p.y.into()), ("z", p.z.into())]);
                    }
                    rec(fields)
                })
                .collect();
            let profile = p.profile.as_ref().map(|f| {
                rec(vec![
                    ("name", f.name.clone().into()),
                    ("factor", (f.factor as f64).into()),
                ])
            });
            rec(vec![
                ("number", (i + 1).into()),
                ("source_number", (p.source_index + 1).into()),
                ("name", t.name.clone().into()),
                ("line", t.line.clone().into()),
                ("terminus", t.terminus.clone().into()),
                ("track", p.track.clone().into()),
                ("reverse", p.reverse.into()),
                ("profile_index", p.profile_index.into()),
                ("profile", profile.into()),
                ("departure", (t.departure + offset).into()),
                ("arrival", (t.end + offset).into()),
                ("stops", V::List(stops)),
            ])
        })
        .collect();
    Ok(rec(vec![
        ("line", plan.line.into()),
        ("number", plan.number.into()),
        ("ai_group", plan.ai_group.into()),
        ("extra", plan.extra.into()),
        ("today", plan.available.into()),
        ("time_offset", offset.into()),
        ("trips", V::List(entries)),
    ]))
}

fn tour_time_offset(
    plan: &omsi_sim::timetable_run::TourPlan,
    duty: Option<&omsi_sim::timetable_run::PlayerDuty>,
) -> f64 {
    duty.and_then(|d| {
        plan.trips
            .get(d.first_trip + d.trip_index)
            .zip(d.trips.get(d.trip_index))
    })
    .map_or(0.0, |(raw, active)| active.departure - raw.trip.departure)
}

#[cfg(test)]
mod tour_tests {
    use super::*;
    use omsi_sim::timetable_run::{PlannedTrip, PlayerDuty, TourPlan, TourPlanTrip};
    fn planned(departure: f64) -> PlannedTrip {
        PlannedTrip {
            name: "N1".into(),
            line: "N1".into(),
            terminus: "End".into(),
            departure,
            end: departure + 60.0,
            stops: vec![],
        }
    }
    #[test]
    fn a_single_selected_trip_aligns_its_full_tour_after_midnight() {
        let entry = |departure| TourPlanTrip {
            source_index: 0,
            track: String::new(),
            reverse: false,
            profile_index: 0,
            profile: None,
            trip: planned(departure),
        };
        let plan = TourPlan {
            line: "N1".into(),
            number: "1".into(),
            ai_group: String::new(),
            extra: String::new(),
            available: true,
            trips: vec![entry(85800.0), entry(86700.0)],
        };
        let duty = PlayerDuty::new("N1".into(), "1".into(), vec![planned(300.0)], 0, 1, true);
        assert_eq!(tour_time_offset(&plan, Some(&duty)), -86400.0);
        assert_eq!(
            plan.trips[0].trip.departure + tour_time_offset(&plan, Some(&duty)),
            -600.0
        );
        assert_eq!(
            plan.trips[1].trip.departure + tour_time_offset(&plan, Some(&duty)),
            300.0
        );
        assert_eq!(tour_time_offset(&plan, None), 0.0);
    }
}
