//! What the timetable and the rest of the game ask of the traffic and tell it.

use super::*;

impl Traffic {
    /// Would a vehicle of type `ty` with its origin at `pos`, heading `heading` (and its
    /// coupled parts, straight behind it) touch one that is already there - an AI vehicle,
    /// or one of `keep_clear`? Bodies are compared with half a metre to spare, not centres:
    /// an articulated bus reaches 6 m ahead of its origin and 12 m behind it, and one put
    /// down 9.3 m from the player's bus stood 1.8 m inside it.
    pub fn blocked(&mut self, ty: &Arc<VehicleType>, pos: DVec3, heading: f64) -> bool {
        let grown = |mut b: omsi_sim::collision::Obb| {
            b.half += glam::DVec2::splat(0.5);
            b
        };
        let mut bodies = vec![grown(omsi_sim::collision::Obb::from_box(
            ty.def.bounding_box.unwrap_or(DEFAULT_BOX),
            pos,
            omsi_sim::vehicle::body_heading(&ty.def, heading, false),
        ))];
        let (mut origin, mut lead, mut lead_rev) = (pos, ty.clone(), false);
        for (t, rev) in self.trailer_chain(ty) {
            let (back, front) = omsi_sim::vehicle::coupling_points(&lead, lead_rev, &t, rev);
            // each car stands along the consist's heading, turned round by its own
            // (absolute) orientation - never by the car in front of it
            let (center, car_heading) = omsi_sim::vehicle::coupling_placement(
                origin,
                heading,
                omsi_sim::vehicle::body_reversed(&lead.def, lead_rev),
                back.y,
                omsi_sim::vehicle::body_reversed(&t.def, rev),
                front.y,
            );
            bodies.push(grown(omsi_sim::collision::Obb::from_box(
                t.def.bounding_box.unwrap_or(DEFAULT_BOX),
                center,
                car_heading,
            )));
            origin = center;
            lead = t;
            lead_rev = rev;
        }
        self.touches(&bodies, pos)
    }

    /// Do `bodies` (around `pos`) touch an AI vehicle or one of `keep_clear`?
    pub fn touches(&self, bodies: &[omsi_sim::collision::Obb], pos: DVec3) -> bool {
        let reach = bodies
            .iter()
            .map(|b| (b.center - pos.truncate()).length() + b.half.length())
            .fold(0.0, f64::max)
            + 40.0;
        let touches = |o: &omsi_sim::collision::Obb| bodies.iter().any(|b| b.overlaps(o));
        self.sim.keep_clear.iter().any(|o| touches(o))
            || self
                .cars
                .iter()
                .filter(|c| (c.vehicle.position - pos).length() < reach)
                .any(|c| vehicle_bodies(&c.vehicle).iter().any(|o| touches(o)))
    }

    /// Take all random AI cars off the road now, keeping timetable buses. Returns how many
    /// vehicles were removed. The configured target is unchanged, so random traffic can
    /// populate the roads again normally.
    pub fn clear_random(&mut self) -> usize {
        let ids: Vec<u64> = self.sim.cars.iter().filter(|c| !c.is_bus()).map(|c| c.id).collect();
        let removed = ids.len();
        for id in ids {
            self.remove_car(id);
        }
        removed
    }

    /// Take a car off the road now (the player took over its tour); its renders follow with
    /// `view_sync::apply_traffic`.
    pub fn remove_car(&mut self, id: u64) -> bool {
        let Some(c) = self.sim.take_off_id(id, omsi_sim::ai_traffic::control::Removal::TakenOver) else {
            return false;
        };
        self.drop_sounds(c.id);
        self.events.push(TrafficEvent::Released { id: c.id });
        true
    }

    /// Tell a scheduled bus's script who wants in or out (`PAX_Entry<i>_Req`,
    /// `PAX_Exit<i>_Req`) and who stands in its doorways (`_Busy`): the stock AI door
    /// scripts open the rear doors only for a stop request, which comes from the exit
    /// requests.
    pub fn set_pax_requests(&mut self, id: u64, doors: &crate::humans::DoorWants) {
        if let Some(c) = self.sim.cars.iter_mut().find(|c| c.id == id) {
            crate::humans::Humans::write_door_requests(&mut c.vehicle, doors);
        }
    }
}
