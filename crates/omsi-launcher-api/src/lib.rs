//! The launcher page's commands, the same for every window that shows the page: the desktop
//! launcher (Tauri, `launcher/src-tauri`) and the phone app's web view (`omsi-app`'s
//! `android_launcher`). A command is a name and its arguments as JSON; it answers JSON, or the
//! bytes of a picture (`call_bytes`), or an error text the page shows as it is. What only the
//! window around the page can do - its events, stepping aside while a game runs, drawing with
//! the game's renderer - it is asked through `Platform`.

pub mod i18n;
mod timetable;
mod updates;
mod versions;

pub use versions::installing;

use omsi_launcher_lib as core;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub type Answer = Result<Value, String>;

/// What the window around the page does for the commands.
pub trait Platform: Send + Sync {
    /// Tell the page something (`vehicles-progress` ...).
    fn emit(&self, _event: &str, _payload: Value) {}
    /// A game was started from the page.
    fn launched(&self) {}
    /// The game's renderer, for the bus preview and the city map: the launcher service's
    /// request and its answer (see `omsi-app`'s `launcher::service`).
    fn renderer(&self, req: Value, wait: Duration) -> Result<(Value, Vec<u8>), String>;
    /// The self-check's mode (`OPENOMSI_LAUNCHER_SELFTEST`), if one runs.
    fn selftest(&self) -> Option<String> {
        None
    }
    fn before_update(&self) {}
    fn quit(&self) {}
}

fn err(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

fn to_json<T: serde::Serialize>(v: T) -> Answer {
    serde_json::to_value(v).map_err(err)
}

// --- start ---------------------------------------------------------------------------

/// Everything the page needs before its first picture: versions, where things are, the
/// settings, the remembered duty and favourites, the languages.
fn bootstrap(p: &dyn Platform) -> Answer {
    if versions::available() {
        versions::settle();
    }
    let config = core::load_config();
    let settings = core::get_settings().unwrap_or_else(|_| core::settings_from_text(None));
    let lang = settings.get("language").and_then(|v| v.as_str()).unwrap_or("ENG").to_string();
    let root = core::root_dir().map(|p| p.to_string_lossy().to_string());
    let choice: Value = std::fs::read_to_string(core::data_dir().join("launcher-duty.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(json!({}));
    let favourites: Vec<String> = std::fs::read_to_string(core::data_dir().join("favourite-buses.txt")).map(|t| t.lines().map(str::trim).filter(|l| !l.is_empty()).map(|l| l.replace('\\', "/").to_lowercase()).collect()).unwrap_or_default();
    let languages: Vec<Value> = core::LANGUAGES.iter().map(|(code, name, iso, _)| json!({ "code": code, "name": name, "iso": iso })).collect();
    Ok(json!({
        "version": omsi_platform::VERSION,
        "platform": std::env::consts::OS,
        "config": config,
        "root": root.as_ref().ok(),
        "root_error": root.as_ref().err().map(err),
        "game": core::game_binary().map(|p| p.to_string_lossy().to_string()),
        "content_dir": core::content_dir().map(|p| p.to_string_lossy().to_string()),
        "data_dir": core::data_dir().to_string_lossy(),
        "settings": settings,
        "language": core::language_iso(&lang),
        "languages": languages,
        "choice": choice,
        "favourites": favourites,
        "selftest": p.selftest(),
        "versions": versions::available(),
        "updated": if versions::swapped() { None } else { updates::just_updated() },
        "now": core::local_now().map(|(y, mo, d, h, mi)| json!({ "date": format!("{y:04}-{mo:02}-{d:02}"), "minutes": h * 60 + mi })),
    }))
}

/// The interface texts in `lang` (the game's tables; English is the key).
fn translations(lang: String) -> Answer {
    to_json(crate::i18n::table(&lang))
}

/// The OMSI 2 folder and the game program (Setup).
fn config_save(root: String, game: String) -> Answer {
    let chosen = root.trim().to_string();
    if !chosen.is_empty() {
        let missing = omsi_cfg::missing_original_essentials(std::path::Path::new(&chosen));
        if !missing.is_empty() {
            return Ok(json!({ "saved": false, "chosen": chosen, "missing": missing }));
        }
    }
    let mut c = core::load_config();
    c.root = chosen.clone();
    c.game = game.trim().to_string();
    core::save_config(&c).map_err(err)?;
    omsi_cfg::content_changed();
    let config = core::load_config();
    let other = (!chosen.is_empty() && std::path::Path::new(&chosen) != std::path::Path::new(&config.root)).then(|| config.root.clone());
    let r = core::root_dir().map(|p| p.to_string_lossy().to_string());
    Ok(json!({ "saved": true, "config": config, "root": r.as_ref().ok(), "root_error": r.err().map(err), "game": core::game_binary(), "used_other": other }))
}

// --- content ---------------------------------------------------------------------------

fn maps() -> Answer {
    to_json(core::list_maps().map_err(err)?)
}

/// The buses; the page hears how far the reading is (`vehicles-progress`).
fn vehicles(p: &dyn Platform) -> Answer {
    let last = std::sync::Mutex::new(std::time::Instant::now());
    let list = core::list_vehicles_progress(|_, done, total| {
        let mut t = last.lock().unwrap_or_else(|e| e.into_inner());
        if t.elapsed().as_millis() > 120 || done == total {
            *t = std::time::Instant::now();
            p.emit("vehicles-progress", json!({ "done": done, "total": total }));
        }
    })
    .map_err(err)?;
    to_json(list)
}

fn weather() -> Answer {
    to_json(core::list_weather().map_err(err)?)
}

/// The airport whose METAR report is the map's current weather.
fn nearest_airport(map: String) -> Answer {
    let root = core::root_dir().map_err(err)?;
    Ok(json!(core::nearest_airport(&root.to_string_lossy(), &map)))
}

/// The airports whose METAR the current weather can take (OMSI's `Weather/ICAO.txt`).
fn metar_airports() -> Answer {
    let root = core::root_dir().map_err(err)?;
    Ok(json!(core::metar_airports(&root)))
}

fn custom_weather(weather: String) -> Answer {
    let c = core::custom_weather(&weather).map_err(err)?;
    Ok(json!({
        "visibility_m": c.visibility_m, "brightness": c.brightness, "wind_dir": c.wind_dir, "wind_speed": c.wind_speed, "temp_c": c.temp_c, "humidity": c.humidity, "pressure": c.pressure,
        "cloud_cover": c.cloud_cover, "cloud_base_m": c.cloud_base_m, "precip": c.precip, "precip_intensity": c.precip_intensity, "road_wetness": c.road_wetness, "snow_cover": c.snow_cover, "snow_on_road": c.snow_on_road,
    }))
}

fn custom_weather_text(w: Value) -> Answer {
    let d = core::CustomWeather::default();
    let n = |k: &str, or: f32| w.get(k).and_then(Value::as_f64).map_or(or, |x| x as f32);
    let on = |k: &str| w.get(k).and_then(Value::as_bool).unwrap_or(false);
    let c = core::CustomWeather {
        visibility_m: n("visibility_m", d.visibility_m),
        brightness: n("brightness", d.brightness),
        wind_dir: n("wind_dir", d.wind_dir),
        wind_speed: n("wind_speed", d.wind_speed),
        temp_c: n("temp_c", d.temp_c),
        humidity: n("humidity", d.humidity),
        pressure: n("pressure", d.pressure),
        cloud_cover: n("cloud_cover", d.cloud_cover),
        cloud_base_m: n("cloud_base_m", d.cloud_base_m),
        precip: n("precip", d.precip as f32).round() as i32,
        precip_intensity: n("precip_intensity", d.precip_intensity),
        road_wetness: n("road_wetness", d.road_wetness),
        snow_cover: on("snow_cover"),
        snow_on_road: on("snow_on_road"),
    };
    Ok(json!(c.encode()))
}

fn lines(map: String, date: String) -> Answer {
    to_json(core::list_lines(&map, &date).map_err(err)?)
}

fn ibis(bus: String, hof: String, line: String) -> Answer {
    to_json(core::ibis_info(&bus, &hof, &line).map_err(err)?)
}

/// The depot file the duty would use (see `core::default_hof`).
fn default_hof(map: Value, bus: Value, date: String, line: Option<String>) -> Answer {
    let map: Option<core::MapInfo> = serde_json::from_value(map).ok();
    let bus: Option<core::VehicleInfo> = serde_json::from_value(bus).ok();
    Ok(json!(core::default_hof(map.as_ref(), bus.as_ref(), &date, line.as_deref())))
}

fn saved_situations(map: String) -> Answer {
    to_json(core::saved_situations(&map))
}

/// Delete one of the situations `saved_situations` lists for `map` (#2556).
fn delete_situation(map: String, file: String) -> Answer {
    Ok(json!(core::delete_situation(&map, Path::new(&file)).map_err(err)?))
}

/// A season and its phase chosen: the date the game will have (none: by date), and the word
/// the game's `--season` takes.
fn season_date(season: String, phase: String, date: String, map: String, keep: bool) -> Answer {
    use omsi_weather::season_phase::{map_latitude, phase_date, SeasonChoice};
    let Some(choice) = SeasonChoice::parse(&format!("{season}-{phase}")) else { return Ok(json!({ "date": null, "word": null })) };
    let num = |r: std::ops::Range<usize>| date.get(r).and_then(|x| x.parse::<u32>().ok());
    let root = core::root_dir().unwrap_or_default();
    let lat = map_latitude(&root, &map);
    let year = num(0..4).unwrap_or(1989) as i32;
    let (y, m, d) = match (num(5..7), num(8..10)) {
        (Some(m), Some(d)) if keep => phase_date(choice, (year, m, d), lat),
        _ => {
            let (m, d) = choice.day(lat);
            (year, m, d)
        }
    };
    Ok(json!({ "date": format!("{y:04}-{m:02}-{d:02}"), "word": choice.word() }))
}

fn tutorials() -> Answer {
    Ok(json!(core::tutorials().into_iter().map(|(n, title, text)| json!({ "number": n, "title": title, "text": text })).collect::<Vec<_>>()))
}

// --- profiles --------------------------------------------------------------------------

fn profiles() -> Answer {
    to_json(core::list_profiles().map_err(err)?)
}

fn profile(name: String) -> Answer {
    to_json(core::get_profile(&name).map_err(err)?)
}

fn profile_create(name: String, sex: String) -> Answer {
    to_json(core::create_profile(&name, &sex).map_err(err)?)
}

fn profile_delete(name: String) -> Answer {
    core::delete_profile(&name).map_err(err)?;
    Ok(json!(true))
}

/// The driver the next duty is driven by.
fn profile_select(name: String) -> Answer {
    let mut c = core::load_config();
    c.profile = name;
    core::save_config(&c).map_err(err)?;
    to_json(c)
}

// --- settings --------------------------------------------------------------------------

fn settings_get() -> Answer {
    core::get_settings().map_err(err)
}

/// Save the settings (the whole set, as `settings_get` gave it, changed); answers them as
/// read back.
fn settings_save(settings: Value) -> Answer {
    let _one = SETTINGS_WRITE.lock().unwrap_or_else(|e| e.into_inner());
    core::save_settings(&settings).map_err(err)?;
    core::get_settings().map_err(err)
}

static SETTINGS_WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn settings_patch(patch: Value) -> Answer {
    let _one = SETTINGS_WRITE.lock().unwrap_or_else(|e| e.into_inner());
    core::save_settings(&patched(core::get_settings().map_err(err)?, &patch)).map_err(err)?;
    core::get_settings().map_err(err)
}

fn patched(mut settings: Value, patch: &Value) -> Value {
    if let (Some(to), Some(from)) = (settings.as_object_mut(), patch.as_object()) {
        for (k, v) in from {
            to.insert(k.clone(), v.clone());
        }
    }
    settings
}

/// Every setting as it comes (Reset).
fn settings_defaults() -> Answer {
    Ok(core::settings_from_text(None))
}

fn option_presets() -> Answer {
    Ok(json!(core::option_presets().into_iter().map(|(name, v)| json!({ "name": name, "settings": v })).collect::<Vec<_>>()))
}

/// The four quality presets of a graphics mode (vanilla, enhanced ...).
fn graphics_presets(mode: String) -> Answer {
    let mode = core::graphics_mode(&mode);
    Ok(json!(core::graphics_presets_for(mode).into_iter().map(|(name, v)| json!({ "name": name, "settings": v })).collect::<Vec<_>>()))
}

fn optimise_arm(target: u32) -> Answer {
    core::optimise::arm(target).map_err(err)?;
    Ok(core::optimise::status())
}

fn optimise_restore() -> Answer {
    let _one = SETTINGS_WRITE.lock().unwrap_or_else(|e| e.into_inner());
    core::optimise::restore().map_err(err)
}

fn graphics_profiles() -> Answer {
    to_json(core::graphics_profiles())
}

fn graphics_profile_save(name: String, settings: Value) -> Answer {
    Ok(json!(core::save_graphics_profile(&name, &settings).map_err(err)?))
}

fn graphics_profile_delete(name: String) -> Answer {
    core::delete_graphics_profile(&name).map_err(err)?;
    Ok(json!(true))
}

fn keybindings_get() -> Answer {
    core::get_keybindings().map_err(err)
}

fn keybindings_save(bindings: Value) -> Answer {
    core::save_keybindings(&bindings).map_err(err)?;
    core::get_keybindings().map_err(err)
}

fn radio_stations() -> Answer {
    let _one = RADIO_WRITE.lock().unwrap_or_else(|e| e.into_inner());
    stations_json()
}

fn stations_json() -> Answer {
    Ok(json!(omsi_cfg::radio::own_stations().into_iter().map(|(name, address)| json!({ "name": name, "address": address })).collect::<Vec<_>>()))
}

fn radio_stations_save(list: Vec<Value>) -> Answer {
    let text = |s: &Value, k: &str| s.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let list: Vec<(String, String)> = list.iter().map(|s| (text(s, "name"), text(s, "address"))).collect();
    let _one = RADIO_WRITE.lock().unwrap_or_else(|e| e.into_inner());
    omsi_cfg::radio::save_stations(&list).map_err(err)?;
    stations_json()
}

static RADIO_WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());

// --- mods ------------------------------------------------------------------------------

fn mods_status() -> Answer {
    to_json(core::mods_status().map_err(err)?)
}

fn add_hofs(files: Vec<String>) -> Answer {
    let files: Vec<PathBuf> = files.into_iter().map(PathBuf::from).collect();
    Ok(json!(core::add_shared_hofs(&files).map_err(err)?))
}

fn mod_set_enabled(id: String, on: bool) -> Answer {
    Ok(json!(core::mod_set_enabled(&id, on).map_err(err)?))
}

fn mod_remove(id: String) -> Answer {
    Ok(json!(core::mod_remove(&id).map_err(err)?))
}

/// What installing `path` would take (size, room, whether it can be used in place).
fn mod_inspect(path: String) -> Answer {
    to_json(core::inspect_mod(Path::new(&path)).map_err(err)?)
}

/// Start installing `path` (`mode`: auto, extract, inplace); `poll` follows it.
fn mod_install(path: String, mode: String) -> Answer {
    to_json(core::start_install(Path::new(&path), if mode.is_empty() { "auto" } else { &mode }).map_err(err)?)
}

// --- games -----------------------------------------------------------------------------

/// What the page asks every few seconds: the content's stamp (a new one: the lists are read
/// again), the install jobs and the running games.
fn poll() -> Answer {
    to_json(core::poll().map_err(err)?)
}

fn stop_game(pid: u32) -> Answer {
    Ok(json!({ "ended_by_itself": core::stop_instance(pid).map_err(err)? }))
}

pub fn game_ended(game: &core::instances::Instance) -> Option<(&'static str, Value)> {
    if game.running || game.stopping.is_some() || game.killed {
        return None;
    }
    let log = std::path::Path::new(&game.log);
    if let Some(why) = core::crash::disconnect_of(log) {
        return Some(("game-disconnected", json!({ "why": why })));
    }
    let (what, tail) = core::crash::crash_of(log)?;
    let silent = what.contains("closed without a word");
    Some(("game-crashed", json!({
        "what": what,
        "tail": tail,
        "gap": core::crash::CRASH_TAIL_GAP,
        "lost": what.contains("graphics device was lost"),
        "silent": silent,
        "vulkan": silent && core::crash::died_compiling_on_vulkan(&tail),
        "version": omsi_platform::VERSION,
        "os": std::env::consts::OS,
    })))
}

fn game_log(pid: u32, lines: Option<usize>) -> Answer {
    to_json(core::log_tail(pid, lines.unwrap_or(80)).map_err(err)?)
}

/// Start a game for the duty (the window may step aside while it runs: `Platform::launched`).
fn launch(p: &dyn Platform, duty: Value) -> Answer {
    let d: core::Duty = serde_json::from_value(duty).map_err(err)?;
    let launched = core::launch(&d).map_err(err)?;
    core::log_to_file(&format!("game {}: {}", launched.pid, launched.command));
    p.launched();
    to_json(launched)
}

/// The command line a duty becomes (shown under the Start button on request).
fn duty_args(duty: Value) -> Answer {
    let d: core::Duty = serde_json::from_value(duty).map_err(err)?;
    Ok(json!(core::duty_args(&d).map_err(err)?))
}

// --- multiplayer -----------------------------------------------------------------------

fn check_join(text: String) -> Answer {
    Ok(core::check_join(&text))
}

fn servers_path() -> PathBuf {
    core::data_dir().join("servers.json")
}

/// The saved servers, the official one first.
fn servers_get() -> Answer {
    let mut list: Vec<Value> = std::fs::read_to_string(servers_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    if !list.iter().any(|s| s.get("address").and_then(|a| a.as_str()).is_some_and(omsi_net::official::is_alias)) {
        list.insert(0, json!({ "name": omsi_net::official::NAME, "address": omsi_net::official::ALIAS }));
    }
    Ok(json!(list))
}

fn servers_save(list: Value) -> Answer {
    std::fs::write(servers_path(), serde_json::to_vec_pretty(&list).map_err(err)?).map_err(err)?;
    Ok(json!(true))
}

/// A server's or a session code's status: name, map, players, its buses.
fn server_query(address: String) -> Answer {
    let info = if omsi_net::looks_like_code(&address) { host_status(&address) } else { omsi_net::ws::query(&address, true) };
    let i = info?;
    let icon = (!i.icon.is_empty()).then(|| format!("data:image/png;base64,{}", base64(&i.icon)));
    Ok(json!({
        "name": i.name, "motd": i.motd, "map": i.map, "players": i.players, "max_players": i.max_players,
        "version": i.version, "icon": icon, "time": i.time, "weather": i.weather, "password": i.password,
        "vehicles": i.vehicles, "reached_at": i.reached_at,
    }))
}

/// A code host's first responsive status endpoint, including its tunnel.
fn host_status(code: &str) -> Result<omsi_net::ws::ServerInfo, String> {
    omsi_net::ws::query_session(code)
}

fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(A[(n >> 18) as usize & 63] as char);
        out.push(A[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { A[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { A[n as usize & 63] as char } else { '=' });
    }
    out
}

// --- what the page keeps ---------------------------------------------------------------

/// The duty as chosen, kept for the next start (`launcher-duty.json`, which the Swift
/// launcher's model reads as well).
fn choice_save(choice: Value) -> Answer {
    std::fs::write(core::data_dir().join("launcher-duty.json"), serde_json::to_vec_pretty(&choice).map_err(err)?).map_err(err)?;
    Ok(json!(true))
}

/// The starred buses (one bus file a line).
fn favourites_save(list: Vec<String>) -> Answer {
    let text: String = list.iter().map(|l| format!("{}\n", l.replace('\\', "/").to_lowercase())).collect();
    std::fs::write(core::data_dir().join("favourite-buses.txt"), text).map_err(err)?;
    Ok(json!(true))
}

/// The player's own notes on maps and buses (#1791), kept in the data folder: one object,
/// a note by its key (`map:maps/x/global.cfg`, `bus:vehicles/x/y.bus`).
fn notes_path() -> PathBuf {
    core::data_dir().join("notes.json")
}

fn notes_get() -> Answer {
    Ok(std::fs::read_to_string(notes_path()).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).filter(Value::is_object).unwrap_or_else(|| json!({})))
}

fn notes_save(key: String, text: String) -> Answer {
    let mut all = notes_get()?;
    let key = key.replace('\\', "/").to_lowercase();
    if let Some(o) = all.as_object_mut() {
        if text.trim().is_empty() {
            o.remove(&key);
        } else {
            o.insert(key, json!(text));
        }
    }
    std::fs::write(notes_path(), serde_json::to_vec_pretty(&all).map_err(err)?).map_err(err)?;
    Ok(json!(true))
}

// --- the player's depot (`depot.json`): buses of their own that keep their wear ---------

/// The depots and their buses, each bus with what its last session left: odometer, fuel,
/// dirt, broken parts.
fn depot_get() -> Answer {
    let d = core::depot::load().map_err(err)?;
    let buses: Vec<Value> = d
        .buses
        .iter()
        .map(|b| {
            let mut v = serde_json::to_value(b).unwrap_or_default();
            if let Some(o) = v.as_object_mut() {
                o.remove("wear");
                o.insert("odometer".into(), json!(b.odometer()));
                o.insert("fuel".into(), json!(b.fuel()));
                o.insert("dirt".into(), json!(b.dirt()));
                o.insert("faults".into(), json!(b.faults()));
            }
            v
        })
        .collect();
    Ok(json!({ "buses": buses, "groups": d.group_names() }))
}

/// A new bus in the depot, with a plate none of the others has; its id.
fn depot_add(bus: String, paint: Option<String>, number: Option<String>, name: String, group: Option<String>) -> Answer {
    let id = core::depot::update(|d| {
        let plate = d.new_plate(plate_seed());
        Ok(d.add(core::depot::DepotBus { name, bus, paint: paint.unwrap_or_default(), plate, number: number.unwrap_or_default(), group: group.unwrap_or_default(), ..Default::default() }))
    })
    .map_err(err)?;
    Ok(json!(id))
}

/// A depot bus's name, paint, plate or fleet number changed (its wear stays).
fn depot_update(id: String, patch: Value) -> Answer {
    core::depot::update(|d| {
        let b = d.get_mut(&id).ok_or_else(|| anyhow::anyhow!("no depot bus {id}"))?;
        let text = |k: &str| patch.get(k).and_then(Value::as_str).map(str::to_string);
        if let Some(v) = text("name") {
            b.name = v;
        }
        if let Some(v) = text("paint") {
            b.paint = v;
        }
        if let Some(v) = text("plate") {
            b.plate = v.trim().to_uppercase();
        }
        if let Some(v) = text("number") {
            b.number = v;
        }
        if let Some(v) = text("group") {
            b.group = v;
        }
        Ok(())
    })
    .map_err(err)?;
    Ok(json!(true))
}

/// Refuel, wash or repair a depot bus (all of it, or one part): its next drive starts so.
fn depot_service(id: String, kind: String) -> Answer {
    core::depot::update(|d| {
        let b = d.get_mut(&id).ok_or_else(|| anyhow::anyhow!("no depot bus {id}"))?;
        if !b.service(&kind) {
            anyhow::bail!("no depot service {kind}");
        }
        Ok(())
    })
    .map_err(err)?;
    Ok(json!(true))
}

fn depot_remove(id: String) -> Answer {
    core::depot::update(|d| {
        d.remove(&id);
        Ok(())
    })
    .map_err(err)?;
    Ok(json!(true))
}

/// The player's depots: a new one, one renamed (with its buses) or one removed (its buses
/// stay, in no depot).
fn depot_group(op: &str, name: String, to: Option<String>) -> Answer {
    let names = core::depot::update(|d| {
        let done = match op {
            "add" => d.add_group(&name),
            "rename" => d.rename_group(&name, to.as_deref().unwrap_or_default()),
            _ => {
                d.remove_group(&name);
                true
            }
        };
        if !done {
            anyhow::bail!("there is already a depot called {}", to.as_deref().unwrap_or(&name).trim());
        }
        Ok(d.group_names())
    })
    .map_err(err)?;
    Ok(json!(names))
}

/// Another plate none of the depot's buses has.
fn depot_plate() -> Answer {
    let d = core::depot::load().map_err(err)?;
    Ok(json!(d.new_plate(plate_seed())))
}

fn plate_seed() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|t| t.as_nanos() as u64).unwrap_or(0)
}

// --- the game's renderer: the bus preview and the city map (see `service`) -----------

/// What the bus preview shows: the bus, its paint, and the map, weather, time and date that
/// light it.
fn preview_look(p: &dyn Platform, bus: String, paint: String, map: String, weather: String, time: i32, date: String) -> Answer {
    p.renderer(json!({ "cmd": "look", "bus": bus, "paint": paint, "map": map, "weather": weather, "time": time, "date": date }), Duration::from_secs(10))?;
    Ok(json!(true))
}

/// Start the preview's helper and its renderer ahead of the first picture.
fn preview_warm(p: &dyn Platform) -> Answer {
    p.renderer(json!({ "cmd": "warm" }), Duration::from_secs(60))?;
    Ok(json!(true))
}

/// The preview's next picture at `w` x `h`, after turning it by a drag (`dx`, `dy` pixels) and
/// bringing it nearer by `zoom`: four bytes (little-endian) giving the length of a JSON state
/// (`busy`, `moving`, `failed`, `error`), the state, then a JPEG when the picture changed
/// since the last one asked for.
#[allow(clippy::too_many_arguments)]
fn preview_frame(p: &dyn Platform, w: u32, h: u32, force: bool, dx: Option<f32>, dy: Option<f32>, zoom: Option<f32>, quality: Option<u8>, view: Option<String>) -> Result<Vec<u8>, String> {
    let req = json!({ "cmd": "frame", "w": w.clamp(64, 4096), "h": h.clamp(64, 4096), "force": force, "dx": dx.unwrap_or(0.0), "dy": dy.unwrap_or(0.0), "zoom": zoom.unwrap_or(1.0), "quality": quality.unwrap_or(90), "view": view });
    let (head, jpeg) = p.renderer(req, Duration::from_secs(30))?;
    let head = head.to_string().into_bytes();
    let mut out = Vec::with_capacity(4 + head.len() + jpeg.len());
    out.extend_from_slice(&(head.len() as u32).to_le_bytes());
    out.extend_from_slice(&head);
    out.extend_from_slice(&jpeg);
    Ok(out)
}

/// The map with the trip's route over it (see `mapview::export` in the launcher crate).
fn route_map(p: &dyn Platform, map: String, date: String, trip: String, surface: Option<bool>) -> Answer {
    serde_json::from_slice(&route_map_bytes(p, map, date, trip, surface)?).map_err(err)
}

fn route_map_bytes(p: &dyn Platform, map: String, date: String, trip: String, surface: Option<bool>) -> Result<Vec<u8>, String> {
    let (_, body) = p.renderer(json!({ "cmd": "map", "map": map, "date": date, "trip": trip, "surface": surface.unwrap_or(true) }), Duration::from_secs(120))?;
    Ok(body)
}

fn keyboard_actions(p: &dyn Platform, mut req: Value) -> Answer {
    req["cmd"] = json!("key_actions");
    let (_, body) = p.renderer(req, Duration::from_secs(30))?;
    serde_json::from_slice(&body).map_err(err)
}

fn controllers(p: &dyn Platform, op: &str, mut req: Value, wait: u64) -> Answer {
    req["cmd"] = json!("pads");
    req["op"] = json!(op);
    let (mut head, _) = p.renderer(req, Duration::from_secs(wait))?;
    if let Some(o) = head.as_object_mut() {
        for k in ["id", "len", "ok"] {
            o.remove(k);
        }
    }
    Ok(head)
}

/// The support package (logs, settings, the machine), written by the game program into
/// the data folder; answers where it went.
fn support_package() -> Answer {
    let game = core::game_binary().ok_or("the game program was not found")?;
    let out = core::data_dir().join(format!("support-{}.zip", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)));
    let status = std::process::Command::new(game).arg("--export-diagnostics").arg(&out).status().map_err(err)?;
    if !status.success() || !out.exists() {
        return Err("the game could not write the support package".into());
    }
    Ok(json!(out.to_string_lossy()))
}


/// A picture of the content (`maps/Grundorf/picture.jpg`) from the content folder, an archive
/// used in place or the OMSI folder: pictures only, and nothing outside those folders.
pub fn content_picture(rel: &str) -> Option<Vec<u8>> {
    let rel = rel.replace('\\', "/");
    let ext = rel.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    let inside = !rel.is_empty() && Path::new(&rel).components().all(|c| matches!(c, std::path::Component::Normal(_)));
    if !inside || !matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "bmp" | "gif" | "webp") {
        return None;
    }
    let p = core::content_path(&rel).ok()?;
    omsi_cfg::vfs::read(&p).ok()
}

/// A value of the arguments (`null` when it is not there: an `Option` takes that as `None`).
fn arg<T: serde::de::DeserializeOwned>(args: &Value, name: &str) -> Result<T, String> {
    serde_json::from_value(args.get(name).cloned().unwrap_or(Value::Null)).map_err(|e| format!("{name}: {e}"))
}

/// Run a command of the page (on the caller's thread: a window runs it on a worker).
/// A folder's sub-folders and (`archives`) its mod archives, for the page's own browser on a
/// phone, which has no file dialog: up to 0.2.34 the phone's launcher browsed the storage
/// itself to install a mod, and since the page took over "Add folder" and "Add ZIP" did
/// nothing there (#2513, #2526, #2567). No path: the shared storage (or the home folder).
fn list_dir(path: Option<String>, archives: bool) -> Answer {
    let start = || {
        ["/storage/emulated/0", "/sdcard"]
            .iter()
            .map(std::path::PathBuf::from)
            .find(|p| p.is_dir())
            .or_else(|| std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(std::path::PathBuf::from))
            .unwrap_or_else(|| std::path::PathBuf::from("/"))
    };
    let dir = path.filter(|p| !p.trim().is_empty()).map(std::path::PathBuf::from).unwrap_or_else(start);
    let rd = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let (mut dirs, mut files) = (Vec::new(), Vec::new());
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let Ok(kind) = e.file_type() else { continue };
        if kind.is_dir() {
            dirs.push(name);
        } else if archives && [".zip", ".7z", ".rar"].iter().any(|x| name.to_ascii_lowercase().ends_with(x)) {
            files.push(name);
        }
    }
    dirs.sort_by_key(|n| n.to_lowercase());
    files.sort_by_key(|n| n.to_lowercase());
    Ok(json!({
        "path": dir.to_string_lossy(),
        "parent": dir.parent().map(|p| p.to_string_lossy().to_string()),
        "dirs": dirs,
        "files": files,
    }))
}

pub fn call(cmd: &str, args: &Value, p: &dyn Platform) -> Answer {
    match cmd {
        "bootstrap" => bootstrap(p),
        "translations" => translations(arg::<String>(args, "lang")?),
        "config_save" => config_save(arg::<String>(args, "root")?, arg::<String>(args, "game")?),
        "maps" => maps(),
        "list_dir" => list_dir(arg::<Option<String>>(args, "path")?, arg::<Option<bool>>(args, "archives")?.unwrap_or(false)),
        "vehicles" => vehicles(p),
        "weather" => weather(),
        "nearest_airport" => nearest_airport(arg::<String>(args, "map")?),
        "tour_buses" => Ok(json!(core::tour_buses(&arg::<String>(args, "map")?, &arg::<String>(args, "group")?, &arg::<String>(args, "date")?)
            .into_iter()
            .map(|(file, paint, number)| json!({ "file": file, "paint": paint, "number": number }))
            .collect::<Vec<_>>())),
        "metar_airports" => metar_airports(),
        "custom_weather" => custom_weather(arg::<String>(args, "weather")?),
        "custom_weather_text" => custom_weather_text(arg::<Value>(args, "custom")?),
        "lines" => lines(arg::<String>(args, "map")?, arg::<String>(args, "date")?),
        "ibis" => ibis(arg::<String>(args, "bus")?, arg::<String>(args, "hof")?, arg::<String>(args, "line")?),
        "default_hof" => default_hof(arg::<Value>(args, "map")?, arg::<Value>(args, "bus")?, arg::<String>(args, "date")?, arg::<Option<String>>(args, "line")?),
        "saved_situations" => saved_situations(arg::<String>(args, "map")?),
        "delete_situation" => delete_situation(arg::<String>(args, "map")?, arg::<String>(args, "file")?),
        "season_date" => season_date(arg::<String>(args, "season")?, arg::<String>(args, "phase")?, arg::<String>(args, "date")?, arg::<String>(args, "map")?, arg::<bool>(args, "keep")?),
        "tutorials" => tutorials(),
        "timetable" => timetable::load(arg::<String>(args, "map")?),
        "timetable_save" => timetable::save(arg::<String>(args, "map")?, arg::<Vec<timetable::LineEdit>>(args, "lines")?),
        "timetable_reset" => timetable::reset(arg::<String>(args, "map")?),
        "profiles" => profiles(),
        "profile" => profile(arg::<String>(args, "name")?),
        "profile_create" => profile_create(arg::<String>(args, "name")?, arg::<String>(args, "sex")?),
        "profile_delete" => profile_delete(arg::<String>(args, "name")?),
        "profile_select" => profile_select(arg::<String>(args, "name")?),
        "update_status" => Ok(updates::status()),
        "update_check" => Ok(updates::check(arg::<Option<bool>>(args, "again")?.unwrap_or(false))),
        "update_install" => Ok(updates::install(p)),
        "update_dismiss" => Ok(updates::dismiss()),
        "settings_get" => settings_get(),
        "settings_save" => settings_save(arg::<Value>(args, "settings")?),
        "settings_patch" => settings_patch(arg::<Value>(args, "patch")?),
        "settings_defaults" => settings_defaults(),
        "option_presets" => option_presets(),
        "graphics_presets" => graphics_presets(arg::<String>(args, "mode")?),
        "graphics_profiles" => graphics_profiles(),
        "graphics_profile_save" => graphics_profile_save(arg::<String>(args, "name")?, arg::<Value>(args, "settings")?),
        "graphics_profile_delete" => graphics_profile_delete(arg::<String>(args, "name")?),
        "optimise_status" => Ok(core::optimise::status()),
        "optimise_arm" => optimise_arm(arg::<u32>(args, "target")?),
        "optimise_cancel" => {
            core::optimise::disarm();
            Ok(core::optimise::status())
        }
        "optimise_restore" => optimise_restore(),
        "keybindings_get" => keybindings_get(),
        "keybindings_save" => keybindings_save(arg::<Value>(args, "bindings")?),
        "radio_stations" => radio_stations(),
        "radio_stations_save" => radio_stations_save(arg::<Vec<Value>>(args, "list")?),
        "mods_status" => mods_status(),
        "add_hofs" => add_hofs(arg::<Vec<String>>(args, "files")?),
        "mod_set_enabled" => mod_set_enabled(arg::<String>(args, "id")?, arg::<bool>(args, "on")?),
        "mod_remove" => mod_remove(arg::<String>(args, "id")?),
        "mod_inspect" => mod_inspect(arg::<String>(args, "path")?),
        "mod_install" => mod_install(arg::<String>(args, "path")?, arg::<String>(args, "mode")?),
        "mod_cancel" => Ok(json!(core::install::cancel(arg::<u64>(args, "id")?))),
        "mods_clear_finished" => {
            core::install::clear_finished();
            Ok(json!(true))
        }
        "poll" => poll(),
        "stop_game" => stop_game(arg::<u32>(args, "pid")?),
        "game_log" => game_log(arg::<u32>(args, "pid")?, arg::<Option<usize>>(args, "lines")?),
        "model_check" => content_check(arg::<String>(args, "bus")?, false),
        "content_check" => content_check(arg::<String>(args, "path")?, arg::<Option<bool>>(args, "deep")?.unwrap_or(false)),
        "launch" => launch(p, arg::<Value>(args, "duty")?),
        "duty_args" => duty_args(arg::<Value>(args, "duty")?),
        "check_join" => check_join(arg::<String>(args, "text")?),
        "servers_get" => servers_get(),
        "servers_save" => servers_save(arg::<Value>(args, "list")?),
        "server_query" => server_query(arg::<String>(args, "address")?),
        "choice_save" => choice_save(arg::<Value>(args, "choice")?),
        "favourites_save" => favourites_save(arg::<Vec<String>>(args, "list")?),
        "depot_get" => depot_get(),
        "depot_add" => depot_add(arg::<String>(args, "bus")?, arg::<Option<String>>(args, "paint")?, arg::<Option<String>>(args, "number")?, arg::<String>(args, "name")?, arg::<Option<String>>(args, "group")?),
        "depot_group_add" => depot_group("add", arg::<String>(args, "name")?, None),
        "depot_group_rename" => depot_group("rename", arg::<String>(args, "name")?, arg::<Option<String>>(args, "to")?),
        "depot_group_remove" => depot_group("remove", arg::<String>(args, "name")?, None),
        "depot_update" => depot_update(arg::<String>(args, "id")?, arg::<Value>(args, "patch")?),
        "depot_remove" => depot_remove(arg::<String>(args, "id")?),
        "depot_service" => depot_service(arg::<String>(args, "id")?, arg::<String>(args, "kind")?),
        "depot_plate" => depot_plate(),
        "notes_get" => notes_get(),
        "notes_save" => notes_save(arg::<String>(args, "key")?, arg::<String>(args, "text")?),
        "preview_look" => preview_look(p, arg::<String>(args, "bus")?, arg::<String>(args, "paint")?, arg::<String>(args, "map")?, arg::<String>(args, "weather")?, arg::<i32>(args, "time")?, arg::<String>(args, "date")?),
        "preview_warm" => preview_warm(p),
        "route_map" => route_map(p, arg::<String>(args, "map")?, arg::<String>(args, "date")?, arg::<String>(args, "trip")?, arg(args, "surface")?),
        "keyboard_actions" => keyboard_actions(p, json!({ "lang": arg::<Option<String>>(args, "lang")?, "actions": arg::<Option<Vec<String>>>(args, "actions")?, "vehicles": arg::<Option<Vec<String>>>(args, "vehicles")?, "catalog": arg::<Option<bool>>(args, "catalog")?, "scan": arg::<Option<bool>>(args, "scan")?, "have": arg::<Option<String>>(args, "have")? })),
        "controllers_load" => controllers(p, "load", json!({}), 20),
        "controllers_state" => controllers(p, "state", json!({ "off": arg::<Option<Vec<String>>>(args, "off")?.unwrap_or_default() }), 5),
        "controllers_save" => controllers(p, "save", json!({ "devices": arg::<Value>(args, "devices")? }), 20),
        "controllers_ffb_test" => controllers(p, "ffb_test", json!({ "name": arg::<String>(args, "name")?, "axis": arg::<usize>(args, "axis")?, "strength": arg::<Option<f32>>(args, "strength")?, "token": arg::<Option<u64>>(args, "token")? }), 20),
        "controllers_release" => controllers(p, "release", json!({}), 5),
        "controllers_ffb_stop" => controllers(p, "ffb_stop", json!({ "token": arg::<Option<u64>>(args, "token")? }), 5),
        "support_package" => support_package(),
        "versions" => versions::list(),
        "version_install" => versions::install(arg::<String>(args, "version")?),
        "version_status" => versions::status(),
        "version_restart" => versions::restart(p),
        "prepare_tunnel" => {
            std::thread::spawn(omsi_net::tunnel::ensure_cloudflared);
            Ok(Value::Null)
        }
        _ => Err(format!("unknown command {cmd}")),
    }
}

/// Run a command that answers bytes (the bus preview's pictures).
pub fn call_bytes(cmd: &str, args: &Value, p: &dyn Platform) -> Result<Vec<u8>, String> {
    match cmd {
        "content_picture" => content_picture(&arg::<String>(args, "path")?).ok_or_else(|| "not found".to_string()),
        "route_map" => route_map_bytes(p, arg::<String>(args, "map")?, arg::<String>(args, "date")?, arg::<String>(args, "trip")?, arg(args, "surface")?),
        "map_ground" => {
            let mut req = args.clone();
            req["cmd"] = json!("map_ground");
            let (_, body) = p.renderer(req, Duration::from_secs(120))?;
            Ok(body)
        }
        "preview_frame" => preview_frame(p, arg(args, "w")?, arg(args, "h")?, arg::<Option<bool>>(args, "force")?.unwrap_or(false), arg(args, "dx")?, arg(args, "dy")?, arg(args, "zoom")?, arg(args, "quality")?, arg(args, "view")?),
        _ => Err(format!("unknown command {cmd}")),
    }
}

#[cfg(test)]
mod setup_tests {
    use super::*;

    #[test]
    fn a_folder_that_is_no_complete_omsi_2_is_not_saved() {
        let home = std::env::temp_dir().join(format!("openomsi-setup-{}", std::process::id()));
        let not_omsi = home.join("Downloads");
        std::fs::create_dir_all(&not_omsi).unwrap();
        let was = (std::env::var_os("HOME"), std::env::var_os("USERPROFILE"));
        std::env::set_var("HOME", &home);
        std::env::set_var("USERPROFILE", &home);
        let r = config_save(not_omsi.to_string_lossy().to_string(), String::new()).unwrap();
        let written = home.join(".openomsi").join("launcher.json").exists();
        for (k, v) in [("HOME", was.0), ("USERPROFILE", was.1)] {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
        assert_eq!(r["saved"], json!(false));
        assert!(!r["missing"].as_array().unwrap().is_empty());
        assert!(!written);
        std::fs::remove_dir_all(&home).ok();
    }
}

#[cfg(test)]
mod settings_tests {
    use super::*;

    #[test]
    fn a_launcher_change_keeps_what_the_game_changed_meanwhile() {
        let game_wrote = "volume=0.25\nwheel_range=540\nmsaa=4\n";
        let on_disk = core::settings_from_text(Some(game_wrote));
        let text = core::settings_to_text(&patched(on_disk, &json!({ "msaa": 8 })), Some(game_wrote));
        let now = core::settings_from_text(Some(&text));
        assert_eq!(now["msaa"], json!(8));
        assert_eq!(now["volume"], json!(0.25));
        assert_eq!(now["wheel_range"], json!(540.0));
    }
}

#[cfg(test)]
mod game_end_tests {
    use super::*;

    #[test]
    fn a_crash_and_a_server_kick_are_told_and_a_stop_or_a_clean_end_is_not() {
        let dir = std::env::temp_dir().join(format!("openomsi-game-end-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let game = |pid: u32, log: &str| {
            let p = dir.join(format!("game-{pid}.log"));
            std::fs::write(&p, log).unwrap();
            core::instances::Instance { pid, log: p.to_string_lossy().to_string(), ..Default::default() }
        };
        let crashed = game(11, "[t INFO x] starting the game: omsi\n[t ERROR omsi_render] ending the session: the graphics device was lost (Unknown: Out of memory)\n[t INFO x] game ends\n");
        let (event, what) = game_ended(&crashed).unwrap();
        assert_eq!(event, "game-crashed");
        assert_eq!(what["lost"], json!(true));
        let kicked = game(12, &format!("[t INFO x] starting the game: omsi\n[t WARN openomsi_game::lan] {}Expelled: dangerous driving\n[t INFO x] game ends\n", omsi_net::LEFT_SERVER));
        assert_eq!(game_ended(&kicked).map(|(e, w)| (e, w["why"].clone())), Some(("game-disconnected", json!("Expelled: dangerous driving"))));
        assert!(game_ended(&game(13, "[t INFO x] starting the game: omsi\n[t INFO x] game ends\n")).is_none());
        let mut stopped = game(14, "[t INFO x] starting the game: omsi\n[t ERROR x] the game stopped on an error: panicked at a.rs:1:1\n");
        assert!(game_ended(&stopped).is_some());
        stopped.stopping = Some(1);
        assert!(game_ended(&stopped).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[cfg(test)]
mod custom_weather_tests {
    use super::*;

    #[test]
    fn a_weather_edited_on_the_page_keeps_every_value() {
        let mut w = custom_weather("custom:vis=800;t=-5;pt=2;pi=200;cb=900;p=990".into()).unwrap();
        w["wind_speed"] = json!(12.5);
        w["precip"] = json!(1);
        w["snow_on_road"] = json!(true);
        let text = custom_weather_text(w).unwrap();
        let back = core::CustomWeather::parse(text.as_str().unwrap()).unwrap();
        let want = core::CustomWeather { visibility_m: 800.0, temp_c: -5.0, precip: 1, precip_intensity: 200.0, cloud_base_m: 900.0, pressure: 990.0, wind_speed: 12.5, snow_on_road: true, ..Default::default() };
        assert_eq!(back, want);
    }
}

// --- the content test (Settings) -----------------------------------------------------

/// The game program loads `path` - a vehicle, a scenery object, a spline, a human, a map, a
/// mod's folder or zip - as it would for a drive (`openomsi --check`) and answers its
/// loaders' warnings and errors and whatever it names that is not there as JSON. `deep`: a
/// map's objects and splines are loaded whole (models, meshes, textures), not only read.
fn content_check(path: String, deep: bool) -> Answer {
    let game = core::game_binary().ok_or_else(|| "the game program was not found: set it under Setup".to_string())?;
    let root = core::root_dir().map_err(err)?;
    let mut cmd = std::process::Command::new(&game);
    // (a mod's folders come one to a line)
    cmd.arg("--check").args(path.lines().map(str::trim).filter(|l| !l.is_empty())).arg("--root").arg(&root).env("OMSI_NO_GUARD", "1");
    if deep {
        cmd.arg("--deep");
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().map_err(|e| format!("{}: {e}", game.display()))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().rev().find(|l| l.trim_start().starts_with('{')).ok_or_else(|| {
        let last = String::from_utf8_lossy(&out.stderr);
        format!("the game gave no answer ({})", last.lines().last().unwrap_or("").trim())
    })?;
    serde_json::from_str(line).map_err(err)
}
