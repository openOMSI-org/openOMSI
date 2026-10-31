//! Preflight the whole handover before moving ownership out of the player.

use crate::App;
use omsi_sim::ai_traffic::model::BusSetup;
use omsi_sim::ai_traffic::bus_service::AiTimetable;
use omsi_sim::traffic::LaneKind;
use omsi_sim::timetable_run::RouteContinuation;

pub(super) struct Plan {
    pub lane: usize,
    pub s: f32,
    pub bus: Option<BusSetup>,
    pub departure: Option<usize>,
    pub continuation: Option<RouteContinuation>,
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
        return Ok(Plan { lane, s, bus: Some(bus), departure: None, continuation: None });
    };
    let schedule = app.session.schedule.as_ref().ok_or("The timetable is not ready")?;
    let world = app.world.as_ref().ok_or("The world is not ready")?;
    let departure = schedule.handover_departure(duty).ok_or("The current timetable departure is no longer valid")?;
    let trip = duty.trips.get(duty.trip_index).ok_or("The current timetable trip is no longer valid")?;
    let plan = schedule.prepare_handover_route(&*world.data(), &mut traffic.sim, duty, &p.vehicle)?;
    let timetable = AiTimetable {
        line: trip.line.clone(), terminus: trip.terminus.clone(),
        stops: trip.stops.iter().map(|s| (s.object_id, s.name.clone(), s.arr as f32, s.dep as f32)).collect(),
    };
    let bus = BusSetup { route: plan.route, stops: plan.stops, number: None, hof: p.vehicle.host.hof.clone(), timetable };
    Ok(Plan { lane: plan.lane, s: plan.s, bus: Some(bus), departure: Some(departure), continuation: plan.continuation })
}
