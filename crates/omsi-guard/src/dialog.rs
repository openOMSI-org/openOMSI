//! What the guard tells the player, in a message box that needs no graphics card: Windows'
//! own, macOS' through `osascript`, Linux' through zenity or kdialog (else on the terminal),
//! in the settings' language where there is a text for it (English otherwise).

use omsi_launcher_lib::i18n::{tr, trf};

/// The messages the guard can show.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// The version did not start; a newer one was installed and starts now.
    Updated { from: String, to: String },
    /// The version did not start; the one before it is back and starts now.
    WentBack { from: String, to: String },
    /// The project lists the version as broken; it was replaced (by `to`).
    ReplacedBroken { from: String, to: String, why: String },
    /// The version did not start; it starts once more in safe mode.
    SafeMode { version: String },
    /// Nothing helped: what the player can do.
    GiveUp { version: String, log: String },
}

const DOWNLOADS: &str = "https://github.com/openOMSI-org/openOMSI/releases/latest";

impl Message {
    /// The text in the settings' language.
    pub fn text(&self) -> String {
        match self {
            Message::Updated { from, to } => trf("openOMSI {from} could not start on this computer.\n\nIt has updated itself to {to} and starts again now.", &[("from", from), ("to", to)]),
            Message::WentBack { from, to } => trf("openOMSI {from} could not start on this computer.\n\nIt went back to {to}, which ran here, and starts again now. {from} will not be installed again, only a newer version.", &[("from", from), ("to", to)]),
            Message::ReplacedBroken { from, to, why } if why.is_empty() => trf("The project lists openOMSI {from} as broken.\n\nIt was replaced by {to} and starts again now.", &[("from", from), ("to", to)]),
            Message::ReplacedBroken { from, to, why } => trf("The project lists openOMSI {from} as broken: {why}.\n\nIt was replaced by {to} and starts again now.", &[("from", from), ("to", to), ("why", why)]),
            Message::SafeMode { version } => trf("openOMSI {version} could not start on this computer, and there is no newer or earlier version for it yet.\n\nIt tries once more in safe mode (simple graphics, no plugins).", &[("version", version)]),
            Message::GiveUp { version, log } => trf("openOMSI {version} does not start on this computer, and it could not heal itself.\n\nDownload the latest version: {url}\n\nThe start's log: {log}\nTo report it, attach a support package: openomsi --export-diagnostics support.json", &[("version", version), ("url", &DOWNLOADS), ("log", log)]),
        }
    }

    fn is_final(&self) -> bool {
        matches!(self, Message::GiveUp { .. })
    }
}

/// Show `m` to the player. A message that only says what happens next does not wait for the
/// player; the last one does, and offers the download page.
pub fn show(m: &Message) {
    let text = m.text();
    crate::note(&format!("message: {}", text.replace('\n', " ")));
    if let Some(file) = omsi_cfg::flags::OMSI_GUARD_DIALOG_FILE.var() {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(file) {
            let _ = writeln!(f, "{}\n---", text);
        }
        return;
    }
    platform_show(&text, m.is_final());
}

#[cfg(target_os = "macos")]
fn platform_show(text: &str, last: bool) {
    let q = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let quoted = q(text);
    let script = if last {
        let (close, open) = (q(&tr("Close")), q(&tr("Open the download page")));
        format!("set r to display dialog \"{quoted}\" with title \"openOMSI\" buttons {{\"{close}\", \"{open}\"}} default button 2 with icon caution\nif button returned of r is \"{open}\" then open location \"{DOWNLOADS}\"")
    } else {
        format!("display dialog \"{quoted}\" with title \"openOMSI\" buttons {{\"{}\"}} default button 1 giving up after 20", q(&tr("OK")))
    };
    let mut cmd = std::process::Command::new("osascript");
    cmd.arg("-e").arg(script);
    if last {
        let _ = cmd.status();
    } else {
        let _ = cmd.spawn();
    }
}

#[cfg(windows)]
fn platform_show(text: &str, last: bool) {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, IDYES, MB_ICONINFORMATION, MB_ICONWARNING, MB_OK, MB_SETFOREGROUND, MB_YESNO};
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let body = if last { format!("{text}\n\n{}", tr("Open the download page now?")) } else { text.to_string() };
    let (b, t) = (wide(&body), wide("openOMSI"));
    let style = if last { MB_YESNO | MB_ICONWARNING } else { MB_OK | MB_ICONINFORMATION };
    // SAFETY: two NUL-terminated wide strings that live through the call
    let answer = unsafe { MessageBoxW(None, PCWSTR(b.as_ptr()), PCWSTR(t.as_ptr()), style | MB_SETFOREGROUND) };
    if last && answer == IDYES {
        omsi_platform::updater::open_url(DOWNLOADS);
    }
}

#[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
fn platform_show(text: &str, last: bool) {
    let kind = if last { "--error" } else { "--info" };
    let shown = std::process::Command::new("zenity").arg(kind).arg("--title=openOMSI").arg(format!("--text={text}")).status().map(|s| s.code().is_some()).unwrap_or(false)
        || std::process::Command::new("kdialog").arg("--title").arg("openOMSI").arg(if last { "--error" } else { "--msgbox" }).arg(text).status().map(|s| s.code().is_some()).unwrap_or(false);
    if !shown {
        eprintln!("openOMSI: {text}");
    }
}

#[cfg(any(target_os = "android", not(any(unix, windows))))]
fn platform_show(text: &str, _last: bool) {
    // (a phone: the launcher shows what happened; here it goes into the logs)
    eprintln!("openOMSI: {text}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_message_names_the_versions() {
        omsi_launcher_lib::i18n::pin_language("ENG");
        let ms = [
            Message::Updated { from: "0.2.31".into(), to: "0.2.32".into() },
            Message::WentBack { from: "0.2.31".into(), to: "0.2.30".into() },
            Message::ReplacedBroken { from: "0.2.31".into(), to: "0.2.30".into(), why: "saves".into() },
            Message::SafeMode { version: "0.2.31".into() },
            Message::GiveUp { version: "0.2.31".into(), log: "/x/guard.log".into() },
        ];
        for m in &ms {
            let t = m.text();
            assert!(t.contains("0.2.31") && !t.contains('{'), "{t}");
        }
        assert!(ms[2].text().contains("saves"));
        assert!(ms[4].text().contains(DOWNLOADS));
    }
}
