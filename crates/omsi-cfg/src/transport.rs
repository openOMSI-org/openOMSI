//! Transport modes: which kind of public transport a vehicle is and a map is made for.
//!
//! OMSI 2 knows buses, and a single rail flag. openOMSI lets a vehicle or a map say what it
//! is with one bare keyword per mode (an openOMSI extension; OMSI 2 skips what it does not
//! know, so a file that uses them still loads there):
//!
//! ```text
//! [bus_transport]
//! [tram_transport]
//! [railway_transport]
//! [metro_transport]
//! [trolleybus_transport]
//! ```
//!
//! A vehicle names the modes it runs in (mostly one), a map the modes it is built for.
//! [`admit`] is the one place that decides whether a vehicle may run on a map:
//!
//! * a map that names no mode takes every vehicle (all OMSI 2 maps and mods);
//! * a vehicle that names no mode is judged by the mode the game works out for it from its
//!   file (see `omsi_sim::transport`), so a stock bus is still a bus;
//! * otherwise the vehicle's modes and the map's must share one.

use std::fmt;

/// A kind of public transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TransportMode {
    Bus,
    Tram,
    Railway,
    Metro,
    Trolleybus,
}

impl TransportMode {
    pub const ALL: [TransportMode; 5] = [TransportMode::Bus, TransportMode::Tram, TransportMode::Railway, TransportMode::Metro, TransportMode::Trolleybus];

    /// The mode's name as the launcher and the log show it.
    pub fn name(self) -> &'static str {
        match self {
            TransportMode::Bus => "bus",
            TransportMode::Tram => "tram",
            TransportMode::Railway => "railway",
            TransportMode::Metro => "metro",
            TransportMode::Trolleybus => "trolleybus",
        }
    }

    /// The keyword (lower-cased, without brackets) that declares the mode in a file.
    pub fn keyword(self) -> &'static str {
        match self {
            TransportMode::Bus => "bus_transport",
            TransportMode::Tram => "tram_transport",
            TransportMode::Railway => "railway_transport",
            TransportMode::Metro => "metro_transport",
            TransportMode::Trolleybus => "trolleybus_transport",
        }
    }

    /// The mode a keyword such as `bus_transport` declares.
    pub fn from_keyword(k: &str) -> Option<TransportMode> {
        TransportMode::ALL.into_iter().find(|m| m.keyword().eq_ignore_ascii_case(k.trim()))
    }

    /// The mode a free-form name stands for (`tram`, `Strassenbahn`, `u-bahn`, ...).
    pub fn from_name(s: &str) -> Option<TransportMode> {
        match s.trim().to_ascii_lowercase().as_str() {
            "bus" | "coach" => Some(TransportMode::Bus),
            "tram" | "streetcar" | "strassenbahn" | "tramway" => Some(TransportMode::Tram),
            "railway" | "rail" | "train" | "sbahn" | "s-bahn" | "freight" => Some(TransportMode::Railway),
            "metro" | "subway" | "ubahn" | "u-bahn" | "underground" => Some(TransportMode::Metro),
            "trolleybus" | "trolley" | "trolleybuses" | "trolleycoach" => Some(TransportMode::Trolleybus),
            _ => None,
        }
    }

    /// Runs on rails.
    pub fn is_rail(self) -> bool {
        matches!(self, TransportMode::Tram | TransportMode::Railway | TransportMode::Metro)
    }

    /// Runs on the road (buses, trolleybuses).
    pub fn is_road(self) -> bool {
        !self.is_rail()
    }

    fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

impl fmt::Display for TransportMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A set of modes. Empty means "not declared".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct ModeSet(u8);

impl ModeSet {
    pub const EMPTY: ModeSet = ModeSet(0);

    pub fn of(m: TransportMode) -> ModeSet {
        ModeSet(m.bit())
    }

    pub fn insert(&mut self, m: TransportMode) {
        self.0 |= m.bit();
    }

    pub fn contains(self, m: TransportMode) -> bool {
        self.0 & m.bit() != 0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn intersects(self, other: ModeSet) -> bool {
        self.0 & other.0 != 0
    }

    pub fn iter(self) -> impl Iterator<Item = TransportMode> {
        TransportMode::ALL.into_iter().filter(move |m| self.contains(*m))
    }

    /// The first (lowest) mode of the set.
    pub fn first(self) -> Option<TransportMode> {
        self.iter().next()
    }

    /// `bus, tram` (empty for an empty set).
    pub fn describe(self) -> String {
        self.iter().map(|m| m.name()).collect::<Vec<_>>().join(", ")
    }

    /// Reads the mode a keyword declares into the set; false for any other keyword.
    pub fn declare_keyword(&mut self, keyword: &str) -> bool {
        match TransportMode::from_keyword(keyword) {
            Some(m) => {
                self.insert(m);
                true
            }
            None => false,
        }
    }

    /// Reads a list of names (`tram metro`, `tram, metro`).
    pub fn parse_names(text: &str) -> ModeSet {
        let mut s = ModeSet::EMPTY;
        for w in text.split(|c: char| c == ',' || c == ';' || c.is_whitespace()) {
            if let Some(m) = TransportMode::from_name(w) {
                s.insert(m);
            }
        }
        s
    }
}

impl FromIterator<TransportMode> for ModeSet {
    fn from_iter<T: IntoIterator<Item = TransportMode>>(iter: T) -> Self {
        let mut s = ModeSet::EMPTY;
        for m in iter {
            s.insert(m);
        }
        s
    }
}

/// What [`admit`] says about a vehicle on a map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Admission {
    /// Runs: the map names no mode, or the vehicle shares one with it.
    Allowed,
    /// The vehicle does not run on this map; the text says why, ready for a dialog or the log.
    Refused(String),
}

impl Admission {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Admission::Allowed)
    }
}

/// May a vehicle with `vehicle` modes (declared, or worked out when the file declares none) run
/// on a map built for `map` modes? `declared` says whether the vehicle's file named its modes;
/// it only changes the wording of a refusal.
pub fn admit(map: ModeSet, vehicle: ModeSet, declared: bool) -> Admission {
    if map.is_empty() || map.intersects(vehicle) {
        return Admission::Allowed;
    }
    let how = if declared { "is made for" } else { "counts as" };
    Admission::Refused(format!("This map is made for {} only; the vehicle {how} {}.", map.describe(), if vehicle.is_empty() { "nothing known".to_string() } else { vehicle.describe() }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use TransportMode::*;

    #[test]
    fn keywords_round_trip() {
        for m in TransportMode::ALL {
            assert_eq!(TransportMode::from_keyword(m.keyword()), Some(m));
            assert_eq!(TransportMode::from_keyword(&m.keyword().to_ascii_uppercase()), Some(m));
        }
        assert_eq!(TransportMode::from_keyword("rail_class"), None);
    }

    #[test]
    fn names_cover_the_usual_spellings() {
        assert_eq!(TransportMode::from_name("Strassenbahn"), Some(Tram));
        assert_eq!(TransportMode::from_name("u-bahn"), Some(Metro));
        assert_eq!(TransportMode::from_name("train"), Some(Railway));
        assert_eq!(TransportMode::from_name("spaceship"), None);
        assert_eq!(ModeSet::parse_names("tram, metro").describe(), "tram, metro");
    }

    #[test]
    fn a_map_without_modes_takes_everything() {
        assert!(admit(ModeSet::EMPTY, ModeSet::of(Bus), true).is_allowed());
        assert!(admit(ModeSet::EMPTY, ModeSet::EMPTY, false).is_allowed());
    }

    #[test]
    fn a_bus_does_not_run_on_a_tram_map() {
        let tram_map = ModeSet::of(Tram);
        assert!(!admit(tram_map, ModeSet::of(Bus), true).is_allowed());
        assert!(admit(tram_map, ModeSet::of(Tram), true).is_allowed());
        let mixed: ModeSet = [Bus, Tram].into_iter().collect();
        assert!(admit(mixed, ModeSet::of(Bus), false).is_allowed());
        assert!(!admit(mixed, ModeSet::of(Metro), true).is_allowed());
    }

    #[test]
    fn refusal_names_both_sides() {
        let Admission::Refused(t) = admit(ModeSet::of(Metro), ModeSet::of(Bus), false) else { panic!() };
        assert!(t.contains("metro") && t.contains("bus") && t.contains("counts as"), "{t}");
    }
}
