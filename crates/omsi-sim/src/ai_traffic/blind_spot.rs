//! The blind spot around the player's bus (#2090): what stands beside it and behind it.
//!
//! OMSI 2 scripts cannot ask the game what is around the vehicle, so a bus add-on could only
//! fake a blind spot monitor. Here the traffic measures it: three zones in the bus's own
//! frame - left, right and rear - and the nearest car in each, as the gap between the two
//! bodies. A bus's scripts read the result as `BSM_Left`, `BSM_Right`, `BSM_Rear` (0 or 1)
//! and their `_Dist` in metres; a vehicle whose `[varlist]` has none of them is untouched.

use super::model::PlayerBox;
use glam::DVec3;

/// How big the zones are, in metres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Zone {
    /// How far out from the bus's side a car still counts as beside it.
    pub side: f32,
    /// How far ahead of the rear body line the side zone reaches, so a car just past the
    /// front still counts (0: only alongside the body).
    pub overhang: f32,
    /// How far behind the rear body line the rear zone reaches.
    pub rear: f32,
}

impl Zone {
    /// Beside the body up to 3.5 m out, and 20 m behind.
    pub const DEFAULT: Zone = Zone {
        side: 3.5,
        overhang: 2.0,
        rear: 20.0,
    };
}

impl Default for Zone {
    fn default() -> Self {
        Zone::DEFAULT
    }
}

/// What the traffic found: the gap (m) to the nearest body in each zone, `None` when clear.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BlindSpot {
    pub left: Option<f32>,
    pub right: Option<f32>,
    pub rear: Option<f32>,
}

impl BlindSpot {
    /// Nothing in any zone.
    pub fn clear(&self) -> bool {
        self.left.is_none() && self.right.is_none() && self.rear.is_none()
    }

    /// The variables a bus's scripts read, in the order of [`BlindSpot::variables`].
    pub fn values(&self) -> [f32; 6] {
        [
            if self.left.is_some() { 1.0 } else { 0.0 },
            self.left.unwrap_or(0.0),
            if self.right.is_some() { 1.0 } else { 0.0 },
            self.right.unwrap_or(0.0),
            if self.rear.is_some() { 1.0 } else { 0.0 },
            self.rear.unwrap_or(0.0),
        ]
    }

    /// The names of those variables (what a `[varlist]` has to declare to see them).
    pub const variables: [&'static str; 6] = [
        "BSM_Left",
        "BSM_Left_Dist",
        "BSM_Right",
        "BSM_Right_Dist",
        "BSM_Rear",
        "BSM_Rear_Dist",
    ];
}

/// Measure the zones for the player's bus (`player`: centre, heading in degrees, half length,
/// half width, speed) against `bodies`: each a centre and half width, in the world frame.
/// Only bodies whose centre lies in a zone count; the gap is from the bus's body side (or
/// rear) to the other body's.
pub fn rise(
    obstacles: impl IntoIterator<Item = (DVec3, f32)>,
    player: PlayerBox,
    zone: Zone,
) -> BlindSpot {
    let (centre, heading, half_len, half_width, _) = player;
    // the traffic keeps its geometry in f64; the zones and the answer stay in f32
    let (half_len, half_width) = (half_len as f64, half_width as f64);
    let (zone_side, zone_overhang, zone_rear) =
        (zone.side as f64, zone.overhang as f64, zone.rear as f64);
    let r = heading.to_radians();
    // the world directions of the bus's own frame (`heading_dir`: 0 faces +y)
    let fwd = DVec3::new(r.sin(), r.cos(), 0.0);
    let right = DVec3::new(r.cos(), -r.sin(), 0.0);
    let mut out = BlindSpot::default();
    for (pos, other_half) in obstacles {
        let other_half = other_half as f64;
        let d = pos - centre;
        let along = d.dot(fwd);
        let side = d.dot(right);
        // the gap between the two bodies across the road; a side zone is outside a flank,
        // from the rear body line through the front overhang, never through the bus itself
        let lateral = side.abs() - half_width - other_half;
        if lateral > 0.0
            && lateral <= zone_side
            && along >= -half_len
            && along <= half_len + zone_overhang
        {
            let gap = lateral.max(0.0) as f32;
            let slot = if side > 0.0 {
                &mut out.right
            } else {
                &mut out.left
            };
            if slot.is_none_or(|g| gap < g) {
                *slot = Some(gap);
            }
        }
        // behind the body: from the bus's rear body line to the other body's front flank
        let behind = -along - half_len - other_half;
        if behind >= 0.0 && behind <= zone_rear && lateral <= zone_side {
            let behind = behind as f32;
            if out.rear.is_none_or(|g| behind < g) {
                out.rear = Some(behind);
            }
        }
    }
    out
}

impl crate::vehicle::VehicleInstance {
    /// Write `spot` into the bus's scripts (#2090): `BSM_Left`, `BSM_Right`, `BSM_Rear`
    /// (1 while something is there) and their `_Dist` (metres). Returns how many of the six
    /// the bus declares; a bus that declares none is left exactly as it was, since setting
    /// an undeclared variable does nothing - that is what keeps OMSI 2 add-ons working.
    pub fn apply_blind_spot(&mut self, spot: BlindSpot) -> usize {
        let mut written = 0;
        for (name, value) in BlindSpot::variables.iter().zip(spot.values()) {
            written += usize::from(self.set_var(name, value));
        }
        written
    }
}

impl super::TrafficSim {
    /// What stands beside and behind the player's bus now (#2090): the AI cars around it,
    /// measured in the bus's own frame. A car already taken off the road does not count.
    pub fn blind_spot(&self, player: PlayerBox) -> BlindSpot {
        self.blind_spot_with(player, Zone::DEFAULT)
    }

    /// [`TrafficSim::blind_spot`] with zones of the caller's size.
    pub fn blind_spot_with(&self, player: PlayerBox, zone: Zone) -> BlindSpot {
        rise(
            self.cars
                .iter()
                .filter(|c| !c.gone)
                .map(|c| (c.vehicle.position, c.half_width)),
            player,
            zone,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The player's bus: 12 m long, 2.55 m wide, facing +y (heading 0), at the origin.
    fn bus() -> PlayerBox {
        (DVec3::ZERO, 0.0, 6.0, 1.275, 0.0)
    }

    #[test]
    fn a_car_beside_the_bus_is_in_the_zone_of_its_side() {
        // a 1 m wide car whose near flank is 1.5 m from the bus's right side
        let spot = rise([(DVec3::new(3.275, 0.0, 0.0), 0.5)], bus(), Zone::default());
        assert_eq!(spot.right, Some(1.5), "{spot:?}");
        assert!(spot.left.is_none());
        assert!(spot.rear.is_none());
        // and the same on the other side
        let spot = rise(
            [(DVec3::new(-3.275, 0.0, 0.0), 0.5)],
            bus(),
            Zone::default(),
        );
        assert_eq!(spot.left, Some(1.5));
        assert!(spot.right.is_none());
    }

    #[test]
    fn the_nearest_body_is_the_one_reported() {
        let far = (DVec3::new(5.275, 0.0, 0.0), 0.5); // 3.5 m out: at the edge of the zone
        let near = (DVec3::new(2.275, 0.0, 0.0), 0.5); // 0.5 m out
        let spot = rise([far, near], bus(), Zone::default());
        assert_eq!(
            spot.right,
            Some(0.5),
            "the nearer car is the one that counts"
        );
    }

    #[test]
    fn a_car_across_the_street_or_ahead_is_in_no_zone() {
        // 25 m out: beyond the zone's width
        assert!(rise([(DVec3::new(25.0, 0.0, 0.0), 0.5)], bus(), Zone::default()).clear());
        // straight ahead, in the bus's own lane, both far away and inside the longitudinal
        // window: neither is a flank, so neither is a side blind spot
        assert!(rise([(DVec3::new(0.0, 20.0, 0.0), 0.5)], bus(), Zone::default()).clear());
        assert!(rise([(DVec3::new(0.0, 7.0, 0.0), 0.5)], bus(), Zone::default()).clear());
        // far behind, past the rear zone's reach
        assert!(rise([(DVec3::new(0.0, -40.0, 0.0), 0.5)], bus(), Zone::default()).clear());
        // overlapping the bus's own width is not outside a flank either
        assert!(rise([(DVec3::new(1.0, 0.0, 0.0), 0.5)], bus(), Zone::default()).clear());
    }

    #[test]
    fn a_car_behind_the_bus_is_measured_from_the_rear_body_line() {
        // its front 3 m behind the bus's rear (its centre 6 + 3 + 1 = 10 m behind)
        let spot = rise([(DVec3::new(0.0, -10.0, 0.0), 1.0)], bus(), Zone::default());
        assert_eq!(spot.rear, Some(3.0), "{spot:?}");
        assert!(
            spot.left.is_none() && spot.right.is_none(),
            "a car directly behind is not beside"
        );
    }

    #[test]
    fn a_car_just_behind_the_rear_is_not_in_a_side_zone() {
        // its centre 6.5 m behind the bus, so its front flank touches the rear body line
        let spot = rise([(DVec3::new(0.0, -6.5, 0.0), 0.5)], bus(), Zone::default());
        assert_eq!(spot.rear, Some(0.0), "{spot:?}");
        assert!(
            spot.left.is_none() && spot.right.is_none(),
            "the rear is not a side flank"
        );
    }

    #[test]
    fn the_zone_turns_with_the_bus() {
        // the bus heading east (+x): a car to its south (driver's right) is in the right zone
        let player: PlayerBox = (DVec3::ZERO, 90.0, 6.0, 1.275, 0.0);
        let spot = rise(
            [(DVec3::new(0.0, -3.275, 0.0), 0.5)],
            player,
            Zone::default(),
        );
        assert_eq!(spot.right, Some(1.5), "{spot:?}");
        let spot = rise(
            [(DVec3::new(0.0, 3.275, 0.0), 0.5)],
            player,
            Zone::default(),
        );
        assert_eq!(spot.left, Some(1.5));
    }

    #[test]
    fn the_variables_a_bus_reads() {
        let spot = BlindSpot {
            left: None,
            right: Some(1.5),
            rear: None,
        };
        assert_eq!(spot.values(), [0.0, 0.0, 1.0, 1.5, 0.0, 0.0]);
        assert_eq!(BlindSpot::variables.len(), spot.values().len());
        assert!(BlindSpot::default().clear());
    }
}
