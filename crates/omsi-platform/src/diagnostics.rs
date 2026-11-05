use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, TryLockError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const KEEP: Duration = Duration::from_secs(30 * 24 * 3600);
const MAX_FILES: usize = 100;
const UP_TO_MS: [f64; 10] = [8.4, 11.2, 16.8, 20.0, 25.0, 33.4, 50.0, 100.0, 250.0, 1000.0];
const STACK_FRAMES: usize = 24;
const RECORD_VERSION: u32 = 1;

type Record = Option<(PathBuf, Value)>;

static CURRENT: Mutex<Record> = Mutex::new(None);
static GAME: AtomicBool = AtomicBool::new(false);
static PURGED: AtomicBool = AtomicBool::new(false);
static SEQUENCE: AtomicU32 = AtomicU32::new(0);

#[derive(Default)]
pub struct Frames {
    counts: [u32; UP_TO_MS.len() + 1],
    seconds: f64,
    worst_ms: f64,
}

impl Frames {
    pub fn add(&mut self, dt: f32) {
        let ms = dt as f64 * 1000.0;
        if !ms.is_finite() || ms < 0.0 {
            return;
        }
        let i = UP_TO_MS.iter().position(|&b| ms <= b).unwrap_or(UP_TO_MS.len());
        self.counts[i] = self.counts[i].saturating_add(1);
        self.seconds += dt as f64;
        self.worst_ms = self.worst_ms.max(ms);
    }

    pub fn to_json(&self) -> Value {
        let frames: u64 = self.counts.iter().map(|&c| c as u64).sum();
        let fps = if self.seconds > 0.0 { frames as f64 / self.seconds } else { 0.0 };
        json!({
            "frames": frames,
            "seconds": self.seconds.round(),
            "average_fps": (fps * 10.0).round() / 10.0,
            "worst_ms": self.worst_ms.round(),
            "up_to_ms": UP_TO_MS,
            "counts": self.counts,
        })
    }
}

pub fn dir() -> PathBuf {
    omsi_launcher_lib::data_dir().join("diagnostics")
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|t| t.as_secs()).unwrap_or(0)
}

fn opened(dir: &Path) -> (PathBuf, Value) {
    let path = dir.join(format!("{}-{}-{}.json", now(), std::process::id(), SEQUENCE.fetch_add(1, Ordering::Relaxed)));
    (path, json!({
        "record_version": RECORD_VERSION, "version": crate::VERSION, "build": crate::BUILD,
        "os": std::env::consts::OS, "architecture": std::env::consts::ARCH,
        "started_unix_seconds": now(), "end": "open",
    }))
}

fn is_open(record: &Value) -> bool {
    record["end"] == "open"
}

fn carried(mut fresh: Value, old: &Value) -> Value {
    for key in ["started_unix_seconds", "error", "panics"] {
        if !old[key].is_null() {
            fresh[key] = old[key].clone();
        }
    }
    fresh["record_version"] = json!(RECORD_VERSION);
    fresh["recorded_unix_seconds"] = json!(now());
    fresh
}

fn lock_briefly(cell: &'static Mutex<Record>) -> Option<MutexGuard<'static, Record>> {
    for _ in 0..100 {
        match cell.try_lock() {
            Ok(guard) => return Some(guard),
            Err(TryLockError::Poisoned(e)) => return Some(e.into_inner()),
            Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(10)),
        }
    }
    None
}

pub fn game_started() {
    GAME.store(true, Ordering::Relaxed);
}

pub fn start(value: Value) {
    let _ = save_in(&CURRENT, dir(), value, true);
}

pub fn save(value: Value) {
    let _ = save_in(&CURRENT, dir(), value, false);
}

pub fn ended(end: &str, fresh: Option<Value>) {
    ended_in(&CURRENT, &GAME, end, fresh);
}

pub fn panicked(error: Value) {
    panicked_in(&CURRENT, &GAME, &dir(), error);
}

fn save_in(cell: &'static Mutex<Record>, dir: PathBuf, value: Value, new_game: bool) -> Option<std::thread::JoinHandle<()>> {
    let mut current = match cell.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::Poisoned(e)) => e.into_inner(),
        Err(TryLockError::WouldBlock) => return None,
    };
    let (path, old) = match current.take() {
        Some((p, old)) if is_open(&old) && (!new_game || old["session"].is_null()) => (p, old),
        Some(record) if !new_game => {
            *current = Some(record);
            return None;
        }
        _ => opened(&dir),
    };
    let mut value = carried(value, &old);
    value["end"] = json!("open");
    crate::support_bundle::clean_json(&mut value);
    *current = Some((path, value));
    drop(current);
    std::thread::Builder::new().name("diagnostics".into()).spawn(move || {
        if !PURGED.swap(true, Ordering::Relaxed) {
            purge(&dir, SystemTime::now());
        }
        let current = cell.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((path, value)) = current.as_ref() {
            write(path, value);
        }
    }).ok()
}

fn ended_in(cell: &'static Mutex<Record>, game: &AtomicBool, end: &str, fresh: Option<Value>) {
    game.store(false, Ordering::Relaxed);
    let Some(mut current) = lock_briefly(cell) else { return };
    let Some((path, old)) = current.take() else { return };
    if !is_open(&old) {
        *current = Some((path, old));
        return;
    }
    let mut value = match fresh {
        Some(fresh) => carried(fresh, &old),
        None => carried(old.clone(), &old),
    };
    value["end"] = json!(end);
    crate::support_bundle::clean_json(&mut value);
    write(&path, &value);
    *current = Some((path, value));
}

fn panicked_in(cell: &'static Mutex<Record>, game: &AtomicBool, dir: &Path, mut error: Value) {
    let Some(mut current) = lock_briefly(cell) else { return };
    let (path, mut value) = match current.take() {
        Some((p, v)) if is_open(&v) => (p, v),
        _ if game.load(Ordering::Relaxed) => opened(dir),
        other => {
            *current = other;
            return;
        }
    };
    error["recorded_unix_seconds"] = json!(now());
    value["recorded_unix_seconds"] = json!(now());
    value["panics"] = json!(value["panics"].as_u64().unwrap_or(0) + 1);
    value["error"] = error;
    crate::support_bundle::clean_json(&mut value);
    write(&path, &value);
    *current = Some((path, value));
}

pub fn panic_report(info: &std::panic::PanicHookInfo, backtrace: &std::backtrace::Backtrace) -> Value {
    let payload = info.payload();
    let message = payload.downcast_ref::<&str>().map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_default();
    let mut message: String = omsi_launcher_lib::crash::redact(&without_quoted_text(&message))
        .chars().filter(|c| !c.is_control()).collect();
    let cut = (0..=message.len().min(200)).rev().find(|&i| message.is_char_boundary(i)).unwrap_or(0);
    message.truncate(cut);
    let (file, line) = info.location().map(|l| (source_file(l.file()), l.line())).unzip();
    json!({
        "message": message, "file": file, "line": line, "build": crate::BUILD,
        "stack": stack(&backtrace.to_string()),
        "thread": std::thread::current().name().map(|n| n.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').take(40).collect::<String>()),
    })
}

fn without_quoted_text(text: &str) -> String {
    let text = text.split(['\'', '"']).next().unwrap_or_default();
    let mut out = String::new();
    for (i, part) in text.split('`').enumerate() {
        if i % 2 == 0 {
            out.push_str(part);
        } else if part.contains("::") || part.ends_with("()") || ["Err", "Ok", "None", "Some"].contains(&part) {
            out.push_str(&format!("`{part}`"));
        } else {
            out.push_str("`...`");
        }
    }
    out
}

fn source_file(file: &str) -> String {
    let file = file.replace('\\', "/");
    if file.starts_with("crates/") {
        file
    } else {
        file.rsplit('/').next().unwrap_or_default().to_string()
    }
}

fn stack(text: &str) -> Vec<String> {
    const MACHINERY: &[&str] = &["std::", "core::", "alloc::", "<std::", "<core::", "<alloc::", "rust_", "__", "_start", "main", "<unknown>"];
    let symbols: Vec<&str> = text.lines()
        .filter_map(|l| {
            let (n, symbol) = l.trim_start().split_once(": ")?;
            n.parse::<u32>().ok()?;
            Some(symbol.trim())
        })
        .collect();
    let thrown = symbols.iter().rposition(|s| s.contains("panicking") || s.contains("begin_unwind")).map_or(0, |i| i + 1);
    symbols[thrown..].iter()
        .filter(|s| !MACHINERY.iter().any(|m| s.starts_with(m)))
        .map(|&s| match s.rsplit_once("::h") {
            Some((name, hash)) if hash.len() == 16 && hash.chars().all(|c| c.is_ascii_hexdigit()) => name,
            _ => s,
        })
        .take(STACK_FRAMES)
        .map(String::from)
        .collect()
}

fn write(path: &Path, value: &Value) {
    let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) else { return };
    let _ = std::fs::create_dir_all(dir);
    let temp = path.with_extension("tmp");
    if let Ok(bytes) = serde_json::to_vec_pretty(value) {
        if std::fs::write(&temp, bytes).is_ok() {
            let _ = std::fs::rename(temp, path);
        }
    }
}

fn records(dir: &Path) -> Vec<(SystemTime, PathBuf)> {
    let mut all: Vec<(SystemTime, PathBuf)> = std::fs::read_dir(dir).into_iter().flatten().flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json" || x == "tmp"))
        .filter_map(|p| Some((std::fs::metadata(&p).ok()?.modified().ok()?, p)))
        .collect();
    all.sort_by(|a, b| b.0.cmp(&a.0));
    all
}

fn purge(dir: &Path, now: SystemTime) {
    for (i, (modified, path)) in records(dir).into_iter().enumerate() {
        let old = now.duration_since(modified).is_ok_and(|age| age > KEEP);
        if old || i >= MAX_FILES {
            let _ = std::fs::remove_file(path);
        }
    }
}

pub fn recent() -> Vec<(String, Value)> {
    let now = SystemTime::now();
    records(&dir()).into_iter()
        .filter(|(modified, path)| path.extension().is_some_and(|x| x == "json") && now.duration_since(*modified).map_or(true, |age| age <= KEEP))
        .filter_map(|(_, path)| {
            let bytes = std::fs::read(&path).ok().filter(|b| b.len() <= 256 * 1024)?;
            Some((path.file_name()?.to_string_lossy().into_owned(), serde_json::from_slice(&bytes).ok()?))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_times_fall_into_their_bands() {
        let mut f = Frames::default();
        for dt in [0.005, 0.016, 0.016, 0.040, 0.120, 2.0, f32::NAN, -1.0] {
            f.add(dt);
        }
        let j = f.to_json();
        assert_eq!(j["frames"], 6);
        assert_eq!(j["counts"], json!([1, 0, 2, 0, 0, 0, 1, 0, 1, 0, 1]));
        assert_eq!(j["worst_ms"], 2000.0);
    }

    #[test]
    fn the_stack_keeps_the_game_frames_without_hashes() {
        let text = "   0: std::backtrace::Backtrace::force_capture\n             at /rustc/abc/library/std/src/backtrace.rs:312:13\n   1: omsi_app::run::{{closure}}::h0123456789abcdef\n   2: core::panicking::panic_fmt\n   3: omsi_sim::bus::Bus::step::h0123456789abcdef\n             at /home/isaac/openOMSI/crates/omsi-sim/src/bus.rs:10:5\n   4: <unknown>\n   5: omsi_app::frame\n   6: main\n";
        assert_eq!(stack(text), ["omsi_sim::bus::Bus::step", "omsi_app::frame"]);
    }

    #[test]
    fn quoted_text_leaves_the_panic_message() {
        assert_eq!(without_quoted_text("byte index 5 is out of bounds of `Isaac said hi`"), "byte index 5 is out of bounds of `...`");
        assert_eq!(without_quoted_text("called `Result::unwrap()` on an `Err` value: Os { message: \"C:\\Users\\x\" }"),
            "called `Result::unwrap()` on an `Err` value: Os { message: ");
        assert_eq!(without_quoted_text("it is inside 'é' of `abcdé`"), "it is inside ");
        assert_eq!(without_quoted_text("index out of bounds: the len is 0 but the index is 11"), "index out of bounds: the len is 0 but the index is 11");
    }

    #[test]
    fn only_workspace_sources_keep_their_folders() {
        assert_eq!(source_file("crates/omsi-sim/src/bus.rs"), "crates/omsi-sim/src/bus.rs");
        assert_eq!(source_file("/Users/isaac/.cargo/registry/src/index/wgpu-25.0.0/src/lib.rs"), "lib.rs");
        assert_eq!(source_file("C:\\Users\\isaac\\.cargo\\registry\\src\\x\\y.rs"), "y.rs");
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("omsi-diagnostics-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn saved(dir: &Path) -> Vec<Value> {
        let mut all: Vec<(String, Value)> = std::fs::read_dir(dir).unwrap().flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .map(|e| (e.file_name().to_string_lossy().into_owned(), serde_json::from_slice(&std::fs::read(e.path()).unwrap()).unwrap()))
            .collect();
        all.sort_by_key(|(name, _)| name.rsplit('-').next().and_then(|n| n.trim_end_matches(".json").parse::<u32>().ok()));
        all.into_iter().map(|(_, v)| v).collect()
    }

    fn written(handle: Option<std::thread::JoinHandle<()>>) {
        handle.expect("a write").join().unwrap();
    }

    #[test]
    fn a_game_keeps_one_record_to_its_end_and_the_next_game_gets_its_own() {
        static CELL: Mutex<Record> = Mutex::new(None);
        static IN_GAME: AtomicBool = AtomicBool::new(true);
        let dir = temp_dir("games");
        let game = |frames: u32| json!({"session": {"map": "maps/A/global.cfg"}, "frame_times": {"frames": frames}});
        written(save_in(&CELL, dir.clone(), game(1), true));
        panicked_in(&CELL, &IN_GAME, &dir, json!({"message": "caught"}));
        written(save_in(&CELL, dir.clone(), game(2), false));
        ended_in(&CELL, &IN_GAME, "clean", Some(game(3)));
        assert!(save_in(&CELL, dir.clone(), game(4), false).is_none());
        ended_in(&CELL, &IN_GAME, "device_lost", None);
        panicked_in(&CELL, &IN_GAME, &dir, json!({"message": "in the launcher after the game"}));
        assert_eq!(saved(&dir).len(), 1);
        IN_GAME.store(true, Ordering::Relaxed);
        let first = &saved(&dir)[0];
        assert_eq!(first["end"], "clean");
        assert_eq!(first["frame_times"]["frames"], 3);
        assert_eq!(first["error"]["message"], "caught");
        assert_eq!(first["panics"], 1);
        assert_eq!(first["record_version"], RECORD_VERSION);
        assert!(first["started_unix_seconds"].is_u64());
        written(save_in(&CELL, dir.clone(), game(5), true));
        written(save_in(&CELL, dir.clone(), game(6), true));
        let all = saved(&dir);
        assert_eq!(all.len(), 3);
        assert_eq!((all[1]["end"].as_str(), all[1]["frame_times"]["frames"].as_u64()), (Some("open"), Some(5)));
        assert!(all[2]["error"].is_null());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_panic_while_loading_stays_with_its_game_and_other_programs_keep_none() {
        static GAME_CELL: Mutex<Record> = Mutex::new(None);
        static TOOL_CELL: Mutex<Record> = Mutex::new(None);
        static IN_GAME: AtomicBool = AtomicBool::new(true);
        static IN_TOOL: AtomicBool = AtomicBool::new(false);
        let dir = temp_dir("loading");
        panicked_in(&GAME_CELL, &IN_GAME, &dir, json!({"message": "loading"}));
        written(save_in(&GAME_CELL, dir.clone(), json!({"session": {"map": "maps/A/global.cfg"}}), true));
        let all = saved(&dir);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0]["error"]["message"], "loading");
        assert_eq!(all[0]["session"]["map"], "maps/A/global.cfg");
        let tool = temp_dir("tool");
        panicked_in(&TOOL_CELL, &IN_TOOL, &tool, json!({"message": "helper"}));
        assert!(saved(&tool).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&tool);
    }

    #[test]
    fn records_older_than_thirty_days_and_past_the_cap_go() {
        let dir = std::env::temp_dir().join(format!("omsi-diagnostics-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..MAX_FILES + 3 {
            std::fs::write(dir.join(format!("{i}.json")), "{}").unwrap();
        }
        std::fs::write(dir.join("note.txt"), "").unwrap();
        purge(&dir, SystemTime::now());
        assert_eq!(records(&dir).len(), MAX_FILES);
        purge(&dir, SystemTime::now() + KEEP + Duration::from_secs(60));
        assert!(records(&dir).is_empty());
        assert!(dir.join("note.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
