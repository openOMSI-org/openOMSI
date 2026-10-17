//! The Administration menu of a game that administers a dedicated server from afar: what the
//! server told it (its players, its state, what became of each operation), when to ask again,
//! and the menu's lines made of that. It talks to the server over the session itself (the
//! commands of `reply`), never to an address of its own: the server may be anywhere.
//!
//! The lines are `(label, action)`; the actions are the game's to carry out (`AdminMenu`):
//! a command for the server, or another page of the menu. A player is always named by number
//! and connection, so a line picked a moment before the list changed still means the player
//! it showed - or nobody, when they are gone.

use crate::commands::Command;
use omsi_admin::{Player, PlayerState, Role, Status, Target};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

/// How often the open menu asks for the players and the state.
const EVERY: Duration = Duration::from_secs(3);
/// An answer not come in this long: the server does not answer (or is older than this game).
const SILENT: Duration = Duration::from_secs(6);
/// The results kept for the menu.
const RESULTS: usize = 6;

/// What the menu knows of the server it administers.
#[derive(Default)]
pub struct Panel {
    /// The role the server gave us (None: an older server said only `admin-ok`).
    pub role: Option<Role>,
    pub players: Vec<Player>,
    incoming: Vec<Player>,
    /// The players on the server (the list may be cut).
    pub total: usize,
    state: HashMap<String, String>,
    /// What became of the latest operations, newest first: (record, status, message).
    pub results: VecDeque<(u64, Status, String)>,
    asked: Option<Instant>,
    /// An answer came since the last question.
    answered: bool,
}

/// What a line of the menu asks the game to do.
#[derive(Debug, Clone, PartialEq)]
pub enum AdminMenu {
    /// `admin <line>` to the server.
    Send(String),
    /// The players' page.
    Players,
    /// A player's page.
    Player(Target),
    /// A question before sending a player away (`kick` or `ban`).
    Confirm(String, Target),
    /// The full list of the older menu.
    Classic,
    /// Stop administering.
    Logout,
    Nothing,
}

impl AdminMenu {
    pub fn parse(action: &str) -> AdminMenu {
        let (verb, arg) = action.split_once(' ').unwrap_or((action, ""));
        let target = || Target::parse_tag(arg);
        match verb {
            "send" if !arg.trim().is_empty() => AdminMenu::Send(arg.trim().to_string()),
            "players" => AdminMenu::Players,
            "pick" => target().map(AdminMenu::Player).unwrap_or(AdminMenu::Nothing),
            "ask-kick" | "ask-ban" => target().map(|t| AdminMenu::Confirm(verb[4..].to_string(), t)).unwrap_or(AdminMenu::Nothing),
            "classic" => AdminMenu::Classic,
            "logout" => AdminMenu::Logout,
            _ => AdminMenu::Nothing,
        }
    }
}

impl Panel {
    /// A command from the server: whether it was the menu's (and the menu should show it).
    pub fn take(&mut self, c: &Command) -> bool {
        match c {
            Command::AdminPlayer(p) => {
                if self.incoming.len() < 64 {
                    self.incoming.push(p.clone());
                }
                false
            }
            Command::AdminListEnd { total, .. } => {
                self.players = std::mem::take(&mut self.incoming);
                self.total = *total;
                self.answered = true;
                true
            }
            Command::AdminState(kv) => {
                self.state = kv.iter().cloned().collect();
                if let Some(r) = self.state.get("role").and_then(|r| Role::parse(r)) {
                    self.role = Some(r);
                }
                self.answered = true;
                true
            }
            Command::AdminResult { record, status, message } => {
                self.results.retain(|r| r.0 != *record || *record == 0);
                self.results.push_front((*record, *status, message.clone()));
                self.results.truncate(RESULTS);
                true
            }
            Command::AdminOk(role) => {
                *self = Panel { role: *role, ..Panel::default() };
                true
            }
            _ => false,
        }
    }

    /// What to ask the server now, while the menu is open (nothing more often than every
    /// three seconds, and never twice before an answer or the time for one).
    pub fn due(&mut self, now: Instant) -> Vec<&'static str> {
        let waiting = self.asked.is_some_and(|a| !self.answered && now.saturating_duration_since(a) < SILENT);
        let fresh = self.asked.is_some_and(|a| now.saturating_duration_since(a) < EVERY);
        if waiting || fresh {
            return Vec::new();
        }
        self.asked = Some(now);
        self.answered = false;
        self.incoming.clear();
        vec!["admin-state", "admin-list"]
    }

    /// The server has not answered what was asked in time.
    pub fn silent(&self, now: Instant) -> bool {
        self.asked.is_some_and(|a| !self.answered && now.saturating_duration_since(a) >= SILENT)
    }

    /// Ask again at once (after an operation, the state it changed).
    pub fn soon(&mut self) {
        self.asked = None;
    }

    fn get(&self, k: &str) -> &str {
        self.state.get(k).map(String::as_str).unwrap_or("")
    }

    fn num(&self, k: &str) -> Option<f64> {
        self.get(k).split('/').next().and_then(|v| v.parse().ok())
    }

    fn may_world(&self) -> bool {
        self.role != Some(Role::Moderator)
    }

    pub fn player(&self, t: Target) -> Option<&Player> {
        self.players.iter().find(|p| p.id == t.id && p.conn == t.conn)
    }

    fn describe(p: &Player, tr: &dyn Fn(&str) -> String) -> String {
        let what = match p.state {
            PlayerState::Driving => tr("driving a bus"),
            PlayerState::OnFoot => tr("on foot"),
            PlayerState::Aboard => format!("{} #{}", tr("aboard the bus of"), p.aboard.unwrap_or(0)),
            PlayerState::Joining => tr("joining"),
        };
        let bus = if p.vehicle.is_empty() { String::new() } else { format!(", {}", p.vehicle) };
        let role = p.admin.map(|r| format!(", {}", tr(r.name()))).unwrap_or_default();
        format!("{what}{bus}{role}, {} min", p.online_secs / 60)
    }

    /// The main page: the server, the players' page, the world, what happened.
    pub fn main_lines(&self, now: Instant, tr: &dyn Fn(&str) -> String) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = Vec::new();
        let noop = |l: String| (l, "noop".to_string());
        if self.silent(now) {
            out.push(noop(tr("The server does not answer the new menu (it may be older than this game): the full list below still works")));
        } else if self.state.is_empty() {
            out.push(noop(tr("Asking the server...")));
        } else {
            let role = self.role.map(|r| tr(r.name())).unwrap_or_else(|| tr("admin"));
            out.push(noop(format!("{}: {role} · {} {} · {} {}", tr("You administer this server"), tr("Players"), self.get("players"), tr("Clock"), crate::admin::unesc(self.get("time")))));
            out.push(noop(format!("{}: {}", tr("Weather"), crate::admin::unesc(self.get("weather")))));
        }
        out.push((format!("{} ({})...", tr("Players"), self.players.len()), "players".into()));
        if !self.state.is_empty() {
            self.world_lines(&mut out, tr);
        }
        for (_, status, msg) in self.results.iter().take(4) {
            out.push(noop(format!("{}: {msg}", tr(status_word(*status)))));
        }
        out.push((tr("Full list of commands (older menu)..."), "classic".into()));
        out.push((tr("Stop administering"), "logout".into()));
        out
    }

    fn world_lines(&self, out: &mut Vec<(String, String)>, tr: &dyn Fn(&str) -> String) {
        let noop = |l: String| (l, "noop".to_string());
        if !self.may_world() {
            out.push(noop(tr("The clock, the weather, the traffic and the passengers are for an admin")));
            return;
        }
        // the clock: locked on the real time
        if self.get("rt") == "1" {
            out.push(noop(tr("Clock: follows the server's real time (it cannot be moved)")));
        } else {
            for (label, secs) in [("Clock: 06:00 (morning)", 6 * 3600), ("Clock: 12:00 (noon)", 12 * 3600), ("Clock: 18:00 (evening)", 18 * 3600), ("Clock: 23:00 (night)", 23 * 3600)] {
                out.push((tr(label), format!("send clock {secs}")));
            }
            out.push((tr("Clock +1 hour"), "send time 3600".into()));
            out.push((tr("Clock -1 hour"), "send time -3600".into()));
            let speed = self.num("speed").unwrap_or(1.0);
            for s in [1.0, 2.0, 4.0, 8.0, 15.0, 30.0] {
                let mark = if (s - speed).abs() < 1e-6 { format!("  {}", tr("(now)")) } else { String::new() };
                out.push((format!("{} x{s}{mark}", tr("Time speed")), format!("send speed {s}")));
            }
        }
        // the weather: locked under METAR
        if !self.get("metar").is_empty() && self.get("metar") != "-" {
            out.push(noop(format!("{} ({})", tr("Weather: follows the METAR report (it cannot be changed)"), self.get("metar"))));
        } else {
            out.push((tr("Next weather"), "send weather next".into()));
        }
        // the traffic, in steps of ten
        let (traffic, max) = (self.num("traffic").unwrap_or(0.0) as i64, self.get("traffic").split('/').nth(1).and_then(|m| m.parse::<i64>().ok()).unwrap_or(120));
        out.push((format!("{}: {traffic} / {max} (+10)", tr("Traffic")), format!("send traffic {}", (traffic + 10).min(max))));
        out.push((format!("{}: {traffic} / {max} (-10)", tr("Traffic")), format!("send traffic {}", (traffic - 10).max(0))));
        out.push((tr("Clear the AI traffic (a jam)"), "send traffic clear".into()));
        // the passengers: off on the server, or a density in quarters
        if self.get("pax") == "0" {
            out.push(noop(tr("Passengers: off on this server (passengers = 0)")));
        } else {
            let d = self.num("dens").unwrap_or(1.0);
            let top = self.get("dens").split('/').nth(1).and_then(|m| m.parse::<f64>().ok()).unwrap_or(3.0);
            let counts = format!("{} {}, {} {}, {} {}", self.get("wait"), tr("waiting"), self.get("walk"), tr("walking"), self.get("aboard"), tr("aboard"));
            out.push(noop(format!("{}: {d:.2} ({counts})", tr("Passenger density"))));
            for (label, v) in [("+0.25", (d + 0.25).min(top)), ("-0.25", (d - 0.25).max(0.0)), ("1", 1.0), ("0", 0.0)] {
                out.push((format!("{} → {label}", tr("Passenger density")), format!("send passengers {v}")));
            }
        }
    }

    /// The players' page: every player the server listed, by number and connection.
    pub fn player_lines(&self, tr: &dyn Fn(&str) -> String) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = self.players.iter().map(|p| (format!("{} (#{}) - {}", p.name, p.id, Self::describe(p, tr)), format!("pick {}", p.target().tag()))).collect();
        if out.is_empty() {
            out.push((tr("Nobody else is on the server"), "noop".into()));
        } else if self.total > self.players.len() {
            out.push((format!("{} {}", self.total - self.players.len(), tr("more not listed")), "noop".into()));
        }
        out
    }

    /// A player's page: who, and what may be done (nothing when they are gone).
    pub fn one_player_lines(&self, t: Target, tr: &dyn Fn(&str) -> String) -> Vec<(String, String)> {
        let Some(p) = self.player(t) else {
            return vec![(tr("That player is no longer on the server (or came back on a new connection): pick them again"), "players".into())];
        };
        vec![
            (format!("{} (#{}, {} {}) - {}", p.name, p.id, tr("connection"), p.conn, Self::describe(p, tr)), "noop".into()),
            (tr("Send away..."), format!("ask-kick {}", t.tag())),
            (tr("Send away for this server run..."), format!("ask-ban {}", t.tag())),
        ]
    }

    /// The question before sending a player away.
    pub fn confirm_lines(&self, verb: &str, t: Target, tr: &dyn Fn(&str) -> String) -> Vec<(String, String)> {
        let Some(p) = self.player(t) else {
            return vec![(tr("That player is no longer on the server (or came back on a new connection): pick them again"), "players".into())];
        };
        let q = if verb == "ban" { tr("Send this player away for the rest of this server run?") } else { tr("Send this player away?") };
        vec![(format!("{q} {} (#{}, {} {})", p.name, p.id, tr("connection"), p.conn), "noop".into()), (tr("Yes, send them away"), format!("send {verb} {}", t.tag())), (tr("Cancel"), format!("pick {}", t.tag()))]
    }
}

fn status_word(s: Status) -> &'static str {
    match s {
        Status::Applied => "Applied",
        Status::Pending => "Sent",
        Status::Rejected => "Refused",
        Status::Queued => "Waiting",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tr(s: &str) -> String {
        s.to_string()
    }

    fn feed(p: &mut Panel, from_server: &[&str]) {
        for line in from_server {
            if let Some(c) = Command::parse(1, line) {
                p.take(&c);
            }
        }
    }

    #[test]
    fn the_menu_asks_at_its_pace() {
        let mut p = Panel::default();
        let t0 = Instant::now();
        assert_eq!(p.due(t0), vec!["admin-state", "admin-list"]);
        // no answer yet: no second question, until the time for one has passed
        assert!(p.due(t0 + Duration::from_secs(4)).is_empty());
        assert!(!p.silent(t0 + Duration::from_secs(4)));
        assert!(p.silent(t0 + Duration::from_secs(7)));
        assert_eq!(p.due(t0 + Duration::from_secs(7)).len(), 2);
        // answered: again three seconds after the question
        feed(&mut p, &["admin-end 0 0"]);
        let t1 = t0 + Duration::from_secs(7);
        assert!(p.due(t1 + Duration::from_secs(1)).is_empty());
        assert_eq!(p.due(t1 + Duration::from_secs(3)).len(), 2);
    }

    #[test]
    fn players_and_choices_by_connection() {
        let mut p = Panel::default();
        feed(&mut p, &["admin-ok admin", "admin-p 2 17 driving - - 600 MAN_SD77 Pichau", "admin-p 3 18 on_foot moderator - 60 - pichau_fan", "admin-end 2 2", "admin-s role=admin time=08:30 speed=1 rt=0 traffic=30/120 pax=1 dens=1.00/3 wait=3 walk=4 aboard=1 players=2/16 metar=- weather=Weather/#CAVOK.owt"]);
        assert_eq!(p.role, Some(Role::Admin));
        let lines = p.player_lines(&tr);
        assert_eq!(lines[0], ("Pichau (#2) - driving a bus, MAN_SD77, 10 min".into(), "pick 2#17".into()));
        assert_eq!(lines[1].1, "pick 3#18");
        // the player's page and the question name the connection; the answer sends it
        let t = Target { id: 2, conn: 17 };
        assert_eq!(AdminMenu::parse(&p.one_player_lines(t, &tr)[1].1), AdminMenu::Confirm("kick".into(), t));
        let c = p.confirm_lines("kick", t, &tr);
        assert!(c[0].0.contains("Pichau (#2, connection 17)"), "{c:?}");
        assert_eq!(AdminMenu::parse(&c[1].1), AdminMenu::Send("kick 2#17".into()));
        // gone, or back on another connection: nothing to do but pick again
        feed(&mut p, &["admin-p 2 99 driving - - 1 - Pichau", "admin-end 1 1"]);
        assert_eq!(p.confirm_lines("kick", t, &tr).len(), 1);
        assert_eq!(AdminMenu::parse(&p.confirm_lines("kick", t, &tr)[0].1), AdminMenu::Players);
    }

    #[test]
    fn the_world_lines_follow_the_role_and_the_locks() {
        let state = "admin-s role=admin time=08:30 speed=2 rt=0 traffic=30/120 pax=1 dens=1.00/3 wait=3 walk=4 aboard=1 players=2/16 metar=- weather=Rain";
        let mut p = Panel::default();
        feed(&mut p, &["admin-ok admin", state]);
        let lines = p.main_lines(Instant::now(), &tr);
        let has = |a: &str| lines.iter().any(|l| l.1 == a);
        assert!(has("send clock 21600") && has("send speed 4") && has("send weather next") && has("send traffic 40") && has("send passengers 1.25") && has("send passengers 0"));
        assert!(lines.iter().any(|l| l.0.contains("x2  (now)")));
        // the real time and METAR: no clock, no weather; passengers off: no density
        feed(&mut p, &["admin-s role=admin time=08:30 speed=1 rt=1 traffic=30/120 pax=0 dens=1.00/3 wait=0 walk=0 aboard=0 players=2/16 metar=EDDB weather=x"]);
        let lines = p.main_lines(Instant::now(), &tr);
        assert!(!lines.iter().any(|l| l.1.starts_with("send clock") || l.1.starts_with("send speed") || l.1.starts_with("send weather") || l.1.starts_with("send passengers")));
        assert!(lines.iter().any(|l| l.0.contains("EDDB")));
        // a moderator: no world at all
        feed(&mut p, &["admin-ok moderator", state.replace("role=admin", "role=moderator").as_str()]);
        let lines = p.main_lines(Instant::now(), &tr);
        assert!(!lines.iter().any(|l| l.1.starts_with("send ")));
        // results, newest first
        feed(&mut p, &["admin-result 4 rejected a moderator may not change the server's world", "admin-result 5 applied said to everybody"]);
        let lines = p.main_lines(Instant::now(), &tr);
        let r: Vec<&String> = lines.iter().filter(|l| l.0.starts_with("Applied:") || l.0.starts_with("Refused:")).map(|l| &l.0).collect();
        assert_eq!(r, vec!["Applied: said to everybody", "Refused: a moderator may not change the server's world"]);
    }
}
