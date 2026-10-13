//! The page's commands: those of every launcher window (`omsi-launcher-api`, run on a
//! worker so the window never waits) and what only this desktop window does - its file and
//! folder pickers (on the window's thread, as macOS wants them), the system's file manager and
//! browser, stepping aside while a game runs, the self-check.

use omsi_launcher_lib as core;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

type Answer = Result<Value, String>;

fn err(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

/// Run `f` on a worker thread.
async fn work<F: FnOnce() -> Answer + Send + 'static>(f: F) -> Answer {
    tauri::async_runtime::spawn_blocking(f).await.map_err(err)?
}

pub fn handler() -> impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static {
    tauri::generate_handler![call, call_bytes, pick_folder, pick_file, pick_mod, open_path, open_url, update_relaunch, selftest_done, fit_zoom]
}

/// This window, for the commands of `omsi-launcher-api`.
struct Window(AppHandle);

impl omsi_launcher_api::Platform for Window {
    fn emit(&self, event: &str, payload: Value) {
        let _ = self.0.emit(event, payload);
    }
    /// The window stays where it is while the game runs (it is the player's to close or
    /// minimise): a launcher that vanished at every start looked as if it had closed.
    fn launched(&self) {
        crate::service::release();
    }
    fn before_update(&self) {
        crate::service::release();
    }
    fn renderer(&self, req: Value, wait: Duration) -> Result<(Value, Vec<u8>), String> {
        crate::service::ask(req, wait)
    }
    fn selftest(&self) -> Option<String> {
        crate::selftest().then(|| std::env::var("OPENOMSI_LAUNCHER_SELFTEST").unwrap_or_default())
    }
    fn quit(&self) {
        self.0.exit(0);
    }
}

/// A command of the page (see `omsi-launcher-api`).
#[tauri::command]
async fn call(app: AppHandle, cmd: String, args: Value) -> Answer {
    work(move || omsi_launcher_api::call(&cmd, &args, &Window(app))).await
}

/// A command that answers bytes (the bus preview's pictures), as they are.
#[tauri::command]
async fn call_bytes(app: AppHandle, cmd: String, args: Value) -> Result<tauri::ipc::Response, String> {
    tauri::async_runtime::spawn_blocking(move || omsi_launcher_api::call_bytes(&cmd, &args, &Window(app)).map(tauri::ipc::Response::new)).await.map_err(err)?
}

/// A word to the page when the last running game has ended (its profile, runs and saved
/// situations are read again).
pub fn start_watch(app: AppHandle) {
    std::thread::Builder::new()
        .name("launcher watch".into())
        .spawn(move || {
            let mut was: Vec<core::instances::Instance> = Vec::new();
            loop {
                std::thread::sleep(std::time::Duration::from_secs(2));
                let asked = std::time::Instant::now();
                let all = core::list_instances();
                let running: Vec<_> = all.iter().filter(|i| i.running).cloned().collect();
                if running.is_empty() {
                    crate::service::no_game_since(asked);
                }
                for ended in was.iter().filter_map(|w| all.iter().find(|a| a.id == w.id && !a.running)) {
                    if let Some((event, what)) = omsi_launcher_api::game_ended(ended) {
                        core::log_to_file(&format!("game {} ended: {event}", ended.pid));
                        let _ = app.emit(event, what);
                    }
                }
                if !was.is_empty() && running.is_empty() {
                    let _ = app.emit("games-ended", ());
                }
                was = running;
            }
        })
        .ok();
}

// --- the system ------------------------------------------------------------------------

/// A folder picker (the window's thread: these commands are not async).
#[tauri::command]
fn pick_folder(title: String) -> Option<String> {
    core::pick_folder(&title).map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
fn pick_file(title: String) -> Option<String> {
    core::pick_file(&title).map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
fn pick_mod(archive: bool) -> Option<String> {
    core::pick_mod(archive).map(|p| p.to_string_lossy().to_string())
}

/// Show a folder or file in the system's file manager.
#[tauri::command]
async fn open_path(path: String) -> Answer {
    work(move || {
        let p = PathBuf::from(&path);
        if !p.exists() {
            return Err(format!("{path} is not there"));
        }
        open_with_system(p.as_os_str())
    })
    .await
}

/// A web page in the browser (only http and https).
#[tauri::command]
async fn open_url(url: String) -> Answer {
    work(move || {
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err("not a web address".into());
        }
        open_with_system(std::ffi::OsStr::new(&url))
    })
    .await
}

fn open_with_system(what: &std::ffi::OsStr) -> Answer {
    #[cfg(target_os = "macos")]
    let mut c = std::process::Command::new("open");
    #[cfg(windows)]
    let mut c = {
        let mut c = std::process::Command::new("explorer");
        c.arg("/n,");
        c
    };
    #[cfg(not(any(target_os = "macos", windows)))]
    let mut c = std::process::Command::new("xdg-open");
    c.arg(what).spawn().map_err(err)?;
    Ok(json!(true))
}

#[tauri::command]
fn fit_zoom(window: tauri::WebviewWindow, size: f64) -> Answer {
    let dpi = window.scale_factor().map_err(err)?;
    let inner = window.inner_size().map_err(err)?;
    let (w, h) = (inner.width as f64 / dpi, inner.height as f64 / dpi);
    let zoom = (w / 1440.0).min(h / 820.0).clamp(1.0, 2.2) * size.clamp(0.8, 1.5);
    window.set_zoom(zoom).map_err(err)?;
    Ok(json!(zoom))
}

#[tauri::command]
fn update_relaunch(app: AppHandle) -> Answer {
    let place = omsi_platform::updater::install_place().map_err(err)?;
    let game = place.exe.with_file_name(if cfg!(windows) { "openomsi.exe" } else { "openomsi" });
    let program = if game.is_file() { game } else { place.exe.clone() };
    omsi_launcher_lib::log_to_file(&format!("update: starting {} after the update", program.display()));
    std::process::Command::new(&program).current_dir(&place.dir).spawn().map_err(err)?;
    app.exit(0);
    Ok(json!(true))
}

/// The page's self-check is over: its report on stdout, and the program ends (failed checks:
/// exit code 1).
#[tauri::command]
fn selftest_done(app: AppHandle, report: Value) {
    if !crate::selftest() {
        return;
    }
    println!("{}", serde_json::to_string_pretty(&report).unwrap_or_default());
    let ok = report.get("failed").and_then(|f| f.as_array()).is_some_and(|f| f.is_empty());
    app.exit(if ok { 0 } else { 1 });
}
