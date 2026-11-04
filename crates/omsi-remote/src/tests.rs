use super::*;
use crate::layouts::{file_name, Scope, Store};
use crate::pairing::{Devices, Pairing};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

fn temp_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let d = std::env::temp_dir().join(format!("omsi-remote-{name}-{}-{nanos}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn layout(theme: &str) -> Value {
    json!({ "v": 1, "theme": theme, "pages": [{ "name": "P", "widgets": [] }] })
}

#[test]
fn commands_are_read_and_odd_names_refused() {
    let c = |v: Value| json::command(&v);
    assert_eq!(c(json!({ "t": "act", "name": "door_1", "down": true })), Some(Command::Action { name: "door_1".into(), down: true }));
    assert_eq!(c(json!({ "t": "act", "name": "door 1", "down": true })), None);
    assert_eq!(c(json!({ "t": "act", "name": "x".repeat(65), "down": true })), None);
    assert_eq!(c(json!({ "t": "act", "name": "horn" })), None);
    assert_eq!(c(json!({ "t": "indicator", "value": 3 })), Some(Command::Indicator(3)));
    assert_eq!(c(json!({ "t": "indicator", "value": 4 })), None);
    assert_eq!(c(json!({ "t": "route", "index": 2 })), Some(Command::Route(2)));
    assert_eq!(c(json!({ "t": "route", "index": -1 })), None);
    assert_eq!(c(json!({ "t": "line", "line": " 32E " })), Some(Command::Line("32E".into())));
    assert_eq!(c(json!({ "t": "line", "line": "" })), None);
    assert_eq!(c(json!({ "t": "game", "name": "view_set_outside" })), Some(Command::Game("view_set_outside".into())));
    assert_eq!(c(json!({ "t": "nope" })), None);
    assert_eq!(json::drive(&json!({ "s": 0.25, "g": 2.0, "b": -1.0 })), Some((0.25, 1.0, 0.0)));
    assert_eq!(json::drive(&json!({ "s": "left", "g": 0.0, "b": 0.0 })), None);
    assert_eq!(json::drive(&json!({ "s": 0.0, "g": 0.0 })), None);
}

#[test]
fn a_device_gets_only_the_parts_that_changed() {
    let a = json!({ "vehicle": { "motion": { "speedKmh": 10 }, "doors": { "anyOpen": false } }, "env": { "t": 1 }, "paused": false });
    let full: Value = serde_json::from_str(&json::state_delta(None, &a).unwrap()).unwrap();
    assert_eq!(full["full"], true);
    assert_eq!(full["vehicle"]["doors"]["anyOpen"], false);
    assert!(json::state_delta(Some(&a), &a).is_none());
    let mut b = a.clone();
    b["vehicle"]["motion"]["speedKmh"] = json!(12);
    let d: Value = serde_json::from_str(&json::state_delta(Some(&a), &b).unwrap()).unwrap();
    assert_eq!(d["vehicle"]["motion"]["speedKmh"], 12);
    assert!(d["vehicle"].get("doors").is_none());
    assert!(d.get("env").is_none() && d.get("full").is_none());
    let mut c = b.clone();
    c["vehicle"] = Value::Null;
    let d: Value = serde_json::from_str(&json::state_delta(Some(&b), &c).unwrap()).unwrap();
    assert_eq!((d["full"].clone(), d["vehicle"].clone()), (json!(true), Value::Null));
}

#[test]
fn vehicle_keys_become_safe_file_names() {
    assert_eq!(file_name("MAN_SD202/SD202 Hamburg.bus"), "MAN_SD202_SD202_Hamburg.bus.json");
    assert_eq!(file_name("../../etc/passwd"), "_.._etc_passwd.json");
    assert_eq!(file_name("_all"), "vehicle.json");
    assert_eq!(file_name(""), "vehicle.json");
    assert!(!file_name("a/../../b").contains('/'));
}

#[test]
fn the_vehicles_own_layout_beats_the_mods_and_the_common_one() {
    let dir = temp_dir("layouts");
    let folder = dir.join("Vehicles").join("Bus");
    std::fs::create_dir_all(&folder).unwrap();
    let store = Store::new(dir.join("layouts"));
    assert!(store.resolve("Bus/a.bus", &folder).is_none());
    assert!(store.save("Bus/a.bus", Scope::All, &layout("all")));
    assert_eq!(store.resolve("Bus/a.bus", &folder).unwrap().0, Scope::All);
    std::fs::write(folder.join("remote.json"), layout("mod").to_string()).unwrap();
    assert_eq!(store.resolve("Bus/a.bus", &folder).unwrap().0, Scope::Mod);
    assert!(store.save("Bus/a.bus", Scope::Vehicle, &layout("mine")));
    let (scope, l) = store.resolve("Bus/a.bus", &folder).unwrap();
    assert_eq!((scope, l["theme"].as_str()), (Scope::Vehicle, Some("mine")));
    assert_eq!(store.resolve("Bus/b.bus", &folder).unwrap().0, Scope::Mod);
    store.reset("Bus/a.bus", Scope::Vehicle);
    assert_eq!(store.resolve("Bus/a.bus", &folder).unwrap().0, Scope::Mod);
    assert!(!store.save("Bus/a.bus", Scope::Vehicle, &json!({ "v": 2, "pages": [] })));
    assert!(store.save("Bus/a.bus", Scope::Vehicle, &json!({ "v": 1, "pages": [{ "widgets": [] }], "theme": "x".repeat(300_000) })));
    assert!(store.resolve("Bus/a.bus", &folder).unwrap().1.get("theme").is_none());
}

#[test]
fn a_layout_is_cleaned_before_it_is_kept_or_sent() {
    let bad = json!({
        "v": 1,
        "accent": "url(http://evil/beacon)",
        "pages": [
            {},
            { "name": "A", "grid": { "l": ["x", 1], "p": [99, -3] }, "widgets": [
                { "kind": "button", "pos": { "l": [0, 0, 1, 1e8], "p": ["a"] }, "color": "red; background:url(x)", "lampColor": "#3ddc5c", "dp": 900, "label": "ok", "real": false },
                { "label": "no kind" }
            ] }
        ]
    });
    let c = crate::layouts::clean(&bad).unwrap();
    assert!(c.get("accent").is_none());
    assert_eq!(c["pages"].as_array().unwrap().len(), 1);
    let page = &c["pages"][0];
    assert!(page["grid"].get("l").is_none());
    assert_eq!(page["grid"]["p"], json!([16, 1]));
    let w = &page["widgets"].as_array().unwrap();
    assert_eq!(w.len(), 1);
    assert_eq!(w[0]["pos"]["l"], json!([0, 0, 1, 16]));
    assert!(w[0]["pos"].get("p").is_none());
    assert!(w[0].get("color").is_none() && w[0].get("dp").is_none());
    assert_eq!(w[0]["lampColor"], "#3ddc5c");
    assert_eq!(w[0]["real"], false);
}

#[test]
fn pairing_needs_the_code_and_closes_after_five_wrong_ones() {
    let dir = temp_dir("pairing");
    let path = dir.join("devices.txt");
    let mut d = Devices::load(path.clone());
    assert!(!d.knows("whatever"));
    let (a, b): (std::net::IpAddr, std::net::IpAddr) = ("192.168.1.20".parse().unwrap(), "192.168.1.30".parse().unwrap());
    let token = match d.pair("123456", " 123456 ", "iPad", a) {
        Pairing::Paired(t) => t,
        _ => panic!("the right code pairs"),
    };
    assert!(d.knows(&token));
    assert!(Devices::load(path.clone()).knows(&token));
    for _ in 0..5 {
        assert!(matches!(d.pair("123456", "000000", "", a), Pairing::Wrong { renew: false }));
    }
    assert!(matches!(d.pair("123456", "123456", "", a), Pairing::Locked));
    assert!(matches!(d.pair("123456", "123456", "", b), Pairing::Paired(_)));
    for k in 0..25 {
        let ip = std::net::IpAddr::from([10, 0, 0, k]);
        assert!(matches!(d.pair("123456", "000000", "", ip), Pairing::Wrong { renew } if renew == (k == 24)));
    }
    assert!(matches!(d.pair("123456", "123456", "", b), Pairing::Locked));
    d.forget_all();
    assert!(!Devices::load(path).knows(&token));
}

#[test]
fn codes_have_six_digits() {
    for _ in 0..50 {
        let p = pairing::new_pin();
        assert_eq!(p.len(), 6);
        assert!(p.bytes().all(|c| c.is_ascii_digit()));
    }
}

#[test]
fn the_page_its_files_and_the_icons_are_served() {
    assert_eq!(server::route("/").0, "200 OK");
    assert_eq!(server::route("/?x=1").0, "200 OK");
    assert_eq!(server::route("/app.js").1, "text/javascript; charset=utf-8");
    assert_eq!(server::route("/icons/door_front.svg").1, "image/svg+xml");
    assert_eq!(server::route("/icons/stop_request.svg").0, "200 OK");
    assert_eq!(server::route("/icons/../../Cargo.toml").0, "404 Not Found");
    assert_eq!(server::route("/nothing").0, "404 Not Found");
    let list: Vec<String> = serde_json::from_slice(&server::route("/icons.json").2).unwrap();
    assert!(list.iter().any(|n| n == "door_front"));
}

#[test]
fn every_text_of_the_page_is_listed_for_translation() {
    let page = [include_str!("../page/index.html"), include_str!("../page/app.js"), include_str!("../page/widgets.js"), include_str!("../page/editor.js"), include_str!("../page/drive.js")].concat();
    for t in TEXTS {
        assert!(page.contains(&format!("\"{t}\"")), "{t} is no longer on the page");
    }
}

struct Client(tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>);

impl Client {
    fn open(port: u16) -> Client {
        let (ws, _) = tungstenite::connect(format!("ws://127.0.0.1:{port}/ws")).unwrap();
        if let tungstenite::stream::MaybeTlsStream::Plain(s) = ws.get_ref() {
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        }
        Client(ws)
    }

    fn send(&mut self, v: Value) {
        self.0.send(tungstenite::Message::Text(v.to_string().into())).unwrap();
    }

    fn next(&mut self, t: &str) -> Value {
        loop {
            if let tungstenite::Message::Text(m) = self.0.read().unwrap() {
                let v: Value = serde_json::from_str(&m).unwrap();
                if v["t"] == t {
                    return v;
                }
            }
        }
    }
}

fn wait_for(remote: &Remote, n: usize) -> Vec<Command> {
    let since = Instant::now();
    let mut got = Vec::new();
    while got.len() < n && since.elapsed() < Duration::from_secs(5) {
        got.extend(remote.take_commands());
        std::thread::sleep(Duration::from_millis(10));
    }
    got
}

#[test]
fn a_device_pairs_drives_and_lets_go_of_what_it_held() {
    let dir = temp_dir("session");
    let mut remote = Remote::start(0, dir.clone()).unwrap();
    remote.set_texts("fr", &[("Doors", "Portes".into()), ("Horn", "Horn".into())]);
    remote.set_vehicle(Some(Vehicle { key: "Bus/a.bus".into(), name: "Bus A".into(), folder: dir.join("none"), actions: vec!["horn".into()], aliases: Vec::new(), depot: None, doors: Vec::new() }));
    remote.set_state(None, &omsi_sim::vehicle_api::ApiValue::Null, false);

    let mut c = Client::open(remote.port());
    c.send(json!({ "t": "pair", "pin": remote.pin(), "name": "test" }));
    assert_eq!(c.next("denied")["reason"], "closed");
    remote.set_pairing(true);
    c.send(json!({ "t": "pair", "pin": "999999x" }));
    assert_eq!(c.next("denied")["reason"], "pin");
    let pin = remote.pin();
    c.send(json!({ "t": "pair", "pin": pin, "name": "test" }));
    let token = c.next("paired")["token"].as_str().unwrap().to_string();
    assert_ne!(remote.pin(), pin);
    assert!(remote.take_paired());
    assert!(!remote.take_paired());
    let texts = c.next("texts");
    assert_eq!(texts["lang"], "fr");
    assert_eq!(texts["texts"]["Doors"], "Portes");
    assert!(texts["texts"].get("Horn").is_none());
    let v = c.next("vehicle");
    assert_eq!(v["name"], "Bus A");
    assert!(v["layout"].is_null());
    c.next("state");

    c.send(json!({ "t": "act", "name": "horn", "down": true }));
    c.send(json!({ "t": "indicator", "value": 1 }));
    assert_eq!(wait_for(&remote, 2), vec![Command::Action { name: "horn".into(), down: true }, Command::Indicator(1)]);

    c.send(json!({ "t": "drive", "s": -2.0, "g": 0.5, "b": 0.0 }));
    c.send(json!({ "t": "ping", "id": 7 }));
    assert_eq!(c.next("pong")["id"], 7);
    let d = remote.drive().unwrap();
    assert_eq!((d.steering, d.throttle, d.brake), (-1.0, 0.5, 0.0));
    std::thread::sleep(Duration::from_millis(300));
    let d = remote.drive().unwrap();
    assert_eq!((d.steering, d.throttle, d.brake), (-1.0, 0.0, 0.0));
    std::thread::sleep(Duration::from_millis(1300));
    assert!(remote.drive().is_none());
    c.send(json!({ "t": "layout.save", "scope": "vehicle", "layout": layout("cockpit") }));
    assert_eq!(c.next("layout.done")["ok"], true);
    let v = c.next("vehicle");
    assert_eq!(v["layout"]["scope"], "vehicle");
    assert_eq!(v["layout"]["data"]["theme"], "cockpit");

    drop(c);
    assert_eq!(wait_for(&remote, 1), vec![Command::Action { name: "horn".into(), down: false }]);

    let mut again = Client::open(remote.port());
    again.send(json!({ "t": "hello", "token": token }));
    assert_eq!(again.next("vehicle")["layout"]["data"]["theme"], "cockpit");

    let mut held = Client::open(remote.port());
    held.send(json!({ "t": "hello", "token": token }));
    held.next("vehicle");
    held.send(json!({ "t": "act", "name": "kw_m_enginestart", "down": true }));
    assert_eq!(wait_for(&remote, 1), vec![Command::Action { name: "kw_m_enginestart".into(), down: true }]);
    assert_eq!(remote.release_all(), vec!["kw_m_enginestart".to_string()]);
    remote.forget_devices();
    assert_eq!(held.next("denied")["reason"], "token");

    let mut stranger = Client::open(remote.port());
    stranger.send(json!({ "t": "hello", "token": "0".repeat(64) }));
    assert_eq!(stranger.next("denied")["reason"], "token");
    stranger.send(json!({ "t": "act", "name": "horn", "down": true }));
    stranger.send(json!({ "t": "drive", "s": 1.0, "g": 1.0, "b": 0.0 }));
    std::thread::sleep(Duration::from_millis(200));
    assert!(remote.take_commands().is_empty());
    assert!(remote.drive().is_none());
}

#[test]
fn the_pairing_address_fits_a_qr_code() {
    let url = pairing_url("192.168.1.20", 8642, "012345");
    assert_eq!(url, "http://192.168.1.20:8642/#pin=012345");
    let (n, cells) = qr(&url).unwrap();
    assert_eq!(cells.len(), n * n);
    assert!((21..=41).contains(&n));
}

#[test]
fn map_messages_carry_rounded_points() {
    let roads = crate::nav::roads_message([100.0, 200.0], &[(vec![[1.234, -5.678], [2.0, 3.0]], 7.25)]);
    let v: Value = serde_json::from_str(&roads).unwrap();
    assert_eq!(v["roads"][0]["p"], json!([1.2, -5.7, 2.0, 3.0]));
    let info = crate::nav::NavInfo { anchor: [0.0, 0.0], route: vec![[0.0, 0.0], [10.0, 0.0]], turn: Some(crate::nav::Turn { direction: 1, angle: 88.0, metres: 120.4, street: Some("Hauptstr.".into()) }), stop: None, stops: vec![[5.0, 1.0]], street: None, limit: Some(50.0), on_route: true };
    let v: Value = serde_json::from_str(&crate::nav::nav_message(&info)).unwrap();
    assert_eq!(v["turn"]["m"], 120.4);
    assert_eq!(v["turn"]["street"], "Hauptstr.");
    assert_eq!(v["stops"], json!([5.0, 1.0]));
}

#[test]
fn the_socket_is_only_opened_by_a_page_reached_by_its_address() {
    for host in ["192.168.1.20:8642", "127.0.0.1", "[fe80::1]:8642", "[::1]", "localhost:8642", "my-pc.local:8642", "DESKTOP-ABC:8642", "pc.home.arpa"] {
        assert!(crate::session::local_host(host), "{host}");
    }
    for host in ["evil.example:8642", "evil.example", "192.168.1.20.evil.example:8642", ""] {
        assert!(!crate::session::local_host(host), "{host}");
    }
}
