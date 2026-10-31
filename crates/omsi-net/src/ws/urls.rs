//! Gateway URL normalization and status endpoint discovery.

/// A WebSocket address for what a player typed. HTTP addresses append `/ws` to
/// their base path; WebSocket addresses preserve explicit paths and default to `/ws`.
/// None for anything else (an address or a code).
pub fn ws_url(target: &str) -> Option<String> {
    let t = target.trim();
    let bare = !t.contains("://");
    // Backslashes are URL path separators in some parsers, not part of a tunnel host.
    if bare && t.contains('\\') {
        return None;
    }
    let input = if bare { format!("https://{t}") } else { t.to_string() };
    let parsed = ureq::get(&input).request_url().ok()?;
    let mut url = parsed.as_url().clone();
    if bare {
        let host = url.host_str()?;
        let name = host.strip_suffix(".trycloudflare.com")?;
        if !name.split('.').all(|label| !label.is_empty() && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')) {
            return None;
        }
    }
    let (scheme, append) = match url.scheme() {
        "https" => ("wss", true),
        "http" => ("ws", true),
        "wss" => ("wss", false),
        "ws" => ("ws", false),
        _ => return None,
    };
    url.set_scheme(scheme).ok()?;
    let path = url.path().trim_end_matches('/');
    if append && !path.ends_with("/ws") {
        url.set_path(&format!("{path}/ws"));
    } else if path.is_empty() {
        url.set_path("/ws");
    } else if append {
        url.set_path(&path.to_string());
    }
    // Fragments are local browser state and never part of a gateway request.
    url.set_fragment(None);
    Some(url.into())
}

/// The HTTP(S) base of a server address (for `/status` and `/icon.png`).
pub fn http_base(target: &str) -> Option<String> {
    let parsed = ureq::get(&ws_url(target)?).request_url().ok()?;
    let mut url = parsed.as_url().clone();
    let scheme = if url.scheme() == "wss" { "https" } else { "http" };
    url.set_scheme(scheme).ok()?;
    let path = url.path().strip_suffix("/ws").unwrap_or(url.path()).to_string();
    url.set_path(&path);
    url.set_query(None);
    url.set_fragment(None);
    Some(url.as_str().trim_end_matches('/').to_string())
}

/// The web addresses a server may answer at, for whatever address a player gave: a web
/// address as it is; a bare `host` or `host:port` (an IP, a domain) at the
/// port ten above it (the game's port was given: a server's web port is its game port + 10
/// unless `web_port` says otherwise), at the servers' default 27025, and over https.
/// An explicitly typed port is tried first, before port + 10 and HTTPS.
pub fn web_bases(target: &str) -> Vec<String> {
    if let Some(b) = http_base(target) {
        return vec![b];
    }
    let t = target.trim().trim_end_matches('/');
    let t = t.strip_suffix("/ws").unwrap_or(t);
    if t.is_empty() || t.contains(char::is_whitespace) {
        return Vec::new();
    }
    // (an IPv6 address is written in brackets with a port: [::1]:27025)
    let (host, port) = match t.rsplit_once(':') {
        Some((h, p)) if !h.is_empty() && (!h.contains(':') || h.ends_with(']')) => match p.parse::<u16>() {
            Ok(p) => (h.to_string(), Some(p)),
            Err(_) => (t.to_string(), None),
        },
        _ if t.contains(':') && !t.starts_with('[') => (format!("[{t}]"), None),
        _ => (t.to_string(), None),
    };
    let mut out = Vec::new();
    let mut add = |u: String| {
        if !out.contains(&u) {
            out.push(u);
        }
    };
    if let Some(p) = port {
        add(format!("http://{host}:{p}"));
        if p <= 65525 {
            add(format!("http://{host}:{}", p + 10));
        }
        add(format!("https://{host}"));
    }
    add(format!("http://{host}:27025"));
    if port.is_none() {
        add(format!("https://{host}"));
        add(format!("http://{host}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        for (http, ws) in [("https", "wss"), ("http", "ws")] {
            for suffix in ["", "/", "/ws"] {
                let target = format!("{ws}://example.org{suffix}");
                assert_eq!(ws_url(&target), Some(format!("{ws}://example.org/ws")));
                assert_eq!(http_base(&target), Some(format!("{http}://example.org")));
            }
            assert_eq!(ws_url(&format!("{ws}://example.org?token=abc#local")), Some(format!("{ws}://example.org/ws?token=abc")));
            assert_eq!(ws_url(&format!("{ws}://example.org/custom/?token=abc")), Some(format!("{ws}://example.org/custom/?token=abc")));
            assert_eq!(ws_url(&format!("{http}://example.org/prefix/?token=abc")), Some(format!("{ws}://example.org/prefix/ws?token=abc")));
            assert_eq!(http_base(&format!("{ws}://example.org/prefix/ws?token=abc#local")), Some(format!("{http}://example.org/prefix")));
        }
        assert_eq!(ws_url("https://abc.trycloudflare.com").as_deref(), Some("wss://abc.trycloudflare.com/ws"));
        assert_eq!(ws_url("abc.trycloudflare.com").as_deref(), Some("wss://abc.trycloudflare.com/ws"));
        assert_eq!(ws_url("http://10.0.0.2:27025/").as_deref(), Some("ws://10.0.0.2:27025/ws"));
        assert_eq!(ws_url("192.168.1.4:27015"), None);
        assert_eq!(ws_url("abc.trycloudflare.com/ws").as_deref(), Some("wss://abc.trycloudflare.com/ws"));
        assert_eq!(ws_url("abc.trycloudflare.com:443/ws").as_deref(), Some("wss://abc.trycloudflare.com/ws"));
        assert_eq!(ws_url("ABC.TryCloudflare.com/ws").as_deref(), Some("wss://abc.trycloudflare.com/ws"));
        assert_eq!(ws_url("me@abc.trycloudflare.com/ws").as_deref(), Some("wss://me@abc.trycloudflare.com/ws"));
        for bogus in ["play.example.org/abc.trycloudflare.com/", "play.example.org?abc.trycloudflare.com", "play.example.org#abc.trycloudflare.com", "play.example.org:1/abc.trycloudflare.com/", "evil.example\\.trycloudflare.com/ws", "abc.trycloudflare.com@evil.example/ws", "abc.trycloudflare.com:1@evil.example/ws", "evil.example%5C.trycloudflare.com/ws", "evil example.trycloudflare.com", ".trycloudflare.com", "a..trycloudflare.com", "abc.trycloudflare.com:x/ws"] {
            assert_eq!(ws_url(bogus), None);
        }
        assert_eq!(http_base("https://abc.trycloudflare.com").as_deref(), Some("https://abc.trycloudflare.com"));
        assert_eq!(web_bases("192.168.1.4:27015"), ["http://192.168.1.4:27015", "http://192.168.1.4:27025", "https://192.168.1.4"]);
        assert_eq!(web_bases("play.example.org"), ["http://play.example.org:27025", "https://play.example.org", "http://play.example.org"]);
        assert_eq!(web_bases("play.example.org:47015"), ["http://play.example.org:47015", "http://play.example.org:47025", "https://play.example.org", "http://play.example.org:27025"]);
        assert_eq!(web_bases("play.example.org:65535"), ["http://play.example.org:65535", "https://play.example.org", "http://play.example.org:27025"]);
        assert_eq!(web_bases("http://play.example.org:47015"), ["http://play.example.org:47015"]);
        assert_eq!(web_bases("https://play.example.org"), ["https://play.example.org"]);
        assert_eq!(web_bases("::1")[0], "http://[::1]:27025");
        assert_eq!(web_bases("[::1]:27025"), ["http://[::1]:27025", "http://[::1]:27035", "https://[::1]"]);
    }
}
