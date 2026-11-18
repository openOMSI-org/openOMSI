//! Parked cars: beside which lanes they stand, cars parking in a free space and parked
//! cars pulling out.

use super::*;
use crate::scene::World;

impl Traffic {
    /// Now and then a car near the player parks: a space at the kerb that a parked car has
    /// left is taken by a car of the same kind driving up the lane beside it - it indicates,
    /// slows down, stops beside the space, moves over into it and stands there as the
    /// parked car it was. Called with every population pass (about every two seconds).
    pub fn park_in(&mut self, world: &World, center: DVec3) {
        let forced = omsi_cfg::flags::OMSI_PARK_IN.parse::<f64>();
        if self.rand_f() >= forced.unwrap_or(0.04) {
            return;
        }
        let debug = omsi_cfg::flags::OMSI_DEBUG_TRAFFIC.is_set() || forced.is_some();
        let taken: hashbrown::HashSet<i64> = self.sim.cars.iter().filter_map(|c| c.park.map(|p| p.key)).collect();
        let mut spots = world.free_parking();
        spots.retain(|(k, p)| !taken.contains(k) && (30.0..260.0).contains(&(p.pos - center).truncate().length()));
        spots.sort_by_key(|s| s.0);
        let mut why: Vec<String> = Vec::new();
        while !spots.is_empty() {
            let (key, p) = spots.swap_remove(self.rand() as usize % spots.len());
            let Some((l, s, _)) = self.sim.net.nearest_lane_near(p.pos, LaneKind::Street) else { continue };
            let lane = &self.sim.net.lanes[l];
            if lane.no_cars || s < 8.0 || s > lane.length() - 4.0 {
                continue;
            }
            let (q, h) = lane.at(s);
            let hr = (h as f64).to_radians();
            let lat = (p.pos - q).dot(DVec3::new(hr.cos(), -hr.sin(), 0.0)) as f32;
            let mut dh = (p.heading - h as f64).rem_euclid(360.0);
            if dh > 180.0 {
                dh -= 360.0;
            }
            // beside the lane on the right and in line with it (a space across the kerb or in
            // a row is not driven into)
            if !(1.2..4.2).contains(&lat) || dh.abs() > 12.0 {
                why.push(format!("space {key}: {lat:+.1} m beside lane {l}, {dh:+.0} deg"));
                continue;
            }
            let folder = p.sco.parent().map(|d| d.to_string_lossy().to_lowercase());
            // a car of that kind coming up the lane, far enough off to slow down gently
            let mut best: Option<(usize, f32)> = None;
            for (i, c) in self.sim.cars.iter().enumerate() {
                if c.is_bus() || c.gone || c.park.is_some() || c.passing.is_some() || c.state.change.is_some() || c.pull_out > 0.0 {
                    continue;
                }
                if c.vehicle.ty.def.path.parent().map(|d| d.to_string_lossy().to_lowercase()) != folder {
                    continue;
                }
                if c.state.speed > 15.0 || c.state.lateral.abs() > 0.2 {
                    continue;
                }
                let Some(&(_, dl)) = self.way_lanes(&c.state, 160.0).iter().find(|w| w.0 == l) else { continue };
                let d = dl + s;
                let need = c.state.speed * c.state.speed / 2.0 + 20.0;
                if d > need && d < 150.0 && best.map(|b| d < b.1).unwrap_or(true) {
                    best = Some((i, d));
                }
            }
            let Some((i, d)) = best else {
                why.push(format!("space {key}: no car of its kind coming up lane {l}"));
                continue;
            };
            let car = &mut self.sim.cars[i];
            car.park = Some(ParkPlan { key, lane: l, s, lat, ramped: false, done: false });
            // (it parks: no more route to plan than to here)
            car.gone = false;
            if debug {
                log::info!("car {} parks in the space of parked car {key} ({:.0} m ahead, {lat:.1} m right of lane {l})", car.id, d);
            }
            return;
        }
        if debug && !why.is_empty() {
            log::info!("park-in: none this time ({})", why.join("; "));
        }
    }

    /// Now and then a car parked at the kerb near the player drives off: the parked object
    /// goes (its space stays empty) and the AI car of the same folder takes its place, in
    /// the parking position, indicating, and pulls out into its lane once the road behind
    /// it is clear. Called with every population pass (about every two seconds).
    pub fn pull_out_parked(&mut self, world: &World, center: DVec3) {
        let forced = omsi_cfg::flags::OMSI_PARKED_PULL_OUT.parse::<f64>();
        // about one car a minute
        if self.rand_f() >= forced.unwrap_or(0.035) {
            return;
        }
        let debug = omsi_cfg::flags::OMSI_DEBUG_TRAFFIC.is_set() || forced.is_some();
        let mut candidates: Vec<(i64, crate::scene::ParkedObject)> = world
            .parked_objects
            .lock()
            .iter()
            .filter(|(_, p)| {
                let d = (p.pos - center).truncate().length();
                (25.0..180.0).contains(&d)
            })
            .map(|(k, p)| (*k, p.clone()))
            .collect();
        candidates.sort_by_key(|c| c.0);
        if debug {
            log::info!("parked pull-out: {} parked cars in range ({} loaded)", candidates.len(), world.parked_objects.lock().len());
        }
        let mut why: Vec<String> = Vec::new();
        let mut tried = 0;
        while !candidates.is_empty() && tried < 8 {
            tried += 1;
            let (key, p) = candidates.swap_remove(self.rand() as usize % candidates.len());
            // the AI car of the same folder (a parked Golf is `parked_vw_golf_2.sco` next to
            // `ai_vw_golf_2.bus`)
            let folder = p.sco.parent().map(|d| d.to_string_lossy().to_lowercase());
            let Some(ty) = self
                .types
                .iter()
                .filter(|t| t.2 == LaneKind::Street)
                .find(|t| t.0.def.path.parent().map(|d| d.to_string_lossy().to_lowercase()) == folder)
                .map(|t| t.0.clone())
            else {
                why.push(format!("no AI car in {folder:?}"));
                continue;
            };
            let Some((l, s, _)) = self.sim.net.nearest_lane_near(p.pos, LaneKind::Street) else {
                why.push("no lane".into());
                continue;
            };
            let lane = &self.sim.net.lanes[l];
            if lane.no_cars || s < 4.0 || s > lane.length() - 4.0 {
                continue;
            }
            let (q, h) = lane.at(s);
            let hr = (h as f64).to_radians();
            let right = DVec3::new(hr.cos(), -hr.sin(), 0.0);
            let lat = (p.pos - q).dot(right) as f32;
            // beside the lane on the right, facing its way (one parked in the lane itself stands
            // bumper to bumper in a row it cannot steer out of)
            let mut dh = (p.heading - h as f64).rem_euclid(360.0);
            if dh > 180.0 {
                dh -= 360.0;
            }
            if !(0.8..4.5).contains(&lat) || dh.abs() > 30.0 {
                why.push(format!("{lat:+.1} m beside lane {l}, {dh:+.0} deg to it"));
                continue;
            }
            // nobody close by on the road
            if self.sim.cars.iter().any(|c| (c.vehicle.position - p.pos).length() < 30.0) || !self.spawn_clear(&ty, q, h as f64) {
                why.push("road not clear".into());
                continue;
            }
            // (the world's side now; the object hidden by the view: `TrafficEvent::ParkedDeparted`)
            let Some(gone) = world.take_departing_parked(key) else {
                continue;
            };
            self.events.push(TrafficEvent::ParkedDeparted { instances: gone.instances });
            if let Some(list) = self.sim.parked.get_mut(&l) {
                if let Some(j) = (0..list.len()).min_by(|&a, &b| (list[a].0 - s).abs().total_cmp(&(list[b].0 - s).abs())) {
                    if (list[j].0 - s).abs() < 3.0 {
                        list.swap_remove(j);
                    }
                }
            }
            let seed = self.rand();
            let id = self.create_car(world, center, LaneKind::Street, l, s, ty.clone(), seed, None, None, Some(0.0), None);
            let net = &self.sim.net;
            if let Some(car) = self.sim.cars.iter_mut().find(|c| c.id == id) {
                car.state.lateral = lat;
                car.state.lateral_target = 0.0;
                car.state.lateral_ramp = (lat, 0.0, car.state.odometer, (lat * 6.0).clamp(8.0, 16.0));
                car.body = place_body(net, &car.state, &mut car.vehicle, MotionKind::Road);
                car.pull_out = 2.0 + (seed % 1000) as f32 / 400.0;
                car.state.blinker = 1;
            }
            if debug {
                log::info!("parked car {key} ({}) at ({:.1}, {:.1}) pulls out as car {id}, {lat:.1} m right of lane {l}", ty.def.path.display(), p.pos.x, p.pos.y);
            }
            return;
        }
        if debug && !why.is_empty() {
            log::info!("parked pull-out: none this time ({})", why.join("; "));
        }
    }

    /// The cars that have finished parking (the simulation takes them off, see
    /// `TrafficSim::take_parked`): the parked object stands in their place from now on
    /// (shown by the view: `TrafficEvent::ParkedReturned`), and their renders go back to the
    /// world at the next sync.
    pub(crate) fn settle_parked(&mut self, world: &World) {
        while let Some((c, p)) = self.sim.take_parked() {
            if let Some(instances) = world.take_returning_parked(p.key) {
                self.events.push(TrafficEvent::ParkedReturned { instances });
                self.sim.space_taken_back(&p);
            }
            if omsi_cfg::flags::OMSI_DEBUG_TRAFFIC.is_set() {
                log::info!("car {} has parked (space {})", c.id, p.key);
            }
            self.drop_sounds(c.id);
            self.events.push(TrafficEvent::Retired { id: c.id });
        }
    }
}
