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
    tauri::generate_handler![call, call_bytes, pick_folder, pick_file, pick_mod, open_path, open_url, classic_launcher, selftest_done]
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
    fn renderer(&self, req: Value, wait: Duration) -> Result<(Value, Vec<u8>), String> {
        crate::service::ask(req, wait)
    }
    fn selftest(&self) -> Option<String> {
        crate::selftest().then(|| std::env::var("OPENOMSI_LAUNCHER_SELFTEST").unwrap_or_default())
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
            let mut running = 0usize;
            loop {
                std::thread::sleep(std::time::Duration::from_secs(2));
                let asked = std::time::Instant::now();
                let now = core::list_instances().iter().filter(|i| i.running).count();
                if now == 0 {
                    crate::service::no_game_since(asked);
                }
                if running > 0 && now == 0 {
                    let _ = app.emit("games-ended", ());
                }
                running = now;
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

/// Switch to the game's own launcher window (and close this one).
#[tauri::command]
fn classic_launcher(app: AppHandle) -> Answer {
    if !crate::classic_launcher() {
        return Err("the game program was not found".into());
    }
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
