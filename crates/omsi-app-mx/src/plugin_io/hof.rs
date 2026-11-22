//! Convert the loaded HOF's parsed data to owned plugin snapshots. No disk I/O.
use omsi_plugin::api::Value;
use omsi_vehicle::hof::{BusStop, Hof, Terminus};

fn record(fields: Vec<(&str, Value)>) -> Value {
    Value::Map(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

fn strings(values: &[String]) -> Value {
    Value::List(values.iter().cloned().map(Value::Str).collect())
}

fn lists(values: &[Vec<String>]) -> Value {
    Value::List(values.iter().map(|row| strings(row)).collect())
}

fn terminus(index: usize, t: &Terminus) -> Value {
    record(vec![
        ("index", (index as i64).into()),
        ("code", (t.code as i64).into()),
        ("identifier", t.texture_id.clone().into()),
        ("terminus_stop", t.terminus_stop.clone().into()),
        ("all_exit", t.all_exit.into()),
        ("strings", strings(&t.strings)),
    ])
}

fn busstop(index: usize, s: &BusStop) -> Value {
    record(vec![
        ("index", (index as i64).into()),
        ("identifier", s.ident.clone().into()),
        ("strings", strings(&s.strings)),
    ])
}

pub(crate) fn find_terminus(hof: &Hof, identifier: &str) -> Option<Value> {
    hof.termini
        .iter()
        .enumerate()
        .find(|(_, t)| t.texture_id == identifier)
        .map(|(i, t)| terminus(i, t))
}

pub(crate) fn find_busstop(hof: &Hof, identifier: &str) -> Option<Value> {
    hof.bus_stops
        .iter()
        .enumerate()
        .find(|(_, s)| s.ident == identifier)
        .map(|(i, s)| busstop(i, s))
}

pub(crate) fn snapshot(hof: &Hof) -> Value {
    record(vec![
        ("path", hof.path.to_string_lossy().into_owned().into()),
        ("name", hof.name.clone().into()),
        ("service_trip", hof.service_trip.clone().into()),
        ("global_strings", strings(&hof.global_strings)),
        (
            "string_count_terminus",
            (hof.string_count_terminus as i64).into(),
        ),
        (
            "string_count_busstop",
            (hof.string_count_busstop as i64).into(),
        ),
        (
            "termini",
            Value::List(
                hof.termini
                    .iter()
                    .enumerate()
                    .map(|(i, t)| terminus(i, t))
                    .collect(),
            ),
        ),
        (
            "bus_stops",
            Value::List(
                hof.bus_stops
                    .iter()
                    .enumerate()
                    .map(|(i, s)| busstop(i, s))
                    .collect(),
            ),
        ),
        (
            "info_trips",
            Value::List(
                hof.info_trips
                    .iter()
                    .enumerate()
                    .map(|(i, t)| {
                        record(vec![
                            ("index", (i as i64).into()),
                            ("code", t.code.clone().into()),
                            ("name", t.name.clone().into()),
                            ("route", t.route.clone().into()),
                            ("line", t.line.clone().into()),
                            ("extra", strings(&t.extra)),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("info_busstop_lists", lists(&hof.info_busstop_lists)),
        ("info_busstops", lists(&hof.info_busstops)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_vehicle::hof::InfoTrip;

    fn fixture() -> Hof {
        Hof {
            path: "Vehicles/Bus/Depot.hof".into(),
            name: "Délpest".into(),
            service_trip: "Empty".into(),
            global_strings: vec![String::new(), "Őrs".into()],
            string_count_terminus: 10,
            string_count_busstop: 2,
            termini: vec![
                Terminus {
                    code: 123,
                    texture_id: "First".into(),
                    all_exit: true,
                    strings: vec![String::new()],
                    ..Default::default()
                },
                Terminus {
                    code: 123,
                    texture_id: "[184] End (1)".into(),
                    terminus_stop: Some("Stop ID".into()),
                    strings: (0..10).map(|i| format!("field {i}")).collect(),
                    ..Default::default()
                },
                Terminus {
                    code: 456,
                    texture_id: "[184] End (1)".into(),
                    ..Default::default()
                },
            ],
            bus_stops: vec![BusStop {
                ident: "Stop ID".into(),
                strings: vec!["Stop display".into(), String::new()],
            }],
            info_trips: vec![InfoTrip {
                code: "0018401".into(),
                name: "Outward".into(),
                route: "1".into(),
                line: "184".into(),
                extra: vec!["custom".into(), String::new()],
            }],
            info_busstop_lists: vec![vec!["Stop ID".into(), "Other ID".into()]],
            info_busstops: vec![vec!["raw".into(), String::new()]],
        }
    }

    #[test]
    fn exact_lookup_uses_internal_identifier_and_first_matching_index() {
        let h = fixture();
        let t = find_terminus(&h, "[184] End (1)").unwrap();
        assert_eq!(t.get("index"), Some(&1.into()));
        assert_eq!(t.get("code"), Some(&123.into()));
        assert_eq!(t.get("terminus_stop"), Some(&"Stop ID".into()));
        assert_eq!(t.get("strings").unwrap().items()[8], "field 8".into());
        assert_eq!(
            find_terminus(&h, "First").unwrap().get("terminus_stop"),
            Some(&Value::Nil)
        );
        assert_eq!(
            find_terminus(&h, "First").unwrap().get("all_exit"),
            Some(&true.into())
        );
        for name in [
            "[184] end (1)",
            " [184] End (1)",
            "123",
            "field 0",
            "missing",
        ] {
            assert!(find_terminus(&h, name).is_none(), "{name}");
        }
        assert_eq!(
            find_busstop(&h, "Stop ID").unwrap().get("index"),
            Some(&0.into())
        );
        assert!(find_busstop(&h, "stop id").is_none());
        assert!(find_terminus(&Hof::default(), "missing").is_none());
    }

    #[test]
    fn snapshot_preserves_all_parsed_fields_and_is_owned() {
        let mut h = fixture();
        let data = snapshot(&h);
        assert_eq!(data.get("path"), Some(&"Vehicles/Bus/Depot.hof".into()));
        assert_eq!(data.get("name"), Some(&"Délpest".into()));
        assert_eq!(data.get("service_trip"), Some(&"Empty".into()));
        assert_eq!(
            data.get("global_strings"),
            Some(&strings(&h.global_strings))
        );
        assert_eq!(data.get("string_count_terminus"), Some(&10.into()));
        assert_eq!(data.get("string_count_busstop"), Some(&2.into()));
        assert_eq!(
            data.get("termini").unwrap().items()[1],
            find_terminus(&h, "[184] End (1)").unwrap()
        );
        assert_eq!(
            data.get("bus_stops").unwrap().items()[0],
            find_busstop(&h, "Stop ID").unwrap()
        );
        let trip = &data.get("info_trips").unwrap().items()[0];
        for (key, value) in [
            ("code", "0018401"),
            ("name", "Outward"),
            ("route", "1"),
            ("line", "184"),
        ] {
            assert_eq!(trip.get(key), Some(&value.into()));
        }
        assert_eq!(trip.get("extra"), Some(&strings(&h.info_trips[0].extra)));
        assert_eq!(
            data.get("info_busstop_lists"),
            Some(&lists(&h.info_busstop_lists))
        );
        assert_eq!(data.get("info_busstops"), Some(&lists(&h.info_busstops)));
        h.global_strings[1] = "new depot".into();
        h.termini[1].texture_id = "New ID".into();
        assert_ne!(data, snapshot(&h));
        assert_eq!(data.get("global_strings").unwrap().items()[1], "Őrs".into());
        assert!(find_terminus(&h, "[184] End (1)").is_some()); // the duplicate is now first
        assert!(find_terminus(&h, "New ID").is_some());
        assert_eq!(
            snapshot(&Hof::default()).get("termini"),
            Some(&Value::List(vec![]))
        );
    }
}
