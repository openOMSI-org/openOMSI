//! The session code as a bridge over the internet: two players behind home routers find
//! each other without a VPN.
//!
//! * STUN (RFC 5389): the session's own UDP socket asks public STUN servers what address
//!   the router gave it on the internet (`public`). The same datagrams keep that mapping
//!   open while the session runs.
//! * UPnP: the host asks its router to forward its port (most home routers do), which
//!   makes it reachable from anywhere at its public address.
//! * A rendezvous: the host posts its addresses under a topic named after the session id on
//!   a public message relay (ntfy.sh); a joining game posts its own under the topic's
//!   `-c` twin and reads the host's. The host then sends a few datagrams to the joining
//!   game's public address while the latter says hello to the host's - the two routers
//!   see traffic both ways and let it through (UDP hole punching). Only who wants to meet
//!   whom goes over the relay: addresses, never the game.
//!
//! What is left out: two routers that both change their port for every destination
//! (symmetric NAT, some mobile networks) cannot be punched through; a dedicated server
//! (see docs/SERVER.md) or a VPN is the way there.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, ToSocketAddrs, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Public STUN servers (any one answering is enough).
const STUN_SERVERS: &[&str] = &[
    "stun.l.google.com:19302",
    "stun.cloudflare.com:3478",
    "stun1.l.google.com:19302",
];
/// The message relay the rendezvous goes through.
const RELAY: &str = "https://ntfy.sh";
const STUN_MAGIC: u32 = 0x2112_A442;
/// How often the NAT mapping is refreshed (routers forget an idle UDP mapping after 30 s
/// or more).
const STUN_EVERY: f32 = 20.0;
/// A punching datagram: the receiver drops it (see `is_bridge_packet`).
pub const PUNCH: &[u8] = b"\xFFPUNCH";

/// What the bridge found out, shared between the session and the relay thread.
#[derive(Default)]
pub struct Shared {
    /// Our address on the internet (STUN), and the one the router forwards (UPnP).
    pub public: Option<SocketAddr>,
    pub forwarded: Option<SocketAddr>,
    /// Host: joining games' addresses to punch towards.
    pub punch: Vec<SocketAddr>,
    /// Client: the host's addresses as it posted them.
    pub host_addrs: Vec<SocketAddr>,
    /// What the relay said last (for the status line).
    pub note: String,
    /// The router's port forwarding we asked for (UPnP), to be taken back at the end.
    pub mapping: Option<Mapping>,
}

/// A port forwarding the router made for us: its control address, service and port.
#[derive(Debug, Clone)]
pub struct Mapping {
    url: String,
    service: String,
    port: u16,
}

pub struct Bridge {
    pub shared: Arc<Mutex<Shared>>,
    stun: Vec<SocketAddr>,
    stun_acc: f32,
    txn: [u8; 12],
    /// Addresses being punched towards, until when.
    punching: Vec<(SocketAddr, Instant)>,
    punch_acc: f32,
    stop: Arc<std::sync::atomic::AtomicBool>,
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        // the router's forwarding goes with the session (it stayed open for its two-hour
        // lease after every game)
        if let Some(m) = self
            .shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .mapping
            .take()
        {
            upnp_remove(&m);
        }
    }
}

/// The relay topic of a session: a hash of the whole session id, so neither the relay nor
/// anyone reading the topic learns the id (it guards the session's mods, see `lan_mods`).
fn topic(session: u64) -> String {
    use sha2::{Digest, Sha256};
    let h = Sha256::new()
        .chain_update(b"omsi2rw-topic")
        .chain_update(session.to_le_bytes())
        .finalize();
    format!("omsi2rw-{}", hex(&h[..12]))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// HMAC-SHA-256 of a relay post under a key only holders of the session code can derive:
/// a post without it (anybody else writing to the public topic) is ignored.
fn mac(session: u64, text: &str) -> String {
    use sha2::{Digest, Sha256};
    let key = Sha256::new()
        .chain_update(b"omsi2rw-relay-key")
        .chain_update(session.to_le_bytes())
        .finalize();
    let mut k = [0u8; 64];
    k[..32].copy_from_slice(&key);
    let pad = |c: u8| k.map(|b| b ^ c);
    let inner = Sha256::new()
        .chain_update(pad(0x36))
        .chain_update(text.as_bytes())
        .finalize();
    let outer = Sha256::new()
        .chain_update(pad(0x5c))
        .chain_update(inner)
        .finalize();
    hex(&outer[..16])
}

/// `text` followed by its MAC.
fn signed(session: u64, text: &str) -> String {
    format!("{text} #{}", mac(session, text))
}

/// The text of a relay post whose MAC is right, else None.
fn verified(session: u64, msg: &str) -> Option<&str> {
    let (text, m) = msg.rsplit_once(" #")?;
    (mac(session, text) == m.trim()).then_some(text)
}

/// At most this many joining games' addresses are punched towards at a time.
const MAX_PUNCH: usize = 32;

/// Whether a datagram belongs to the bridge (a STUN answer or a punch), not the game.
pub fn is_bridge_packet(data: &[u8]) -> bool {
    data.starts_with(PUNCH)
        || (data.len() >= 20
            && u32::from_be_bytes([data[4], data[5], data[6], data[7]]) == STUN_MAGIC)
}

impl Bridge {
    /// Start the bridge of a session: `host` posts its addresses and punches towards the
    /// joining games; a client posts its own and learns the host's. `local` lists this
    /// machine's own addresses (the LAN ones) with the session's port. Set
    /// `OMSI_NO_BRIDGE=1` to leave the internet alone (tests, a LAN party).
    pub fn start(
        host: bool,
        session: u64,
        key: Option<Arc<crate::hostkey::HostKey>>,
        local: Vec<SocketAddr>,
        port: u16,
    ) -> Option<Bridge> {
        if cfg!(test) || omsi_cfg::flags::OMSI_NO_BRIDGE.live_os().is_some() {
            return None;
        }
        let shared = Arc::new(Mutex::new(Shared::default()));
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (sh, st) = (shared.clone(), stop.clone());
        let spawned = std::thread::Builder::new()
            .name("lan bridge".into())
            .spawn(move || {
                if host {
                    if let Some((fwd, mapping)) = upnp_forward(port) {
                        log::info!("LAN bridge: the router forwards {fwd} to this computer (UPnP)");
                        let mut g = sh.lock().unwrap_or_else(|e| e.into_inner());
                        g.forwarded = Some(fwd);
                        g.mapping = Some(mapping);
                    }
                }
                relay_loop(host, session, key.as_deref(), local, &sh, &st);
            });
        if spawned.is_err() {
            return None;
        }
        // (resolved once; the servers' addresses do not change during a session)
        let stun: Vec<SocketAddr> = STUN_SERVERS
            .iter()
            .filter_map(|s| {
                s.to_socket_addrs()
                    .ok()
                    .and_then(|mut a| a.find(|x| x.is_ipv4()))
            })
            .collect();
        Some(Bridge {
            shared,
            stun,
            stun_acc: STUN_EVERY,
            txn: {
                let r = crate::random_session_id().to_le_bytes();
                let mut t = [0u8; 12];
                t[..8].copy_from_slice(&r);
                t[8..].copy_from_slice(&(crate::random_session_id() as u32).to_le_bytes());
                t
            },
            punching: Vec::new(),
            punch_acc: 0.0,
            stop,
        })
    }

    /// Every tick of the session: keep the NAT mapping, punch towards the players that asked.
    pub fn tick(&mut self, dt: f32, socket: &UdpSocket) {
        self.stun_acc += dt;
        if self.stun_acc >= STUN_EVERY {
            self.stun_acc = 0.0;
            let mut req = Vec::with_capacity(20);
            req.extend_from_slice(&0x0001u16.to_be_bytes()); // binding request
            req.extend_from_slice(&0u16.to_be_bytes());
            req.extend_from_slice(&STUN_MAGIC.to_be_bytes());
            req.extend_from_slice(&self.txn);
            for s in &self.stun {
                let _ = socket.send_to(&req, s);
            }
        }
        let now = Instant::now();
        {
            let mut sh = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            for a in sh.punch.drain(..) {
                if self.punching.len() < MAX_PUNCH && !self.punching.iter().any(|(b, _)| *b == a) {
                    log::info!("LAN bridge: opening the way to {a}");
                    self.punching.push((a, now + Duration::from_secs(15)));
                }
            }
        }
        self.punching.retain(|(_, until)| *until > now);
        self.punch_acc += dt;
        if self.punch_acc >= 0.25 && !self.punching.is_empty() {
            self.punch_acc = 0.0;
            for (a, _) in &self.punching {
                let _ = socket.send_to(PUNCH, a);
            }
        }
    }

    /// A datagram of the bridge's own (see `is_bridge_packet`): a STUN answer tells our
    /// public address.
    pub fn receive(&mut self, data: &[u8]) {
        if data.len() < 20 || data[8..20] != self.txn[..] {
            return;
        }
        if let Some(addr) = parse_mapped(data) {
            let mut sh = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            if sh.public != Some(addr) {
                log::info!("LAN bridge: this computer is {addr} on the internet (STUN)");
                sh.public = Some(addr);
            }
        }
    }

    pub fn public(&self) -> Option<SocketAddr> {
        let sh = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        sh.forwarded.or(sh.public)
    }

    /// Client: the host's addresses the relay has told so far.
    pub fn host_addrs(&self) -> Vec<SocketAddr> {
        self.shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .host_addrs
            .clone()
    }
}

/// XOR-MAPPED-ADDRESS (or MAPPED-ADDRESS) of a STUN binding success answer.
fn parse_mapped(d: &[u8]) -> Option<SocketAddr> {
    if u16::from_be_bytes([d[0], d[1]]) != 0x0101 {
        return None;
    }
    let len = u16::from_be_bytes([d[2], d[3]]) as usize;
    let mut i = 20;
    let end = (20 + len).min(d.len());
    let mut plain = None;
    while i + 4 <= end {
        let t = u16::from_be_bytes([d[i], d[i + 1]]);
        let l = u16::from_be_bytes([d[i + 2], d[i + 3]]) as usize;
        let v = &d[i + 4..(i + 4 + l).min(end)];
        if v.len() >= 8 && v[1] == 1 {
            let port = u16::from_be_bytes([v[2], v[3]]);
            let ip = [v[4], v[5], v[6], v[7]];
            match t {
                0x0020 => {
                    let m = STUN_MAGIC.to_be_bytes();
                    let port = port ^ (STUN_MAGIC >> 16) as u16;
                    let ip = Ipv4Addr::new(ip[0] ^ m[0], ip[1] ^ m[1], ip[2] ^ m[2], ip[3] ^ m[3]);
                    return Some(SocketAddr::V4(SocketAddrV4::new(ip, port)));
                }
                0x0001 => plain = Some(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::from(ip), port))),
                _ => {}
            }
        }
        i += 4 + ((l + 3) & !3);
    }
    plain
}

/// The relay side, on its own thread: post our addresses, read the other side's.
fn relay_loop(
    host: bool,
    session: u64,
    key: Option<&crate::hostkey::HostKey>,
    local: Vec<SocketAddr>,
    sh: &Arc<Mutex<Shared>>,
    stop: &std::sync::atomic::AtomicBool,
) {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout_read(Duration::from_secs(8))
        .user_agent("openOMSI")
        .build();
    let (mine, theirs) = if host {
        (topic(session), format!("{}-c", topic(session)))
    } else {
        (format!("{}-c", topic(session)), topic(session))
    };
    let mut last_post: Option<(Instant, String)> = None;
    // (the host reposts every quarter of an hour: a joining game reads the last half hour)
    let mut since = "30m".to_string();
    // ntfy.sh answers too many requests from one address with 429 for a while - and counts
    // the messages of a day: the host asked every second and posted every 30 s, and after an
    // hour or two of hosting the relay shut it out: nobody could join any more. It asks every
    // few seconds now (a joining game more often, for the minute and a half it looks), posts
    // only what changed or every 15 minutes, and waits longer after each refusal.
    let mut backoff = Duration::ZERO;
    let mut last_poll: Option<Instant> = None;
    let mut seen: std::collections::HashSet<String> = Default::default();
    let started = Instant::now();
    let mut renewed = Instant::now();
    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
        // the router's forwarding, asked again before its lease runs out
        if host && renewed.elapsed() > RENEW_EVERY {
            renewed = Instant::now();
            let port = sh
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .mapping
                .as_ref()
                .map(|m| m.port);
            if let Some(port) = port {
                match upnp_forward(port) {
                    Some((fwd, mapping)) => {
                        let mut g = sh.lock().unwrap_or_else(|e| e.into_inner());
                        g.forwarded = Some(fwd);
                        g.mapping = Some(mapping);
                    }
                    None => log::info!(
                        "LAN bridge: the router did not renew the forwarding of port {port}"
                    ),
                }
            }
        }
        // a client stops once it is in (it asks again after a lost connection by being
        // started anew); a host keeps its door open for the whole session
        if !host && started.elapsed() > Duration::from_secs(90) {
            break;
        }
        let (public, forwarded) = {
            let s = sh.lock().unwrap_or_else(|e| e.into_inner());
            (s.public, s.forwarded)
        };
        let mut addrs: Vec<SocketAddr> = forwarded.into_iter().chain(public).collect();
        addrs.extend(local.iter().copied());
        addrs.dedup();
        // (the player's nonce is its identity towards the host and never leaves the
        // session: anyone reading the topic could otherwise take its place)
        let text = addrs
            .iter()
            .take(8)
            .map(|a| a.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let due = match &last_post {
            None => true,
            Some((t, prev)) => {
                *prev != text || t.elapsed() > Duration::from_secs(if host { 900 } else { 10 })
            }
        };
        // (a client posts once it knows its public address, or after 3 s without it)
        let ready = host || public.is_some() || started.elapsed() > Duration::from_secs(3);
        let waiting = last_poll.is_some_and(|t| t.elapsed() < backoff);
        if due && ready && !addrs.is_empty() && !waiting {
            let post = match (host, key) {
                (true, Some(k)) => k.seal("relay", &format!("H {} {text}", crate::hostkey::now())),
                (true, None) => String::new(),
                (false, _) => signed(session, &format!("C - {text}")),
            };
            match if post.is_empty() {
                Ok(())
            } else {
                agent
                    .post(&format!("{RELAY}/{mine}"))
                    .set("Cache", "yes")
                    .send_string(&post)
                    .map(|_| ())
            } {
                Ok(_) => last_post = Some((Instant::now(), text.clone())),
                Err(e) => {
                    sh.lock().unwrap_or_else(|e| e.into_inner()).note =
                        format!("relay unreachable ({e})");
                    // (tried again soon, not after the full quarter of an hour)
                    last_post = Some((
                        Instant::now() - Duration::from_secs(if host { 840 } else { 5 }),
                        text.clone(),
                    ));
                    backoff = refused(backoff);
                    last_poll = Some(Instant::now());
                }
            }
        }
        // the other side's posts
        let every = Duration::from_secs(if host { 6 } else { 2 }).max(backoff);
        let poll_due = last_poll.is_none_or(|t| t.elapsed() >= every);
        let polled = if poll_due {
            last_poll = Some(Instant::now());
            match agent
                .get(&format!("{RELAY}/{theirs}/json?poll=1&since={since}"))
                .call()
            {
                Ok(r) => {
                    backoff = Duration::ZERO;
                    Some(r)
                }
                Err(e) => {
                    backoff = refused(backoff);
                    log::debug!(
                        "LAN bridge: relay: {e} (next try in {:.0} s)",
                        backoff.as_secs_f32()
                    );
                    None
                }
            }
        } else {
            None
        };
        if let Some(resp) = polled {
            let body = resp.into_string().unwrap_or_default();
            for line in body.lines() {
                let Some(msg) = json_field(line, "message") else {
                    continue;
                };
                if let Some(id) = json_field(line, "id") {
                    if !seen.insert(id.clone()) {
                        continue;
                    }
                    since = id;
                }
                let Some((kind, found)) = relay_post(host, session, &msg, crate::hostkey::now())
                else {
                    continue;
                };
                let mut s = sh.lock().unwrap_or_else(|e| e.into_inner());
                if host && kind == "C" {
                    if s.punch.len() < MAX_PUNCH {
                        s.punch.extend(found);
                    }
                } else if !host && kind == "H" {
                    for a in found {
                        if !s.host_addrs.contains(&a) {
                            s.host_addrs.push(a);
                        }
                    }
                    s.note = "found the host through the relay".into();
                }
            }
        }
        for _ in 0..10 {
            if stop.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

fn relay_post(
    host: bool,
    session: u64,
    msg: &str,
    now: u64,
) -> Option<(&'static str, Vec<SocketAddr>)> {
    let (kind, list) = if host {
        let text = verified(session, msg)?;
        ("C", text.strip_prefix("C ")?.split_once(' ')?.1)
    } else {
        let text = crate::hostkey::opened(session, "relay", msg)?;
        let (at, list) = text.strip_prefix("H ")?.split_once(' ')?;
        if !crate::hostkey::fresh(at.parse().ok()?, now, crate::hostkey::MAX_AGE) {
            return None;
        }
        ("H", list)
    };
    Some((
        kind,
        list.split(',')
            .filter_map(|a| a.trim().parse().ok())
            .take(8)
            .collect(),
    ))
}

/// The wait after the relay refused or failed: 10 s, doubled each time up to 5 minutes.
fn refused(was: Duration) -> Duration {
    (was * 2).clamp(Duration::from_secs(10), Duration::from_secs(300))
}

/// Host: tell joining games of `session` the WebSocket address its session is also reached
/// at (a Cloudflare tunnel, see `ws`): for players whose routers cannot be punched through.
/// Posted under the session's topic as `W <time> <url>`, signed with the host's key; call again every minute or so.
pub fn post_tunnel(key: &crate::hostkey::HostKey, url: &str) {
    if cfg!(test) || omsi_cfg::flags::OMSI_NO_BRIDGE.live_os().is_some() {
        return;
    }
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(6))
        .user_agent("openOMSI")
        .build();
    let post = key.seal("relay", &format!("W {} {url}", crate::hostkey::now()));
    if let Err(e) = agent
        .post(&format!("{RELAY}/{}", topic(key.session())))
        .set("Cache", "yes")
        .send_string(&post)
    {
        log::warn!("LAN bridge: the tunnel address could not be posted: {e}");
    }
}

/// Joining game: the WebSocket address the host of `session` posted last (see
/// `post_tunnel`), if any within the last hours.
pub fn lookup_tunnel(session: u64) -> Option<String> {
    if cfg!(test) || omsi_cfg::flags::OMSI_NO_BRIDGE.live_os().is_some() {
        return None;
    }
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(8))
        .user_agent("openOMSI")
        .build();
    let body = agent
        .get(&format!("{RELAY}/{}/json?poll=1&since=6h", topic(session)))
        .call()
        .ok()?
        .into_string()
        .ok()?;
    tunnel_of(session, &body, crate::hostkey::now())
}

fn tunnel_of(session: u64, body: &str, now: u64) -> Option<String> {
    body.lines()
        .filter_map(|l| json_field(l, "message"))
        .filter_map(|m| {
            let (at, url) = crate::hostkey::opened(session, "relay", &m)?
                .strip_prefix("W ")?
                .split_once(' ')?;
            let at: u64 = at.parse().ok()?;
            let url = url.trim();
            (crate::hostkey::fresh(at, now, crate::hostkey::MAX_AGE)
                && url.starts_with("https://")
                && url.len() < 300
                && !url.contains(char::is_whitespace))
            .then(|| (at, url.to_string()))
        })
        .max_by_key(|(at, _)| *at)
        .map(|(_, u)| u)
}

/// A string field of a flat JSON object (the relay's answers need no more than this).
pub(crate) fn json_field(line: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\":\"");
    let start = line.find(&pat)? + pat.len();
    let mut out = String::new();
    let mut chars = line[start..].chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => {
                if let Some(n) = chars.next() {
                    out.push(match n {
                        'n' => '\n',
                        't' => '\t',
                        other => other,
                    });
                }
            }
            c => out.push(c),
        }
    }
    None
}

/// How long the router keeps a forwarding we asked for (s). It is asked again well before
/// that for as long as the session runs: the forwarding used to be asked once for two hours,
/// and after two hours of hosting nobody could reach the game any more.
const LEASE: u32 = 3600;
/// How often the forwarding is asked again.
const RENEW_EVERY: Duration = Duration::from_secs(20 * 60);

/// Ask the router (UPnP IGD) to forward UDP `port` to this computer; the address it is
/// reachable at from the internet then.
fn upnp_forward(port: u16) -> Option<(SocketAddr, Mapping)> {
    let sock = UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    sock.set_read_timeout(Some(Duration::from_millis(1500)))
        .ok()?;
    let search = "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 1\r\nST: urn:schemas-upnp-org:device:InternetGatewayDevice:1\r\n\r\n";
    sock.send_to(search.as_bytes(), "239.255.255.250:1900")
        .ok()?;
    let mut buf = [0u8; 2048];
    let gateway = default_gateway();
    let deadline = Instant::now() + Duration::from_secs(3);
    let (router, location) = loop {
        if Instant::now() > deadline {
            return None;
        }
        let (n, from) = sock.recv_from(&mut buf).ok()?;
        let reply = String::from_utf8_lossy(&buf[..n]).to_string();
        let location = reply.lines().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim()
                .eq_ignore_ascii_case("location")
                .then(|| v.trim().to_string())
        });
        if let (SocketAddr::V4(f), Some(loc)) = (from, location) {
            if router_ip(*f.ip())
                && gateway.is_none_or(|g| g == *f.ip())
                && http_host(&loc) == Some(*f.ip())
            {
                break (from, loc);
            }
        }
        log::info!("LAN bridge: an answer to the router search from {from} was not taken");
    };
    let router_v4 = match router {
        SocketAddr::V4(v) => *v.ip(),
        SocketAddr::V6(_) => return None,
    };
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(4))
        .build();
    let desc = agent.get(&location).call().ok()?.into_string().ok()?;
    // the WAN connection service and its control address
    let (service, control) = [
        "urn:schemas-upnp-org:service:WANIPConnection:1",
        "urn:schemas-upnp-org:service:WANIPConnection:2",
        "urn:schemas-upnp-org:service:WANPPPConnection:1",
    ]
    .iter()
    .find_map(|svc| {
        let at = desc.find(svc)?;
        let rest = &desc[at..];
        let c0 = rest.find("<controlURL>")? + "<controlURL>".len();
        let c1 = rest[c0..].find("</controlURL>")? + c0;
        Some((svc.to_string(), rest[c0..c1].trim().to_string()))
    })?;
    let base = {
        let after = location.find("://").map(|i| i + 3).unwrap_or(0);
        let end = location[after..]
            .find('/')
            .map(|i| i + after)
            .unwrap_or(location.len());
        location[..end].to_string()
    };
    let url = if control.contains("://") {
        control
    } else {
        format!(
            "{base}{}{control}",
            if control.starts_with('/') { "" } else { "/" }
        )
    };
    if http_host(&url) != Some(router_v4) {
        log::info!(
            "LAN bridge: the router's control address {url:?} is not the router's own; no forwarding asked"
        );
        return None;
    }
    // our address towards the router
    let probe = UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    probe.connect(router).ok()?;
    let me = match probe.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(v) => v,
        _ => return None,
    };
    let soap = |action: &str, args: &str| -> Option<String> {
        let body = format!(
            "<?xml version=\"1.0\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body><u:{action} xmlns:u=\"{service}\">{args}</u:{action}></s:Body></s:Envelope>"
        );
        agent
            .post(&url)
            .set("Content-Type", "text/xml; charset=\"utf-8\"")
            .set("SOAPAction", &format!("\"{service}#{action}\""))
            .send_string(&body)
            .ok()?
            .into_string()
            .ok()
    };
    soap(
        "AddPortMapping",
        &format!(
            "<NewRemoteHost></NewRemoteHost><NewExternalPort>{port}</NewExternalPort><NewProtocol>UDP</NewProtocol><NewInternalPort>{port}</NewInternalPort><NewInternalClient>{me}</NewInternalClient><NewEnabled>1</NewEnabled><NewPortMappingDescription>openOMSI</NewPortMappingDescription><NewLeaseDuration>{LEASE}</NewLeaseDuration>"
        ),
    )?;
    // the same port over TCP: the host's mods go to the joining players that way (with the
    // UDP port alone forwarded they timed out and were never fetched)
    let _ = soap(
        "AddPortMapping",
        &format!(
            "<NewRemoteHost></NewRemoteHost><NewExternalPort>{port}</NewExternalPort><NewProtocol>TCP</NewProtocol><NewInternalPort>{port}</NewInternalPort><NewInternalClient>{me}</NewInternalClient><NewEnabled>1</NewEnabled><NewPortMappingDescription>openOMSI mods</NewPortMappingDescription><NewLeaseDuration>{LEASE}</NewLeaseDuration>"
        ),
    );
    let mapping = Mapping {
        url: url.clone(),
        service: service.clone(),
        port,
    };
    let ext = soap("GetExternalIPAddress", "")?;
    let a = ext.find("<NewExternalIPAddress>")? + "<NewExternalIPAddress>".len();
    let b = ext[a..].find('<')? + a;
    let ip: Ipv4Addr = ext[a..b].trim().parse().ok()?;
    // a router behind another (carrier NAT) forwards to an address nobody outside reaches
    if !public_ipv4(ip) {
        upnp_remove(&mapping);
        return None;
    }
    Some((SocketAddr::V4(SocketAddrV4::new(ip, port)), mapping))
}

fn default_gateway() -> Option<Ipv4Addr> {
    let found = gateway_of_system();
    found.filter(|g| router_ip(*g))
}

#[cfg(target_os = "linux")]
fn gateway_of_system() -> Option<Ipv4Addr> {
    let table = std::fs::read_to_string("/proc/net/route").ok()?;
    table.lines().skip(1).find_map(|l| {
        let f: Vec<&str> = l.split_whitespace().collect();
        (f.len() > 2 && f[1] == "00000000").then(|| {
            u32::from_str_radix(f[2], 16)
                .ok()
                .map(|g| Ipv4Addr::from(g.swap_bytes()))
        })?
    })
}

#[cfg(any(target_os = "macos", target_os = "freebsd"))]
fn gateway_of_system() -> Option<Ipv4Addr> {
    let out = std::process::Command::new("route")
        .args(["-n", "get", "default"])
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).lines().find_map(|l| {
        l.trim()
            .strip_prefix("gateway:")
            .and_then(|g| g.trim().parse().ok())
    })
}

#[cfg(windows)]
fn gateway_of_system() -> Option<Ipv4Addr> {
    use std::os::windows::process::CommandExt;
    let out = std::process::Command::new("route")
        .args(["print", "-4", "0.0.0.0"])
        .creation_flags(0x0800_0000)
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).lines().find_map(|l| {
        let f: Vec<&str> = l.split_whitespace().collect();
        (f.len() >= 3 && f[0] == "0.0.0.0" && f[1] == "0.0.0.0").then(|| f[2].parse().ok())?
    })
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "freebsd",
    windows
)))]
fn gateway_of_system() -> Option<Ipv4Addr> {
    None
}

fn router_ip(ip: Ipv4Addr) -> bool {
    (ip.is_private() || ip.is_link_local()) && !ip.is_loopback()
}

fn http_host(url: &str) -> Option<Ipv4Addr> {
    let rest = url.trim().strip_prefix("http://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = match authority.rsplit_once(':') {
        Some((h, p)) if p.parse::<u16>().is_ok() => h,
        _ => authority,
    };
    let ip: Ipv4Addr = host.parse().ok()?;
    router_ip(ip).then_some(ip)
}

fn public_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, _, _] = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || a == 0
        || a >= 240
        || (a == 100 && (64..128).contains(&b)))
}

/// Take a port forwarding back (UDP and TCP), briefly: the game is ending.
fn upnp_remove(m: &Mapping) {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(2))
        .build();
    for proto in ["UDP", "TCP"] {
        let body = format!(
            "<?xml version=\"1.0\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body><u:DeletePortMapping xmlns:u=\"{}\"><NewRemoteHost></NewRemoteHost><NewExternalPort>{}</NewExternalPort><NewProtocol>{proto}</NewProtocol></u:DeletePortMapping></s:Body></s:Envelope>",
            m.service, m.port
        );
        let ok = agent
            .post(&m.url)
            .set("Content-Type", "text/xml; charset=\"utf-8\"")
            .set(
                "SOAPAction",
                &format!("\"{}#DeletePortMapping\"", m.service),
            )
            .send_string(&body)
            .is_ok();
        log::info!(
            "LAN bridge: the router's {proto} forwarding of port {} {}",
            m.port,
            if ok {
                "was taken back"
            } else {
                "could not be taken back"
            }
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stun_answer_is_read() {
        // a binding success with XOR-MAPPED-ADDRESS 203.0.113.5:40000
        let txn = [7u8; 12];
        let mut d = vec![0x01, 0x01, 0, 12];
        d.extend_from_slice(&STUN_MAGIC.to_be_bytes());
        d.extend_from_slice(&txn);
        let m = STUN_MAGIC.to_be_bytes();
        let port = 40000u16 ^ (STUN_MAGIC >> 16) as u16;
        d.extend_from_slice(&[0x00, 0x20, 0, 8, 0, 1]);
        d.extend_from_slice(&port.to_be_bytes());
        d.extend_from_slice(&[203 ^ m[0], 0 ^ m[1], 113 ^ m[2], 5 ^ m[3]]);
        assert!(is_bridge_packet(&d));
        assert_eq!(parse_mapped(&d), Some("203.0.113.5:40000".parse().unwrap()));
    }

    #[test]
    fn only_the_router_itself_is_asked_for_a_forwarding() {
        let r: Ipv4Addr = "192.168.1.1".parse().unwrap();
        assert!(router_ip(r));
        assert_eq!(http_host("http://192.168.1.1:5000/rootDesc.xml"), Some(r));
        assert_eq!(http_host("http://192.168.1.1/ctl/IPConn"), Some(r));
        assert_eq!(http_host("http://127.0.0.1:8080/admin"), None);
        assert_eq!(http_host("http://203.0.113.9/desc.xml"), None);
        assert_eq!(http_host("https://192.168.1.1/desc.xml"), None);
        assert_eq!(http_host("http://evil@192.168.1.1/desc.xml"), None);
        assert_eq!(http_host("http://router.lan/desc.xml"), None);
        assert_eq!(http_host("file:///etc/passwd"), None);
        assert!(public_ipv4("81.2.3.4".parse().unwrap()));
        for bad in [
            "10.0.0.1",
            "127.0.0.1",
            "100.64.1.1",
            "0.0.0.0",
            "255.255.255.255",
            "224.0.0.1",
            "169.254.1.1",
            "203.0.113.5",
        ] {
            assert!(!public_ipv4(bad.parse().unwrap()), "{bad}");
        }
    }

    #[test]
    fn relay_lines_are_read() {
        let l = r#"{"id":"abc123","time":1,"event":"message","topic":"t","message":"H 00000000000000ff 1.2.3.4:27015,192.168.1.2:27015"}"#;
        assert_eq!(json_field(l, "id").as_deref(), Some("abc123"));
        assert_eq!(
            json_field(l, "message").as_deref(),
            Some("H 00000000000000ff 1.2.3.4:27015,192.168.1.2:27015")
        );
    }
}

#[cfg(test)]
mod relay_tests {
    use super::*;

    #[test]
    fn a_refusing_relay_is_asked_less_and_less_often() {
        let mut b = Duration::ZERO;
        let mut waits = Vec::new();
        for _ in 0..7 {
            b = super::refused(b);
            waits.push(b.as_secs());
        }
        assert_eq!(waits, vec![10, 20, 40, 80, 160, 300, 300]);
        // and the forwarding is asked again well within its lease
        assert!(super::RENEW_EVERY.as_secs() * 2 < super::LEASE as u64);
    }

    #[test]
    fn relay_posts_are_signed_by_the_session() {
        let post = signed(0xABCDEF, "W 0 https://a.trycloudflare.com");
        assert_eq!(
            verified(0xABCDEF, &post),
            Some("W 0 https://a.trycloudflare.com")
        );
        // another session's key, a changed text, a post without a MAC
        assert_eq!(verified(0xABCDEE, &post), None);
        assert_eq!(verified(0xABCDEF, &post.replace("a.try", "b.try")), None);
        assert_eq!(verified(0xABCDEF, "W 0 https://evil.example"), None);
        // the topic tells nothing of the session id
        assert!(!topic(0xABCDEF).contains("abcdef"));
    }

    fn relay_line(msg: &str) -> String {
        format!("{{\"id\":\"x\",\"event\":\"message\",\"message\":\"{msg}\"}}")
    }

    #[test]
    fn only_the_host_key_of_the_code_announces_the_host() {
        let key = crate::hostkey::HostKey::generate().unwrap();
        let other = crate::hostkey::HostKey::generate().unwrap();
        let s = key.session();
        let real = key.seal("relay", "H 1000 1.2.3.4:27015");
        assert_eq!(
            relay_post(false, s, &real, 1000),
            Some(("H", vec!["1.2.3.4:27015".parse().unwrap()]))
        );
        assert!(relay_post(false, s, &real, 1000 + 7300).is_some());
        assert_eq!(relay_post(false, s, &real, 1000 + 25 * 3600), None);
        assert_eq!(
            relay_post(false, s, &other.seal("relay", "H 1000 6.6.6.6:27015"), 1000),
            None
        );
        assert_eq!(
            relay_post(false, s, &signed(s, "H - 6.6.6.6:27015"), 1000),
            None
        );
        assert_eq!(relay_post(true, s, &real, 1000), None);
        assert_eq!(
            relay_post(true, s, &signed(s, "C - 5.6.7.8:4000"), 1000),
            Some(("C", vec!["5.6.7.8:4000".parse().unwrap()]))
        );
    }

    #[test]
    fn a_tunnel_is_taken_only_from_the_host_key_and_the_newest_wins() {
        let key = crate::hostkey::HostKey::generate().unwrap();
        let other = crate::hostkey::HostKey::generate().unwrap();
        let s = key.session();
        let body = [
            relay_line(&key.seal("relay", "W 1000 https://old.trycloudflare.com")),
            relay_line(&key.seal("relay", "W 2000 https://new.trycloudflare.com")),
            relay_line(&other.seal("relay", "W 3000 https://evil.trycloudflare.com")),
            relay_line(&signed(s, "W 0 https://evil2.trycloudflare.com")),
            relay_line(&key.seal("relay", "W 1500 http://plain.example")),
        ]
        .join("\n");
        assert_eq!(
            tunnel_of(s, &body, 2100).as_deref(),
            Some("https://new.trycloudflare.com")
        );
        assert_eq!(
            tunnel_of(s, &body, 2000 + 2 * 3600).as_deref(),
            Some("https://new.trycloudflare.com")
        );
        assert_eq!(tunnel_of(s, &body, 2000 + 25 * 3600), None);
        assert_eq!(
            tunnel_of(other.session(), &body, 3000).as_deref(),
            Some("https://evil.trycloudflare.com")
        );
    }
}
