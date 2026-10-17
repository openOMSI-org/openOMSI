//! Join a server as a watcher with no vehicle and print what arrives of every player: the
//! bus, whether its content fingerprint came along, and how big the player's INFO is.
//!   cargo run -p omsi-net --example watch_players -- 127.0.0.1:27015 [seconds]
use omsi_net::{LanSession, Pose, WorldInfo};
use std::time::{Duration, Instant};

fn main() {
    let mut args = std::env::args().skip(1);
    let addr = args.next().unwrap_or_else(|| "127.0.0.1:27015".into());
    let secs: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(20);
    let addrs = std::net::ToSocketAddrs::to_socket_addrs(&addr).expect("address").collect();
    let world = WorldInfo { map: "maps/Grundorf/global.cfg".into(), ..Default::default() };
    let mut lan = LanSession::join_addr(addrs, None, "watcher", world).expect("join");
    let mine = Pose::default();
    let end = Instant::now() + Duration::from_secs(secs);
    let mut shown = std::collections::HashMap::new();
    while Instant::now() < end {
        lan.tick(0.05, &mine);
        for p in lan.peers().filter(|p| p.has_info) {
            let q = &p.pose;
            let line = format!(
                "player {} '{}' at ({:.1}, {:.1}, {:.2}) heading {:.1}: {} | paint '{}' | content '{}' | height {:.2} | {} texts, {} freetex",
                q.id,
                q.name,
                q.x,
                q.y,
                q.z,
                q.heading,
                q.bus,
                q.paint,
                q.content,
                q.height,
                q.texts.len(),
                q.freetex.len()
            );
            if shown.get(&q.id) != Some(&line) {
                println!("{line}");
                shown.insert(q.id, line);
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
