//! A dedicated server's configuration (`server.cfg`, written with the defaults and a comment
//! for every key when it is missing), its list of allowed buses, and its players as the web
//! gateway's `GET /players` lists them. The server run itself is the app's (`server.rs`).

use anyhow::{Context, Result};
use std::path::Path;

/// What `server.cfg` says (see `DEFAULT_CFG`).
#[derive(Debug, Clone)]
pub struct ServerCfg {
    pub name: String,
    pub motd: String,
    pub map: String,
    pub date: Option<String>,
    pub time: String,
    pub weather: Option<String>,
    pub traffic: usize,
    pub timetable: bool,
    pub passengers: bool,
    /// `passenger_density`: the factor on the map's own passenger demand (its hourly
    /// `[trafficdensity_passenger]` and each stop's own numbers). None: the key is not in the
    /// file (or not a valid number), and the server goes on using its machine's
    /// `settings.cfg` `pax_density`, as it always did.
    pub passenger_density: Option<f32>,
    pub port: u16,
    pub web_port: u16,
    pub max_players: usize,
    pub tunnel: bool,
    pub radius: i32,
    pub icon: Vec<u8>,
    /// Players who say `/admin <password>` in the chat administer the server (empty: nobody).
    pub admin_password: String,
    /// The same with this password gives the moderator's role: the players and the chat,
    /// not the world (empty: none; one equal to `admin_password` is the admin's).
    pub moderator_password: String,
    /// Who may reach the web panel and its API (`admin_web`: `local`, `remote`, `off`).
    pub admin_web: omsi_admin::WebMode,
    /// Pages of these origins may act besides the server's own (`admin_web_origins`).
    pub admin_web_origins: Vec<String>,
    /// The proxies whose word about a request's source and scheme is believed
    /// (`admin_trusted_proxies`; this machine by default).
    pub admin_trusted_proxies: Vec<std::net::IpAddr>,
    /// The header in which they name the client (`admin_proxy_header`; the last
    /// `X-Forwarded-For` by default).
    pub admin_proxy_header: omsi_admin::ProxyHeader,
    /// Players through Cloudflare's proxy and a proxy of this machine are told by
    /// `CF-Connecting-IP` (`gateway_proxy_header = cf-connecting-ip`), not the last
    /// `X-Forwarded-For`.
    pub gateway_cf_connecting_ip: bool,
    /// How fast the server's clock runs (1 real time).
    pub time_speed: f64,
    /// The clock follows the server machine's real date and time (the speed and the
    /// administration's clock are ignored then).
    pub real_time: bool,
    /// The weather follows an airport's METAR report, downloaded here; the players are told its
    /// values and need no sync of their own.
    pub metar_sync: bool,
    /// The airport of that report (ICAO; empty: the one nearest the map).
    pub metar_station: String,
    /// Only these buses may be driven on the server (vehicle files, empty: every bus the
    /// server has installed).
    pub vehicles: Vec<String>,
    /// `GET /players` on the web port tells who drives what and where (for a web map).
    pub share_positions: bool,
    /// A player whose bus the server lacks sends it, and the server passes it on to the
    /// others (`mods::upload`).
    pub accept_uploads: bool,
    /// Days a bus players sent is kept when nobody uses it (`mods::vehicles::collect`).
    pub uploads_keep_days: u64,
    /// The map (and what it uses) goes to players who join without it (`mods`); off,
    /// they install it themselves - the server's line is kept for the players' buses. A
    /// `server.cfg` without the key shares it, as servers did before the key came; the new
    /// file (`DEFAULT_CFG`) says 0.
    pub share_maps: bool,
    /// The GreenTeaSpeak server and channel the players talk in (`voice`).
    pub voice: Option<crate::voice::VoiceServer>,
}

pub const DEFAULT_CFG: &str = "\
# openOMSI dedicated server
# (key = value; lines starting with # are comments)

# shown in the players' server list and when they join
name = openOMSI server
motd = Welcome! Drive safely.

# the map (relative to the OMSI 2 folder), the start date (YYYY-MM-DD, empty: today),
# the time of day and the weather (a .owt of the OMSI 2 folder, empty: the map's default,
# cycle: one after another through the day, as the month allows)
map = maps/Berlin-Spandau/global.cfg
date =
time = 08:00
weather =

# the shared world: random traffic (cars), timetable buses, waiting passengers
traffic = 30
timetable = 1
passengers = 1

# how many people the map's stops and pavements get: a factor on the map's own demand for
# the time of day (1 = as the map says, 0.5 = half, 2 = twice; 0 to 3). 0 brings no new
# people but keeps the passengers on (passengers = 0 switches them off). Not the AI traffic.
# The admins can change it while the server runs; the people already there stay
passenger_density = 1

# UDP port of the session, TCP port of the web gateway (status, icon, WebSocket players)
port = 27015
web_port = 27025
max_players = 16

# start a free Cloudflare quick tunnel (needs cloudflared) and print its https address
tunnel = 1

# how many tiles round the map's first entry point are kept loaded (0: all of them)
radius = 0

# players who say \"/admin <password>\" in the chat get the Administration menu (Esc):
# send players away, bring them, the clock, its speed, the weather, the traffic and the
# passengers. The password needs at least 12 characters; empty: nobody
admin_password =
# the same with this password: a moderator, who sees the players and sends them away, talks
# to them, but leaves the clock, the weather, the traffic, the passengers and bringing
# players to their bus alone
# (at least 12 characters; empty: no moderators)
moderator_password =

# the web panel (http://127.0.0.1:<web_port>/admin/) and its API: local = from this machine
# only (not through the tunnel or a proxy); remote = also over HTTPS through the proxies in
# admin_trusted_proxies (this machine by default: with tunnel = 1 that is the public tunnel -
# then the passwords alone guard it); off = none. admin_web_origins: other https:// sites
# whose pages may use it, separated by ;
admin_web = local
admin_web_origins =
admin_trusted_proxies =
# the header in which those proxies name the client: x-forwarded-for (its last address: Caddy,
# nginx, Cloudflare), x-real-ip or cf-connecting-ip - only that one is believed
admin_proxy_header = x-forwarded-for
# the same for the players who come by WebSocket through a proxy of this machine:
# x-forwarded-for, or cf-connecting-ip with Cloudflare's proxy (the orange cloud) in front
gateway_proxy_header = x-forwarded-for

# how fast the clock runs (1 = real time, 2 = twice as fast, up to 30)
time_speed = 1

# the clock follows this machine's real date and time (1 = on; the time and date above are
# then only for the very first moment, and time_speed and the admin's clock are ignored)
real_time = 0

# the weather follows the real METAR report of an airport (1 = on; the weather above is then
# only for the first moment, and the admins cannot change it). The server downloads the report
# every ten minutes and tells the players its values, so they need no METAR sync of their own.
# metar_station is the airport's ICAO code, e.g. EDDB (empty: the one nearest the map)
metar_sync = 0
metar_station =

# the buses players may drive, separated by ; (vehicle files such as
# Vehicles/MAN_SD200/MAN_SD77.bus; empty: every bus installed on the server). The players'
# vehicle menu offers only these, and a player who drives another bus is sent away
vehicles =

# tell anyone who asks the web port (GET /players) the players' names, buses, lines and
# positions - for a live map of the server on a website; tell your players when it is on
share_positions = 0

# a player whose bus this server does not have sends it the files that bus uses (the variant
# and paint driven, not the whole pack); the server keeps them and passes them on to every
# other player, so nobody sees a stand-in. Taken without asking and kept after a restart
# (in ~/.openomsi/lan-store). With it on and vehicles empty, the players may choose any bus.
# 1 = on, 0 = off
accept_uploads = 1
# days a bus players sent is kept when nobody drives it; when the disk runs short, the buses
# nobody drives now go first, the least lately used first (no bus is refused for its size)
uploads_keep_days = 30
# a player who joins without the server's map gets it from the server (the map and the
# objects, splines and vehicles it uses - gigabytes for a big map). Off: players install the
# map themselves and are told which one is missing; the server's bandwidth goes to the
# players' buses. 1 = on, 0 = off
share_maps = 0

# voice chat: the players hear each other where they stand, through GreenTeaSpeak and its
# openOMSI plugin (as SaltyChat does for FiveM). voice_server_uid is the voice server's unique
# id (its info panel; needed: without it there is no voice chat), voice_channel the in-game
# channel's id or name (empty: no voice chat), voice_range how far a player is heard (m).
# voice_radio (1/0): drivers get a bindable map-wide bus radio on top of proximity voice.
# The channel and its password are sent to every player who joins
voice_server_uid =
voice_channel =
voice_channel_password =
voice_range = 20
voice_radio = 1
";

impl ServerCfg {
    /// The players' buses and the map as this config says (`mods`): whether the server takes
    /// its players' buses, how long it keeps the ones nobody drives, and whether it passes
    /// the map on to players who join without it.
    pub fn apply_to_mods(&self) {
        crate::mods::upload::accept(self.accept_uploads);
        crate::mods::vehicles::keep_for_days(self.uploads_keep_days);
        crate::mods::share_maps(self.share_maps);
    }

    /// `traffic_max`: the most AI vehicles the game allows (`traffic` is capped there).
    pub fn load(path: &Path, traffic_max: usize) -> Result<ServerCfg> {
        if !path.is_file() {
            std::fs::write(path, DEFAULT_CFG).with_context(|| format!("writing {}", path.display()))?;
            log::info!("server: {} did not exist; written with the defaults", path.display());
        }
        let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let mut kv: std::collections::HashMap<String, String> = Default::default();
        for line in text.lines() {
            let l = line.trim();
            if l.is_empty() || l.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = l.split_once('=') {
                kv.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
            }
        }
        let get = |k: &str, d: &str| kv.get(k).cloned().filter(|v| !v.is_empty()).unwrap_or_else(|| d.to_string());
        let flag = |k: &str, d: bool| kv.get(k).map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")).unwrap_or(d);
        let num = |k: &str, d: i64| kv.get(k).and_then(|v| v.parse::<i64>().ok()).unwrap_or(d);
        let dir = path.parent().unwrap_or(Path::new("."));
        let icon = std::fs::read(dir.join("server-icon.png")).ok().filter(|b| b.starts_with(b"\x89PNG") && b.len() < 256 * 1024).unwrap_or_default();
        Ok(remote_needs_long_passwords(ServerCfg {
            name: get("name", "openOMSI server"),
            motd: get("motd", ""),
            map: get("map", "maps/Berlin-Spandau/global.cfg"),
            date: kv.get("date").cloned().filter(|v| !v.is_empty()),
            time: get("time", "08:00"),
            weather: kv.get("weather").cloned().filter(|v| !v.is_empty()),
            traffic: (num("traffic", 30).max(0) as usize).min(traffic_max),
            timetable: flag("timetable", true),
            passengers: flag("passengers", true),
            passenger_density: kv.get("passenger_density").filter(|v| !v.is_empty()).and_then(|v| {
                passenger_density(v).map_err(|e| log::warn!("server.cfg: passenger_density = {v}: {e}; the machine's own passengers setting is used")).ok()
            }),
            port: num("port", 27015).clamp(1, 65535) as u16,
            web_port: num("web_port", 27025).clamp(1, 65535) as u16,
            max_players: num("max_players", 16).clamp(1, 64) as usize,
            tunnel: flag("tunnel", true),
            radius: num("radius", 0) as i32,
            icon,
            admin_password: admin_password(kv.get("admin_password").cloned().unwrap_or_default()),
            moderator_password: checked_password(kv.get("moderator_password").cloned().unwrap_or_default(), "moderator_password"),
            admin_web: web_mode(kv.get("admin_web").map(String::as_str)),
            admin_web_origins: kv.get("admin_web_origins").map(|v| v.split(';').map(|o| o.trim().trim_end_matches('/').to_string()).filter(|o| o.starts_with("https://") || o.starts_with("http://")).collect()).unwrap_or_default(),
            admin_trusted_proxies: trusted_proxies(kv.get("admin_trusted_proxies").map(String::as_str).unwrap_or("")),
            admin_proxy_header: proxy_header(kv.get("admin_proxy_header").map(String::as_str).unwrap_or("")),
            gateway_cf_connecting_ip: kv.get("gateway_proxy_header").is_some_and(|v| v.trim().eq_ignore_ascii_case("cf-connecting-ip")),
            time_speed: kv.get("time_speed").and_then(|v| v.parse::<f64>().ok()).filter(|v| v.is_finite()).unwrap_or(1.0).clamp(1.0, 30.0),
            real_time: flag("real_time", false),
            metar_sync: flag("metar_sync", false),
            metar_station: kv.get("metar_station").map(|v| v.chars().filter(|c| c.is_ascii_alphabetic()).take(4).collect::<String>().to_ascii_uppercase()).unwrap_or_default(),
            vehicles: kv.get("vehicles").map(|v| v.split(';').map(|x| x.trim().replace('\\', "/")).filter(|x| !x.is_empty()).collect()).unwrap_or_default(),
            share_positions: flag("share_positions", false),
            accept_uploads: flag("accept_uploads", true),
            uploads_keep_days: num("uploads_keep_days", 30).clamp(1, 3650) as u64,
            share_maps: flag("share_maps", true),
            voice: crate::voice::VoiceServer::from_kv(|k| kv.get(k).cloned()),
        }))
    }
}

/// `admin_web = remote` puts the login on the internet: its passwords need
/// `REMOTE_MIN_PASSWORD` characters, or the panel stays on this machine (and the log says so).
fn remote_needs_long_passwords(mut cfg: ServerCfg) -> ServerCfg {
    let short = |p: &str| !p.is_empty() && p.chars().count() < crate::admin::REMOTE_MIN_PASSWORD;
    if cfg.admin_web == omsi_admin::WebMode::Remote && (short(&cfg.admin_password) || short(&cfg.moderator_password)) {
        log::warn!("server.cfg: admin_web = remote needs passwords of {} characters at least: the web administration stays on this machine (local)", crate::admin::REMOTE_MIN_PASSWORD);
        cfg.admin_web = omsi_admin::WebMode::Local;
    }
    cfg
}

/// (one check for `server.cfg` and the admins: `omsi_admin::passenger_density`)
pub use omsi_admin::{passenger_density, PASSENGER_DENSITY_MAX};

/// The installed maps as server.cfg names them (`maps/<folder>/global.cfg`), sorted.
pub fn installed_maps() -> Vec<String> {
    let mut maps: Vec<String> = omsi_cfg::read_dir_merged("maps")
        .into_iter()
        .filter(|d| omsi_cfg::vfs::exists(&d.join("global.cfg")))
        .filter_map(|d| d.file_name().map(|n| format!("maps/{}/global.cfg", n.to_string_lossy())))
        .collect();
    maps.sort_by_key(|m| m.to_lowercase());
    maps
}

/// `admin_web`: none (a `server.cfg` from before the web panel) or a wrong word is off - an
/// update opens no door the operator did not ask for; a new server's file says `local`.
fn web_mode(v: Option<&str>) -> omsi_admin::WebMode {
    let Some(v) = v else { return omsi_admin::WebMode::Off };
    omsi_admin::WebMode::parse(v).unwrap_or_else(|| {
        log::warn!("server.cfg: admin_web = {v} is not local, remote or off: off");
        omsi_admin::WebMode::Off
    })
}

/// `admin_proxy_header`: a wrong word is the default (the last `X-Forwarded-For`).
fn proxy_header(v: &str) -> omsi_admin::ProxyHeader {
    omsi_admin::ProxyHeader::parse(v).unwrap_or_else(|| {
        log::warn!("server.cfg: admin_proxy_header = {v} is not x-forwarded-for, x-real-ip or cf-connecting-ip: x-forwarded-for");
        omsi_admin::ProxyHeader::ForwardedFor
    })
}

/// `admin_trusted_proxies`: addresses separated by `;` (empty: this machine).
fn trusted_proxies(v: &str) -> Vec<std::net::IpAddr> {
    let list: Vec<std::net::IpAddr> = v
        .split(';')
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .filter_map(|a| a.parse().map_err(|_| log::warn!("server.cfg: admin_trusted_proxies: '{a}' is not an address: left out")).ok())
        .collect();
    if list.is_empty() {
        omsi_admin::WebCfg::default().trusted_proxies
    } else {
        list
    }
}

fn admin_password(password: String) -> String {
    checked_password(password, "admin_password")
}

/// A password of `server.cfg` (`key`), or none when it is too short (and the log says so).
fn checked_password(password: String, key: &str) -> String {
    if !password.is_empty() && password.chars().count() < crate::admin::MIN_PASSWORD {
        log::warn!("server.cfg: {key} has fewer than {} characters: it lets nobody in until it is longer", crate::admin::MIN_PASSWORD);
        return String::new();
    }
    password
}

/// The buses a dedicated server allows (`vehicles`; empty: every bus it has).
pub static SERVER_VEHICLES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();

/// Whether a `vehicles` list allows the bus `file` (an empty list: every bus). A player's
/// game may name it with a folder before it (`D:/OMSI 2/Vehicles/…/….bus`).
pub fn allows(list: &[String], file: &str) -> bool {
    let norm = |s: &str| s.trim().replace('\\', "/").to_ascii_lowercase();
    let f = norm(file);
    list.is_empty() || list.iter().map(|v| norm(v)).any(|v| f == v || f.ends_with(&format!("/{v}")))
}

/// A server's `vehicles` list holds: a player who drives another bus (put down in the game's
/// own vehicle menu, which a game before this one did not limit to the list, #1183) is sent
/// away and told which buses the server has. It may join again with one of them.
pub fn enforce_vehicles(lan: &mut omsi_net::LanSession) {
    let Some(list) = SERVER_VEHICLES.get().filter(|l| !l.is_empty()) else { return };
    let out: Vec<(u32, String)> = lan.peers().filter(|p| p.has_info && !p.pose.bus.is_empty() && !allows(list, &p.pose.bus)).map(|p| (p.pose.id, p.pose.bus.clone())).collect();
    for (id, bus) in out {
        log::warn!("server: player {id} drives {bus}, which the vehicles list does not allow: sent away");
        let names: Vec<&str> = list.iter().map(|v| v.rsplit('/').next().unwrap_or(v)).collect();
        lan.kick(id, &format!("this server allows only these buses: {}", names.join(", ")), false);
    }
}

/// A player for `GET /players`: on foot - a walker in its state - where it walks, or with the
/// bus it sits in (`Walker::aboard`; the walker's own point lags behind that bus); otherwise
/// with the bus it drives. The walker comes first: a player who got out keeps its vehicle, so
/// such a state carries `FLAG_VEHICLE` (the parked bus) and the walker. `None`: no place known.
pub fn player_info<'a>(q: &omsi_net::Pose, pose_of: impl Fn(u32) -> Option<&'a omsi_net::Pose>) -> Option<omsi_net::ws::PlayerInfo> {
    let driving = q.walker.is_none() && q.has_vehicle();
    let (x, y, heading, speed_kmh, aboard) = match q.walker {
        Some(w) => match w.aboard.and_then(|a| pose_of(a.owner)).filter(|b| b.has_vehicle()) {
            Some(b) => (b.x, b.y, b.heading, b.speed_kmh, Some(b.id)),
            // (a walker's speed is in m/s)
            None => (w.x, w.y, w.heading, w.speed * 3.6, None),
        },
        None if driving => (q.x, q.y, q.heading, q.speed_kmh, None),
        None => return None,
    };
    Some(omsi_net::ws::PlayerInfo {
        id: q.id,
        name: q.name.clone(),
        // (what a player on foot last drove is not what it drives)
        bus: if driving { q.bus.clone() } else { String::new() },
        line: if driving { q.line.clone() } else { String::new() },
        destination: if driving { q.destination.clone() } else { String::new() },
        tour: q.tour.clone(),
        x,
        y,
        heading,
        speed_kmh,
        on_foot: !driving,
        aboard,
        lat_lon: omsi_map::world_to_lat_lon(x, y),
    })
}

#[cfg(test)]
mod passenger_density_tests {
    use super::ServerCfg;

    fn load(text: &str) -> ServerCfg {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("omsi-pax-cfg-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("server.cfg");
        std::fs::write(&path, text).unwrap();
        let cfg = ServerCfg::load(&path, 120).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        cfg
    }

    /// An old server.cfg (no key) keeps what it had: the machine's own setting; `passengers`
    /// stays the switch it was (`passengers = 50` is not fifty passengers).
    #[test]
    fn old_and_new_files_are_read() {
        let old = load("passengers = 1\n");
        assert!(old.passengers);
        assert_eq!(old.passenger_density, None);
        let new = load("passengers = 1\npassenger_density = 2.5\n");
        assert_eq!(new.passenger_density, Some(2.5));
        let off = load("passengers = 0\npassenger_density = 0\n");
        assert!(!off.passengers);
        assert_eq!(off.passenger_density, Some(0.0));
        // a wrong value: as if it were not there (and said in the log)
        for bad in ["-1", "10", "lots", "NaN"] {
            assert_eq!(load(&format!("passenger_density = {bad}\n")).passenger_density, None, "{bad}");
        }
        // the old trap: a number for the switch is not a density, and not on
        assert!(!load("passengers = 50\n").passengers);
    }

    /// The file written for a new server says what it means, and is read back as 1.
    #[test]
    fn the_default_file_has_the_density() {
        let d = load(super::DEFAULT_CFG);
        assert_eq!(d.passenger_density, Some(1.0));
        assert!(d.passengers);
        // and keeps the web administration on this machine
        assert_eq!(d.admin_web, omsi_admin::WebMode::Local);
        assert_eq!(d.admin_trusted_proxies, omsi_admin::WebCfg::default().trusted_proxies);
        assert_eq!(d.admin_proxy_header, omsi_admin::ProxyHeader::ForwardedFor);
    }

    /// The web administration's keys: a wrong word is never the open door.
    #[test]
    fn the_web_administration_keys() {
        // (a file from before the web panel: no panel after the update, until it says so)
        let old = load("admin_password = a long password\n");
        assert_eq!(old.admin_web, omsi_admin::WebMode::Off);
        assert_eq!(load("admin_web = lcoal\n").admin_web, omsi_admin::WebMode::Off);
        assert_eq!(load("admin_web = local\n").admin_web, omsi_admin::WebMode::Local);
        let r = load("admin_web = remote\nadmin_web_origins = https://admin.example.org/; javascript:x; http://lan.example\nadmin_trusted_proxies = 10.0.0.1; nonsense; ::1\n");
        assert_eq!(r.admin_web, omsi_admin::WebMode::Remote);
        assert_eq!(r.admin_web_origins, vec!["https://admin.example.org".to_string(), "http://lan.example".to_string()]);
        assert_eq!(r.admin_trusted_proxies, vec!["10.0.0.1".parse::<std::net::IpAddr>().unwrap(), "::1".parse().unwrap()]);
        assert_eq!(load("admin_web = public\n").admin_web, omsi_admin::WebMode::Off);
        assert_eq!(load("admin_web = off\n").admin_web, omsi_admin::WebMode::Off);
        assert_eq!(load("admin_proxy_header = X-Real-IP\n").admin_proxy_header, omsi_admin::ProxyHeader::RealIp);
        assert_eq!(load("admin_proxy_header = cf-connecting-ip\n").admin_proxy_header, omsi_admin::ProxyHeader::CfConnectingIp);
        assert_eq!(load("admin_proxy_header = forwarded\n").admin_proxy_header, omsi_admin::ProxyHeader::ForwardedFor);
        assert!(!load("").gateway_cf_connecting_ip);
        assert!(load("gateway_proxy_header = CF-Connecting-IP\n").gateway_cf_connecting_ip);
        assert!(!load("gateway_proxy_header = x-forwarded-for\n").gateway_cf_connecting_ip);
        // remote with a password too short for the internet: the panel stays on this machine
        let short = load("admin_web = remote\nadmin_password = twelve chars\n");
        assert_eq!((short.admin_password.as_str(), short.admin_web), ("twelve chars", omsi_admin::WebMode::Local));
        let long = load("admin_web = remote\nadmin_password = sixteen chars ok\nmoderator_password = and this one too\n");
        assert_eq!(long.admin_web, omsi_admin::WebMode::Remote);
        let short_mod = load("admin_web = remote\nadmin_password = sixteen chars ok\nmoderator_password = twelve chars\n");
        assert_eq!(short_mod.admin_web, omsi_admin::WebMode::Local);
    }
}

#[cfg(test)]
mod vehicles_tests {
    use super::{admin_password, allows};

    #[test]
    fn a_short_admin_password_leaves_the_administration_off() {
        assert_eq!(admin_password("1234".into()), "");
        assert_eq!(admin_password(String::new()), "");
        // (12 at least: the game's login can be overheard and guessed offline)
        assert_eq!(admin_password("eight888".into()), "");
        assert_eq!(admin_password("twelve chars".into()), "twelve chars");
    }

    #[test]
    fn the_list_allows_its_buses_only() {
        let list = vec!["Vehicles/MAN_SD200/MAN_SD77.bus".to_string(), "Vehicles/MAN_SD202/MAN_D92.bus".to_string()];
        assert!(allows(&list, "Vehicles/MAN_SD200/MAN_SD77.bus"));
        // another case, backslashes, a folder before it
        assert!(allows(&list, "vehicles\\man_sd200\\MAN_SD77.bus"));
        assert!(allows(&list, "D:/OMSI 2/Vehicles/MAN_SD202/MAN_D92.bus"));
        // the issue's: another bus of the same folder, a mod bus
        assert!(!allows(&list, "Vehicles/MAN_SD200/MAN_SD83.bus"));
        assert!(!allows(&list, "Vehicles/Some_Mod/Bus.bus"));
        // (a file whose name only ends like a listed one)
        assert!(!allows(&list, "Vehicles/MAN_SD200/XMAN_SD77.bus"));
        // no list: every bus
        assert!(allows(&[], "Vehicles/Some_Mod/Bus.bus"));
    }
}

#[cfg(test)]
mod cfg_tests {
    use super::{ServerCfg, DEFAULT_CFG};

    fn load(text: &str) -> ServerCfg {
        let dir = std::env::temp_dir().join(format!("omsi-server-cfg-{}-{}", std::process::id(), text.len()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("server.cfg");
        std::fs::write(&path, text).unwrap();
        let cfg = ServerCfg::load(&path, 100).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        cfg
    }

    /// A server that upgrades with its old `server.cfg` (no `share_maps`) goes on passing its
    /// map to players who join without it; only the file written new leaves it to them.
    #[test]
    fn an_old_server_cfg_still_shares_the_map() {
        assert!(load("name = old server\nmap = maps/Grundorf/global.cfg\n").share_maps);
        assert!(!load(DEFAULT_CFG).share_maps);
        assert!(!load("share_maps = 0\n").share_maps);
    }
}

#[cfg(test)]
mod players_tests {
    use super::player_info;
    use omsi_net::{Aboard, Pose, Walker, FLAG_VEHICLE};

    fn bus(id: u32, x: f64, y: f64) -> Pose {
        Pose { id, name: format!("p{id}"), bus: "Vehicles/MAN_SD200/MAN_SD77.bus".into(), line: "37".into(), x, y, heading: 90.0, speed_kmh: 40.0, flags: FLAG_VEHICLE, ..Default::default() }
    }

    #[test]
    fn a_player_is_where_it_is() {
        let none = |_| None;
        // driving: the bus
        let d = player_info(&bus(2, 100.0, 200.0), none).unwrap();
        assert_eq!((d.x, d.y, d.speed_kmh, d.on_foot, d.aboard, d.line.as_str()), (100.0, 200.0, 40.0, false, None, "37"));
        // on foot: the walker, not the zeroed vehicle fields
        let mut w = Pose { id: 3, name: "p3".into(), bus: "Vehicles/MAN_SD200/MAN_SD77.bus".into(), ..Default::default() };
        w.walker = Some(Walker { x: 10.0, y: 20.0, heading: 45.0, speed: 1.5, ..Default::default() });
        let f = player_info(&w, none).unwrap();
        assert_eq!((f.x, f.y, f.heading, f.on_foot, f.bus.as_str()), (10.0, 20.0, 45.0, true, ""));
        assert!((f.speed_kmh - 5.4).abs() < 1e-4);
        // aboard player 2's bus: placed with it
        let driver = bus(2, 100.0, 200.0);
        w.walker = Some(Walker { x: 90.0, y: 190.0, aboard: Some(Aboard { owner: 2, ..Default::default() }), ..Default::default() });
        let a = player_info(&w, |id| (id == 2).then_some(&driver)).unwrap();
        assert_eq!((a.x, a.y, a.speed_kmh, a.on_foot, a.aboard), (100.0, 200.0, 40.0, true, Some(2)));
        // aboard a bus that is gone: its own point
        let g = player_info(&w, none).unwrap();
        assert_eq!((g.x, g.y, g.aboard), (90.0, 190.0, None));
        // got out of its own bus: the state keeps FLAG_VEHICLE and the parked bus's place, and
        // has the walker - the walker wins
        let mut out = bus(5, 100.0, 200.0);
        out.walker = Some(Walker { x: 104.0, y: 197.0, heading: 180.0, speed: 1.0, ..Default::default() });
        assert!(out.has_vehicle());
        let o = player_info(&out, none).unwrap();
        assert_eq!((o.x, o.y, o.heading, o.on_foot, o.aboard, o.bus.as_str(), o.line.as_str()), (104.0, 197.0, 180.0, true, None, "", ""));
        assert!((o.speed_kmh - 3.6).abs() < 1e-4);
        // and sitting in another player's bus after getting out of its own: with that bus
        out.walker = Some(Walker { x: 1.0, y: 1.0, aboard: Some(Aboard { owner: 2, ..Default::default() }), ..Default::default() });
        let oa = player_info(&out, |id| (id == 2).then_some(&driver)).unwrap();
        assert_eq!((oa.x, oa.y, oa.on_foot, oa.aboard), (100.0, 200.0, true, Some(2)));
        // neither a bus nor a walker: not listed
        assert!(player_info(&Pose { id: 4, ..Default::default() }, none).is_none());
    }
}
