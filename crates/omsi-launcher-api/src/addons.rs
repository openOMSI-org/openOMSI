//! The Add-ons page's list: projects others make for openOMSI, each a link to its own page.
//! The list is `addons.json` of [openOMSI-org/add-ons-list](https://github.com/openOMSI-org/add-ons-list),
//! read from the web each time the page opens, so an entry merged or removed there shows or
//! goes for everyone without a release (`docs/ADDONS.md` says how one gets in). Without a
//! connection the page says the list could not be loaded. The launcher downloads, installs
//! and runs nothing of an add-on - the page opens the homepage in the browser.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Digest;
use std::collections::{BTreeMap, HashSet};

pub const LIST_URL: &str =
    "https://raw.githubusercontent.com/openOMSI-org/add-ons-list/main/addons.json";
/// Where an entry's icon is: `{FILES_URL}<id>/<file>`.
const FILES_URL: &str = "https://raw.githubusercontent.com/openOMSI-org/add-ons-list/main/addons/";

/// The list's format; a newer one is not read (the page then says to update openOMSI).
const VERSION: u32 = 1;
const MAX_LIST: u64 = 1 << 20;
const MAX_ICON: u64 = 10_000;

#[derive(Deserialize)]
struct List {
    version: u32,
    /// (each read on its own: an entry that does not read is left out, the rest still shows)
    addons: Vec<Value>,
}

/// The icon beside an entry's `addon.json`, as `tools/build_list.py` wrote it into the list.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
struct Icon {
    file: String,
    size: u64,
    sha256: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Addon {
    id: String,
    name: String,
    author: String,
    /// In English; `translations` holds it in other languages (by the code of `app.yml`).
    description: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    translations: BTreeMap<String, String>,
    homepage: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    icon: Option<Icon>,
    #[serde(default)]
    tags: Vec<String>,
}

/// Whether a homepage is a plain https address of a named website, as the browser reads it
/// (the URL standard, as the page's `new URL` does): no user name or password before the host
/// (`https://example.org@localhost/` goes to localhost), no port, no IP address or localhost,
/// and none of the characters other programs read in other ways (a backslash, quotes, `<>`):
/// the system opens the address, not only a browser that follows the standard.
fn homepage_ok(text: &str) -> bool {
    let odd = |c: char| {
        c.is_whitespace()
            || c.is_control()
            || matches!(
                c,
                '\\' | '"' | '\'' | '<' | '>' | '^' | '`' | '{' | '}' | '|'
            )
    };
    if !text.starts_with("https://") || text.len() > 200 || text.chars().any(odd) {
        return false;
    }
    let Ok(u) = url::Url::parse(text) else {
        return false;
    };
    let named = matches!(u.host(), Some(url::Host::Domain(d)) if d.contains('.') && !d.ends_with('.') && d != "localhost" && !d.ends_with(".localhost"));
    u.scheme() == "https"
        && u.username().is_empty()
        && u.password().is_none()
        && u.port().is_none()
        && named
}

/// What is wrong with an entry, if anything: it is then left out of the page.
fn problem(a: &Addon) -> Option<String> {
    let short = |text: &str, max: usize| !text.trim().is_empty() && text.chars().count() <= max;
    let plain = |text: &str| {
        !text.is_empty()
            && text
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    };
    if !plain(&a.id) || a.id.len() > 40 {
        return Some("the id is not lowercase letters, digits and - (at most 40)".into());
    }
    if !short(&a.name, 40) || !short(&a.author, 40) {
        return Some("the name and the author are 1 to 40 characters".into());
    }
    if !short(&a.description, 200) || !a.translations.values().all(|d| short(d, 200)) {
        return Some("a description is 1 to 200 characters".into());
    }
    if !a.translations.keys().all(|l| plain(l) && l.len() <= 8) {
        return Some("a translation's language is not a code of app.yml".into());
    }
    if !homepage_ok(&a.homepage) {
        return Some("the homepage is not a plain https address of a named website".into());
    }
    if let Some(icon) = &a.icon {
        let hex = icon.sha256.len() == 64
            && icon
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if !matches!(icon.file.as_str(), "icon.svg" | "icon.png") || icon.size > MAX_ICON || !hex {
            return Some("the icon is not an icon.svg or icon.png of at most 10 KB".into());
        }
    }
    if a.tags.len() > 5 || !a.tags.iter().all(|t| plain(t) && t.len() <= 20) {
        return Some("at most 5 tags, each lowercase and at most 20 characters".into());
    }
    None
}

fn read(text: &str) -> Result<Vec<Addon>, String> {
    let list: List =
        serde_json::from_str(text).map_err(|e| format!("the add-on list does not read: {e}"))?;
    if list.version > VERSION {
        return Err(
            "the add-on list is newer than this launcher: update openOMSI to see it".into(),
        );
    }
    let mut ids = HashSet::new();
    Ok(list
        .addons
        .into_iter()
        .filter_map(|v| serde_json::from_value::<Addon>(v).ok())
        .filter(|a| problem(a).is_none() && ids.insert(a.id.clone()))
        .collect())
}

/// The icon as a `data:` address the page shows, when the file is the one the list names
/// (else the card shows a globe). Nothing of it is loaded by the page from the web.
fn picture(id: &str, icon: &Icon, fetch: Fetch) -> Option<String> {
    let bytes = fetch(&format!("{FILES_URL}{id}/{}", icon.file), MAX_ICON).ok()?;
    let sum: String = sha2::Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if bytes.len() as u64 != icon.size || sum != icon.sha256 {
        return None;
    }
    let kind = if icon.file.ends_with(".svg") {
        "image/svg+xml"
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else {
        return None;
    };
    Some(format!("data:{kind};base64,{}", base64(&bytes)))
}

fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            out.push(if i <= c.len() {
                ABC[(n >> (18 - 6 * i) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    out
}

/// How the list and the icons are downloaded (a URL, the most bytes taken).
type Fetch<'a> = &'a (dyn Fn(&str, u64) -> anyhow::Result<Vec<u8>> + Sync);

/// The add-ons the page lists, in the list's order, each icon as `picture`.
fn load(fetch: Fetch) -> Result<Value, String> {
    // (the page puts "could not be loaded, check the internet connection" above this)
    let text = fetch(LIST_URL, MAX_LIST).map_err(|e| e.to_string())?;
    let list = read(&String::from_utf8_lossy(&text))?;
    let pictures: Vec<Option<String>> = std::thread::scope(|s| {
        let jobs: Vec<_> = list
            .iter()
            .map(|a| s.spawn(|| a.icon.as_ref().and_then(|i| picture(&a.id, i, fetch))))
            .collect();
        jobs.into_iter().map(|j| j.join().ok().flatten()).collect()
    });
    let out = list
        .into_iter()
        .zip(pictures)
        .map(|(a, pic)| {
            let mut v = serde_json::to_value(&a).unwrap_or(Value::Null);
            v["icon"] = serde_json::json!(pic);
            v
        })
        .collect();
    Ok(Value::Array(out))
}

pub fn list() -> Result<Value, String> {
    load(&|url, max| crate::plugin_catalog::fetch(url, max))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addon() -> Addon {
        serde_json::from_value(serde_json::json!({
            "id": "an-addon", "name": "An add-on", "author": "Someone",
            "description": "What it does.", "homepage": "https://example.org/an-addon",
        }))
        .unwrap()
    }

    #[test]
    fn only_https_homepages() {
        for url in [
            "http://example.org",
            "javascript:alert(1)",
            "file:///etc/passwd",
            "https://",
            "https:// example.org",
            "https://localhost/",
            "omsi://localhost/x",
        ] {
            let a = Addon {
                homepage: url.into(),
                ..addon()
            };
            assert!(problem(&a).is_some(), "{url}");
        }
        assert_eq!(problem(&addon()), None);
    }

    /// The host is the one the browser goes to: a user name or password before it, a port,
    /// an IP address or localhost under another spelling are refused (review of #2875).
    #[test]
    fn the_homepage_is_judged_by_its_parsed_host() {
        for url in [
            "https://example.com@localhost/",
            "https://example.com@localhost",
            "https://user:pass@example.org/",
            "https://:pass@example.org/",
            "https://example.org:8443/",
            "https://127.0.0.1/",
            "https://0x7f.0.0.1/",
            "https://[::1]/",
            "https://shop.localhost/",
            "https://LOCALHOST/",
            "https://example.org\\@localhost/",
            "https:example.org",
        ] {
            assert!(!homepage_ok(url), "{url}");
        }
        for url in [
            "https://example.org",
            "https://example.org/an-addon?x=1#top",
            "https://Example.ORG/",
            "https://example.org/team@example/",
        ] {
            assert!(homepage_ok(url), "{url}");
        }
    }

    #[test]
    fn long_texts_and_odd_icons_are_refused() {
        assert!(
            problem(&Addon {
                description: "x".repeat(201),
                ..addon()
            })
            .is_some()
        );
        assert!(
            problem(&Addon {
                name: " ".into(),
                ..addon()
            })
            .is_some()
        );
        assert!(
            problem(&Addon {
                id: "An Add-on".into(),
                ..addon()
            })
            .is_some()
        );
        assert!(
            problem(&Addon {
                translations: BTreeMap::from([("de".into(), "x".repeat(201))]),
                ..addon()
            })
            .is_some()
        );
        let icon = |file: &str, size: u64, sha256: &str| Icon {
            file: file.into(),
            size,
            sha256: sha256.into(),
        };
        let sum = "0".repeat(64);
        for bad in [
            icon("../logo.svg", 10, &sum),
            icon("https://example.org/icon.png", 10, &sum),
            icon("icon.gif", 10, &sum),
            icon("icon.svg", 10_001, &sum),
            icon("icon.svg", 10, "ABC"),
            icon("icon.svg", 10, &"A".repeat(64)),
        ] {
            assert!(
                problem(&Addon {
                    icon: Some(bad.clone()),
                    ..addon()
                })
                .is_some(),
                "{bad:?}"
            );
        }
        assert_eq!(
            problem(&Addon {
                icon: Some(icon("icon.svg", 10, &sum)),
                ..addon()
            }),
            None
        );
    }

    #[test]
    fn a_broken_entry_or_a_second_id_is_left_out() {
        let text = r#"{ "version": 1, "addons": [
            { "id": "one", "name": "One", "author": "A", "description": "D", "homepage": "https://one.example" },
            { "id": "two", "name": "Two", "author": "A", "description": "D", "homepage": "http://two.example" },
            { "id": "one", "name": "One again", "author": "B", "description": "D", "homepage": "https://other.example" }
        ] }"#;
        let list = read(text).unwrap();
        assert_eq!(
            list.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
            ["One"]
        );
        assert!(read(r#"{ "version": 2, "addons": [] }"#).is_err());
    }

    #[test]
    fn base64_is_the_standard_one() {
        for (raw, enc) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
        ] {
            assert_eq!(base64(raw.as_bytes()), enc);
        }
    }

    fn hex(b: &[u8]) -> String {
        sha2::Sha256::digest(b)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// The list comes from the web each time: what it says now is what the page shows, an
    /// icon only when it is the file the list names, and no connection is an error.
    #[test]
    fn the_list_and_its_icons_come_from_the_web() {
        let svg = b"<svg xmlns='http://www.w3.org/2000/svg'/>".to_vec();
        let list = serde_json::json!({ "version": 1, "addons": [
            { "id": "one", "name": "One", "author": "A", "description": "D", "homepage": "https://one.example",
              "icon": { "file": "icon.svg", "size": svg.len(), "sha256": hex(&svg) } },
            { "id": "two", "name": "Two", "author": "A", "description": "D", "homepage": "https://two.example",
              "icon": { "file": "icon.svg", "size": svg.len(), "sha256": "0".repeat(64) } },
            { "id": "three", "name": "Three", "author": "A", "description": "D", "homepage": "https://three.example",
              "tags": ["x"], "unknown": true }
        ] })
        .to_string();
        let fetch = |url: &str, max: u64| -> anyhow::Result<Vec<u8>> {
            let body = if url == LIST_URL {
                list.clone().into_bytes()
            } else if url.starts_with(FILES_URL) && url.ends_with("/icon.svg") {
                svg.clone()
            } else {
                anyhow::bail!("no such file")
            };
            assert!(body.len() as u64 <= max);
            Ok(body)
        };
        let got = load(&fetch).unwrap();
        let got = got.as_array().unwrap();
        assert_eq!(
            got.len(),
            2,
            "an entry with a field the list does not have is left out"
        );
        assert_eq!(got[0]["id"], "one");
        assert!(
            got[0]["icon"]
                .as_str()
                .unwrap()
                .starts_with("data:image/svg+xml;base64,")
        );
        assert_eq!(
            got[1]["icon"],
            Value::Null,
            "an icon that is not the named file is not shown"
        );

        let offline = |_: &str, _: u64| -> anyhow::Result<Vec<u8>> { anyhow::bail!("dns error") };
        assert!(load(&offline).unwrap_err().contains("dns error"));
    }
}
