//! The privacy-filtered support package (`openomsi --export-diagnostics`, Setup → Export
//! diagnostics): what may go in, the logs' projection, and the ZIP. The game fills in what
//! it ran with (`omsi-app`'s `support_bundle::snapshot`); nothing is sent anywhere.

use serde_json::Value;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const LOG_BYTES: u64 = 2 * 1024 * 1024;

/// Conservative for labels: a path, network address or credential is never useful
/// as a GPU/driver/controller label. Paths of content are handled separately.
pub fn redact_label(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let sensitive = ["token", "password", "passwd", "secret", "cookie", "authorization",
        "credential", "session", "discord", "serial", "api_key", "apikey", "bearer"];
    // (a colon alone is no reason: the build's date has one, "13:05")
    let address = |w: &str| {
        let w = w.trim_matches(['[', ']', '(', ')', ',', ';']);
        w.parse::<std::net::IpAddr>().is_ok() || w.parse::<std::net::SocketAddr>().is_ok()
            // (a Steam or Discord id: 17 digits or more)
            || w.chars().filter(char::is_ascii_digit).count() >= 15
    };
    if text.len() > 256 || text.contains('/') || text.contains('\\') || text.contains('@')
        || text.contains("://") || sensitive.iter().any(|s| lower.contains(s))
        || text.split_whitespace().any(address)
    {
        "[redacted]".into()
    } else {
        text.chars().filter(|c| !c.is_control()).collect()
    }
}

const SHAREABLE_FOLDERS: [&str; 2] = ["maps/", "Vehicles/"];

fn is_source_file(text: &str) -> bool {
    text.starts_with("crates/") && text.ends_with(".rs") && !text.contains("..")
        && text.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | '-'))
}

pub fn content_name(text: &str) -> String {
    let text = text.replace('\\', "/");
    if !SHAREABLE_FOLDERS.iter().any(|f| text.starts_with(f))
        || text.split('/').any(|c| c == ".." || c.is_empty())
        || text.contains(':')
    {
        return "[redacted]".into();
    }
    text.split('/').map(redact_label).collect::<Vec<_>>().join("/")
}

pub fn clean_json(value: &mut Value) {
    match value {
        Value::String(s) => {
            *s = if is_source_file(s) {
                s.clone()
            } else if SHAREABLE_FOLDERS.iter().any(|f| s.starts_with(f)) {
                content_name(s)
            } else { redact_label(s) };
        }
        Value::Array(a) => a.iter_mut().for_each(clean_json),
        Value::Object(o) => o.values_mut().for_each(clean_json),
        _ => {}
    }
}

/// No raw strings from a log enter a bundle. Even known events carry only finite
/// numbers and a fixed event name (so a credential on a continuation line is safe).
fn log_projection(text: &str) -> String {
    const EVENTS: &[(&str, &str)] = &[
        ("stutter: frame", "stutter"), ("profile ", "profile"),
        ("graphics device was lost", "device_lost"), ("device lost (", "device_lost"),
        ("pipelines failed", "pipeline_failure"), ("graphics device opened", "device_opened"),
        ("graphics adapter:", "adapter"), ("opening graphics device:", "device_request"),
        ("renderer:", "renderer"), ("system:", "system"),
        ("without multisampling", "msaa_fallback"), ("basic pipelines", "basic_fallback"),
        ("graphics card cannot keep up", "graphics_governor"),
        ("status:", "status"), ("game stopped on an error", "panic"),
    ];
    let mut out = String::from("Privacy projection: free text, paths, addresses and unrecognised log lines omitted. Numeric fields are positional, not named.\n");
    for (line_index, line) in text.lines().enumerate() {
        let lower = line.to_ascii_lowercase();
        if let Some((_, event)) = EVENTS.iter().find(|(needle, _)| lower.contains(needle)) {
            // Do not harvest numbers from credential-bearing lines either.
            let private = ["token", "password", "secret", "cookie", "authorization", "serial", "bearer"]
                .iter().any(|word| lower.contains(word));
            let numbers: Vec<f64> = if private { vec![] } else {
                // Parse only the fixed stutter header; a free-form number could
                // itself be a password, phone number or hardware serial.
                line.split_once("stutter: frame ").and_then(|(_, rest)| {
                    let words: Vec<_> = rest.split_whitespace().take(5).collect();
                    if words.len() != 5 || words[1] != "took" || words[3] != "ms" { return None; }
                    let frame = words[0].parse::<u32>().ok()?;
                    let ms = words[2].parse::<f64>().ok().filter(|n| n.is_finite() && *n >= 0.0)?;
                    Some(vec![frame as f64, ms])
                }).unwrap_or_default()
            };
            out.push_str(&format!("{} {} {:?}\n", line_index + 1, event, numbers));
        }
    }
    out
}

fn tail(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let size = file.metadata()?.len();
    file.seek(SeekFrom::Start(size.saturating_sub(LOG_BYTES)))?;
    let mut bytes = Vec::new();
    file.take(LOG_BYTES).read_to_end(&mut bytes)?;
    let text = String::from_utf8_lossy(&bytes);
    // A tail starting inside a line cannot reliably identify that line's context.
    Ok(if size > LOG_BYTES { text.split_once('\n').map(|(_, t)| t).unwrap_or("").into() } else { text.into_owned() })
}

/// Where a game started from the launcher keeps its snapshot (`~/.openomsi/support/<id>.json`).
pub fn session_dir() -> PathBuf {
    omsi_launcher_lib::data_dir().join("support")
}

fn summary(value: &Value) -> String {
    format!("openOMSI support package (schema 1)\n\n{}\n\nLogs are privacy projections, not raw log copies. Null means unavailable.\nRoots and archives are aliases; no absolute paths or archive filenames are exported.\nhistory/ holds a record of each game of the last 30 days: the day it was played, frame times, how it ended and its error, filtered the same way.\nSettings are allowlisted; no environment, command line, chat, player identity, LAN codes, addresses, configuration files or serial numbers are attached.\n", serde_json::to_string_pretty(value).unwrap_or_default())
}

fn archive(out: &Path, value: &Value, logs: &[(String, String)], history: &[(String, Value)]) -> anyhow::Result<()> {
    // create_new: a mistaken filename must never overwrite an unrelated file.
    let file = std::fs::OpenOptions::new().write(true).create_new(true).open(out)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let result = (|| -> anyhow::Result<()> {
        zip.start_file("diagnostics.txt", opts)?;
        zip.write_all(summary(value).as_bytes())?;
        zip.start_file("diagnostics.json", opts)?;
        zip.write_all(&serde_json::to_vec_pretty(value)?)?;
        for (name, text) in logs {
            zip.start_file(format!("logs/{name}"), opts)?;
            zip.write_all(log_projection(text).as_bytes())?;
        }
        for (name, record) in history {
            zip.start_file(format!("history/{name}"), opts)?;
            zip.write_all(&serde_json::to_vec_pretty(record)?)?;
        }
        Ok(())
    })();
    let result = result.and_then(|_| zip.finish().map(|_| ()).map_err(Into::into));
    if result.is_err() { let _ = std::fs::remove_file(out); }
    result
}

/// The caller supplies trusted in-process metadata. Only fixed app log basenames
/// are read; no directory recursion and no user's configuration is attached.
pub fn export(out: &Path, value: &Value) -> anyhow::Result<()> {
    let dir = omsi_launcher_lib::data_dir();
    let mut value = value.clone();
    let recent = omsi_launcher_lib::list_instances().into_iter().max_by_key(|i| i.started);
    if let Some(instance) = &recent {
        let path = session_dir().join(format!("{}.json", instance.id));
        if let Ok(file) = std::fs::File::open(path) {
            let mut bytes = Vec::new();
            file.take(64 * 1024).read_to_end(&mut bytes)?;
            if let Ok(mut game) = serde_json::from_slice::<Value>(&bytes) {
                // Cache files are not raw attachments. Unknown keys are discarded,
                // and all remaining strings pass the same privacy filter again.
                filter_keys(&mut game, SNAPSHOT_KEYS);
                clean_json(&mut game);
                value["last_recorded_game"] = game;
            }
        }
    }
    let game_log = recent.as_ref().filter(|i| i.slot > 1 && i.slot <= 128)
        .map(|i| format!("game-{}.log", i.slot)).unwrap_or_else(|| "game.log".into());
    let logs: Vec<_> = [game_log.as_str(), "launcher.log", "crash.log"].iter()
        .filter_map(|name| tail(&dir.join(name)).ok().map(|text| (name.to_string(), text))).collect();
    to_the_day(&mut value);
    archive(out, &value, &logs, &history())
}

fn history() -> Vec<(String, Value)> {
    crate::diagnostics::recent().into_iter().enumerate()
        .map(|(i, (_, mut record))| {
            filter_keys(&mut record, &[SNAPSHOT_KEYS, HISTORY_KEYS].concat());
            clean_json(&mut record);
            to_the_day(&mut record);
            (format!("game-{}.json", i + 1), record)
        })
        .collect()
}

fn to_the_day(value: &mut Value) {
    match value {
        Value::Object(o) => {
            for (key, v) in o.iter_mut() {
                match v.as_u64() {
                    Some(t) if key.ends_with("unix_seconds") => *v = t.saturating_sub(t % 86_400).into(),
                    _ => to_the_day(v),
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(to_the_day),
        _ => {}
    }
}

const SNAPSHOT_KEYS: &[&str] = &["schema_version", "recorded_unix_seconds", "source", "version", "build",
        "os", "architecture", "os_version", "graphics", "backend", "device_type",
        "adapter", "vendor_id", "device_id", "driver", "driver_info", "device_features", "device_limits",
        "max_texture_dimension_2d", "max_buffer_size", "max_storage_buffer_binding_size", "max_bind_groups",
        "max_sampled_textures_per_shader_stage", "max_storage_buffers_per_shader_stage", "msaa", "ssao",
        "render_scale", "shadow_size", "basic_pipelines", "device_lost", "graphics_effective", "graphics_settings", "preset",
        "backend_requested", "shadows", "mirrors", "mirror_size", "reflections", "vsync", "max_fps",
        "session", "map", "bus", "line", "tour", "content_roots", "mounted_archives", "alias", "priority",
        "files", "bytes", "controllers", "name", "vid_pid", "axes", "buttons",
        "gamepad", "ffb_capable", "ffb_effect_available", "physical_memory_bytes", "adapter_vram_mb",
        "adapter_texture_mb", "gpu_memory_bytes", "textures", "meshes"];

const HISTORY_KEYS: &[&str] = &["record_version", "started_unix_seconds", "end", "error", "message", "file", "stack",
    "thread", "panics", "frame_times", "frames", "seconds", "average_fps", "worst_ms", "up_to_ms", "counts"];

fn filter_keys(value: &mut Value, keys: &[&str]) {
    match value {
        Value::Object(o) => { o.retain(|k, _| keys.contains(&k.as_str())); o.values_mut().for_each(|v| filter_keys(v, keys)); }
        Value::Array(a) => a.iter_mut().for_each(|v| filter_keys(v, keys)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn labels_redact_personal_paths_addresses_and_credentials() {
        for s in ["/home/isaac/private.txt", "C:\\Users\\Isaac\\Desktop\\private.txt",
            "\\\\server\\share\\private.txt", "192.168.0.1", "2001:db8::1", "joined 10.0.0.7:5555",
            "https://user:password@server", "Bearer abcd", "cookie=abcd",
            "Discord credentials", "serial ABC123", "api_key=abcd", "token abcd", "Steam 76561198000000000", "user 123456789012345678"] {
            assert_eq!(redact_label(s), "[redacted]", "{s}");
        }
        assert_eq!(redact_label("AMD Radeon RX 6750 XT"), "AMD Radeon RX 6750 XT");
        assert_eq!(redact_label("Steam Deck Controller"), "Steam Deck Controller");
        assert_eq!(redact_label("5f409baf 2026-10-08 13:05"), "5f409baf 2026-10-08 13:05");
        assert_eq!(content_name("Vehicles/Bus/vehicle.bus"), "Vehicles/Bus/vehicle.bus");
        assert_eq!(content_name("maps/../../private.txt"), "[redacted]");
        assert_eq!(content_name("/Users/isaac/OMSI 2/maps/Grundorf/global.cfg"), "[redacted]");
        let mut v = json!(["crates/omsi-sim/src/bus.rs", "crates/../../Users/isaac/x.rs", "crates/a b/x.rs", "crates/notes.txt"]);
        clean_json(&mut v);
        assert_eq!(v, json!(["crates/omsi-sim/src/bus.rs", "[redacted]", "[redacted]", "[redacted]"]));
    }

    #[test]
    fn cached_snapshot_is_filtered_again_before_export() {
        let mut v = json!({"password": "123456", "unknown_filename.txt": "private",
            "graphics": {"adapter": "C:\\Users\\Isaac\\gpu", "vendor_id": 4098, "cookie": "SECRET"}});
        filter_keys(&mut v, SNAPSHOT_KEYS);
        clean_json(&mut v);
        assert_eq!(v, json!({"graphics": {"adapter": "[redacted]", "vendor_id": 4098}}));
    }

    #[test]
    fn a_game_record_keeps_its_frame_times_and_error_but_no_private_text() {
        let mut v = json!({"end": "panic", "chat": "hello", "frame_times": {"frames": 10, "counts": [10]},
            "error": {"message": "cannot open C:\\Users\\Isaac\\x", "file": "crates/omsi-sim/src/bus.rs", "line": 3,
                "stack": ["omsi_sim::bus::Bus::step"], "player": "Isaac"}});
        filter_keys(&mut v, &[SNAPSHOT_KEYS, HISTORY_KEYS].concat());
        clean_json(&mut v);
        let mut times = json!({"started_unix_seconds": 1_793_785_869u64, "error": {"recorded_unix_seconds": 1_793_786_000u64}, "frames": 7,
            "last_recorded_game": {"recorded_unix_seconds": 1_793_785_870u64}});
        to_the_day(&mut times);
        assert_eq!(times, json!({"started_unix_seconds": 1_793_750_400u64, "error": {"recorded_unix_seconds": 1_793_750_400u64}, "frames": 7,
            "last_recorded_game": {"recorded_unix_seconds": 1_793_750_400u64}}));
        assert_eq!(v, json!({"end": "panic", "frame_times": {"frames": 10, "counts": [10]},
            "error": {"message": "[redacted]", "file": "crates/omsi-sim/src/bus.rs", "line": 3,
                "stack": ["omsi_sim::bus::Bus::step"]}}));
    }

    #[test]
    fn log_projection_never_copies_free_text_even_in_known_events() {
        let input = "renderer: /home/isaac/private.txt 192.168.0.1 cookie=SECRET\n\
            profile gpu: password SECRET 123\nsecret continuation line\n\
            LAN: hosting session 1a2b on port 7777 as 'Isaac' (protocol 9), code ABCD-EFGH\n\
            LAN chat <Isaac> meet at 12\n\
            stutter: frame 42 took 100 ms (render 80)\n\
            renderer: unrelated-personal-filename.txt\n";
        let out = log_projection(input);
        for private in ["isaac", "Isaac", "private.txt", "192.168", "cookie", "SECRET", "123", "ABCD", "meet", "unrelated-personal"] {
            assert!(!out.contains(private), "{out}");
        }
        assert!(out.contains("stutter [42.0, 100.0]"));
    }

    #[test]
    fn archive_has_only_fixed_entries_and_refuses_overwrite() {
        let path = std::env::temp_dir().join(format!("omsi-support-test-{}.zip", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let history = [("1-2.json".to_string(), json!({"end": "clean"}))];
        archive(&path, &json!({"schema_version": 1}), &[("game.log".into(), "chat: SECRET".into())], &history).unwrap();
        let mut z = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        assert_eq!(z.len(), 4);
        assert!(z.by_name("history/1-2.json").is_ok());
        assert_eq!(z.by_index(0).unwrap().name().unwrap(), "diagnostics.txt");
        assert_eq!(z.by_index(1).unwrap().name().unwrap(), "diagnostics.json");
        let mut log = String::new();
        z.by_name("logs/game.log").unwrap().read_to_string(&mut log).unwrap();
        assert!(!log.contains("SECRET"));
        assert!(archive(&path, &Value::Null, &[], &[]).is_err());
        drop(z);
        std::fs::remove_file(path).unwrap();
    }
}
