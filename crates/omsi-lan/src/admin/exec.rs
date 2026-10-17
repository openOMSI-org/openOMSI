//! One way of doing an administration operation on a dedicated server, whoever asks: a
//! player's Administration menu (`admin <action>`), the machine's own tool (`POST /admin`)
//! or the web panel (an `omsi_admin::Job`). Each is checked against the role of whoever asks
//! and the player it names (still the one picked: `Target`), done on the server loop's tick -
//! the session's part at once, the world's part by the loop right after (`WorldJob`) - and
//! answered with what became of it (`report`: the record, the audit log, and an `admin-result`
//! to the player's game that asked).

use super::{act_on_session, beside, Action, Admin, Done as SessionDone, ServerAdmin, ServerRules, WeatherAction, Whom, LOCAL_ADMIN};
use omsi_admin::{Actor, Hub, Job, Op, Origin, Outcome, Perm, Player, PlayerState, Role, Snapshot, Target, TrafficOp, WeatherOp};
use omsi_net::LanSession;
use std::sync::{Arc, OnceLock};

/// The dedicated server's hub (its records, sessions and audit log), set once at its start.
static HUB: OnceLock<Arc<Hub>> = OnceLock::new();

/// A dedicated server's administration starts: its hub, for the loop and the web gateway.
pub fn install_hub(hub: Arc<Hub>) -> bool {
    HUB.set(hub).is_ok()
}

/// The dedicated server's hub (None: no dedicated server in this process).
pub fn hub() -> Option<&'static Arc<Hub>> {
    HUB.get()
}

/// An operation on the server's own world, for the host loop (its clock, its weather, its
/// traffic, its people): who asked, its record, the action.
#[derive(Debug, Clone)]
pub struct WorldJob {
    pub actor: Actor,
    pub record: u64,
    pub action: Action,
}

/// What `exec` did.
#[derive(Debug, Clone, PartialEq)]
pub enum Exec {
    Done(Outcome),
    /// Checked; the host loop does it on the server's world (`ServerAdmin::take_world`).
    World,
}

/// What an action needs.
pub fn perm_of(a: &Action) -> Perm {
    match a {
        Action::Time(_) | Action::Speed(_) | Action::Clock(_) | Action::Weather(_) | Action::Traffic(_) | Action::PassengerDensity(_) => Perm::World,
        // (moving other players' buses - admins' too - is the admin's)
        Action::Bring(_) | Action::BringAll => Perm::World,
        _ => Perm::Moderate,
    }
}

/// The action of an operation of the web panel (the same the game's menu sends).
pub fn action_of(op: &Op) -> Action {
    match op {
        Op::Kick { target, reason } => Action::Kick { id: target.id, ban: false, why: reason.clone() },
        Op::Ban { target, reason } => Action::Kick { id: target.id, ban: true, why: reason.clone() },
        Op::Say(t) => Action::Say(t.clone()),
        Op::Tell { target, text } => Action::Tell { id: target.id, text: text.clone() },
        Op::Clock(s) => Action::Clock(*s),
        Op::Shift(s) => Action::Time(*s),
        Op::Speed(f) => Action::Speed(*f),
        Op::Weather(WeatherOp::Next) => Action::Weather(WeatherAction::Next),
        Op::Weather(WeatherOp::Set(f)) => Action::Weather(WeatherAction::Set(f.clone())),
        Op::Traffic(TrafficOp::Density(n)) => Action::Traffic(n.to_string()),
        Op::Traffic(TrafficOp::Clear) => Action::Traffic("clear".into()),
        Op::PassengerDensity(d) => Action::PassengerDensity(*d),
    }
}

/// The player an action is for.
fn subject(a: &Action) -> Option<u32> {
    match a {
        Action::Kick { id, .. } | Action::Bring(id) | Action::Goto(id) | Action::Tell { id, .. } | Action::Duty { id, .. } | Action::DutyOff(id) | Action::Unstick(id) => Some(*id),
        Action::Service { whom: Whom::Player(id), .. } => Some(*id),
        Action::Notify { who, .. } => who.parse().ok(),
        _ => None,
    }
}

/// The players as the admins see them.
pub fn players(lan: &LanSession, adm: &ServerAdmin) -> Vec<Player> {
    let mut v: Vec<Player> = lan
        .peers()
        .map(|p| {
            let q = &p.pose;
            let state = match q.walker {
                _ if !p.has_pose => PlayerState::Joining,
                Some(w) if w.aboard.is_some() => PlayerState::Aboard,
                Some(_) => PlayerState::OnFoot,
                None if q.has_vehicle() => PlayerState::Driving,
                None => PlayerState::Joining,
            };
            let driving = state == PlayerState::Driving;
            Player {
                id: q.id,
                conn: p.conn,
                name: omsi_admin::clean(&q.name, 64),
                state,
                vehicle: if driving { q.bus.clone() } else { String::new() },
                line: if driving { q.line.clone() } else { String::new() },
                tour: q.tour.clone(),
                aboard: q.walker.and_then(|w| w.aboard).map(|a| a.owner),
                online_secs: p.joined.elapsed().as_secs(),
                admin: adm.admins.get(&q.id).copied(),
            }
        })
        .collect();
    v.sort_by_key(|p| p.id);
    v
}

/// The server's state and players for the panels (the loop, about once a second).
pub fn publish(hub: &Hub, lan: &LanSession, adm: &ServerAdmin, mut server: omsi_admin::ServerState) {
    let players = players(lan, adm);
    server.players = players.len();
    hub.publish(Snapshot { server, players, at: Some(std::time::Instant::now()) });
}

/// How the records name a player's game.
pub(super) fn actor_of(lan: &LanSession, from: u32, role: Role) -> Actor {
    if from == LOCAL_ADMIN {
        return Actor { label: "local tool (POST /admin)".into(), role, origin: Origin::Local };
    }
    Actor { label: super::reply::player_label(lan, from), role, origin: Origin::Game(from) }
}

/// Tell what became of operation `record`: its record and the audit log, and the player's
/// game that asked (`admin-result <record> <status> <message>`).
pub fn report(lan: &mut LanSession, hub: Option<&Hub>, actor: &Actor, record: u64, out: &Outcome) {
    match hub {
        Some(h) => h.finish(record, actor, out),
        None => log::info!("admin: {} -> {} {}", actor.label, out.status.name(), out.message),
    }
    if let Origin::Game(pid) = actor.origin {
        lan.command(pid, &format!("admin-result {record} {} {}", out.status.name(), out.message));
    }
}

/// An `admin <action>` line: the action, and the player picked in it if it names one as
/// `<number>#<connection>` (the new menus do; an older game's `kick 3` names no connection).
/// Only the word where the action names its player counts: a `12#3` in a chat line or a
/// reason is text.
pub fn split_guard(line: &str) -> (String, Option<Target>) {
    let mut words: Vec<&str> = line.split(' ').collect();
    // (where each action that names a player names it)
    let at = match words.first().copied().unwrap_or("") {
        "kick" | "ban" | "bring" | "goto" | "tell" | "duty" | "duty-off" | "unstick" | "notify" => 1,
        "service" => 2,
        _ => return (line.to_string(), None),
    };
    let Some(guard) = words.get(at).and_then(|w| Target::parse_tag(w)) else { return (line.to_string(), None) };
    let id = guard.id.to_string();
    words[at] = &id;
    (words.join(" "), Some(guard))
}

/// An `admin <action>` from a player's game (or the machine's own tool), whose role is `role`.
#[allow(clippy::too_many_arguments)]
pub(super) fn from_game(lan: &mut LanSession, from: u32, role: Role, arg: &str, adm: &mut ServerAdmin, positions: &dyn Fn(u32) -> Option<(glam::DVec3, f64)>, rules: &ServerRules, hub: Option<&Hub>) {
    let actor = actor_of(lan, from, role);
    let (line, guard) = split_guard(arg.trim());
    let verb = line.split(' ').next().unwrap_or("").to_string();
    let action = Action::parse(&line);
    // (who the player is now, for the records: they may be gone once it is done)
    let target_name = action.as_ref().and_then(subject).and_then(|id| players(lan, adm).into_iter().find(|p| p.id == id)).map(|p| p.label()).unwrap_or_default();
    let record = hub.map(|h| h.begin(&actor, &verb, &omsi_admin::clean(&line, 160), guard, &target_name)).unwrap_or(0);
    let Some(action) = action else {
        report(lan, hub, &actor, record, &Outcome::rejected(format!("'{}' is not an administration command the server takes", omsi_admin::clean(&verb, 24))));
        return;
    };
    run(lan, adm, actor, record, action, guard, positions, rules, hub);
}

/// A request of the web panel, taken from the hub's queue by the loop.
pub fn run_job(lan: &mut LanSession, adm: &mut ServerAdmin, job: Job, positions: &dyn Fn(u32) -> Option<(glam::DVec3, f64)>, rules: &ServerRules, hub: &Hub) {
    if job.stale(std::time::Instant::now()) {
        report(lan, Some(hub), &job.actor, job.id, &Outcome::rejected("it waited too long for the server (it was busy or stuck): ask again"));
        return;
    }
    let action = action_of(&job.op);
    run(lan, adm, job.actor, job.id, action, job.op.target(), positions, rules, Some(hub));
}

#[allow(clippy::too_many_arguments)]
fn run(lan: &mut LanSession, adm: &mut ServerAdmin, actor: Actor, record: u64, action: Action, guard: Option<Target>, positions: &dyn Fn(u32) -> Option<(glam::DVec3, f64)>, rules: &ServerRules, hub: Option<&Hub>) {
    let out = match exec(lan, adm, &actor, &action, guard, positions, rules) {
        Exec::Done(out) => out,
        Exec::World => {
            adm.world.push(WorldJob { actor, record, action });
            return;
        }
    };
    report(lan, hub, &actor, record, &out);
}

/// Check and do an action: its session part at once (`Exec::Done` with what became of it),
/// or the world's left to the host loop (`Exec::World`, checked).
pub fn exec(lan: &mut LanSession, adm: &ServerAdmin, actor: &Actor, action: &Action, guard: Option<Target>, positions: &dyn Fn(u32) -> Option<(glam::DVec3, f64)>, rules: &ServerRules) -> Exec {
    use Exec::Done;
    let perm = perm_of(action);
    if !actor.role.allows(perm) {
        return Done(Outcome::rejected(format!("a {} may not do this: the clock, the weather, the traffic, the passengers and bringing players are the admin's", actor.role.name())));
    }
    let list = players(lan, adm);
    // the player picked is still the one (not who has the number since)
    if let Some(t) = guard {
        if let Err(e) = omsi_admin::resolve(&list, t) {
            return Done(Outcome::rejected(e.message()));
        }
        if subject(action) != Some(t.id) {
            return Done(Outcome::rejected("the player picked is not the one the command names"));
        }
    }
    let who = match subject(action) {
        Some(id) => match list.iter().find(|p| p.id == id) {
            Some(p) => Some(p.clone()),
            None => return Done(Outcome::rejected(format!("no player {id} on the server"))),
        },
        None => None,
    };
    let name = who.as_ref().map(Player::label).unwrap_or_default();
    let from = match actor.origin {
        Origin::Game(id) => Some(id),
        _ => None,
    };
    let at = from.and_then(positions);
    let admin = Admin { id: from, at, sent_away: "sent away by an admin", notice_ok: rules.notice_ok };
    let session = |lan: &mut LanSession| act_on_session(lan, action, &admin);
    match action {
        Action::Kick { id, ban, .. } => {
            if actor.role == Role::Moderator && who.as_ref().and_then(|p| p.admin) == Some(Role::Admin) {
                return Done(Outcome::rejected("a moderator may not send an admin away"));
            }
            session(lan);
            if lan.peers().any(|p| p.pose.id == *id) {
                Done(Outcome::rejected(format!("{name} is still on the server")))
            } else {
                Done(Outcome::applied(format!("{name} sent away{}", if *ban { " for the rest of this server run" } else { "" })))
            }
        }
        Action::Say(_) => {
            session(lan);
            Done(Outcome::applied("said to everybody"))
        }
        Action::Tell { .. } => {
            session(lan);
            Done(Outcome::pending(format!("sent to {name}'s game")))
        }
        Action::Bring(_) | Action::BringAll if at.is_none() => Done(Outcome::rejected("only an admin driving on the server can bring players to their bus")),
        Action::Bring(_) | Action::BringAll => {
            session(lan);
            Done(Outcome::pending("sent to the players' games (a player on foot is put back at the wheel first)"))
        }
        Action::Goto(id) => match (from, positions(*id)) {
            (Some(me), Some((pos, h))) => {
                let (x, y) = beside(pos, h, 8.0);
                lan.command(me, &format!("teleport {x:.2} {y:.2} {:.2} {h:.1}", pos.z));
                Done(Outcome::pending(format!("your game goes to {name}")))
            }
            (None, _) => Done(Outcome::rejected("only from the game")),
            (_, None) => Done(Outcome::rejected(format!("{name} has no bus to go to"))),
        },
        Action::Duty { .. } | Action::DutyOff(_) | Action::Service { .. } | Action::Unstick(_) | Action::Notify { .. } => {
            let what = if who.is_some() { format!("{name}'s game") } else { "the players' games".to_string() };
            match session(lan) {
                SessionDone::Session | SessionDone::AlsoHere => Done(Outcome::pending(format!("sent to {what}; its answer is in the server's log"))),
                SessionDone::World => Done(Outcome::rejected("not for a server")),
            }
        }
        Action::Time(_) | Action::Speed(_) | Action::Clock(_) if rules.real_time => Done(Outcome::rejected("the clock follows this machine's real time (real_time = 1 in server.cfg): it cannot be moved or sped up")),
        Action::Weather(_) if rules.metar => Done(Outcome::rejected("the weather follows the METAR report (metar_sync = 1 in server.cfg): it cannot be changed")),
        Action::Weather(WeatherAction::Set(f)) if !omsi_admin::weather_file_ok(f) => Done(Outcome::rejected("not a Weather/….owt file")),
        Action::Traffic(arg) if super::TrafficOrder::parse(arg, rules.traffic_max).is_none() => Done(Outcome::rejected(format!("a traffic density from 0 to {} or 'clear'", rules.traffic_max))),
        Action::PassengerDensity(_) if !adm.passengers_on => Done(Outcome::rejected("the server has no passengers (passengers = 0 in server.cfg)")),
        Action::Time(_) | Action::Speed(_) | Action::Clock(_) | Action::Weather(_) | Action::Traffic(_) | Action::PassengerDensity(_) => Exec::World,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_player_picked_is_taken_out_of_the_line() {
        assert_eq!(split_guard("kick 3#17 spam and 4#2"), ("kick 3 spam and 4#2".into(), Some(Target { id: 3, conn: 17 })));
        assert_eq!(split_guard("kick 3 spam"), ("kick 3 spam".into(), None));
        assert_eq!(split_guard("say #1 rule: be nice"), ("say #1 rule: be nice".into(), None));
        // only where the action names its player: in a chat line or a reason it is text
        assert_eq!(split_guard("say meet at stop 12#3"), ("say meet at stop 12#3".into(), None));
        assert_eq!(split_guard("say 12#3 hi"), ("say 12#3 hi".into(), None));
        assert_eq!(split_guard("kick 3 spam like 12#3"), ("kick 3 spam like 12#3".into(), None));
        assert_eq!(split_guard("service repair 4#8"), ("service repair 4".into(), Some(Target { id: 4, conn: 8 })));
        assert_eq!(split_guard("tell 5#2 see 6#1"), ("tell 5 see 6#1".into(), Some(Target { id: 5, conn: 2 })));
        // (a notice for one player names them, as its tag does; one for all names nobody)
        let (line, guard) = split_guard("notify 3#17 9 5 info Hi");
        assert_eq!(guard, Some(Target { id: 3, conn: 17 }));
        assert_eq!(Action::parse(&line).as_ref().and_then(subject), Some(3));
        assert_eq!(Action::parse("notify all 9 5 info Hi").as_ref().and_then(subject), None);
    }

    #[test]
    fn world_and_moderation() {
        assert_eq!(perm_of(&Action::Kick { id: 1, ban: false, why: None }), Perm::Moderate);
        assert_eq!(perm_of(&Action::Say("x".into())), Perm::Moderate);
        assert_eq!(perm_of(&Action::Clock(0.0)), Perm::World);
        assert_eq!(perm_of(&Action::Weather(WeatherAction::Next)), Perm::World);
        assert_eq!(perm_of(&Action::PassengerDensity(1.0)), Perm::World);
        assert_eq!(perm_of(&Action::BringAll), Perm::World);
        assert_eq!(perm_of(&Action::Bring(2)), Perm::World);
        assert_eq!(perm_of(&Action::Goto(2)), Perm::Moderate);
        assert_eq!(perm_of(&Action::Unstick(2)), Perm::Moderate);
        // the web's operations are the menu's actions
        let t = Target { id: 3, conn: 9 };
        assert_eq!(action_of(&Op::Kick { target: t, reason: Some("x".into()) }), Action::parse("kick 3 x").unwrap());
        assert_eq!(action_of(&Op::Ban { target: t, reason: None }), Action::parse("ban 3").unwrap());
        assert_eq!(action_of(&Op::Clock(30600.0)), Action::parse("clock 30600").unwrap());
        assert_eq!(action_of(&Op::Traffic(TrafficOp::Clear)), Action::parse("traffic clear").unwrap());
        assert_eq!(action_of(&Op::PassengerDensity(1.5)), Action::parse("passengers 1.5").unwrap());
    }
}
