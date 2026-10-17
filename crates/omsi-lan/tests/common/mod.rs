//! What the administration's tests share: a server as the dedicated one runs it
//! (`server_command` on every command that comes, the web's requests from the hub, the
//! world's part, the players published once in a while), players who join it, and the
//! game's way of proving a password.
#![allow(dead_code)]

use omsi_admin::{Audit, Auth, AuthCfg, Hub, Passwords, Status};
use omsi_lan::admin::{answer, server_command, ServerAdmin, ServerRules};
use omsi_net::{LanSession, Pose, WorldInfo};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub fn world() -> WorldInfo {
    WorldInfo { map: "maps/Grundorf/global.cfg".into(), date: "1989-05-30".into(), time: 9.0 * 3600.0, ..Default::default() }
}

pub struct Server {
    pub lan: LanSession,
    pub adm: ServerAdmin,
    pub hub: Arc<Hub>,
}

impl Server {
    pub fn new(port: u16) -> Server {
        std::env::set_var("OMSI_NO_BRIDGE", "1");
        let lan = LanSession::host(port, "server", world(), false).unwrap();
        let mut adm = ServerAdmin::default();
        adm.password = "admin secret".into();
        adm.moderator_password = "mod secret".into();
        adm.passengers_on = true;
        let hub = Arc::new(Hub::new(Auth::new(Passwords { admin: "admin secret".into(), moderator: "mod secret".into() }, AuthCfg::default()), Audit::new(None, 0)));
        Server { lan, adm, hub }
    }

    /// One step of the server's loop: the session, the commands that came, the web's
    /// requests, the world's part, the state for the panels.
    pub fn step(&mut self) {
        self.lan.tick(0.05, &Pose::default());
        let rules = ServerRules { real_time: false, traffic_max: 120, notice_ok: &|_| true, metar: false };
        self.adm.prune(&self.lan);
        for (from, text) in self.lan.take_commands() {
            server_command(&mut self.lan, from, &text, &mut self.adm, &|_| None, &rules, Some(&self.hub));
        }
        for job in self.hub.take(16) {
            omsi_lan::admin::run_job(&mut self.lan, &mut self.adm, job, &|_| None, &rules, &self.hub);
        }
        for job in self.adm.take_world() {
            if let omsi_lan::admin::Action::PassengerDensity(d) = job.action {
                self.adm.passenger_density = Some(d);
            }
            omsi_lan::admin::report(&mut self.lan, Some(&self.hub), &job.actor, job.record, &omsi_admin::Outcome::applied("done"));
        }
        let state = omsi_admin::ServerState { passengers: true, passenger_density: self.adm.passenger_density.unwrap_or(1.0), ..Default::default() };
        omsi_lan::admin::publish(&self.hub, &self.lan, &self.adm, state);
    }
}

/// A player's game and the commands the server sent it.
pub struct Player {
    pub lan: LanSession,
    pub heard: Vec<String>,
}

impl Player {
    pub fn join(server: &mut Server, port: u16, name: &str) -> Player {
        let mut lan = LanSession::join(&format!("127.0.0.1:{port}"), name, world(), Duration::from_millis(10)).unwrap();
        let t0 = Instant::now();
        while !lan.connected && t0.elapsed() < Duration::from_secs(5) {
            lan.tick(1.0, &Pose::default());
            server.step();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(lan.connected, "{name} joined");
        Player { lan, heard: Vec::new() }
    }

    pub fn step(&mut self) {
        self.lan.tick(0.05, &Pose::default());
        self.heard.extend(self.lan.take_commands().into_iter().filter(|(from, _)| *from == 1).map(|(_, t)| t));
    }
}

/// Steps everybody until `done` says so (or 5 s).
pub fn until(server: &mut Server, players: &mut [&mut Player], mut done: impl FnMut(&Server, &[&mut Player]) -> bool) -> bool {
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(5) {
        for p in players.iter_mut() {
            p.step();
        }
        server.step();
        if done(server, players) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

/// The game's `/admin <password>`: `auth?`, the challenge answered, the server's word.
pub fn login(server: &mut Server, p: &mut Player, others: &mut [&mut Player], password: &str) -> String {
    p.heard.clear();
    p.lan.command(1, "auth?");
    let mut all: Vec<&mut Player> = vec![p];
    all.extend(others.iter_mut().map(|x| &mut **x));
    assert!(until(server, &mut all, |_, ps| ps[0].heard.iter().any(|h| h.starts_with("admin-challenge") || h == "admin-no")));
    let challenge = all[0].heard.iter().find_map(|h| h.strip_prefix("admin-challenge ")).map(str::to_string);
    let Some(challenge) = challenge else { return "admin-no".into() };
    let a = answer(&challenge, password).unwrap();
    all[0].heard.clear();
    all[0].lan.command(1, &format!("auth {a}"));
    assert!(until(server, &mut all, |_, ps| ps[0].heard.iter().any(|h| h.starts_with("admin-ok") || h == "admin-no" || h == "admin-locked")));
    all[0].heard.iter().find(|h| h.starts_with("admin-")).cloned().unwrap()
}

pub fn listed(p: &Player) -> Vec<omsi_admin::Player> {
    p.heard.iter().filter_map(|h| h.strip_prefix("admin-p ")).filter_map(omsi_lan::admin::parse_player).collect()
}

pub fn results(p: &Player) -> Vec<(Status, String)> {
    p.heard
        .iter()
        .filter_map(|h| h.strip_prefix("admin-result "))
        .filter_map(|r| {
            let mut w = r.splitn(3, ' ');
            let _id = w.next()?;
            Some((Status::parse(w.next()?)?, w.next().unwrap_or("").to_string()))
        })
        .collect()
}

