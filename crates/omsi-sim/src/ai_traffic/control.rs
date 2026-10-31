//! What the timetable and the rest of the game ask of the traffic and tell it (the parts
//! that need no world: see omsi-app's `traffic::control` for the rest).

use super::*;

/// Why a car was taken off the road. Whatever the cause, it gives back what it held (its
/// berths, its place in their queues) once, as it goes (`TrafficSim::take_off`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Removal {
    /// Out of range: it drives on as a dormant car (`DormantCar`).
    Dormant,
    /// The ground under it was unloaded.
    Unloaded,
    /// Its way or its trip is over (the end of the network, parked, given up).
    Finished,
    /// The player took it over (its tour, the vehicle), or the timetable took it back.
    TakenOver,
    /// Its route no longer exists on the network.
    InvalidRoute,
}

impl Removal {
    pub const COUNT: usize = 5;
}

impl TrafficSim {
    /// Take car `i` off the road (`cause`): it leaves the list (the last car takes its
    /// place) and gives up its berths and its place in their queues.
    pub fn take_off(&mut self, i: usize, cause: Removal) -> AiCar {
        let c = self.cars.swap_remove(i);
        self.berths.release_car(c.id);
        self.removals[cause as usize] += 1;
        if self.debug_population {
            log::info!("population t={:.1}: car {} taken off ({cause:?})", self.time, c.id);
        }
        c
    }

    /// The next car that has parked (`ParkPlan::done`: in its space and standing), taken off
    /// the road with its plan: the parked object takes its place, which the view does
    /// (see [`TrafficSim::space_taken_back`]). The cars are taken in the order they stand
    /// in the list, as the view sync took them one by one.
    pub fn take_parked(&mut self) -> Option<(AiCar, ParkPlan)> {
        let i = self.cars.iter().position(|c| c.park.is_some_and(|p| p.done))?;
        let plan = self.cars[i].park?;
        Some((self.take_off(i, Removal::Finished), plan))
    }

    /// The parked object is back in the space of `plan`: a car of the traffic may park
    /// there no more, it is one of the lane's parked cars again.
    pub fn space_taken_back(&mut self, plan: &ParkPlan) {
        self.parked.entry(plan.lane).or_default().push((plan.s, plan.lat));
    }

    /// Take the car of id `id` off the road (`cause`), if it is on it.
    pub fn take_off_id(&mut self, id: u64, cause: Removal) -> Option<AiCar> {
        let i = self.cars.iter().position(|c| c.id == id)?;
        Some(self.take_off(i, cause))
    }

    /// The lanes the other way along one-way paths `lanes` (see `Schedule`'s route
    /// building: OMSI's timetable buses drive a path against its direction where the trip's
    /// station links or track say so). Only timetable routes use them: they carry no traffic
    /// and no light. Returns how many were added.
    pub fn add_reverse_twins(&mut self, lanes: &[usize]) -> usize {
        let mut new = Vec::new();
        for &l in lanes {
            if !self.twinned.insert(l) {
                continue;
            }
            let o = &self.net.lanes[l];
            let pts: Vec<DVec3> = o.points.iter().rev().copied().collect();
            let mut t = crate::traffic::LaneBuilder::polyline(pts, o.kind, o.width);
            t.key = o.key;
            t.reversed = !o.reversed;
            t.speed_limit_kmh = o.speed_limit_kmh;
            t.source = o.source;
            t.offset = o.offset;
            t.name = o.name.clone();
            t.invisible = o.invisible;
            t.priority = o.priority;
            t.density = 0.0;
            t.no_cars = true;
            new.push(t);
        }
        let n = new.len();
        if n > 0 {
            let added = self.net.extend(new, 1.5);
            log::debug!("traffic: {n} lanes added for timetable routes that drive a one-way path the other way ({:?})", added);
        }
        n
    }

    /// A lane from the end of `a` to the start of `b`, for a timetable route that jumps a
    /// gap in the map (a road piece deleted after the timetable's tracks were recorded - a
    /// dozen such holes of 10 to 150 m on Novi Sad): a smooth curve with the two lanes'
    /// headings at its ends, used by the timetable only. None when the two do not line up.
    pub fn add_connector(&mut self, a: usize, b: usize) -> Option<usize> {
        let (la, lb) = (&self.net.lanes[a], &self.net.lanes[b]);
        let (p0, p1) = (la.end(), lb.start());
        let d = (p1 - p0).truncate();
        let len = d.length();
        if !(2.0..=150.0).contains(&len) || la.kind != lb.kind {
            return None;
        }
        let dir = |h: f32| {
            let r = (h as f64).to_radians();
            glam::DVec2::new(r.sin(), r.cos())
        };
        let (t0, t1) = (dir(la.end_heading()), dir(lb.start_heading()));
        let chord = d / len;
        // the lanes point along the gap or turn across it (a junction whose turning path
        // the map lost, 66 of them on Novi Sad), never a reversal
        if t0.dot(chord) < 0.3 || t1.dot(chord) < 0.3 || t0.dot(t1) < -0.2 {
            return None;
        }
        // (tangents as long as the gap make a straight gap a smooth S; across a turn they
        // would swing out past the corner)
        let len_t = len * if t0.dot(t1) > 0.9 { 1.0 } else { 0.6 };
        let n = ((len / 3.0).ceil() as usize).max(2);
        let pts: Vec<DVec3> = (0..=n)
            .map(|i| {
                let t = i as f64 / n as f64;
                let (h00, h10, h01, h11) = (2.0 * t * t * t - 3.0 * t * t + 1.0, t * t * t - 2.0 * t * t + t, -2.0 * t * t * t + 3.0 * t * t, t * t * t - t * t);
                let xy = p0.truncate() * h00 + t0 * len_t * h10 + p1.truncate() * h01 + t1 * len_t * h11;
                DVec3::new(xy.x, xy.y, p0.z + (p1.z - p0.z) * t)
            })
            .collect();
        let mut l = crate::traffic::LaneBuilder::polyline(pts, la.kind, la.width);
        l.speed_limit_kmh = la.speed_limit_kmh.min(lb.speed_limit_kmh);
        l.name = "(timetable connector)".into();
        l.density = 0.0;
        l.no_cars = true;
        let added = self.net.extend(vec![l], 1.5);
        log::debug!("traffic: a {len:.0} m connector lane {} from lane {a} to lane {b} for a timetable route", added.start);
        Some(added.start)
    }

    /// Hand timetable bus `ci` the next trip of its tour: its route from the lane it is on
    /// (`route[0]` is that lane, `s` where it is on it) and the trip's stops. It stays where
    /// it stands; a stop right there is served in place (its layover).
    pub fn reroute(&mut self, ci: usize, route: Vec<usize>, s: f32, stops: Vec<(usize, f32, f32, f64, i64, f32)>, layover: bool) {
        let net = &self.net;
        let car = &mut self.cars[ci];
        let lane = car.state.lane;
        car.state.route = route;
        car.state.route_index = 0;
        car.state.lane = lane;
        car.state.s = s;
        car.state.change = None;
        car.state.planned_next = None;
        car.state.ahead.clear();
        car.state.plan_next(net);
        car.gone = false;
        let stops = stops.into_iter().map(super::bus_service::Stop::from_tuple).collect();
        let id = car.id;
        self.berths.release_car(id);
        let car = &mut self.cars[ci];
        match car.bus.as_mut() {
            Some(b) => b.restart(stops, layover),
            None => {
                let mut b = BusService::new(stops);
                b.layover = layover;
                car.bus = Some(Box::new(b));
            }
        }
    }

    /// Let timetable bus `ci` go at the end of its trip: it drives on as other traffic and
    /// is taken off as soon as nobody sees it.
    pub fn release(&mut self, ci: usize) {
        let net = &self.net;
        let car = &mut self.cars[ci];
        let st = &mut car.state;
        st.route.clear();
        st.route_index = 0;
        st.planned_next = None;
        st.ahead.clear();
        st.plan_next(net);
        if let Some(b) = car.bus.as_mut() {
            b.restart(Vec::new(), false);
        }
        car.gone = true;
        let id = car.id;
        self.berths.release_car(id);
        let car = &mut self.cars[ci];
        car.vehicle.host.schedule_active = 0.0;
        car.vehicle.host.tt_line.clear();
        car.vehicle.host.tt_stops.clear();
        car.vehicle.host.tt_stop_ids.clear();
        car.vehicle.host.tt_busstop_index = -1;
        car.vehicle.host.tt_terminus_index = -1;
        car.vehicle.host.tt_delay = 0.0;
        car.vehicle.set_var("schedule_active", 0.0);
    }

    /// The AI on the roads: (cars, buses, cars asleep far from everybody, parked cars).
    pub fn counts(&self) -> (usize, usize, usize, usize) {
        let buses = self.cars.iter().filter(|c| c.is_bus()).count();
        (self.cars.len() - buses, buses, self.dormant.len(), self.parked.values().map(Vec::len).sum())
    }

    /// Front, rear and side collision zones of AI vehicles (for the player's collisions),
    /// including the rear sections of articulated buses and trailers.
    pub fn boxes(&self, near: DVec3, radius: f64) -> Vec<crate::collision::Obb> {
        self.cars
            .iter()
            .filter(|c| (c.vehicle.position - near).length() < radius)
            .flat_map(|c| {
                let bb = c
                    .vehicle
                    .ty
                    .def
                    .bounding_box
                    .unwrap_or([2.0, 4.5, 1.6, 0.0, 0.0, 0.8]);
                // Moving, and with a mass of its own: a car that runs into the bus is no
                // bulldozer. Use the model heading so reversed vehicles and buses agree
                // with their visible body and impact zones.
                let h = c.vehicle.body_heading().to_radians();
                let v = glam::DVec2::new(h.sin(), h.cos()) * c.state.speed as f64;
                let (mass, id) = (c.vehicle.physics.mass_kg, c.id);
                let rear = c.vehicle.trailers.iter().filter_map(move |t| {
                    t.ty.def.bounding_box.map(|bb| {
                        impact_zones(bb, t.position, t.body_heading(), v, mass, id)
                    })
                });
                std::iter::once(impact_zones(
                    bb,
                    c.vehicle.position,
                    c.vehicle.body_heading(),
                    v,
                    mass,
                    id,
                ))
                .chain(rear)
                .flatten()
            })
            .collect()
    }

    /// Deliver the player's moving-vehicle contacts to the AI cars so their collision
    /// scripts and per-instance damage visuals respond to the same impact.
    pub fn player_impacts(&mut self, impacts: Vec<crate::vehicle::DynamicImpact>) {
        for impact in impacts {
            let Some(id) = impact
                .obstacle_id
                .checked_neg()
                .and_then(|id| id.checked_sub(2))
                .and_then(|id| u64::try_from(id).ok())
            else {
                continue;
            };
            let Some(car) = self.cars.iter_mut().find(|car| car.id == id) else {
                continue;
            };
            car.body.collision_impulse(
                impact.push.truncate(),
                impact.speed,
                impact.mass,
                car.vehicle.physics.mass_kg,
            );
            let inverse = car.vehicle.body_rotation().inverse();
            let point = inverse.transform_point3((impact.point - car.vehicle.position).as_vec3());
            let push = inverse.transform_vector3(impact.push.as_vec3());
            car.vehicle
                .receive_dynamic_impact(point, push, impact.speed, impact.energy);
        }
    }

    /// Position and heading of a car by id (None once it is gone).
    pub fn car_pose(&self, id: u64) -> Option<(DVec3, f64)> {
        self.cars
            .iter()
            .find(|c| c.id == id)
            .map(|c| (c.vehicle.position, c.vehicle.heading))
    }

    /// Keep a scheduled bus at its stop for at least `secs` more with the doors open:
    /// passengers are still queueing at a door or stepping in.
    /// The passengers' wishes for the timetable buses' next stops (see `stop_wishes`).
    pub fn set_stop_wishes(&mut self, alighting: hashbrown::HashSet<u64>, waiting: hashbrown::HashSet<i64>) {
        self.stop_wishes = Some((alighting, waiting));
    }

    pub fn hold_boarding(&mut self, id: u64, stop: Option<i64>, secs: f32) {
        if let Some(c) = self.cars.iter_mut().find(|c| c.id == id) {
            if let Some(b) = c.bus.as_mut() {
                b.hold(stop, secs);
            }
        }
    }

    /// A passenger of the timetable bus `id` asked for its next stop: its script gets the same
    /// vehicle trigger as the player's bus (`int_haltewunsch`), which the stock AI door scripts
    /// open the doors for.
    pub fn request_stop(&mut self, id: u64) {
        if let Some(c) = self.cars.iter_mut().find(|c| c.id == id) {
            c.vehicle.trigger("int_haltewunsch");
        }
    }
}

pub(super) fn impact_zones(
    bb: [f32; 6],
    position: glam::DVec3,
    heading: f64,
    velocity: glam::DVec2,
    mass: f32,
    id: u64,
) -> [crate::collision::Obb; 4] {
    let [width, length, height, cx, cy, cz] = bb;
    let part = |w, l, x, y| {
        crate::collision::Obb::from_box([w, l, height, x, y, cz], position, heading)
            .moving(velocity, mass, id)
    };
    [
        part(width, length * 0.25, cx, cy + length * 0.375),
        part(width, length * 0.25, cx, cy - length * 0.375),
        part(width * 0.5, length * 0.5, cx - width * 0.25, cy),
        part(width * 0.5, length * 0.5, cx + width * 0.25, cy),
    ]
}
