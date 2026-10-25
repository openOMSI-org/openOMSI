//! Which kind of rail vehicle, which kind of track.

use omsi_vehicle::Vehicle;

/// The kind of a rail vehicle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RailClass {
    /// Street tram: light, short, sharp curves, runs in traffic.
    Tram,
    /// Underground or light metro: own right of way, third rail.
    Metro,
    /// Railway train (suburban, regional, long distance, freight).
    Train,
}

impl RailClass {
    pub fn name(self) -> &'static str {
        match self {
            RailClass::Tram => "tram",
            RailClass::Metro => "metro",
            RailClass::Train => "train",
        }
    }

    pub fn from_name(s: &str) -> Option<RailClass> {
        match s.trim().to_ascii_lowercase().as_str() {
            "tram" | "streetcar" | "strassenbahn" => Some(RailClass::Tram),
            "metro" | "subway" | "ubahn" | "u-bahn" | "underground" => Some(RailClass::Metro),
            "train" | "rail" | "railway" | "sbahn" | "s-bahn" | "freight" => Some(RailClass::Train),
            _ => None,
        }
    }

    /// The class of a vehicle, `None` for a road vehicle. A file may say it outright with
    /// `[rail_class]`. Otherwise it is told from what the file declares, the way OMSI 2's
    /// own rail vehicles differ: a `[contact_shoe]` takes its current from a third rail
    /// (metro); a heavy car (over 45 t) or a locomotive is a railway train; the rest - light cars
    /// on bogies, however long - are trams.
    pub fn of(def: &Vehicle) -> Option<RailClass> {
        use omsi_cfg::transport::TransportMode;
        match def.rail_mode()? {
            TransportMode::Metro => Some(RailClass::Metro),
            TransportMode::Railway => Some(RailClass::Train),
            _ => Some(RailClass::Tram),
        }
    }

    /// How the class performs where nothing in the vehicle's scripts tells (an AI-only
    /// script, or a vehicle driven by pedals alone): its power per tonne, the most
    /// tractive effort it starts with as an acceleration (m/s²), the braking of a service
    /// application at full pedal, the emergency one, and the speed it is built for (m/s).
    /// Figures of typical vehicles: a KT4D tram (260 kW on 18 t, 1.3 m/s² start and
    /// brake), a Berlin A3 metro car, an S-Bahn BR 481 quarter train (1.0 m/s²).
    pub fn performance(self) -> Performance {
        match self {
            RailClass::Tram => Performance { kw_per_tonne: 14.0, start_accel: 1.2, service_brake: 1.3, emergency_brake: 2.8, top_speed: 19.5 },
            RailClass::Metro => Performance { kw_per_tonne: 14.0, start_accel: 1.1, service_brake: 1.2, emergency_brake: 1.8, top_speed: 22.0 },
            RailClass::Train => Performance { kw_per_tonne: 9.0, start_accel: 0.9, service_brake: 0.9, emergency_brake: 1.3, top_speed: 44.0 },
        }
    }

    /// Rolling resistance of the running gear as a fraction of the weight at walking pace,
    /// and its rise with speed (per m/s): a tram's grooved rails and small wheels cost the
    /// most, a long train's large wheels on welded rail the least (the Davis equation's
    /// A and B terms for light rail and mainline: 2-4 N/kN and 0.02-0.04 N/kN per m/s).
    pub fn running_resistance(self) -> (f32, f32) {
        match self {
            RailClass::Tram => (0.0035, 4.0e-5),
            RailClass::Metro => (0.0025, 3.0e-5),
            RailClass::Train => (0.0015, 3.0e-5),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Performance {
    pub kw_per_tonne: f32,
    pub start_accel: f32,
    pub service_brake: f32,
    pub emergency_brake: f32,
    pub top_speed: f32,
}

/// The kind of a piece of track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrackClass {
    /// Rails laid in or beside a street, among its traffic: tram track.
    Street,
    /// Railway of its own: main line, suburban or metro, with a right of way.
    Mainline,
}

/// What is known of a piece of track to class it.
#[derive(Debug, Clone, Copy, Default)]
pub struct TrackFacts {
    /// A street lane within a few metres of it (a tram in the road).
    pub street_beside: bool,
    /// Its smallest curve radius (m).
    pub min_radius: f32,
    /// A third rail runs beside it.
    pub third_rail: bool,
}

impl TrackClass {
    /// A track beside a street is tram track; tighter than a railway ever bends (under
    /// 60 m), too; any other is a railway.
    pub fn of(f: TrackFacts) -> TrackClass {
        if f.street_beside || (f.min_radius > 0.0 && f.min_radius < 60.0 && !f.third_rail) {
            TrackClass::Street
        } else {
            TrackClass::Mainline
        }
    }

    /// Whether a vehicle of `class` belongs on this track. A tram may run on a railway
    /// (in a few cities it does) and a metro is bound to its third rail; a railway train
    /// has no business in a street, and neither has a metro.
    pub fn takes(self, class: RailClass, third_rail: bool) -> bool {
        match (self, class) {
            (_, RailClass::Metro) => third_rail && self == TrackClass::Mainline,
            (TrackClass::Street, RailClass::Train) => false,
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_vehicle::vehicle::Axle;

    fn rail(mass: f32, length: f32, driven_axles: usize) -> Vehicle {
        let axle = |driven| Axle { long: 0.0, max_width: 1.4, min_width: 1.4, wheel_diameter: 0.7, spring: 100.0, max_force: 100.0, damper: 10.0, driven, inertia_inv: 0.0 };
        Vehicle { mass, boogies: Some(5.0), axles: (0..4).map(|i| axle(i < driven_axles)).collect(), bounding_box: Some([2.4, length, 3.4, 0.0, 0.0, 0.0]), ..Default::default() }
    }

    #[test]
    fn a_road_vehicle_has_no_rail_class() {
        assert_eq!(RailClass::of(&Vehicle { mass: 12.0, ..Default::default() }), None);
    }

    #[test]
    fn size_and_shoes_tell_the_kind() {
        assert_eq!(RailClass::of(&rail(18.0, 14.0, 4)), Some(RailClass::Tram), "a KT4D");
        assert_eq!(RailClass::of(&rail(80.0, 20.0, 4)), Some(RailClass::Train), "a locomotive");
        assert_eq!(RailClass::of(&rail(52.0, 26.0, 2)), Some(RailClass::Train), "a heavy coach");
        assert_eq!(RailClass::of(&rail(40.0, 42.0, 4)), Some(RailClass::Tram), "a long low-floor tram is no train");
        let mut metro = rail(30.0, 16.0, 4);
        metro.contact_shoes.push([0.0; 6]);
        assert_eq!(RailClass::of(&metro), Some(RailClass::Metro), "a third rail");
        // the file may say it
        let mut tram = rail(80.0, 20.0, 4);
        tram.rail_class = Some("Tram".into());
        assert_eq!(RailClass::of(&tram), Some(RailClass::Tram));
        assert_eq!(RailClass::from_name("U-Bahn"), Some(RailClass::Metro));
        assert_eq!(RailClass::from_name("bus"), None);
    }

    #[test]
    fn tracks_are_told_by_their_surroundings() {
        let street = TrackFacts { street_beside: true, min_radius: 300.0, third_rail: false };
        let line = TrackFacts { street_beside: false, min_radius: 400.0, third_rail: false };
        let loop_ = TrackFacts { street_beside: false, min_radius: 25.0, third_rail: false };
        assert_eq!(TrackClass::of(street), TrackClass::Street);
        assert_eq!(TrackClass::of(line), TrackClass::Mainline);
        assert_eq!(TrackClass::of(loop_), TrackClass::Street, "no railway bends so tight");
        assert_eq!(TrackClass::of(TrackFacts { third_rail: true, ..loop_ }), TrackClass::Mainline);
    }

    #[test]
    fn every_class_knows_its_track() {
        use RailClass::*;
        assert!(TrackClass::Street.takes(Tram, false));
        assert!(TrackClass::Mainline.takes(Tram, false));
        assert!(!TrackClass::Street.takes(Train, false), "a train does not run in the street");
        assert!(TrackClass::Mainline.takes(Train, false));
        assert!(TrackClass::Mainline.takes(Metro, true));
        assert!(!TrackClass::Mainline.takes(Metro, false), "a metro needs its third rail");
        assert!(!TrackClass::Street.takes(Metro, true));
    }
}
