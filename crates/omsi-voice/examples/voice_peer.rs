//! One player of a voice chat smoke test, as a process of its own: it hosts a session or
//! joins one, talks on a key (or only listens), and says what it heard. Two of them over
//! the loopback test the whole chain - the engine, the relay on the host, the network - with
//! a synthetic tone for the microphone (`OMSI_VOICE_TEST_TONE`), no device and no person:
//!
//! ```text
//! OMSI_VOICE_TEST_TONE=440 cargo run -p omsi-voice --example voice_peer -- host 27500 12 listen 0
//! OMSI_VOICE_TEST_TONE=880 cargo run -p omsi-voice --example voice_peer -- join 27500 12 radio 40
//! ```
//!
//! Arguments: `host <port> <seconds> <talk> <x>` or `join <port> <seconds> <talk> <x>`, with
//! `talk` one of `nearby`, `radio`, `listen`, and `x` the player's place (m) on the map.

use omsi_net::{FLAG_VEHICLE, LanSession, Pose, WorldInfo};
use omsi_voice::{Channel, Frame, Settings, Voice};
use std::net::SocketAddr;
use std::time::{Duration, Instant};

fn main() {
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Warn)
        .try_init();
    let a: Vec<String> = std::env::args().collect();
    let (role, port, secs, talk, x) = (
        a.get(1).map(String::as_str).unwrap_or("host"),
        a.get(2)
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(27500),
        a.get(3).and_then(|v| v.parse::<u64>().ok()).unwrap_or(10),
        a.get(4).map(String::as_str).unwrap_or("listen"),
        a.get(5).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0),
    );
    let world = WorldInfo {
        map: "m".into(),
        date: "1989-05-30".into(),
        time: 32400.0,
        weather: String::new(),
        season: String::new(),
    };
    let mut lan = if role == "host" {
        LanSession::host(port, "host", world, false).expect("host")
    } else {
        LanSession::join_addr(
            vec![SocketAddr::from(([127, 0, 0, 1], port))],
            None,
            "guest",
            world,
        )
        .expect("join")
    };
    let pose = Pose {
        name: role.into(),
        bus: "Vehicles/x.bus".into(),
        x,
        y: 0.0,
        z: 0.0,
        flags: FLAG_VEHICLE,
        ..Default::default()
    };
    let voice = Voice::new();
    voice.set_settings(&Settings {
        enabled: true,
        ..Default::default()
    });
    let channel = match talk {
        "nearby" => Some(Channel::Nearby),
        "radio" => Some(Channel::Radio),
        _ => None,
    };
    let (mut sent, mut got) = (0u64, 0u64);
    let mut heard_levels: Vec<f32> = Vec::new();
    let mut heard_from = std::collections::BTreeSet::new();
    let start = Instant::now();
    let mut last = Instant::now();
    let mut asked = false;
    while start.elapsed() < Duration::from_secs(secs) {
        let dt = last.elapsed().as_secs_f32();
        last = Instant::now();
        lan.tick(dt, &pose);
        if lan.connected && !asked {
            asked = true;
            // (the key goes down once the session is up)
            voice.set_transmit(channel);
        }
        for o in voice.take_outgoing() {
            if !o.end {
                sent += 1;
            }
            lan.voice_send(o.channel.id(), o.end, o.seq, &o.payload);
        }
        for v in lan.take_voice() {
            got += 1;
            heard_from.insert((v.from, v.channel));
            if let Some(c) = Channel::from_id(v.channel) {
                voice.feed(Frame {
                    from: v.from,
                    channel: c,
                    seq: v.seq,
                    end: v.end,
                    payload: v.payload,
                });
            }
        }
        let st = voice.status();
        if let Some(t) = st.talkers.first() {
            heard_levels.push(t.level_db);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let max = heard_levels.iter().cloned().fold(-120.0f32, f32::max);
    println!(
        "RESULT role={role} connected={} sent={sent} received={got} decoded_ticks={} max_level_db={max:.1} heard_from={:?}",
        lan.connected,
        heard_levels.len(),
        heard_from
    );
}
