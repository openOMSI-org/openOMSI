//! The game menu's "Player list" in a multiplayer session: every player by name and number,
//! what they do and drive, and their role - for everybody. For whoever administers (the host
//! of a game started by code, or a player who proved the admin or moderator password on a
//! dedicated server) a player's page offers sending them away and banning their machine, and
//! a page lists the bans with the button to lift them.
//!
//! The actions are the administration's own lines (`kick`, `hwban`, `unban`, see
//! `omsi_lan::admin`), done by `admin::run` - on this game for a host, on the server for an
//! admin from afar, with the same checks of the role and of the player picked (number *and*
//! connection) as every other way of administering.

use crate::game_lists::ListKind;
use crate::App;
use omsi_admin::{BanView, Player, PlayerState, Role, Target};
use std::sync::atomic::{AtomicUsize, Ordering};

/// The reasons a player sent away or banned can be given (the first: none).
const REASONS: [&str; 5] = ["", "Reckless driving or blocking others", "Insulting or spamming in the chat", "Cheating or exploiting the game", "Against the server's rules"];

/// The reason picked on the open page.
static REASON: AtomicUsize = AtomicUsize::new(0);

fn reason() -> &'static str {
    REASONS[REASON.load(Ordering::Relaxed) % REASONS.len()]
}

/// What this game may do about the players: the host of its own session may everything; an
/// admin of a dedicated server what the server gave them (None: nothing).
pub(crate) fn rights(app: &App) -> Option<Role> {
    let lan = app.net.lan.as_ref()?;
    match lan.role {
        omsi_net::Role::Host => Some(Role::Admin),
        omsi_net::Role::Client if app.net.is_admin => Some(app.net.admin_panel.role.unwrap_or(Role::Admin)),
        omsi_net::Role::Client => None,
    }
}

/// The players as this game knows them: a server's admin gets the server's own list, anybody
/// else the session's.
fn players(app: &App) -> Vec<Player> {
    let Some(lan) = app.net.lan.as_ref() else { return Vec::new() };
    if crate::admin_panel::remote(app) && !app.net.admin_panel.players.is_empty() {
        return app.net.admin_panel.players.clone();
    }
    let mut v = omsi_lan::admin::players(lan, &omsi_lan::admin::ServerAdmin::default());
    if !v.iter().any(|p| p.id == lan.my_id) {
        let state = if app.session.on_foot.is_some() { PlayerState::OnFoot } else if app.player.is_some() { PlayerState::Driving } else { PlayerState::Joining };
        v.push(Player { id: lan.my_id, conn: 0, name: lan.my_name.clone(), state, vehicle: app.args.bus.clone().filter(|_| app.player.is_some()).unwrap_or_default(), line: String::new(), tour: String::new(), aboard: None, online_secs: 0, admin: (lan.role == omsi_net::Role::Host).then_some(Role::Admin) });
    }
    v.sort_by_key(|p| p.id);
    v
}

fn vehicle_name(file: &str) -> String {
    let f = file.replace('\\', "/");
    let n = f.rsplit('/').next().unwrap_or(&f);
    n.rsplit_once('.').map(|(a, _)| a).unwrap_or(n).to_string()
}

fn describe(p: &Player, tr: &dyn Fn(&str) -> String) -> String {
    let what = match p.state {
        PlayerState::Driving => tr("driving"),
        PlayerState::OnFoot => tr("on foot"),
        PlayerState::Aboard => tr("aboard a bus"),
        PlayerState::Joining => tr("joining"),
    };
    let bus = Some(vehicle_name(&p.vehicle)).filter(|b| !b.is_empty()).map(|b| format!(", {b}")).unwrap_or_default();
    format!("{what}{bus}")
}

/// The role a player is shown with: the host of a session, an admin or a moderator.
fn role_word(p: &Player, tr: &dyn Fn(&str) -> String) -> String {
    match p.admin {
        Some(r) => format!(", {}", tr(r.name())),
        None if p.id == 1 => format!(", {}", tr("host")),
        None => String::new(),
    }
}

fn tag(app: &App, t: Target) -> String {
    if crate::admin_panel::remote(app) {
        t.tag()
    } else {
        t.id.to_string()
    }
}

/// The lines of a page of the player list.
pub(crate) fn items(app: &App, kind: &ListKind) -> Vec<(String, String)> {
    let tr = |t: &str| omsi_ui::tr(t).into_owned();
    let noop = |l: String| (l, "noop".to_string());
    let rights = rights(app);
    let list = players(app);
    let my_id = app.net.lan.as_ref().map(|l| l.my_id).unwrap_or(0);
    match kind {
        ListKind::PlayerList => {
            let mut out = vec![noop(format!("{} ({})", tr("Players in this session"), list.len()))];
            out.push((format!("{}: {}", tr("Voice chat of everybody else"), if app.settings.voice_all_off { tr("off") } else { tr("on") }), "vall".into()));
            for p in &list {
                let you = if p.id == my_id { format!(" ({})", tr("you")) } else { String::new() };
                let silenced = if p.id != my_id && app.sound.voice_mutes.is_muted(&p.name) { format!(" [{}]", tr("muted")) } else { String::new() };
                let label = format!("{} (#{}){you} - {}{}{silenced}", p.name, p.id, describe(p, &tr), role_word(p, &tr));
                // (a page of their own for every other player: their voice is muted there, and
                // for whoever may act on them they are sent away; nobody acts on themselves)
                if p.id != my_id {
                    out.push((label, format!("pick {}", p.target().tag())));
                } else {
                    out.push(noop(label));
                }
            }
            if rights.is_some() {
                let n = bans(app).len();
                out.push((format!("{} ({n})...", tr("Banned machines")), "bans".into()));
            } else {
                out.push(noop(tr("Only the host and the server's admins can send players away")));
            }
            out.push((tr("Back"), "back".into()));
            out
        }
        ListKind::PlayerPage(t) => {
            let Some(p) = list.iter().find(|p| p.id == t.id && p.conn == t.conn) else {
                return vec![(tr("That player is no longer here: pick them again"), "list".into())];
            };
            let muted = app.sound.voice_mutes.is_muted(&p.name);
            let mut out = vec![
                noop(format!("{} (#{}) - {}{}", p.name, p.id, describe(p, &tr), role_word(p, &tr))),
                // (a mute is this player's own: by name, kept between sessions)
                if muted {
                    (tr("Voice chat: muted. Hear them again"), format!("vunmute {}", t.tag()))
                } else {
                    (tr("Voice chat: audible. Mute them"), format!("vmute {}", t.tag()))
                },
            ];
            // (an admin's: silencing a player's voice for the whole session, or taking them away)
            let acts = rights.is_some() && !(rights == Some(Role::Moderator) && p.admin == Some(Role::Admin));
            if acts {
                out.push((tr("Silence their voice chat for everybody"), format!("run vmute {}", tag(app, *t))));
                out.push((tr("Allow their voice chat again"), format!("run vunmute {}", tag(app, *t))));
                out.push((tr("Send away..."), format!("do kick {}", t.tag())));
                out.push((tr("Ban this machine..."), format!("do ban {}", t.tag())));
            }
            out.push((tr("Back"), "list".into()));
            out
        }
        ListKind::PlayerDo(verb, t) => {
            let Some(p) = list.iter().find(|p| p.id == t.id && p.conn == t.conn) else { return vec![(tr("That player is no longer here: pick them again"), "list".into())] };
            let who = format!("{} (#{})", p.name, p.id);
            let why = reason();
            let mut out = vec![noop(tr(if verb == "ban" { "Ban the machine of {who}" } else { "Send away {who}" }).replace("{who}", &who))];
            out.push((format!("{}: {}", tr("Reason"), if why.is_empty() { tr("none") } else { tr(why) }), "reason".into()));
            let tg = tag(app, *t);
            let r = if why.is_empty() { String::new() } else { format!(" {why}") };
            if verb == "ban" {
                // the session's own list, or (the admin's) all the owner's sessions - for a day, a week, or for good
                for (scope_label, scope, owner) in [(tr("This session"), "session", false), (tr("All my sessions"), "all", true)] {
                    if owner && rights != Some(Role::Admin) {
                        continue;
                    }
                    for (span, h) in [(tr("for a day"), "24h"), (tr("for a week"), "7d"), (tr("for good"), "")] {
                        let hours = if h.is_empty() { String::new() } else { format!(" {h}") };
                        out.push((format!("{scope_label} - {span}"), format!("run hwban {tg} {scope}{hours}{r}")));
                    }
                }
                out.push((tr("Only for this run (by name and address, not the machine)"), format!("run ban {tg}{r}")));
            } else {
                out.push((tr("Send away now"), format!("run kick {tg}{r}")));
            }
            out.push((tr("Cancel"), format!("pick {}", t.tag())));
            out
        }
        ListKind::BanList => {
            let mut out = vec![noop(tr("Banned machines (a ban holds the machine, not the name)"))];
            let now = omsi_admin::unix_now();
            let bans = bans(app);
            if bans.is_empty() {
                out.push(noop(tr("Nobody is banned")));
            }
            for b in &bans {
                let until = b.until.map(|u| format!(", {}", tr(&format!("ends in {}", omsi_lan::admin::ban::span(u.saturating_sub(now)))))).unwrap_or_else(|| format!(", {}", tr("for good")));
                let name = if b.name.is_empty() { tr("unknown name") } else { b.name.clone() };
                let reason = if b.reason.is_empty() { String::new() } else { format!(" - {}", b.reason) };
                let label = format!("{name} [{}{until}]{reason}", if b.scope == "all" { tr("all sessions") } else { tr("this session") });
                // (the owner's list is the admin's: a moderator only sees it)
                if rights == Some(Role::Admin) {
                    out.push((format!("{} {label}", tr("Lift:")), format!("unban {}", b.id)));
                } else {
                    out.push(noop(label));
                }
            }
            out.push((tr("Back"), "list".into()));
            out
        }
        _ => Vec::new(),
    }
}

/// The bans that hold: the host's own lists, or what the server last told its admin.
fn bans(app: &App) -> Vec<BanView> {
    if crate::admin_panel::remote(app) {
        app.net.admin_panel.bans.clone()
    } else {
        omsi_lan::admin::ban::views()
    }
}

/// A line of a page picked: the page to show next (None: back to the game menu).
pub(crate) fn run(app: &mut App, kind: &ListKind, action: &str) -> Option<ListKind> {
    let (verb, arg) = action.split_once(' ').unwrap_or((action, ""));
    let target = || Target::parse_tag(arg.split(' ').next().unwrap_or(""));
    match verb {
        "pick" => {
            REASON.store(0, Ordering::Relaxed);
            target().map(ListKind::PlayerPage).or(Some(ListKind::PlayerList))
        }
        "do" => {
            let (what, tg) = arg.split_once(' ')?;
            Some(ListKind::PlayerDo(what.to_string(), Target::parse_tag(tg)?))
        }
        "reason" => {
            REASON.store((REASON.load(Ordering::Relaxed) + 1) % REASONS.len(), Ordering::Relaxed);
            Some(kind.clone())
        }
        // (the administration's own line, checked once more where it is done)
        "run" if rights(app).is_some() => {
            crate::admin::run(app, arg);
            if crate::admin_panel::remote(app) {
                app.net.admin_panel.soon();
            }
            Some(ListKind::PlayerList)
        }
        "unban" if rights(app) == Some(Role::Admin) => {
            crate::admin::run(app, action);
            if crate::admin_panel::remote(app) {
                app.net.admin_panel.soon();
            }
            Some(ListKind::BanList)
        }
        // the player's own voice mute (by name, kept), and every other voice off
        "vmute" | "vunmute" => {
            let t = target()?;
            if let Some(p) = players(app).into_iter().find(|p| p.id == t.id && p.conn == t.conn) {
                if !app.sound.voice_mutes.set(&p.name, verb == "vmute") {
                    app.service_msg = Some(("The mute list is full".into(), 3.0));
                }
            }
            Some(ListKind::PlayerPage(t))
        }
        "vall" => {
            app.settings.voice_all_off = !app.settings.voice_all_off;
            crate::game_lists::remember_setting("voice_all_off", &(app.settings.voice_all_off as u8).to_string());
            Some(ListKind::PlayerList)
        }
        "bans" => Some(ListKind::BanList),
        "list" | "back" => Some(ListKind::PlayerList),
        "noop" => Some(kind.clone()),
        _ => Some(kind.clone()),
    }
}

/// The page a "back" from `kind` goes to.
pub(crate) fn back(kind: &ListKind) -> Option<ListKind> {
    match kind {
        ListKind::PlayerList => None,
        ListKind::PlayerPage(_) | ListKind::BanList => Some(ListKind::PlayerList),
        ListKind::PlayerDo(_, t) => Some(ListKind::PlayerPage(*t)),
        _ => None,
    }
}

pub(crate) fn is_page(k: Option<&ListKind>) -> bool {
    matches!(k, Some(ListKind::PlayerList | ListKind::PlayerPage(_) | ListKind::PlayerDo(..) | ListKind::BanList))
}

/// Each frame: while a page is open, show the players as they are now (somebody joined or
/// left, or changed what they drive) - a signature of the list, so the page is made again
/// only when it changed.
pub(crate) fn tick(app: &mut App) {
    if !is_page(app.menus.list_kind.as_ref()) {
        return;
    }
    let sig: Vec<(u32, u64, PlayerState)> = players(app).iter().map(|p| (p.id, p.conn, p.state)).collect();
    let bans = bans(app).len();
    let key = (sig, bans);
    thread_local! {
        static SEEN: std::cell::RefCell<Option<(Vec<(u32, u64, PlayerState)>, usize)>> = const { std::cell::RefCell::new(None) };
    }
    let changed = SEEN.with(|s| {
        let mut s = s.borrow_mut();
        let changed = s.as_ref().is_some_and(|old| *old != key);
        *s = Some(key);
        changed
    });
    if changed {
        app.refresh_list();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reasons_cycle_and_start_with_none() {
        assert_eq!(REASONS[0], "");
        assert!(REASONS.iter().all(|r| r.len() <= 120 && !r.contains('\n')));
    }

    #[test]
    fn a_vehicle_file_is_shown_by_its_name() {
        assert_eq!(vehicle_name("Vehicles/MAN_SD200/MAN_SD77.bus"), "MAN_SD77");
        assert_eq!(vehicle_name(""), "");
    }

    #[test]
    fn back_goes_up_one_page() {
        let t = Target { id: 3, conn: 9 };
        assert_eq!(back(&ListKind::PlayerList), None);
        assert_eq!(back(&ListKind::PlayerPage(t)), Some(ListKind::PlayerList));
        assert_eq!(back(&ListKind::PlayerDo("ban".into(), t)), Some(ListKind::PlayerPage(t)));
        assert_eq!(back(&ListKind::BanList), Some(ListKind::PlayerList));
    }
}
