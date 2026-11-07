//! Discord Rich Presence over the local Discord IPC connection.

use omsi_launcher_lib::i18n::{tr, trf};
use std::io::{self, Read, Write};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const DEFAULT_APP_ID: &str = "1568733650634145792";
const WEBSITE: &str = "https://openomsi.org/";
const COMMUNITY: &str = "https://discord.gg/openomsi";
const MAX_FRAME: usize = 64 * 1024;
#[cfg(not(test))]
const POLL: Duration = Duration::from_millis(100);
#[cfg(test)]
const POLL: Duration = Duration::from_millis(10);
#[cfg(not(test))]
const CONNECT_RETRY: Duration = Duration::from_secs(20);
#[cfg(test)]
const CONNECT_RETRY: Duration = Duration::from_millis(40);
#[cfg(not(test))]
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(test)]
const RESPONSE_TIMEOUT: Duration = Duration::from_millis(250);
#[cfg(not(test))]
const UPDATE_INTERVAL: Duration = Duration::from_secs(5);
#[cfg(test)]
const UPDATE_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Presence {
    pub details: String,
    pub state: String,
    pub large_text: String,
    pub small: Option<(&'static str, String)>,
    pub party: Option<(String, usize, usize)>,
    pub buttons: Vec<(String, &'static str)>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Show {
    pub map: bool,
    pub bus: bool,
    pub line: bool,
    pub stop: bool,
    pub players: bool,
    pub buttons: bool,
}

impl Default for Show {
    fn default() -> Self {
        Self { map: true, bus: true, line: true, stop: true, players: true, buttons: true }
    }
}

impl Show {
    pub const KEYS: [&'static str; 6] = ["discord_map", "discord_bus", "discord_line", "discord_stop", "discord_players", "discord_buttons"];

    fn slots(&mut self) -> [&mut bool; 6] {
        [&mut self.map, &mut self.bus, &mut self.line, &mut self.stop, &mut self.players, &mut self.buttons]
    }

    pub fn set(&mut self, key: &str, on: bool) {
        if let Some(i) = Self::KEYS.iter().position(|k| *k == key) {
            *self.slots()[i] = on;
        }
    }

    pub fn to_text(mut self) -> String {
        Self::KEYS.iter().zip(self.slots()).map(|(k, on)| format!("{k}={}\n", *on as u8)).collect()
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Game<'a> {
    pub map: Option<&'a str>,
    pub bus: Option<(&'a str, &'a str)>,
    pub duty: Option<(&'a str, &'a str)>,
    pub next_stop: Option<(&'a str, f32)>,
    pub players: Option<(u64, usize, usize)>,
}

#[derive(Default)]
pub struct ServerLimit {
    asked: Option<(String, Arc<AtomicUsize>)>,
}

impl ServerLimit {
    pub fn get(&mut self, target: &str, ask: impl FnOnce(&str) -> Option<usize> + Send + 'static) -> usize {
        if self.asked.as_ref().is_none_or(|(asked, _)| asked != target) {
            let max = Arc::new(AtomicUsize::new(0));
            let found = max.clone();
            let server = target.to_string();
            let _ = std::thread::Builder::new().name("discord-server-size".into()).spawn(move || {
                if let Some(n) = ask(&server) {
                    found.store(n, Ordering::Relaxed);
                }
            });
            self.asked = Some((target.to_string(), max));
        }
        self.asked.as_ref().map_or(0, |(_, max)| max.load(Ordering::Relaxed))
    }

    pub fn forget(&mut self) {
        self.asked = None;
    }
}

fn party_id(session: u64) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(format!("openomsi-discord-party:{session}"));
    digest[..12].iter().map(|b| format!("{b:02x}")).collect()
}

impl Presence {
    pub fn for_game(game: &Game, show: Show) -> Option<Self> {
        let map = game.map.filter(|name| !name.trim().is_empty())?;
        let bus = game.bus.map(|(short, full)| {
            let short = short.trim();
            let full = full.trim();
            (if short.is_empty() { full } else { short }, full)
        });
        let multiplayer = game.players.is_some() && show.players;
        let mut details = Vec::new();
        if show.map {
            details.push(compact(map, 24));
        }
        if let (true, Some((line, _))) = (show.line, game.duty) {
            details.push(trf("Line {line}", &[("line", &line.trim())]));
        }
        let party = game.players.filter(|(session, n, max)| multiplayer && *session != 0 && *n > 0 && max >= n).map(|(session, n, max)| (party_id(session), n, max));
        let mut state = Vec::new();
        let next_stop = game.next_stop.filter(|(name, _)| show.stop && game.duty.is_some() && !name.trim().is_empty());
        match (bus, next_stop) {
            (Some(_), Some((stop, delay))) => state.push(format!("{}: {} ({})", tr("Next stop"), compact(stop, 28), delay_text(delay))),
            (Some((short, _)), None) => {
                if show.bus {
                    state.push(compact(short, 16));
                }
                match game.duty {
                    Some((_, tour)) if show.line => state.push(trf("Tour {tour}", &[("tour", &tour.trim())])),
                    None => state.push(tr("Free drive")),
                    _ => {}
                }
                if state.is_empty() {
                    state.push(tr("Driving"));
                }
            }
            (None, _) => state.push(tr("On foot")),
        }
        if multiplayer && party.is_none() {
            state.push(tr("Multiplayer"));
        }
        let mut large_text = String::from("openOMSI");
        if show.map {
            large_text.push_str(&format!(" | {map}"));
        }
        if let (true, Some((_, full))) = (show.bus, bus) {
            large_text.push_str(&format!(" | {full}"));
        }
        let small = if multiplayer {
            ("online", tr("Multiplayer"))
        } else if bus.is_some() {
            ("bus", tr("Driving"))
        } else {
            ("walk", tr("On foot"))
        };
        Some(Self {
            details: details.join(" | "),
            state: state.join(" | "),
            large_text,
            small: Some(small),
            party,
            buttons: buttons(show.buttons),
        })
    }

    pub fn for_launcher(enabled: bool, launching: bool, game_running: bool, show_buttons: bool) -> Option<Self> {
        (enabled && !launching && !game_running).then(|| Self {
            details: tr("In launcher"),
            state: tr("Preparing a drive"),
            large_text: "openOMSI".into(),
            buttons: buttons(show_buttons),
            ..Default::default()
        })
    }
}

fn buttons(shown: bool) -> Vec<(String, &'static str)> {
    if !shown {
        return Vec::new();
    }
    vec![(tr("Get openOMSI"), WEBSITE), (tr("Join the Discord"), COMMUNITY)]
}

fn delay_text(delay: f32) -> String {
    let secs = delay.abs().round() as i64;
    format!("{}{}:{:02}", if delay < 0.0 { "-" } else { "+" }, secs / 60, secs % 60)
}

/// Shorten at a word boundary without interpreting names supplied by content packs.
fn compact(text: &str, limit: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= limit {
        return text.to_string();
    }
    // (cut at a word where one ends in the limit, else the letters that leave room for the dots)
    let prefix: String = text.chars().take(limit).collect();
    let short = match prefix.rsplit_once(' ') {
        Some((words, _)) => words.trim_end().to_string(),
        None => text.chars().take(limit.saturating_sub(3)).collect(),
    };
    format!("{short}...")
}

pub struct Discord {
    wanted: Arc<Mutex<Option<Presence>>>,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}

#[cfg(unix)]
type Pipe = std::os::unix::net::UnixStream;
#[cfg(windows)]
struct Pipe(windows::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Drop for Pipe {
    fn drop(&mut self) {
        let _ = unsafe { windows::Win32::Foundation::CloseHandle(self.0) };
    }
}

#[cfg(windows)]
fn overlapped_io(
    handle: windows::Win32::Foundation::HANDLE,
    buffer: &mut [u8],
    write: bool,
) -> io::Result<usize> {
    use windows::core::PCWSTR;
    use windows::Win32::{
        Foundation::WAIT_OBJECT_0,
        System::{
            Threading::{CreateEventW, WaitForSingleObject},
            IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED},
        },
    };

    let event = unsafe { CreateEventW(None, true, false, PCWSTR::null()) }
        .map_err(|e| io::Error::other(e.to_string()))?;
    let mut overlapped = OVERLAPPED {
        hEvent: event,
        ..Default::default()
    };
    let start = if write {
        unsafe {
            windows::Win32::Storage::FileSystem::WriteFile(
                handle,
                Some(buffer),
                None,
                Some(&mut overlapped),
            )
        }
    } else {
        unsafe {
            windows::Win32::Storage::FileSystem::ReadFile(
                handle,
                Some(buffer),
                None,
                Some(&mut overlapped),
            )
        }
    };
    if let Err(error) = start {
        if error.code().0 != 0x8007_03e5u32 as i32 {
            let _ = unsafe { windows::Win32::Foundation::CloseHandle(event) };
            return Err(io::Error::other(error.to_string()));
        }
    } else {
        let mut transferred = 0;
        let result = unsafe { GetOverlappedResult(handle, &overlapped, &mut transferred, false) };
        let _ = unsafe { windows::Win32::Foundation::CloseHandle(event) };
        result.map_err(|e| io::Error::other(e.to_string()))?;
        if !write && transferred == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Discord IPC closed",
            ));
        }
        return Ok(transferred as usize);
    }
    let wait = unsafe { WaitForSingleObject(event, POLL.as_millis() as u32) };
    if wait != WAIT_OBJECT_0 {
        let _ = unsafe { CancelIoEx(handle, Some(&overlapped)) };
        let mut transferred = 0;
        let result = unsafe { GetOverlappedResult(handle, &overlapped, &mut transferred, true) };
        let _ = unsafe { windows::Win32::Foundation::CloseHandle(event) };
        if result.is_ok() {
            if !write && transferred == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "Discord IPC closed",
                ));
            }
            return Ok(transferred as usize);
        }
        return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "Discord IPC operation timed out",
        ));
    }
    let mut transferred = 0;
    let result = unsafe { GetOverlappedResult(handle, &overlapped, &mut transferred, false) };
    let _ = unsafe { windows::Win32::Foundation::CloseHandle(event) };
    result.map_err(|e| io::Error::other(e.to_string()))?;
    if !write && transferred == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "Discord IPC closed",
        ));
    }
    Ok(transferred as usize)
}

#[cfg(windows)]
impl Read for Pipe {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        overlapped_io(self.0, buf, false)
    }
}

#[cfg(windows)]
impl Write for Pipe {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        overlapped_io(self.0, &mut buf.to_vec(), true)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Debug, PartialEq)]
struct Frame {
    opcode: u32,
    body: Vec<u8>,
}

fn connect() -> Option<Pipe> {
    for i in 0..10 {
        #[cfg(unix)]
        {
            let dirs = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"]
                .iter()
                .filter_map(|k| std::env::var(k).ok())
                .chain(["/tmp".to_string()]);
            for dir in dirs {
                let path = std::path::Path::new(&dir).join(format!("discord-ipc-{i}"));
                if let Ok(pipe) = Pipe::connect(path) {
                    if pipe.set_read_timeout(Some(POLL)).is_ok()
                        && pipe.set_write_timeout(Some(POLL)).is_ok()
                    {
                        return Some(pipe);
                    }
                }
            }
        }
        #[cfg(windows)]
        {
            use windows::core::PCWSTR;
            use windows::Win32::{
                Foundation::{GENERIC_READ, GENERIC_WRITE},
                Storage::FileSystem::{
                    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_FLAG_OVERLAPPED, FILE_SHARE_MODE,
                    OPEN_EXISTING,
                },
            };
            let path: Vec<u16> = format!(r"\\.\pipe\discord-ipc-{i}")
                .encode_utf16()
                .chain([0])
                .collect();
            if let Ok(handle) = unsafe {
                CreateFileW(
                    PCWSTR(path.as_ptr()),
                    GENERIC_READ.0 | GENERIC_WRITE.0,
                    FILE_SHARE_MODE(0),
                    None,
                    OPEN_EXISTING,
                    FILE_FLAGS_AND_ATTRIBUTES(FILE_FLAG_OVERLAPPED.0),
                    None,
                )
            } {
                return Some(Pipe(handle));
            }
        }
    }
    None
}

fn write_frame(pipe: &mut impl Write, opcode: u32, body: &[u8]) -> io::Result<()> {
    if body.len() > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Discord frame too large",
        ));
    }
    let mut frame = Vec::with_capacity(8 + body.len());
    frame.extend_from_slice(&opcode.to_le_bytes());
    frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
    frame.extend_from_slice(body);
    pipe.write_all(&frame)
}

fn decode_frame(pending: &mut Vec<u8>) -> io::Result<Option<Frame>> {
    if pending.len() >= 8 {
        let opcode = u32::from_le_bytes(pending[..4].try_into().unwrap());
        let len = u32::from_le_bytes(pending[4..8].try_into().unwrap()) as usize;
        if len > MAX_FRAME {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Discord frame too large",
            ));
        }
        if pending.len() >= 8 + len {
            let body = pending[8..8 + len].to_vec();
            pending.drain(..8 + len);
            return Ok(Some(Frame { opcode, body }));
        }
    }

    Ok(None)
}

/// One bounded read per poll keeps partial frames while allowing shutdown and deadlines.
fn read_frame(pipe: &mut impl Read, pending: &mut Vec<u8>) -> io::Result<Option<Frame>> {
    if let Some(frame) = decode_frame(pending)? {
        return Ok(Some(frame));
    }
    let mut chunk = [0; 4096];
    match pipe.read(&mut chunk) {
        Ok(0) => Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "Discord IPC closed",
        )),
        Ok(n) => {
            pending.extend_from_slice(&chunk[..n]);
            decode_frame(pending)
        }
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
            ) =>
        {
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

fn text(frame: &Frame) -> Option<serde_json::Value> {
    serde_json::from_slice(&frame.body).ok()
}

fn answer_ping(pipe: &mut impl Write, frame: &Frame) -> io::Result<bool> {
    if frame.opcode == 3 {
        write_frame(pipe, 4, &frame.body)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

fn activity(presence: &Presence, started: u64, full: bool) -> serde_json::Value {
    let mut activity = serde_json::json!({ "timestamps": { "start": started } });
    for (key, text) in [("details", &presence.details), ("state", &presence.state)] {
        if !text.trim().is_empty() {
            activity[key] = serde_json::json!(text.chars().take(120).collect::<String>());
        }
    }
    if full {
        let mut assets = serde_json::json!({ "large_image": "logo", "large_text": presence.large_text.chars().take(120).collect::<String>() });
        if let Some((image, text)) = &presence.small {
            assets["small_image"] = serde_json::json!(image);
            assets["small_text"] = serde_json::json!(text.chars().take(120).collect::<String>());
        }
        activity["assets"] = assets;
        if let Some((id, players, max)) = &presence.party {
            activity["party"] = serde_json::json!({ "id": id, "size": [players, max] });
        }
        if !presence.buttons.is_empty() {
            activity["buttons"] = presence.buttons.iter().map(|(label, url)| serde_json::json!({ "label": label.chars().take(32).collect::<String>(), "url": url })).collect();
        }
    }
    activity
}

fn nonce(value: &serde_json::Value) -> Option<&str> {
    value.get("nonce")?.as_str()
}

impl Discord {
    pub fn start(app_id: &str) -> Option<Self> {
        let app_id = if app_id.trim().is_empty() {
            DEFAULT_APP_ID
        } else {
            app_id.trim()
        };
        if !app_id.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let wanted = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let worker_wanted = wanted.clone();
        let worker_stop = stop.clone();
        let app_id = app_id.to_string();
        let started = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let worker = std::thread::Builder::new()
            .name("discord-rich-presence".into())
            .spawn(move || run(app_id, worker_wanted, worker_stop, started))
            .ok()?;
        Some(Self {
            wanted,
            stop,
            worker: Some(worker),
        })
    }

    pub fn set(&self, presence: Option<Presence>) {
        if let Ok(mut wanted) = self.wanted.lock() {
            *wanted = presence;
        }
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
    }

    pub fn is_stopping(&self) -> bool {
        self.stop.load(Ordering::Acquire)
    }

    pub fn is_finished(&self) -> bool {
        self.worker
            .as_ref()
            .is_none_or(|worker| worker.is_finished())
    }
}

impl Drop for Discord {
    fn drop(&mut self) {
        self.stop();
        // Disconnecting is background work too: never wait on IPC in the UI thread.
        if self.is_finished() {
            if let Some(worker) = self.worker.take() {
                if worker.join().is_err() {
                    log::error!("Discord: presence worker panicked");
                }
            }
        }
    }
}

fn run(app_id: String, wanted: Arc<Mutex<Option<Presence>>>, stop: Arc<AtomicBool>, started: u64) {
    run_with(app_id, wanted, stop, started, connect)
}

fn run_with<T: Read + Write>(
    app_id: String,
    wanted: Arc<Mutex<Option<Presence>>>,
    stop: Arc<AtomicBool>,
    started: u64,
    mut connector: impl FnMut() -> Option<T>,
) {
    let mut pipe = None;
    let mut ready = false;
    let mut pending = Vec::new();
    let mut last_try = Instant::now() - CONNECT_RETRY;
    let mut connected_at = None;
    let mut last_update = Instant::now() - UPDATE_INTERVAL;
    let mut shown: Option<Option<Presence>> = None;
    let mut waiting_for: Option<(String, Instant, Option<Presence>, bool)> = None;
    let mut sequence = 0u64;

    loop {
        if stop.load(Ordering::Acquire) {
            if let Some(pipe) = pipe.as_mut() {
                if ready {
                    sequence += 1;
                    let msg = serde_json::json!({ "cmd": "SET_ACTIVITY", "args": { "pid": std::process::id(), "activity": null }, "nonce": sequence.to_string() }).to_string();
                    let _ = write_frame(pipe, 1, msg.as_bytes());
                }
                let _ = write_frame(pipe, 2, &[]);
            }
            return;
        }

        if pipe.is_none() {
            if last_try.elapsed() >= CONNECT_RETRY {
                last_try = Instant::now();
                if let Some(mut candidate) = connector() {
                    let hello = serde_json::json!({ "v": 1, "client_id": app_id }).to_string();
                    if write_frame(&mut candidate, 0, hello.as_bytes()).is_ok() {
                        pipe = Some(candidate);
                        connected_at = Some(Instant::now());
                        pending.clear();
                        waiting_for = None;
                        ready = false;
                        shown = None;
                        last_update = Instant::now() - UPDATE_INTERVAL;
                    }
                }
            }
            std::thread::sleep(POLL);
            continue;
        }

        let current = wanted.lock().ok().and_then(|w| w.clone());
        if !ready && connected_at.is_some_and(|at| at.elapsed() > RESPONSE_TIMEOUT) {
            pipe = None;
            connected_at = None;
            continue;
        }
        if let Some((_, sent_at, _, _)) = &waiting_for {
            if sent_at.elapsed() > RESPONSE_TIMEOUT {
                pipe = None;
                continue;
            }
        } else if ready
            && shown.as_ref() != Some(&current)
            && last_update.elapsed() >= UPDATE_INTERVAL
        {
            sequence += 1;
            let activity = current.as_ref().map(|p| activity(p, started, true));
            let msg = serde_json::json!({ "cmd": "SET_ACTIVITY", "args": { "pid": std::process::id(), "activity": activity }, "nonce": sequence.to_string() }).to_string();
            if write_frame(pipe.as_mut().unwrap(), 1, msg.as_bytes()).is_err() {
                pipe = None;
                continue;
            }
            waiting_for = Some((sequence.to_string(), Instant::now(), current, true));
            last_update = Instant::now();
        }

        match read_frame(pipe.as_mut().unwrap(), &mut pending) {
            Ok(Some(frame)) if answer_ping(pipe.as_mut().unwrap(), &frame).unwrap_or(false) => {
                continue
            }
            Ok(Some(frame)) => {
                let Some(value) = text(&frame) else {
                    pipe = None;
                    continue;
                };
                if frame.opcode != 1 {
                    pipe = None;
                    continue;
                }
                let evt = value.get("evt").and_then(|v| v.as_str());
                if evt == Some("READY") {
                    log::info!("Discord: connected");
                    ready = true;
                    shown = None;
                    last_update = Instant::now() - UPDATE_INTERVAL;
                    continue;
                }
                if evt == Some("ERROR")
                    || value.get("cmd").and_then(|v| v.as_str()) == Some("ERROR")
                {
                    log::warn!(
                        "Discord: IPC request failed: {}",
                        value.get("data").unwrap_or(&value)
                    );
                    if let Some((expected, _, Some(submitted), true)) = waiting_for.take() {
                        if nonce(&value) == Some(expected.as_str()) {
                            sequence += 1;
                            let activity = activity(&submitted, started, false);
                            let msg = serde_json::json!({ "cmd": "SET_ACTIVITY", "args": { "pid": std::process::id(), "activity": activity }, "nonce": sequence.to_string() }).to_string();
                            if write_frame(pipe.as_mut().unwrap(), 1, msg.as_bytes()).is_ok() {
                                waiting_for = Some((
                                    sequence.to_string(),
                                    Instant::now(),
                                    Some(submitted),
                                    false,
                                ));
                                continue;
                            }
                        }
                    }
                    pipe = None;
                    ready = false;
                    connected_at = None;
                    continue;
                }
                if let Some((expected, _, submitted, _)) = waiting_for.as_ref() {
                    if nonce(&value) == Some(expected.as_str()) {
                        shown = Some(submitted.clone());
                        waiting_for = None;
                    }
                }
            }
            Ok(None) => {}
            Err(e) => {
                log::debug!("Discord: IPC disconnected: {e}");
                pipe = None;
                ready = false;
                connected_at = None;
                pending.clear();
                waiting_for = None;
                shown = None;
            }
        }
        std::thread::sleep(POLL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair() -> (std::net::TcpStream, std::net::TcpStream) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let client = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (peer, _) = listener.accept().unwrap();
        for stream in [&client, &peer] {
            stream.set_read_timeout(Some(POLL)).unwrap();
            stream.set_write_timeout(Some(POLL)).unwrap();
            stream.set_nodelay(true).unwrap();
        }
        (client, peer)
    }

    fn receive_buffered(
        pipe: &mut std::net::TcpStream,
        pending: &mut Vec<u8>,
        until: Instant,
    ) -> Frame {
        loop {
            if let Some(frame) = read_frame(pipe, pending).unwrap() {
                return frame;
            }
            assert!(
                Instant::now() < until,
                "timed out waiting for a Discord IPC frame"
            );
        }
    }

    fn receive(pipe: &mut std::net::TcpStream, until: Instant) -> Frame {
        receive_buffered(pipe, &mut Vec::new(), until)
    }

    fn ready_fragmented(pipe: &mut std::net::TcpStream) {
        let body = br#"{"cmd":"DISPATCH","evt":"READY","data":{}}"#;
        let mut packet = Vec::new();
        packet.extend_from_slice(&1u32.to_le_bytes());
        packet.extend_from_slice(&(body.len() as u32).to_le_bytes());
        packet.extend_from_slice(body);
        pipe.write_all(&packet[..3]).unwrap();
        std::thread::sleep(Duration::from_millis(15));
        pipe.write_all(&packet[3..]).unwrap();
    }

    fn acknowledge(pipe: &mut std::net::TcpStream, request: &Frame) {
        let request: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        let response =
            serde_json::json!({ "cmd": "SET_ACTIVITY", "data": {}, "nonce": request["nonce"] });
        write_frame(pipe, 1, response.to_string().as_bytes()).unwrap();
    }

    #[test]
    fn presence_requires_a_map_and_describes_session() {
        omsi_launcher_lib::i18n::pin_language("ENG");
        let show = Show::default();
        let bus = Some(("Bus", "Full bus name"));
        assert_eq!(Presence::for_game(&Game { bus, ..Default::default() }, show), None);
        assert_eq!(Presence::for_game(&Game { map: Some(" "), ..Default::default() }, show), None);
        let online = Game { map: Some("Map"), bus, duty: Some((" 5 ", " 2 ")), players: Some((7, 3, 16)), ..Default::default() };
        let p = Presence::for_game(&online, show).unwrap();
        assert_eq!(p.details, "Map | Line 5");
        assert_eq!(p.state, "Bus | Tour 2");
        assert_eq!(p.large_text, "openOMSI | Map | Full bus name");
        assert_eq!(p.small, Some(("online", "Multiplayer".into())));
        assert_eq!(p.party, Some((party_id(7), 3, 16)));
        assert_eq!(p.buttons.len(), 2);
        let walk = Presence::for_game(&Game { map: Some("Map"), ..Default::default() }, show).unwrap();
        assert_eq!(walk.state, "On foot");
        assert_eq!(walk.small, Some(("walk", "On foot".into())));
        assert_eq!(walk.party, None);
    }

    #[test]
    fn each_part_can_be_hidden() {
        omsi_launcher_lib::i18n::pin_language("ENG");
        let game = Game { map: Some("Map"), bus: Some(("Bus", "Full bus name")), duty: Some(("5", "2")), next_stop: Some(("Hauptbahnhof", 83.0)), players: Some((7, 3, 16)) };
        let all = Presence::for_game(&game, Show::default()).unwrap();
        assert_eq!(all.state, "Next stop: Hauptbahnhof (+1:23)");
        assert!(all.large_text.ends_with("Full bus name"));
        let early = Presence::for_game(&Game { next_stop: Some(("Hauptbahnhof", -5.4)), ..game }, Show::default()).unwrap();
        assert!(early.state.contains("(-0:05)"));
        let none = Show { map: false, bus: false, line: false, stop: false, players: false, buttons: false };
        let hidden = Presence::for_game(&game, none).unwrap();
        assert_eq!(hidden.details, "");
        assert_eq!(hidden.state, "Driving");
        assert_eq!(hidden.large_text, "openOMSI");
        assert_eq!(hidden.small, Some(("bus", "Driving".into())));
        assert_eq!(hidden.party, None);
        assert!(hidden.buttons.is_empty());
        let a = activity(&hidden, 1, true);
        assert!(a.get("details").is_none());
        assert!(a.get("party").is_none() && a.get("buttons").is_none());
        let no_stop = Presence::for_game(&game, Show { stop: false, ..Show::default() }).unwrap();
        assert_eq!(no_stop.state, "Bus | Tour 2");
        let mut show = Show::default();
        show.set("discord_stop", false);
        show.set("unknown", false);
        assert_eq!(show, Show { stop: false, ..Show::default() });
        assert!(show.to_text().contains("discord_stop=0\ndiscord_players=1\n"));
        let unknown_max = Presence::for_game(&Game { players: Some((7, 3, 0)), ..game }, Show::default()).unwrap();
        assert_eq!(unknown_max.party, None);
        assert!(unknown_max.state.ends_with("Multiplayer"));
        let no_session = Presence::for_game(&Game { players: Some((0, 3, 16)), ..game }, Show::default()).unwrap();
        assert_eq!(no_session.party, None);
    }

    #[test]
    fn each_session_is_its_own_party_and_hides_the_session() {
        assert_eq!(party_id(42), party_id(42));
        assert_ne!(party_id(42), party_id(43));
        assert_eq!(party_id(42).len(), 24);
        assert!(!party_id(42).contains("42"));
    }

    #[test]
    fn server_limit_follows_the_server_and_ignores_a_late_answer() {
        let (release_a, wait_a) = std::sync::mpsc::channel::<()>();
        let mut limit = ServerLimit::default();
        assert_eq!(limit.get("a", move |_| wait_a.recv().ok().map(|_| 16)), 0);
        limit.get("b", |_| Some(64));
        let until = Instant::now() + Duration::from_secs(2);
        while limit.get("b", |_| panic!("asked twice")) != 64 {
            assert!(Instant::now() < until);
            std::thread::sleep(POLL);
        }
        release_a.send(()).unwrap();
        std::thread::sleep(POLL * 5);
        assert_eq!(limit.get("b", |_| panic!("asked twice")), 64);
        assert_eq!(limit.get("a", |_| None), 0);
        limit.forget();
        limit.get("a", |_| Some(8));
        let until = Instant::now() + Duration::from_secs(2);
        while limit.get("a", |_| panic!("asked twice")) != 8 {
            assert!(Instant::now() < until);
            std::thread::sleep(POLL);
        }
    }

    #[test]
    fn frames_reject_oversize_and_parse_fragmented_headers() {
        let mut pipe = pair();
        pipe.0
            .set_read_timeout(Some(Duration::from_millis(5)))
            .unwrap();
        pipe.1.write_all(&[1, 0, 0]).unwrap();
        let mut pending = Vec::new();
        assert_eq!(read_frame(&mut pipe.0, &mut pending).unwrap(), None);
        pipe.1.write_all(&[0, 2, 0, 0, 0, b'{', b'}']).unwrap();
        assert_eq!(
            receive_buffered(
                &mut pipe.0,
                &mut pending,
                Instant::now() + Duration::from_secs(2)
            ),
            Frame {
                opcode: 1,
                body: b"{}".to_vec()
            }
        );
        pending.clear();
        pending.extend_from_slice(&1u32.to_le_bytes());
        pending.extend_from_slice(&(MAX_FRAME as u32 + 1).to_le_bytes());
        assert_eq!(
            decode_frame(&mut pending).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn ping_gets_the_same_payload_back_as_pong() {
        let (mut client, mut server) = pair();
        let payload = br#"{"nonce":"ping"}"#;
        write_frame(&mut server, 3, payload).unwrap();
        let frame = receive(&mut client, Instant::now() + Duration::from_secs(2));
        assert!(answer_ping(&mut client, &frame).unwrap());
        let pong = receive(&mut server, Instant::now() + Duration::from_secs(2));
        assert_eq!(
            pong,
            Frame {
                opcode: 4,
                body: payload.to_vec()
            }
        );
    }

    #[test]
    fn local_discord_peer_handles_ready_asset_failure_ping_and_reconnect() {
        omsi_launcher_lib::i18n::pin_language("ENG");
        use std::collections::VecDeque;

        let (client_one, mut peer_one) = pair();
        let (client_two, mut peer_two) = pair();
        client_one.set_read_timeout(Some(POLL)).unwrap();
        client_one.set_write_timeout(Some(POLL)).unwrap();
        client_two.set_read_timeout(Some(POLL)).unwrap();
        client_two.set_write_timeout(Some(POLL)).unwrap();
        peer_one.set_read_timeout(Some(POLL)).unwrap();
        peer_two.set_read_timeout(Some(POLL)).unwrap();
        let connections = Arc::new(Mutex::new(VecDeque::from([client_one, client_two])));
        let connector_queue = connections.clone();
        let wanted = Arc::new(Mutex::new(Some(Presence {
            details: "Bus | Map".into(),
            state: "Free drive".into(),
            large_text: "Full bus name".into(),
            ..Default::default()
        })));
        let stop = Arc::new(AtomicBool::new(false));
        let worker_wanted = wanted.clone();
        let worker_stop = stop.clone();
        let worker = std::thread::spawn(move || {
            run_with(
                DEFAULT_APP_ID.into(),
                worker_wanted,
                worker_stop,
                1234,
                move || connector_queue.lock().unwrap().pop_front(),
            )
        });
        let deadline = Instant::now() + Duration::from_secs(2);

        let hello = receive(&mut peer_one, deadline);
        let hello: serde_json::Value = serde_json::from_slice(&hello.body).unwrap();
        assert_eq!(hello["client_id"], DEFAULT_APP_ID);
        ready_fragmented(&mut peer_one);

        let first = receive(&mut peer_one, deadline);
        let first_json: serde_json::Value = serde_json::from_slice(&first.body).unwrap();
        assert_eq!(first_json["args"]["activity"]["details"], "Bus | Map");
        assert_eq!(
            first_json["args"]["activity"]["assets"]["large_image"],
            "logo"
        );
        let error = serde_json::json!({ "evt": "ERROR", "nonce": first_json["nonce"], "data": { "code": 400, "message": "unknown asset" } });
        write_frame(&mut peer_one, 1, error.to_string().as_bytes()).unwrap();

        let fallback = receive(&mut peer_one, deadline);
        let fallback: serde_json::Value = serde_json::from_slice(&fallback.body).unwrap();
        assert_eq!(fallback["args"]["activity"]["details"], "Bus | Map");
        assert!(fallback["args"]["activity"].get("assets").is_none());
        acknowledge(
            &mut peer_one,
            &Frame {
                opcode: 1,
                body: fallback.to_string().into_bytes(),
            },
        );

        let ping = serde_json::json!({ "nonce": "peer-ping" });
        write_frame(&mut peer_one, 3, ping.to_string().as_bytes()).unwrap();
        let pong = receive(&mut peer_one, deadline);
        assert_eq!(pong.opcode, 4);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&pong.body).unwrap(),
            ping
        );

        drop(peer_one);
        let hello = receive(&mut peer_two, deadline);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&hello.body).unwrap()["client_id"],
            DEFAULT_APP_ID
        );
        ready_fragmented(&mut peer_two);
        let resent = receive(&mut peer_two, deadline);
        let resent: serde_json::Value = serde_json::from_slice(&resent.body).unwrap();
        assert_eq!(resent["args"]["activity"]["details"], "Bus | Map");
        assert_eq!(resent["args"]["activity"]["timestamps"]["start"], 1234);
        stop.store(true, Ordering::Release);
        worker.join().unwrap();
        let mut pending = Vec::new();
        let clear = receive_buffered(&mut peer_two, &mut pending, deadline);
        let clear: serde_json::Value = serde_json::from_slice(&clear.body).unwrap();
        assert!(clear["args"]["activity"].is_null());
        assert_eq!(
            receive_buffered(&mut peer_two, &mut pending, deadline).opcode,
            2
        );
    }

    #[test]
    fn unanswered_activity_times_out_and_reconnects() {
        omsi_launcher_lib::i18n::pin_language("ENG");
        use std::sync::atomic::AtomicUsize;

        let (client, mut peer) = pair();
        client.set_read_timeout(Some(POLL)).unwrap();
        client.set_write_timeout(Some(POLL)).unwrap();
        peer.set_read_timeout(Some(POLL)).unwrap();
        let attempts = Arc::new(AtomicUsize::new(0));
        let connector_attempts = attempts.clone();
        let wanted = Arc::new(Mutex::new(Some(Presence {
            details: "Map".into(),
            state: "On foot".into(),
            large_text: "openOMSI".into(),
            ..Default::default()
        })));
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker = std::thread::spawn(move || {
            let mut client = Some(client);
            run_with(
                DEFAULT_APP_ID.into(),
                wanted,
                worker_stop,
                1234,
                move || {
                    connector_attempts.fetch_add(1, Ordering::Relaxed);
                    client.take()
                },
            )
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        let _hello = receive(&mut peer, deadline);
        ready_fragmented(&mut peer);
        let _unanswered = receive(&mut peer, deadline);
        while attempts.load(Ordering::Relaxed) < 2 && Instant::now() < deadline {
            std::thread::sleep(POLL);
        }
        assert!(attempts.load(Ordering::Relaxed) >= 2);
        stop.store(true, Ordering::Release);
        worker.join().unwrap();
    }

    #[test]
    fn activity_truncates_by_unicode_scalar_and_keeps_timestamp() {
        let p = Presence {
            details: "🚌".repeat(130),
            state: "line".into(),
            large_text: "openOMSI".into(),
            small: Some(("bus", "Driving".into())),
            party: Some(("abc".into(), 2, 8)),
            buttons: vec![("x".repeat(40), WEBSITE)],
        };
        let a = activity(&p, 1234, true);
        assert_eq!(a["details"].as_str().unwrap().chars().count(), 120);
        assert_eq!(a["timestamps"]["start"], 1234);
        assert_eq!(a["assets"]["large_image"], "logo");
        assert_eq!(a["assets"]["small_image"], "bus");
        assert_eq!(a["party"]["size"], serde_json::json!([2, 8]));
        assert_eq!(a["party"]["id"], "abc");
        assert_eq!(a["buttons"][0]["label"].as_str().unwrap().len(), 32);
        assert_eq!(a["buttons"][0]["url"], WEBSITE);
        let plain = activity(&p, 1234, false);
        assert!(plain.get("assets").is_none() && plain.get("party").is_none() && plain.get("buttons").is_none());
    }

    #[test]
    fn launcher_yields_to_games_and_respects_the_switch() {
        omsi_launcher_lib::i18n::pin_language("ENG");
        let idle = Presence::for_launcher(true, false, false, true).unwrap();
        assert_eq!(idle.details, "In launcher");
        assert_eq!(idle.buttons.len(), 2);
        assert!(Presence::for_launcher(true, false, false, false).unwrap().buttons.is_empty());
        assert_eq!(Presence::for_launcher(false, false, false, true), None);
        assert_eq!(Presence::for_launcher(true, true, false, true), None);
        assert_eq!(Presence::for_launcher(true, false, true, true), None);
        assert_eq!(Presence::for_launcher(true, false, false, true), Some(idle));
    }

    #[test]
    fn dropping_presence_does_not_wait_for_a_connecting_worker() {
        use std::sync::mpsc;
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let (release, connecting) = mpsc::channel();
        let (done, finished) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            connecting.recv().unwrap();
            assert!(worker_stop.load(Ordering::Acquire));
            done.send(()).unwrap();
        });
        let discord = Discord {
            wanted: Arc::new(Mutex::new(None)),
            stop,
            worker: Some(worker),
        };
        let (dropped, returned) = mpsc::channel();
        let ui = std::thread::spawn(move || {
            drop(discord);
            dropped.send(()).unwrap();
        });
        let result = returned.recv_timeout(Duration::from_millis(250));
        release.send(()).unwrap();
        ui.join().unwrap();
        finished.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(
            result.is_ok(),
            "dropping presence blocked on the connection"
        );
    }

    #[test]
    fn short_bus_label_preserves_full_name_in_tooltip() {
        omsi_launcher_lib::i18n::pin_language("ENG");
        let game = Game {
            map: Some("Grundorf"),
            bus: Some(("MB O550 Euro3 Automatik", "Thueringer Wald MB O550 Euro3 Automatik")),
            duty: Some(("76", "1")),
            ..Default::default()
        };
        let presence = Presence::for_game(&game, Show::default()).unwrap();
        assert_eq!(presence.details, "Grundorf | Line 76");
        assert_eq!(presence.state, "MB O550 Euro3... | Tour 1");
        assert!(presence
            .large_text
            .contains("Thueringer Wald MB O550 Euro3 Automatik"));
        assert_eq!(compact("🚌".repeat(20).as_str(), 16).chars().count(), 16);
    }

    #[test]
    fn a_driven_bus_with_an_empty_type_uses_its_full_name() {
        omsi_launcher_lib::i18n::pin_language("ENG");
        for short in ["", "   "] {
            let free = Game { map: Some("Grundorf"), bus: Some((short, "MAN NL202")), ..Default::default() };
            let free = Presence::for_game(&free, Show::default()).unwrap();
            assert_eq!(free.state, "MAN NL202 | Free drive");
            let duty = Game { map: Some("Grundorf"), bus: Some((short, "MAN NL202")), duty: Some(("76", "1")), ..Default::default() };
            let duty = Presence::for_game(&duty, Show::default()).unwrap();
            assert_eq!(duty.state, "MAN NL202 | Tour 1");
            assert!(duty.large_text.ends_with("MAN NL202"));
        }
    }

    #[test]
    fn frame_reader_performs_only_one_read_per_poll() {
        struct ByteReader {
            bytes: std::io::Cursor<Vec<u8>>,
            reads: usize,
        }
        impl Read for ByteReader {
            fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
                self.reads += 1;
                self.bytes.read(&mut buffer[..1])
            }
        }
        let mut packet = Vec::new();
        packet.extend_from_slice(&1u32.to_le_bytes());
        packet.extend_from_slice(&(MAX_FRAME as u32).to_le_bytes());
        packet.resize(MAX_FRAME + 8, b' ');
        let mut reader = ByteReader {
            bytes: std::io::Cursor::new(packet),
            reads: 0,
        };
        let mut pending = Vec::new();
        for expected in 1..=32 {
            assert_eq!(read_frame(&mut reader, &mut pending).unwrap(), None);
            assert_eq!(reader.reads, expected);
        }
    }

    #[test]
    fn empty_application_id_uses_the_project_application() {
        assert_eq!(DEFAULT_APP_ID, "1568733650634145792");
    }
}
