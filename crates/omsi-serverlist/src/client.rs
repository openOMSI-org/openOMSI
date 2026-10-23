//! The launcher's and the game's view of the public list: the service's answer read and
//! checked entry by entry. The service is not believed: every text is cleaned and moderated
//! here (`moderation::show_listing`), an icon is shown only if this client cleared it, and
//! an entry that is no joinable address is dropped.

use crate::moderation::{show_listing, IconVerdict, Moderator, Shown, MAX_ICON_BYTES};
use serde_json::Value;

/// The most servers one list shows.
pub const MAX_ENTRIES: usize = 200;

/// A server of the list, ready to show.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: String,
    /// Where to join it: `https://….trycloudflare.com` or `http://host:port` (the web port).
    pub address: String,
    pub name: String,
    pub motd: String,
    pub map: String,
    pub players: u32,
    pub max_players: u32,
    pub version: String,
    /// A name or message was covered.
    pub covered: bool,
    /// The icon (a PNG) this client cleared; None: show the default one.
    pub icon: Option<Vec<u8>>,
}

/// An address the list may carry: `https://host[/…]`, or `http://host:port` with a port;
/// no credentials, no spaces.
pub fn address_ok(a: &str) -> bool {
    let rest = match a.strip_prefix("https://") {
        Some(r) => r,
        None => match a.strip_prefix("http://") {
            Some(r) if r.rsplit_once(':').is_some_and(|(_, p)| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())) => r,
            _ => return false,
        },
    };
    let host = rest.split('/').next().unwrap_or("");
    !host.is_empty() && host.len() <= 80 && !host.contains('@') && a.len() <= 200 && a.bytes().all(|b| b.is_ascii_graphic()) && host.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':' | b'[' | b']'))
}

fn text(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or("").to_string()
}

fn count(v: &Value, k: &str) -> u32 {
    v.get(k).and_then(Value::as_u64).unwrap_or(0).min(10_000) as u32
}

/// The entries of the service's answer (`{"servers":[…]}`), moderated. `icon` fetches the PNG
/// of an entry by its id (called only for entries that say they have one, a few at most).
pub fn entries_of(answer: &Value, m: &dyn Moderator, icon: &mut dyn FnMut(&str) -> Option<Vec<u8>>) -> Vec<Entry> {
    let Some(list) = answer.get("servers").and_then(Value::as_array) else { return Vec::new() };
    let mut out = Vec::new();
    for s in list.iter().take(MAX_ENTRIES) {
        let address = text(s, "address");
        let id = text(s, "id");
        if !address_ok(&address) || id.is_empty() || id.len() > 40 || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            continue;
        }
        let png = s.get("has_icon").and_then(Value::as_bool).unwrap_or(false).then(|| icon(&id)).flatten().filter(|p| p.len() <= MAX_ICON_BYTES);
        let Shown { name, motd, icon_ok, covered } = show_listing(m, &text(s, "name"), &text(s, "motd"), png.as_deref());
        out.push(Entry {
            id,
            address,
            name,
            motd,
            map: crate::moderation::clean(&text(s, "map"), 120),
            players: count(s, "players"),
            max_players: count(s, "max_players"),
            version: crate::moderation::clean(&text(s, "version"), 40),
            covered,
            icon: if icon_ok { png } else { None },
        });
    }
    out
}

/// Ask the service for the list (blocking; a few seconds at most) and moderate it.
pub fn fetch(service: &str, m: &dyn Moderator) -> Result<Vec<Entry>, String> {
    let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(8)).build();
    let body = agent.get(&format!("{}/v1/servers", service.trim_end_matches('/'))).call().map_err(|e| format!("the server list could not be reached ({e})"))?.into_string().map_err(|e| e.to_string())?;
    if body.len() > 1 << 20 {
        return Err("the server list answered with too much".into());
    }
    let v: Value = serde_json::from_str(&body).map_err(|_| "the server list answered with something else".to_string())?;
    let mut icons = 0;
    let mut get_icon = |id: &str| -> Option<Vec<u8>> {
        // (a client does not fetch hundreds of pictures: the first few entries have theirs)
        icons += 1;
        if icons > 60 {
            return None;
        }
        let r = agent.get(&format!("{}/v1/servers/{id}/icon.png", service.trim_end_matches('/'))).call().ok()?;
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut std::io::Read::take(r.into_reader(), MAX_ICON_BYTES as u64 + 1), &mut buf).ok()?;
        Some(buf)
    };
    Ok(entries_of(&v, m, &mut get_icon))
}

/// Whether an entry's icon was cleared (for the UI's placeholder).
pub fn has_icon(e: &Entry) -> bool {
    e.icon.is_some()
}

/// The verdict a client reaches on an icon, for logs.
pub fn icon_state(m: &dyn Moderator, png: &[u8]) -> &'static str {
    match m.check_icon(png) {
        IconVerdict::Cleared => "cleared",
        IconVerdict::Flagged(_) => "flagged",
        IconVerdict::Unchecked => "unchecked",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::moderation::{BuiltinModerator, Verdict};
    use serde_json::json;

    struct Double;
    impl Moderator for Double {
        fn check_text(&self, t: &str) -> Verdict {
            if t.contains("FLAGGED") { Verdict::Flagged("test") } else { Verdict::Clean }
        }
        fn check_icon(&self, png: &[u8]) -> IconVerdict {
            if png == b"good" { IconVerdict::Cleared } else if png == b"bad" { IconVerdict::Flagged("test") } else { IconVerdict::Unchecked }
        }
    }

    #[test]
    fn addresses_of_the_list() {
        for ok in ["https://abc-def.trycloudflare.com", "https://play.example.org/", "http://203.0.113.9:27025", "http://[2001:db8::1]:27025"] {
            assert!(address_ok(ok), "{ok}");
        }
        for bad in ["", "http://203.0.113.9", "ftp://x.example", "https://user@x.example", "https://x y", "javascript:alert(1)", "https://", "http://host:", "https://x.example/\u{7f}"] {
            assert!(!address_ok(bad), "{bad}");
        }
    }

    /// The service is not believed: bad addresses are dropped, flagged text is covered, an
    /// icon is shown only when this client cleared it, numbers are bounded.
    #[test]
    fn the_service_answer_is_checked_entry_by_entry() {
        let answer = json!({"servers": [
            {"id": "a1", "address": "https://one.trycloudflare.com", "name": "Fine", "motd": "Hi", "map": "Berlin", "players": 3, "max_players": 16, "version": "0.3.40", "has_icon": true},
            {"id": "a2", "address": "https://two.trycloudflare.com", "name": "FLAGGED name", "motd": "ok", "has_icon": true},
            {"id": "a3", "address": "javascript:alert(1)", "name": "Evil"},
            {"id": "a4", "address": "http://203.0.113.9:27025", "name": "Third", "players": 99999999, "has_icon": true},
            {"id": "a 5", "address": "https://five.example", "name": "bad id"},
            "not an object"
        ]});
        let mut asked = Vec::new();
        let mut icon = |id: &str| {
            asked.push(id.to_string());
            Some(match id { "a1" => b"good".to_vec(), "a2" => b"good".to_vec(), _ => b"bad".to_vec() })
        };
        let list = entries_of(&answer, &Double, &mut icon);
        assert_eq!(list.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), vec!["a1", "a2", "a4"]);
        assert_eq!(list[0].icon.as_deref(), Some(&b"good"[..]));
        assert!(list[1].covered && list[1].name == "[hidden]");
        assert_eq!(list[1].icon.as_deref(), Some(&b"good"[..]), "the icon is judged on its own");
        assert_eq!(list[2].icon, None);
        assert_eq!(list[2].players, 10_000);
        // nothing at all: an empty list, no panic
        assert!(entries_of(&json!({}), &Double, &mut icon).is_empty());
        assert!(entries_of(&json!({"servers": 5}), &Double, &mut icon).is_empty());
        assert_eq!(asked, vec!["a1", "a2", "a4"]);
    }

    #[test]
    fn with_the_builtin_moderator_no_custom_icon_is_shown() {
        let answer = json!({"servers": [{"id": "a1", "address": "https://one.example.org", "name": "Fine", "has_icon": true}]});
        let list = entries_of(&answer, &BuiltinModerator, &mut |_| Some(vec![0x89, b'P', b'N', b'G']));
        assert_eq!(list.len(), 1);
        assert!(!has_icon(&list[0]));
    }
}
