//! The launcher is another program: the desktop launcher (`launcher/`, a Tauri window over
//! omsi-launcher-core, `openomsi-launcher-app`) beside the game, or any program
//! `OMSI_LAUNCHER` names. A plain start opens it; the game has no launcher window of its own.

use anyhow::{anyhow, Result};
use omsi_launcher_lib::i18n::{fill, tr, trf};
use std::path::{Path, PathBuf};

/// The desktop launcher's program name.
const APP_LAUNCHER: &str = if cfg!(windows) { "openomsi-launcher-app.exe" } else { "openomsi-launcher-app" };

/// The desktop launcher: beside this program (in the same bundle on macOS; a development
/// build's folder when it was built into the same target folder, see docs/BUILDING.md).
pub fn find_app_launcher() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let bin = exe.parent()?.join(APP_LAUNCHER);
    bin.is_file().then_some(bin)
}

/// A plain start: the desktop launcher. Where it is missing, or ends at once with an error
/// (a system without its web view: no WebKitGTK library on Linux, no WebView2 runtime on an
/// old Windows - which is offered to be installed), the player is told why and where it
/// should be, and the program ends with an error. The start guard counts the launcher as
/// started (`healthy`) once it runs, and a start that failed as one to heal. `fatal_message`
/// shows the player what went wrong (the game's own dialog).
pub fn open_app_launcher(fatal_message: fn(&str)) -> Result<()> {
    let exe = std::env::current_exe().unwrap_or_default();
    let dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    let Some(bin) = find_app_launcher() else {
        let text = "The openOMSI launcher ({launcher}) was not found. It belongs in the same folder as the game ({folder}).\n\nPlease download openOMSI again and keep all of its files together.";
        let vars: &[(&str, &dyn std::fmt::Display)] = &[("launcher", &APP_LAUNCHER), ("folder", &dir.display())];
        fatal_message(&trf(text, vars));
        anyhow::bail!("{}", fill(text.to_string(), vars));
    };
    let failed = start_app_launcher(&bin, &exe, false);
    let english = failed.as_ref().map(|(text, why)| fill(text.to_string(), &[("launcher", &bin.display()), ("why", why)]));
    #[cfg(windows)]
    let failed = failed.and_then(|why| webview2_rescue(&bin, &exe, why));
    #[cfg(not(windows))]
    let failed = failed.map(|why| said(&bin, why));
    if let Some(why) = failed {
        fatal_message(&format!("{why}\n\n{}", webview_hint()));
        anyhow::bail!("{}", english.unwrap_or(why));
    }
    crate::healthy(crate::journal::Kind::Launcher);
    Ok(())
}

type Failure = (&'static str, String);

fn said(bin: &Path, (text, why): Failure) -> String {
    trf(text, &[("launcher", &bin.display()), ("why", &why)])
}

/// Start the desktop launcher; what went wrong when it did not start or ended at once.
/// `fresh`: its web view starts from a new profile (the old one set aside, see the launcher's
/// `fresh_webview_profile`).
fn start_app_launcher(bin: &Path, exe: &Path, fresh: bool) -> Option<Failure> {
    log::info!("starting the desktop launcher {}{}", bin.display(), if fresh { " (a fresh web view profile)" } else { "" });
    let mut cmd = std::process::Command::new(bin);
    cmd.env("OPENOMSI_BIN", exe);
    if fresh {
        cmd.env("OPENOMSI_WEBVIEW_FRESH", "1");
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return Some(("The openOMSI launcher ({launcher}) could not be started: {why}.", e.to_string())),
    };
    // (a program that cannot even load - Linux without WebKitGTK's library - or whose window
    // cannot open ends at once with an error)
    let t0 = std::time::Instant::now();
    while t0.elapsed() < std::time::Duration::from_secs(2) {
        match child.try_wait() {
            Ok(Some(status)) if !status.success() => return Some(("The openOMSI launcher ({launcher}) stopped right after it started ({why}).", status.to_string())),
            Ok(Some(_)) | Err(_) => break,
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
        }
    }
    None
}

/// The launcher ended at once on Windows: what its web view needs. Only a Windows without
/// the WebView2 runtime is offered Microsoft's installer (and the launcher tried again once it
/// is in); with the runtime there the launcher is tried once more with a fresh web view
/// profile (a broken one stops WebView2 as well), and what still fails is said as it is - the
/// installer, which only answers "already installed" then, was offered at every start before.
#[cfg(windows)]
fn webview2_rescue(bin: &Path, exe: &Path, why: Failure) -> Option<String> {
    match omsi_platform::webview2::runtime() {
        None => {
            log::warn!("desktop launcher: {}; no WebView2 runtime is registered", why.1);
            if offer_webview2() {
                start_app_launcher(bin, exe, false).map(|again| said(bin, again))
            } else {
                Some(said(bin, why))
            }
        }
        Some(rt) if !rt.present => {
            log::warn!("desktop launcher: {}; WebView2 {} is registered, its files are missing", why.1, rt.version);
            Some(format!("{}\n\n{}", said(bin, why), trf("Microsoft Edge WebView2 {version} is registered on this Windows, but its files are missing (removed by a clean-up tool?). Repair it: Settings > Apps > Installed apps > Microsoft Edge WebView2 Runtime > Modify > Repair, or install Microsoft's \"Evergreen Standalone Installer\" from https://developer.microsoft.com/microsoft-edge/webview2/", &[("version", &rt.version)])))
        }
        Some(rt) => {
            log::warn!("desktop launcher: {}; WebView2 {} is installed - trying a fresh web view profile", why.1, rt.version);
            let mut text = said(bin, start_app_launcher(bin, exe, true)?);
            if let Some(e) = launcher_error() {
                text.push_str(&format!("\n\n{}", trf("Its log says: {error}", &[("error", &e)])));
            }
            Some(format!("{text}\n\n{}", trf("Microsoft Edge WebView2 {version} is installed, so it is not missing.", &[("version", &rt.version)])))
        }
    }
}

/// What the launcher last wrote about its window not opening (its log, launcher.log).
#[cfg(windows)]
fn launcher_error() -> Option<String> {
    let log = std::fs::read_to_string(omsi_launcher_lib::data_dir().join("launcher.log")).ok()?;
    let line = log.lines().rev().take(20).find(|l| l.contains("could not open"))?;
    Some(line.split_once("could not open: ").map(|(_, e)| e).unwrap_or(line).trim().to_string())
}

/// Windows without WebView2 (the launcher's window could not open): Microsoft's installer,
/// when the player wants it. True once it is installed.
#[cfg(windows)]
fn offer_webview2() -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, IDYES, MB_ICONQUESTION, MB_YESNO};
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (t, c) = (wide(&tr("The openOMSI launcher shows its window with Microsoft Edge WebView2, which this Windows does not have yet.\n\nInstall it now with Microsoft's installer?")), wide("openOMSI"));
    // SAFETY: both strings are NUL-terminated and live for the call
    if unsafe { MessageBoxW(None, PCWSTR(t.as_ptr()), PCWSTR(c.as_ptr()), MB_YESNO | MB_ICONQUESTION) } != IDYES {
        return false;
    }
    match omsi_platform::webview2::install() {
        Ok(()) => true,
        // (the installer's "already installed" is an error code too: what counts is the runtime)
        Err(e) => {
            log::warn!("WebView2: {e:#}");
            omsi_platform::webview2::runtime().is_some_and(|r| r.present)
        }
    }
}

/// What the launcher needs of the system to show its window.
fn webview_hint() -> String {
    if cfg!(windows) {
        match omsi_platform::webview2::runtime() {
            Some(rt) if rt.present => trf("Its log is in the openOMSI data folder (launcher.log); Microsoft Edge WebView2 {version} is installed.", &[("version", &rt.version)]),
            _ => tr("The launcher's window needs Microsoft Edge WebView2 (part of Windows 10 and 11; for older systems: https://developer.microsoft.com/microsoft-edge/webview2/)."),
        }
    } else if cfg!(target_os = "linux") {
        let base = tr("The launcher's window needs WebKitGTK 4.1 (the package libwebkit2gtk-4.1-0 on Debian and Ubuntu, webkit2gtk4.1 on Fedora, webkit2gtk-4.1 on Arch).");
        // (SteamOS ships without it and its system is read-only, #2458)
        let steamos = std::fs::read_to_string("/etc/os-release").is_ok_and(|t| t.lines().any(|l| l.trim() == "ID=steamos" || l.trim() == "ID=\"steamos\""));
        if steamos {
            format!("{base}\n\n{}", tr("SteamOS does not include it. In Desktop Mode, in Konsole: sudo steamos-readonly disable, then sudo pacman -S webkit2gtk-4.1, then sudo steamos-readonly enable (a SteamOS update can remove it again). See docs/USER_GUIDE.md."))
        } else {
            base
        }
    } else {
        tr("Its log is in the openOMSI data folder (launcher.log).")
    }
}

/// Start the program `OMSI_LAUNCHER` names and return once it is running. `Ok(false)` when
/// it names none that is there.
pub fn open_launcher() -> Result<bool> {
    let Some(bin) = omsi_cfg::flags::OMSI_LAUNCHER.os().map(PathBuf::from).filter(|p| p.is_file()) else {
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
