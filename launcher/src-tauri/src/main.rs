//! openOMSI's desktop launcher: a Tauri window whose page (`launcher/ui`) puts the duty
//! together, keeps the driver's profile, the settings, key bindings and mods, and starts the
//! game. Everything it knows comes from `omsi-launcher-core` (the same functions the old
//! launcher window and `openomsi-launcher --cli` use), called here as Tauri commands.
//!
//! The game program finds this launcher beside itself and opens it on a plain start; when
//! the webview cannot start here (no WebView2 runtime on an old Windows, no WebKitGTK), the
//! game's own launcher window opens instead (`classic_launcher`).

// a window, not a console program, on Windows
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod commands;
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

fn main() {
    omsi_cfg::migrate_legacy_data_dir();
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
            let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
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
            commands::start_watch(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
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
        if !classic_launcher() {
            std::process::exit(1);
        }
    }
}

/// The game's own launcher window (`openomsi --launcher`), for a system without a webview.
fn classic_launcher() -> bool {
    let Some(game) = omsi_launcher_lib::game_binary() else { return false };
    omsi_launcher_lib::log_to_file(&format!("opening the game's own launcher: {}", game.display()));
    // (on Windows it offers to install WebView2, which is what is missing there)
    std::process::Command::new(game).arg("--launcher").env("OPENOMSI_NO_WEBVIEW", "1").spawn().is_ok()
}
