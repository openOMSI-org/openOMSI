//! Administration of a LAN session (Esc menu, Administration): the players (send away, send
//! away for the session, bring here, go to), the clock (an hour on or back, how fast it
//! runs), the weather, a notice to everybody.
//!
//! The host of a game started by code administers its own session from its game menu. A
//! dedicated server has no screen: a player who knows its `admin_password` says
//! `/admin <password>` in the chat, and the server's answer opens the same menu in that
//! player's game - every line of it is then sent to the server as `admin <action>` and done
//! there (`server_command`).
//!
//! The password never goes over the network: the player's game asks (`auth?`), the server
//! answers with a fresh challenge, its salt and rounds (`admin-challenge <hex> <salt>
//! <rounds>`), and the game sends `auth <HMAC(PBKDF2(password, salt, rounds), challenge)>`.
//! Wrong answers lock the administration for a while; a player's rights end when the
//! player leaves.
//!
//! Actions (the text of a line, also what goes over the network): `kick <id>`,
//! `ban <id>`, `bring <id>`, `goto <id>`, `time <seconds>`, `speed <factor>`,
//! `weather next`, `say <text>`, `bringall`, `service <repair|refuel|wash> <id|all>`,
//! `unstick <id>`, `clock <seconds of the day>`, `traffic next`, `traffic clear`,
//! `weather cycle`, `weather set <Weather/file.owt>`.

use crate::App;
use omsi_net::Role;

pub(crate) use omsi_lan::admin::{duty_arg, ServerAdmin, LOCAL_ADMIN};
use omsi_lan::admin::{act_on_session, answer, beside, Action, Admin, Done, WeatherAction};
use omsi_lan::commands::Command;

/// The clock speeds offered.
pub(crate) const SPEEDS: [f64; 6] = [1.0, 2.0, 4.0, 8.0, 15.0, 30.0];

/// The lines of the administration menu: (label, action).
pub(crate) fn items(app: &App) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Some(lan) = app.net.lan.as_ref() else { return out };
    let mut peers: Vec<(u32, String)> = lan.peers().map(|p| (p.pose.id, if p.pose.name.is_empty() { format!("Player {}", p.pose.id) } else { p.pose.name.clone() })).collect();
    peers.sort();
    for (id, name) in &peers {
        if *id == lan.my_id {
            continue;
        }
        // (the labels in the interface's language around the player's name)
        let tr = |t: &str| omsi_ui::tr(t).into_owned();
        out.push((format!("{name}: {}", tr("go to")), format!("goto {id}")));
        out.push((format!("{name}: {}", tr("bring here")), format!("bring {id}")));
        out.push((format!("{name}: {}", tr("repair and refuel their bus")), format!("service repair {id}")));
        out.push((format!("{name}: {}", tr("put their bus back on its wheels")), format!("unstick {id}")));
        out.push((format!("{name}: {}", tr("send away")), format!("kick {id}")));
        out.push((format!("{name}: {}", tr("send away for the session")), format!("ban {id}")));
    }
    if peers.len() > 1 {
        out.push((omsi_ui::tr("Bring everybody here").into_owned(), "bringall".into()));
        out.push((omsi_ui::tr("Everybody: repair").into_owned(), "service repair all".into()));
        out.push((omsi_ui::tr("Everybody: refuel").into_owned(), "service refuel all".into()));
        out.push((omsi_ui::tr("Everybody: wash").into_owned(), "service wash all".into()));
    }
    for (label, secs) in [("Clock: 06:00 (morning)", 6 * 3600), ("Clock: 12:00 (noon)", 12 * 3600), ("Clock: 18:00 (evening)", 18 * 3600), ("Clock: 23:00 (night)", 23 * 3600)] {
        out.push((omsi_ui::tr(label).into_owned(), format!("clock {secs}")));
    }
    if let Some(t) = app.session.traffic.as_ref() {
        out.push((format!("{}: {} ({})", omsi_ui::tr("Traffic"), t.target, omsi_ui::tr("more / less")), "traffic next".into()));
    }
    out.push(("Clock +1 hour".into(), "time 3600".into()));
    out.push(("Clock -1 hour".into(), "time -3600".into()));
    let speed = lan.clock_speed;
    for s in SPEEDS {
        let mark = if (s - speed).abs() < 1e-6 { format!("  {}", omsi_ui::tr("(now)")) } else { String::new() };
        out.push((format!("{} x{s}{mark}", omsi_ui::tr("Time speed")), format!("speed {s}")));
    }
    out.push(("Next weather".into(), "weather next".into()));
    // the weather cycle, and each installed weather by name
    let cycling = app.session.weather_cycle.is_some();
    out.push((format!("{}: {}", omsi_ui::tr("Weather cycle"), if cycling { omsi_ui::tr("on") } else { omsi_ui::tr("off") }), "weather cycle".into()));
    for (file, w) in crate::weather_cycle::installed() {
        let now = app.args.weather.as_deref().is_some_and(|c| c.replace('\\', "/").eq_ignore_ascii_case(&file));
        let mark = if now { format!("  {}", omsi_ui::tr("(now)")) } else { String::new() };
        out.push((format!("{}: {}{mark}", omsi_ui::tr("Weather"), w.name), format!("weather set {file}")));
    }
    if app.session.traffic.is_some() {
        out.push((omsi_ui::tr("Clear the AI traffic (a jam)").into_owned(), "traffic clear".into()));
    }
    out.push(("Back".into(), "back".into()));
    out
}

/// A line of the menu chosen in this game.
pub(crate) fn run(app: &mut App, action: &str) {
    if action == "back" {
        return;
    }
    let Some(lan) = app.net.lan.as_mut() else { return };
    if lan.role == Role::Client {
        // (a server's admin: the server does it)
        lan.command(1, &format!("admin {action}"));
        app.service_msg = Some((format!("Sent to the server: {action}"), 3.0));
        return;
    }
    host_action(app, action);
}

/// Carry out an administration action as the host of a game: what it asks of the session
/// is done as a server does it (`omsi_lan::admin::act_on_session`), what it asks of this
/// game's own world here.
fn host_action(app: &mut App, action: &str) {
    let Some(action) = Action::parse(action) else {
        log::info!("admin: unknown action '{action}'");
        return;
    };
    let at = app.player.as_ref().map(|p| (p.vehicle.position, p.vehicle.heading));
    let notice_ok = |text: &str| crate::ui::Notice::parse(text).is_some();
    let admin = Admin { id: None, at, sent_away: "sent away by the host", notice_ok: &notice_ok };
    let Some(done) = app.net.lan.as_mut().map(|l| act_on_session(l, &action, &admin)) else { return };
    match (action, done) {
        (Action::Notify { notice, .. }, Done::AlsoHere) => {
            if let Some((nid, n)) = crate::ui::Notice::parse(&notice) {
                log::info!("LAN: the server's notice {nid}: {}", n.text);
                crate::ui::push_notice(&mut app.menus.notices, n);
            }
        }
        // (everybody's bus: the host's own too)
        (Action::Service { kind, .. }, Done::AlsoHere) => app.run_service(&kind, "player"),
        (Action::Goto(id), _) => match app.net.remotes.remotes.get(&id).map(|r| (r.vehicle().position, r.vehicle().heading)) {
            Some((pos, heading)) => {
                teleport_beside(app, pos, heading);
                app.service_event("teleport", "player", None);
            }
            None => app.service_msg = Some(("That player has no bus to go to".into(), 3.0)),
        },
        (Action::Time(s), _) => app.shift_clock(s),
        (Action::Speed(_), _) if app.real_time_locked() => app.service_msg = Some(("The time speed is fixed while the real-time sync is on".into(), 3.0)),
        (Action::Speed(s), _) => {
            if let Some(l) = app.net.lan.as_mut() {
                l.clock_speed = s;
            }
            app.service_msg = Some((format!("Time speed x{s}"), 3.0));
        }
        (Action::Clock(s), _) => {
            let d = (s - app.clock.time + 43_200.0).rem_euclid(86_400.0) - 43_200.0;
            app.shift_clock(d);
        }
        (Action::Weather(WeatherAction::Cycle), _) => {
            if app.session.weather_cycle.take().is_none() {
                let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(7);
                let mut c = crate::weather_cycle::Cycle::new(seed);
                // (the first change soon, not in an hour)
                c.next_in = 60.0;
                app.session.weather_cycle = Some(c);
            }
            let on = app.session.weather_cycle.is_some();
            app.service_msg = Some((format!("Weather cycle {}", if on { "on" } else { "off" }), 3.0));
        }
        // (only an installed weather file: the name comes from the admin's game)
        (Action::Weather(WeatherAction::Set(file)), _) if !file.contains("..") && file.to_ascii_lowercase().starts_with("weather/") && file.to_ascii_lowercase().ends_with(".owt") => {
            app.change_weather(Some(file), true, 1.0);
        }
        (Action::Weather(_), _) => app.next_weather(),
        // (a dedicated server's: a game's own is its Options' passengers setting)
        (Action::PassengerDensity(_), _) => app.service_msg = Some((omsi_ui::tr("The passenger density is set in Options here").into_owned(), 3.0)),
        (Action::Traffic(arg), _) if arg == "clear" => clear_ai_traffic(app),
        (Action::Traffic(_), _) => {
            if let Some(t) = app.session.traffic.as_mut() {
                t.target = crate::game_lists::next_step(&crate::game_lists::TRAFFIC, t.target);
                app.args.traffic = t.target;
                app.service_msg = Some((format!("Traffic: {}", t.target), 3.0));
            }
        }
        _ => {}
    }
}

/// Take the current random AI traffic off the road. Timetable buses are kept, and the
/// configured random traffic target will populate the roads again normally.
pub(crate) fn clear_ai_traffic(app: &mut App) {
    if app.net.lan.as_ref().is_some_and(|l| l.role == Role::Client) {
        app.service_msg = Some(("In a LAN session only the host can clear AI traffic".into(), 3.0));
        return;
    }
    if let (Some(t), Some(w), Some(r), Some(scene)) = (app.session.traffic.as_mut(), app.world.as_ref(), app.renderer.as_ref(), app.scene.as_mut()) {
        let removed = t.clear_random();
        crate::view_sync::apply_traffic(t, &mut app.gfx.sim_view.traffic, w, r, scene);
        app.service_msg = Some((format!("{removed} AI vehicles taken off the road"), 3.0));
    }
}

/// The own bus (or the walker) put beside another vehicle.
fn teleport_beside(app: &mut App, pos: glam::DVec3, heading: f64) {
    let (x, y) = beside(pos, heading, 8.0);
    teleport(app, glam::DVec3::new(x, y, pos.z), heading);
}

/// Our bus put at `at` facing `heading` (stopped), with the walker back at its wheel.
pub(crate) fn teleport(app: &mut App, at: glam::DVec3, heading: f64) {
    if app.session.on_foot.is_some() {
        app.back_to_bus();
    }
    // on the level at the height asked for (a car park under a building, a road under a
    // bridge), else the highest ground there (a place picked on the map, at no height)
    let ground = app.world.as_ref().and_then(|w| {
        let near = (at.z != 0.0)
            .then(|| w.data.peek().drive_probe(at.x, at.y, at.z + 1.5).below)
            .flatten()
            .filter(|b| (at.z - b).abs() < 3.0);
        near.or_else(|| w.walk_height(at.x, at.y))
    });
    let z = ground.unwrap_or(at.z);
    if let Some(p) = app.player.as_mut() {
        // (as a joining player's bus is moved off an occupied spawn: `lan::clear_spawn`)
        let origin = glam::DVec3::new(at.x, at.y, z);
        p.vehicle.position = origin;
        p.vehicle.heading = heading;
        if let Some(rb) = p.vehicle.rigid.as_mut() {
            rb.place(origin, heading);
        }
        for t in p.vehicle.trailers.iter_mut() {
            t.realign();
        }
        // (settled on the ground there; where its tiles are still to be read - a street far
        // off picked on the map - it waits at the street's height for them, as the frame
        // holds a bus with no ground under it, instead of dropping through first)
        if ground.is_some() {
            for _ in 0..3 {
                p.vehicle.update(1.0 / 30.0);
            }
        }
        log::info!("teleported to ({:.1}, {:.1}) heading {heading:.0}", at.x, at.y);
    }
}

/// What another game's command asks of ours (`omsi_lan::commands::Command`, read there),
/// carried out here: our bus, our world, our screen - and the answers the session expects.
pub(crate) fn carry_out(app: &mut App, from: u32, command: Command) {
    match command {
        // (host → us) put our bus there
        Command::Teleport { at, heading } => {
            teleport(app, at, heading);
            app.service_msg = Some(("The host brought you to them".into(), 4.0));
            app.service_event("teleport", "host", None);
        }
        // (host → us) the host's object editor: a map object moved, turned or deleted…
        Command::ObjectEdit { id, moved, turned, deleted } => {
            let e = crate::scene::ObjectEdit { moved, turned, deleted };
            let same = app.world.as_ref().and_then(|w| w.object_edits.lock().get(&id).copied()) == Some(e);
            if let (false, Some(w), Some(r), Some(scene)) = (same, app.world.clone(), app.renderer.as_ref(), app.scene.as_mut()) {
                log::info!("LAN: the host's editor moved object {id} by ({:.2}, {:.2}, {:.2}), turned {:.1}, deleted {}", e.moved.x, e.moved.y, e.moved.z, e.turned, e.deleted);
                w.apply_object_edit(r, scene, id, e);
            }
        }
        // … or a new object (a copy), where it stands now
        Command::ObjectAdd { id, at, heading, removed, file } => {
            if let (Some(w), Some(r), Some(scene)) = (app.world.clone(), app.renderer.as_ref(), app.scene.as_mut()) {
                if let Some(g) = app.menus.remote_added.remove(&id) {
                    w.remove_helper_object(r, scene, g);
                }
                if !removed {
                    let path = app.args.root.join(&file);
                    if let Some(g) = w.add_helper_object(r, scene, &path.to_string_lossy(), at, heading, &[]) {
                        app.menus.remote_added.insert(id, g);
                    }
                }
            }
        }
        // (host → us) a service for our bus, or our bus back on its wheels
        Command::Service(kind) => {
            app.run_service(&kind, "host");
            app.service_msg = Some((format!("The host: {kind}"), 3.0));
        }
        Command::Unstick => {
            if let Some(p) = app.player.as_ref() {
                let (at, heading) = (p.vehicle.position, p.vehicle.heading);
                teleport(app, at, heading);
                app.service_msg = Some(("The host put your bus back on its wheels".into(), 4.0));
                app.service_event("reset", "host", None);
            }
        }
        // (server → us) prove the password without sending it
        Command::AdminChallenge(challenge) => {
            let pw = app.net.remotes.admin_password.take();
            if let (Some(pw), Some(l)) = (pw, app.net.lan.as_mut()) {
                match answer(&challenge, &pw) {
                    Some(a) => l.command(1, &format!("auth {a}")),
                    None => log::warn!("LAN: the server's admin challenge is not one this game answers (an older or a foreign server)"),
                }
            }
        }
        // (host → us) a duty given by the server's dispatch: that trip of the tour (by its place
        // among the tour's trips, as the tour list counts them), from that stop (its number
        // among the trip's stations; a stop the trip does not serve: the next one it serves),
        // as the tour list's start button does: the bus stays where it is. The host hears
        // `duty-ok <line> <tour> <trip> <stop>` or `duty-no <why>`.
        Command::Duty(duty) => {
            let reply = match duty {
                None => "duty-no malformed".to_string(),
                Some((line, tour, trip, station)) => {
                    let stops = app.session.schedule.as_ref().map(|s| s.tour_trip_stops(&line, &tour, trip)).unwrap_or_default();
                    match stops.iter().position(|s| s.1 >= station).or(stops.len().checked_sub(1)) {
                        None => "duty-no unknown tour or trip".to_string(),
                        Some(chosen) => {
                            crate::game_lists::start_duty_at(app, &line, &tour, trip, chosen);
                            let ok = app.session.duty.as_ref().is_some_and(|d| d.line.eq_ignore_ascii_case(&line) && d.tour.eq_ignore_ascii_case(&tour));
                            log::info!("LAN: the server gave us line {line} tour {tour}, trip {trip} from stop {chosen}: {}", if ok { "taken" } else { "not taken" });
                            if ok {
                                format!("duty-ok {}", duty_arg(&line, &tour, trip, stops[chosen].1))
                            } else {
                                "duty-no no duty".to_string()
                            }
                        }
                    }
                }
            };
            if let Some(l) = app.net.lan.as_mut() {
                l.command(1, &reply);
            }
        }
        // (host → us) a notification over the navigator for a few seconds; the host hears that
        // it was shown (`notify-seen <id>`): a game that does not know `notify` stays silent,
        // and the host can say it in the chat instead
        Command::Notify(text) => {
            if let Some((id, n)) = crate::ui::Notice::parse(&text) {
                log::info!("LAN: the server's notice {id}: {}", n.text);
                crate::ui::push_notice(&mut app.menus.notices, n);
                if let Some(l) = app.net.lan.as_mut() {
                    l.command(1, &format!("notify-seen {id}"));
                }
            }
        }
        // (host → us) the server's dispatch takes the duty back: free drive, as the game menu's
        // "end the duty"; the host hears `duty-off-ok` (there was one) or `duty-off-none`
        Command::DutyOff => {
            let had = app.session.duty.take().map(|d| format!("{} {}", d.line, d.tour));
            // (the bus's own timetable with it, as "end the duty" clears it, #1317)
            if let Some(p) = app.player.as_mut() {
                crate::game_lists::clear_timetable(&mut p.vehicle);
            }
            log::info!("LAN: the server took our duty back ({})", had.as_deref().unwrap_or("we had none"));
            if had.is_some() {
                app.service_msg = Some(("The dispatch took the duty back: free drive".into(), 6.0));
            }
            if let Some(l) = app.net.lan.as_mut() {
                l.command(1, if had.is_some() { "duty-off-ok" } else { "duty-off-none" });
            }
        }
        Command::AdminLocked => app.service_msg = Some(("Too many wrong admin passwords: try again later".into(), 4.0)),
        // (server → us) the password was right: the menu is ours
        Command::AdminOk(role) => {
            app.net.is_admin = true;
            crate::admin_panel::take(app, &Command::AdminOk(role));
            app.service_msg = Some(("You administer this server now: Esc menu, Administration".into(), 6.0));
        }
        // (server → us) what its admin's menu shows
        c @ (Command::AdminResult { .. } | Command::AdminPlayer(_) | Command::AdminListEnd { .. } | Command::AdminState(_)) => crate::admin_panel::take(app, &c),
        Command::AdminNo => app.service_msg = Some(("Wrong admin password".into(), 4.0)),
        // (a game hosting by code sent a notice: the player's game showed it)
        Command::NotifySeen(id) => log::info!("LAN: player {from} saw notice {id}"),
        // (the session's own: `lan_command`)
        Command::Trigger(_) | Command::VoiceAsked | Command::Voice(_) => {}
    }
}

/// What of the game a dedicated server's administration goes by (`metar`: its weather
/// follows a METAR report).
pub(crate) fn server_rules(metar: bool) -> omsi_lan::admin::ServerRules<'static> {
    fn notice_ok(text: &str) -> bool {
        crate::ui::Notice::parse(text).is_some()
    }
    omsi_lan::admin::ServerRules { real_time: crate::real_time::server_real(), traffic_max: crate::game_lists::TRAFFIC_MAX, notice_ok: &notice_ok, metar }
}

/// The bus fallen through the world (a hole in the ground, a tile that was not there yet,
/// a mod's road without a surface) fell for ever. Where it last stood on the ground is
/// kept every second; a bus more than 8 m under the ground there is - or 40 m under where
/// it last stood, where there is no ground - is put back there.
pub(crate) fn guard_fall(app: &mut App, dt: f32) {
    let Some(p) = app.player.as_ref() else { return };
    let at = p.vehicle.position;
    // the ground under the bus (the face at or below it: on a car park's lower level the
    // building's roof is not its ground - measured from the roof, a bus driving under it
    // had fallen through the world and was put up there), and the highest there is
    let (under, ground) = match app.world.as_ref() {
        Some(w) => (w.data.peek().drive_probe(at.x, at.y, at.z + 1.5).below, w.walk_height(at.x, at.y)),
        None => (None, None),
    };
    app.session.safe_age += dt;
    let fallen = match (under, ground) {
        (Some(_), _) => false,
        (None, Some(g)) => at.z < g - 8.0,
        (None, None) => app.session.safe_pose.map(|s| at.z < s.0.z - 40.0).unwrap_or(false),
    };
    if fallen {
        if let Some((pos, heading)) = app.session.safe_pose {
            log::warn!("the bus fell through the world at ({:.1}, {:.1}, {:.1}): put back at ({:.1}, {:.1})", at.x, at.y, at.z, pos.x, pos.y);
            teleport(app, pos, heading);
            app.service_msg = Some(("The bus fell through the ground: it was put back where it last stood".into(), 5.0));
            app.service_event("teleport", "game", None);
        }
        return;
    }
    if app.session.safe_age >= 1.0 {
        if let Some(g) = under.filter(|g| (at.z - g).abs() < 2.5) {
            app.session.safe_age = 0.0;
            app.session.safe_pose = Some((glam::DVec3::new(at.x, at.y, g), p.vehicle.heading));
        }
    }
}

#[cfg(test)]
mod traffic_order_tests {
    use omsi_lan::admin::TrafficOrder;

    #[test]
    fn a_traffic_order_is_read() {
        let max = crate::game_lists::TRAFFIC_MAX;
        assert_eq!(TrafficOrder::parse("clear", max), Some(TrafficOrder::Clear));
        assert_eq!(TrafficOrder::parse(" 20 ", max), Some(TrafficOrder::Density(20)));
        assert_eq!(TrafficOrder::parse("0", max), Some(TrafficOrder::Density(0)));
        assert_eq!(TrafficOrder::parse("400", max), Some(TrafficOrder::Density(120)));
        assert_eq!(TrafficOrder::parse("120", max), Some(TrafficOrder::Density(crate::game_lists::TRAFFIC_MAX)));
        for bad in ["", "next", "-5", "2.5"] {
            assert_eq!(TrafficOrder::parse(bad, max), None, "{bad}");
        }
    }
}
