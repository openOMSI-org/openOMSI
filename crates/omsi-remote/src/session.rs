use crate::layouts::{Scope, MAX_LAYOUT};
use crate::pairing::Pairing;
use crate::{json, lock, Command, Shared};
use serde_json::{json as j, Value};
use std::io::ErrorKind;
use std::net::{IpAddr, TcpStream};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::protocol::WebSocketConfig;
use tungstenite::{Message, WebSocket};

const MAX_CLIENTS: usize = 8;
const MAX_HELD: usize = 32;
const HELLO_WITHIN: Duration = Duration::from_secs(10);
const RELEASE_AFTER: Duration = Duration::from_secs(4);
const SILENT_FOR: Duration = Duration::from_secs(30);
const PING_EVERY: Duration = Duration::from_secs(1);
const MAX_BACKLOG: usize = 4 << 20;

type Ws = WebSocket<TcpStream>;

enum Got {
    Text(String),
    Nothing,
    Closed,
}

fn receive(ws: &mut Ws, last_in: &mut Instant) -> Result<Got, String> {
    match ws.read() {
        Ok(Message::Text(t)) => {
            *last_in = Instant::now();
            Ok(Got::Text(t.to_string()))
        }
        Ok(Message::Close(_)) => Ok(Got::Closed),
        Ok(_) => Ok(Got::Nothing),
        Err(tungstenite::Error::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => Ok(Got::Nothing),
        Err(tungstenite::Error::ConnectionClosed) | Err(tungstenite::Error::AlreadyClosed) => Ok(Got::Closed),
        Err(e) => Err(e.to_string()),
    }
}

fn send(ws: &mut Ws, text: &str) -> Result<(), String> {
    match ws.send(Message::Text(text.into())) {
        Ok(()) => Ok(()),
        Err(tungstenite::Error::Io(e)) if e.kind() == ErrorKind::WouldBlock => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

fn authenticate(ws: &mut Ws, shared: &Shared, from: IpAddr) -> Result<bool, String> {
    let since = Instant::now();
    let mut last_in = Instant::now();
    while since.elapsed() < HELLO_WITHIN && !shared.stop.load(Ordering::Relaxed) {
        let text = match receive(ws, &mut last_in)? {
            Got::Text(t) => t,
            Got::Closed => return Ok(false),
            Got::Nothing => {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }
        };
        let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        let field = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let answer = match field("t").as_str() {
            "hello" if lock(&shared.devices).knows(&field("token")) => return Ok(true),
            "hello" => j!({ "t": "denied", "reason": "token" }),
            "pair" if !shared.pairing_open.load(Ordering::Relaxed) => j!({ "t": "denied", "reason": "closed" }),
            "pair" => {
                let pin = lock(&shared.pin).clone();
                let result = lock(&shared.devices).pair(&pin, &field("pin"), &field("name"), from);
                match result {
                    Pairing::Paired(token) => {
                        shared.new_pin();
                        shared.paired.fetch_add(1, Ordering::Relaxed);
                        send(ws, &j!({ "t": "paired", "token": token }).to_string())?;
                        return Ok(true);
                    }
                    Pairing::Wrong { renew } => {
                        if renew {
                            shared.new_pin();
                        }
                        j!({ "t": "denied", "reason": "pin" })
                    }
                    Pairing::Locked => j!({ "t": "denied", "reason": "locked" }),
                }
            }
            _ => continue,
        };
        send(ws, &answer.to_string())?;
        let _ = ws.flush();
    }
    Ok(false)
}

fn handle(ws: &mut Ws, text: &str, shared: &Shared, id: u64) -> Result<(), String> {
    let Ok(v) = serde_json::from_str::<Value>(text) else { return Ok(()) };
    let t = v.get("t").and_then(Value::as_str).unwrap_or("");
    if t == "ping" {
        send(ws, &j!({ "t": "pong", "id": v.get("id").cloned().unwrap_or(Value::Null) }).to_string())?;
        let _ = ws.flush();
        return Ok(());
    }
    if t == "layout.save" || t == "layout.reset" {
        let Some(scope) = v.get("scope").and_then(Value::as_str).and_then(Scope::parse) else { return Ok(()) };
        let Some(key) = lock(&shared.vehicle).as_ref().map(|x| x.key.clone()) else { return Ok(()) };
        let done = if t == "layout.save" {
            v.get("layout").is_some_and(|l| shared.store.save(&key, scope, l))
        } else {
            shared.store.reset(&key, scope);
            true
        };
        send(ws, &j!({ "t": "layout.done", "ok": done }).to_string())?;
        if done {
            shared.rebuild_vehicle();
        } else {
            log::info!("remote cockpit: a layout for {key} was refused");
        }
        return Ok(());
    }
    if t == "drive" {
        if let Some((steering, throttle, brake)) = json::drive(&v) {
            let mut drive = lock(&shared.drive);
            if drive.is_none_or(|d| d.session == id || d.at.elapsed() > crate::STEER_FRESH) {
                *drive = Some(crate::Drive { session: id, at: Instant::now(), steering, throttle, brake });
            }
        }
        return Ok(());
    }
    let Some(cmd) = json::command(&v) else { return Ok(()) };
    if let Command::Action { name, down } = &cmd {
        let mut held = lock(&shared.held);
        let mine = held.entry(id).or_default();
        let was = mine.iter().position(|h| h == name);
        match (*down, was) {
            (true, Some(_)) | (false, None) => return Ok(()),
            (true, None) if mine.len() >= MAX_HELD => return Ok(()),
            (true, None) => mine.push(name.clone()),
            (false, Some(i)) => {
                mine.swap_remove(i);
            }
        }
    }
    shared.send(cmd);
    Ok(())
}

fn release(shared: &Shared, id: u64) {
    let mut drive = lock(&shared.drive);
    if drive.is_some_and(|d| d.session == id) {
        *drive = None;
    }
    drop(drive);
    let names = lock(&shared.held).remove(&id).unwrap_or_default();
    for name in names {
        shared.send(Command::Action { name, down: false });
    }
}

fn pump(ws: &mut Ws, shared: &Shared, id: u64) -> Result<(), String> {
    let generation = shared.generation.load(Ordering::Relaxed);
    let (mut texts, mut vehicle, mut state, mut sign) = (0u64, 0u64, 0u64, u64::MAX);
    let mut sent: Option<std::sync::Arc<Value>> = None;
    let mut channels: Vec<(&'static str, u64)> = Vec::new();
    let mut last_in = Instant::now();
    let mut last_ping = Instant::now();
    while !shared.stop.load(Ordering::Relaxed) {
        if shared.generation.load(Ordering::Relaxed) != generation {
            let _ = send(ws, &j!({ "t": "denied", "reason": "token" }).to_string());
            break;
        }
        let mut idle = true;
        loop {
            match receive(ws, &mut last_in)? {
                Got::Text(t) => {
                    idle = false;
                    handle(ws, &t, shared, id)?;
                }
                Got::Nothing => break,
                Got::Closed => return Ok(()),
            }
        }
        let (mut out, now): (Vec<std::sync::Arc<String>>, Option<std::sync::Arc<Value>>) = {
            let f = shared.feed();
            let mut out = Vec::new();
            if f.texts_seq != texts && !f.texts.is_empty() {
                texts = f.texts_seq;
                out.push(f.texts.clone());
            }
            if f.vehicle_seq != vehicle {
                vehicle = f.vehicle_seq;
                out.push(f.vehicle_msg.clone());
                sent = None;
            }
            for (name, msg, seq) in &f.channels {
                match channels.iter_mut().find(|c| c.0 == *name) {
                    Some(c) if c.1 == *seq => {}
                    Some(c) => {
                        c.1 = *seq;
                        out.push(msg.clone());
                    }
                    None => {
                        channels.push((name, *seq));
                        out.push(msg.clone());
                    }
                }
            }
            let now = (f.state_seq != state && !f.state.is_null()).then(|| {
                state = f.state_seq;
                f.state.clone()
            });
            (out, now)
        };
        let sign_now = shared.sign_seq.load(Ordering::Relaxed);
        if sign_now != sign {
            sign = sign_now;
            let has = lock(&shared.sign).is_some();
            out.push(std::sync::Arc::new(j!({ "t": "sign", "v": sign_now, "has": has }).to_string()));
        }
        if let Some(now) = now {
            if let Some(delta) = json::state_delta(sent.as_deref(), &now) {
                out.push(std::sync::Arc::new(delta));
            }
            sent = Some(now);
        }
        for m in &out {
            idle = false;
            send(ws, m)?;
        }
        if !out.is_empty() {
            let _ = ws.flush();
        }
        if last_ping.elapsed() > PING_EVERY {
            last_ping = Instant::now();
            let _ = ws.send(Message::Ping(Vec::new().into()));
        }
        if last_in.elapsed() > RELEASE_AFTER {
            release(shared, id);
        }
        if last_in.elapsed() > SILENT_FOR {
            return Err("nothing heard for half a minute".into());
        }
        if idle {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    let _ = ws.close(None);
    Ok(())
}

pub(crate) fn local_host(host: &str) -> bool {
    let name = host.rsplit_once(':').filter(|(h, p)| !h.ends_with(':') && p.bytes().all(|b| b.is_ascii_digit())).map_or(host, |(h, _)| h);
    let name = name.trim_start_matches('[').trim_end_matches(']');
    let lower = name.to_ascii_lowercase();
    let local = [".local", ".lan", ".home.arpa", ".internal", ".localdomain"].iter().any(|s| lower.ends_with(s));
    !lower.is_empty() && (!lower.contains('.') || local || name.parse::<IpAddr>().is_ok())
}

fn same_origin(req: &Request) -> bool {
    let header = |n: &str| req.headers().get(n).and_then(|v| v.to_str().ok());
    match (header("origin"), header("host")) {
        (None, _) => true,
        (Some(origin), Some(host)) => local_host(host) && origin.strip_prefix("http://").is_some_and(|o| o.eq_ignore_ascii_case(host)),
        _ => false,
    }
}

pub(crate) fn run(stream: TcpStream, shared: &Shared) -> Result<(), String> {
    let ip = stream.peer_addr().map_err(|e| e.to_string())?.ip();
    let peer = ip.to_string();
    let _ = stream.set_nodelay(true);
    let config = WebSocketConfig::default().max_message_size(Some(MAX_LAYOUT * 2)).max_frame_size(Some(MAX_LAYOUT * 2)).max_write_buffer_size(MAX_BACKLOG);
    let check = |req: &Request, res: Response| -> Result<Response, ErrorResponse> {
        if same_origin(req) {
            return Ok(res);
        }
        let mut refuse = ErrorResponse::new(Some("another site".into()));
        *refuse.status_mut() = tungstenite::http::StatusCode::FORBIDDEN;
        Err(refuse)
    };
    let mut ws = tungstenite::accept_hdr_with_config(stream, check, Some(config)).map_err(|e| e.to_string())?;
    ws.get_mut().set_read_timeout(Some(Duration::from_millis(2))).map_err(|e| e.to_string())?;
    ws.get_mut().set_write_timeout(Some(Duration::from_secs(2))).map_err(|e| e.to_string())?;
    let texts = shared.feed().texts.clone();
    if !texts.is_empty() {
        let _ = send(&mut ws, &texts);
    }
    if !authenticate(&mut ws, shared, ip)? {
        let _ = ws.close(None);
        return Ok(());
    }
    if shared.clients.fetch_add(1, Ordering::Relaxed) >= MAX_CLIENTS {
        shared.clients.fetch_sub(1, Ordering::Relaxed);
        let _ = send(&mut ws, &j!({ "t": "denied", "reason": "full" }).to_string());
        let _ = ws.close(None);
        return Ok(());
    }
    let id = shared.sessions.fetch_add(1, Ordering::Relaxed);
    log::info!("remote cockpit: {peer} connected");
    let r = pump(&mut ws, shared, id);
    shared.clients.fetch_sub(1, Ordering::Relaxed);
    release(shared, id);
    log::info!("remote cockpit: {peer} left");
    r
}
