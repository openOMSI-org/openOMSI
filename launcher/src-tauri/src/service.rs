//! The game program as the launcher's helper (`openomsi --launcher-service`, no window): the
//! bus preview drawn by the game's own renderer, as the game's launcher window draws it, and
//! the city map with a trip's route read the way the navigator reads it. Started once, on the
//! first request, and again if it ended; requests carry a number and the answers find their
//! request by it, so a map being read does not hold up the pictures.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

type Pending = Arc<Mutex<HashMap<u64, Sender<(Value, Vec<u8>)>>>>;

struct Running {
    child: Child,
    stdin: ChildStdin,
    pending: Pending,
}

static SERVICE: Mutex<Option<Running>> = Mutex::new(None);
static NEXT: AtomicU64 = AtomicU64::new(1);
static LOOK: Mutex<Option<Value>> = Mutex::new(None);
static PLAYING: AtomicBool = AtomicBool::new(false);
static RELEASED: Mutex<Option<Instant>> = Mutex::new(None);

fn start() -> Result<Running, String> {
    let game = omsi_launcher_lib::game_binary().ok_or("the game program was not found")?;
    let log = std::fs::File::create(omsi_launcher_lib::data_dir().join("launcher-service.log")).map_err(|e| e.to_string())?;
    let mut child = Command::new(&game)
        .arg("--launcher-service")
        .env("RUST_LOG", "info")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(log))
        .spawn()
        .map_err(|e| format!("starting {}: {e}", game.display()))?;
    let stdin = child.stdin.take().ok_or("no stdin")?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let pending: Pending = Arc::default();
    let p = pending.clone();
    std::thread::Builder::new()
        .name("launcher service".into())
        .spawn(move || {
            let mut r = BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                if r.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                let Ok(head) = serde_json::from_str::<Value>(line.trim()) else { continue };
                let mut body = vec![0u8; head.get("len").and_then(Value::as_u64).unwrap_or(0) as usize];
                if r.read_exact(&mut body).is_err() {
                    break;
                }
                let id = head.get("id").and_then(Value::as_u64).unwrap_or(0);
                if let Some(tx) = p.lock().unwrap_or_else(|e| e.into_inner()).remove(&id) {
                    let _ = tx.send((head, body));
                }
            }
            // (the service ended: whoever still waits hears it at once)
            p.lock().unwrap_or_else(|e| e.into_inner()).clear();
        })
        .map_err(|e| e.to_string())?;
    omsi_launcher_lib::log_to_file(&format!("launcher service started: {}", game.display()));
    Ok(Running { child, stdin, pending })
}

/// Ask the service; its answer and the bytes after it.
pub fn ask(mut req: Value, wait: Duration) -> Result<(Value, Vec<u8>), String> {
    let look = req.get("cmd").and_then(Value::as_str) == Some("look");
    let drawing = matches!(req.get("cmd").and_then(Value::as_str), Some("look" | "warm" | "frame"));
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    req["id"] = json!(id);
    let (tx, rx) = channel();
    {
        let mut s = SERVICE.lock().unwrap_or_else(|e| e.into_inner());
        if drawing && PLAYING.load(Ordering::Relaxed) {
            if look {
                *LOOK.lock().unwrap_or_else(|e| e.into_inner()) = Some(req);
            }
            return Ok((json!({ "ok": true, "busy": false, "failed": false, "moving": false, "picture": true }), Vec::new()));
        }
        if s.as_mut().is_some_and(|r| r.child.try_wait().ok().flatten().is_some()) {
            *s = None;
        }
        if s.is_none() {
            let mut r = start()?;
            if let Some(last) = LOOK.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
                let _ = writeln!(r.stdin, "{last}");
            }
            *s = Some(r);
        }
        if look {
            *LOOK.lock().unwrap_or_else(|e| e.into_inner()) = Some(req.clone());
        }
        let r = s.as_mut().unwrap();
        r.pending.lock().unwrap_or_else(|e| e.into_inner()).insert(id, tx);
        if writeln!(r.stdin, "{req}").and_then(|_| r.stdin.flush()).is_err() {
            *s = None;
            return Err("the preview helper ended".into());
        }
    }
    let (head, body) = rx.recv_timeout(wait).map_err(|_| "the preview helper did not answer".to_string())?;
    if head.get("ok").and_then(Value::as_bool) == Some(false) {
        return Err(head.get("error").and_then(Value::as_str).unwrap_or("the preview helper failed").to_string());
    }
    Ok((head, body))
}

/// End the service with the launcher.
pub fn stop() {
    if let Some(mut r) = SERVICE.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = writeln!(r.stdin, "{}", json!({ "cmd": "quit" }));
        let _ = r.stdin.flush();
        std::thread::sleep(Duration::from_millis(100));
        let _ = r.child.kill();
    }
}

pub fn no_game_since(asked: Instant) {
    let _s = SERVICE.lock().unwrap_or_else(|e| e.into_inner());
    if RELEASED.lock().unwrap_or_else(|e| e.into_inner()).is_none_or(|t| t < asked) {
        PLAYING.store(false, Ordering::Relaxed);
    }
}

pub fn release() {
    let mut s = SERVICE.lock().unwrap_or_else(|e| e.into_inner());
    PLAYING.store(true, Ordering::Relaxed);
    *RELEASED.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
    let Some(mut r) = s.take() else { return };
    drop(s);
    std::thread::spawn(move || {
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(10) && !r.pending.lock().unwrap_or_else(|e| e.into_inner()).is_empty() {
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = writeln!(r.stdin, "{}", json!({ "cmd": "quit" }));
        let _ = r.stdin.flush();
        while t.elapsed() < Duration::from_secs(20) && matches!(r.child.try_wait(), Ok(None)) {
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = r.child.kill();
        let _ = r.child.wait();
    });
}
