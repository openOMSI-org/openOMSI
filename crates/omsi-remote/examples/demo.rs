use omsi_remote::{Command, Remote, Vehicle};
use omsi_sim::vehicle_api::ApiValue;
use std::time::{Duration, Instant};

fn n(x: f64) -> ApiValue {
    ApiValue::Num(x)
}

fn s(x: &str) -> ApiValue {
    ApiValue::Str(x.into())
}

fn m(items: Vec<(&str, ApiValue)>) -> ApiValue {
    ApiValue::Map(items.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

struct Bus {
    doors: [bool; 3],
    parking: bool,
    hazard: bool,
    indicator: u8,
    lights: bool,
    engine: bool,
    destination: String,
    line: String,
}

fn state(b: &Bus, t: f64) -> ApiValue {
    let moving = true;
    let speed = if moving { 25.0 + 20.0 * (t * 0.2).sin() } else { 0.0 };
    let stops = ["Hauptbahnhof", "Rathaus", "Marktplatz", "Schillerstraße", "Friedhof", "Betriebshof"];
    m(vec![
        ("motion", m(vec![("speedKmh", n((speed * 10.0f64).round() / 10.0)), ("odometerKm", n(12345.6 + t / 100.0)), ("x", n(pose(t).0)), ("y", n(pose(t).1)), ("heading", n(pose(t).2))])),
        ("engine", m(vec![("running", ApiValue::Bool(b.engine)), ("rpm", n(if b.engine { 700.0 + speed * 40.0 } else { 0.0 })), ("gear", n(if moving { 2.0 } else { 0.0 })), ("tankContent", n(182.0))])),
        ("electrics", m(vec![("on", ApiValue::Bool(true))])),
        ("doors", m(vec![("count", n(3.0)), ("anyOpen", ApiValue::Bool(b.doors.iter().any(|d| *d))), ("list", ApiValue::List(b.doors.iter().enumerate().map(|(i, d)| m(vec![("number", n(i as f64 + 1.0)), ("open", ApiValue::Bool(*d))])).collect()))])),
        ("passengers", m(vec![("onboard", n(23.0)), ("exits", ApiValue::List(vec![m(vec![("requested", ApiValue::Bool(false))]), m(vec![("requested", ApiValue::Bool(t as i64 % 20 > 10))])]))])),
        ("lights", m(vec![("headlights", n(if b.lights { 1.0 } else { 0.0 })), ("hazard", ApiValue::Bool(b.hazard)), ("indicatorLeft", ApiValue::Bool((b.indicator == 1 || b.hazard) && (t * 2.0) as i64 % 2 == 0)), ("indicatorRight", ApiValue::Bool((b.indicator == 2 || b.hazard) && (t * 2.0) as i64 % 2 == 0)), ("interior", ApiValue::Bool(true))])),
        ("brakes", m(vec![("parking", ApiValue::Bool(b.parking)), ("stop", ApiValue::Bool(b.doors.iter().any(|d| *d))), ("kneeling", ApiValue::Bool(false))])),
        ("cabin", m(vec![("temperature", n(21.5))])),
        ("route", m(vec![
            ("line", s(&b.line)),
            ("destination", s(&b.destination)),
            ("delaySec", n(75.0)),
            ("current", m(vec![("name", s("Marktplatz"))])),
            ("stops", ApiValue::List(stops.iter().enumerate().map(|(i, x)| m(vec![("index", n(i as f64)), ("name", s(x)), ("departure", s(&format!("14:{:02}", 10 + i * 3))), ("served", ApiValue::Bool(i < 2)), ("current", ApiValue::Bool(i == 2))])).collect())),
        ])),
    ])
}

fn lap() -> &'static (Vec<[f64; 2]>, Vec<f64>) {
    static LAP: std::sync::OnceLock<(Vec<[f64; 2]>, Vec<f64>)> = std::sync::OnceLock::new();
    LAP.get_or_init(|| {
        let pts: Vec<[f64; 2]> = (0..=480)
            .map(|i| {
                let a = i as f64 / 480.0 * std::f64::consts::TAU;
                [230.0 * a.cos() + 50.0 * (2.0 * a).cos(), 170.0 * a.sin() + 35.0 * (3.0 * a).sin()]
            })
            .collect();
        let mut len = vec![0.0];
        for w in pts.windows(2) {
            len.push(len.last().unwrap() + ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt());
        }
        (pts, len)
    })
}

fn at(d: f64) -> (usize, [f64; 2]) {
    let (pts, len) = lap();
    let d = d.rem_euclid(*len.last().unwrap());
    let i = len.partition_point(|l| *l <= d).clamp(1, pts.len() - 1);
    let k = (d - len[i - 1]) / (len[i] - len[i - 1]).max(1e-6);
    (i, [pts[i - 1][0] + (pts[i][0] - pts[i - 1][0]) * k, pts[i - 1][1] + (pts[i][1] - pts[i - 1][1]) * k])
}

fn pose(t: f64) -> (f64, f64, f64) {
    let d = t * 9.0;
    let (_, p) = at(d);
    let (_, q) = at(d + 2.0);
    (p[0], p[1], (q[0] - p[0]).atan2(q[1] - p[1]).to_degrees().rem_euclid(360.0))
}

fn corners(t: f64) -> Vec<[f32; 2]> {
    (0..=60).map(|k| at(t * 9.0 + k as f64 * 10.0).1).map(|p| [p[0] as f32, p[1] as f32]).collect()
}

fn streets() -> Vec<(Vec<[f32; 2]>, f32)> {
    let mut roads = vec![(lap().0.iter().map(|p| [p[0] as f32, p[1] as f32]).collect(), 9.0)];
    roads.push(((0..=40).map(|i| { let x = -450.0 + i as f32 * 25.0; [x, 40.0 * (x / 120.0).sin() + x * 0.15] }).collect(), 13.0));
    roads.push(((0..=40).map(|i| { let y = -400.0 + i as f32 * 22.0; [60.0 * (y / 150.0).cos() - 40.0, y] }).collect(), 11.0));
    for k in 0..9 {
        let a = k as f64 / 9.0 * std::f64::consts::TAU + 0.3;
        let (_, o) = at(a / std::f64::consts::TAU * lap().1.last().unwrap());
        let (dx, dy) = (a.cos() as f32, a.sin() as f32);
        let bend = if k % 2 == 0 { 1.0 } else { -1.0 };
        roads.push(((0..=12).map(|j| { let s = j as f32 * 22.0; let w = bend * (s / 70.0).sin() * 18.0; [o[0] as f32 + dx * s - dy * w, o[1] as f32 + dy * s + dx * w] }).collect(), 6.5));
    }
    roads
}

fn env(t: f64) -> ApiValue {
    let secs = 14 * 3600 + 15 * 60 + t as i64;
    m(vec![
        ("time", m(vec![("asString", s(&format!("{:02}:{:02}:{:02}", secs / 3600 % 24, secs / 60 % 60, secs % 60)))])),
        ("date", m(vec![("asString", s("11.10.2026"))])),
        ("weather", m(vec![("temperature", n(14.0))])),
    ])
}

fn depot() -> ApiValue {
    let dests = ["Betriebshof", "Hauptbahnhof", "Friedhof", "Nicht einsteigen"];
    m(vec![
        ("routes", ApiValue::List((0..4).map(|i| m(vec![("index", n(i as f64)), ("line", s(&format!("{}", 10 + i))), ("name", s(&format!("Route {i}"))), ("destination", s(dests[i % 4]))])).collect())),
        ("destinations", ApiValue::List(dests.iter().enumerate().map(|(i, d)| m(vec![("index", n(i as f64)), ("code", n(100.0 + i as f64)), ("name", s(d))])).collect())),
    ])
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let port = args.get(1).and_then(|p| p.parse().ok()).unwrap_or(8642);
    let dir = std::env::temp_dir().join("omsi-remote-demo");
    let mut r = Remote::start(port, dir).expect("port");
    println!("http://127.0.0.1:{}  code {}", r.port(), r.pin());
    r.set_texts("en", &[]);
    r.set_pairing(true);
    r.set_vehicle(Some(Vehicle { key: "MAN_SD202/SD202.bus".into(), name: "MAN SD202".into(), folder: std::env::temp_dir(), actions: vec!["bus_doorfront0".into(), "bus_dooraft".into(), "horn".into()], aliases: vec![("door_1".into(), "door_haltewunsch".into()), ("door_2".into(), "bus_doorfront1".into()), ("door_3".into(), "bus_doorfront2".into()), ("parking_brake_toggle".into(), "notloese_p".into()), ("kw_m_enginestart".into(), "kw_m_enginestart".into())], depot: Some(depot()), doors: vec![vec![0, 1], vec![2, 3]] }));
    if let (Some(root), Some(bus)) = (args.get(2), args.get(3)) {
        let root = std::path::PathBuf::from(root);
        let ty = omsi_sim::VehicleType::load(&root, std::path::Path::new(bus)).expect("vehicle");
        r.set_skins(omsi_remote::extract_skins(&ty, &root, None));
    }
    let mut b = Bus { doors: [false; 3], parking: true, hazard: false, indicator: 0, lights: false, engine: true, destination: "Hauptbahnhof".into(), line: "32".into() };
    let start = Instant::now();
    let mut frame = 0u64;
    loop {
        for c in r.take_commands() {
            println!("{c:?}");
            match c {
                Command::Action { name, down: true } => match name.as_str() {
                    "door_1" => b.doors[0] = !b.doors[0],
                    "door_2" => b.doors[1] = !b.doors[1],
                    "door_3" => b.doors[2] = !b.doors[2],
                    "doors_all" => b.doors = [!b.doors.iter().any(|d| *d); 3],
                    "parking_brake_toggle" => b.parking = !b.parking,
                    "kw_scheinwerfer_toggle" => b.lights = !b.lights,
                    _ => {}
                },
                Command::Indicator(3) => b.hazard = !b.hazard,
                Command::Indicator(v) => b.indicator = if b.indicator == v { 0 } else { v },
                Command::StartUp => b.engine = !b.engine,
                Command::Destination(i) => b.destination = ["Betriebshof", "Hauptbahnhof", "Friedhof", "Nicht einsteigen"][i % 4].into(),
                Command::Route(i) => b.line = format!("{}", 10 + i),
                Command::Line(l) => b.line = l,
                Command::ClearLine => b.line.clear(),
                _ => {}
            }
        }
        let t = start.elapsed().as_secs_f64();
        r.set_state(Some(&state(&b, t)), &env(t), false);
        frame += 1;
        if frame % 30 == 0 {
            if let Some(d) = r.drive() {
                println!("drive {:.2} {:.2} {:.2}", d.steering, d.throttle, d.brake);
            }
        }
        if frame % 24 == 1 {
            let roads = streets();
            r.publish("roads", omsi_remote::nav::roads_message([0.0, 0.0], &roads));
            let left = 380.0 - ((t * 9.0) % 380.0) as f32;
            let info = omsi_remote::nav::NavInfo {
                anchor: [0.0, 0.0],
                route: corners(t),
                turn: Some(omsi_remote::nav::Turn { direction: 1, angle: 90.0, metres: left, street: Some("Schillerstraße".into()) }),
                stop: Some(("Marktplatz".into(), left + 250.0)),
                stops: [left + 250.0, left + 900.0].iter().map(|d| at(t * 9.0 + *d as f64).1).map(|p| [p[0] as f32, p[1] as f32]).collect(),
                street: Some("Hauptstraße".into()),
                limit: Some(50.0),
                on_route: true,
            };
            r.publish("nav", omsi_remote::nav::nav_message(&info));
        }
        std::thread::sleep(Duration::from_millis(16));
    }
}
