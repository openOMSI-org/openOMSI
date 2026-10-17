//! A load test of a server: `n` players that join it, each with a bus the server has, start
//! at a place on its roads (`x,y`: the server's log names one, "first neighbouring lane pair
//! at (x, y)") and then drive where its AI cars drive (each bot takes one car's place, frame
//! by frame, so they spread over the roads the server simulates). Every 10 s: how many
//! players each bot sees and what the server sends them all (the bytes they receive).
//!   cargo run --release -p omsi-net --example load_bots -- 127.0.0.1:27015 30 600 493655,2979715 "Vehicles/MAN_SD200/MAN_SD80.bus;Vehicles/MAN_SD200/MAN_SD77.bus"
//! A bus given as `path@v1-...` is announced by that fingerprint: the server draws the bus it
//! keeps by that content (one a player sent), as it does for a real player; `path~paint`
//! drives it in that paint (`~`, not `#`: a bus file name may start with `#`); `name=path...`
//! names the bot that drives it (else `bot00`...). The start may say `x,y,z,heading` (degrees): the bots line up
//! one behind the other in that direction, 16 m apart, at that height; a sixth argument
//! `stand` keeps them there instead of following the AI cars. `trail:<player>` hides them
//! under the map until that player has driven far enough, then parks them along the way the
//! player drove (25 m from where it began, 16 m apart): on the road, at its height and bends.
use omsi_net::{LanSession, Pose, WorldInfo};
use std::time::{Duration, Instant};

struct Bot {
    lan: LanSession,
    pose: Pose,
    /// The AI car it drives in place of (none yet: it stands where it started).
    car: Option<u32>,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let addr = args.next().unwrap_or_else(|| "127.0.0.1:27015".into());
    let n: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(10);
    let secs: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(300);
    let start_at: Vec<f64> = args.next().unwrap_or_default().split(',').filter_map(|v| v.trim().parse().ok()).collect();
    let (x0, y0) = (start_at.first().copied().unwrap_or(0.0), start_at.get(1).copied().unwrap_or(0.0));
    let (z0, heading) = (start_at.get(2).copied().unwrap_or(0.0), start_at.get(3).copied());
    let buses: Vec<String> = args.next().unwrap_or_else(|| "Vehicles/MAN_SD200/MAN_SD80.bus".into()).split(';').map(str::to_string).collect();
    let mode = args.next().unwrap_or_default();
    let leader = mode.strip_prefix("trail:").map(str::to_string);
    let stand = mode == "stand" || leader.is_some();
    // (x, y, z, heading, how far along) of the way the leader drove
    let mut trail: Vec<(f64, f64, f64, f32, f64)> = Vec::new();
    let mut parked = false;
    let need = 25.0 + 16.0 * n.max(1).saturating_sub(1) as f64 + 8.0;
    let addrs: Vec<std::net::SocketAddr> = std::net::ToSocketAddrs::to_socket_addrs(&addr).expect("address").collect();
    let mut bots: Vec<Bot> = Vec::new();
    let (start, dt) = (Instant::now(), 1.0 / 30.0);
    let (mut last, mut last_bytes, mut joined) = (Instant::now(), 0u64, Instant::now() - Duration::from_secs(9));
    while start.elapsed() < Duration::from_secs(secs) {
        let step = Instant::now();
        // one more every 2 s (thirty at once would be no players' way of coming)
        if bots.len() < n && joined.elapsed() >= Duration::from_secs(2) {
            let i = bots.len();
            // (`name=` before a bus names the bot that drives it - what it tests, say)
            let given = &buses[i % buses.len()];
            let (name, given) = match given.split_once('=').filter(|(n, _)| !n.is_empty() && !n.contains(['/', '\\'])) {
                Some((n, b)) => (n.to_string(), b.to_string()),
                None => (format!("bot{i:02}"), given.clone()),
            };
            let lan = LanSession::join_addr(addrs.clone(), None, &name, WorldInfo::default()).expect("join");
            // (a few metres apart, as buses at a stop)
            let (spec, paint) = given.rsplit_once('~').map(|(b, p)| (b.to_string(), p.to_string())).unwrap_or_else(|| (given.clone(), String::new()));
            let (bus, content) = spec.split_once('@').map(|(b, f)| (b.to_string(), f.to_string())).unwrap_or_else(|| (spec.clone(), String::new()));
            // (in a row along `heading`, the first a little ahead of the start: a player
            // who starts there is not inside one of them)
            let (x, y, h) = match heading {
                Some(h) => {
                    let (s, c) = h.to_radians().sin_cos();
                    let d = 25.0 + 16.0 * i as f64;
                    (x0 + s * d, y0 + c * d, h as f32)
                }
                None => (x0 + 15.0 * i as f64, y0, 0.0),
            };
            // (under the map until there is a way to park them on)
            let z = if leader.is_some() { -1000.0 } else { z0 };
            let pose = Pose { name, bus, content, paint, length: 12.0, width: 2.5, height: 3.0, x, y, z, heading: h, flags: omsi_net::FLAG_VEHICLE, ..Default::default() };
            bots.push(Bot { lan, pose, car: None });
            joined = Instant::now();
            if bots.len() == n {
                println!("{:>4} s: all {n} bots joined {addr}", start.elapsed().as_secs());
            }
        }
        if let (Some(name), false) = (&leader, parked) {
            let seen = bots.first().and_then(|b| b.lan.peers().find(|p| p.pose.name == *name).map(|p| (p.pose.x, p.pose.y, p.pose.z, p.pose.heading)));
            if let Some((x, y, z, h)) = seen {
                match trail.last().copied() {
                    None => trail.push((x, y, z, h, 0.0)),
                    Some(t) => {
                        let d = ((x - t.0).powi(2) + (y - t.1).powi(2)).sqrt();
                        if d >= 1.0 {
                            trail.push((x, y, z, h, t.4 + d));
                        }
                    }
                }
            }
            if bots.len() == n && trail.last().is_some_and(|t| t.4 >= need) {
                for (i, b) in bots.iter_mut().enumerate() {
                    let d = 25.0 + 16.0 * i as f64;
                    let k = trail.iter().position(|t| t.4 >= d).unwrap_or(trail.len() - 1).max(1);
                    let (a, c) = (trail[k - 1], trail[k]);
                    let f = if c.4 > a.4 { ((d - a.4) / (c.4 - a.4)).clamp(0.0, 1.0) } else { 0.0 };
                    (b.pose.x, b.pose.y, b.pose.z, b.pose.heading) = (a.0 + (c.0 - a.0) * f, a.1 + (c.1 - a.1) * f, a.2 + (c.2 - a.2) * f, c.3);
                }
                parked = true;
                println!("{:>4} s: parked along {need:.0} m of {name}'s way", start.elapsed().as_secs());
            }
        }
        for (i, b) in bots.iter_mut().enumerate() {
            for (_, frame) in b.lan.take_world() {
                if stand {
                    continue;
                }
                let car = match b.car {
                    Some(id) => frame.cars.iter().find(|c| c.id == id),
                    // the i-th car it hears of, so that the bots spread
                    None => frame.cars.get(i % frame.cars.len().max(1)),
                };
                if let Some(c) = car {
                    b.car = Some(c.id);
                    (b.pose.x, b.pose.y, b.pose.z, b.pose.heading, b.pose.speed_kmh) = (c.x, c.y, c.z, c.heading, c.speed * 3.6);
                } else if frame.gone.iter().any(|&(person, id)| !person && Some(id) == b.car) {
                    b.car = None;
                }
            }
            b.lan.tick(dt as f32, &b.pose);
        }
        if last.elapsed() >= Duration::from_secs(10) && leader.is_some() && !parked {
            println!("{:>4} s: {:.0} m of {need:.0} m driven", start.elapsed().as_secs(), trail.last().map(|t| t.4).unwrap_or(0.0));
        }
        if last.elapsed() >= Duration::from_secs(10) {
            let bytes: u64 = bots.iter().map(|b| b.lan.received).sum();
            let seen: Vec<usize> = bots.iter().map(|b| b.lan.peers().filter(|p| p.has_info).count()).collect();
            let driving = bots.iter().filter(|b| b.car.is_some()).count();
            println!(
                "{:>4} s: {driving}/{n} driving, they see {}..{} players, the server sends them {:.0} kB/s",
                start.elapsed().as_secs(),
                seen.iter().min().unwrap_or(&0),
                seen.iter().max().unwrap_or(&0),
                (bytes - last_bytes) as f64 / 1024.0 / last.elapsed().as_secs_f64()
            );
            (last, last_bytes) = (Instant::now(), bytes);
        }
        std::thread::sleep(Duration::from_secs_f64(dt).saturating_sub(step.elapsed()));
    }
    for b in &bots {
        b.lan.leave();
    }
}
