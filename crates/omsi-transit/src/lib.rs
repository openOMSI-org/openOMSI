//! The transit data of a map: the files of its `transport` folder.
//!
//! | File | What |
//! | --- | --- |
//! | `network.cfg` | which tracks serve which mode, how they are powered |
//! | `stations.cfg` | stations and their platforms |
//! | `routes.cfg` | routes: segments between stations and the services that run over them |
//! | `catenary.cfg` | the overhead wire (tram, railway, trolleybus) |
//! | `thirdrail.cfg` | the third rail (metro) |
//! | `switches.cfg` | track and overhead switches |
//! | `depots.cfg` | depots |
//! | `signals.cfg` | signals and blocks |
//!
//! Every file is optional; a map without the folder behaves as it always did. See
//! `docs/TRANSPORT_FORMATS.md`.

pub mod doc;
pub mod electric;
pub mod infra;
pub mod power;
pub mod routes;
pub mod stations;

use doc::Issue;
use omsi_cfg::transport::ModeSet;
use std::path::Path;

pub use electric::Electrification;
pub use power::Supply;
pub use infra::{Depots, Network, Signals, Switches};
pub use routes::Routes;
pub use stations::Stations;

/// The folder inside a map's folder.
pub const FOLDER: &str = "transport";

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TransitData {
    pub network: Network,
    pub stations: Stations,
    pub routes: Routes,
    pub catenary: Option<Electrification>,
    pub third_rail: Option<Electrification>,
    pub switches: Switches,
    pub depots: Depots,
    pub signals: Signals,
    /// Which files were found.
    pub files: Vec<String>,
}

impl TransitData {
    /// Is there anything at all?
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Reads `<map_dir>/transport/*.cfg`; the problems found go to `issues`. Reading never fails:
    /// an unreadable file is an error in the list.
    pub fn load(map_dir: &Path, issues: &mut Vec<Issue>) -> TransitData {
        let dir = map_dir.join(FOLDER);
        let mut out = TransitData::default();
        let read = |name: &str, issues: &mut Vec<Issue>| -> Option<String> {
            let path = omsi_cfg::resolve_path(&dir, name);
            match std::fs::read(&path) {
                Ok(bytes) => Some(omsi_cfg::decode_text(&bytes)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => {
                    issues.push(Issue { file: name.to_string(), line: 0, error: true, message: format!("cannot read: {e}") });
                    None
                }
            }
        };
        let mut files = Vec::new();
        if let Some(t) = read("network.cfg", issues) {
            out.network = Network::parse(&t, "network.cfg", issues);
            files.push("network.cfg");
        }
        if let Some(t) = read("stations.cfg", issues) {
            out.stations = Stations::parse(&t, "stations.cfg", issues);
            files.push("stations.cfg");
        }
        if let Some(t) = read("depots.cfg", issues) {
            out.depots = Depots::parse(&t, "depots.cfg", issues);
            files.push("depots.cfg");
        }
        if let Some(t) = read("routes.cfg", issues) {
            out.routes = Routes::parse(&t, "routes.cfg", issues);
            files.push("routes.cfg");
        }
        if let Some(t) = read("catenary.cfg", issues) {
            out.catenary = Some(Electrification::parse(&t, "catenary.cfg", false, issues));
            files.push("catenary.cfg");
        }
        if let Some(t) = read("thirdrail.cfg", issues) {
            out.third_rail = Some(Electrification::parse(&t, "thirdrail.cfg", true, issues));
            files.push("thirdrail.cfg");
        }
        if let Some(t) = read("switches.cfg", issues) {
            out.switches = Switches::parse(&t, "switches.cfg", issues);
            files.push("switches.cfg");
        }
        if let Some(t) = read("signals.cfg", issues) {
            out.signals = Signals::parse(&t, "signals.cfg", issues);
            files.push("signals.cfg");
        }
        out.files = files.into_iter().map(str::to_string).collect();
        out
    }

    /// Cross-file checks: routes against stations and depots, depots against stations, stations
    /// against the map's modes, switches' signals. `map_modes` is the map's `global.cfg` modes.
    pub fn check(&self, map_modes: ModeSet, issues: &mut Vec<Issue>) {
        self.routes.check("routes.cfg", &self.stations, &self.depots.ids(), map_modes, issues);
        let mut is = doc::Issues::new("stations.cfg");
        for s in &self.stations.list {
            if !map_modes.is_empty() && !s.modes.is_empty() && !s.modes.intersects(map_modes) {
                is.warn(s.line, format!("station {} serves {}; the map is made for {}", s.id, s.modes.describe(), map_modes.describe()));
            }
        }
        issues.append(&mut is.list);
        let mut is = doc::Issues::new("depots.cfg");
        for d in &self.depots.list {
            if let Some(st) = &d.station {
                if self.stations.get(st).is_none() {
                    is.error(d.line, format!("depot {}: station {st} is not in stations.cfg", d.id));
                }
            }
        }
        issues.append(&mut is.list);
        let mut is = doc::Issues::new("switches.cfg");
        for w in &self.switches.list {
            if let Some(sig) = &w.locked_by {
                if !self.signals.signals.iter().any(|s| s.id.eq_ignore_ascii_case(sig)) {
                    is.error(w.line, format!("switch {}: signal {sig} is not in signals.cfg", w.id));
                }
            }
        }
        issues.append(&mut is.list);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_map_without_the_folder_is_empty() {
        let dir = std::env::temp_dir().join("omsi-transit-empty-test");
        let _ = std::fs::create_dir_all(&dir);
        let mut issues = Vec::new();
        let t = TransitData::load(&dir, &mut issues);
        assert!(t.is_empty() && issues.is_empty());
    }

    #[test]
    fn a_whole_map_loads_and_checks() {
        let dir = std::env::temp_dir().join(format!("omsi-transit-full-test-{}", std::process::id()));
        let tdir = dir.join(FOLDER);
        std::fs::create_dir_all(&tdir).unwrap();
        let w = |n: &str, t: &str| std::fs::write(tdir.join(n), t).unwrap();
        w("stations.cfg", "[station]\nid A\nmodes metro\n[station]\nid B\nmodes metro\n[station]\nid Dep_stop\nmodes metro\n");
        w("depots.cfg", "[depot]\nid Dep\nmodes metro\ngate 0 0 0\nstation Dep_stop\n");
        w("routes.cfg", "[route]\nid M\nmode metro\n[segment]\nid s\nfrom A\nto B\n[service]\nid x\nsegments s\nheadway 3 min\ndepot Dep\n");
        w("thirdrail.cfg", "[thirdrail_net]\n[thirdrail_section]\nid T\n[thirdrail_span]\nsection T\np 0 0 0\np 10 0 0\n");
        w("switches.cfg", "[switch]\nid W\nmodes metro\npos 1 2 3\nlocked_by S9\n");
        let mut issues = Vec::new();
        let t = TransitData::load(&dir, &mut issues);
        t.check(ModeSet::of(omsi_cfg::transport::TransportMode::Metro), &mut issues);
        let all: Vec<String> = issues.iter().map(|i| i.to_string()).collect();
        assert_eq!(all.len(), 1, "{all:?}");
        assert!(all[0].contains("signal S9"));
        assert_eq!(t.files.len(), 5);
        assert!(t.third_rail.is_some() && t.catenary.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
