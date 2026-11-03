//! A dedicated server's direct address, found by itself (`server.cfg`'s `public_address`
//! left empty): a game joining through the tunnel tries it first (`gateway::ws_join_target`).
//!
//! A machine with an internet address of its own (on one of its adapters) is reached there.
//! One behind a router is reached at the address the internet sees (STUN) only if the router
//! forwards the web port - and never behind a carrier's NAT (CGNAT), where that address is
//! the carrier's. So it tells that one only if it reaches itself there; a router that cannot
//! loop back loses the shortcut, but nobody is sent to an address that does not answer.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

/// The web port, when the address is to be found (`auto`).
static WEB_PORT: OnceLock<u16> = OnceLock::new();
/// Found, or being tried: not again.
static STARTED: AtomicBool = AtomicBool::new(false);

/// Find the address by itself, on `web_port`.
pub(super) fn auto(web_port: u16) {
    let _ = WEB_PORT.set(web_port);
}

/// Called as the status is told: once the address can be known, it is (once).
pub(super) fn find(lan: &omsi_net::LanSession) {
    let Some(&port) = WEB_PORT.get() else { return };
    if STARTED.load(Ordering::Relaxed) {
        return;
    }
    let own = omsi_net::addrs::local_addresses().into_iter().find(|a| a.kind == omsi_net::addrs::AddrKind::Public).map(|a| a.ip);
    let seen = lan.public_addr().and_then(|a| match a.ip() {
        IpAddr::V4(ip) => Some(ip),
        IpAddr::V6(_) => None,
    });
    let Some(choice) = choose(own, seen) else { return };
    STARTED.store(true, Ordering::Relaxed);
    let addr = SocketAddr::from((choice.ip(), port));
    if let Choice::Own(_) = choice {
        log::info!("server: players joining through the tunnel try this machine's address {addr} first (public_address = off: never)");
        crate::lan::update_server_direct(&addr.to_string());
        return;
    }
    std::thread::spawn(move || {
        if omsi_net::ws::reachable(&format!("http://{addr}"), Duration::from_secs(3)) {
            log::info!("server: reached itself at {addr}: players joining through the tunnel try it first (public_address = off: never)");
            crate::lan::update_server_direct(&addr.to_string());
        } else {
            log::info!("server: not reached at {addr}, the address the internet sees (no forwarding of the web port, or a carrier's NAT): players join through the tunnel; public_address in server.cfg can name another");
        }
    });
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Choice {
    /// An internet address of this machine's own: told as it is.
    Own(Ipv4Addr),
    /// The one the internet sees behind a router: told if the server reaches itself there.
    Seen(Ipv4Addr),
}

impl Choice {
    fn ip(self) -> Ipv4Addr {
        match self {
            Choice::Own(ip) | Choice::Seen(ip) => ip,
        }
    }
}

/// Which address to tell: this machine's own internet address, else the one the internet
/// sees (to be tried first), else none yet.
fn choose(own: Option<Ipv4Addr>, seen: Option<Ipv4Addr>) -> Option<Choice> {
    // (a carrier's NAT range on an adapter is no internet address: 100.64.0.0/10)
    let cgnat = |ip: &Ipv4Addr| ip.octets()[0] == 100 && (64..128).contains(&ip.octets()[1]);
    let own = own.filter(|ip| !cgnat(ip));
    own.map(Choice::Own).or(seen.filter(|ip| !ip.is_private() && !ip.is_loopback() && !cgnat(ip)).map(Choice::Seen))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An internet address of the machine's own is told at once; behind a router the one the
    /// internet sees is only tried (the server must reach itself there); an address in a
    /// carrier's NAT range is none; nothing known yet, nothing told.
    #[test]
    fn the_machines_own_address_first_the_seen_one_only_tried() {
        let own = Ipv4Addr::new(203, 0, 113, 7);
        let seen = Ipv4Addr::new(198, 51, 100, 9);
        assert_eq!(choose(Some(own), Some(seen)), Some(Choice::Own(own)));
        assert_eq!(choose(None, Some(seen)), Some(Choice::Seen(seen)));
        assert_eq!(choose(Some(Ipv4Addr::new(100, 72, 1, 2)), Some(seen)), Some(Choice::Seen(seen)), "a CGNAT adapter is no internet address");
        assert_eq!(choose(None, Some(Ipv4Addr::new(100, 72, 1, 2))), None);
        assert_eq!(choose(None, Some(Ipv4Addr::new(192, 168, 1, 5))), None);
        assert_eq!(choose(None, None), None);
    }
}
