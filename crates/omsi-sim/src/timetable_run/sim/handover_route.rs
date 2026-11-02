//! Prepare the loaded part of a driver's trip without losing its deferred route.

use super::*;
use crate::VehicleInstance;
use crate::ai_traffic::bus_service::Stop;
use crate::traffic::LaneKind;

/// Geometry ready for adoption, and the opaque remainder owned by the timetable.
pub struct HandoverRoute {
    pub lane: usize,
    pub s: f32,
    pub route: Vec<usize>,
    pub stops: Vec<Stop>,
    pub layover: bool,
    pub continuation: Option<RouteContinuation>,
}

/// A preflight result has no car identity until the existing vehicle is adopted.
pub struct RouteContinuation(RunningTrip);

impl ScheduleSim {
    pub fn prepare_handover_route(
        &self,
        world: &dyn TimetableWorld,
        traffic: &mut TrafficSim,
        duty: &PlayerDuty,
        vehicle: &VehicleInstance,
    ) -> Result<HandoverRoute, String> {
        let trip = duty
            .trips
            .get(duty.trip_index)
            .ok_or("The current timetable trip is no longer valid")?;
        let source = self
            .data
            .trip(&trip.name)
            .ok_or("The current timetable trip is no longer valid")?;
        let stations = trip_stations(source);
        let (steps, track) = self.steps_of(&trip.name, &stations);
        let station_steps = trip_station_steps(source, track, steps.len());
        if stations.len() != trip.stops.len() {
            return Err("The current timetable stops no longer match the trip".into());
        }
        add_twins(traffic, &steps);
        let slots = self.slots(world, traffic, &steps, None);
        let min_step = if track && duty.next_stop > 0 {
            station_steps
                .get(duty.next_stop.saturating_sub(1))
                .copied()
                .flatten()
                .unwrap_or(0)
        } else {
            0
        };
        let (at, lane, s, _) = slots
            .iter()
            .enumerate()
            .filter_map(|(at, slot)| {
                if at < min_step || (!track && steps[at].leg + 1 < duty.next_stop) {
                    return None;
                }
                let Slot::Lane(lane) = *slot else {
                    return None;
                };
                let l = traffic.net.lanes.get(lane)?;
                if l.kind != LaneKind::Street || !l.allows(vehicle.ty.def.ai_veh_type) {
                    return None;
                }
                let (s, distance) = l.nearest_point(vehicle.position)?;
                let turn = (l.at(s).1 as f64 - vehicle.heading + 540.0).rem_euclid(360.0) - 180.0;
                (distance <= 6.0 && turn.abs() <= 35.0).then_some((at, lane, s, distance))
            })
            .min_by(|a, b| a.3.total_cmp(&b.3))
            .ok_or("The bus is not on a compatible lane of its timetable route")?;
        // Never bridge over a tile that is still streaming, even if later tiles are ready.
        let end = slots[at..]
            .iter()
            .position(|s| *s == Slot::Waiting)
            .map(|n| at + n)
            .unwrap_or(slots.len());
        let section: Vec<_> = slots[at..end]
            .iter()
            .filter_map(|s| {
                if let Slot::Lane(l) = s {
                    Some(*l)
                } else {
                    None
                }
            })
            .collect();
        add_connectors(traffic, &section);
        let (route, index) = bridge_gaps(&traffic.net, &section);
        let mut served: Vec<_> = trip
            .stops
            .iter()
            .enumerate()
            .map(|(i, stop)| i < duty.next_stop || !stop.calls())
            .collect();
        let mut projected = Vec::new();
        let mut from = 0;
        for (i, planned) in trip.stops.iter().enumerate() {
            if served[i] {
                continue;
            }
            let pos = planned.position.or_else(|| {
                world
                    .object_positions()
                    .get(&planned.object_id)
                    .map(|p| p.0)
            });
            let side = world.stop_side(planned.object_id);
            let target = station_route(station_steps[i], at, &slots[at..end], &index);
            let stop = pos.and_then(|pos| {
                project_stop(
                    &traffic.net,
                    &route,
                    pos,
                    Some(if end < slots.len() { STOP_REACH } else { 60.0 }),
                    from,
                    side,
                    target,
                )
            });
            let Some((ri, ss, lat)) = stop else {
                if end < slots.len() {
                    continue;
                }
                return Err("A remaining timetable stop is not on the loaded route".into());
            };
            if ri == 0 && ss + 25.0 < s {
                return Err("A remaining timetable stop is not on the loaded route".into());
            }
            from = ri;
            served[i] = true;
            projected.push((ri, ss, lat, planned.dep, planned.object_id, side));
        }
        place_stops(&traffic.net, &route, 0, &mut projected, &vehicle.ty, false);
        let stops = projected
            .into_iter()
            .map(|(ri, ss, bay, depart, id, side)| Stop {
                ri,
                s: if ri == 0 { ss.max(s) } else { ss },
                bay,
                depart,
                id,
                side,
            })
            .collect();
        let continuation = (end < slots.len()).then(|| {
            RouteContinuation(RunningTrip {
                car: 0,
                steps,
                next: end,
                stations: trip.stops.iter().map(|s| (s.object_id, s.dep)).collect(),
                served,
                station_steps,
            })
        });
        Ok(HandoverRoute {
            lane,
            s,
            route,
            stops,
            layover: duty.next_stop == 0,
            continuation,
        })
    }

    /// Commit only after the vehicle has changed owner; failed preflight owns nothing.
    pub fn resume_handover_route(
        &mut self,
        traffic: &mut TrafficSim,
        car: u64,
        continuation: Option<RouteContinuation>,
    ) {
        let bus = traffic
            .cars
            .iter_mut()
            .find(|c| c.id == car)
            .and_then(|c| c.bus.as_mut())
            .expect("the adopted timetable bus is present");
        self.forget_route(car);
        bus.route_open = continuation.is_some();
        bus.waiting_tile = continuation
            .as_ref()
            .and_then(|c| c.0.steps[c.0.next].key.map(|k| k.tile));
        if let Some(RouteContinuation(mut run)) = continuation {
            run.car = car;
            self.running.push(run);
        }
    }
}

#[cfg(test)]
#[path = "handover_route_tests.rs"]
mod tests;
