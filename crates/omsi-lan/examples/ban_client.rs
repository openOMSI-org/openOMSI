//! A player for trying the machine bans against a running `admin_demo` from another process:
//!
//! ```text
//! cargo run -p omsi-lan --example ban_client -- <udp port> <name> <machine> [seconds]
//! ```
//! It joins `127.0.0.1:<udp port>` as `<name>`, from the machine called `<machine>` (the
//! same word gives the same hardware identifier, as one computer would), and prints one line
//! when it is let in (`JOINED id`), sent away (`KICKED reason`) or turned away
//! (`TURNED_AWAY reason`), and `DONE` when `seconds` (30) are over.

use omsi_net::{LanSession, Pose, WorldInfo};
use std::time::{Duration, Instant};

fn main() {
    let mut a = std::env::args().skip(1);
    let port = a.next().expect("udp port");
    let name = a.next().expect("name");
    let machine = a.next().expect("machine");
    let secs: u64 = a.next().and_then(|s| s.parse().ok()).unwrap_or(30);
    unsafe { std::env::set_var("OMSI_NO_BRIDGE", "1") };
    let mut lan = LanSession::join(
        &format!("127.0.0.1:{port}"),
        &name,
        WorldInfo::default(),
        Duration::from_millis(10),
    )
    .expect("a socket");
    lan.hardware_id = Some(omsi_lan::bans::derive_hardware_id(&format!(
        "demo-{machine}"
    )));
    let start = Instant::now();
    let mut joined = false;
    while start.elapsed() < Duration::from_secs(secs) {
        lan.tick(0.05, &Pose::default());
        let _ = lan.take_commands();
        if let Some(why) = lan.turned_away.clone() {
            println!("TURNED_AWAY {why}");
            break;
        }
        if lan.connected && !joined && lan.my_id != 0 {
            joined = true;
            println!("JOINED {}", lan.my_id);
        }
        for e in lan.take_events() {
            if let omsi_net::LanEvent::Notice(n) = e {
                if n.contains("sent away") || n.contains("banned") {
                    println!("NOTICE {n}");
                }
            }
        }
        if joined && !lan.connected {
            println!("KICKED {}", lan.turned_away.clone().unwrap_or_default());
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    println!("DONE");
}
