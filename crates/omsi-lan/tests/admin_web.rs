//! The web administration end to end: the server's real gateway (HTTP on this machine), its
//! session (UDP), the admin's requests from another thread as a browser sends them, and the
//! server's loop doing them on its own tick - and the same operation from the game's menu,
//! with the same result.

mod common;
use common::*;
use omsi_admin::Status;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

/// One HTTP request as a browser's `fetch` sends it; the status and the JSON answered.
fn http(port: u16, method: &str, path: &str, token: Option<&str>, body: Option<Value>, extra: &str) -> (u16, Value) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let body = body.map(|b| b.to_string()).unwrap_or_default();
    let auth = token.map(|t| format!("Authorization: Bearer {t}\r\n")).unwrap_or_default();
    let req = format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{auth}{extra}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
    s.write_all(req.as_bytes()).unwrap();
    let mut out = Vec::new();
    let _ = s.read_to_end(&mut out);
    let text = String::from_utf8_lossy(&out).to_string();
    let status = text.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    assert!(!head.to_ascii_lowercase().contains("access-control-allow-origin") || !path.starts_with("/api/admin"), "{head}");
    (status, serde_json::from_str(body).unwrap_or(Value::String(body.to_string())))
}

/// What the admin's page did, for the test to check.
struct PageRun {
    found: Vec<String>,
    op: Value,
}

/// The admin's page: log in, search `pichau`, pick the exact name, send them away, follow the
/// operation until the server says what became of it.
fn page(port: u16) -> PageRun {
    let (st, v) = http(port, "POST", "/api/admin/login", None, Some(json!({"password": "admin secret"})), "");
    assert_eq!(st, 200, "{v}");
    let token = v["token"].as_str().unwrap().to_string();
    let t = Some(token.as_str());
    // (the players the loop has published)
    let t0 = Instant::now();
    let players = loop {
        let (_, v) = http(port, "GET", "/api/admin/players?q=pichau", t, None, "");
        let list = v["players"].as_array().cloned().unwrap_or_default();
        if list.len() >= 2 || t0.elapsed() > Duration::from_secs(5) {
            break list;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let found: Vec<String> = players.iter().map(|p| p["name"].as_str().unwrap_or("").to_string()).collect();
    let pick = players.iter().find(|p| p["name"] == "Pichau").expect("Pichau listed").clone();
    let (st, v) = http(port, "POST", "/api/admin/ops", t, Some(json!({"op": "kick", "target": {"id": pick["id"], "conn": pick["conn"]}, "reason": "spam"})), "");
    assert_eq!(st, 202, "{v}");
    assert_eq!(v["op"]["status"], "queued");
    let id = v["op"]["id"].as_u64().unwrap();
    let t0 = Instant::now();
    let op = loop {
        let (_, v) = http(port, "GET", &format!("/api/admin/ops/{id}"), t, None, "");
        if v["op"]["status"] != "queued" || t0.elapsed() > Duration::from_secs(5) {
            break v["op"].clone();
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    PageRun { found, op }
}

#[test]
fn the_web_panels_way_and_the_games_way_do_the_same() {
    let port = 28790;
    let mut s = Server::new(port);
    let info = omsi_net::ws::ServerInfo { local_admin_password: "admin secret".into(), ..Default::default() };
    let gw = omsi_net::ws::WsGateway::start(SocketAddr::from(([127, 0, 0, 1], 0)), SocketAddr::from(([127, 0, 0, 1], port)), info).unwrap();
    omsi_lan::gateway::open_admin_api(s.hub.clone(), omsi_admin::WebCfg::default());
    let web = gw.addr.port();
    let mut pichau = Player::join(&mut s, port, "Pichau");
    let mut fan = Player::join(&mut s, port, "pichau_fan");
    let mut admin = Player::join(&mut s, port, "Chefe");
    // the page on its own thread, the server's loop on this one
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(page(web));
    });
    let t0 = Instant::now();
    let run = loop {
        if let Ok(r) = rx.try_recv() {
            break r;
        }
        assert!(t0.elapsed() < Duration::from_secs(15), "the page did not finish");
        pichau.step();
        fan.step();
        admin.step();
        s.step();
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(run.found, vec!["Pichau", "pichau_fan"]);
    assert_eq!(run.op["status"], "applied", "{}", run.op);
    assert!(run.op["message"].as_str().unwrap().starts_with("Pichau (#"), "{}", run.op);
    assert!(until(&mut s, &mut [&mut pichau, &mut fan], |_, ps| ps[0].lan.turned_away.is_some()));
    assert_eq!(pichau.lan.turned_away.as_deref(), Some("spam"));
    assert!(fan.lan.connected && fan.lan.turned_away.is_none());
    // the same operation from the game's menu: the same result
    assert_eq!(login(&mut s, &mut admin, &mut [&mut fan], "admin secret"), "admin-ok admin");
    let p = s.lan.peers().find(|p| p.pose.name == "pichau_fan").map(|p| (p.pose.id, p.conn)).unwrap();
    admin.heard.clear();
    admin.lan.command(1, &format!("admin kick {}#{} spam", p.0, p.1));
    assert!(until(&mut s, &mut [&mut admin, &mut fan], |_, ps| !results(ps[0]).is_empty() && ps[1].lan.turned_away.is_some()));
    let (st, msg) = results(&admin).remove(0);
    assert_eq!(st, Status::Applied);
    assert!(msg.starts_with("pichau_fan (#"), "{msg}");
    assert_eq!(fan.lan.turned_away.as_deref(), Some("spam"));
    // both in the audit log, each with who asked
    let audit = s.hub.audit_since(0, 100);
    assert!(audit.iter().any(|e| e.action == "kick" && e.result == "applied" && e.actor.starts_with("web#") && e.target.starts_with("Pichau")), "{audit:?}");
    assert!(audit.iter().any(|e| e.action == "kick" && e.result == "applied" && e.actor.contains("Chefe") && e.target.starts_with("pichau_fan")), "{audit:?}");

    // the door stays shut to what comes through a tunnel or a proxy, the old one too
    let (st, _) = http(web, "POST", "/api/admin/login", None, Some(json!({"password": "admin secret"})), "CF-Connecting-IP: 203.0.113.9\r\n");
    assert_eq!(st, 403);
    let (st, _) = http(web, "GET", "/api/admin/state", None, None, "");
    assert_eq!(st, 401);
    // the machine's own POST /admin keeps its contract (one command a line, 202)
    let mut c = TcpStream::connect(("127.0.0.1", web)).unwrap();
    c.write_all(b"POST /admin HTTP/1.1\r\nHost: x\r\nX-Admin-Password: admin secret\r\nContent-Length: 13\r\n\r\nclock 30600\r\n").unwrap();
    let mut out = String::new();
    let _ = c.read_to_string(&mut out);
    assert!(out.starts_with("HTTP/1.1 202"), "{out}");
    assert_eq!(gw.info.lock().unwrap().local_admin_queue, ["clock 30600"]);
    let mut c = TcpStream::connect(("127.0.0.1", web)).unwrap();
    c.write_all(b"POST /admin HTTP/1.1\r\nHost: x\r\nX-Forwarded-For: 203.0.113.9\r\nX-Admin-Password: admin secret\r\nContent-Length: 9\r\n\r\nsay hello").unwrap();
    let mut out = String::new();
    let _ = c.read_to_string(&mut out);
    assert!(out.starts_with("HTTP/1.1 403"), "{out}");
    omsi_net::ws_api::set_api(None);
}
