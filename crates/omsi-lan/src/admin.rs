//! A dedicated server's administration (see the app's `admin` for the whole picture): the
//! admins and their challenges, the commands players send the server (`server_command`),
//! and the pieces the host's own game menu shares with it - the password's answer, a duty's
//! argument, whom a notice is for.

use omsi_admin::{Hub, Role};
use omsi_net::LanSession;
use std::collections::HashMap;

mod action;
mod exec;
pub mod panel;
mod reply;
pub mod world;
pub use action::{act_on_session, Action, Admin, Done, WeatherAction, Whom};
pub use exec::{action_of, exec, hub, install_hub, perm_of, players, publish, report, run_job, split_guard, Exec, WorldJob};
pub use reply::{esc, parse_player, player_line, unesc};

/// What of the game a dedicated server's administration goes by.
pub struct ServerRules<'a> {
    /// The server's clock follows the real time: no admin moves it or changes its speed.
    pub real_time: bool,
    /// The most AI traffic the game allows (`traffic <density>` is held to it).
    pub traffic_max: usize,
    /// Whether a notice (`notify`) is one a game can show.
    pub notice_ok: &'a dyn Fn(&str) -> bool,
    /// The weather follows a METAR report: no admin changes it.
    pub metar: bool,
}

/// A finite number (NaN passes `clamp` through and would stop everybody's clock).
pub fn finite(text: &str) -> Option<f64> {
    text.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

pub const KDF_ROUNDS: u32 = 600_000;
const MIN_ROUNDS: u32 = 100_000;
const MAX_ROUNDS: u32 = 5_000_000;
pub const MIN_PASSWORD: usize = 12;
/// The passwords' least length when the web panel is open to the internet (`admin_web = remote`).
pub const REMOTE_MIN_PASSWORD: usize = 16;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 || !s.is_ascii() {
        return None;
    }
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()).collect()
}

fn derive(password: &str, salt: &[u8], rounds: u32) -> [u8; 32] {
    let mut key = [0u8; 32];
    let rounds = std::num::NonZeroU32::new(rounds.max(1)).expect("not zero");
    ring::pbkdf2::derive(ring::pbkdf2::PBKDF2_HMAC_SHA256, rounds, salt, password.as_bytes(), &mut key);
    key
}

fn keyed_response(key: &[u8; 32], challenge: &str) -> String {
    let mut ctx = ring::hmac::Context::with_key(&ring::hmac::Key::new(ring::hmac::HMAC_SHA256, key));
    ctx.update(b"omsi2rw-admin");
    ctx.update(challenge.as_bytes());
    hex(ctx.sign().as_ref())
}

pub fn answer(challenge: &str, password: &str) -> Option<String> {
    let mut it = challenge.split_whitespace();
    let (c, salt, rounds) = (it.next()?, it.next()?, it.next()?.parse::<u32>().ok()?);
    if it.next().is_some() || c.len() > 64 || !(MIN_ROUNDS..=MAX_ROUNDS).contains(&rounds) {
        return None;
    }
    let salt = unhex(salt).filter(|s| (16..=64).contains(&s.len()))?;
    Some(keyed_response(&derive(password, &salt, rounds), c))
}

fn fresh_salt() -> [u8; 16] {
    use ring::rand::SecureRandom;
    let mut salt = [0u8; 16];
    if ring::rand::SystemRandom::new().fill(&mut salt).is_err() {
        salt[..8].copy_from_slice(&omsi_net::random_session_id().to_le_bytes());
        salt[8..].copy_from_slice(&omsi_net::random_session_id().to_le_bytes());
    }
    salt
}

/// Equal strings, compared in time independent of where they differ.
pub fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// A point `side` metres to the right of a vehicle at `pos` facing `heading`.
pub fn beside(pos: glam::DVec3, heading: f64, side: f64) -> (f64, f64) {
    let h = heading.to_radians();
    (pos.x + h.cos() * side, pos.y - h.sin() * side)
}

/// `duty`'s argument: `<line> <tour> <trip> <stop>`, a space in the line or tour name written
/// `%20` (and `%` as `%25`).
pub fn duty_arg(line: &str, tour: &str, trip: usize, stop: usize) -> String {
    let esc = |s: &str| s.trim().replace('%', "%25").replace(' ', "%20");
    format!("{} {} {trip} {stop}", esc(line), esc(tour))
}

/// `duty_arg` read back: (line, tour, trip, stop).
pub fn parse_duty(arg: &str) -> Option<(String, String, usize, usize)> {
    let unesc = |s: &str| s.replace("%20", " ").replace("%25", "%");
    let v: Vec<&str> = arg.split_whitespace().collect();
    let [line, tour, trip, stop] = v[..] else { return None };
    Some((unesc(line), unesc(tour), trip.parse().ok()?, stop.parse().ok()?))
}

/// Who a `notify` is for: `all`, every other player and the host's own screen; a player's
/// number, that player - or the host's own screen when it is the host's number (`peers` has
/// only the others, and `LanSession::command` sends nothing to oneself).
pub fn notice_targets(who: &str, others: impl Iterator<Item = u32>, my_id: u32) -> (Vec<u32>, bool) {
    if who == "all" {
        return (others.filter(|id| *id != my_id).collect(), true);
    }
    match who.trim().parse::<u32>() {
        Ok(id) if id == my_id => (Vec::new(), true),
        Ok(id) => (vec![id], false),
        Err(_) => (Vec::new(), false),
    }
}

/// The administration of a dedicated server (`omsi --server`): what it keeps.
#[derive(Default)]
pub struct ServerAdmin {
    pub password: String,
    /// `moderator_password` (empty: none): its players see and moderate, and leave the world
    /// alone (`omsi_admin::Role`).
    pub moderator_password: String,
    /// Who administers the server from their game, with their role (and the machine's own
    /// tool, `LOCAL_ADMIN`, while it says something).
    pub admins: HashMap<u32, Role>,
    /// The clock moved by an admin (s).
    pub shift: f64,
    /// The operations on the server's own world, for the host loop (`take_world`).
    world: Vec<WorldJob>,
    /// The server has people at all (`passengers` of `server.cfg`; switched at start only).
    pub passengers_on: bool,
    /// The passenger density the server applies (its `passenger_density`, or an admin's
    /// `passengers <factor>`); None: the machine's own passengers setting. The host loop
    /// reads it for every step's demand, so a change counts from the next step on and is
    /// never multiplied twice.
    pub passenger_density: Option<f32>,
    /// The challenge each asking player was given (used once).
    challenges: std::collections::HashMap<u32, String>,
    /// When wrong answers came lately: from everybody (the lock for all, when many sources
    /// guess at once) and from each source (its address: a player who reconnects is the
    /// same source).
    failures: Vec<std::time::Instant>,
    failures_from: HashMap<String, Vec<std::time::Instant>>,
    /// The passwords' keys for the challenges (both with one salt), and the passwords they
    /// were made of.
    kdf: Option<Kdf>,
    admin_at: HashMap<u32, Option<std::net::SocketAddr>>,
}

struct Kdf {
    of: (String, String),
    salt: [u8; 16],
    admin: [u8; 32],
    moderator: Option<[u8; 32]>,
}

/// A dedicated server admin's order for the AI traffic.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrafficOrder {
    Density(usize),
    Clear,
}

impl TrafficOrder {
    /// `clear`, or a density (held to 0 .. the top of the game menu's Traffic steps, the
    /// same limit as server.cfg's `traffic`: it was 100 here, 120 in the menu, #1327).
    pub fn parse(arg: &str, max: usize) -> Option<TrafficOrder> {
        match arg.trim() {
            "clear" => Some(TrafficOrder::Clear),
            v => v.parse::<usize>().ok().map(|n| TrafficOrder::Density(n.min(max))),
        }
    }
}

/// Wrong answers within `LOCK_WINDOW` that lock one source out; from all sources, that lock
/// the administration for everybody (a source counts `LOCK_AFTER` at most, so that takes
/// many guessing at once).
const LOCK_AFTER: usize = 5;
const LOCK_ALL: usize = 30;
const LOCK_WINDOW: std::time::Duration = std::time::Duration::from_secs(120);

impl ServerAdmin {
    /// Forget the rights and challenges of players no longer in the session.
    pub fn prune(&mut self, lan: &LanSession) {
        let here: HashMap<u32, Option<std::net::SocketAddr>> = lan.peers().map(|p| (p.pose.id, p.addr)).collect();
        let at = &self.admin_at;
        self.admins.retain(|id, _| {
            let moved = matches!((at.get(id), here.get(id)), (Some(a), Some(h)) if a != h);
            if moved {
                log::warn!("server: player {id} talks from another address now: no longer an admin until the password is given again");
            }
            here.contains_key(id) && !moved
        });
        let admins = &self.admins;
        self.admin_at.retain(|id, _| admins.contains_key(id));
        self.challenges.retain(|id, _| here.contains_key(id));
    }

    /// Whether a password lets anybody in.
    fn enabled(&self) -> bool {
        !self.password.is_empty() || !self.moderator_password.is_empty()
    }

    /// The keys of the passwords (made once: slow on purpose), with their salt.
    fn derived(&mut self) -> &Kdf {
        let of = (self.password.clone(), self.moderator_password.clone());
        if self.kdf.as_ref().map(|k| &k.of) != Some(&of) {
            let salt = fresh_salt();
            let admin = derive(&self.password, &salt, KDF_ROUNDS);
            // (a moderator's password equal to the admin's gives the admin's role)
            let moderator = (!self.moderator_password.is_empty() && self.moderator_password != self.password).then(|| derive(&self.moderator_password, &salt, KDF_ROUNDS));
            self.kdf = Some(Kdf { of, salt, admin, moderator });
        }
        self.kdf.as_ref().expect("just made")
    }

    /// The role an answer to `challenge` proves, if any.
    fn role_of(&mut self, challenge: &str, response: &str) -> Option<Role> {
        let has_admin = !self.password.is_empty();
        let k = self.derived();
        if has_admin && same(response, &keyed_response(&k.admin, challenge)) {
            return Some(Role::Admin);
        }
        k.moderator.as_ref().filter(|m| same(response, &keyed_response(m, challenge))).map(|_| Role::Moderator)
    }

    /// The operations on the server's world asked since the last time, in order.
    pub fn take_world(&mut self) -> Vec<WorldJob> {
        std::mem::take(&mut self.world)
    }

    fn locked(&mut self, source: &str) -> bool {
        self.failures.retain(|t| t.elapsed() < LOCK_WINDOW);
        self.failures_from.retain(|_, v| {
            v.retain(|t| t.elapsed() < LOCK_WINDOW);
            !v.is_empty()
        });
        self.failures.len() >= LOCK_ALL || self.failures_from.get(source).is_some_and(|v| v.len() >= LOCK_AFTER)
    }

    fn failed(&mut self, source: String) {
        let now = std::time::Instant::now();
        self.failures.push(now);
        // (so many sources at most: a flood of them forgets some)
        if self.failures_from.len() >= 1024 && !self.failures_from.contains_key(&source) {
            self.failures_from.clear();
        }
        self.failures_from.entry(source).or_default().push(now);
    }
}

/// Who a wrong answer is counted against: the player's address (the same when they come
/// back; the one behind the web gateway, not the gateway's loopback socket), or their
/// number when there is none (this machine, or no address).
fn source_of(lan: &LanSession, from: u32) -> String {
    counted_source(lan.peers().find(|p| p.pose.id == from).and_then(|p| p.addr), from)
}

fn counted_source(addr: Option<std::net::SocketAddr>, from: u32) -> String {
    match addr.and_then(omsi_net::source_ip) {
        Some(ip) => ip.to_string(),
        None => format!("player {from}"),
    }
}

/// A challenge for the password's proof: 128 bits from the system's secure random.
fn fresh_challenge() -> String {
    use ring::rand::SecureRandom;
    let mut c = [0u8; 16];
    if ring::rand::SystemRandom::new().fill(&mut c).is_err() {
        return format!("{:016x}{:016x}", omsi_net::random_session_id(), omsi_net::random_session_id());
    }
    hex(&c)
}

/// Who the commands of the web gateway's `POST /admin` come from: no player has this id.
pub const LOCAL_ADMIN: u32 = u32::MAX;

/// A command a player sent the dedicated server (`hub`: the administration's records, see
/// `exec`).
#[allow(clippy::too_many_arguments)]
pub fn server_command(lan: &mut LanSession, from: u32, text: &str, adm: &mut ServerAdmin, positions: &dyn Fn(u32) -> Option<(glam::DVec3, f64)>, rules: &ServerRules, hub: Option<&Hub>) {
    let (verb, arg) = text.split_once(' ').unwrap_or((text, ""));
    match verb {
        // a joining game asks which voice server the session talks on (`voice`)
        "voice?" => lan.command(from, &crate::voice::VoiceServer::command(crate::voice::hosted().as_ref())),
        "auth?" => {
            if !adm.enabled() {
                lan.command(from, "admin-no");
            } else if adm.locked(&source_of(lan, from)) {
                lan.command(from, "admin-locked");
            } else {
                let c = fresh_challenge();
                let salt = adm.derived().salt;
                adm.challenges.insert(from, c.clone());
                lan.command(from, &format!("admin-challenge {c} {} {KDF_ROUNDS}", hex(&salt)));
            }
        }
        "auth" => {
            let challenge = adm.challenges.remove(&from);
            if adm.locked(&source_of(lan, from)) {
                lan.command(from, "admin-locked");
            } else if let (true, Some(c)) = (adm.enabled(), challenge) {
                match adm.role_of(&c, arg.trim()) {
                    Some(role) => {
                        adm.admins.insert(from, role);
                        adm.admin_at.insert(from, lan.peers().find(|p| p.pose.id == from).and_then(|p| p.addr));
                        let who = reply::player_label(lan, from);
                        log::info!("server: {who} administers the server now ({})", role.name());
                        if let Some(h) = hub {
                            h.note(&who, Some(role), "login", "ok", "the game's Administration menu");
                        }
                        // (a game before the roles reads the first word only)
                        lan.command(from, &format!("admin-ok {}", role.name()));
                    }
                    None => {
                        adm.failed(source_of(lan, from));
                        let who = reply::player_label(lan, from);
                        log::warn!("server: {who} gave a wrong admin password");
                        if let Some(h) = hub {
                            h.note(&who, None, "login", "refused", "wrong password (the game's Administration menu)");
                        }
                        lan.command(from, "admin-no");
                    }
                }
            } else {
                lan.command(from, "admin-no");
            }
        }
        // an admin's game stops administering (its menu's "log out")
        "admin-logout" => {
            if let Some(role) = adm.admins.remove(&from) {
                adm.admin_at.remove(&from);
                let who = reply::player_label(lan, from);
                if let Some(h) = hub {
                    h.note(&who, Some(role), "logout", "ok", "the game's Administration menu");
                }
                lan.command(from, "admin-result 0 applied you no longer administer this server");
            }
        }
        "admin" | "admin-list" | "admin-state" => {
            let Some(role) = adm.admins.get(&from).copied() else {
                // (the machine's own tool is always let in by the gateway; a player is told)
                if from != LOCAL_ADMIN {
                    lan.command(from, "admin-result 0 rejected you do not administer this server: say /admin <password> in the chat");
                }
                return;
            };
            match verb {
                "admin-list" => reply::players_list(lan, from, &players(lan, adm), arg),
                "admin-state" => reply::state(lan, from, hub, role),
                _ => exec::from_game(lan, from, role, arg, adm, positions, rules, hub),
            }
        }
        // a player's game took the duty it was given (`duty`), or could not: said for the tool
        // that gave it
        "duty-off-ok" => log::info!("server: player {from} left the duty"),
        "duty-off-none" => log::info!("server: player {from} had no duty to leave"),
        "duty-ok" => log::info!("server: player {from} took duty {}", arg.trim()),
        "duty-no" => log::info!("server: player {from} could not take the duty: {}", arg.trim()),
        // a player's game showed a notification (`notify`): said for the tool that sent it
        "notify-seen" => log::info!("server: player {from} saw notice {}", arg.trim()),
        _ => log::info!("server: command '{text}' from player {from} not taken"),
    }
}

#[cfg(test)]
mod password_tests {
    use super::*;

    /// One player's wrong passwords lock that player out, not the admins; only many sources
    /// guessing at once lock everybody.
    #[test]
    fn wrong_passwords_lock_their_source() {
        let mut adm = ServerAdmin::default();
        for _ in 0..LOCK_AFTER {
            adm.failed("10.0.0.5".into());
        }
        assert!(adm.locked("10.0.0.5"));
        assert!(!adm.locked("10.0.0.6"));
        // (each source counts its share until the lock for all)
        for n in 0..(LOCK_ALL / LOCK_AFTER) {
            for _ in 0..LOCK_AFTER {
                adm.failed(format!("192.0.2.{n}"));
            }
        }
        assert!(adm.locked("10.0.0.6"));
    }

    /// A wrong answer counts against the player's own address, or their number when they
    /// are on this machine (the web gateway's loopback is not one source for all).
    #[test]
    fn a_wrong_answer_counts_against_its_player() {
        assert_eq!(counted_source(Some("203.0.113.9:5000".parse().unwrap()), 1), "203.0.113.9");
        assert_eq!(counted_source(Some("127.0.0.1:41000".parse().unwrap()), 3), "player 3");
        assert_eq!(counted_source(Some("127.0.0.1:41001".parse().unwrap()), 4), "player 4");
        assert_eq!(counted_source(None, 5), "player 5");
    }

    /// The challenges come from the secure random: 128 bits, a new one each time.
    #[test]
    fn challenges_are_fresh() {
        let (a, b) = (fresh_challenge(), fresh_challenge());
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn the_answer_needs_the_password_and_a_slow_key() {
        let salt = "00112233445566778899aabbccddeeff";
        let a = answer(&format!("c0ffee {salt} {MIN_ROUNDS}"), "correct horse").unwrap();
        assert_eq!(a, keyed_response(&derive("correct horse", &unhex(salt).unwrap(), MIN_ROUNDS), "c0ffee"));
        assert_ne!(answer(&format!("c0ffee {salt} {MIN_ROUNDS}"), "wrong horse"), Some(a));
        assert_eq!(answer(&format!("c0ffee {salt} 1"), "correct horse"), None);
        assert_eq!(answer("c0ffee", "correct horse"), None);
        assert_eq!(answer(&format!("c0ffee 00 {MIN_ROUNDS}"), "correct horse"), None);
        assert_eq!(answer(&format!("c0ffee {salt} {}", MAX_ROUNDS + 1), "correct horse"), None);
    }

    #[test]
    fn the_server_keeps_its_key_until_the_password_changes() {
        let mut adm = ServerAdmin { password: "a long password".into(), ..Default::default() };
        let first = (adm.derived().salt, adm.derived().admin);
        assert_eq!((adm.derived().salt, adm.derived().admin), first);
        adm.password = "another long one".into();
        assert_ne!((adm.derived().salt, adm.derived().admin), first);
    }

    /// The answer proves the role of the password it was made with, and nothing else: one
    /// key for each, with one salt; a moderator's password like the admin's is the admin's.
    #[test]
    fn the_answer_proves_its_role() {
        let mut adm = ServerAdmin { password: "admin secret".into(), moderator_password: "mod secret".into(), ..Default::default() };
        let k = adm.derived();
        let challenge = format!("c0ffee {} {KDF_ROUNDS}", hex(&k.salt));
        let (a, m) = (answer(&challenge, "admin secret").unwrap(), answer(&challenge, "mod secret").unwrap());
        assert_eq!(adm.role_of("c0ffee", &a), Some(Role::Admin));
        assert_eq!(adm.role_of("c0ffee", &m), Some(Role::Moderator));
        assert_eq!(adm.role_of("c0ffee", &answer(&challenge, "guess").unwrap()), None);
        // the same answer for another challenge proves nothing
        assert_eq!(adm.role_of("c0ffef", &a), None);
        // no admin password: the moderator's alone
        let mut adm = ServerAdmin { moderator_password: "mod secret".into(), ..Default::default() };
        let challenge = format!("c0ffee {} {KDF_ROUNDS}", hex(&adm.derived().salt));
        assert_eq!(adm.role_of("c0ffee", &answer(&challenge, "mod secret").unwrap()), Some(Role::Moderator));
        assert_eq!(adm.role_of("c0ffee", &answer(&challenge, "").unwrap()), None);
    }
}

#[cfg(test)]
mod duty_tests {
    use super::{duty_arg, parse_duty};

    #[test]
    fn a_duty_goes_to_the_game_and_back() {
        assert_eq!(duty_arg("15", "5", 2, 7), "15 5 2 7");
        assert_eq!(parse_duty("15 5 2 7"), Some(("15".into(), "5".into(), 2, 7)));
        // names with spaces and percent signs
        let a = duty_arg("KI-Zug", "ZOB RB 100%", 0, 0);
        assert_eq!(a, "KI-Zug ZOB%20RB%20100%25 0 0");
        assert_eq!(parse_duty(&a), Some(("KI-Zug".into(), "ZOB RB 100%".into(), 0, 0)));
        // missing or extra parts, not numbers: not a duty
        for bad in ["", "15", "15 5", "15 5 2", "15 5 2 7 9", "15 5 x 7", "15 5 2 -1"] {
            assert_eq!(parse_duty(bad), None, "{bad}");
        }
    }
}

#[cfg(test)]
mod notice_target_tests {
    use super::notice_targets;

    #[test]
    fn all_is_every_player_and_the_host_itself() {
        // the host is player 1, the others 2 and 5
        assert_eq!(notice_targets("all", [2, 5].into_iter(), 1), (vec![2, 5], true));
        assert_eq!(notice_targets("all", [].into_iter(), 1), (vec![], true));
        // one player; the host's own number: its own screen, nothing sent
        assert_eq!(notice_targets("5", [2, 5].into_iter(), 1), (vec![5], false));
        assert_eq!(notice_targets("1", [2, 5].into_iter(), 1), (vec![], true));
        // not a number: nobody
        assert_eq!(notice_targets("x", [2, 5].into_iter(), 1), (vec![], false));
    }
}

#[cfg(test)]
mod passenger_density_command_tests {
    use super::*;

    fn session() -> LanSession {
        // (no rendezvous on the internet from a test)
        std::env::set_var("OMSI_NO_BRIDGE", "1");
        LanSession::host(0, "server", omsi_net::WorldInfo::default(), true).unwrap()
    }

    fn run(lan: &mut LanSession, adm: &mut ServerAdmin, text: &str) {
        let rules = ServerRules { real_time: false, traffic_max: 120, notice_ok: &|_| true, metar: false };
        adm.admins.insert(LOCAL_ADMIN, Role::Admin);
        server_command(lan, LOCAL_ADMIN, &format!("admin {text}"), adm, &|_| None, &rules, None);
        // (the host loop's part: the world's operations)
        for job in adm.take_world() {
            if let Action::PassengerDensity(d) = job.action {
                adm.passenger_density = Some(d);
            }
        }
    }

    /// `passengers <factor>` sets the density the host loop applies from then on; a wrong
    /// value changes nothing, and a server without passengers has none to change.
    #[test]
    fn an_admin_changes_the_passenger_density() {
        let mut lan = session();
        let mut adm = ServerAdmin { passengers_on: true, ..Default::default() };
        run(&mut lan, &mut adm, "passengers 2");
        assert_eq!(adm.passenger_density, Some(2.0));
        for bad in ["passengers -1", "passengers 9", "passengers NaN", "passengers 1;kick 2"] {
            run(&mut lan, &mut adm, bad);
            assert_eq!(adm.passenger_density, Some(2.0), "{bad}");
        }
        run(&mut lan, &mut adm, "passengers 0");
        assert_eq!(adm.passenger_density, Some(0.0));
        let mut off = ServerAdmin { passengers_on: false, ..Default::default() };
        run(&mut lan, &mut off, "passengers 2");
        assert_eq!(off.passenger_density, None);
        // (not an admin: nothing)
        let mut adm = ServerAdmin { passengers_on: true, ..Default::default() };
        let rules = ServerRules { real_time: false, traffic_max: 120, notice_ok: &|_| true, metar: false };
        server_command(&mut lan, 5, "admin passengers 2", &mut adm, &|_| None, &rules, None);
        assert!(adm.take_world().is_empty());
        assert_eq!(adm.passenger_density, None);
    }
}
