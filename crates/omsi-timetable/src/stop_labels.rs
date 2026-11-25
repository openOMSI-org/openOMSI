//! Stop labels (#2462): what a stop's sign says, worked out from the map's own timetable.
//!
//! A map author draws every `Haltestellenschild` by hand today - the name, the lines and the
//! directions the lines go in, copied out of the timetable into the sign's texture or a
//! `[texttexture]`. All of it is already in `TTData`: the stop is in `Busstops.cfg`, the trips
//! that call at it are in `*.ttp`, and each of those trips names its line and its terminus.
//! This module turns that into one label per stop, and writes the labels out as a CSV a sign
//! generator (or a person with a text editor) can read.
//!
//! What is *not* here: drawing the sign. The picture, its design and where it is placed on the
//! map are the map's business; this is the data side, which is the part that can be wrong
//! (a stop served in one direction only, a name written two ways, a platform nobody calls at).

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use crate::TimetableData;

/// One line calling at a stop, and where it goes from there.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StopLine {
    /// The line's name as the timetable writes it (`Line::name`, also the trip's `line`).
    pub line: String,
    /// The destinations the line's trips reaching this stop end at, sorted. Two directions of
    /// the same line give two: the sign in the middle of a route names both.
    pub destinations: Vec<String>,
}

/// A stop's sign: the name, and the lines that call at it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StopLabel {
    /// The stop's id, as `Trip::stations` and the map's stop objects use it.
    pub id: i64,
    /// The platform: a stop served on both sides of the street is two `[busstop]` entries
    /// with one name and two groups.
    pub platform: i32,
    pub name: String,
    pub lines: Vec<StopLine>,
}

impl StopLabel {
    /// What the sign reads: `58 | 58, Spandau, Rathaus`. A stop no trip calls at has no lines,
    /// and only its name.
    pub fn text(&self) -> String {
        if self.lines.is_empty() {
            return self.name.clone();
        }
        let lines: Vec<String> = self
            .lines
            .iter()
            .map(|l| {
                if l.destinations.is_empty() {
                    l.line.clone()
                } else {
                    format!("{} {}", l.line, l.destinations.join(" / "))
                }
            })
            .collect();
        format!("{} | {}", self.name, lines.join(", "))
    }
}

/// The label of every stop of a map, in the map's order.
///
/// A stop no trip calls at is kept: its sign exists on the map whether or not a line uses it
/// (out of service, or a timetable that has not been written yet), and dropping it would hide
/// exactly the stops a map author wants to see on the list.
pub fn labels(data: &TimetableData) -> Vec<StopLabel> {
    // the names of the stops, by id: a trip whose terminus is empty ends at the last stop it
    // calls at, and that stop's name is its destination
    let names: BTreeMap<i64, &str> = data
        .bus_stops
        .iter()
        .map(|s| (s.object_id, s.name.as_str()))
        .collect();
    // (line, destination) per stop id
    let mut served: BTreeMap<i64, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    for trip in &data.trips {
        let line = trip.line.trim();
        if line.is_empty() {
            continue;
        }
        let mut destination = trip.terminus.trim().to_string();
        if destination.is_empty() {
            destination = trip
                .stations
                .last()
                .and_then(|id| names.get(id))
                .map(|n| n.trim().to_string())
                .unwrap_or_default();
        }
        let mut seen: Vec<i64> = Vec::new();
        for id in &trip.stations {
            if seen.contains(id) {
                continue;
            }
            seen.push(*id);
            let entry = served
                .entry(*id)
                .or_default()
                .entry(line.to_string())
                .or_default();
            if !destination.is_empty() && !entry.iter().any(|d| d == &destination) {
                entry.push(destination.clone());
            }
        }
    }
    data.bus_stops
        .iter()
        .map(|s| {
            let mut lines: Vec<StopLine> = served
                .get(&s.object_id)
                .map(|m| {
                    m.iter()
                        .map(|(line, destinations)| {
                            let mut destinations = destinations.clone();
                            destinations.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
                            StopLine {
                                line: line.clone(),
                                destinations,
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            lines.sort_by(|a, b| line_order(&a.line, &b.line));
            StopLabel {
                id: s.object_id,
                platform: s.group,
                name: s.name.clone(),
                lines,
            }
        })
        .collect()
}

/// Lines by their number where they have one: 5 before 12 before 58 before N1 before X30, the
/// way a sign is read, and not the alphabetical order that puts 12 before 5.
fn line_order(a: &str, b: &str) -> std::cmp::Ordering {
    fn key(s: &str) -> (u8, u64, String) {
        let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
        match digits.parse::<u64>() {
            Ok(n) if digits.len() == s.trim().len() => (0, n, String::new()),
            Ok(n) => (1, n, s.trim().to_lowercase()),
            Err(_) => (2, 0, s.trim().to_lowercase()),
        }
    }
    key(a).cmp(&key(b))
}

/// The CSV's header: one row per line at a stop, `destination` repeated for each of them.
pub const CSV_HEADER: &str = "stop_id,platform,name,line,destination";

/// The labels as CSV (RFC 4180 quoting), a row per stop and line; a stop with no line is one
/// row with the two empty fields.
pub fn to_csv(labels: &[StopLabel]) -> String {
    let mut out = String::from(CSV_HEADER);
    out.push_str("\r\n");
    for l in labels {
        if l.lines.is_empty() {
            row(
                &mut out,
                &[&l.id.to_string(), &l.platform.to_string(), &l.name, "", ""],
            );
            continue;
        }
        for line in &l.lines {
            if line.destinations.is_empty() {
                row(
                    &mut out,
                    &[
                        &l.id.to_string(),
                        &l.platform.to_string(),
                        &l.name,
                        &line.line,
                        "",
                    ],
                );
            } else {
                for d in &line.destinations {
                    row(
                        &mut out,
                        &[
                            &l.id.to_string(),
                            &l.platform.to_string(),
                            &l.name,
                            &line.line,
                            d,
                        ],
                    );
                }
            }
        }
    }
    out
}

fn row(out: &mut String, fields: &[&str]) {
    for (i, f) in fields.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        if f.contains([',', '"', '\n', '\r']) {
            out.push('"');
            out.push_str(&f.replace('"', "\"\""));
            out.push('"');
        } else {
            out.push_str(f);
        }
    }
    out.push_str("\r\n");
}

/// Write the labels of the map in `map_dir` (the folder holding `global.cfg`, with its
/// `TTData`) to `out`; how many stops the file lists.
pub fn export(map_dir: &Path, out: &Path) -> std::io::Result<usize> {
    let data = TimetableData::load(map_dir);
    let labels = labels(&data);
    let text = to_csv(&labels);
    if let Some(dir) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let mut f = std::fs::File::create(out)?;
    f.write_all(text.as_bytes())?;
    Ok(labels.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BusStopEntry, Trip};

    fn stop(name: &str, group: i32, id: i64) -> BusStopEntry {
        BusStopEntry {
            name: name.to_string(),
            group,
            object_id: id,
            params: [0.0; 3],
        }
    }

    fn trip(line: &str, terminus: &str, stations: &[i64]) -> Trip {
        Trip {
            line: line.to_string(),
            terminus: terminus.to_string(),
            stations: stations.to_vec(),
            ..Default::default()
        }
    }

    fn data(stops: Vec<BusStopEntry>, trips: Vec<Trip>) -> TimetableData {
        TimetableData {
            bus_stops: stops,
            trips,
            ..Default::default()
        }
    }

    #[test]
    fn a_stop_is_named_with_its_lines_and_both_directions() {
        let d = data(
            vec![stop("Rathaus", 0, 101)],
            vec![
                trip("58", "Spandau", &[100, 101, 102]),
                trip("58", "Rathaus", &[102, 101, 100]),
                trip("5", "Hauptbahnhof", &[101, 103]),
            ],
        );
        let l = &labels(&d)[0];
        assert_eq!(l.name, "Rathaus");
        assert_eq!(
            l.lines,
            vec![
                StopLine {
                    line: "5".into(),
                    destinations: vec!["Hauptbahnhof".into()]
                },
                StopLine {
                    line: "58".into(),
                    destinations: vec!["Rathaus".into(), "Spandau".into()]
                },
            ],
            "two directions of one line, and 5 before 58"
        );
    }

    #[test]
    fn a_stop_no_line_calls_at_keeps_its_sign() {
        let d = data(
            vec![stop("Betriebshof", 0, 7)],
            vec![trip("5", "Hauptbahnhof", &[101])],
        );
        let l = &labels(&d)[0];
        assert!(l.lines.is_empty());
        assert_eq!(l.text(), "Betriebshof");
        assert_eq!(
            to_csv(&labels(&d)),
            format!("{CSV_HEADER}\r\n7,0,Betriebshof,,\r\n")
        );
    }

    #[test]
    fn a_trip_without_a_terminus_ends_where_its_last_stop_is() {
        let d = data(
            vec![stop("Rathaus", 0, 101), stop("Hauptbahnhof", 0, 103)],
            vec![trip("5", "", &[101, 103])],
        );
        let l = &labels(&d)[0];
        assert_eq!(
            l.lines[0].destinations,
            vec!["Hauptbahnhof"],
            "the last stop names the direction"
        );
    }

    #[test]
    fn both_platforms_of_one_stop_get_a_label() {
        let d = data(
            vec![stop("Rathaus", 0, 101), stop("Rathaus", 1, 101)],
            vec![trip("5", "Hauptbahnhof", &[101, 103])],
        );
        let ls = labels(&d);
        assert_eq!(ls.len(), 2, "two platforms, two signs");
        assert_eq!((ls[0].platform, ls[1].platform), (0, 1));
        assert!(
            ls.iter().all(|l| l.lines.len() == 1),
            "the trips name the stop id, not the platform"
        );
    }

    #[test]
    fn a_trip_that_calls_twice_gives_one_destination() {
        // (a loop: the same stop before and after a turning circle)
        let d = data(
            vec![stop("Rathaus", 0, 101)],
            vec![trip("5", "Rathaus", &[101, 102, 101])],
        );
        let l = &labels(&d)[0];
        assert_eq!(l.lines[0].destinations, vec!["Rathaus"]);
    }

    #[test]
    fn the_csv_quotes_what_csv_must() {
        let d = data(
            vec![stop("Rathaus, \"alt\"", 0, 101)],
            vec![trip("5", "Hauptbahnhof, Nord", &[101])],
        );
        let csv = to_csv(&labels(&d));
        assert!(
            csv.contains("\r\n101,0,\"Rathaus, \"\"alt\"\"\",5,\"Hauptbahnhof, Nord\"\r\n"),
            "{csv}"
        );
    }

    #[test]
    fn lines_are_ordered_as_a_sign_reads_them() {
        let mut v = vec!["X30", "12", "N1", "5", "58"];
        v.sort_by(|a, b| super::line_order(a, b));
        assert_eq!(v, ["5", "12", "58", "N1", "X30"]);
    }

    #[test]
    fn the_labels_come_out_of_a_map_s_folder() {
        // (a map as the game reads it: `global.cfg` beside a `TTData` folder, one stop used by
        // one line and one stop nobody calls at)
        let dir = std::env::temp_dir().join(format!("openomsi-stop-labels-{}", std::process::id()));
        let tt = dir.join("TTData");
        std::fs::create_dir_all(&tt).unwrap();
        std::fs::write(dir.join("global.cfg"), "[name]\nLabel town\n").unwrap();
        std::fs::write(
            tt.join("Busstops.cfg"),
            "[busstop]\nRathaus\n0\n101\n0\n0\n0\n\n[busstop]\nBetriebshof\n0\n7\n0\n0\n0\n",
        )
        .unwrap();
        std::fs::write(tt.join("5.ttl"), "[line]\n5\n").unwrap();
        std::fs::write(
            tt.join("5-1.ttp"),
            "[trip]\n\nHauptbahnhof\n5\n\n[station_typ2]\n101\n[station_typ2]\n103\n",
        )
        .unwrap();
        let out = dir.join("out/stops.csv");
        let n = super::export(&dir, &out).unwrap();
        let csv = std::fs::read_to_string(&out).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(n, 2, "two stops, one of them out of service");
        assert_eq!(
            csv,
            format!("{CSV_HEADER}\r\n101,0,Rathaus,5,Hauptbahnhof\r\n7,0,Betriebshof,,\r\n")
        );
    }

    #[test]
    fn the_signs_text_reads_as_a_sign() {
        let l = StopLabel {
            id: 101,
            platform: 0,
            name: "Rathaus".into(),
            lines: vec![
                StopLine {
                    line: "5".into(),
                    destinations: vec!["Hauptbahnhof".into()],
                },
                StopLine {
                    line: "58".into(),
                    destinations: vec!["Rathaus".into(), "Spandau".into()],
                },
            ],
        };
        assert_eq!(l.text(), "Rathaus | 5 Hauptbahnhof, 58 Rathaus / Spandau");
    }
}
