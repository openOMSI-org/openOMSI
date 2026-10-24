//! openOMSI's desktop launcher: a Tauri window whose page (`launcher/ui`) puts the duty
//! together, keeps the driver's profile, the settings, key bindings and mods, and starts the
//! game. Everything it knows comes from `omsi-launcher-core` (the same functions the old
//! launcher window and `openomsi-launcher --cli` use), called here as Tauri commands.
//!
//! The game program finds this launcher beside itself and opens it on a plain start; when
//! the webview cannot start here (no WebView2 runtime on an old Windows, no WebKitGTK), the
//! program ends with an error, which the game tells the player about.

// a window, not a console program, on Windows
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod commands;
mod data_folder;
mod discord;
mod protocol;
mod service;

use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

/// `OPENOMSI_LAUNCHER_SELFTEST=1`: the window stays hidden (and out of the Dock), the page
/// checks itself - commands, the `omsi://` pictures, the font, every page - prints its report
/// and the program ends with it (see `ui/js/selftest.js`). Checks never put a window on the
/// screen.
pub(crate) fn selftest() -> bool {
    std::env::var_os("OPENOMSI_LAUNCHER_SELFTEST").is_some_and(|v| !v.is_empty() && v != "0")
}

/// `OPENOMSI_WEBVIEW_FRESH` (the game starts the launcher so after its window could not open
/// on a Windows that has WebView2): the web view's profile (`EBWebView` in the launcher's data
/// folder) is set aside - a broken one stops WebView2 from starting - and where that folder
/// cannot be written at all, one in the temporary folder is used. None: Tauri's own place.
fn fresh_webview_profile(app: &tauri::App) -> Option<std::path::PathBuf> {
    if !cfg!(windows) || std::env::var_os("OPENOMSI_WEBVIEW_FRESH").is_none() {
        return None;
    }
    let temp = std::env::temp_dir().join("openOMSI-webview");
    let Ok(dir) = app.path().app_local_data_dir() else {
        return Some(temp);
    };
    if std::fs::create_dir_all(&dir).is_err() {
        omsi_launcher_lib::log_to_file(&format!("webview: {} cannot be made, the profile goes to {}", dir.display(), temp.display()));
        return Some(temp);
    }
    let profile = dir.join("EBWebView");
    if profile.exists() {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let aside = dir.join(format!("EBWebView.old-{secs}"));
        if let Err(e) = std::fs::rename(&profile, &aside) {
            omsi_launcher_lib::log_to_file(&format!("webview: the profile {} cannot be set aside ({e}), using {}", profile.display(), temp.display()));
            return Some(temp);
        }
        omsi_launcher_lib::log_to_file(&format!("webview: the profile was set aside as {}", aside.display()));
    }
    Some(dir)
}

/// Tell the games started from now on which screen this window stands on (#1959, #2025: the
/// game opened on the main screen): the middle of the screen the window is on, else (no
/// screen known) the window's own middle.
fn remember_screen(window: &tauri::Window) {
    let at = window
        .current_monitor()
        .ok()
        .flatten()
        .map(|m| (m.position().x + m.size().width as i32 / 2, m.position().y + m.size().height as i32 / 2))
        .or_else(|| {
            let (p, s) = (window.outer_position().ok()?, window.outer_size().ok()?);
            Some((p.x + s.width as i32 / 2, p.y + s.height as i32 / 2))
        });
    if at.is_some() {
        omsi_launcher_lib::instances::set_screen_at(at);
    }
}

fn main() {
    // Restore folder grants before migration, logging, or bootstrap can read user data.
    if let Err(e) = data_folder::restore_access() {
        eprintln!("data folder access could not be restored: {e}");
    }
    // A panic ends the launcher with code 101, and on Windows it has no console to say why:
    // the start guard only saw "ended on an error before its first frames" (#2574). Its
    // message and place go into launcher.log, which the support package carries.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let at = info.location().map(|l| format!(" at {}:{}", l.file(), l.line())).unwrap_or_default();
        let what = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_default();
        let thread = std::thread::current().name().unwrap_or("unnamed").to_string();
        omsi_launcher_lib::log_to_file(&format!("the launcher panicked on thread '{thread}'{at}: {what}"));
        default_hook(info);
    }));
    omsi_cfg::migrate_legacy_data_dir();
    if let Ok(exe) = std::env::current_exe() {
        let _ = omsi_cfg::user_data::initialize_for(&exe);
    }
    omsi_launcher_lib::adopt_own_inputs();
    let args: Vec<String> = std::env::args().collect();
    // (the terminal commands of the old launcher binary work here too)
    if args.get(1).is_some_and(|a| a == "--cli") {
        let cmd = args.get(2).cloned().unwrap_or_default();
        let arg = args.get(3).cloned().unwrap_or_else(|| "{}".into());
        match omsi_launcher_lib::cli(&cmd, &arg) {
            Ok(v) => println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default()),
            Err(e) => {
                eprintln!("error: {e:#}");
                std::process::exit(1);
            }
        }
        return;
    }
    omsi_launcher_lib::log_to_file(&format!("launcher (Tauri) {} starting", omsi_platform::VERSION));
    omsi_launcher_lib::cleanup();

    let result = tauri::Builder::default()
        .register_asynchronous_uri_scheme_protocol("omsi", protocol::handle)
        .invoke_handler(commands::handler())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            if selftest() {
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }
            let mut builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()));
            if let Some(dir) = fresh_webview_profile(app) {
                builder = builder.data_directory(dir);
            }
            let window = builder
                .title("openOMSI")
                .inner_size(1280.0, 800.0)
                .min_inner_size(960.0, 620.0)
                .center()
                .prevent_overflow()
                // (the page's own background: no white window before the first picture)
                .background_color(tauri::webview::Color(23, 24, 27, 255))
                .visible(false)
                .show_when_loaded(!selftest())
                // (the page keeps polling the running games while the window rests behind one)
                .background_throttling(tauri::utils::config::BackgroundThrottlingPolicy::Disabled)
                .on_page_load(|_, p| {
                    if selftest() {
                        eprintln!("page {:?}: {}", p.event(), p.url());
                    }
                })
                .app_mode(true)
                .build()?;
            if let Some(icon) = app.default_window_icon().cloned() {
                let _ = window.set_icon(icon);
            }
            remember_screen(&window.as_ref().window());
            commands::start_watch(app.handle().clone());
            if !selftest() {
                discord::start();
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(
                event,
                tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) | tauri::WindowEvent::ScaleFactorChanged { .. } | tauri::WindowEvent::Focused(true)
            ) {
                remember_screen(window);
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if omsi_launcher_api::installing() {
                    api.prevent_close();
                }
            }
            // the window closed while it rests behind a game: the program ends with it
            if let tauri::WindowEvent::Destroyed = event {
                if window.app_handle().webview_windows().is_empty() {
                    window.app_handle().exit(0);
                }
            }
        })
        .build(tauri::generate_context!())
        .map(|app| {
            app.run(|_, e| match e {
                tauri::RunEvent::ExitRequested { api, .. } if omsi_launcher_api::installing() => api.prevent_exit(),
                tauri::RunEvent::Exit => service::stop(),
                _ => {}
            })
        });

    if let Err(e) = result {
        omsi_launcher_lib::log_to_file(&format!("the launcher's window could not open: {e}"));
        eprintln!("the launcher's window could not open: {e}");
        std::process::exit(1);
    }
}
