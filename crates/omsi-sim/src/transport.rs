//! Which transport mode a vehicle belongs to, for the map-admission rule
//! ([`omsi_cfg::transport::admit`]).

use omsi_cfg::transport::{admit, Admission, ModeSet};
use omsi_vehicle::Vehicle;

/// The modes of a vehicle and whether its file declared them. A file that names none is judged
/// by its shape, so every OMSI 2 vehicle and mod keeps working: a rail vehicle by its
/// [`RailClass`] (tram, metro, railway), a vehicle with `[trolley_pole]` gear a bus and a
/// trolleybus (it also runs where only buses are expected, as it always did), anything else a bus.
pub fn modes_of(def: &Vehicle) -> (ModeSet, bool) {
    def.transport_modes()
}

/// May `def` run on a map built for `map` modes?
pub fn admission(map: ModeSet, def: &Vehicle) -> Admission {
    let (modes, declared) = modes_of(def);
    admit(map, modes, declared)
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_cfg::transport::TransportMode;
    use omsi_cfg::CfgFile;

    fn vehicle(text: &str) -> Vehicle {
        Vehicle::parse(&CfgFile::from_str("x.bus", text))
    }

    #[test]
    fn untagged_vehicles_are_judged_by_their_shape() {
        let bus = vehicle("[mass]\n12\n");
        assert_eq!(modes_of(&bus), (ModeSet::of(TransportMode::Bus), false));
        let tram = vehicle("[mass]\n18\n[boogies]\n1\n");
        assert_eq!(modes_of(&tram), (ModeSet::of(TransportMode::Tram), false));
        let metro = vehicle("[mass]\n30\n[boogies]\n1\n[contact_shoe]\n1\n0\n0\n0\n0\n0\n");
        assert_eq!(modes_of(&metro).0, ModeSet::of(TransportMode::Metro));
        let trolley = vehicle("[mass]\n12\n[trolley_pole]\nside left\npos -0.3 1.0 4.1\n");
        assert!(modes_of(&trolley).0.contains(TransportMode::Bus) && modes_of(&trolley).0.contains(TransportMode::Trolleybus));
    }

    #[test]
    fn a_declared_mode_wins_over_the_shape() {
        let v = vehicle("[mass]\n18\n[boogies]\n1\n[railway_transport]\n");
        assert_eq!(modes_of(&v), (ModeSet::of(TransportMode::Railway), true));
    }

    #[test]
    fn the_map_decides() {
        let bus = vehicle("[mass]\n12\n");
        let tram = vehicle("[mass]\n18\n[boogies]\n1\n[tram_transport]\n");
        let tram_map = ModeSet::of(TransportMode::Tram);
        assert!(!admission(tram_map, &bus).is_allowed());
        assert!(admission(tram_map, &tram).is_allowed());
        assert!(admission(ModeSet::EMPTY, &bus).is_allowed());
        assert!(admission(ModeSet::EMPTY, &tram).is_allowed());
    }
}
