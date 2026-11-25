//! Microsoft's WebView2, which the desktop launcher draws its page with on Windows: part of
//! Windows 11 and of an updated Windows 10, missing on some older or trimmed systems. Where
//! the launcher's window could not open, the game offers to install it (its `launcher_link`)
//! with Microsoft's own small installer (the "Evergreen Bootstrapper", which downloads the
//! runtime itself) - only when Windows really has no runtime (`runtime`): a launcher that
//! failed for another reason with the runtime there was taken for one without it, and the
//! installer, which says "already installed", was offered at every start.

/// Microsoft's permanent link to the Evergreen Bootstrapper.
const BOOTSTRAPPER: &str = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";

/// Download the installer and run it, waiting until it has finished; Ok once it reported
/// success. Only Windows has WebView2.
pub fn install() -> anyhow::Result<()> {
    if !cfg!(windows) {
        anyhow::bail!("WebView2 is only needed on Windows");
    }
    let path = std::env::temp_dir().join("MicrosoftEdgeWebview2Setup.exe");
    let resp = crate::updater::agent().get(BOOTSTRAPPER).call()?;
    let mut out = std::fs::File::create(&path)?;
    std::io::copy(&mut resp.into_reader(), &mut out)?;
    drop(out);
    // (its own window shows the progress; Windows asks for the administrator's consent)
    let status = std::process::Command::new(&path).arg("/install").status()?;
    let _ = std::fs::remove_file(&path);
    if !status.success() {
        anyhow::bail!("the WebView2 installer ended with {status}");
    }
    Ok(())
}

/// The WebView2 runtime as Windows has it registered.
#[derive(Clone, Debug)]
pub struct Runtime {
    pub version: String,
    /// Its program was found (a runtime whose files a "clean-up" tool removed stays registered,
    /// and its installer then says it is installed).
    pub present: bool,
}

/// The runtime Windows has registered (machine-wide or for this user), if any.
pub fn runtime() -> Option<Runtime> {
    #[cfg(windows)]
    {
        registered::runtime()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
mod registered {
    use super::Runtime;
    use std::path::PathBuf;
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW,
    };
    use windows::core::PCWSTR;

    /// The runtime's client key of Microsoft's updater (the same for every installation).
    const CLIENT: &str = r"Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";

    fn text(root: HKEY, key: &str, name: &str) -> Option<String> {
        let wide = |s: &str| {
            s.encode_utf16()
                .chain(std::iter::once(0))
                .collect::<Vec<u16>>()
        };
        let (key, name) = (wide(key), wide(name));
        let mut buf = vec![0u16; 1024];
        let mut len = (buf.len() * 2) as u32;
        // SAFETY: both names are NUL-terminated, the buffer's size in bytes is in `len`
        let r = unsafe {
            RegGetValueW(
                root,
                PCWSTR(key.as_ptr()),
                PCWSTR(name.as_ptr()),
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&mut len),
            )
        };
        if r.is_err() {
            return None;
        }
        buf.truncate(len as usize / 2);
        let s = String::from_utf16_lossy(&buf)
            .trim_end_matches('\0')
            .trim()
            .to_string();
        (!s.is_empty()).then_some(s)
    }

    pub(super) fn runtime() -> Option<Runtime> {
        let keys = [
            (
                HKEY_LOCAL_MACHINE,
                format!(r"SOFTWARE\WOW6432Node\{CLIENT}"),
            ),
            (HKEY_LOCAL_MACHINE, format!(r"SOFTWARE\{CLIENT}")),
            (HKEY_CURRENT_USER, format!(r"Software\{CLIENT}")),
        ];
        let mut found: Option<Runtime> = None;
        for (root, key) in keys {
            let Some(version) = text(root, &key, "pv").filter(|v| v != "0.0.0.0") else {
                continue;
            };
            // (where it lives: the updater's own note, else the places its installers use)
            let mut dirs: Vec<PathBuf> = text(root, &key, "location")
                .map(PathBuf::from)
                .into_iter()
                .collect();
            for (var, sub) in [
                ("ProgramFiles(x86)", r"Microsoft\EdgeWebView\Application"),
                ("ProgramFiles", r"Microsoft\EdgeWebView\Application"),
                ("LOCALAPPDATA", r"Microsoft\EdgeWebView\Application"),
            ] {
                if let Some(base) = std::env::var_os(var) {
                    dirs.push(PathBuf::from(base).join(sub));
                }
            }
            let present = dirs
                .iter()
                .any(|d| d.join(&version).join("msedgewebview2.exe").is_file());
            if present {
                return Some(Runtime { version, present });
            }
            found.get_or_insert(Runtime { version, present });
        }
        found
    }
}
