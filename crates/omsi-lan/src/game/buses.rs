//! The players' buses' step of [`tick_send`](super::tick_send), once a frame: a server
//! keeping its players' buses learns which are announced and in use, a joining game finds
//! the way to its host's mods, and our own bus is identified and sent (`mods::upload`).
use super::*;

/// The way to the host's mods is being found (`mods_route`, in the background).
static ROUTING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Where the host's mods connection is reached from here (`host` is the session's address):
/// that address, or - its TCP port out of reach (a router forwards the UDP session only, or
/// we came in over a WebSocket, whose "host" is a relay on this machine) - the local end of
/// a tunnel through its web gateway. None when there is no way. (Can wait a few seconds.)
fn mods_route(host: std::net::SocketAddr, session: u64) -> Option<std::net::SocketAddr> {
    let joined_url = crate::gateway::joined_url();
    if joined_url.is_none() && std::net::TcpStream::connect_timeout(&host, Duration::from_secs(3)).is_ok() {
        return Some(host);
    }
    let base = joined_url
        .or_else(|| omsi_net::bridge::lookup_tunnel(session).and_then(|u| omsi_net::ws::ws_url(&u)))
        // (joined straight over UDP: the host's own web port, as for the start's fetch)
        .or_else(|| crate::gateway::host_web_gateway(host))?;
    let tcp_url = format!("{}/tcp", base.strip_suffix("/ws").unwrap_or(&base));
    match omsi_net::ws::tcp_forward(&tcp_url) {
        Ok(local) => {
            log::info!("LAN mods: the host's TCP port is out of reach; through {tcp_url}");
            Some(local)
        }
        Err(e) => {
            log::warn!("LAN mods: {e}");
            Some(host)
        }
    }
}

/// `mine`: our pose this frame; `player`: our vehicle, when we drive one.
pub(super) fn tick(lan: &mut LanSession, args: &LanConfig, mine: &Pose, player: Option<&omsi_sim::VehicleInstance>) {
    // a server that keeps its players' buses: the ones its players announce now are the
    // ones on their way (`vehicles::wait_for` waits for no other), every second
    if lan.role == Role::Host && crate::mods::upload::accepting() {
        static SAID: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);
        let mut said = SAID.lock().unwrap_or_else(|e| e.into_inner());
        if said.is_none_or(|t| t.elapsed() > Duration::from_secs(1)) {
            *said = Some(Instant::now());
            crate::mods::vehicles::announce(lan.peers().map(|p| p.pose.content.as_str()).chain(std::iter::once(mine.content.as_str())).filter(|c| omsi_net::is_fingerprint(c)));
        }
    }
    // a server that keeps its players' buses: the ones driven now are in use
    if lan.role == Role::Host && crate::mods::upload::accepting() {
        static SEEN: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);
        let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
        if seen.is_none_or(|t| t.elapsed() > Duration::from_secs(60)) {
            *seen = Some(Instant::now());
            // (and the hosting game's own, kept for the others)
            let fps: Vec<String> = lan.peers().map(|p| p.pose.content.clone()).chain(std::iter::once(mine.content.clone())).filter(|c| omsi_net::is_fingerprint(c)).collect();
            crate::mods::vehicles::in_use(fps.iter().map(|s| s.as_str()));
        }
    }
    // (a joining game knows its host for the buses by content, however its start went)
    // (by the same way as the start's fetch would have: through the web gateway's tunnel when
    // we came in over a WebSocket - its "host" is only a relay on this machine)
    if let (Role::Client, Some(host), Some(_)) = (lan.role, lan.host, lan.welcome.as_ref()) {
        let session = lan.session;
        if !crate::mods::upload::knows_host(session) && !ROUTING.swap(true, std::sync::atomic::Ordering::Relaxed) {
            std::thread::spawn(move || {
                if let Some(route) = mods_route(host, session) {
                    crate::mods::upload::session_host(route, session);
                }
                ROUTING.store(false, std::sync::atomic::Ordering::Relaxed);
            });
        }
    }
    // what our bus is, by its content (`mods::identity`), for whoever draws it
    crate::mods::upload::identify(&args.root, &mine.bus, &mine.paint, crate::mods::upload::own_files(&args.root, player));
    // our bus to a server that lacks it, and the others' buses it passes on (`mods::upload`);
    // a hosting game keeps its own bus for the players who join it
    if lan.role == Role::Client {
        crate::mods::upload::tick(&args.root, &mine.bus, &mine.paint, crate::mods::upload::own_files(&args.root, player));
    } else if !args.server {
        crate::mods::upload::keep_own(&args.root, &mine.bus, &mine.paint, crate::mods::upload::own_files(&args.root, player));
    }
}
