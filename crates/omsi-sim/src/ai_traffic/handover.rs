//! Transfer an existing road vehicle without replacing its script or passenger identity.

use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::service_tests::{Fixture, road};

    #[test]
    fn release_drives_the_same_instance_away_without_a_timetable() {
        let f = Fixture::new();
        let mut traffic = road(&f);
        let mut vehicle = VehicleInstance::new(f.ty.clone(), crate::VehicleHost::new(crate::SimClock::default()));
        vehicle.position = DVec3::new(0.0, 100.0, 0.0);
        let id = traffic.adopt_vehicle(vehicle, 0, 100.0, None);
        traffic.release(0);
        let car = &traffic.cars[0];
        assert_eq!(car.id, id);
        assert_eq!(car.vehicle.position.y, 100.0);
        assert!(car.gone);
        assert!(car.state.route.is_empty());
        assert_eq!(car.vehicle.host.schedule_active, 0.0);
    }

    #[test]
    fn a_streaming_frontier_does_not_abandon_the_service_after_the_route_timeout() {
        let f = Fixture::new();
        let mut traffic = road(&f);
        let end = traffic.net.lanes[0].length();
        let mut vehicle = VehicleInstance::new(f.ty.clone(), crate::VehicleHost::new(crate::SimClock::default()));
        vehicle.position = traffic.net.lanes[0].at(end - 1.0).0;
        let bus = model::BusSetup {
            route: vec![0], stops: Vec::new(), number: None, hof: None,
            timetable: bus_service::AiTimetable { line: "L".into(), terminus: "End".into(), stops: Vec::new() },
        };
        let id = traffic.adopt_vehicle(vehicle, 0, end - 1.0, Some(bus));
        let car = &mut traffic.cars[0];
        car.stopped = super::super::tick::ROUTE_WAIT_MAX + 1.0;
        let service = car.bus.as_mut().unwrap();
        service.route_open = true;
        service.waiting_tile = Some((99, 99));
        traffic.tick(0.02, None);
        let car = traffic.cars.iter().find(|c| c.id == id).unwrap();
        assert!(!car.gone, "an unloaded tile is not an irreparable route gap");
        assert!(car.bus.as_ref().unwrap().route_open);
        assert!(!car.bus.as_ref().unwrap().trip_done());
        assert_eq!(car.state.route, vec![0]);
    }

    #[test]
    fn a_ready_tile_waits_for_the_timetable_update_before_leaving_the_frontier() {
        let f = Fixture::new();
        let mut traffic = road(&f);
        let end = traffic.net.lanes[0].length();
        let mut vehicle = VehicleInstance::new(f.ty.clone(), crate::VehicleHost::new(crate::SimClock::default()));
        vehicle.position = traffic.net.lanes[0].at(end - 1.0).0;
        let setup = model::BusSetup {
            route: vec![0], stops: Vec::new(), number: None, hof: None,
            timetable: bus_service::AiTimetable { line: "L".into(), terminus: "End".into(), stops: Vec::new() },
        };
        traffic.adopt_vehicle(vehicle, 0, end - 1.0, Some(setup));
        traffic.lane_tiles.insert((99, 99));
        let car = &mut traffic.cars[0];
        car.stopped = super::super::tick::ROUTE_WAIT_MAX + 1.0;
        let service = car.bus.as_mut().unwrap();
        service.route_open = true;
        service.waiting_tile = Some((99, 99));
        traffic.tick(0.02, None);
        assert!(!traffic.cars[0].gone);
        assert!(traffic.cars[0].bus.as_ref().unwrap().route_open);
        assert_eq!(traffic.cars[0].state.route, vec![0]);
    }

    #[test]
    fn a_loaded_but_broken_route_still_uses_the_existing_timeout() {
        let f = Fixture::new();
        let mut traffic = road(&f);
        let end = traffic.net.lanes[0].length();
        let mut vehicle = VehicleInstance::new(f.ty.clone(), crate::VehicleHost::new(crate::SimClock::default()));
        vehicle.position = traffic.net.lanes[0].at(end - 1.0).0;
        let setup = model::BusSetup {
            route: vec![0], stops: Vec::new(), number: None, hof: None,
            timetable: bus_service::AiTimetable { line: "L".into(), terminus: "End".into(), stops: Vec::new() },
        };
        traffic.adopt_vehicle(vehicle, 0, end - 1.0, Some(setup));
        let car = &mut traffic.cars[0];
        car.stopped = super::super::tick::ROUTE_WAIT_MAX + 1.0;
        let service = car.bus.as_mut().unwrap();
        service.route_open = true;
        service.waiting_tile = None;
        traffic.tick(0.02, None);
        assert!(traffic.cars[0].gone);
        assert!(!traffic.cars[0].bus.as_ref().unwrap().route_open);
    }

    #[test]
    fn adoption_keeps_pose_physics_and_script_host() {
        let f = Fixture::new();
        let mut traffic = road(&f);
        let mut vehicle = VehicleInstance::new(f.ty.clone(), crate::VehicleHost::new(crate::SimClock::default()));
        vehicle.position = DVec3::new(0.7, 100.0, 0.4);
        vehicle.heading = 3.0;
        vehicle.physics.speed = 0.12;
        vehicle.host.tt_line = "kept".into();
        let id = traffic.adopt_vehicle(vehicle, 0, 100.0, None);
        let car = traffic.cars.iter().find(|c| c.id == id).unwrap();
        assert_eq!(car.vehicle.position, DVec3::new(0.7, 100.0, 0.4));
        assert_eq!(car.vehicle.heading, 3.0);
        assert_eq!(car.vehicle.physics.speed, 0.12);
        assert_eq!(car.vehicle.host.tt_line, "kept");
        assert!(Arc::ptr_eq(&car.vehicle.ty, &f.ty));
        assert!((car.state.speed - 0.12).abs() < 0.001);
        let taken = traffic.take_off_id(id, Removal::TakenOver).unwrap();
        assert_eq!(taken.vehicle.position.y, 100.0);
        assert!(traffic.cars.is_empty());
        assert!(traffic.take_off_id(id, Removal::TakenOver).is_none());
    }
}

impl TrafficSim {
    /// The caller validates lane admission before moving its owned instance here.
    pub fn adopt_vehicle(&mut self, vehicle: VehicleInstance, lane: usize, s: f32, bus: Option<BusSetup>) -> u64 {
        let pose = (vehicle.position, vehicle.heading, vehicle.pitch, vehicle.bank, vehicle.physics.clone());
        let ty = vehicle.ty.clone();
        let scheme = vehicle.host.paint_scheme.flatten();
        let seed = self.next_id.wrapping_mul(0x101);
        let kind = self.net.lanes[lane].kind;
        let id = self.place_car(vehicle, kind, lane, s, ty, seed, scheme, None, Some(pose.4.speed.max(0.0)), bus);
        let car = self.cars.last_mut().expect("the adopted car was just appended");
        let v = &mut car.vehicle;
        (v.position, v.heading, v.pitch, v.bank, v.physics) = pose;
        car.state.speed = v.physics.speed.max(0.0);
        // Keep the initial lateral offset; returning to the way is the ordinary AI's job.
        let (at, heading) = self.net.lanes[lane].at(s);
        let h = heading.to_radians();
        car.state.lateral = ((v.position.x - at.x) * h.cos() as f64 - (v.position.y - at.y) * h.sin() as f64) as f32;
        car.state.lateral_target = 0.0;
        car.state.lateral_ramp = (car.state.lateral, car.state.lateral, 0.0, 1.0);
        car.body.adopt_pose(v);
        id
    }
}
