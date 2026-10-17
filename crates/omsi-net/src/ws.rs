//! The session's datagrams over a WebSocket, for the ways in that carry nothing but HTTP: a
//! Cloudflare tunnel (`cloudflared tunnel --url …` gives a free `https://….trycloudflare.com`
//! address and passes WebSockets, never UDP), a reverse proxy in front of a server.
//!
//! Nothing of the game protocol changes: each side keeps talking UDP to a socket on its own
//! machine and the bridge carries every datagram as one binary WebSocket message.
//!
//! * [`WsGateway`] (the host's or the server's side) listens for HTTP on a TCP port. A
//!   WebSocket there gets a UDP socket of its own on 127.0.0.1, so the session sees every
//!   player coming in this way as an address of its own. The same port answers
//!   `GET /status` (a small JSON object about the server, for the launcher's list),
//!   `GET /icon.png` and, when the server shares them (`share_positions`), `GET /players`:
//!   who drives what and where, for a web map of the server. A dedicated server with an
//!   admin password also takes `POST /admin`
//!   from the machine it runs on (see [`local_admin`]): the administration a tool beside the
//!   server uses, without joining the session.
//! * [`WsClient`] (a joining game) connects to `wss://…/ws`, binds a UDP socket on
//!   127.0.0.1 and gives its address to `LanSession::join`; whatever the game sends there
//!   goes over the WebSocket and back.

#[path = "ws/session.rs"]
mod session;
pub use session::query_session;

use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use crate::ws_http::{header, last_header, peek_head, read_body, read_head, refuse, REQUEST_TIME};
use tungstenite::protocol::WebSocketConfig;
use tungstenite::{Message, WebSocket};

/// What a server tells about itself (`GET /status`, and the launcher's list).
#[derive(Debug, Clone, Default)]
pub struct ServerInfo {
    pub name: String,
    pub motd: String,
    pub map: String,
    pub players: usize,
    pub max_players: usize,
    pub version: String,
    /// A PNG (64x64 like a Minecraft server's), empty for none.
    pub icon: Vec<u8>,
    /// The time of day and weather the world has now (for the list).
    pub time: String,
    pub weather: String,
    pub password: bool,
    /// The buses that may be driven there (vehicle files, `Vehicles/…/….bus`): what the
    /// host has installed, or a server's own list. Empty: not said (an older game).
    pub vehicles: Vec<String>,
    /// Where it answered (`http(s)://…`), set by `query`: when using WebSocket, a
    /// server added by its bare address (`1.2.3.4`, `host:27025`) is joined there.
    pub reached_at: String,
    /// `GET /players` answers (the server shares its players' positions); otherwise 404.
    pub players_public: bool,
    /// The players now, for `GET /players`.
    pub player_list: Vec<PlayerInfo>,
    /// `POST /admin` from this machine with this password (empty: no such door).
    pub local_admin_password: String,
    /// The admin commands that came in that way, for the host loop to run.
    pub local_admin_queue: Vec<String>,
    /// When wrong passwords came lately (they lock the door for a while).
    pub local_admin_failures: Vec<Instant>,
    /// A dedicated server's shared world now (`"world"` in `GET /status`); none elsewhere.
    pub world: Option<WorldCounts>,
}

/// What a dedicated server's shared world holds: the AI cars on the roads (`cars`), its
/// timetable buses (`buses`), the cars put to sleep far from every player (`dormant`), the
/// parked cars (`parked`), the people walking (`walking`), waiting at a stop (`waiting`) or
/// in a bus (`aboard`), the traffic density asked for (`traffic`, as `server.cfg`'s), whether
/// the server has people at all (`passengers`) and the passenger density it applies now
/// (`passenger_density`, the factor on the map's own demand).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WorldCounts {
    pub cars: usize,
    pub buses: usize,
    pub dormant: usize,
    pub parked: usize,
    pub walking: usize,
    pub waiting: usize,
    pub aboard: usize,
    pub traffic: usize,
    pub passengers: bool,
    pub passenger_density: f32,
}

impl WorldCounts {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"cars\":{},\"buses\":{},\"dormant\":{},\"parked\":{},\"walking\":{},\"waiting\":{},\"aboard\":{},\"traffic\":{},\"passengers\":{},\"passenger_density\":{}}}",
            self.cars,
            self.buses,
            self.dormant,
            self.parked,
            self.walking,
            self.waiting,
            self.aboard,
            self.traffic,
            self.passengers,
            if self.passenger_density.is_finite() { format!("{:.2}", self.passenger_density) } else { "null".into() }
        )
    }
}

/// A player as `GET /players` tells it: a web map of the server draws it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayerInfo {
    pub id: u32,
    pub name: String,
    /// Vehicle file (`Vehicles/…/….bus`), empty for a player on foot.
    pub bus: String,
    pub line: String,
    pub destination: String,
    /// The timetable tour, `<line>/<tour>` (empty for none).
    pub tour: String,
    /// World metres (x east, y north) and heading (degrees, clockwise from north): the bus
    /// driven, or the player on foot, or the bus the player sits in.
    pub x: f64,
    pub y: f64,
    pub heading: f32,
    pub speed_kmh: f32,
    /// Not driving: walking, or aboard another player's bus (`aboard`: that player's id).
    pub on_foot: bool,
    pub aboard: Option<u32>,
    /// Where that is on the earth, on a `[worldcoordinates]` map.
    pub lat_lon: Option<(f64, f64)>,
}

impl PlayerInfo {
    pub fn to_json(&self) -> String {
        let num = |v: f64, digits: usize| if v.is_finite() { format!("{v:.digits$}") } else { "null".into() };
        let (lat, lon) = match self.lat_lon {
            Some((a, o)) => (num(a, 6), num(o, 6)),
            None => ("null".into(), "null".into()),
        };
        format!(
            "{{\"id\":{},\"name\":{},\"bus\":{},\"line\":{},\"destination\":{},\"tour\":{},\"x\":{},\"y\":{},\"heading\":{},\"speed_kmh\":{},\"on_foot\":{},\"aboard\":{},\"lat\":{},\"lon\":{}}}",
            self.id,
            json_str(&self.name),
            json_str(&self.bus),
            json_str(&self.line),
            json_str(&self.destination),
            json_str(&self.tour),
            num(self.x, 1),
            num(self.y, 1),
            num(self.heading as f64, 1),
            num(self.speed_kmh as f64, 1),
            self.on_foot,
            self.aboard.map(|a| a.to_string()).unwrap_or_else(|| "null".into()),
            lat,
            lon
        )
    }
}

/// `GET /players`: a JSON array of the players.
pub fn players_json(players: &[PlayerInfo]) -> String {
    format!("[{}]", players.iter().map(PlayerInfo::to_json).collect::<Vec<_>>().join(","))
}

impl ServerInfo {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"name\":{},\"motd\":{},\"map\":{},\"players\":{},\"max_players\":{},\"version\":{},\"icon\":{},\"time\":{},\"weather\":{},\"password\":{},\"protocol\":{},\"vehicles\":{},\"world\":{}}}",
            json_str(&self.name),
            json_str(&self.motd),
            json_str(&self.map),
            self.players,
            self.max_players,
            json_str(&self.version),
            !self.icon.is_empty(),
            json_str(&self.time),
            json_str(&self.weather),
            self.password,
            crate::PROTOCOL,
            json_str(&self.vehicles.join(";")),
            self.world.map(|w| w.to_json()).unwrap_or_else(|| "null".into())
        )
    }

    /// Read what `to_json` wrote (the launcher asking a server).
    pub fn from_json(s: &str) -> Option<ServerInfo> {
        let text = |k: &str| json_value(s, k).map(|v| v.strip_prefix('"').and_then(|v| v.strip_suffix('"')).unwrap_or(&v).to_string().replace("\\\"", "\"").replace("\\\\", "\\").replace("\\n", "\n"));
        let num = |k: &str| json_value(s, k).and_then(|v| v.trim().parse::<usize>().ok());
        Some(ServerInfo {
            name: text("name")?,
            motd: text("motd").unwrap_or_default(),
            map: text("map").unwrap_or_default(),
            players: num("players").unwrap_or(0),
            max_players: num("max_players").unwrap_or(0),
            version: text("version").unwrap_or_default(),
            icon: if json_value(s, "icon").map(|v| v.trim() == "true").unwrap_or(false) { vec![1] } else { Vec::new() },
            time: text("time").unwrap_or_default(),
            weather: text("weather").unwrap_or_default(),
            password: json_value(s, "password").map(|v| v.trim() == "true").unwrap_or(false),
            vehicles: text("vehicles").map(|v| v.split(';').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()).unwrap_or_default(),
            reached_at: String::new(),
            ..Default::default()
        })
    }
}

fn json_str(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            c if c.is_control() => {}
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// The raw value of `key` in a flat JSON object (a string with its quotes, or a number).
fn json_value(s: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\":");
    let start = s.find(&pat)? + pat.len();
    let rest = s[start..].trim_start();
    if let Some(r) = rest.strip_prefix('"') {
        let mut out = String::from("\"");
        let mut esc = false;
        for c in r.chars() {
            if esc {
                out.push('\\');
                out.push(c);
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                out.push('"');
                return Some(out);
            } else {
                out.push(c);
            }
        }
        None
    } else {
        let end = rest.find([',', '}']).unwrap_or(rest.len());
        Some(rest[..end].to_string())
    }
}

/// A WebSocket address for what a player typed: `https://x` → `wss://x/ws`, `http://x` →
/// `ws://x/ws`, `ws(s)://…` as it is. None for anything else (an address or a code).
pub fn ws_url(target: &str) -> Option<String> {
    let t = target.trim();
    let (scheme, rest) = if let Some(r) = t.strip_prefix("https://") {
        ("wss://", r)
    } else if let Some(r) = t.strip_prefix("http://") {
        ("ws://", r)
    } else if t.starts_with("wss://") || t.starts_with("ws://") {
        return Some(t.to_string());
    } else if t.ends_with(".trycloudflare.com") || t.contains(".trycloudflare.com/") {
        ("wss://", t)
    } else {
        return None;
    };
    let rest = rest.trim_end_matches('/');
    Some(if rest.ends_with("/ws") { format!("{scheme}{rest}") } else { format!("{scheme}{rest}/ws") })
}

/// The `https://` base of a server address (for `/status` and `/icon.png`).
pub fn http_base(target: &str) -> Option<String> {
    let u = ws_url(target)?;
    let u = u.strip_suffix("/ws").unwrap_or(&u).to_string();
    Some(u.replacen("wss://", "https://", 1).replacen("ws://", "http://", 1))
}

/// The web addresses a server may answer at, for whatever address a player gave: a web
/// address as it is; a bare `host` or `host:port` (an IP, a domain) at the
/// port ten above it (the game's port was given: a server's web port is its game port + 10
/// unless `web_port` says otherwise), at the servers' default 27025, and over https.
/// An explicitly typed port is tried first, before port + 10 and HTTPS.
pub fn web_bases(target: &str) -> Vec<String> {
    if let Some(b) = http_base(target) {
        return vec![b];
    }
    let t = target.trim().trim_end_matches('/');
    let t = t.strip_suffix("/ws").unwrap_or(t);
    if t.is_empty() || t.contains(char::is_whitespace) {
        return Vec::new();
    }
    // (an IPv6 address is written in brackets with a port: [::1]:27025)
    let (host, port) = match t.rsplit_once(':') {
        Some((h, p)) if !h.is_empty() && (!h.contains(':') || h.ends_with(']')) => match p.parse::<u16>() {
            Ok(p) => (h.to_string(), Some(p)),
            Err(_) => (t.to_string(), None),
        },
        _ if t.contains(':') && !t.starts_with('[') => (format!("[{t}]"), None),
        _ => (t.to_string(), None),
    };
    let mut out = Vec::new();
    let mut add = |u: String| {
        if !out.contains(&u) {
            out.push(u);
        }
    };
    if let Some(p) = port {
        add(format!("http://{host}:{p}"));
        if p <= 65525 {
            add(format!("http://{host}:{}", p + 10));
        }
        add(format!("https://{host}"));
    }
    add(format!("http://{host}:27025"));
    if port.is_none() {
        add(format!("https://{host}"));
        add(format!("http://{host}"));
    }
    out
}

/// Ask a server (by its address as typed) about itself: its status and its icon.
pub fn query(target: &str, with_icon: bool) -> Result<ServerInfo, String> {
    let target = &crate::official::resolve_target(target)?;
    let bases = web_bases(target);
    if bases.is_empty() {
        return Err("no address given".into());
    }
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(6)).user_agent("openOMSI").build();
    let mut err = String::new();
    let mut found = None;
    for base in bases {
        match agent.get(&format!("{base}/status")).call().map_err(|e| e.to_string()).and_then(|r| r.into_string().map_err(|e| e.to_string())) {
            Ok(body) => match ServerInfo::from_json(&body) {
                Some(i) => {
                    found = Some((base, i));
                    break;
                }
                None => err = "the answer is not an openOMSI server's".into(),
            },
            Err(e) => {
                if err.is_empty() {
                    err = e;
                }
            }
        }
    }
    let (base, mut info) = found.ok_or(err)?;
    info.reached_at = base.clone();
    if with_icon && !info.icon.is_empty() {
        info.icon.clear();
        if let Ok(r) = agent.get(&format!("{base}/icon.png")).call() {
            let mut buf = Vec::new();
            if r.into_reader().take(512 * 1024).read_to_end(&mut buf).is_ok() && buf.starts_with(b"\x89PNG") {
                info.icon = buf;
            }
        }
    }
    Ok(info)
}

/// Connections the gateway serves at once (each is a thread): the places of both kinds
/// below. One whose request has not shown its kind yet holds a thread only, for at most
/// `HEAD_WAIT` (and one source has its own ceiling, `PER_SOURCE`).
const MAX_CONNECTIONS: usize = MAX_ORDINARY + MAX_TUNNELS;

/// Players, status requests and the icon carried at once: the 64 the hardening set for
/// everything. One more is answered `503`. Mod streams never take one of these places.
const MAX_ORDINARY: usize = if cfg!(test) { 3 } else { 64 };

/// Mod streams (`/tcp`) carried at once, apart from the players' places; one more is told
/// the mods server's `ERR busy` and its game asks again in a moment. With 64 for
/// everything, fifteen players fetching a new player's bus (a list and four pieces each)
/// took every place, and one more player could not join at all.
const MAX_TUNNELS: usize = if cfg!(test) { 3 } else { 160 };
const PER_SOURCE: usize = 16;
const HEAD_WAIT: Duration = Duration::from_secs(3);
const DATAGRAM_LIMIT: usize = 4096;
const STREAM_LIMIT: usize = 128 * 1024;

fn datagram_config() -> WebSocketConfig {
    WebSocketConfig::default().read_buffer_size(8 * 1024).max_message_size(Some(DATAGRAM_LIMIT)).max_frame_size(Some(DATAGRAM_LIMIT))
}

fn stream_config() -> WebSocketConfig {
    WebSocketConfig::default().max_message_size(Some(STREAM_LIMIT)).max_frame_size(Some(STREAM_LIMIT))
}

fn gateway_clients() -> &'static Mutex<std::collections::HashMap<SocketAddr, std::net::IpAddr>> {
    static CLIENTS: std::sync::OnceLock<Mutex<std::collections::HashMap<SocketAddr, std::net::IpAddr>>> = std::sync::OnceLock::new();
    CLIENTS.get_or_init(Default::default)
}

pub(crate) fn gateway_client(local: SocketAddr) -> Option<std::net::IpAddr> {
    gateway_clients().lock().unwrap_or_else(|e| e.into_inner()).get(&local).copied()
}

pub(crate) struct GatewayClient(SocketAddr);

impl GatewayClient {
    pub(crate) fn register(local: SocketAddr, ip: std::net::IpAddr) -> GatewayClient {
        gateway_clients().lock().unwrap_or_else(|e| e.into_inner()).insert(local, ip);
        GatewayClient(local)
    }
}

impl Drop for GatewayClient {
    fn drop(&mut self) {
        gateway_clients().lock().unwrap_or_else(|e| e.into_inner()).remove(&self.0);
    }
}

#[derive(Default)]
struct Sources(Mutex<std::collections::HashMap<std::net::IpAddr, usize>>);

struct SourceSlot(Arc<Sources>, std::net::IpAddr);

impl Sources {
    fn take(self: &Arc<Self>, source: std::net::IpAddr) -> Option<SourceSlot> {
        let key = source_key(source);
        let mut open = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let n = open.entry(key).or_insert(0);
        if *n >= PER_SOURCE {
            return None;
        }
        *n += 1;
        Some(SourceSlot(self.clone(), key))
    }
}

impl Drop for SourceSlot {
    fn drop(&mut self) {
        let mut open = self.0 .0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(n) = open.get_mut(&self.1) {
            *n -= 1;
            if *n == 0 {
                open.remove(&self.1);
            }
        }
    }
}

/// The places of one kind of connection (`MAX_ORDINARY`, `MAX_TUNNELS`): one is taken when
/// a request shows its kind and given back when its connection ends.
#[derive(Default)]
struct Places(AtomicUsize);

struct Place<'a>(&'a AtomicUsize);

impl Places {
    /// A place, when fewer than `max` are taken.
    fn take(&self, max: usize) -> Option<Place<'_>> {
        self.0.fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| (n < max).then_some(n + 1)).ok().map(|_| Place(&self.0))
    }
}

impl Drop for Place<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

/// The gateway's places: the players' and status requests', and the mod streams'.
#[derive(Default)]
struct Admission {
    ordinary: Places,
    tunnels: Places,
}

/// The host's or server's side (see the module).
pub struct WsGateway {
    pub addr: SocketAddr,
    pub info: Arc<Mutex<ServerInfo>>,
    /// Players connected over WebSockets now.
    pub connected: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
}

impl Drop for WsGateway {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl WsGateway {
    /// Listen on `listen` (TCP) and carry WebSockets to the session at `target` (UDP).
    pub fn start(listen: SocketAddr, target: SocketAddr, info: ServerInfo) -> std::io::Result<WsGateway> {
        let listener = TcpListener::bind(listen)?;
        let addr = listener.local_addr()?;
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let info = Arc::new(Mutex::new(info));
        let connected = Arc::new(AtomicUsize::new(0));
        let (st, inf, conn) = (stop.clone(), info.clone(), connected.clone());
        std::thread::Builder::new().name("ws gateway".into()).spawn(move || {
            // connections served at once (each its own thread): more are closed at once
            let open = Arc::new(AtomicUsize::new(0));
            let admission = Arc::new(Admission::default());
            let sources = Arc::new(Sources::default());
            while !st.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, peer)) => {
                        if open.load(Ordering::Relaxed) >= MAX_CONNECTIONS {
                            log::debug!("ws gateway: {peer}: {MAX_CONNECTIONS} connections open already; closed");
                            drop(stream);
                            continue;
                        }
                        let slot = if peer.ip().is_loopback() {
                            None
                        } else {
                            match sources.take(peer.ip()) {
                                Some(slot) => Some(slot),
                                None => {
                                    log::debug!("ws gateway: {peer}: {PER_SOURCE} connections open already from there; closed");
                                    drop(stream);
                                    continue;
                                }
                            }
                        };
                        open.fetch_add(1, Ordering::Relaxed);
                        let (st, inf, conn, held, src, adm) = (st.clone(), inf.clone(), conn.clone(), open.clone(), sources.clone(), admission.clone());
                        let spawned = std::thread::Builder::new().name("ws player".into()).spawn(move || {
                            if let Err(e) = serve(stream, slot, target, &inf, &st, &conn, &src, &adm) {
                                log::debug!("ws gateway: {peer}: {e}");
                            }
                            held.fetch_sub(1, Ordering::Relaxed);
                        });
                        if spawned.is_err() {
                            open.fetch_sub(1, Ordering::Relaxed);
                        }
                    }
                    Err(e) if e.kind() == ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(40)),
                    Err(e) => {
                        log::warn!("ws gateway: accept: {e}");
                        std::thread::sleep(Duration::from_millis(200));
                    }
                }
            }
        })?;
        log::info!("ws gateway: WebSockets on {addr} carry the session at {target} (GET /status, /icon.png)");
        Ok(WsGateway { addr, info, connected, stop })
    }
}

/// Wrong local admin passwords within `ADMIN_LOCK_WINDOW` that close the door for a while.
const ADMIN_LOCK_AFTER: usize = 5;
const ADMIN_LOCK_WINDOW: Duration = Duration::from_secs(120);

/// Behind Cloudflare's proxy and one of this machine, the player's address is Cloudflare's
/// `CF-Connecting-IP`, not the last `X-Forwarded-For` (an edge's): `gateway_proxy_header`.
static CF_FIRST: AtomicBool = AtomicBool::new(false);

/// Believe `CF-Connecting-IP` before `X-Forwarded-For` (`gateway_proxy_header =
/// cf-connecting-ip`): only with Cloudflare in front, which writes it over the client's own.
pub fn believe_cf_connecting_ip(on: bool) {
    CF_FIRST.store(on, Ordering::Relaxed);
}

fn client_ip(head: &str, peer: Option<SocketAddr>, cf_first: bool) -> Option<std::net::IpAddr> {
    let peer = peer?;
    if !peer.ip().is_loopback() {
        return Some(peer.ip());
    }
    // (the address the proxy added - the last of X-Forwarded-For: a CF-Connecting-IP or an
    // earlier address may be the client's own, passed on by a proxy on this machine; Cloudflare
    // writes both, the same - unless the operator says Cloudflare is in front)
    let forwarded = || last_header(head, "x-forwarded-for").and_then(|v| v.rsplit(',').next());
    let cf = || last_header(head, "cf-connecting-ip");
    if cf_first { cf().or_else(forwarded) } else { forwarded().or_else(cf) }
        .map(str::trim)
        .and_then(|v| v.parse::<std::net::IpAddr>().ok())
        .filter(|ip| !ip.is_loopback())
}

pub(crate) fn source_key(ip: std::net::IpAddr) -> std::net::IpAddr {
    match ip {
        std::net::IpAddr::V6(v6) if v6.to_ipv4_mapped().is_none() => {
            let s = v6.segments();
            std::net::IpAddr::V6(std::net::Ipv6Addr::new(s[0], s[1], s[2], s[3], 0, 0, 0, 0))
        }
        std::net::IpAddr::V6(v6) => std::net::IpAddr::V4(v6.to_ipv4_mapped().expect("mapped")),
        v4 => v4,
    }
}

/// Compare two secrets in a time that does not tell how much of them matched.
fn same_secret(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut d = (a.len() ^ b.len()) as u8 | (a.len() != b.len()) as u8;
    for i in 0..a.len().max(b.len()) {
        d |= a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0);
    }
    d == 0
}

/// `POST /admin`: admin commands (one a line, as the Administration menu sends them:
/// `clock 30600`, `weather next`, `say …`, `kick 3` …) for a dedicated server, from a tool on
/// the same machine - a web dispatch page, a script. Only from the loopback, only with the
/// server's admin password in `X-Admin-Password`; five wrong ones in two minutes close it
/// for a while. A reverse proxy on the same machine forwards from 127.0.0.1 too: it must
/// not pass `/admin` on.
pub fn local_admin(request: &[u8], peer: Option<SocketAddr>, info: &Mutex<ServerInfo>) -> (&'static str, String) {
    let text = String::from_utf8_lossy(request);
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let mut i = info.lock().unwrap_or_else(|e| e.into_inner());
    if i.local_admin_password.is_empty() {
        return ("404 Not Found", "no admin password on this server".into());
    }
    if !peer.map(|p| p.ip().is_loopback()).unwrap_or(false) {
        return ("403 Forbidden", "only from this machine".into());
    }
    // A tunnel or a proxy on this machine connects from the loopback as well: the server's
    // own cloudflared tunnel (`tunnel --url http://127.0.0.1:<web_port>`) would have put the
    // door on the internet behind the password alone. What came through one says so.
    if ["cf-connecting-ip", "cf-ray", "x-forwarded-for", "forwarded", "x-real-ip"].iter().any(|h| header(head, h).is_some()) {
        return ("403 Forbidden", "only from this machine, not through a tunnel or proxy".into());
    }
    if !head.starts_with("POST ") {
        return ("405 Method Not Allowed", "POST admin commands, one a line".into());
    }
    i.local_admin_failures.retain(|t| t.elapsed() < ADMIN_LOCK_WINDOW);
    if i.local_admin_failures.len() >= ADMIN_LOCK_AFTER {
        return ("429 Too Many Requests", "too many wrong passwords: try again later".into());
    }
    if !same_secret(header(head, "x-admin-password").unwrap_or(""), &i.local_admin_password) {
        i.local_admin_failures.push(Instant::now());
        return ("401 Unauthorized", "wrong admin password".into());
    }
    let commands: Vec<String> = body.lines().map(str::trim).filter(|l| !l.is_empty()).take(10).map(|l| l.chars().take(200).collect()).collect();
    let n = commands.len();
    i.local_admin_queue.extend(commands);
    ("202 Accepted", format!("{n} command(s) taken"))
}

/// One TCP connection to the gateway: a status request, the icon, a player's WebSocket or a
/// mod stream.
#[allow(clippy::too_many_arguments)]
fn serve(stream: TcpStream, slot: Option<SourceSlot>, target: SocketAddr, info: &Mutex<ServerInfo>, stop: &AtomicBool, connected: &AtomicUsize, sources: &Arc<Sources>, admission: &Admission) -> Result<(), String> {
    let peer = stream.peer_addr().ok();
    stream.set_nonblocking(false).map_err(|e| e.to_string())?;
    stream.set_read_timeout(Some(HEAD_WAIT)).map_err(|e| e.to_string())?;
    let mut first = [0u8; 1];
    stream.peek(&mut first).map_err(|e| e.to_string())?;
    let deadline = Instant::now() + REQUEST_TIME;
    let req = String::from_utf8_lossy(&peek_head(&stream, deadline)).to_string();
    stream.set_read_timeout(Some(HEAD_WAIT)).map_err(|e| e.to_string())?;
    let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
    let upgrade = req.to_ascii_lowercase().contains("upgrade: websocket");
    let client = client_ip(&req, peer, CF_FIRST.load(Ordering::Relaxed));
    let source = client.map(|ip| ip.to_string()).unwrap_or_else(|| peer.map(|p| p.ip().to_string()).unwrap_or_else(|| "?".into()));
    let _slot = match (slot, client) {
        (Some(slot), _) => Some(slot),
        (None, Some(ip)) => Some(sources.take(ip).ok_or_else(|| format!("{source}: {PER_SOURCE} connections open already"))?),
        (None, None) => None,
    };
    let tunnel = upgrade && path.starts_with("/tcp");
    // (a player or a status request takes one of the players' places, for its whole life:
    // the mod streams have places of their own, below)
    let _place = if tunnel {
        None
    } else {
        match admission.ordinary.take(MAX_ORDINARY) {
            Some(place) => Some(place),
            None => {
                log::debug!("ws gateway: {source}: {MAX_ORDINARY} players and requests carried already; one more told to come back");
                refuse(stream, "503 Service Unavailable", "Retry-After: 2\r\n");
                return Ok(());
            }
        }
    };
    if !upgrade || tunnel {
        stream.set_read_timeout(Some(Duration::from_secs(10))).map_err(|e| e.to_string())?;
    }
    if !upgrade {
        let mut s = stream;
        // (the request is read off the socket before the answer: some proxies wait) - the
        // whole head, however it came in pieces: one cut short would lose the headers past
        // the cut - a tunnel's or a proxy's marks, the Authorization
        let mut request = Vec::new();
        if let Err(status) = read_head(&mut s, &mut request, deadline) {
            refuse(s, status, "");
            return Ok(());
        }
        // a dedicated server's administration (`ws_api`): its own answer and headers
        if crate::ws_api::wants(&path) {
            read_body(&mut s, &mut request, deadline);
            let r = crate::ws_api::answer(&request, peer);
            let mut hdr = format!("HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n", r.status, r.content_type, r.body.len());
            for (k, v) in &r.headers {
                hdr.push_str(&format!("{k}: {}\r\n", v.replace(['\r', '\n'], " ")));
            }
            hdr.push_str("\r\n");
            s.write_all(hdr.as_bytes()).map_err(|e| e.to_string())?;
            s.write_all(&r.body).map_err(|e| e.to_string())?;
            return Ok(());
        }
        let (status, ctype, body): (&str, &str, Vec<u8>) = match path.as_str() {
            "/admin" => {
                read_body(&mut s, &mut request, deadline);
                let (status, text) = local_admin(&request, peer, info);
                (status, "text/plain; charset=utf-8", text.into_bytes())
            }
            "/status" | "/status.json" => ("200 OK", "application/json", info.lock().unwrap_or_else(|e| e.into_inner()).to_json().into_bytes()),
            "/players" | "/players.json" => {
                let i = info.lock().unwrap_or_else(|e| e.into_inner());
                if i.players_public {
                    ("200 OK", "application/json", players_json(&i.player_list).into_bytes())
                } else {
                    ("404 Not Found", "text/plain", b"this server does not share its players' positions".to_vec())
                }
            }
            "/icon.png" => {
                let icon = info.lock().unwrap_or_else(|e| e.into_inner()).icon.clone();
                if icon.is_empty() {
                    ("404 Not Found", "text/plain", b"no icon".to_vec())
                } else {
                    ("200 OK", "image/png", icon)
                }
            }
            _ => {
                let i = info.lock().unwrap_or_else(|e| e.into_inner()).clone();
                let page = format!("<!doctype html><meta charset=utf-8><title>{0}</title><body style=\"font-family:sans-serif;background:#16181c;color:#eee;padding:40px\"><h1>{0}</h1><p>{1}</p><p>Map: {2} &middot; {3}/{4} players</p><p>Add this address in openOMSI &rarr; Multiplayer &rarr; Servers.</p>", html(&i.name), html(&i.motd), html(&i.map), i.players, i.max_players);
                ("200 OK", "text/html; charset=utf-8", page.into_bytes())
            }
        };
        let hdr = format!("HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n", body.len());
        s.write_all(hdr.as_bytes()).map_err(|e| e.to_string())?;
        s.write_all(&body).map_err(|e| e.to_string())?;
        return Ok(());
    }
    if tunnel {
        // a byte stream to the session's TCP port (the host's mods): the way the files go
        // where only HTTP gets through
        let mut ws = tungstenite::accept_with_config(stream, Some(stream_config())).map_err(|e| e.to_string())?;
        let Some(_place) = admission.tunnels.take(MAX_TUNNELS) else {
            log::info!("ws gateway: {MAX_TUNNELS} mod streams open already; one more told to come back");
            // (what the mods server says when it has no place: the game knows to ask again)
            let _ = ws.send(Message::Binary(b"ERR busy\n".to_vec().into()));
            let _ = ws.close(None);
            let _ = ws.flush();
            return Ok(());
        };
        // (the 10 s of the request's read held every chunk that long)
        ws.get_mut().set_read_timeout(Some(Duration::from_millis(5))).map_err(|e| e.to_string())?;
        let tcp = TcpStream::connect(SocketAddr::from(([127, 0, 0, 1], target.port()))).map_err(|e| e.to_string())?;
        return pump_tcp(&mut ws, tcp, stop);
    }
    let mut ws = tungstenite::accept_with_config(stream, Some(datagram_config())).map_err(|e| e.to_string())?;
    let udp = UdpSocket::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    udp.set_nonblocking(true).map_err(|e| e.to_string())?;
    // the session sees this player at the socket's 127.0.0.1 address ("joined from
    // 127.0.0.1:<port>"): say who that is, for a server's operator
    let _client = udp.local_addr().ok().map(|local| {
        log::info!("gateway: player WebSocket from {source} on {local}");
        client.map(|ip| GatewayClient::register(local, ip))
    });
    ws.get_mut().set_read_timeout(Some(Duration::from_millis(5))).map_err(|e| e.to_string())?;
    connected.fetch_add(1, Ordering::Relaxed);
    let r = pump(&mut ws, &udp, |u, d| u.send_to(d, target).map(|_| ()), stop);
    connected.fetch_sub(1, Ordering::Relaxed);
    r
}

fn html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Carry datagrams both ways between a WebSocket and a UDP socket until either side goes.
/// `send` hands a message from the WebSocket to the UDP side.
fn pump<S: Read + Write>(ws: &mut WebSocket<S>, udp: &UdpSocket, mut send: impl FnMut(&UdpSocket, &[u8]) -> std::io::Result<()>, stop: &AtomicBool) -> Result<(), String> {
    let mut buf = vec![0u8; 2048];
    let mut last_in = Instant::now();
    let mut last_ping = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        let mut idle = true;
        // WebSocket → UDP
        loop {
            match ws.read() {
                Ok(Message::Binary(d)) => {
                    last_in = Instant::now();
                    idle = false;
                    let _ = send(udp, &d);
                }
                Ok(Message::Close(_)) => return Ok(()),
                Ok(_) => last_in = Instant::now(),
                Err(tungstenite::Error::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => break,
                Err(tungstenite::Error::ConnectionClosed) | Err(tungstenite::Error::AlreadyClosed) => return Ok(()),
                Err(e) => return Err(e.to_string()),
            }
        }
        // UDP → WebSocket
        loop {
            match udp.recv_from(&mut buf) {
                Ok((n, _)) => {
                    idle = false;
                    if let Err(e) = ws.send(Message::Binary(buf[..n].to_vec().into())) {
                        if !matches!(&e, tungstenite::Error::Io(io) if io.kind() == ErrorKind::WouldBlock) {
                            return Err(e.to_string());
                        }
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == ErrorKind::ConnectionReset => break,
                Err(e) => return Err(e.to_string()),
            }
        }
        let _ = ws.flush();
        // (proxies close a WebSocket that stays quiet for a minute or two)
        if last_ping.elapsed() > Duration::from_secs(20) {
            last_ping = Instant::now();
            let _ = ws.send(Message::Ping(Vec::new().into()));
        }
        if last_in.elapsed() > Duration::from_secs(90) {
            return Err("nothing heard for 90 s".into());
        }
        if idle {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    let _ = ws.close(None);
    Ok(())
}

/// Carry a TCP stream both ways over a WebSocket until either side closes.
fn pump_tcp<S: Read + Write>(ws: &mut WebSocket<S>, mut tcp: TcpStream, stop: &AtomicBool) -> Result<(), String> {
    tcp.set_nonblocking(true).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut last = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        let mut idle = true;
        loop {
            match ws.read() {
                Ok(Message::Binary(d)) => {
                    idle = false;
                    last = Instant::now();
                    tcp.set_nonblocking(false).ok();
                    let r = tcp.write_all(&d);
                    tcp.set_nonblocking(true).ok();
                    r.map_err(|e| e.to_string())?;
                }
                Ok(Message::Close(_)) => return Ok(()),
                Ok(_) => {}
                Err(tungstenite::Error::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => break,
                Err(tungstenite::Error::ConnectionClosed) | Err(tungstenite::Error::AlreadyClosed) => return Ok(()),
                Err(e) => return Err(e.to_string()),
            }
        }
        // (up to 2 MB of what the TCP side has before the WebSocket is looked at again: one
        // chunk per turn held a download to a few hundred kB/s)
        for _ in 0..32 {
            match tcp.read(&mut buf) {
                Ok(0) => {
                    let _ = ws.close(None);
                    let _ = ws.flush();
                    return Ok(());
                }
                Ok(n) => {
                    idle = false;
                    last = Instant::now();
                    ws.write(Message::Binary(buf[..n].to_vec().into())).map_err(|e| e.to_string())?;
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) => return Err(e.to_string()),
            }
        }
        let _ = ws.flush();
        if last.elapsed() > Duration::from_secs(120) {
            return Err("stream idle for 2 min".into());
        }
        if idle {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    Ok(())
}

/// A local TCP port whose connections go over WebSockets to `url` (`wss://…/tcp`): the
/// host's mods fetched through its tunnel.
pub fn tcp_forward(url: &str) -> std::io::Result<SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    let url = url.to_string();
    std::thread::Builder::new().name("ws tcp forward".into()).spawn(move || {
        for conn in listener.incoming().flatten() {
            let url = url.clone();
            std::thread::spawn(move || {
                let r = (|| -> Result<(), String> {
                    let (mut ws, _) = tungstenite::client::connect_with_config(&url, Some(stream_config()), 3).map_err(|e| format!("{url}: {e}"))?;
                    match ws.get_mut() {
                        tungstenite::stream::MaybeTlsStream::Plain(s) => s.set_read_timeout(Some(Duration::from_millis(5))),
                        tungstenite::stream::MaybeTlsStream::Rustls(s) => s.get_mut().set_read_timeout(Some(Duration::from_millis(5))),
                        _ => Ok(()),
                    }
                    .map_err(|e| e.to_string())?;
                    pump_tcp(&mut ws, conn, &AtomicBool::new(false))
                })();
                if let Err(e) = r {
                    log::warn!("ws tcp forward: {e}");
                }
            });
        }
    })?;
    Ok(addr)
}

/// A joining game's side (see the module).
pub struct WsClient {
    /// Where the game sends its datagrams (the session "host" as the game sees it).
    pub local: SocketAddr,
    stop: Arc<AtomicBool>,
    pub alive: Arc<AtomicBool>,
    /// How often the bridge opened a new WebSocket after a break (the game says hello again
    /// at once instead of waiting for the host to be missed).
    reconnects: Arc<AtomicUsize>,
}

impl Drop for WsClient {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl WsClient {
    /// One WebSocket to `url`, with the short read timeout the bridge loop needs (a dead
    /// tunnel must not hold the game for ever).
    fn open(url: &str) -> Result<WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        let u = url.to_string();
        std::thread::spawn(move || {
            let _ = tx.send(tungstenite::client::connect_with_config(&u, Some(datagram_config()), 3).map(|x| x.0).map_err(|e| format!("{u}: {e}")));
        });
        let mut ws = rx.recv_timeout(Duration::from_secs(12)).map_err(|_| format!("{url}: no answer within 12 s"))??;
        match ws.get_mut() {
            tungstenite::stream::MaybeTlsStream::Plain(s) => s.set_read_timeout(Some(Duration::from_millis(5))),
            tungstenite::stream::MaybeTlsStream::Rustls(s) => s.get_mut().set_read_timeout(Some(Duration::from_millis(5))),
            _ => Ok(()),
        }
        .map_err(|e| e.to_string())?;
        Ok(ws)
    }

    /// Connect to `url` (`wss://…/ws`) and give the local address to join. When the
    /// connection breaks later, the bridge opens a new one by itself and keeps the local
    /// address: the game's session finds its way back without a restart (the server knows
    /// the player again by its nonce).
    pub fn connect(url: &str) -> Result<WsClient, String> {
        let mut ws = Self::open(url)?;
        let udp = UdpSocket::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        udp.set_nonblocking(true).map_err(|e| e.to_string())?;
        let local = udp.local_addr().map_err(|e| e.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let alive = Arc::new(AtomicBool::new(true));
        let reconnects = Arc::new(AtomicUsize::new(0));
        let (st, al, rc) = (stop.clone(), alive.clone(), reconnects.clone());
        let url = url.to_string();
        std::thread::Builder::new()
            .name("ws client".into())
            .spawn(move || {
                // the game's own socket: replies go to wherever it sent from
                let game: Arc<Mutex<Option<SocketAddr>>> = Arc::new(Mutex::new(None));
                let udp2 = udp.try_clone().expect("udp clone");
                let g2 = game.clone();
                // (the game's datagrams are read in `pump`'s UDP half; the address they came
                // from is where the WebSocket's answers go)
                let mut buf = vec![0u8; 2048];
                let mut last_in = Instant::now();
                let mut last_ping = Instant::now();
                while !st.load(Ordering::Relaxed) {
                    let r: Result<(), String> = (|| {
                        while !st.load(Ordering::Relaxed) {
                            let mut idle = true;
                            loop {
                                match ws.read() {
                                    Ok(Message::Binary(d)) => {
                                        last_in = Instant::now();
                                        idle = false;
                                        if let Some(to) = *g2.lock().unwrap_or_else(|e| e.into_inner()) {
                                            let _ = udp2.send_to(&d, to);
                                        }
                                    }
                                    Ok(Message::Close(_)) => return Err("the server closed the connection".into()),
                                    Ok(_) => last_in = Instant::now(),
                                    Err(tungstenite::Error::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => break,
                                    Err(tungstenite::Error::ConnectionClosed) | Err(tungstenite::Error::AlreadyClosed) => return Err("the connection was closed".into()),
                                    Err(e) => return Err(e.to_string()),
                                }
                            }
                            loop {
                                match udp.recv_from(&mut buf) {
                                    Ok((n, from)) => {
                                        idle = false;
                                        *game.lock().unwrap_or_else(|e| e.into_inner()) = Some(from);
                                        if let Err(e) = ws.send(Message::Binary(buf[..n].to_vec().into())) {
                                            if !matches!(&e, tungstenite::Error::Io(io) if io.kind() == ErrorKind::WouldBlock) {
                                                return Err(e.to_string());
                                            }
                                        }
                                    }
                                    Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                                    Err(e) if e.kind() == ErrorKind::ConnectionReset => break,
                                    Err(e) => return Err(e.to_string()),
                                }
                            }
                            let _ = ws.flush();
                            if last_ping.elapsed() > Duration::from_secs(20) {
                                last_ping = Instant::now();
                                let _ = ws.send(Message::Ping(Vec::new().into()));
                            }
                            if last_in.elapsed() > Duration::from_secs(90) {
                                return Err("the server has been silent for 90 s".into());
                            }
                            if idle {
                                std::thread::sleep(Duration::from_millis(2));
                            }
                        }
                        let _ = ws.close(None);
                        Ok(())
                    })();
                    if st.load(Ordering::Relaxed) {
                        break;
                    }
                    if let Err(e) = r {
                        log::warn!("ws client {url}: {e}; connecting again");
                    }
                    // a new connection, with a pause that grows to a few seconds
                    let mut wait = 500u64;
                    loop {
                        if st.load(Ordering::Relaxed) {
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(wait));
                        match Self::open(&url) {
                            Ok(w) => {
                                ws = w;
                                // what the game said while the way was down is stale
                                while udp.recv_from(&mut buf).is_ok() {}
                                last_in = Instant::now();
                                last_ping = Instant::now();
                                log::info!("ws client {url}: connected again");
                                rc.fetch_add(1, Ordering::Relaxed);
                                break;
                            }
                            Err(e) => {
                                log::info!("ws client: still no connection ({e})");
                                wait = (wait * 2).min(5000);
                            }
                        }
                    }
                }
                al.store(false, Ordering::Relaxed);
            })
            .map_err(|e| e.to_string())?;
        log::info!("ws client: the session is reached through {local}");
        Ok(WsClient { local, stop, alive, reconnects })
    }

    /// How many times the connection was made again after a break.
    pub fn reconnects(&self) -> usize {
        self.reconnects.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        assert_eq!(ws_url("https://abc.trycloudflare.com").as_deref(), Some("wss://abc.trycloudflare.com/ws"));
        assert_eq!(ws_url("abc.trycloudflare.com").as_deref(), Some("wss://abc.trycloudflare.com/ws"));
        assert_eq!(ws_url("http://10.0.0.2:27025/").as_deref(), Some("ws://10.0.0.2:27025/ws"));
        assert_eq!(ws_url("192.168.1.4:27015"), None);
        assert_eq!(http_base("https://abc.trycloudflare.com").as_deref(), Some("https://abc.trycloudflare.com"));
        assert_eq!(web_bases("192.168.1.4:27015"), ["http://192.168.1.4:27015", "http://192.168.1.4:27025", "https://192.168.1.4"]);
        assert_eq!(web_bases("play.example.org"), ["http://play.example.org:27025", "https://play.example.org", "http://play.example.org"]);
        assert_eq!(web_bases("play.example.org:47015"), ["http://play.example.org:47015", "http://play.example.org:47025", "https://play.example.org", "http://play.example.org:27025"]);
        assert_eq!(web_bases("play.example.org:65535"), ["http://play.example.org:65535", "https://play.example.org", "http://play.example.org:27025"]);
        assert_eq!(web_bases("http://play.example.org:47015"), ["http://play.example.org:47015"]);
        assert_eq!(web_bases("https://play.example.org"), ["https://play.example.org"]);
        assert_eq!(web_bases("::1")[0], "http://[::1]:27025");
        assert_eq!(web_bases("[::1]:27025"), ["http://[::1]:27025", "http://[::1]:27035", "https://[::1]"]);
    }

    #[test]
    fn status_round_trip() {
        let i = ServerInfo { name: "Spandau \"1\"".into(), motd: "hi".into(), map: "maps/Berlin-Spandau/global.cfg".into(), players: 2, max_players: 16, version: "0.1".into(), icon: vec![1, 2], time: "08:00".into(), weather: "Sommerlich".into(), password: false, vehicles: vec!["Vehicles/MAN_SD202/SD202.bus".into()], ..Default::default() };
        let j = i.to_json();
        let b = ServerInfo::from_json(&j).unwrap();
        assert_eq!(b.name, i.name);
        assert_eq!(b.vehicles, i.vehicles);
        assert_eq!(b.players, 2);
        assert_eq!(b.max_players, 16);
        assert!(!b.icon.is_empty());
    }

    #[test]
    fn a_websocket_behind_a_proxy_is_told_by_the_forwarded_address() {
        let local: SocketAddr = "127.0.0.1:50000".parse().unwrap();
        let far: SocketAddr = "203.0.113.7:50000".parse().unwrap();
        let req = |h: &str| format!("GET / HTTP/1.1\r\nHost: x\r\n{h}Upgrade: websocket\r\n\r\n");
        let ip = |s: &str| Some(s.parse::<std::net::IpAddr>().unwrap());
        fn client_ip_x(h: &str, p: Option<SocketAddr>) -> Option<std::net::IpAddr> {
            client_ip(h, p, false)
        }
        assert_eq!(client_ip_x(&req("X-Forwarded-For: 10.0.0.1, 198.51.100.4\r\n"), Some(local)), ip("198.51.100.4"));
        assert_eq!(client_ip_x(&req("X-Real-IP: 2001:db8::1\r\n"), Some(local)), None);
        // (the proxy's own address, the last X-Forwarded-For: a CF-Connecting-IP through a
        // proxy on this machine may be the client's own)
        assert_eq!(client_ip_x(&req("CF-Connecting-IP: 192.0.2.9\r\nX-Forwarded-For: 198.51.100.4\r\n"), Some(local)), ip("198.51.100.4"));
        assert_eq!(client_ip_x(&req("X-Forwarded-For: 192.0.2.9\r\nX-Forwarded-For: 198.51.100.4\r\n"), Some(local)), ip("198.51.100.4"));
        assert_eq!(client_ip_x(&req("CF-Connecting-IP: 192.0.2.9\r\n"), Some(local)), ip("192.0.2.9"));
        // no proxy: the peer; a header from the internet is not believed
        assert_eq!(client_ip_x(&req(""), Some(local)), None);
        assert_eq!(client_ip_x(&req("X-Forwarded-For: 127.0.0.1\r\n"), Some(local)), None);
        assert_eq!(client_ip_x(&req("X-Forwarded-For: 198.51.100.4\r\n"), Some(far)), ip("203.0.113.7"));
        assert_eq!(client_ip_x(&req("X-Forwarded-For: not-an-address\r\n"), Some(local)), None);
        assert_eq!(client_ip_x(&req(""), None), None);
        // Cloudflare in front of a proxy of this machine (`gateway_proxy_header =
        // cf-connecting-ip`): the player, not the edge the last X-Forwarded-For names
        let both = req("CF-Connecting-IP: 192.0.2.9\r\nX-Forwarded-For: 192.0.2.9, 172.70.1.1\r\n");
        assert_eq!(client_ip(&both, Some(local), true), ip("192.0.2.9"));
        assert_eq!(client_ip(&both, Some(local), false), ip("172.70.1.1"));
        assert_eq!(client_ip(&req("X-Forwarded-For: 198.51.100.4\r\n"), Some(local), true), ip("198.51.100.4"));
        assert_eq!(client_ip(&req("CF-Connecting-IP: 192.0.2.9\r\n"), Some(far), true), ip("203.0.113.7"));
    }

    #[test]
    fn status_counts_a_servers_world() {
        let mut i = ServerInfo { name: "NEROSY".into(), players: 2, vehicles: vec!["Vehicles/A/a.bus".into()], ..Default::default() };
        assert!(i.to_json().ends_with(",\"world\":null}"));
        i.world = Some(WorldCounts { cars: 41, buses: 7, dormant: 12, parked: 230, walking: 55, waiting: 18, aboard: 9, traffic: 30, passengers: true, passenger_density: 1.5 });
        let j = i.to_json();
        assert!(j.ends_with(",\"world\":{\"cars\":41,\"buses\":7,\"dormant\":12,\"parked\":230,\"walking\":55,\"waiting\":18,\"aboard\":9,\"traffic\":30,\"passengers\":true,\"passenger_density\":1.50}}"), "{j}");
        // the launcher still reads the rest (the world's keys are none of the server's own)
        let back = ServerInfo::from_json(&j).unwrap();
        assert_eq!((back.name.as_str(), back.players, back.vehicles.len()), ("NEROSY", 2, 1));
    }

    #[test]
    fn local_admin_door_is_shut_to_tunnels() {
        let info = Mutex::new(ServerInfo { local_admin_password: "s3cret".into(), ..Default::default() });
        let req = b"POST /admin HTTP/1.1\r\nX-Admin-Password: s3cret\r\nCf-Connecting-Ip: 203.0.113.9\r\nContent-Length: 6\r\n\r\nsay hi";
        assert_eq!(local_admin(req, Some(SocketAddr::from(([127, 0, 0, 1], 5000))), &info).0, "403 Forbidden");
        assert!(info.lock().unwrap().local_admin_queue.is_empty());
    }

    #[test]
    fn local_admin_door() {
        let info = Mutex::new(ServerInfo::default());
        let here = Some(SocketAddr::from(([127, 0, 0, 1], 5000)));
        let post = |pw: &str, body: &str| format!("POST /admin HTTP/1.1\r\nHost: x\r\nX-Admin-Password: {pw}\r\nContent-Length: {}\r\n\r\n{body}", body.len()).into_bytes();
        // no password: no door
        assert_eq!(local_admin(&post("", "say hi"), here, &info).0, "404 Not Found");
        info.lock().unwrap().local_admin_password = "s3cret".into();
        // not from this machine
        assert_eq!(local_admin(&post("s3cret", "say hi"), Some(SocketAddr::from(([10, 0, 0, 2], 5000))), &info).0, "403 Forbidden");
        assert_eq!(local_admin(b"GET /admin HTTP/1.1\r\nX-Admin-Password: s3cret\r\n\r\n", here, &info).0, "405 Method Not Allowed");
        assert_eq!(local_admin(&post("s3cre", "say hi"), here, &info).0, "401 Unauthorized");
        assert!(info.lock().unwrap().local_admin_queue.is_empty());
        let (st, _) = local_admin(&post("s3cret", "clock 30600\r\n\r\nweather set Weather/#CAVOK.owt\n"), here, &info);
        assert_eq!(st, "202 Accepted");
        assert_eq!(info.lock().unwrap().local_admin_queue, ["clock 30600", "weather set Weather/#CAVOK.owt"]);
        // five wrong passwords close the door, the right one included
        for _ in 0..4 {
            local_admin(&post("nope", "say x"), here, &info);
        }
        assert_eq!(local_admin(&post("s3cret", "say x"), here, &info).0, "429 Too Many Requests");
        assert!(same_secret("abc", "abc") && !same_secret("abc", "abd") && !same_secret("abc", "abcd") && !same_secret("", "a"));
    }

    /// The whole head is read, however long (within its cap) and however it comes in pieces: a
    /// tunnel's mark pushed past the first read by a padded header still shuts the door, and a
    /// head past the cap is refused.
    #[test]
    fn the_whole_head_is_read() {
        let gw = WsGateway::start("127.0.0.1:0".parse().unwrap(), "127.0.0.1:9".parse().unwrap(), ServerInfo { local_admin_password: "s3cret".into(), ..Default::default() }).unwrap();
        let ask = |pieces: &[Vec<u8>]| {
            let mut c = TcpStream::connect(gw.addr).unwrap();
            c.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            for p in pieces {
                c.write_all(p).unwrap();
                c.flush().unwrap();
                std::thread::sleep(Duration::from_millis(60));
            }
            let mut out = String::new();
            let _ = c.read_to_string(&mut out);
            out
        };
        let pad = format!("X-Pad: {}\r\n", "a".repeat(3000));
        let start = format!("POST /admin HTTP/1.1\r\nHost: x\r\nX-Admin-Password: s3cret\r\n{pad}");
        let tunnel = b"Cf-Connecting-Ip: 203.0.113.9\r\nContent-Length: 6\r\n\r\nsay hi".to_vec();
        let out = ask(&[start.clone().into_bytes(), tunnel]);
        assert!(out.starts_with("HTTP/1.1 403"), "{out}");
        assert!(gw.info.lock().unwrap().local_admin_queue.is_empty());
        // the same, from this machine itself and in pieces: taken
        let out = ask(&[start.into_bytes(), b"Content-Length: 6\r\n".to_vec(), b"\r\nsay hi".to_vec()]);
        assert!(out.starts_with("HTTP/1.1 202"), "{out}");
        // past the cap
        let out = ask(&[format!("GET /status HTTP/1.1\r\nX-Pad: {}\r\n\r\n", "a".repeat(crate::ws_http::HEAD_MAX)).into_bytes()]);
        assert!(out.starts_with("HTTP/1.1 431"), "{out}");
        // who the client is, from the whole head: a proxy's mark past a padded first part
        let pad = format!("X-Pad: {}\r\n", "a".repeat(3000));
        let peeked = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let mut c = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
            let (srv, _) = listener.accept().unwrap();
            c.write_all(format!("GET /ws HTTP/1.1\r\nHost: x\r\n{pad}").as_bytes()).unwrap();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(60));
                let _ = c.write_all(b"X-Forwarded-For: 198.51.100.4\r\nUpgrade: websocket\r\n\r\n");
                std::thread::sleep(Duration::from_millis(500));
            });
            String::from_utf8_lossy(&peek_head(&srv, Instant::now() + Duration::from_secs(5))).to_string()
        };
        assert!(peeked.contains("Upgrade: websocket"), "{}", peeked.len());
        assert_eq!(client_ip(&peeked, Some(SocketAddr::from(([127, 0, 0, 1], 1))), false), Some("198.51.100.4".parse().unwrap()));
    }


    #[test]
    fn players_list() {
        let p = PlayerInfo { id: 3, name: "Anna \"A\"".into(), bus: "Vehicles/MAN_SD200/MAN_SD77.bus".into(), line: "37".into(), x: 894179.74, y: 4196165.3, heading: 200.0, speed_kmh: 31.25, lat_lon: Some((52.535412, 13.199642)), ..Default::default() };
        let j = players_json(&[p.clone(), PlayerInfo { id: 4, x: f64::NAN, ..Default::default() }]);
        assert!(j.starts_with("[{\"id\":3,\"name\":\"Anna \\\"A\\\"\""), "{j}");
        assert!(j.contains("\"line\":\"37\""), "{j}");
        assert!(j.contains("\"x\":894179.7,"), "{j}");
        assert!(j.contains("\"on_foot\":false,\"aboard\":null,\"lat\":52.535412,\"lon\":13.199642}"), "{j}");
        assert!(j.contains("\"id\":4,") && j.contains("\"x\":null") && j.ends_with("\"lat\":null,\"lon\":null}]"), "{j}");
        assert_eq!(players_json(&[]), "[]");
    }

    #[test]
    fn the_client_connects_again_when_the_way_breaks() {
        // a stand-in server: the first connection echoes one datagram and drops, the second
        // is a good one
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            for round in 0..2 {
                let Ok((s, _)) = l.accept() else { return };
                let Ok(mut ws) = tungstenite::accept(s) else { return };
                loop {
                    match ws.read() {
                        Ok(Message::Binary(d)) => {
                            let _ = ws.send(Message::Binary(d));
                            if round == 0 {
                                break;
                            }
                        }
                        Ok(_) => {}
                        Err(_) => break,
                    }
                }
            }
        });
        let client = WsClient::connect(&format!("ws://{addr}/ws")).unwrap();
        let game = UdpSocket::bind("127.0.0.1:0").unwrap();
        game.set_read_timeout(Some(Duration::from_millis(250))).unwrap();
        let mut got = [0u8; 64];
        let mut first = false;
        for _ in 0..20 {
            game.send_to(b"ONE", client.local).unwrap();
            if let Ok((n, _)) = game.recv_from(&mut got) {
                first = &got[..n] == b"ONE";
                break;
            }
        }
        assert!(first, "the first connection works");
        let mut again = false;
        for _ in 0..40 {
            game.send_to(b"TWO", client.local).unwrap();
            if let Ok((n, _)) = game.recv_from(&mut got) {
                if &got[..n] == b"TWO" {
                    again = true;
                    break;
                }
            }
        }
        assert!(again, "the datagram came back through the second connection");
        assert!(client.alive.load(Ordering::Relaxed));
        assert_eq!(client.reconnects(), 1);
    }

    /// Mod streams up to `MAX_TUNNELS`; one more is told `ERR busy` (and its game asks again),
    /// and a player's session still gets through with every stream taken.
    #[test]
    fn mod_streams_never_shut_a_player_out() {
        let session = UdpSocket::bind("127.0.0.1:0").unwrap();
        let target = session.local_addr().unwrap();
        // the mods server at the session's port: holds every stream open
        let mods = TcpListener::bind(("127.0.0.1", target.port())).unwrap();
        std::thread::spawn(move || {
            let mut held = Vec::new();
            for c in mods.incoming().flatten() {
                held.push(c);
            }
        });
        let gw = WsGateway::start("127.0.0.1:0".parse().unwrap(), target, ServerInfo::default()).unwrap();
        let forward = tcp_forward(&format!("ws://{}/tcp", gw.addr)).unwrap();
        let mut streams: Vec<TcpStream> = (0..MAX_TUNNELS).map(|_| TcpStream::connect(forward).unwrap()).collect();
        // (each through the gateway to the mods server before the next is counted)
        std::thread::sleep(Duration::from_millis(500));
        let mut one_more = TcpStream::connect(forward).unwrap();
        one_more.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut said = String::new();
        let _ = one_more.read_to_string(&mut said);
        assert_eq!(said, "ERR busy\n");
        // a player joins all the same
        let player = WsClient::connect(&format!("ws://{}/ws", gw.addr)).unwrap();
        let t = Instant::now();
        while gw.connected.load(Ordering::Relaxed) == 0 && t.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(gw.connected.load(Ordering::Relaxed), 1, "the player's session is carried");
        // and a stream that ends gives its place to the next
        drop(streams.pop());
        std::thread::sleep(Duration::from_millis(300));
        let mut next = TcpStream::connect(forward).unwrap();
        next.set_read_timeout(Some(Duration::from_millis(800))).unwrap();
        let mut b = [0u8; 16];
        assert!(matches!(next.read(&mut b), Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)), "carried, not refused");
        drop(player);
    }

    /// The players' places and the mod streams' are counted apart: with every player's place
    /// taken, one more status request is told to come back (`503`) - and a mod stream is still
    /// carried; once a player leaves, the requests get through again.
    #[test]
    fn players_never_take_the_mod_streams_places() {
        let session = UdpSocket::bind("127.0.0.1:0").unwrap();
        let target = session.local_addr().unwrap();
        let mods = TcpListener::bind(("127.0.0.1", target.port())).unwrap();
        std::thread::spawn(move || {
            let mut held = Vec::new();
            for c in mods.incoming().flatten() {
                held.push(c);
            }
        });
        let gw = WsGateway::start("127.0.0.1:0".parse().unwrap(), target, ServerInfo::default()).unwrap();
        let mut players: Vec<WsClient> = (0..MAX_ORDINARY).map(|_| WsClient::connect(&format!("ws://{}/ws", gw.addr)).unwrap()).collect();
        let t = Instant::now();
        while gw.connected.load(Ordering::Relaxed) < MAX_ORDINARY && t.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(gw.connected.load(Ordering::Relaxed), MAX_ORDINARY);
        let status = || {
            let mut s = TcpStream::connect(gw.addr).unwrap();
            s.write_all(b"GET /status HTTP/1.1\r\nHost: gateway\r\n\r\n").unwrap();
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut said = String::new();
            let _ = s.read_to_string(&mut said);
            said
        };
        let said = status();
        assert!(said.starts_with("HTTP/1.1 503"), "{said}");
        // a mod stream all the same: carried, not refused
        let forward = tcp_forward(&format!("ws://{}/tcp", gw.addr)).unwrap();
        let mut stream = TcpStream::connect(forward).unwrap();
        stream.set_read_timeout(Some(Duration::from_millis(800))).unwrap();
        let mut b = [0u8; 16];
        assert!(matches!(stream.read(&mut b), Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)), "the mod stream is carried");
        // a player who leaves gives their place back
        drop(players.pop());
        let t = Instant::now();
        while gw.connected.load(Ordering::Relaxed) >= MAX_ORDINARY && t.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(20));
        }
        std::thread::sleep(Duration::from_millis(100));
        let said = status();
        assert!(said.starts_with("HTTP/1.1 200"), "{said}");
    }

    #[test]
    fn a_message_larger_than_a_datagram_ends_the_websocket() {
        let gw = WsGateway::start("127.0.0.1:0".parse().unwrap(), "127.0.0.1:9".parse().unwrap(), ServerInfo::default()).unwrap();
        let (mut ws, _) = tungstenite::connect(format!("ws://{}/ws", gw.addr)).unwrap();
        if let tungstenite::stream::MaybeTlsStream::Plain(s) = ws.get_mut() {
            s.set_read_timeout(Some(Duration::from_millis(500))).unwrap();
        }
        ws.send(Message::Binary(vec![0u8; 64 * 1024].into())).unwrap();
        let t0 = Instant::now();
        let ended = loop {
            match ws.read() {
                Err(tungstenite::Error::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Ok(Message::Close(_)) | Err(_) => break true,
                Ok(_) => {}
            }
            if t0.elapsed() > Duration::from_secs(10) {
                break false;
            }
        };
        assert!(ended, "the gateway kept a 64 KiB message");
    }

    #[test]
    fn one_source_holds_only_so_many_connections() {
        let sources = Arc::new(Sources::default());
        let ip = |s: &str| s.parse::<std::net::IpAddr>().unwrap();
        let held: Vec<SourceSlot> = (0..PER_SOURCE).map(|_| sources.take(ip("198.51.100.4")).unwrap()).collect();
        assert!(sources.take(ip("198.51.100.4")).is_none());
        assert!(sources.take(ip("203.0.113.7")).is_some());
        drop(held);
        assert!(sources.take(ip("198.51.100.4")).is_some());
        let v6: Vec<SourceSlot> = (0..PER_SOURCE).map(|i| sources.take(ip(&format!("2001:db8:1:2::{i:x}"))).unwrap()).collect();
        assert!(sources.take(ip("2001:db8:1:2:ffff::9")).is_none());
        assert!(sources.take(ip("2001:db8:1:3::1")).is_some());
        drop(v6);
    }

    #[test]
    fn datagrams_go_both_ways() {
        // a stand-in session: echoes every datagram
        let echo = UdpSocket::bind("127.0.0.1:0").unwrap();
        let target = echo.local_addr().unwrap();
        std::thread::spawn(move || {
            let mut b = [0u8; 2048];
            loop {
                if let Ok((n, from)) = echo.recv_from(&mut b) {
                    let _ = echo.send_to(&b[..n], from);
                }
            }
        });
        let gw = WsGateway::start("127.0.0.1:0".parse().unwrap(), target, ServerInfo { name: "t".into(), ..Default::default() }).unwrap();
        let client = WsClient::connect(&format!("ws://{}/ws", gw.addr)).unwrap();
        let game = UdpSocket::bind("127.0.0.1:0").unwrap();
        game.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let mut got = [0u8; 64];
        let mut ok = false;
        for _ in 0..30 {
            game.send_to(b"HELLO", client.local).unwrap();
            if let Ok((n, _)) = game.recv_from(&mut got) {
                ok = &got[..n] == b"HELLO";
                break;
            }
        }
        assert!(ok, "the datagram came back through the WebSocket");
        let st = query(&format!("http://{}", gw.addr), false).unwrap();
        assert_eq!(st.name, "t");
        let st = query(&gw.addr.to_string(), false).unwrap();
        assert_eq!(st.name, "t");
        assert_eq!(st.reached_at, format!("http://{}", gw.addr));
        // the admin door over a real connection: a body sent after the head is read too
        gw.info.lock().unwrap().local_admin_password = "pw".into();
        let mut s = TcpStream::connect(gw.addr).unwrap();
        s.write_all(b"POST /admin HTTP/1.1\r\nHost: x\r\nX-Admin-Password: pw\r\nContent-Length: 11\r\n\r\n").unwrap();
        std::thread::sleep(Duration::from_millis(50));
        s.write_all(b"say hello\r\n").unwrap();
        let mut r = String::new();
        let _ = s.read_to_string(&mut r);
        assert!(r.starts_with("HTTP/1.1 202"), "{r}");
        assert_eq!(gw.info.lock().unwrap().local_admin_queue, ["say hello"]);
        // the players' positions only when the server shares them
        let get = |path: &str| {
            let mut s = TcpStream::connect(gw.addr).unwrap();
            s.write_all(format!("GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes()).unwrap();
            let mut r = String::new();
            let _ = s.read_to_string(&mut r);
            r
        };
        assert!(get("/players").starts_with("HTTP/1.1 404"));
        gw.info.lock().unwrap().players_public = true;
        gw.info.lock().unwrap().player_list = vec![PlayerInfo { id: 1, name: "p".into(), ..Default::default() }];
        let r = get("/players");
        assert!(r.starts_with("HTTP/1.1 200") && r.ends_with("\"lat\":null,\"lon\":null}]"), "{r}");
    }
}
