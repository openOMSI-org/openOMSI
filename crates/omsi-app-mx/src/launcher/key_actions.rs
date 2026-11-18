//! The desktop launcher's "Add binding" catalog (the `key_actions` command of
//! `--launcher-service`): every action a key can be given - the installed key-language files'
//! and OMSI's events, the triggers of every installed bus's scripts (mods' too: found in the
//! background, and kept in a cache for the next start) and the ones the keyboard file has -
//! with the bus files that use each.

use omsi_launcher_lib as core;
use serde_json::{json, Value};
use std::collections::HashMap;

/// An action the catalog offers: its name, what it does, where it was found (the language
/// files, OMSI's events, a bus's script, the keyboard file) and the bus files that use it.
#[derive(Clone)]
struct KeyActionOption {
    action: String,
    label: String,
    sources: Vec<String>,
    bus_paths: Vec<String>,
}

/// Every action the vehicles' section can be given: the language files' texts, OMSI's events
/// (in their own spelling), the buses' script triggers and what the keyboard file has.
fn action_options(names: &omsi_input::describe::ControlNames, script_actions: &HashMap<String, Vec<String>>, bindings: &Value) -> Vec<KeyActionOption> {
    let mut actions: HashMap<String, KeyActionOption> = HashMap::new();
    let mut found = |action: &str, label: Option<&str>, source: Option<String>| {
        let option = actions.entry(action.to_ascii_lowercase()).or_insert_with(|| KeyActionOption {
            action: action.to_string(),
            label: label.map_or_else(|| names.control(action), str::to_string),
            sources: Vec::new(),
            bus_paths: Vec::new(),
        });
        if let Some(source) = source.filter(|s| !option.sources.contains(s)) {
            option.sources.push(source);
        }
    };
    for (action, label) in names.actions() {
        found(&action, Some(&label), Some("Installed key-language files".into()));
    }
    for (action, label) in names.events() {
        found(&action, Some(&label), Some("OMSI event".into()));
    }
    for (action, sources) in script_actions {
        found(action, None, None);
        for source in sources {
            found(action, None, Some(format!("Bus script: {source}")));
        }
    }
    for action in bindings.get("vehicles").and_then(Value::as_array).into_iter().flatten().filter_map(|b| b.get("action").and_then(Value::as_str)) {
        found(action, None, Some("Configured binding".into()));
    }
    // OMSI's event browser uses `events()`, which keeps the spelling of the language files'
    // spelling table: the picker shows that one
    let events: HashMap<String, (String, String)> = names.events().into_iter().map(|(a, l)| (a.to_ascii_lowercase(), (a, l))).collect();
    let mut options: Vec<_> = actions.into_iter().map(|(key, mut option)| {
        if let Some((action, label)) = events.get(&key) {
            option.action = action.clone();
            option.label = label.clone();
        }
        option.bus_paths = bus_source_paths(&option.sources);
        option
    }).collect();
    options.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()).then_with(|| a.action.to_lowercase().cmp(&b.action.to_lowercase())));
    options
}

/// Lower case, every run of other characters one space: "MAN A26" finds `MAN_A26_3D.bus`.
fn normalize_source_query(text: &str) -> String {
    let mut normalized = String::new();
    for c in text.chars() {
        if c.is_alphanumeric() {
            normalized.extend(c.to_lowercase());
        } else if !normalized.ends_with(' ') {
            normalized.push(' ');
        }
    }
    normalized.trim().to_string()
}

/// The bus file of a source (`Bus script: Pack/Bus.bus (Maker Type)`).
fn source_path(source: &str) -> String {
    let source = source.strip_prefix("Bus script: ").unwrap_or(source);
    let source = source.split_once(": ").map(|(_, path)| path).unwrap_or(source);
    source.split_once(" (").map(|(path, _)| path).unwrap_or(source).to_string()
}

/// The bus files among an action's sources, each once.
fn bus_source_paths(sources: &[String]) -> Vec<String> {
    let mut paths = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for source in sources.iter().filter_map(|s| s.strip_prefix("Bus script: ")) {
        let path = source_path(source);
        if seen.insert(path.to_ascii_lowercase()) {
            paths.push(path);
        }
    }
    paths
}

/// The last scan of the buses' scripts, kept on this computer (the data folder's
/// `cache/key-actions.json`) so the picker is full at once on the next start: the actions
/// found, the bus files that use them and how many were read, for one OMSI folder. Nothing
/// leaves the computer; a new scan replaces it every time the picker is first opened.
#[derive(serde::Deserialize, serde::Serialize)]
struct ScriptActionCache {
    version: u32,
    root: String,
    actions: HashMap<String, Vec<String>>,
    source_paths: Vec<String>,
    total_buses: usize,
}

fn script_action_cache_path() -> std::path::PathBuf {
    core::data_dir().join("cache").join("key-actions.json")
}

fn load_script_action_cache(path: &std::path::Path, root: &str) -> Result<Option<ScriptActionCache>, String> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let cache: ScriptActionCache = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    Ok((cache.version == 1 && cache.root == root).then_some(cache))
}

fn save_script_action_cache(path: &std::path::Path, cache: &ScriptActionCache) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| "cache path has no parent directory".to_string())?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let bytes = serde_json::to_vec(cache).map_err(|error| error.to_string())?;
    std::fs::write(path, bytes).map_err(|error| error.to_string())
}

#[derive(Default)]
struct ServiceScan {
    root: String,
    run: u64,
    started: bool,
    cached: Option<(HashMap<String, Vec<String>>, Vec<String>, usize)>,
    found: HashMap<String, Vec<String>>,
    paths: Vec<String>,
    progress: (usize, usize, String),
    complete: bool,
    finished: Option<std::time::Instant>,
}

static SERVICE_SCAN: std::sync::Mutex<Option<ServiceScan>> = std::sync::Mutex::new(None);

fn service_names(root: &str, lang: &str) -> std::sync::Arc<omsi_input::describe::ControlNames> {
    static NAMES: std::sync::Mutex<Option<(String, String, std::sync::Arc<omsi_input::describe::ControlNames>)>> = std::sync::Mutex::new(None);
    let code = omsi_input::describe::language_code(lang);
    let mut held = NAMES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((r, l, names)) = held.as_ref() {
        if r == root && *l == code {
            return names.clone();
        }
    }
    let names = std::sync::Arc::new(omsi_input::describe::ControlNames::load(std::path::Path::new(root), &code));
    *held = Some((root.to_string(), code, names.clone()));
    names
}

struct ScanEnd(String, u64);

impl Drop for ScanEnd {
    fn drop(&mut self) {
        let mut held = SERVICE_SCAN.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(s) = held.as_mut().filter(|s| s.root == self.0 && s.run == self.1 && !s.complete) {
            log::warn!("key action scan stopped before it completed");
            s.complete = true;
            if let Some((actions, paths, total)) = s.cached.take() {
                (s.found, s.paths, s.progress.1) = (actions, paths, total);
            }
        }
    }
}

type ServiceCatalog = (Option<(HashMap<String, Vec<String>>, Vec<String>)>, (usize, usize, String), bool, String);

fn service_scan(root: &str, start: bool, have: &str, lang: &str) -> ServiceCatalog {
    let mut held = SERVICE_SCAN.lock().unwrap_or_else(|e| e.into_inner());
    if held.as_ref().is_none_or(|s| s.root != root) {
        let run = held.as_ref().map_or(0, |s| s.run + 1);
        let cached = load_script_action_cache(&script_action_cache_path(), root)
            .unwrap_or_else(|error| {
                log::warn!("key action catalog cache could not be loaded: {error}");
                None
            })
            .map(|c| (c.actions, c.source_paths, c.total_buses));
        *held = Some(ServiceScan { root: root.to_string(), run, cached, ..Default::default() });
    }
    let s = held.as_mut().expect("set above");
    if start && s.finished.is_some_and(|t| t.elapsed() > std::time::Duration::from_secs(300)) {
        let done = (std::mem::take(&mut s.found), std::mem::take(&mut s.paths), s.progress.1);
        *s = ServiceScan { root: s.root.clone(), run: s.run + 1, cached: Some(done), ..Default::default() };
    }
    if start && !s.started {
        s.started = true;
        let (root, run) = (root.to_string(), s.run);
        std::thread::spawn(move || {
            let _end = ScanEnd(root.clone(), run);
            omsi_input::describe::ControlNames::scan_script_actions(|update| take_scan_update(&root, run, update));
        });
    }
    let progress = match (&s.cached, s.complete, s.progress.1) {
        (Some((_, _, total)), false, 0) => (0, *total, String::new()),
        _ => s.progress.clone(),
    };
    let (actions, paths) = match (&s.cached, s.complete) {
        (Some((actions, paths, _)), false) => (actions, paths),
        _ => (&s.found, &s.paths),
    };
    let stamp = format!("{}:{}:{}:{}:{lang}", s.run, s.complete, actions.len(), paths.len());
    let data = (stamp != have).then(|| (actions.clone(), paths.clone()));
    (data, progress, s.complete, stamp)
}

fn take_scan_update(root: &str, run: u64, update: omsi_input::describe::ScriptActionScanUpdate) {
    let mut held = SERVICE_SCAN.lock().unwrap_or_else(|e| e.into_inner());
    let Some(s) = held.as_mut().filter(|s| s.root == root && s.run == run) else { return };
    s.progress = (update.done, update.total, update.current);
    for (action, source) in update.discovered {
        let path = source_path(&source);
        if !s.paths.iter().any(|p| p.eq_ignore_ascii_case(&path)) {
            s.paths.push(path);
        }
        let sources = s.found.entry(action.to_ascii_lowercase()).or_default();
        if !sources.iter().any(|existing| existing.eq_ignore_ascii_case(&source)) {
            sources.push(source);
        }
    }
    if update.complete {
        s.complete = true;
        s.finished = Some(std::time::Instant::now());
        s.paths.sort_by_key(|path| normalize_source_query(path));
        let cache = ScriptActionCache { version: 1, root: root.to_string(), actions: s.found.clone(), source_paths: s.paths.clone(), total_buses: update.total };
        drop(held);
        if let Err(error) = save_script_action_cache(&script_action_cache_path(), &cache) {
            log::warn!("key action catalog cache could not be saved: {error}");
        }
    }
}

pub fn key_actions(req: &Value) -> Value {
    let _ = core::content_dir();
    let root = core::load_config().root;
    let names = service_names(&root, req.get("lang").and_then(Value::as_str).unwrap_or("ENG"));
    let list = |key: &str| -> Vec<String> { req.get(key).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(String::from).collect() };
    let labels: serde_json::Map<String, Value> = list("actions").into_iter().map(|a| { let label = names.control(&a); (a, json!(label)) }).collect();
    if !req.get("catalog").and_then(Value::as_bool).unwrap_or(false) {
        return json!({ "labels": labels });
    }
    let have = req.get("have").and_then(Value::as_str).unwrap_or("");
    let (data, (done, total, current), complete, stamp) = service_scan(&root, req.get("scan").and_then(Value::as_bool).unwrap_or(true), have, &names.lang);
    let scan = json!({ "done": done, "total": total, "current": current, "complete": complete });
    let Some((scripts, paths)) = data else {
        return json!({ "labels": labels, "scan": scan, "stamp": stamp });
    };
    let bindings = json!({ "vehicles": list("vehicles").into_iter().map(|a| json!({ "action": a })).collect::<Vec<_>>() });
    let index: HashMap<String, usize> = paths.iter().enumerate().map(|(i, p)| (p.to_ascii_lowercase(), i)).collect();
    let options: Vec<Value> = action_options(&names, &scripts, &bindings)
        .into_iter()
        .map(|o| json!({ "action": o.action, "label": o.label, "buses": o.bus_paths.iter().filter_map(|p| index.get(&p.to_ascii_lowercase())).collect::<Vec<_>>() }))
        .collect();
    json!({ "labels": labels, "options": options, "bus_files": paths, "scan": scan, "stamp": stamp })
}

#[cfg(test)]
mod key_actions_tests {
    use super::*;


    #[test]
    fn empty_query_keeps_catalog_entries_past_twelve() {
        let names = omsi_input::describe::ControlNames::from_table("ENG", &[]);
        let script_actions: HashMap<String, Vec<String>> = (0..40)
            .map(|i| (format!("vehicle_action_{i:02}"), Vec::new()))
            .collect();
        let options = action_options(&names, &script_actions, &json!({}));
        assert_eq!(options.len(), 40);
    }

    #[test]
    fn script_action_cache_round_trips_and_is_scoped_to_the_install_root() {
        let dir = std::env::temp_dir().join(format!(
            "openomsi_action_cache_{}_{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),
        ));
        let path = dir.join("key-actions.json");
        let cache = ScriptActionCache {
            version: 1,
            root: "C:\\OMSI 2".into(),
            actions: HashMap::from([(
                "cruise_control".into(),
                vec!["BusPack/Bus.bus (Example Bus)".into()],
            )]),
            source_paths: vec!["BusPack/Bus.bus".into()],
            total_buses: 12,
        };

        save_script_action_cache(&path, &cache).unwrap();
        let loaded = load_script_action_cache(&path, "C:\\OMSI 2").unwrap().unwrap();
        assert_eq!(loaded.actions, cache.actions);
        assert_eq!(loaded.source_paths, cache.source_paths);
        assert_eq!(loaded.total_buses, cache.total_buses);
        assert!(load_script_action_cache(&path, "D:\\OMSI 2").unwrap().is_none());

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_launcher_page_gets_the_scan_as_it_goes() {
        *SERVICE_SCAN.lock().unwrap() = Some(ServiceScan { root: "R".into(), ..Default::default() });
        let update = |found: &[(&str, &str)]| omsi_input::describe::ScriptActionScanUpdate {
            done: 1,
            total: 2,
            current: "A/a.bus".into(),
            discovered: found.iter().map(|(a, s)| (a.to_string(), s.to_string())).collect(),
            complete: false,
        };
        take_scan_update("R", 0, update(&[("ivu_x", "A/a.bus (Maker A)"), ("IVU_X", "A/a.bus (Maker A)")]));
        take_scan_update("other", 0, update(&[("lost", "B/b.bus")]));
        take_scan_update("R", 7, update(&[("stale", "C/c.bus")]));
        let (data, progress, complete, _) = service_scan("R", false, "", "ENG");
        let (actions, paths) = data.unwrap();
        assert_eq!(actions["ivu_x"], ["A/a.bus (Maker A)"]);
        assert!(!actions.contains_key("lost"));
        assert!(!actions.contains_key("stale"));
        assert_eq!(paths, ["A/a.bus"]);
        assert_eq!((progress.0, progress.1), (1, 2));
        assert!(!complete);
    }
}
