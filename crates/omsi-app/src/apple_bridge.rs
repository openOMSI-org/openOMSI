//! The Swift launcher's way into the launcher's data (iPhone, iPad, Mac).
//!
//! Apple's systems get a launcher of their own, written in SwiftUI (`apple/Shared`) - but what
//! it knows and does is not written here twice: it is the launcher's model,
//! `launcher::state::State` (crate omsi-launcher-model: the content lists, the duty chosen, the
//! timetable, profiles, mods, servers ...), read as JSON and changed by actions through two C
//! functions:
//!
//! - `openomsi_bridge(cmd, json)` → a JSON reply `{"ok": true, "value": …}` or
//!   `{"ok": false, "error": "…"}`, freed with `openomsi_bridge_free`;
//! - `openomsi_preview_frame(…)`: the bus preview, drawn by the game's own renderer (the
//!   launcher's showroom) off the screen and copied into the Swift side's pixels.
//!
//! The commands: `init`, `tick {dt}` (the state's background work comes in; the reply is the
//! snapshot the pages are drawn from), `lists` (maps, buses, weathers), `lines`,
//! `map_overview` (the Drive page's map), `act {do: …}` (everything a page changes), `cli`
//! (the launcher core's own commands: presets, tutorials …).
//!
//! On a Mac a game is a process of its own, as the launcher starts it everywhere; on an
//! iPhone or an iPad it plays in this process (`ios.rs` takes the command line over).

use crate::launcher::state::{JoinProto, ServerEntry, State};
use omsi_launcher_lib as core;
use serde_json::{json, Value};
use std::ffi::{c_char, CStr, CString};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

static STATE: Mutex<Option<State>> = Mutex::new(None);
/// Bumped whenever the lists (maps, buses, weathers) or the lines change: the Swift side
/// fetches them again only then.
static LISTS_REV: AtomicU64 = AtomicU64::new(0);
static LINES_REV: AtomicU64 = AtomicU64::new(0);
/// A game runs in this process (an iPhone, an iPad) - from the launch until it ended.
static GAME_RUNNING: AtomicBool = AtomicBool::new(false);
/// A word from the game's start for the launcher (it could not start ...).
static GAME_STATUS: Mutex<Option<String>> = Mutex::new(None);

/// The original installation and the content roots, found once for the whole process (a
/// bare start's `prepare`), and what the launcher shows taken from the settings.
pub(crate) fn prepare_once() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        use clap::Parser;
        let args = crate::cli::Args::parse_from(["openomsi"]);
        if let Err(e) = crate::prepare(args, true) {
            log::error!("{e:#}");
        }
    });
}

/// The game in this process ended (or never started): the launcher is back.
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub(crate) fn game_ended() {
    GAME_RUNNING.store(false, Ordering::Relaxed);
    if let Some(s) = STATE.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        s.launch_hold = None;
        s.load_profile();
        s.poll_now();
    }
}

#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub(crate) fn game_status(text: &str) {
    *GAME_STATUS.lock().unwrap_or_else(|e| e.into_inner()) = Some(text.to_string());
}

// --- the C side ------------------------------------------------------------------------

/// # Safety
/// `cmd` and `arg` are nul-terminated strings (or null); the reply is freed with
/// `openomsi_bridge_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn openomsi_bridge(cmd: *const c_char, arg: *const c_char) -> *mut c_char {
    let read = |p: *const c_char| if p.is_null() { String::new() } else { unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned() };
    let (cmd, arg) = (read(cmd), read(arg));
    let reply = match std::panic::catch_unwind(|| call(&cmd, &arg)) {
        Ok(Ok(v)) => json!({ "ok": true, "value": v }),
        Ok(Err(e)) => json!({ "ok": false, "error": omsi_ui::tr(&format!("{e:#}")) }),
        Err(_) => json!({ "ok": false, "error": omsi_ui::tr("the launcher's `{cmd}` stopped on an error (see the log)").replace("{cmd}", &cmd) }),
    };
    CString::new(reply.to_string()).unwrap_or_default().into_raw()
}

/// # Safety
/// `p` is a reply of `openomsi_bridge` (or null), freed once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn openomsi_bridge_free(p: *mut c_char) {
    if !p.is_null() {
        drop(unsafe { CString::from_raw(p) });
    }
}

fn call(cmd: &str, arg: &str) -> anyhow::Result<Value> {
    let a: Value = serde_json::from_str(arg).unwrap_or(json!({}));
    match cmd {
        "init" => {
            prepare_once();
            let mut g = STATE.lock().unwrap_or_else(|e| e.into_inner());
            if g.is_none() {
                *g = Some(State::new());
                LISTS_REV.fetch_add(1, Ordering::Relaxed);
            }
            Ok(json!({
                "version": crate::startup::VERSION,
                "build": crate::startup::BUILD,
                "platform": if cfg!(target_os = "ios") { "ios" } else { "macos" },
                "in_process": core::IN_PROCESS_GAMES,
                "store": crate::updater::STORE_BUILD,
            }))
        }
        "cli" => {
            let c = a.get("cmd").and_then(|v| v.as_str()).unwrap_or("");
            let arg = a.get("arg").map(|v| v.to_string()).unwrap_or_else(|| "{}".into());
            core::cli(c, &arg)
        }
        "tutorials" => Ok(json!(core::tutorials().into_iter().map(|(n, title, text)| json!({ "n": n, "title": title, "text": text })).collect::<Vec<_>>())),
        "graphics_presets" => {
            Ok(json!(core::graphics_presets().iter().map(|(name, v)| json!({ "name": name, "values": v })).collect::<Vec<_>>()))
        }
        "graphics_profiles" => Ok(json!(core::graphics_profiles())),
        "languages" => Ok(json!(core::LANGUAGES.iter().map(|l| json!({ "code": l.0, "name": l.1, "iso": l.2 })).collect::<Vec<_>>())),
        "map_overview" => {
            let look = with_state(|s| {
                let file = s.choice.map.clone();
                let global = omsi_cfg::find_in_roots(&file).map(|(_, p)| p).unwrap_or_else(|| omsi_cfg::resolve_path(std::path::Path::new(&s.config.root), &file));
                let trip = s.tour().and_then(|t| t.trips.get(s.first_trip().unwrap_or(0))).map(|t| t.name.clone()).unwrap_or_default();
                Ok(crate::launcher::mapview::Look { map: file, global, date: s.choice.date.clone(), trip, entry: s.choice.entry })
            })?;
            // (the tiles are read here, a second or so on a big map: the state is not held
            // meanwhile - the Swift side asks from a queue of its own)
            Ok(crate::launcher::mapview::overview(look))
        }
        "find_root" => {
            let given = a.get("path").and_then(|v| v.as_str()).map(|p| std::path::PathBuf::from(p.trim()));
            match omsi_cfg::copied_install::look(given, core::content_dir) {
                omsi_cfg::copied_install::Look::Root(p) => {
                    with_state(|s| use_root(s, p.to_string_lossy().to_string()))?;
                    Ok(json!({ "found": true }))
                }
                omsi_cfg::copied_install::Look::Waiting => Ok(json!({ "found": false })),
                omsi_cfg::copied_install::Look::Missing(m) => Ok(json!({ "found": false, "missing": m })),
            }
        }
        "api" => {
            let c = a.get("cmd").and_then(|v| v.as_str()).unwrap_or("");
            omsi_launcher_api::call(c, a.get("args").unwrap_or(&json!({})), &Apple).map_err(anyhow::Error::msg)
        }
        _ => with_state(|s| state_call(s, cmd, &a)),
    }
}

struct Apple;

impl omsi_launcher_api::Platform for Apple {
    fn renderer(&self, _req: Value, _wait: std::time::Duration) -> Result<(Value, Vec<u8>), String> {
        Err("the Apple launcher draws its pictures through openomsi_preview_frame".into())
    }
    #[cfg(target_os = "ios")]
    fn previous_run_crash(&self) -> Option<(String, String)> {
        crate::ios::previous_run_crash()
    }
}

fn with_state<R>(f: impl FnOnce(&mut State) -> anyhow::Result<R>) -> anyhow::Result<R> {
    let mut g = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let s = g.as_mut().ok_or_else(|| anyhow::anyhow!("the launcher is not open (`init` first)"))?;
    f(s)
}

fn state_call(s: &mut State, cmd: &str, a: &Value) -> anyhow::Result<Value> {
    match cmd {
        "tick" => {
            let dt = a.get("dt").and_then(|v| v.as_f64()).unwrap_or(0.25) as f32;
            tick(s, dt);
            Ok(snapshot(s))
        }
        "snapshot" => Ok(snapshot(s)),
        "lists" => Ok(json!({ "maps": s.maps, "vehicles": s.vehicles, "weathers": s.weathers, "rev": LISTS_REV.load(Ordering::Relaxed) })),
        "lines" => Ok(json!({ "lines": s.lines, "for": [s.lines_for.0, s.lines_for.1], "rev": LINES_REV.load(Ordering::Relaxed) })),
        "act" => act(s, a),
        _ => anyhow::bail!("unknown command {cmd}"),
    }
}

/// The state's background work, and a game the launcher asked for.
fn tick(s: &mut State, dt: f32) {
    let before = (s.maps.len(), s.vehicles.len(), s.weathers.len(), s.loading_content);
    let lines_before = (s.lines_for.clone(), s.lines.len(), s.loading_lines);
    s.update(dt);
    if before != (s.maps.len(), s.vehicles.len(), s.weathers.len(), s.loading_content) {
        LISTS_REV.fetch_add(1, Ordering::Relaxed);
    }
    if lines_before != (s.lines_for.clone(), s.lines.len(), s.loading_lines) {
        LINES_REV.fetch_add(1, Ordering::Relaxed);
    }
    if let Some(text) = GAME_STATUS.lock().unwrap_or_else(|e| e.into_inner()).take() {
        s.set_status(text, true);
    }
    // the launcher's Start: the game's command line goes to the game (a Mac: a process of
    // its own; an iPhone: this process, `ios.rs` picks it up)
    if let Some(d) = s.queued_launch.take() {
        if core::IN_PROCESS_GAMES {
            preview_release();
            match core::launch(&d) {
                Ok(_) => {
                    GAME_RUNNING.store(true, Ordering::Relaxed);
                    s.set_status("Starting the game...", false);
                    #[cfg(target_os = "ios")]
                    crate::ios::wake();
                }
                Err(e) => s.set_status(format!("{e:#}"), true),
            }
        } else {
            s.spawn_launch(d);
        }
    }
}

fn snapshot(s: &mut State) -> Value {
    let preview = {
        let p = PREVIEW_STATE.lock().unwrap_or_else(|e| e.into_inner());
        let v = PREVIEW_VIEWS.lock().unwrap_or_else(|e| e.into_inner());
        json!({ "busy": p.0, "error": p.1, "view": v.0, "views": v.1 })
    };
    let root_ok = omsi_cfg::missing_original_essentials(std::path::Path::new(&s.config.root)).is_empty();
    let saved: Vec<Value> = s.saved_situations().iter().map(|x| json!({ "file": x.file.to_string_lossy(), "name": x.name, "saved": x.saved })).collect();
    let servers: Vec<Value> = s
        .servers
        .iter()
        .map(|e| {
            let info = s.server_info.get(&e.address).map(|(_, r)| match r {
                Ok(i) => json!({ "ok": true, "name": i.name, "motd": i.motd, "map": i.map, "players": i.players, "max_players": i.max_players, "version": i.version, "time": i.time, "weather": i.weather, "password": i.password, "vehicles": i.vehicles.len() }),
                Err(e) => json!({ "ok": false, "error": omsi_ui::tr(e) }),
            });
            json!({ "name": e.name, "address": e.address, "protocol": e.protocol, "info": info, "official": omsi_net::official::is_alias(&e.address) })
        })
        .collect();
    let mod_info = s.mod_info.as_ref().map(|r| match r {
        Ok(i) => json!({ "ok": true, "info": i }),
        Err(e) => json!({ "ok": false, "error": e }),
    });
    let ibis = s.ibis.as_ref().map(|(_, r)| match r {
        Ok(i) => json!({ "ok": true, "info": i }),
        Err(e) => json!({ "ok": false, "error": e }),
    });
    let in_game = if core::IN_PROCESS_GAMES { GAME_RUNNING.load(Ordering::Relaxed) } else { s.in_game() };
    json!({
        "lists_rev": LISTS_REV.load(Ordering::Relaxed),
        "lines_rev": LINES_REV.load(Ordering::Relaxed),
        "status": { "text": s.status.0, "error": s.status.1, "age": s.status.2.elapsed().as_secs_f32() },
        "loading_content": s.loading_content,
        "loading_lines": s.loading_lines,
        "counts": { "maps": s.maps.len(), "vehicles": s.vehicles.len(), "weathers": s.weathers.len(), "lines": s.lines.len() },
        "choice": s.choice,
        "default_hof": s.default_hof(),
        "season": s.season(),
        "first_trip": s.first_trip(),
        "picked_trip": s.picked_trip(),
        "real_time": s.real_time(),
        "config": { "root": s.config.root, "profile": s.config.profile, "game": s.config.game },
        "root_ok": root_ok,
        "root_problem": if root_ok { String::new() } else { omsi_ui::tr(&crate::launcher::state::root_problem(&s.config.root)).into_owned() },
        "content_dir": core::content_dir().map(|p| p.to_string_lossy().to_string()),
        "data_dir": core::data_dir().to_string_lossy(),
        "profiles": s.profiles,
        "profile": s.profile,
        "settings": s.settings,
        "mods": s.mods,
        "jobs": s.jobs,
        "mod_busy": s.mod_busy,
        "mod_info": mod_info,
        "instances": s.instances,
        "in_game": in_game,
        "crash": s.crash.as_ref().map(|(why, log)| json!({ "why": why, "log": log })),
        "disconnected": s.disconnected,
        "servers": servers,
        "joined_server": s.joined_server,
        "join": { "ok": s.join.0, "text": s.join.1 },
        "saved": saved,
        "save_pick": s.save_pick,
        "ibis": ibis,
        "cmdline": s.cmdline,
        "fresh": s.fresh.keys().collect::<Vec<_>>(),
        "host_vehicles": s.host_vehicles(),
        "preview": preview,
    })
}

fn use_root(s: &mut State, chosen: String) -> anyhow::Result<()> {
    let mut config = s.config.clone();
    config.root = chosen.clone();
    core::save_config(&config)?;
    s.config = config;
    omsi_cfg::content_changed();
    if !chosen.is_empty() {
        omsi_cfg::add_content_root(std::path::PathBuf::from(&chosen));
        if let Some(memo) = crate::startup::root_memo() {
            let _ = std::fs::write(memo, chosen.as_bytes());
        }
    }
    s.config = core::load_config();
    s.set_status("Saved. Reading the content again...", false);
    s.load_content();
    Ok(())
}

fn act(s: &mut State, a: &Value) -> anyhow::Result<Value> {
    let st = |k: &str| a.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let what = st("do");
    match what.as_str() {
        "select_bus" => s.select_bus(&st("file")),
        "select_map" => {
            let file = st("file");
            if s.choice.map != file {
                let span = s.maps.iter().find(|m| m.file == file).and_then(|m| m.year_span);
                s.choice.date = core::fit_date_to_years(&s.choice.date, span);
            }
            s.select_map(&file);
        }
        "paint" => {
            s.choice.paint = st("paint");
            s.touched();
        }
        "hof" => {
            // (empty: the depot file follows the map and the date again)
            let h = st("hof");
            s.choice.hof_manual = !h.is_empty();
            s.choice.hof = if h.is_empty() { s.default_hof() } else { h };
            s.touched();
        }
        "number" => {
            s.choice.number = st("number");
            s.touched();
        }
        "plate" => {
            s.choice.plate = st("plate");
            s.touched();
        }
        "entry" => {
            s.choice.entry = a.get("entry").and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
            s.touched();
        }
        "free" => {
            s.choice.free = a.get("on").and_then(|v| v.as_bool()).unwrap_or(true);
            s.touched();
        }
        "line" => {
            s.choice.free = false;
            let n = st("line");
            if s.choice.line.as_deref() != Some(n.as_str()) {
                s.choice.line = Some(n);
                s.choice.tour = None;
            }
            s.touched();
            s.load_ibis();
        }
        "tour" => {
            let num = st("tour");
            let other_day = s.line().and_then(|l| l.tour(&num)).filter(|t| !t.runs).and_then(|t| t.next_run.clone());
            // a tour of another day moves the date to the next day it runs (OMSI lists only
            // the day's tours)
            if let Some(n) = other_day {
                s.choice.date = n;
                s.load_lines();
            }
            s.choice.tour = Some(num);
            s.touched();
            s.load_ibis();
        }
        "pick_trip" => {
            let index = a.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let departure = a.get("departure").and_then(|v| v.as_f64()).unwrap_or(0.0);
            s.pick_trip(index, departure);
        }
        "time" => {
            s.choice.time = a.get("minutes").and_then(|v| v.as_i64()).unwrap_or(540).rem_euclid(24 * 60) as i32;
            s.touched();
        }
        "date" => {
            s.choice.date = st("date");
            s.choice.season = "auto".into();
            s.choice.own_date = None;
            s.load_lines();
            s.touched();
        }
        "now" => {
            if let Some((y, mo, d, h, m)) = core::local_now() {
                if a.get("time").and_then(|v| v.as_bool()).unwrap_or(true) {
                    s.choice.time = h * 60 + m;
                }
                if a.get("date").and_then(|v| v.as_bool()).unwrap_or(false) {
                    s.choice.date = format!("{y:04}-{mo:02}-{d:02}");
                    s.choice.season = "auto".into();
                    s.choice.own_date = None;
                    s.load_lines();
                }
                s.touched();
            }
        }
        "season" => {
            s.set_season(&st("season"));
            s.touched();
        }
        "phase" => {
            s.choice.phase = st("phase");
            s.season_chosen();
            s.touched();
        }
        "weather" => {
            s.choice.weather = st("weather");
            s.touched();
        }
        "traffic" => {
            s.choice.traffic = a.get("value").and_then(|v| v.as_f64()).unwrap_or(30.0).clamp(0.0, 120.0) as f32;
            s.touched();
        }
        "flag" => {
            let on = a.get("on").and_then(|v| v.as_bool()).unwrap_or(false);
            match st("name").as_str() {
                "passengers" => s.choice.passengers = on,
                "schedule" => s.choice.schedule = on,
                "autostart" => s.choice.autostart = on,
                "comfortable_cabin_on_start" => s.choice.comfortable_cabin_on_start = on,
                "on_foot" => s.choice.on_foot = on,
                other => anyhow::bail!("no such switch: {other}"),
            }
            s.touched();
        }
        "lan" => {
            s.choice.lan_mode = st("mode");
            if a.get("addr").is_some() {
                s.choice.lan_addr = st("addr");
            }
            if s.choice.lan_mode != "join" {
                s.joined_server = None;
            }
            s.touched();
        }
        "launch" => s.launch(),
        "launch_last" => {
            s.save_pick = a.get("pick").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            s.launch_last_situation();
        }
        "launch_tutorial" => s.launch_tutorial(a.get("n").and_then(|v| v.as_u64()).unwrap_or(1) as usize),
        "setting" => {
            let key = st("key");
            let value = a.get("value").cloned().unwrap_or(Value::Null);
            s.settings[key.as_str()] = value;
            s.settings_dirty = 0.3;
        }
        "settings" => {
            if let Some(o) = a.get("values").and_then(|v| v.as_object()) {
                if a.get("replace").and_then(|v| v.as_bool()) == Some(true) {
                    s.settings = json!({});
                }
                for (k, v) in o {
                    s.settings[k.as_str()] = v.clone();
                }
                s.settings_dirty = 0.3;
            }
        }
        "save_settings" => {
            core::save_settings(&s.settings)?;
            s.settings_dirty = 0.0;
        }
        "graphics_profile_save" => {
            let name = core::save_graphics_profile(&st("name"), &s.settings)?;
            return Ok(json!(name));
        }
        "graphics_profile_load" => {
            let p = core::graphics_profiles().get(&st("name")).cloned().ok_or_else(|| anyhow::anyhow!("no such profile"))?;
            core::apply_graphics_profile(&p, &mut s.settings);
            s.settings_dirty = 0.3;
        }
        "graphics_profile_delete" => core::delete_graphics_profile(&st("name"))?,
        "profile" => {
            s.config.profile = st("name");
            core::save_config(&s.config)?;
            s.profile = None;
            s.load_profile();
        }
        "profile_create" => {
            let sex = if st("sex").is_empty() { "M".to_string() } else { st("sex") };
            let p = core::create_profile(st("name").trim(), &sex)?;
            s.config.profile = p.name.clone();
            core::save_config(&s.config)?;
            s.load_profiles();
        }
        "profile_delete" => {
            core::delete_profile(&st("name"))?;
            s.config.profile.clear();
            core::save_config(&s.config)?;
            s.profile = None;
            s.load_profiles();
        }
        "mods" => s.load_mods(),
        "mod_toggle" => s.mod_toggle(st("id"), a.get("on").and_then(|v| v.as_bool()).unwrap_or(true)),
        "mod_remove" => s.mod_remove(st("id")),
        "mod_install" => {
            s.mod_mode = match st("mode").as_str() {
                "extract" => 1,
                "inplace" => 2,
                _ => 0,
            };
            s.install(st("path"));
        }
        "root" => {
            // the Setup page's Save: a folder that is no complete installation is said so,
            // with what it lacks, and nothing is saved (as the launcher's own page does)
            let chosen = st("path").trim().to_string();
            omsi_cfg::content_changed();
            let missing = if chosen.is_empty() { Vec::new() } else { omsi_cfg::missing_original_essentials(std::path::Path::new(&chosen)) };
            if !missing.is_empty() {
                let shown: Vec<&str> = missing.iter().map(String::as_str).take(6).collect();
                anyhow::bail!("{chosen} is not a complete OMSI 2 installation - it lacks {}", shown.join(", "));
            }
            use_root(s, chosen)?;
        }
        "reload" => {
            omsi_cfg::content_changed();
            s.load_content();
        }
        "lines" => s.load_lines(),
        "delete_situation" => {
            let deleted = core::delete_situation(&s.choice.map, std::path::Path::new(&st("file")))?;
            s.last_sit = None;
            return Ok(json!(deleted));
        }
        "server_add" => {
            let address = st("address").trim().to_string();
            if address.is_empty() {
                anyhow::bail!("no address");
            }
            if !s.servers.iter().any(|e| e.address == address) {
                s.servers.push(ServerEntry { name: st("name"), address: address.clone(), protocol: JoinProto::Auto });
                s.save_servers();
            }
            s.ask_server(&address, 0.0);
        }
        "server_remove" => {
            let address = st("address");
            s.servers.retain(|e| e.address != address);
            s.save_servers();
        }
        "server_protocol" => {
            let address = st("address");
            let protocol = serde_json::from_value(json!(st("proto"))).unwrap_or_default();
            if let Some(e) = s.servers.iter_mut().find(|e| e.address == address) {
                e.protocol = protocol;
                s.save_servers();
            }
        }
        "server_ask" => s.ask_server(&st("address"), a.get("every").and_then(|v| v.as_f64()).unwrap_or(10.0) as f32),
        "server_join" => {
            let address = st("address");
            let proto = match a.get("proto").and_then(Value::as_str) {
                Some("udp") => JoinProto::Udp,
                Some("ws" | "websocket") => JoinProto::WebSocket,
                Some(_) => JoinProto::Auto,
                None => s.servers.iter().find(|e| e.address == address)
                    .map(|e| e.protocol).unwrap_or_default(),
            };
            s.join_server(&address, proto);
        }
        "server_leave" => s.leave_server(),
        "stop" => s.stop(a.get("pid").and_then(|v| v.as_u64()).unwrap_or(0) as u32),
        "log" => {
            let pid = a.get("pid").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            s.open_logs.insert(pid);
            s.log_tail(pid);
            return Ok(json!(s.logs.get(&pid).cloned().unwrap_or_default()));
        }
        "dismiss_crash" => s.crash = None,
        "dismiss_disconnected" => s.disconnected = None,
        "diagnostics" => {
            let value = crate::support_bundle::snapshot(None, &crate::settings::Settings::load(), None, None, "launcher");
            let out = core::data_dir().join("support").join(format!("openOMSI-support-{}.json", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)));
            if let Some(d) = out.parent() {
                std::fs::create_dir_all(d)?;
            }
            crate::support_bundle::export(&out, &value)?;
            return Ok(json!(out.to_string_lossy()));
        }
        other => anyhow::bail!("unknown action {other}"),
    }
    Ok(Value::Null)
}

// --- the bus preview -------------------------------------------------------------------
//
// One thread owns a graphics device of its own with the game's renderer and the launcher's
// showroom on it; the Swift side asks for a picture of a size (from a queue of its own) and
// gets the pixels when one was drawn since it last asked.

enum PreviewMsg {
    Want(crate::launcher::showroom::Look),
    Orbit(f32, f32),
    Zoom(f32),
    View(crate::launcher::showroom::PreviewView),
    Frame { w: u32, h: u32, reply: std::sync::mpsc::Sender<Option<(u32, u32, Vec<u8>)>> },
    Release,
}

static PREVIEW: Mutex<Option<std::sync::mpsc::Sender<PreviewMsg>>> = Mutex::new(None);

/// What the preview is doing, for the Swift side to show: a bus being read, or why there is
/// no picture (no graphics device, a bus that cannot be read).
static PREVIEW_STATE: Mutex<(bool, Option<String>)> = Mutex::new((false, None));

static PREVIEW_VIEWS: Mutex<(&'static str, Vec<&'static str>)> = Mutex::new(("outside", Vec::new()));

fn preview_state(busy: bool, error: Option<String>) {
    *PREVIEW_STATE.lock().unwrap_or_else(|e| e.into_inner()) = (busy, error);
}

fn preview_send(m: PreviewMsg) -> bool {
    let mut g = PREVIEW.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_none() {
        if matches!(m, PreviewMsg::Release) {
            return true;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        if std::thread::Builder::new().name("launcher preview".into()).spawn(move || preview_thread(rx)).is_err() {
            return false;
        }
        *g = Some(tx);
    }
    let ok = g.as_ref().is_some_and(|tx| tx.send(m).is_ok());
    if !ok {
        *g = None;
    }
    ok
}

/// The preview's device goes (a game is starting in this process: the memory is its).
fn preview_release() {
    preview_send(PreviewMsg::Release);
    *PREVIEW.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

fn preview_thread(rx: std::sync::mpsc::Receiver<PreviewMsg>) {
    let mut renderer: Option<omsi_render::Renderer> = None;
    let mut quality = crate::launcher::showroom::Quality::now();
    let mut failed = false;
    let mut showroom = crate::launcher::showroom::Showroom::new();
    let mut last = Instant::now();
    let mut delivered = u64::MAX;
    let mut asked = Instant::now();
    while let Ok(m) = rx.recv() {
        match m {
            PreviewMsg::Want(look) => {
                if showroom.follow(&mut quality, false) {
                    renderer = None;
                    failed = false;
                    delivered = u64::MAX;
                }
                showroom.want(look);
            }
            PreviewMsg::Orbit(dx, dy) => showroom.orbit(dx, dy),
            PreviewMsg::Zoom(k) => showroom.zoom_by(k),
            PreviewMsg::View(v) => showroom.set_view(v),
            PreviewMsg::Release => return,
            PreviewMsg::Frame { w, h, reply } => {
                // (asked again after a pause: a preview that came back on the screen gets the
                // picture again, though nothing in it changed)
                if asked.elapsed().as_secs_f32() > 0.5 {
                    delivered = u64::MAX;
                }
                asked = Instant::now();
                if renderer.is_none() && !failed {
                    let instance = crate::startup::graphics_instance();
                    match pollster::block_on(omsi_render::Renderer::new_with(&instance, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb), quality.options)) {
                        Ok(r) => renderer = Some(r),
                        Err(e) => {
                            log::warn!("launcher preview: no renderer: {e:#}");
                            preview_state(false, Some(format!("{}: {e:#}", omsi_ui::tr("No graphics device for the preview"))));
                            failed = true;
                        }
                    }
                }
                let Some(r) = renderer.as_mut() else {
                    let _ = reply.send(None);
                    continue;
                };
                let dt = last.elapsed().as_secs_f32().min(0.1);
                last = Instant::now();
                showroom.update(r, dt);
                preview_state(showroom.busy, showroom.error.clone());
                *PREVIEW_VIEWS.lock().unwrap_or_else(|e| e.into_inner()) = (showroom.view.name(), showroom.views());
                let picture = showroom.preview(r, w, h).filter(|_| showroom.has_picture());
                if picture.is_none() || showroom.drawn == delivered {
                    let _ = reply.send(None);
                    continue;
                }
                delivered = showroom.drawn;
                let _ = reply.send(showroom.pixels(r));
            }
        }
    }
}

/// The bus, its paint and the light of the duty chosen, for the preview.
fn want_preview(s: &State) {
    let c = &s.choice;
    if c.bus.is_empty() || c.map.is_empty() {
        return;
    }
    preview_send(PreviewMsg::Want(crate::launcher::showroom::Look {
        root: std::path::PathBuf::from(&s.config.root),
        map: c.map.clone(),
        bus: c.bus.clone(),
        paint: c.paint.clone(),
        weather: c.weather.clone(),
        time: c.time,
        date: c.date.clone(),
    }));
}

/// A picture of the bus at `w` x `h` into `out` (RGBA, `w * h * 4` bytes at most `cap`):
/// 1 when a new one was written (its size in `out_w`, `out_h`), 0 when the last one stands.
///
/// # Safety
/// `out` points at `cap` writable bytes; `out_w` and `out_h` at writable u32s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn openomsi_preview_frame(w: u32, h: u32, out: *mut u8, cap: usize, out_w: *mut u32, out_h: *mut u32) -> i32 {
    if let Some(s) = STATE.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        want_preview(s);
    }
    let (tx, rx) = std::sync::mpsc::channel();
    if !preview_send(PreviewMsg::Frame { w: w.clamp(16, 4096), h: h.clamp(16, 4096), reply: tx }) {
        return 0;
    }
    match rx.recv() {
        Ok(Some((pw, ph, px))) if px.len() <= cap && !out.is_null() => {
            unsafe {
                std::ptr::copy_nonoverlapping(px.as_ptr(), out, px.len());
                *out_w = pw;
                *out_h = ph;
            }
            1
        }
        _ => 0,
    }
}

/// The preview turned by a drag (points) or zoomed (a factor).
#[unsafe(no_mangle)]
pub extern "C" fn openomsi_preview_orbit(dx: f32, dy: f32) {
    preview_send(PreviewMsg::Orbit(dx, dy));
}

#[unsafe(no_mangle)]
pub extern "C" fn openomsi_preview_zoom(k: f32) {
    preview_send(PreviewMsg::Zoom(k.clamp(0.5, 2.0)));
}

#[unsafe(no_mangle)]
pub extern "C" fn openomsi_preview_view(view: u32) {
    if let Some(v) = crate::launcher::showroom::PreviewView::ALL.get(view as usize) {
        preview_send(PreviewMsg::View(*v));
    }
}
