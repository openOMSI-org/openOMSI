//! What a dedicated server tells an admin's game besides `admin-result`: its players
//! (`admin-list [search]` → an `admin-p` line each, then `admin-end`) and its state
//! (`admin-state` → `admin-s`). Each line is a command of its own (at most `MAX_CHAT`
//! characters), its words escaped where they may hold spaces (`%20`, `%25`), the name last.
//! A game before these commands never asks for them; one that asks a server before them gets
//! no answer, and says so.

use omsi_admin::{Hub, Player, Role};
use omsi_net::LanSession;

/// The most players listed in one answer (a session has fewer).
const MAX_LISTED: usize = 64;

/// `player 3 'Name'`, for the records.
pub fn player_label(lan: &LanSession, id: u32) -> String {
    match lan.peers().find(|p| p.pose.id == id) {
        Some(p) => format!("player {id} '{}'", omsi_admin::clean(&p.pose.name, 64)),
        None => format!("player {id}"),
    }
}

/// A word that may hold spaces, as one word.
pub fn esc(s: &str) -> String {
    let s = s.trim().replace('%', "%25").replace(' ', "%20");
    if s.is_empty() {
        "-".into()
    } else {
        s
    }
}

/// `esc` read back.
pub fn unesc(s: &str) -> String {
    if s == "-" {
        return String::new();
    }
    s.replace("%20", " ").replace("%25", "%")
}

/// A vehicle file as the list shows it: its name without the folders and `.bus`.
fn short_vehicle(file: &str) -> String {
    let f = file.replace('\\', "/");
    let name = f.rsplit('/').next().unwrap_or(&f);
    name.rsplit_once('.').map(|(n, _)| n).unwrap_or(name).chars().take(24).collect()
}

/// One player's line: `admin-p <id> <conn> <state> <admin role|-> <aboard|-> <online s>
/// <vehicle> <name>`.
pub fn player_line(p: &Player) -> String {
    format!(
        "admin-p {} {} {} {} {} {} {} {}",
        p.id,
        p.conn,
        p.state.name(),
        p.admin.map(Role::name).unwrap_or("-"),
        p.aboard.map(|a| a.to_string()).unwrap_or_else(|| "-".into()),
        p.online_secs,
        esc(&short_vehicle(&p.vehicle)),
        p.name
    )
}

/// An `admin-p` line read back (its vehicle as the short name).
pub fn parse_player(arg: &str) -> Option<Player> {
    let mut w = arg.splitn(8, ' ');
    let mut next = || w.next();
    let id = next()?.parse().ok()?;
    let conn = next()?.parse().ok()?;
    let state = omsi_admin::PlayerState::parse(next()?)?;
    let admin = Role::parse(next()?);
    let aboard = next()?.parse().ok();
    let online_secs = next()?.parse().ok()?;
    let vehicle = unesc(next()?);
    let name = next().unwrap_or("").trim().to_string();
    Some(Player { id, conn, name, state, vehicle, line: String::new(), tour: String::new(), aboard, online_secs, admin })
}

/// The players (those a search finds, see `omsi_admin::search`), then `admin-end <count>
/// <total>`.
pub fn players_list(lan: &mut LanSession, to: u32, all: &[Player], query: &str) {
    let found = omsi_admin::search(all, query);
    for p in found.iter().take(MAX_LISTED) {
        lan.command(to, &player_line(p));
    }
    lan.command(to, &format!("admin-end {} {}", found.len().min(MAX_LISTED), all.len()));
}

/// The server's state as an admin's game shows it: `admin-s role=… time=… …` (see
/// `omsi_admin::ServerState`; the words escaped).
pub fn state(lan: &mut LanSession, to: u32, hub: Option<&Hub>, role: Role) {
    let Some(hub) = hub else {
        lan.command(to, &format!("admin-s role={}", role.name()));
        return;
    };
    let snap = hub.snapshot();
    let s = &snap.server;
    let line = format!(
        "admin-s role={} time={} speed={} rt={} traffic={}/{} pax={} dens={:.2}/{} wait={} walk={} aboard={} players={}/{} metar={} weather={}",
        role.name(),
        esc(&s.time),
        s.clock_speed,
        s.real_time as u8,
        s.traffic,
        s.traffic_max,
        s.passengers as u8,
        s.passenger_density,
        s.passenger_density_max,
        s.waiting,
        s.walking,
        s.aboard,
        s.players,
        s.max_players,
        esc(s.metar.as_deref().unwrap_or("")),
        esc(&s.weather.chars().take(40).collect::<String>()),
    );
    lan.command(to, &line);
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_admin::PlayerState;

    #[test]
    fn a_player_line_goes_and_comes_back() {
        let p = Player { id: 3, conn: 17, name: "Pichau  da Silva".into(), state: PlayerState::Driving, vehicle: "Vehicles/MAN SD200/MAN_SD77.bus".into(), line: String::new(), tour: String::new(), aboard: None, online_secs: 95, admin: Some(Role::Moderator) };
        let line = player_line(&p);
        assert!(line.len() <= omsi_net::MAX_CHAT, "{line}");
        let back = parse_player(line.strip_prefix("admin-p ").unwrap()).unwrap();
        assert_eq!((back.id, back.conn, back.state, back.admin, back.aboard, back.online_secs), (3, 17, PlayerState::Driving, Some(Role::Moderator), None, 95));
        assert_eq!((back.vehicle.as_str(), back.name.as_str()), ("MAN_SD77", "Pichau  da Silva"));
        let on_foot = Player { vehicle: String::new(), state: PlayerState::Aboard, aboard: Some(2), admin: None, ..p };
        let back = parse_player(player_line(&on_foot).strip_prefix("admin-p ").unwrap()).unwrap();
        assert_eq!((back.vehicle.as_str(), back.aboard, back.admin, back.state), ("", Some(2), None, PlayerState::Aboard));
        assert_eq!(parse_player("x 1 driving - - 0 - n"), None);
        assert_eq!(unesc(&esc("100% sure")), "100% sure");
    }
}
