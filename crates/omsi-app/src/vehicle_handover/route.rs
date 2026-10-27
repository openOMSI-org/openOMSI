//! Preflight the whole handover before moving ownership out of the player.

use crate::App;
use omsi_sim::ai_traffic::model::BusSetup;
use omsi_sim::ai_traffic::bus_service::{AiTimetable, Stop};
use omsi_sim::traffic::LaneKind;
use omsi_sim::timetable_run::{StopRoute, project_stop, place_stops, add_connectors, bridge_gaps};

pub(super) struct Plan {
    pub lane: usize,
    pub s: f32,
    pub bus: Option<BusSetup>,
    pub departure: Option<usize>,
}

pub(super) fn prepare(app: &mut App) -> Result<Plan, String> {
    let p = app.player.as_ref().ok_or("There is no player bus to hand over")?;
    let traffic = app.session.traffic.as_mut().ok_or("Traffic is not ready")?;
    let pos = p.vehicle.position;
    let heading = p.vehicle.heading;
    let Some(duty) = app.session.duty.as_ref() else {
        let (lane, s, _) = traffic.net.lane_along(pos, heading, LaneKind::Street, 6.0, 35.0)
            .ok_or("No compatible road lane is close enough for AI to drive away")?;
        if !traffic.net.lanes[lane].allows(p.vehicle.ty.def.ai_veh_type) {
            return Err("This road lane does not admit the bus".into());
        }
        let bus = BusSetup {
            route: vec![lane], stops: Vec::new(), number: None, hof: p.vehicle.host.hof.clone(),
            timetable: AiTimetable { line: String::new(), terminus: String::new(), stops: Vec::new() },
        };
        return Ok(Plan { lane, s, bus: Some(bus), departure: None });
    };
    let schedule = app.session.schedule.as_ref().ok_or("The timetable is not ready")?;
    let world = app.world.as_ref().ok_or("The world is not ready")?;
    let departure = schedule.handover_departure(duty).ok_or("The current timetable departure is no longer valid")?;
    let trip = duty.trips.get(duty.trip_index).ok_or("The current timetable trip is no longer valid")?;
    let (route, complete) = schedule.trip_route(world, traffic, &trip.name);
    // Do not silently strand a service at the end of a partly loaded route.
    if !complete { return Err("Wait until the complete trip route is loaded before handing it to AI".into()); }
    add_connectors(&mut traffic.sim, &route);
    let (route, _) = bridge_gaps(&traffic.net, &route);
    let (from, lane, s, _) = route.iter().enumerate().filter_map(|(k, &lane)| {
        let l = traffic.net.lanes.get(lane)?;
        if l.kind != LaneKind::Street || !l.allows(p.vehicle.ty.def.ai_veh_type) { return None; }
        let (s, distance) = l.nearest_point(pos)?;
        let turn = (l.at(s).1 as f64 - heading + 540.0).rem_euclid(360.0) - 180.0;
        (distance <= 6.0 && turn.abs() <= 35.0).then_some((k, lane, s, distance))
    }).min_by(|a, b| a.3.total_cmp(&b.3)).ok_or("The bus is not on a compatible lane of its timetable route")?;
    let route = route[from..].to_vec();
    let mut projected = Vec::new();
    let mut last_ri = 0;
    for planned in trip.stops.iter().skip(duty.next_stop).filter(|s| s.calls()) {
        let at = planned.position.ok_or("A remaining timetable stop has no loaded position")?;
        let side = world.data().stop_side(planned.object_id);
        let (ri, ss, lat) = project_stop(&traffic.net, &route, at, Some(60.0), last_ri, side, StopRoute::Nearest)
            .ok_or("A remaining timetable stop is not on the loaded route")?;
        if ri == 0 && ss + 25.0 < s {
            return Err("A remaining timetable stop is not on the loaded route".into());
        }
        last_ri = ri;
        projected.push((ri, ss, lat, planned.dep, planned.object_id, side));
    }
    place_stops(&traffic.net, &route, 0, &mut projected, &p.vehicle.ty, false);
    let stops = projected.into_iter().map(|(ri, ss, bay, depart, id, side)| Stop {
        ri, s: if ri == 0 { ss.max(s) } else { ss }, bay, depart, id, side,
    }).collect();
    let timetable = AiTimetable {
        line: trip.line.clone(), terminus: trip.terminus.clone(),
        stops: trip.stops.iter().map(|s| (s.object_id, s.name.clone(), s.arr as f32, s.dep as f32)).collect(),
    };
    let bus = BusSetup { route, stops, number: None, hof: p.vehicle.host.hof.clone(), timetable };
    Ok(Plan { lane, s, bus: Some(bus), departure: Some(departure) })
}
