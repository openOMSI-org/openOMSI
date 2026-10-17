//! A dedicated server's administration without the game, to try the panels and their API: a
//! session on this machine with a few players of its own (games that join it over UDP and
//! stand still), the web gateway with the administration, and the panel at
//! `http://127.0.0.1:<web port>/admin/`. There is no world: its operations are answered as
//! applied without changing anything but the passenger density shown.
//!
//! ```text
//! cargo run -p omsi-lan --example admin_demo -- [udp port] [web port]
//! ```
//! Passwords: `admin secret` (admin), `mod secret` (moderator). Ctrl+C ends it.

use omsi_admin::{Audit, Auth, AuthCfg, Hub, Outcome, Passwords, ServerState};
use omsi_lan::admin::{server_command, Action, ServerAdmin, ServerRules};
use omsi_net::{LanSession, Pose, WorldInfo};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn main() {
    let _ = env_logger_like();
    let mut args = std::env::args().skip(1);
    let udp: u16 = args.next().and_then(|a| a.parse().ok()).unwrap_or(27315);
    let web: u16 = args.next().and_then(|a| a.parse().ok()).unwrap_or(27325);
    std::env::set_var("OMSI_NO_BRIDGE", "1");
    let world = WorldInfo { map: "maps/Grundorf/global.cfg".into(), date: "1989-05-30".into(), time: 9.0 * 3600.0, ..Default::default() };
    let mut lan = LanSession::host(udp, "admin demo", world.clone(), false).expect("the UDP port is free");
    let hub = Arc::new(Hub::new(Auth::new(Passwords { admin: "admin secret".into(), moderator: "mod secret".into() }, AuthCfg::default()), Audit::new(None, 0)));
    let info = omsi_net::ws::ServerInfo { name: "admin demo".into(), map: world.map.clone(), local_admin_password: "admin secret".into(), ..Default::default() };
    let _gw = omsi_net::ws::WsGateway::start(SocketAddr::from(([127, 0, 0, 1], web)), SocketAddr::from(([127, 0, 0, 1], udp)), info).expect("the web port is free");
    omsi_lan::gateway::open_admin_api(hub.clone(), omsi_admin::WebCfg::default());
    let mut adm = ServerAdmin::default();
    adm.password = "admin secret".into();
    adm.moderator_password = "mod secret".into();
    adm.passengers_on = true;
    // the players of the demo (names alike on purpose)
    let mut bots: Vec<LanSession> = ["Pichau", "pichau_fan", "Joao", "<b>Bold</b>"].iter().map(|n| LanSession::join(&format!("127.0.0.1:{udp}"), n, world.clone(), Duration::from_millis(10)).expect("a bot")).collect();
    println!("panel: http://127.0.0.1:{web}/admin/  (admin secret / mod secret)");
    let rules = ServerRules { real_time: false, traffic_max: 120, notice_ok: &|_| true, metar: false };
    let (mut clock, mut speed, mut weather, mut traffic) = (9.0 * 3600.0, 1.0, String::from("Weather/#CAVOK.owt"), 30usize);
    let mut last = Instant::now();
    let mut published = Instant::now() - Duration::from_secs(5);
    loop {
        let dt = last.elapsed().as_secs_f64();
        last = Instant::now();
        clock = (clock + dt * speed) % 86_400.0;
        for (k, b) in bots.iter_mut().enumerate() {
            // (driving, but the third on foot)
            let mut pose = Pose { bus: "Vehicles/MAN_SD200/MAN_SD77.bus".into(), line: "37".into(), x: 100.0 * k as f64, flags: omsi_net::FLAG_VEHICLE, ..Default::default() };
            if k == 2 {
                pose = Pose { walker: Some(omsi_net::Walker { x: 10.0, y: 5.0, ..Default::default() }), ..Default::default() };
            }
            b.tick(0.05, &pose);
            let _ = b.take_commands();
        }
        lan.tick(0.05, &Pose::default());
        adm.prune(&lan);
        for (from, text) in lan.take_commands() {
            server_command(&mut lan, from, &text, &mut adm, &|_| None, &rules, Some(&hub));
        }
        for job in hub.take(16) {
            omsi_lan::admin::run_job(&mut lan, &mut adm, job, &|_| None, &rules, &hub);
        }
        for job in adm.take_world() {
            let out = match &job.action {
                Action::PassengerDensity(d) => {
                    adm.passenger_density = Some(*d);
                    Outcome::applied(format!("passenger density {d} (new people from now on)"))
                }
                Action::Clock(s) => {
                    clock = *s;
                    Outcome::applied("clock set")
                }
                Action::Time(s) => {
                    clock = (clock + s).rem_euclid(86_400.0);
                    Outcome::applied("clock moved")
                }
                Action::Speed(s) => {
                    speed = *s;
                    Outcome::applied(format!("time speed x{s}"))
                }
                Action::Weather(omsi_lan::admin::WeatherAction::Set(f)) => {
                    weather = f.clone();
                    Outcome::applied(format!("weather {f}"))
                }
                Action::Traffic(a) => match a.parse::<usize>() {
                    Ok(n) => {
                        traffic = n;
                        Outcome::applied(format!("traffic {n}"))
                    }
                    Err(_) => Outcome::applied("the demo has no cars to clear"),
                },
                _ => Outcome::applied("the demo has no world: nothing changed"),
            };
            omsi_lan::admin::report(&mut lan, Some(&hub), &job.actor, job.record, &out);
        }
        if published.elapsed() > Duration::from_secs(1) {
            published = Instant::now();
            let state = ServerState {
                name: "admin demo".into(),
                map: "maps/Grundorf/global.cfg".into(),
                version: "demo".into(),
                max_players: 16,
                time: format!("{:02}:{:02}", (clock / 3600.0) as u32, (clock / 60.0) as u32 % 60),
                clock,
                clock_speed: speed,
                weather: weather.clone(),
                traffic,
                traffic_max: 120,
                timetable: true,
                passengers: true,
                passenger_density: adm.passenger_density.unwrap_or(1.0),
                passenger_density_max: omsi_admin::PASSENGER_DENSITY_MAX,
                waiting: 12,
                walking: 30,
                aboard: 4,
                ..Default::default()
            };
            omsi_lan::admin::publish(&hub, &lan, &adm, state);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The log on the terminal (`RUST_LOG`-like: info and above), without another dependency.
fn env_logger_like() -> Result<(), log::SetLoggerError> {
    struct L;
    impl log::Log for L {
        fn enabled(&self, m: &log::Metadata) -> bool {
            m.level() <= log::Level::Info
        }
        fn log(&self, r: &log::Record) {
            if self.enabled(r.metadata()) {
                eprintln!("{} {}", r.level(), r.args());
            }
        }
        fn flush(&self) {}
    }
    log::set_logger(&L).map(|()| log::set_max_level(log::LevelFilter::Info))
}
