//! Starting a LAN session and adopting its shared world.

use super::*;

pub(super) fn date_of(clock: &omsi_sim::SimClock) -> String {
    let (d, m) = clock.day_month();
    format!("{:04}-{m:02}-{d:02}", clock.year)
}

/// The name the other players see: `--lan-name` (the launcher passes the driver profile's
/// name), else the personnel file's name, else the computer account's - never a name to
/// type in, and never the bare "Driver" everybody would share.
pub fn player_name(args: &LanConfig) -> String {
    let given = args.lan_name.trim();
    if !given.is_empty() && given != "Driver" {
        return given.to_string();
    }
    if let Some(stem) = args
        .driver
        .as_deref()
        .and_then(|d| Path::new(d).file_stem())
        .map(|s| s.to_string_lossy().to_string())
    {
        if !stem.trim().is_empty() && stem != "Driver" {
            return stem;
        }
    }
    // the account's full name on macOS, else its login name
    #[cfg(target_os = "macos")]
    if let Ok(out) = std::process::Command::new("id").arg("-F").output() {
        let full = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !full.is_empty() {
            return full;
        }
    }
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .ok()
        .filter(|u| !u.trim().is_empty())
        .unwrap_or_else(|| "Driver".into())
}

/// Our world as the other side sees it.
pub fn world_info(args: &LanConfig) -> omsi_net::WorldInfo {
    let clock = args.start_clock();
    omsi_net::WorldInfo {
        map: args.map.replace('\\', "/"),
        date: date_of(&clock),
        time: clock.time,
        weather: args.weather.clone().unwrap_or_default().replace('\\', "/"),
        season: args.season.clone().unwrap_or_default(),
    }
}

use crate::gateway::{ask_server_offers, ws_join_target};

/// Host or join as the command line says (`--lan-host`, `--lan-join`). A session that
/// cannot be started is reported and the game runs on alone. A joining player waits
/// briefly for the host's welcome, so that its map is loaded with the host's world.
pub fn start(args: &LanConfig, host: &dyn LanHost) -> Option<LanSession> {
    // (every hello of this process carries the machine's hardware identifier, see `bans`)
    crate::bans::announce_local_hardware_id();
    // (every bus offered until the server joined now says otherwise, see `ServerOffers`)
    let offers_session = crate::gateway::new_offers_session();
    let world = world_info(args);
    let started = Instant::now();
    let planned = Pose {
        bus: args.bus.clone().unwrap_or_default().replace('\\', "/"),
        ..Default::default()
    };
    let session = match (&args.lan_host, &args.lan_join) {
        (Some(port), _) => {
            // 0 = the default port, or the next free one when another session runs here
            let (p, try_next) = if *port == 0 {
                (omsi_net::DEFAULT_PORT, true)
            } else {
                (*port, false)
            };
            match LanSession::host(p, &player_name(args), world, try_next) {
                Ok(mut s) => {
                    if args.server {
                        s.tell_strangers_the_session();
                    } else {
                        // (a game hosting by code: this session's bans last as long as it runs;
                        // the owner's list is the account's)
                        crate::bans::install(crate::bans::BanStore::open(
                            None,
                            crate::bans::BanStore::default_owner_path(),
                        ));
                    }
                    Some(s)
                }
                Err(e) => {
                    log::warn!("LAN: cannot host on port {p}: {e}");
                    write_failure(&format!("cannot host on port {p}: {e}"));
                    None
                }
            }
        }
        (None, Some(target)) => {
            // a server's address (https://…, a trycloudflare name): over a WebSocket
            let direct = match omsi_net::ws::ws_url(target) {
                Some(url) => match ws_join_target(&url) {
                    Ok(local) => {
                        ask_server_offers(target, offers_session);
                        local
                    }
                    Err(e) => {
                        log::warn!("LAN: cannot reach '{target}': {e}");
                        write_failure(&format!("cannot reach '{target}': {e}"));
                        return None;
                    }
                },
                // (an address over UDP: a server there answers at its web port too; a code
                // or a search has nobody to ask)
                None => {
                    ask_server_offers(target, offers_session);
                    target.clone()
                }
            };
            match crate::join::connect(
                target,
                &direct,
                args.lan_ws_fallback.as_deref(),
                &player_name(args),
                world,
                &planned,
                HOST_ANSWER_WAIT,
            ) {
                Ok(s) => Some(s),
                Err(e) => {
                    log::warn!("LAN: cannot join '{target}': {e}");
                    write_failure(&format!("cannot join '{target}': {e}"));
                    None
                }
            }
        }
        _ => None,
    };
    let mut session = session?;
    if let Some(c) = session.code() {
        log::info!(
            "LAN: other players join with the code {}  (or by an address: {})",
            c.encode(),
            host_addresses(c.port)
                .iter()
                .map(|(label, addr, _)| format!("{label} {addr}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        for a in omsi_net::addrs::local_addresses() {
            log::info!(
                "LAN: address {} on {} ({}{})",
                a.ip,
                a.interface,
                a.label(),
                if a.kind.reachable() {
                    ""
                } else {
                    ", not offered: nobody else reaches it"
                }
            );
        }
    }
    if session.role == Role::Client {
        let t0 = started;
        // nobody answered at the addresses of the code: the host's tunnel, if it posted one
        if session.welcome.is_none()
            && session.rejected.is_none()
            && args
                .lan_join
                .as_deref()
                .map(|t| omsi_net::ws::ws_url(t).is_none())
                .unwrap_or(false)
        {
            if let Some(url) = omsi_net::SessionCode::decode(args.lan_join.as_deref().unwrap_or(""))
                .ok()
                .and_then(|c| omsi_net::bridge::lookup_tunnel(c.session))
            {
                log::info!(
                    "LAN: no answer at the code's addresses; through the host's tunnel {url}"
                );
                if let Some(ws) = omsi_net::ws::ws_url(&url) {
                    match ws_join_target(&ws).and_then(|local| {
                        LanSession::join(
                            &local,
                            &player_name(args),
                            world_info(args),
                            Duration::from_secs(3),
                        )
                    }) {
                        Ok(mut s2) => {
                            let t1 = Instant::now();
                            while t1.elapsed() < WELCOME_WAIT * 2
                                && !s2.connected
                                && s2.rejected.is_none()
                            {
                                s2.tick(0.02, &planned);
                                std::thread::sleep(Duration::from_millis(20));
                            }
                            session = s2;
                        }
                        Err(e) => log::warn!("LAN: the tunnel {url} did not answer: {e}"),
                    }
                }
            }
        }
        match (&session.welcome, &session.rejected) {
            (Some(_), _) => log::info!("LAN: welcome after {:.2} s", t0.elapsed().as_secs_f32()),
            (None, Some(why)) => log::warn!("LAN: turned away: {why}"),
            (None, None) => log::info!(
                "LAN: no welcome within {:.0} s; the host's world is taken over when it comes",
                HOST_ANSWER_WAIT.as_secs_f32()
            ),
        }
    }
    if session.role == Role::Host && !args.server {
        // the way in over the internet that always works (see `open_public_gateway`)
        let info = omsi_net::ws::ServerInfo {
            name: format!("{}'s game", player_name(args)),
            map: args.map.clone(),
            max_players: 64,
            version: host.version(),
            ..Default::default()
        };
        open_public_gateway(&session, info, 0, true);
        host.publish_vehicles(Vec::new());
    }
    write_status(&session, &Default::default(), None);
    Some(session)
}

/// A joining game the server sent or turned away: the reason, once (the game then ends and the
/// launcher shows "Disconnected from the server" with it). None for a host, or while it may play.
pub fn turned_away(lan: &LanSession) -> Option<String> {
    static SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let why = lan
        .turned_away
        .clone()
        .filter(|_| lan.role == Role::Client)?;
    if SAID.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return None;
    }
    log::warn!("{LEFT_SERVER}{why}");
    Some(why)
}

/// The host's mods (see `lan_mods`): the host serves them on its session's port number
/// (TCP), a joining player fetches what it lacks before its world is made, and says how
/// far it has got in the status the launcher shows.
/// What of our command line decides the mods we share as the host.
fn mod_source(args: &LanConfig) -> crate::mods::ModSource {
    crate::mods::ModSource {
        root: args.root.clone(),
        map: args.map.clone(),
        bus: args.bus.clone(),
        weather: args.weather.clone(),
    }
}

pub fn share_mods(args: &mut LanConfig, lan: &mut LanSession) -> Option<crate::mods::Server> {
    match lan.role {
        Role::Host => {
            // (a player's game hosting a session takes the joining players' buses as a server
            // does, and keeps its own for them: `mods::upload::keep_own`; a server takes them
            // as its server.cfg says)
            if !args.server {
                crate::mods::upload::accept(crate::mods::upload::a_hosting_game_takes_uploads());
            }
            if let (Some(listener), Some(key)) = (lan.take_mods_listener(), lan.host_key()) {
                return crate::mods::serve(listener, key, &mod_source(args));
            }
        }
        Role::Client => {
            // (a joining player notes what it reads: what its bus uses, `mods::upload`)
            omsi_cfg::vfs::record_reads(true);
            local_fleet::wait_for_welcome(args, lan);
            let Some(mut host) = lan.host.filter(|_| lan.welcome.is_some()) else {
                log::info!("LAN mods: no host to ask yet (its mods are not fetched)");
                if local_fleet::needs_fleet_bus(args) && lan.rejected.is_none() {
                    args.mod_fetch_error = Some("the server did not let this game in, so its bus could not be asked for: it may be busy admitting other players, try again in a minute".into());
                }
                return None;
            };
            let note = |lan: &mut LanSession, text: String| {
                lan.warnings.retain(|w| !w.starts_with("Host's mods"));
                lan.warnings.push(text);
                write_status(lan, &Default::default(), None);
            };
            note(lan, "Host's mods: looking what is needed...".into());
            let session = lan.session;
            let local_fleet = local_fleet::verified(args, lan);
            if local_fleet {
                log::info!(
                    "LAN: installed fleet matches the announced content; no bus download needed"
                );
            }
            let needs_selected_download = local_fleet::requires_server_download(
                args.allow_server_fleet,
                args.bus.is_some(),
                local_fleet,
            );
            let server_only = needs_selected_download
                && !args
                    .bus
                    .as_deref()
                    .is_some_and(|b| local_fleet::installed(&args.root, b));
            // the host's TCP port is not always reachable (a router forwards the UDP session
            // only, or we came in over a WebSocket): the files go through its tunnel then
            let joined_url = crate::gateway::joined_url();
            let direct = joined_url.is_none()
                && std::net::TcpStream::connect_timeout(&host, Duration::from_secs(3)).is_ok();
            if !direct {
                let base = joined_url
                    .or_else(|| {
                        omsi_net::bridge::lookup_tunnel(session)
                            .and_then(|u| omsi_net::ws::ws_url(&u))
                    })
                    // (joined by code or address, straight over UDP: the host's own web port,
                    // whose WebSockets carry the mods' TCP too, `crate::gateway::host_web_gateway`)
                    .or_else(|| crate::gateway::host_web_gateway(host));
                match base.map(|b| format!("{}/tcp", b.strip_suffix("/ws").unwrap_or(&b))) {
                    Some(tcp_url) => match omsi_net::ws::tcp_forward(&tcp_url) {
                        Ok(local) => {
                            log::info!(
                                "LAN mods: the host's TCP port is out of reach; through {tcp_url}"
                            );
                            host = local;
                        }
                        Err(e) => log::warn!("LAN mods: {e}"),
                    },
                    None => {
                        let why = "no reachable TCP mod channel or WebSocket relay";
                        note(lan, format!("Host's mods could not be downloaded: {why}"));
                        if server_only {
                            args.mod_fetch_error = Some(why.into());
                        }
                        return None;
                    }
                }
            }
            // (in the background: the session is kept alive meanwhile - a join held for
            // minutes was dropped by the host and came in later without the mods)
            let (tx, rx) = std::sync::mpsc::channel::<String>();
            let mut fmap = args.map.clone();
            let froot = args.root.clone();
            // The old map-fetch path remains available, but selected bus
            // downloads require an explicit opt-in from the launcher or CLI.
            let selected_bus = args.bus.clone().filter(|_| needs_selected_download);
            let selected_paint = args.paint.clone();
            let selected_requested = selected_bus.is_some();
            let worker = std::thread::spawn(move || {
                let mut last = Instant::now() - Duration::from_secs(1);
                let r = crate::mods::fetch_selected(
                    &froot,
                    &mut fmap,
                    host,
                    session,
                    selected_bus.as_deref(),
                    selected_paint.as_deref(),
                    &mut |done, total, file| {
                        if last.elapsed() > Duration::from_millis(500) {
                            last = Instant::now();
                            let pct = if total > 0 { done * 100 / total } else { 100 };
                            let _ = tx.send(format!(
                                "Host's mods: {pct}% ({:.0} of {:.0} MB) {file}",
                                done as f64 / 1e6,
                                total as f64 / 1e6
                            ));
                        }
                    },
                );
                (r, fmap)
            });
            let planned = Pose {
                bus: args.bus.clone().unwrap_or_default().replace('\\', "/"),
                ..Default::default()
            };
            while !worker.is_finished() {
                lan.keepalive(0.05, &planned);
                if let Some(line) = rx.try_iter().last() {
                    log::info!("LAN mods: {line}");
                    note(lan, line);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            let (result, map) = worker
                .join()
                .unwrap_or_else(|_| (Err("the download stopped".into()), args.map.clone()));
            args.map = map;
            match result {
                Ok(r) => {
                    let text = if r.fetched > 0 {
                        format!(
                            "Host's mods: {} files ({:.0} MB) fetched (kept for the next join)",
                            r.fetched,
                            r.bytes as f64 / 1e6
                        )
                    } else {
                        "Host's mods: everything the host uses is installed here".to_string()
                    };
                    note(lan, text);
                    let mine = world_info(args);
                    lan.set_world(mine);
                }
                Err(e) => {
                    log::warn!("LAN mods: {e}");
                    note(lan, format!("Host's mods could not be fetched: {e}"));
                    if selected_requested && server_only {
                        args.mod_fetch_error = Some(e);
                    }
                }
            }
        }
    }
    None
}

/// Do `work` (loading a world, which may take minutes on a big map) while the session is
/// kept on another thread: a host answers the players who join meanwhile, a joining game
/// tells its host that it is still there. A dedicated server loads the whole map before its
/// first frame, and nobody could join it until it was done.
pub fn answering_while<T>(
    lan: &mut Option<LanSession>,
    bus: Option<&str>,
    work: impl FnOnce() -> T,
) -> T {
    let Some(session) = lan.take() else {
        return work();
    };
    let planned = Pose {
        bus: bus.unwrap_or_default().replace('\\', "/"),
        ..Default::default()
    };
    let stop = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|s| {
        let keeper = s.spawn(|| {
            let mut session = session;
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                session.keepalive(0.05, &planned);
                std::thread::sleep(Duration::from_millis(50));
            }
            session
        });
        let out = work();
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        match keeper.join() {
            Ok(session) => *lan = Some(session),
            Err(_) => log::warn!("LAN: the session stopped while the world loaded"),
        }
        out
    })
}

/// A joining player plays on the host's map when it is installed here, whatever map was
/// chosen before joining. The host's list of mods also brings it, but that list may not
/// come in time: listing a big add-on map (thousands of objects to find and hash) took the
/// host longer than the joining game waited, and the player was left on its own map, where
/// nobody ever met them.
pub fn take_host_map(args: &mut LanConfig, lan: &mut LanSession) {
    if lan.role != Role::Client {
        return;
    }
    let Some(theirs) = lan
        .welcome
        .as_ref()
        .map(|w| w.world.map.trim().replace('\\', "/"))
    else {
        return;
    };
    // (the same map, however its path was written: from its `maps/` folder on - a map
    // chosen as a whole path, or out of an archive, is still the host's one, and taking it
    // for another map dropped the line and tour chosen: everybody drove without a duty)
    let norm = |s: &str| {
        let s = s.trim().replace('\\', "/").to_ascii_lowercase();
        match s.rfind("maps/") {
            Some(k) => s[k..].to_string(),
            None => s,
        }
    };
    if theirs.is_empty() || norm(&theirs) == norm(&args.map) {
        return;
    }
    if crate::mods::refuse_path(&theirs).is_some() || !norm(&theirs).starts_with("maps/") {
        return;
    }
    match omsi_cfg::find_in_roots(&theirs) {
        Some(_) => {
            log::info!(
                "LAN: the session is on the host's map {theirs} (not {})",
                args.map
            );
            args.map = theirs;
            // (the entry point, depot file and tour chosen were those of the other map)
            args.entry = 0;
            args.spawn = None;
            args.hof = None;
            args.line = None;
            args.tour = None;
            args.trip = None;
            let mine = world_info(args);
            lan.set_world(mine);
        }
        None => {
            let line = format!(
                "the host plays on {theirs}, which is not installed here - install that map to meet the others"
            );
            log::warn!("LAN: {line}");
            if !lan.warnings.contains(&line) {
                lan.warnings.push(line);
            }
        }
    }
}

/// A weather file the host uses, if this machine has it (None: the map's default).
pub fn host_weather(
    args: &LanConfig,
    host: &dyn LanHost,
    weather: &str,
) -> Result<Option<String>, String> {
    let w = weather.trim();
    if w.is_empty() {
        return Ok(None);
    }
    // the natural model and the cycle are made on each machine: no file to have
    if host.weather_without_file(w) {
        return Ok(Some(w.to_string()));
    }
    // a METAR report's values: made into a weather here, no file and no sync of our own
    if let Some(readable) = host.weather_report(w) {
        return if readable {
            Ok(Some(w.to_string()))
        } else {
            Err(format!("the host's weather {w} cannot be read here"))
        };
    }
    let path = omsi_cfg::resolve_path(&args.root, w);
    let inside = !w.contains("..") && !w.starts_with('/') && !w.contains(':');
    if inside && omsi_cfg::vfs::is_file(&path) {
        Ok(Some(w.to_string()))
    } else {
        Err(format!("the host's weather {w} is not installed here"))
    }
}

/// Whether the host's weather `host` (from its clock message) is news to a client whose own
/// is `local` and that last took up `seen`. Empty is a weather too: the map's default.
pub(super) fn runtime_weather_changed(host: &str, local: Option<&str>, seen: Option<&str>) -> bool {
    let norm = |s: &str| s.trim().replace('\\', "/").to_ascii_lowercase();
    let host_norm = norm(host);
    host_norm != norm(local.unwrap_or("")) && seen.is_none_or(|last| norm(last) != host_norm)
}

/// Take the host's world (a joining player, before the map is loaded): the date, the time
/// of day as the host's clock has it now, the weather and the season go into the
/// arguments the world is made from. The map stays ours (a different one is warned about).
pub fn adopt_host_world(
    args: &mut LanConfig,
    host: &dyn LanHost,
    lan: &mut LanSession,
    game: &mut LanGame,
) {
    if lan.role != Role::Client {
        return;
    }
    let Some(w) = lan.welcome.clone() else { return };
    game.adopted = lan.welcomes;
    let hw = &w.world;
    if let Some(c) = host_clock_now(&omsi_net::HostClock {
        world: hw.clone(),
        at: w.at,
        speed: lan.clock_speed,
    }) {
        args.date = Some(date_of(&c));
        args.day_of_year = None;
        args.time = format!(
            "{:02}:{:02}:{:05.2}",
            (c.time / 3600.0) as i32,
            ((c.time % 3600.0) / 60.0) as i32,
            c.time % 60.0
        );
    }
    let mut warnings = Vec::new();
    match host_weather(args, host, &hw.weather) {
        Ok(wt) => args.weather = wt,
        Err(e) => warnings.push(e),
    }
    args.season = (!hw.season.trim().is_empty()).then(|| hw.season.trim().to_string());
    log::info!(
        "LAN: taking the host's world: {} {} weather {} season {}",
        args.date.as_deref().unwrap_or(""),
        args.time,
        args.weather.as_deref().unwrap_or("(the map's)"),
        args.season.as_deref().unwrap_or("(by date)")
    );
    let mut mine = world_info(args);
    if warnings.is_empty() {
        // compared as the host spelled it
        mine.weather = hw.weather.clone();
    }
    lan.set_world(mine);
    for line in warnings {
        log::warn!("LAN: {line}");
        lan.warnings.push(line);
    }
}

/// The host's world after the map was loaded (a late welcome, a reconnect): the clock is
/// set and the weather changed; the season needs the map loaded again, which is said.
pub(super) fn adopt_at_runtime(
    args: &LanConfig,
    host: &dyn LanHost,
    lan: &mut LanSession,
    game: &mut LanGame,
    clock: &omsi_sim::SimClock,
) -> Vec<WorldUpdate> {
    let mut out = Vec::new();
    let Some(w) = lan.welcome.clone() else {
        return out;
    };
    game.adopted = lan.welcomes;
    let hw = w.world.clone();
    if let Some(c) = host_clock_now(&omsi_net::HostClock {
        world: hw.clone(),
        at: w.at,
        speed: lan.clock_speed,
    }) {
        log::info!(
            "LAN: the host's clock: {} {:02}:{:02} (ours was {} {:02}:{:02})",
            date_of(&c),
            (c.time / 3600.0) as i32,
            ((c.time % 3600.0) / 60.0) as i32,
            date_of(clock),
            (clock.time / 3600.0) as i32,
            ((clock.time % 3600.0) / 60.0) as i32
        );
        out.push(WorldUpdate::Clock {
            year: c.year,
            day_of_year: c.day_of_year,
            time: c.time,
        });
    }
    let mut warnings = Vec::new();
    let mut weather = args.weather.clone();
    match host_weather(args, host, &hw.weather) {
        Ok(wt) => {
            let norm = |s: &Option<String>| {
                s.as_deref()
                    .unwrap_or("")
                    .trim()
                    .replace('\\', "/")
                    .to_ascii_lowercase()
            };
            if norm(&wt) != norm(&args.weather) {
                out.push(WorldUpdate::Weather(wt.clone()));
            }
            weather = wt;
        }
        Err(e) => warnings.push(e),
    }
    // the season is in the textures that are loaded: say so when the host's differs
    let mut theirs = args.clone();
    theirs.date = Some(hw.date.clone());
    theirs.day_of_year = None;
    theirs.season = (!hw.season.trim().is_empty()).then(|| hw.season.trim().to_string());
    theirs.weather = weather.clone();
    if let Some((want, loaded)) = host.season_folders(&theirs) {
        if want != loaded {
            warnings.push(format!("the host's season ({}) differs from the one loaded here ({}) - start the game again to see it", want.as_deref().unwrap_or("summer"), loaded.as_deref().unwrap_or("summer")));
        }
    }
    let mut mine = world_info(args);
    mine.weather = if warnings.iter().any(|w| w.contains("weather")) {
        mine.weather
    } else {
        hw.weather.clone()
    };
    lan.set_world(mine);
    for line in warnings {
        log::warn!("LAN: {line}");
        lan.warnings.push(line);
    }
    out
}
