//! Another launcher program: the desktop launcher (`launcher/`, a Tauri window over
//! omsi-launcher-core) when it is beside the game, or any program `OMSI_LAUNCHER` names. The
//! game's own window (`launcher`) is the launcher where neither is there, with `--launcher`,
//! and with `OMSI_CLASSIC_LAUNCHER`.

use super::*;

/// The launcher binary: next to this program, or the launcher's own build folder in the
/// source tree, or in ~/.openomsi.
pub(crate) fn find_launcher() -> Option<PathBuf> {
    let mut cands: Vec<PathBuf> = Vec::new();
    if let Some(p) = omsi_cfg::flags::OMSI_LAUNCHER.os() {
        cands.push(PathBuf::from(p));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            cands.push(dir.join("openomsi-launcher"));
            cands.push(dir.join("openomsi-launcher.exe"));
            cands.push(dir.join("openOMSI.app/Contents/MacOS/openomsi-launcher"));
        }
    }
    if let Some(memo) = root_memo() {
        if let Some(home) = memo.parent() {
            cands.push(home.join(".openomsi/openomsi-launcher"));
        }
    }
    cands.into_iter().find(|p| p.is_file())
}

/// The desktop launcher: beside this program (in the same bundle on macOS), or the
/// launcher's own build folder in the source tree (a development build).
pub(crate) fn find_app_launcher() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let name = if cfg!(windows) { "openomsi-launcher-app.exe" } else { "openomsi-launcher-app" };
    let mut cands = vec![dir.join(name)];
    for a in exe.ancestors().skip(1).take(4) {
        for profile in ["release", "debug"] {
            cands.push(a.join("launcher/src-tauri/target").join(profile).join(name));
        }
    }
    cands.into_iter().find(|p| p.is_file())
}

/// Whether a plain start opens the desktop launcher (openOMSI's Tauri window): only when the
/// player chose it - `launcher_ui=desktop` in the settings, or `OMSI_LAUNCHER_UI=desktop` for
/// one start. This build's own launcher (the Omsi-Hub interface in the game's window) is the
/// one a plain start opens otherwise, and the desktop launcher's WebView2 is never asked for.
pub(crate) fn desktop_launcher_chosen() -> bool {
    if let Ok(v) = omsi_cfg::env::var("OMSI_LAUNCHER_UI") {
        return omsi_launcher_lib::launcher_ui(&v) == "desktop";
    }
    omsi_launcher_lib::get_settings().map(|v| omsi_launcher_lib::launcher_ui(v.get("launcher_ui").and_then(|x| x.as_str()).unwrap_or("new")) == "desktop").unwrap_or(false)
}

/// A plain start: the desktop launcher when it is there (`Ok(true)` once it runs). It opens
/// the game's own launcher by itself where its window cannot start (no webview).
pub(crate) fn open_app_launcher() -> Result<bool> {
    if omsi_cfg::flags::OMSI_CLASSIC_LAUNCHER.is_set() || cfg!(any(target_os = "android", target_os = "ios")) {
        return Ok(false);
    }
    let Some(bin) = find_app_launcher() else {
        return Ok(false);
    };
    log::info!("starting the desktop launcher {}", bin.display());
    let exe = std::env::current_exe().unwrap_or_default();
    match std::process::Command::new(&bin).env("OPENOMSI_BIN", &exe).spawn() {
        Ok(mut child) => {
            // (a program that cannot even load - Linux without WebKitGTK's library - ends at
            // once with an error, before it could open the game's own launcher itself)
            let t0 = std::time::Instant::now();
            while t0.elapsed() < std::time::Duration::from_secs(2) {
                match child.try_wait() {
                    Ok(Some(status)) if !status.success() => {
                        log::warn!("the desktop launcher {} ended at once ({status}): the game's own opens", bin.display());
                        return Ok(false);
                    }
                    Ok(Some(_)) | Err(_) => break,
                    Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
                }
            }
            Ok(true)
        }
        Err(e) => {
            log::warn!("the desktop launcher {} did not start ({e}): the game's own opens", bin.display());
            Ok(false)
        }
    }
}

/// Start the launcher and return once it is running. `Ok(false)` when there is none.
pub(crate) fn open_launcher() -> Result<bool> {
    let Some(bin) = find_launcher() else {
        return Ok(false);
    };
    log::info!("starting the launcher {}", bin.display());
    let exe = std::env::current_exe().unwrap_or_default();
    std::process::Command::new(&bin)
        .env("OPENOMSI_BIN", &exe)
        .spawn()
        .map_err(|e| anyhow!("starting {}: {e}", bin.display()))?;
    Ok(true)
}
