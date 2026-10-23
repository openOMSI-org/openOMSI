//! openOMSI on an iPhone or an iPad: the program the Swift app's `main` hands over to.
//!
//! The app is the Swift one (`apple/`): it keeps UIKit's scene life cycle, and its scene
//! shows the launcher made for Apple's systems (SwiftUI, `apple/Shared`). The game is this
//! library: `openomsi_ios_main` runs winit's event loop (which is UIKit's `UIApplicationMain`),
//! and as on Android the launcher and the game share one process - the launcher hands the
//! game's command line over (`omsi_launcher_lib::launch` keeps it, see `apple_bridge`), the
//! game opens its window on the same scene, in front of the launcher's, and when the session
//! ends (the menu's Quit) that window goes and the launcher is there again.
//!
//! What lives where on the device: the app's own data (settings, profiles, sessions, logs)
//! in `Library/Application Support/openOMSI` (`HOME` for the rest of the program); the
//! original game, the mods and the screenshots in the app's Documents, which the Files app
//! shows as "openOMSI" (on the device, and from a Mac's Finder over a cable).

use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use winit::event_loop::EventLoopProxy;
use winit::platform::ios::{ValidOrientations, WindowAttributesExtIOS};

/// The scene the Swift app connected (its windows are where the game's window goes).
static SCENE_READY: AtomicBool = AtomicBool::new(false);
/// Wakes the event loop from the Swift side (a scene connected, a game asked for).
static PROXY: Mutex<Option<EventLoopProxy<()>>> = Mutex::new(None);

/// Where the original game, the mods and the screenshots go: the app's Documents.
pub fn documents() -> PathBuf {
    PathBuf::from(std::env::var_os("OPENOMSI_CONTAINER").unwrap_or_default()).join("Documents")
}

/// Wake the event loop (from any thread).
pub(crate) fn wake() {
    if let Some(p) = PROXY.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        let _ = p.send_event(());
    }
}

/// The Swift app's `main`: never returns (UIKit's run loop runs the app from here on).
#[unsafe(no_mangle)]
pub extern "C" fn openomsi_ios_main() {
    // HOME is the app's container on iOS: its root may not be written, its Library may
    let container = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    unsafe { std::env::set_var("OPENOMSI_CONTAINER", &container) };
    let home = container.join("Library/Application Support/openOMSI");
    let _ = std::fs::create_dir_all(&home);
    unsafe { std::env::set_var("HOME", &home) };
    let content = documents();
    let _ = std::fs::create_dir_all(&content);
    unsafe { std::env::set_var("OMSI_CONTENT", &content) };
    let crash_files = vec![content.join("crash.log"), home.join("crash.log")];
    std::panic::set_hook(Box::new(move |info| {
        // (one the renderer catches - a graphics interface it cannot open - is no end of
        // the game: logged as one, as on the other systems)
        if omsi_render::catching() {
            log::warn!("caught by the renderer: {info}");
            return;
        }
        let text = format!("the game stopped on an error (build {BUILD}): {info}\n{}", std::backtrace::Backtrace::force_capture());
        log::error!("{text}");
        for f in &crash_files {
            let _ = std::fs::write(f, &text);
        }
    }));
    init_log();
    log::info!("openOMSI {VERSION} for iOS, build {BUILD}");
    log::info!("home {}, content {}", home.display(), content.display());
    let _ = std::fs::write(content.join("README.txt"), README);
    // `openOMSI/env.txt`: the OMSI_* switches a computer takes from its environment, one
    // `NAME=value` a line (a phone has no environment to set; for looking into problems)
    if let Ok(t) = std::fs::read_to_string(content.join("env.txt")) {
        for line in t.lines() {
            if let Some((k, v)) = line.trim().split_once('=') {
                let k = k.trim();
                if k.starts_with("OMSI_") && !k.contains(char::is_whitespace) {
                    log::info!("env.txt: {k}");
                    unsafe { std::env::set_var(k, v.trim()) };
                }
            }
        }
    }
    omsi_cfg::migrate_legacy_data_dir();
    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1) % 1_000_000_000;
    omsi_script::set_session_seed(seed);
    // the original installation and the content roots, as a bare start finds them (the Swift
    // launcher reads them through `apple_bridge`)
    crate::apple_bridge::prepare_once();

    let event_loop = match EventLoop::new() {
        Ok(e) => e,
        Err(e) => {
            log::error!("no event loop: {e}");
            return;
        }
    };
    *PROXY.lock().unwrap_or_else(|e| e.into_inner()) = Some(event_loop.create_proxy());
    let mut shell = Shell { game: None };
    // (UIApplicationMain: the scene delegate of the Swift app connects its scene from here)
    if let Err(e) = event_loop.run_app(&mut shell) {
        log::error!("{e}");
    }
}

/// The Swift app's scene is connected: windows can be shown on it.
#[unsafe(no_mangle)]
pub extern "C" fn openomsi_ios_scene_ready() {
    SCENE_READY.store(true, Ordering::Relaxed);
    wake();
}

// --- the log ---------------------------------------------------------------------------

/// The log to the console (Xcode's, the Mac's Console app) and to `game.log` in the app's
/// data folder; the previous run's is kept as `game-prev.log` for the launcher's crash
/// report (`previous_run_crash`), as on Android.
fn init_log() {
    let dir = omsi_launcher_lib::data_dir();
    let (now, prev) = (dir.join("game.log"), dir.join("game-prev.log"));
    let _ = std::fs::rename(&now, &prev);
    let file = std::fs::File::create(&now).ok();
    let logger = TeeLogger { file: Mutex::new(file) };
    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
}

struct TeeLogger {
    file: Mutex<Option<std::fs::File>>,
}

impl log::Log for TeeLogger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Info && !m.target().starts_with("wgpu_hal") && !m.target().starts_with("naga")
    }

    fn log(&self, r: &log::Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        use std::io::Write;
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
        // (the desktop's env_logger layout: the launcher's `crash_of` reads it)
        let line = format!("[{secs:.3} {:<5} {}] {}", r.level(), r.target(), r.args());
        eprintln!("{line}");
        if let Some(f) = self.file.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            let _ = writeln!(f, "{line}");
        }
    }

    fn flush(&self) {
        if let Some(f) = self.file.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            use std::io::Write;
            let _ = f.flush();
        }
    }
}

/// When the app's previous run closed while a drive was going: what it said last and the
/// end of its log, for the launcher's crash dialog (see `android::previous_run_crash`).
pub(crate) fn previous_run_crash() -> Option<(String, String)> {
    let p = omsi_launcher_lib::data_dir().join("game-prev.log");
    let text = std::fs::read_to_string(&p).ok()?;
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.iter().rposition(|l| l.contains("starting the game:"))?;
    if lines[start..].iter().any(|l| l.contains("game ends") || l.contains("session ended")) {
        return None;
    }
    // sent to the background and not brought back: iOS ended the app there (or the player
    // swiped it away) - no crash, whatever the game was doing
    let back = lines[start..].iter().rposition(|l| l.contains("app in the background"));
    let front = lines[start..].iter().rposition(|l| l.contains("app in front"));
    if back.is_some() && back > front {
        return None;
    }
    if let Some(c) = launcher::crash_of(&p) {
        return Some(c);
    }
    let last = lines.last().map(|l| l.split_once("] ").map(|x| x.1).unwrap_or(l)).unwrap_or("");
    let tail = lines[lines.len().saturating_sub(150)..].join("\n");
    Some((format!("the game closed without a word while it was running (the last it said: {last})"), tail))
}

const README: &str = "openOMSI\n\
\n\
Put a complete copy of OMSI 2 (the folder with Omsi.exe, maps and Vehicles in it) into this\n\
folder as \"OMSI 2\" - with the Files app, or from a Mac's Finder (the iPhone or iPad in the\n\
sidebar, Files, openOMSI) - and choose it in the launcher under Setup. A copy on iCloud Drive\n\
or a USB drive can be chosen there as well.\n\
Mods: copy them into openOMSI/Mods (they are installed when the launcher opens), or install a\n\
folder or a .zip, .7z or .rar from the launcher's Mods page. Screenshots are written to\n\
openOMSI/Screenshots.\n";

// --- the Swift launcher, or the game in front of it -----------------------------------------

struct Shell {
    game: Option<Box<App>>,
}

/// A window for the game, put on the Swift app's scene in front of the Swift launcher.
fn new_window(event_loop: &ActiveEventLoop) -> Option<Arc<Window>> {
    let attrs = Window::default_attributes()
        .with_title("openOMSI")
        // (the game's own controls reach the edges: the system's swipes there wait for a
        // second swipe, and the home bar keeps out of the picture)
        .with_prefers_home_indicator_hidden(true)
        .with_prefers_status_bar_hidden(true)
        .with_preferred_screen_edges_deferring_system_gestures(winit::platform::ios::ScreenEdge::ALL)
        .with_valid_orientations(if crate::apple::is_phone() { ValidOrientations::Landscape } else { ValidOrientations::LandscapeAndPortrait });
    let window = match event_loop.create_window(attrs) {
        Ok(w) => Arc::new(w),
        Err(e) => {
            log::error!("no window: {e}");
            return None;
        }
    };
    crate::apple::attach_window(&window);
    Some(window)
}

impl Shell {
    /// After every event: a game the launcher asked for starts, a game that ended gives the
    /// screen back.
    fn switch(&mut self, event_loop: &ActiveEventLoop) {
        if !SCENE_READY.load(Ordering::Relaxed) {
            return;
        }
        if let Some(game) = self.game.as_mut() {
            if !crate::platform::take_leave() {
                return;
            }
            game.exiting(event_loop);
            let window = game.window.take();
            self.game = None;
            lan_mods::clean_up();
            log::info!("session ended: back to the launcher");
            crate::apple_bridge::game_ended();
            if let Some(w) = window {
                crate::apple::detach_window(&w);
                drop(w);
            }
            return;
        }
        let Some(line) = omsi_launcher_lib::take_in_process_launch() else { return };
        log::info!("starting the game: {}", line.join(" "));
        let mut argv: Vec<String> = std::iter::once("openomsi".to_string()).chain(line).collect();
        // (`OMSI_APPLE_EXIT_AFTER=secs`: the session ends by itself, for checking the way back
        // to the launcher without a person at it)
        if let Ok(s) = std::env::var("OMSI_APPLE_EXIT_AFTER") {
            argv.extend(["--exit-after".to_string(), s]);
        }
        let args = match Args::try_parse_from(&argv) {
            Ok(a) => a,
            Err(e) => {
                log::error!("the launcher's command line: {e}");
                crate::apple_bridge::game_ended();
                return;
            }
        };
        let game = prepare(args, false).and_then(|p| match p {
            Some((args, server)) => make_app(args, server),
            None => Ok(None),
        });
        let mut app = match game {
            Ok(Some(app)) => app,
            Ok(None) => {
                crate::apple_bridge::game_ended();
                return;
            }
            Err(e) => {
                log::error!("the game could not start: {e:#}");
                crate::apple_bridge::game_status(&format!("{}: {e:#}", omsi_ui::tr("The game could not start")));
                crate::apple_bridge::game_ended();
                return;
            }
        };
        let Some(window) = new_window(event_loop) else {
            crate::apple_bridge::game_ended();
            return;
        };
        crate::apple::game_started();
        app.create_window(event_loop, Some(window));
        self.game = Some(Box::new(app));
    }
}

impl ApplicationHandler for Shell {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        log::info!("app in front");
        if let Some(g) = self.game.as_mut() {
            g.resumed(event_loop);
        }
        self.switch(event_loop);
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        // (iOS may end an app in the background without a word: see `previous_run_crash`)
        log::info!("app in the background");
        if let Some(g) = self.game.as_mut() {
            g.suspended(event_loop);
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, _event: ()) {
        self.switch(event_loop);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if let Some(g) = self.game.as_mut() {
            g.window_event(event_loop, id, event);
        }
        self.switch(event_loop);
    }

    fn device_event(&mut self, event_loop: &ActiveEventLoop, id: winit::event::DeviceId, event: DeviceEvent) {
        if let Some(g) = self.game.as_mut() {
            g.device_event(event_loop, id, event);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(g) = self.game.as_mut() {
            event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
            g.about_to_wait(event_loop);
        } else {
            // (the Swift launcher draws itself: nothing to do here until it asks)
            event_loop.set_control_flow(winit::event_loop::ControlFlow::Wait);
        }
        self.switch(event_loop);
    }

    fn memory_warning(&mut self, _event_loop: &ActiveEventLoop) {
        log::warn!("the system is short of memory");
        crate::memory::release_free_memory();
    }

    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(g) = self.game.as_mut() {
            g.exiting(event_loop);
        }
    }
}
