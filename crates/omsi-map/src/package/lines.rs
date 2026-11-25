//! The bus lines of a map (`lines.lin`, optional): number, kind, name, the ways of its routes and where they stop, for
//! the lines panel of the city map.

use super::Package;

/// What kind of vehicle a line is for: each has its own list in the lines panel.
pub const MODAL_BUS: u8 = 0;
pub const MODAL_TRAM: u8 = 1;
pub const MODAL_METRO: u8 = 2;
pub const MODAL_RAIL: u8 = 3;

/// Kinds of bus line.
pub const GROUP_BRT: u8 = 1;
pub const GROUP_EXPRESS: u8 = 2;
pub const GROUP_FEEDER: u8 = 3;

/// One route of a line: what the official table says and the way it runs (world metres).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RouteInfo {
    pub km: f32,
    pub stops_n: u32,
    pub minutes: u32,
    pub circular: bool,
    pub days: String,
    pub model: String,
    pub itinerary: String,
    pub pts: Vec<(f64, f64)>,
    pub stops: Vec<(f64, f64)>,
}

/// A line of the map: number, kind, name and one or two routes.
#[derive(Clone, Debug, PartialEq)]
pub struct LineInfo {
    pub number: String,
    pub group: u8,
    pub modal: u8,
    /// The dates (yyyymmdd) this form of the line runs from and to; 0 is open. A map with eras has a record
    /// for each form a line took.
    pub from: u32,
    pub to: u32,
    pub name: String,
    pub routes: Vec<RouteInfo>,
}

impl Package {
    /// `lines.lin` (see `parse_lines`), called once by `open`.
    pub(super) fn load_lines(&mut self) {
        let Ok(bytes) = omsi_cfg::vfs::read(&self.dir.join("lines.lin")) else {
            return;
        };
        let text = String::from_utf8_lossy(&bytes).into_owned();
        self.lines = parse_lines(&text, &|x, y| self.author_to_world(x, y));
        if !self.lines.is_empty() {
            log::info!("map package: {} lines", self.lines.len());
        }
    }

    pub fn lines(&self) -> &[LineInfo] {
        &self.lines
    }

    /// Whether line record `i` holds on `date` (yyyymmdd; 0 = not known: all do).
    pub fn line_on(&self, i: usize, date: i32) -> bool {
        let l = &self.lines[i];
        date <= 0 || ((l.from == 0 || l.from as i32 <= date) && (l.to == 0 || date <= l.to as i32))
    }
}

/// `lines.lin` text (tab separated; `ORIGEM x y`, `L number group name [modal from to]`, then per route `R km stops
/// minutes circular days model itinerary`, `P x y x y ...` for the way and `S x y ...` for the stops, positions
/// relative to the origin) as lines, positions put onto the game's grid by `to_world`; a line with no usable way is
/// left out.
fn parse_lines(text: &str, to_world: &dyn Fn(f64, f64) -> (f64, f64)) -> Vec<LineInfo> {
    let mut origin = (0.0f64, 0.0f64);
    let mut out: Vec<LineInfo> = Vec::new();
    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        match f[0] {
            "ORIGEM" if f.len() >= 3 => {
                origin = (
                    f[1].trim().parse().unwrap_or(0.0),
                    f[2].trim().parse().unwrap_or(0.0),
                )
            }
            "L" if f.len() >= 4 => {
                let group = match f[2].trim() {
                    "BRT" => GROUP_BRT,
                    "EXP" => GROUP_EXPRESS,
                    _ => GROUP_FEEDER,
                };
                let date = |k: usize| f.get(k).and_then(|v| v.trim().parse().ok()).unwrap_or(0);
                out.push(LineInfo {
                    number: f[1].trim().to_string(),
                    group,
                    modal: match f.get(4).map(|m| m.trim()) {
                        Some("tram") => MODAL_TRAM,
                        Some("metro") => MODAL_METRO,
                        Some("rail") => MODAL_RAIL,
                        _ => MODAL_BUS,
                    },
                    from: date(5),
                    to: date(6),
                    name: f[3].trim().to_string(),
                    routes: Vec::new(),
                });
            }
            "R" if f.len() >= 8 => {
                if let Some(l) = out.last_mut() {
                    l.routes.push(RouteInfo {
                        km: f[1].trim().parse().unwrap_or(0.0),
                        stops_n: f[2].trim().parse().unwrap_or(0),
                        minutes: f[3].trim().parse().unwrap_or(0),
                        circular: f[4].trim() == "1",
                        days: f[5].trim().to_string(),
                        model: f[6].trim().to_string(),
                        itinerary: f[7].trim().to_string(),
                        ..Default::default()
                    });
                }
            }
            "P" | "S" if f.len() >= 2 => {
                let nums: Vec<f64> = f[1]
                    .split_whitespace()
                    .filter_map(|v| v.parse().ok())
                    .collect();
                let pts: Vec<(f64, f64)> = nums
                    .chunks_exact(2)
                    .map(|c| to_world(origin.0 + c[0], origin.1 + c[1]))
                    .collect();
                if let Some(r) = out.last_mut().and_then(|l| l.routes.last_mut()) {
                    if f[0] == "P" {
                        r.pts = pts;
                    } else {
                        r.stops = pts;
                    }
                }
            }
            _ => {}
        }
    }
    out.retain(|l| l.routes.iter().any(|r| r.pts.len() >= 2));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world(x: f64, y: f64) -> (f64, f64) {
        (x, y)
    }

    #[test]
    fn lines_are_read_with_their_routes() {
        let text = "# lines\nORIGEM\t1000\t2000\nL\t110\tBRT\tTerminal - Centre\nR\t12.5\t20\t35\t0\tMon-Fri\tbus_a\tTerminal - Centre\nP\t0 0 100 0 200 50\nS\t0 0 200 50\nR\t12.5\t20\t36\t0\tMon-Fri\tbus_a\tCentre - Terminal\nP\t200 50 0 0\nL\t5\tFEEDER\tNothing\nL\tT1\tBRT\tTram line\ttram\t19900101\t19911231\nR\t3\t4\t9\t0\tdaily\ttram_a\tA - B\nP\t0 0 10 10\n";
        let l = parse_lines(text, &world);
        assert_eq!(l.len(), 2, "a line with no way is left out");
        assert_eq!(
            (l[0].number.as_str(), l[0].group, l[0].modal),
            ("110", GROUP_BRT, MODAL_BUS)
        );
        assert_eq!(l[0].routes.len(), 2);
        assert_eq!(
            l[0].routes[0].pts,
            vec![(1000.0, 2000.0), (1100.0, 2000.0), (1200.0, 2050.0)]
        );
        assert_eq!(l[0].routes[0].stops.len(), 2);
        assert!((l[0].routes[0].km - 12.5).abs() < 1e-6 && l[0].routes[1].minutes == 36);
        assert_eq!(
            (l[1].number.as_str(), l[1].modal, l[1].from, l[1].to),
            ("T1", MODAL_TRAM, 19900101, 19911231)
        );
    }

    #[test]
    fn the_example_has_three_lines() {
        // docs/examples/openomsi-realmap: two bus lines (one feeder, one BRT) and a tram line
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/examples");
        let p = Package::open(&dir).expect("docs/examples/openomsi-realmap is a package");
        let l = p.lines();
        assert_eq!(
            l.iter()
                .map(|x| (x.number.as_str(), x.group, x.modal))
                .collect::<Vec<_>>(),
            [
                ("E1", GROUP_FEEDER, MODAL_BUS),
                ("E2", GROUP_BRT, MODAL_BUS),
                ("T1", GROUP_FEEDER, MODAL_TRAM)
            ]
        );
        assert_eq!(l[0].routes.len(), 2, "out and back");
        assert_eq!(l[0].routes[0].stops.len(), 2);
    }

    #[test]
    fn a_form_of_a_line_runs_between_its_dates() {
        let mut p = Package::test_stub();
        p.lines = vec![LineInfo {
            number: "T1".into(),
            group: GROUP_BRT,
            modal: MODAL_TRAM,
            from: 19900101,
            to: 19911231,
            name: String::new(),
            routes: Vec::new(),
        }];
        assert!(p.line_on(0, 0), "an unknown date lets every form hold");
        assert!(p.line_on(0, 19900101) && p.line_on(0, 19911231));
        assert!(!p.line_on(0, 19891231) && !p.line_on(0, 19920101));
    }
}
