//! The proxy the system is set to use, for the updater and the downloads (#1000: where
//! GitHub is reached only through a proxy, the update check failed although the browser and
//! everything else got through). The proxy variables first (`HTTPS_PROXY`, `ALL_PROXY`,
//! `HTTP_PROXY`, as curl reads them), then the system's own setting: Windows' Internet
//! Options, macOS's network settings. None: straight to the server, as before.

/// The proxy to go through, as `http://host:port` (read once).
pub fn system_proxy() -> Option<String> {
    static FOUND: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    FOUND
        .get_or_init(|| {
            let found = from_env().or_else(from_system);
            if let Some(p) = &found {
                log::info!("network: through the proxy {}", redact(p));
            }
            found
        })
        .clone()
}

fn from_env() -> Option<String> {
    [
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
        "HTTP_PROXY",
        "http_proxy",
    ]
    .iter()
    .filter_map(|v| std::env::var(v).ok())
    .map(|v| v.trim().to_string())
    .find(|v| !v.is_empty())
    .map(|v| with_scheme(&v))
}

#[cfg(windows)]
fn from_system() -> Option<String> {
    use windows::Win32::System::Registry::{
        HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegGetValueW,
    };
    use windows::core::PCWSTR;
    const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    let wide = |s: &str| {
        s.encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>()
    };
    let key = wide(KEY);
    let (enable, server) = (wide("ProxyEnable"), wide("ProxyServer"));
    let mut on = 0u32;
    let mut len = 4u32;
    // SAFETY: the names are NUL-terminated, the value is a DWORD of 4 bytes
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            PCWSTR(enable.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut on as *mut u32).cast()),
            Some(&mut len),
        )
    };
    if r.is_err() || on == 0 {
        return None;
    }
    let mut buf = vec![0u16; 1024];
    let mut len = (buf.len() * 2) as u32;
    // SAFETY: the names are NUL-terminated, the buffer's size in bytes is in `len`
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            PCWSTR(server.as_ptr()),
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
    windows_server(String::from_utf16_lossy(&buf).trim_end_matches('\0'))
}

#[cfg(target_os = "macos")]
fn from_system() -> Option<String> {
    let out = std::process::Command::new("/usr/sbin/scutil")
        .arg("--proxy")
        .output()
        .ok()?;
    scutil_proxy(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn from_system() -> Option<String> {
    None
}

/// Windows' `ProxyServer`: `host:port` for every protocol, or `http=host:port;https=...`
/// one by one (the HTTPS one first; a SOCKS one alone is not used).
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_server(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if !s.contains('=') {
        return Some(with_scheme(s));
    }
    let of = |proto: &str| {
        s.split(';')
            .filter_map(|p| p.split_once('='))
            .find(|(k, _)| k.trim().eq_ignore_ascii_case(proto))
            .map(|(_, v)| v.trim())
            .filter(|v| !v.is_empty())
    };
    of("https").or_else(|| of("http")).map(with_scheme)
}

/// `scutil --proxy`: the HTTPS proxy when it is on, else the HTTP one.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn scutil_proxy(text: &str) -> Option<String> {
    let value = |name: &str| {
        text.lines()
            .filter_map(|l| l.split_once(" : "))
            .find(|(k, _)| k.trim() == name)
            .map(|(_, v)| v.trim().to_string())
    };
    ["HTTPS", "HTTP"].iter().find_map(|p| {
        if value(&format!("{p}Enable")).as_deref() != Some("1") {
            return None;
        }
        let host = value(&format!("{p}Proxy")).filter(|h| !h.is_empty())?;
        Some(match value(&format!("{p}Port")) {
            Some(port) => format!("http://{host}:{port}"),
            None => format!("http://{host}"),
        })
    })
}

/// Hide proxy credentials, including ambiguous malformed addresses, before logging.
pub fn redact(proxy: &str) -> String {
    let proxy = proxy.trim();
    let (scheme, rest) = match proxy.split_once("://") {
        Some((scheme, rest))
            if !scheme.is_empty()
                && scheme
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"+.-".contains(&b)) =>
        {
            (Some(scheme), rest)
        }
        _ => (None, proxy),
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    // A separator before the last @ may belong to an unescaped password, or to a
    // path after the host. Neither interpretation is safe to print as an address.
    if rest.rfind('@').is_some_and(|at| at >= end) {
        return scheme.map_or_else(|| "***".into(), |s| format!("{s}://***"));
    }
    let rest = &rest[..end];
    let (login, host) = match rest.rsplit_once('@') {
        Some((_, host)) => (true, host),
        None => (false, rest),
    };
    let login = if login { "***@" } else { "" };
    match scheme {
        Some(scheme) => format!("{scheme}://{login}{host}"),
        None => format!("{login}{host}"),
    }
}

/// A proxy written without its protocol is an HTTP one.
fn with_scheme(s: &str) -> String {
    if s.contains("://") {
        s.to_string()
    } else {
        format!("http://{s}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_proxy_server_forms() {
        assert_eq!(
            windows_server("127.0.0.1:7890").as_deref(),
            Some("http://127.0.0.1:7890")
        );
        assert_eq!(
            windows_server("http=10.0.0.1:80;https=10.0.0.2:443;ftp=x:21").as_deref(),
            Some("http://10.0.0.2:443")
        );
        assert_eq!(
            windows_server("http=10.0.0.1:80").as_deref(),
            Some("http://10.0.0.1:80")
        );
        assert_eq!(windows_server("socks=127.0.0.1:1080"), None);
        assert_eq!(windows_server(""), None);
    }

    #[test]
    fn redact_hides_the_login_and_keeps_the_address() {
        assert_eq!(redact("http://127.0.0.1:7890"), "http://127.0.0.1:7890");
        assert_eq!(
            redact("http://user:secret@proxy.example:3128"),
            "http://***@proxy.example:3128"
        );
        assert_eq!(
            redact("socks5://user@proxy.example:1080/"),
            "socks5://***@proxy.example:1080"
        );
        assert_eq!(
            redact("http://user:p%40ss%2Fword@proxy.example:3128"),
            "http://***@proxy.example:3128"
        );
        assert_eq!(
            redact("http://user:p@ss/wo:rd#x@proxy.example:3128"),
            "http://***"
        );
        assert_eq!(
            redact("user:secret@proxy.example:3128"),
            "***@proxy.example:3128"
        );
        assert_eq!(
            redact("  http://u:s@proxy.example:3128/path?token=abc#f "),
            "http://***@proxy.example:3128"
        );
        assert_eq!(redact("proxy.example:3128"), "proxy.example:3128");
        assert_eq!(redact("http://u:s@[::1]:8080"), "http://***@[::1]:8080");
    }

    #[test]
    fn redact_of_malformed_addresses_never_shows_the_password() {
        for bad in [
            "",
            "://",
            "http://",
            "http://:secret@",
            "http://user:secret@",
            "@",
            "user:secret@",
            "http://user:secret@/x",
        ] {
            let shown = redact(bad);
            assert!(!shown.contains("secret"), "{bad:?} -> {shown:?}");
        }
        assert_eq!(redact(""), "");
    }

    #[test]
    fn a_scheme_separator_inside_a_password_never_becomes_a_logged_scheme() {
        for address in [
            "user:secret://word@proxy:3128",
            "http://user:secret://word@proxy:3128",
            "u:secret+.-://w@proxy",
        ] {
            let shown = redact(address);
            assert!(
                !shown.contains("secret") && !shown.contains("word") && !shown.contains("user"),
                "{shown}"
            );
        }
        assert_eq!(redact("user:p://w@proxy:3128"), "***");
    }

    #[test]
    fn an_at_sign_after_a_path_separator_does_not_become_a_host() {
        for tail in ["/a@b", "?a@b", "#a@b"] {
            assert_eq!(redact(&format!("http://proxy:3128{tail}")), "http://***");
            assert_eq!(
                redact(&format!("http://u:s@proxy:3128{tail}")),
                "http://***"
            );
        }
        assert_eq!(redact("http://proxy:3128/a?b#c"), "http://proxy:3128");
    }

    #[test]
    fn macos_scutil_proxy() {
        let on = "<dictionary> {\n  HTTPEnable : 1\n  HTTPPort : 8080\n  HTTPProxy : proxy.local\n  HTTPSEnable : 1\n  HTTPSPort : 7890\n  HTTPSProxy : 127.0.0.1\n}\n";
        assert_eq!(scutil_proxy(on).as_deref(), Some("http://127.0.0.1:7890"));
        let http_only = "<dictionary> {\n  HTTPEnable : 1\n  HTTPPort : 8080\n  HTTPProxy : proxy.local\n  HTTPSEnable : 0\n}\n";
        assert_eq!(
            scutil_proxy(http_only).as_deref(),
            Some("http://proxy.local:8080")
        );
        assert_eq!(scutil_proxy("<dictionary> {\n  HTTPEnable : 0\n}\n"), None);
    }
}
