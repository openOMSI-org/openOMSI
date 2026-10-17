//! The ways into a session besides UDP and what they show: the host's WebSocket gateway and
//! its Cloudflare tunnel (with the official server's announcements), a joining game's
//! WebSocket, the gateway's status page a dedicated server keeps up to date, and which buses
//! the server we joined offers. Kept for the whole session in process-wide state.

use omsi_net::LanSession;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The ways into a session besides UDP (see `omsi_net::ws`): the host's WebSocket gateway
/// and its Cloudflare tunnel, or a joining game's WebSocket to a server or a tunnel. Kept
/// for the whole session.
struct WsPath {
    gateway: Option<omsi_net::ws::WsGateway>,
    tunnel: Option<omsi_net::tunnel::Tunnel>,
    /// Held for its connection's lifetime (dropping it closes the link).
    _client: Option<omsi_net::ws::WsClient>,
    /// The WebSocket address joined through (`wss://…/ws`).
    url: Option<String>,
    /// The client's reconnections last seen by `ws_came_back`.
    seen: usize,
}

/// The session's way in from the internet: the host's gateway and tunnel (a server socket
/// and a cloudflared process) or the joining game's WebSocket. Process-wide like the
/// sockets and the process it holds; closed with the session (`close_public_gateway`).
static WS_PATH: std::sync::Mutex<Option<WsPath>> = std::sync::Mutex::new(None);

/// Whether the joining game's WebSocket was made again since this was last asked.
/// (Counted with the connection: a later session's starts from none.)
pub fn ws_came_back() -> bool {
    let Ok(mut w) = WS_PATH.lock() else { return false };
    let Some(w) = w.as_mut() else { return false };
    let n = w._client.as_ref().map(|c| c.reconnects()).unwrap_or(0);
    n != std::mem::replace(&mut w.seen, n)
}

/// Shut the gateway and the tunnel (its cloudflared process) down: at the end of the game.
pub fn close_public_gateway() {
    let path = WS_PATH.lock().ok().and_then(|mut w| w.take());
    drop(path);
}

/// The public address the session is reached at through the tunnel (host), if it has one.
pub fn tunnel_url() -> Option<String> {
    WS_PATH.lock().ok()?.as_ref()?.tunnel.as_ref()?.url.lock().ok()?.clone()
}

/// Host: a WebSocket gateway to the session's port, and a free Cloudflare tunnel in front of
/// it when `cloudflared` is installed, whose address goes to the rendezvous under the
/// session's topic - the way in for a player whose router and ours cannot be punched
/// through (the code alone found a friend across the world once, and then never again).
/// `web_port` 0 picks the session port + 10.
pub fn open_public_gateway(session: &LanSession, info: omsi_net::ws::ServerInfo, web_port: u16, want_tunnel: bool) {
    let Some(udp) = session.local_addr() else { return };
    let target = SocketAddr::from(([127, 0, 0, 1], udp.port()));
    let port = if web_port == 0 { udp.port().saturating_add(10) } else { web_port };
    // (a web port taken: another one, and the log says which - the status page, the tunnel
    // and the players' WebSockets are there instead)
    let gateway = match omsi_net::ws::WsGateway::start(SocketAddr::from(([0, 0, 0, 0], port)), target, info.clone()).or_else(|e| {
        let g = omsi_net::ws::WsGateway::start(SocketAddr::from(([0, 0, 0, 0], 0)), target, info);
        if let Ok(g) = &g {
            log::warn!("LAN: web port {port} cannot be used ({e}): the gateway (status page, WebSocket players, tunnel) is on port {} instead - change web_port or stop what uses {port}", g.addr.port());
        }
        g
    }) {
        Ok(g) => g,
        Err(e) => {
            log::warn!("LAN: no WebSocket gateway: {e}");
            return;
        }
    };
    if let Ok(mut w) = WS_PATH.lock() {
        *w = Some(WsPath { gateway: Some(gateway), tunnel: None, _client: None, url: None, seen: 0 });
    }
    if !want_tunnel || omsi_cfg::flags::OMSI_NO_TUNNEL.is_set() || omsi_cfg::flags::OMSI_NO_BRIDGE.is_set() {
        return;
    }
    // (in the background: cloudflared is fetched first when it is not installed)
    let Some(key) = session.host_key() else { return };
    let gw_port = port_of_gateway();
    let _ = std::thread::Builder::new().name("tunnel".into()).spawn(move || {
        let Some(port) = gw_port else { return };
        let Some(t) = omsi_net::tunnel::Tunnel::start(port) else {
            log::info!("LAN: no tunnel: players whose routers cannot be reached join by the code alone");
            return;
        };
        let mut url = t.url.clone();
        if let Ok(mut w) = WS_PATH.lock() {
            if let Some(w) = w.as_mut() {
                w.tunnel = Some(t);
            }
        }
        // (posted when cloudflared has said the address, and again every half hour: the
        // relay keeps it for hours, a joining game takes the latest - and counts the posts)
        let mut posted: Option<(String, Instant)> = None;
        let mut checked = Instant::now();
        // the official server (`OMSI_OFFICIAL_KEY`: its signing key's file) says where it is
        // reached every five minutes, for the players who type `openomsi`
        let official = omsi_cfg::flags::OMSI_OFFICIAL_KEY.os().and_then(|p| std::fs::read(p).map_err(|e| log::warn!("official key {}: {e}", std::path::Path::new(&p).display())).ok());
        let mut announced: Option<(String, Instant)> = None;
        loop {
            let now = url.lock().ok().and_then(|u| u.clone());
            if let (Some(key), Some(u)) = (official.as_ref(), now.as_ref()) {
                let due = announced.as_ref().is_none_or(|(a, t)| a != u || t.elapsed() > Duration::from_secs(300));
                if due {
                    match omsi_net::official::announce(u, key) {
                        Ok(()) => log::info!("official server: announced at {u}"),
                        Err(e) => log::warn!("official server: not announced: {e}"),
                    }
                    announced = Some((u.clone(), Instant::now()));
                }
            }
            match (now, &posted) {
                (Some(u), None) => {
                    omsi_net::bridge::post_tunnel(&key, &u);
                    posted = Some((u, Instant::now()));
                }
                (Some(u), Some((p, t))) if *p != u || t.elapsed() > Duration::from_secs(1800) => {
                    omsi_net::bridge::post_tunnel(&key, &u);
                    posted = Some((u, Instant::now()));
                }
                _ => {}
            }
            if Arc::strong_count(&url) == 1 {
                return;
            }
            // cloudflared ended (Cloudflare drops a quick tunnel now and then, the network
            // went away): a new one, with a new address, posted again - the session stayed
            // unreachable through the tunnel for the rest of the evening
            if checked.elapsed() > Duration::from_secs(15) {
                checked = Instant::now();
                let dead = WS_PATH.lock().ok().and_then(|mut w| w.as_mut().and_then(|w| w.tunnel.as_mut().map(|t| !t.alive()))).unwrap_or(false);
                if dead {
                    log::warn!("LAN: the tunnel ended; starting a new one");
                    if let Some(t) = omsi_net::tunnel::Tunnel::start(port) {
                        url = t.url.clone();
                        posted = None;
                        if let Ok(mut w) = WS_PATH.lock() {
                            if let Some(w) = w.as_mut() {
                                w.tunnel = Some(t);
                            }
                        }
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    });
}

/// The buses a joining player may choose, on the gateway's status page (the launcher
/// offers only those): `only` when a server's list says, else every bus installed here -
/// read in the background (it takes a moment over all content folders).
/// `installed` lists every bus installed here (called on the thread, only when `only` is empty).
/// A dedicated server (`SERVER_VEHICLES` is set) that takes its players' buses
/// (`mods::upload`) and has no list offers any bus: none is published, so the launcher
/// limits nobody to the buses installed on the server.
pub fn publish_vehicles(only: Vec<String>, installed: impl FnOnce() -> Vec<String> + Send + 'static) {
    if only.is_empty() && crate::server_cfg::SERVER_VEHICLES.get().is_some() && crate::mods::upload::accepting() {
        log::info!("LAN: any bus is offered to joining players (the server takes the ones it lacks)");
        return;
    }
    let _ = std::thread::Builder::new().name("vehicle list".into()).spawn(move || {
        let list: Vec<String> = if only.is_empty() { installed() } else { only };
        log::info!("LAN: {} buses offered to joining players", list.len());
        if let Ok(w) = WS_PATH.lock() {
            if let Some(g) = w.as_ref().and_then(|w| w.gateway.as_ref()) {
                if let Ok(mut i) = g.info.lock() {
                    i.vehicles = list;
                }
            }
        }
    });
}

/// A new session (`lan::start`): every bus offered until its server says otherwise (see
/// `ServerOffers`). Its number, for `ask_server_offers`.
pub fn new_offers_session() -> u64 {
    server_offers_state().reset()
}

/// The WebSocket address a joining game came in through (`wss://…/ws`), if it did.
pub fn joined_url() -> Option<String> {
    WS_PATH.lock().ok().and_then(|w| w.as_ref().and_then(|w| w.url.clone()))
}

/// The buses the server we joined offers (its `vehicles`, else every bus it has), as its
/// status page says, tied to the session that asked: each session starts with every bus
/// offered, and a server's answer counts only while the session that asked it is the one
/// running (on a phone the launcher and the game share one process, one drive after the
/// other).
struct ServerOffers {
    /// The session now: counted up by every `lan::start`.
    session: u64,
    /// The offered buses as `bus_key`s. None: any - not on a server, or it has not said (yet).
    list: Option<std::sync::Arc<std::collections::HashSet<String>>>,
}

impl ServerOffers {
    /// A new session: every bus offered until its server says otherwise. Its number.
    fn reset(&mut self) -> u64 {
        self.session += 1;
        self.list = None;
        self.session
    }

    /// A server's answer to `session`: taken while that session runs, else dropped (an
    /// answer late for a session that ended).
    fn answer(&mut self, session: u64, vehicles: &[String]) -> bool {
        if session != self.session {
            return false;
        }
        self.list = Some(std::sync::Arc::new(offered_keys(vehicles)));
        true
    }
}

static SERVER_OFFERS: std::sync::Mutex<ServerOffers> = std::sync::Mutex::new(ServerOffers { session: 0, list: None });

fn server_offers_state() -> std::sync::MutexGuard<'static, ServerOffers> {
    SERVER_OFFERS.lock().unwrap_or_else(|e| e.into_inner())
}

/// The address to ask for the buses a server offers, for what the player joins: a server's
/// web address, or a host's address (`host`, `host:port`, a port on this computer - its
/// status page is found at that port, ten above it or 27025, see `omsi_net::ws::web_bases`).
/// None for a search of the network or a session code: nobody to ask.
fn offers_query_target(target: &str) -> Option<String> {
    let t = target.trim();
    if t.is_empty() || ["auto", "discover", "search"].iter().any(|w| t.eq_ignore_ascii_case(w)) {
        return None;
    }
    if omsi_net::ws::ws_url(t).is_some() {
        return Some(t.to_string());
    }
    if omsi_net::looks_like_code(t) {
        return None;
    }
    if t.bytes().all(|b| b.is_ascii_digit()) {
        return Some(format!("127.0.0.1:{t}"));
    }
    Some(t.to_string())
}

/// A player joining a server asks it in the background which buses it offers, so that the
/// game menu's "Place a vehicle" and "Swap" offer only those, as the launcher's bus step
/// does: a whitelist in `server.cfg` was dodged by placing another bus in the game (#1183).
pub fn ask_server_offers(target: &str, session: u64) {
    let Some(target) = offers_query_target(target) else { return };
    let _ = std::thread::Builder::new().name("server buses".into()).spawn(move || match omsi_net::ws::query(&target, false) {
        Ok(i) if !i.vehicles.is_empty() => {
            if server_offers_state().answer(session, &i.vehicles) {
                log::info!("LAN: the server offers {} buses", i.vehicles.len());
            }
        }
        Ok(_) => {}
        Err(e) => log::warn!("LAN: the server did not say which buses it offers ({e}); every bus is offered"),
    });
}

/// The buses the server we joined offers, as `bus_key`s (None: any, see `ServerOffers`).
pub fn server_offers() -> Option<std::sync::Arc<std::collections::HashSet<String>>> {
    server_offers_state().list.clone()
}

/// A vehicle file from its `Vehicles` folder on, in lower case and with forward slashes: a
/// server names it as its `server.cfg` does, or under its own content folder.
fn bus_key(s: &str) -> String {
    let s = s.trim().replace('\\', "/").to_ascii_lowercase();
    let parts: Vec<&str> = s.split('/').filter(|p| !p.is_empty()).collect();
    match parts.iter().rposition(|p| *p == "vehicles") {
        Some(i) => parts[i..].join("/"),
        None => parts.join("/"),
    }
}

/// A server's bus list as the keys `offers` looks a bus up in.
fn offered_keys(list: &[String]) -> std::collections::HashSet<String> {
    list.iter().map(|v| bus_key(v)).collect()
}

/// Whether `bus` (a vehicle file as the game's lists name it, `Vehicles/<folder>/<file>`) is
/// one of `offered` (see `server_offers`): the same file, in any case and with either slash.
pub fn offers(offered: &std::collections::HashSet<String>, bus: &str) -> bool {
    offered.contains(&bus_key(bus))
}

/// The port of the WebSocket gateway this game opened.
fn port_of_gateway() -> Option<u16> {
    WS_PATH.lock().ok()?.as_ref()?.gateway.as_ref().map(|g| g.addr.port())
}

/// A server run: what its status page says now.
pub fn update_server_info(players: usize, time: &str, weather: &str) {
    if let Ok(w) = WS_PATH.lock() {
        if let Some(g) = w.as_ref().and_then(|w| w.gateway.as_ref()) {
            if let Ok(mut i) = g.info.lock() {
                i.players = players;
                i.time = time.to_string();
                if !weather.is_empty() {
                    i.weather = weather.to_string();
                }
            }
        }
    }
}

/// A server run: its shared world now, for `GET /status`.
pub fn update_server_world(world: omsi_net::ws::WorldCounts) {
    if let Ok(w) = WS_PATH.lock() {
        if let Some(g) = w.as_ref().and_then(|w| w.gateway.as_ref()) {
            if let Ok(mut i) = g.info.lock() {
                i.world = Some(world);
            }
        }
    }
}

/// A server run: the players `GET /players` lists now.
pub fn update_server_players(list: Vec<omsi_net::ws::PlayerInfo>) {
    if let Ok(w) = WS_PATH.lock() {
        if let Some(g) = w.as_ref().and_then(|w| w.gateway.as_ref()) {
            if let Ok(mut i) = g.info.lock() {
                i.player_list = list;
            }
        }
    }
}

/// A dedicated server's administration on the web gateway: `/api/admin/…` and the panel
/// (`omsi_admin::http`), answered from the hub on the gateway's threads.
struct AdminApi {
    hub: Arc<omsi_admin::Hub>,
    cfg: omsi_admin::WebCfg,
}

impl omsi_net::ws_api::HttpApi for AdminApi {
    fn handle(&self, r: &omsi_net::ws_api::ApiRequest) -> Option<omsi_net::ws_api::ApiResponse> {
        let req = omsi_admin::http::Request { method: r.method, path: r.path, query: r.query, headers: r.headers, body: r.body, peer: r.peer };
        omsi_admin::http::handle(&self.hub, &self.cfg, &req).map(|a| omsi_net::ws_api::ApiResponse { status: a.status, content_type: a.content_type, body: a.body, headers: a.headers })
    }
}

/// The web administration opens on every gateway of this process (who may reach it: `cfg`).
pub fn open_admin_api(hub: Arc<omsi_admin::Hub>, cfg: omsi_admin::WebCfg) {
    omsi_net::ws_api::set_api(Some(Arc::new(AdminApi { hub, cfg })));
}

/// A server run: the admin commands that came to the web gateway's `POST /admin`.
pub fn take_local_admin() -> Vec<String> {
    if let Ok(w) = WS_PATH.lock() {
        if let Some(g) = w.as_ref().and_then(|w| w.gateway.as_ref()) {
            if let Ok(mut i) = g.info.lock() {
                return std::mem::take(&mut i.local_admin_queue);
            }
        }
    }
    Vec::new()
}

/// Joining game: reach `url` (a server's or a host's tunnel) over a WebSocket; the local
/// address to join instead.
pub fn ws_join_target(url: &str) -> Result<String, String> {
    let c = omsi_net::ws::WsClient::connect(url)?;
    let local = c.local;
    if let Ok(mut w) = WS_PATH.lock() {
        *w = Some(WsPath { gateway: None, tunnel: None, _client: Some(c), url: Some(url.to_string()), seen: 0 });
    }
    Ok(local.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every session starts with every bus offered: a server joined before (on a phone the
    /// launcher and the game share one process) no longer limits a drive alone or the next
    /// server, and an answer late for an ended session is dropped (#1183).
    #[test]
    fn a_new_session_forgets_the_last_servers_buses() {
        let mut o = ServerOffers { session: 0, list: None };
        let first = o.reset();
        assert!(o.answer(first, &["Vehicles/MAN_SD200/MAN_SD77.bus".to_string()]));
        assert!(o.list.as_ref().is_some_and(|l| offers(l, "Vehicles/MAN_SD200/MAN_SD77.bus")));
        let second = o.reset();
        assert!(o.list.is_none());
        // (the first server's answer, late)
        assert!(!o.answer(first, &["Vehicles/MAN_SD200/MAN_SD77.bus".to_string()]));
        assert!(o.list.is_none());
        assert!(o.answer(second, &["Vehicles/MAN_SD202/MAN_D92.bus".to_string()]));
        assert!(o.list.as_ref().is_some_and(|l| !offers(l, "Vehicles/MAN_SD200/MAN_SD77.bus") && offers(l, "Vehicles/MAN_SD202/MAN_D92.bus")));
    }

    /// Whom a joining game asks for the buses offered: a server's web address, or a host's
    /// address over UDP (its status page is found from there); nobody for a code or a search
    /// (#1183).
    #[test]
    fn the_buses_offered_are_asked_at_an_address() {
        assert_eq!(offers_query_target("https://abc.trycloudflare.com").as_deref(), Some("https://abc.trycloudflare.com"));
        assert_eq!(offers_query_target(" 203.0.113.5:27015 ").as_deref(), Some("203.0.113.5:27015"));
        assert_eq!(offers_query_target("bus.example.org").as_deref(), Some("bus.example.org"));
        assert_eq!(offers_query_target("27015").as_deref(), Some("127.0.0.1:27015"));
        assert_eq!(offers_query_target("auto"), None);
        assert_eq!(offers_query_target(""), None);
        let code = omsi_net::SessionCode { session: 0x1234_5678_9abc, port: 27015, ips: vec!["192.168.1.20".parse().unwrap()], protocol: omsi_net::PROTOCOL as u8 }.encode();
        assert_eq!(offers_query_target(&code), None);
    }

    /// A server's `vehicles` list (as its `server.cfg` writes it, or every bus under its own
    /// content folder) against the game's lists' `Vehicles/<folder>/<file>`: the same file
    /// only (#1183).
    #[test]
    fn a_server_offers_the_buses_of_its_list_only() {
        let list = offered_keys(&["Vehicles\\MAN_SD200\\MAN_SD77.bus".to_string(), "OMSI 2/Vehicles/MAN_SD202/MAN_D92.bus".to_string()]);
        assert!(offers(&list, "Vehicles/MAN_SD200/MAN_SD77.bus"));
        assert!(offers(&list, "vehicles/man_sd202/man_d92.bus"));
        assert!(offers(&list, "Archives/pack.zip/Vehicles/MAN_SD202/MAN_D92.bus"));
        assert!(!offers(&list, "Vehicles/MAN_SD200/MAN_SD83.bus"));
        assert!(!offers(&list, "Vehicles/MAN_SD202/MAN_D86.bus"));
        // (the same file name in another folder is another bus)
        assert!(!offers(&list, "Vehicles/Mod_SD200/MAN_SD77.bus"));
        assert!(!offers(&offered_keys(&[]), "Vehicles/MAN_SD200/MAN_SD77.bus"));
    }
}
