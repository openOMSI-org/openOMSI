use crate::{lock, Shared};
use std::collections::HashMap;
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

include!(concat!(env!("OUT_DIR"), "/icons.rs"));

const MAX_CONNECTIONS: usize = 96;
const MAX_PER_ADDRESS: usize = 24;
const HEAD_WITHIN: Duration = Duration::from_secs(5);
const MAX_HEAD: usize = 8192;
fn csp(host: &str) -> String {
    let host: String = host.chars().filter(|c| c.is_ascii_alphanumeric() || ".:-[]".contains(*c)).collect();
    format!("default-src 'self'; connect-src 'self' ws://{host}; img-src 'self' data:; style-src 'self'; script-src 'self'; frame-ancestors 'none'")
}

static FILES: &[(&str, &str, &str)] = &[
    ("/", "text/html; charset=utf-8", include_str!("../page/index.html")),
    ("/style.css", "text/css; charset=utf-8", include_str!("../page/style.css")),
    ("/app.js", "text/javascript; charset=utf-8", include_str!("../page/app.js")),
    ("/picto.js", "text/javascript; charset=utf-8", include_str!("../page/picto.js")),
    ("/widgets.js", "text/javascript; charset=utf-8", include_str!("../page/widgets.js")),
    ("/map.js", "text/javascript; charset=utf-8", include_str!("../page/map.js")),
    ("/drive.js", "text/javascript; charset=utf-8", include_str!("../page/drive.js")),
    ("/editor.js", "text/javascript; charset=utf-8", include_str!("../page/editor.js")),
    ("/manifest.webmanifest", "application/manifest+json", include_str!("../page/manifest.webmanifest")),
];

static APP_ICON: &[u8] = include_bytes!("../../../assets/icons/app/openomsi-256.png");

pub(crate) fn start(port: u16, shared: Arc<Shared>) -> std::io::Result<u16> {
    let listener = TcpListener::bind(("0.0.0.0", port))?;
    listener.set_nonblocking(true)?;
    let port = listener.local_addr()?.port();
    let open = Arc::new(AtomicUsize::new(0));
    let by_address: Arc<Mutex<HashMap<IpAddr, usize>>> = Arc::default();
    std::thread::Builder::new().name("remote cockpit".into()).spawn(move || {
        while !shared.stop.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, peer)) => {
                    let ip = peer.ip();
                    {
                        let mut counts = lock(&by_address);
                        if open.load(Ordering::Relaxed) >= MAX_CONNECTIONS || counts.get(&ip).is_some_and(|n| *n >= MAX_PER_ADDRESS) {
                            continue;
                        }
                        *counts.entry(ip).or_default() += 1;
                    }
                    open.fetch_add(1, Ordering::Relaxed);
                    let (client_shared, client_open, client_counts) = (shared.clone(), open.clone(), by_address.clone());
                    let done = move || {
                        client_open.fetch_sub(1, Ordering::Relaxed);
                        let mut counts = lock(&client_counts);
                        if let Some(n) = counts.get_mut(&ip) {
                            *n -= 1;
                            if *n == 0 {
                                counts.remove(&ip);
                            }
                        }
                    };
                    let spawned = std::thread::Builder::new().name("remote cockpit client".into()).spawn(move || {
                        if let Err(e) = serve(stream, &client_shared) {
                            log::debug!("remote cockpit: {e}");
                        }
                        done();
                    });
                    if spawned.is_err() {
                        open.fetch_sub(1, Ordering::Relaxed);
                        let mut counts = lock(&by_address);
                        if let Some(n) = counts.get_mut(&ip) {
                            *n = n.saturating_sub(1);
                            if *n == 0 {
                                counts.remove(&ip);
                            }
                        }
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(40)),
                Err(e) => {
                    log::warn!("remote cockpit: {e}");
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
        }
    })?;
    Ok(port)
}

pub(crate) fn icon(name: &str) -> Option<&'static str> {
    ICONS.iter().find(|(n, _)| *n == name).map(|(_, svg)| *svg)
}

fn icon_list() -> String {
    serde_json::Value::Array(ICONS.iter().map(|(n, _)| serde_json::Value::String(n.to_string())).collect()).to_string()
}

pub(crate) fn route(path: &str) -> (&'static str, &'static str, Vec<u8>) {
    let path = path.split(['?', '#']).next().unwrap_or("/");
    if let Some((_, ctype, body)) = FILES.iter().find(|(p, _, _)| *p == path) {
        return ("200 OK", ctype, body.as_bytes().to_vec());
    }
    if path == "/icon.png" {
        return ("200 OK", "image/png", APP_ICON.to_vec());
    }
    if path == "/icons.json" {
        return ("200 OK", "application/json", icon_list().into_bytes());
    }
    if let Some(svg) = path.strip_prefix("/icons/").and_then(|n| n.strip_suffix(".svg")).and_then(icon) {
        return ("200 OK", "image/svg+xml", svg.as_bytes().to_vec());
    }
    ("404 Not Found", "text/plain; charset=utf-8", b"not found".to_vec())
}

fn skin(rest: &str, shared: &Shared) -> (&'static str, &'static str, Vec<u8>) {
    let (name, query) = rest.split_once('?').unwrap_or((rest, ""));
    let token = query.split('&').find_map(|kv| kv.strip_prefix("k=")).unwrap_or("");
    if !lock(&shared.devices).knows(token) {
        return ("403 Forbidden", "text/plain", b"pair first".to_vec());
    }
    let name = name.strip_suffix(".png").unwrap_or(name);
    if name == "~sign" {
        return match lock(&shared.sign).as_ref() {
            Some(png) => ("200 OK", "image/png", png.to_vec()),
            None => ("404 Not Found", "text/plain", b"no sign".to_vec()),
        };
    }
    match lock(&shared.skins).get(name) {
        Some(png) => ("200 OK", "image/png", png.to_vec()),
        None => ("404 Not Found", "text/plain", b"no picture".to_vec()),
    }
}

fn head(stream: &TcpStream) -> Result<(String, usize), String> {
    let since = Instant::now();
    let mut buf = vec![0u8; MAX_HEAD];
    loop {
        let n = stream.peek(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("closed".into());
        }
        let end = buf[..n].windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4);
        if end.is_some() || n >= MAX_HEAD {
            let size = end.unwrap_or(n);
            return Ok((String::from_utf8_lossy(&buf[..size]).into_owned(), size));
        }
        if since.elapsed() > HEAD_WITHIN {
            return Err("request too slow".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn serve(stream: TcpStream, shared: &Shared) -> Result<(), String> {
    stream.set_nonblocking(false).map_err(|e| e.to_string())?;
    stream.set_read_timeout(Some(HEAD_WITHIN)).map_err(|e| e.to_string())?;
    stream.set_write_timeout(Some(HEAD_WITHIN)).map_err(|e| e.to_string())?;
    let (req, size) = head(&stream)?;
    let mut words = req.split_whitespace();
    let method = words.next().unwrap_or("").to_string();
    let path = words.next().unwrap_or("/").to_string();
    if req.to_ascii_lowercase().contains("upgrade: websocket") && path.starts_with("/ws") {
        return crate::session::run(stream, shared);
    }
    let mut s = stream;
    let mut drain = vec![0u8; size];
    let _ = s.read_exact(&mut drain);
    let (status, ctype, body) = match (method.as_str(), path.strip_prefix("/vt/")) {
        ("GET" | "HEAD", Some(rest)) => skin(rest, shared),
        ("GET" | "HEAD", None) => route(&path),
        _ => ("405 Method Not Allowed", "text/plain", b"method not allowed".to_vec()),
    };
    let hdr = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: {}\r\nConnection: close\r\n\r\n",
        body.len(),
        csp(req.lines().find_map(|l| l.split_once(':').filter(|(k, _)| k.trim().eq_ignore_ascii_case("host")).map(|(_, v)| v.trim())).unwrap_or(""))
    );
    s.write_all(hdr.as_bytes()).map_err(|e| e.to_string())?;
    if method != "HEAD" {
        s.write_all(&body).map_err(|e| e.to_string())?;
    }
    Ok(())
}
