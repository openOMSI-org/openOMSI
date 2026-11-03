//! A dedicated server's direct address, found by itself (`server.cfg`'s `public_address`
//! left empty): a game joining through the tunnel tries it first (`gateway::ws_join_target`).
//!
//! A machine with an internet address of its own (on one of its adapters) is reached there.
//! One behind a router is reached at the address the internet sees (STUN) only if the router
//! forwards the web port - and never behind a carrier's NAT (CGNAT), where that address is
//! the carrier's. So it tells that one only if it reaches itself there; a router that cannot
//! loop back loses the shortcut, but nobody is sent to an address that does not answer.

use omsi_net::addrs::{direct_choice, DirectChoice};
use std::net::{IpAddr, SocketAddr};
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
    let Some(choice) = direct_choice(own, seen) else { return };
    STARTED.store(true, Ordering::Relaxed);
    let addr = SocketAddr::from((choice.ip(), port));
    if let DirectChoice::Own(_) = choice {
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
