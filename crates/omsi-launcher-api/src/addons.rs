//! The Add-ons page's list: projects others make for openOMSI, each a link to its own page
//! (`launcher/ui/addons/addons.json`; `docs/ADDONS.md` says how one gets in). The launcher
//! downloads, installs and runs nothing of them - the page opens the homepage in the browser.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

const LIST: &str = include_str!("../../../launcher/ui/addons/addons.json");

/// The list's format; a newer one is not read (an older launcher shows no add-ons then).
const VERSION: u32 = 1;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct List {
    version: u32,
    addons: Vec<Addon>,
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
    /// A picture in `launcher/ui/addons/` (the page shows no picture from the web).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    icon: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
}

/// Whether a homepage is a plain https address of a named website, as the browser reads it
/// (the URL standard, as the page's `new URL` does): no user name or password before the host
/// (`https://example.org@localhost/` goes to localhost), no port, no IP address or localhost,
/// and none of the characters other programs read in other ways (a backslash, quotes, `<>`):
/// the system opens the address, not only a browser that follows the standard.
fn homepage_ok(text: &str) -> bool {
    let odd = |c: char| c.is_whitespace() || c.is_control() || matches!(c, '\\' | '"' | '\'' | '<' | '>' | '^' | '`' | '{' | '}' | '|');
    if !text.starts_with("https://") || text.len() > 200 || text.chars().any(odd) {
        return false;
    }
    let Ok(u) = url::Url::parse(text) else { return false };
    let named = matches!(u.host(), Some(url::Host::Domain(d)) if d.contains('.') && !d.ends_with('.') && d != "localhost" && !d.ends_with(".localhost"));
    u.scheme() == "https" && u.username().is_empty() && u.password().is_none() && u.port().is_none() && named
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
        if !(plain(icon.trim_end_matches(".svg").trim_end_matches(".png"))
            && (icon.ends_with(".svg") || icon.ends_with(".png")))
        {
            return Some("the icon is not a .svg or .png file of launcher/ui/addons".into());
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
    if list.version != VERSION {
        return Err(format!(
            "the add-on list is of version {}, this launcher reads {VERSION}",
            list.version
        ));
    }
    let mut ids = HashSet::new();
    Ok(list
        .addons
        .into_iter()
        .filter(|a| problem(a).is_none() && ids.insert(a.id.clone()))
        .collect())
}

/// The add-ons the page lists, in the list's order.
pub fn list() -> Result<Value, String> {
    serde_json::to_value(read(LIST)?).map_err(|e| e.to_string())
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

    /// Every entry of the list is shown: none has a problem, and no id is written twice.
    #[test]
    fn the_list_is_valid() {
        let list: List = serde_json::from_str(LIST).unwrap();
        assert_eq!(list.version, VERSION);
        let mut ids = HashSet::new();
        for a in &list.addons {
            assert_eq!(problem(a), None, "{}", a.id);
            assert!(ids.insert(&a.id), "{} is in the list twice", a.id);
            if let Some(icon) = &a.icon {
                let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../launcher/ui/addons")
                    .join(icon);
                let size = std::fs::metadata(&file)
                    .map(|m| m.len())
                    .unwrap_or_else(|_| panic!("{}: no {}", a.id, file.display()));
                assert!(
                    size <= 10_000,
                    "{}: the icon is {size} bytes, at most 10 KB",
                    a.id
                );
            }
        }
        assert_eq!(read(LIST).unwrap().len(), list.addons.len());
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
        for url in ["https://example.org", "https://example.org/an-addon?x=1#top", "https://Example.ORG/", "https://example.org/team@example/"] {
            assert!(homepage_ok(url), "{url}");
        }
    }

    #[test]
    fn long_texts_and_odd_icons_are_refused() {
        assert!(problem(&Addon {
            description: "x".repeat(201),
            ..addon()
        })
        .is_some());
        assert!(problem(&Addon {
            name: " ".into(),
            ..addon()
        })
        .is_some());
        assert!(problem(&Addon {
            id: "An Add-on".into(),
            ..addon()
        })
        .is_some());
        assert!(problem(&Addon {
            translations: BTreeMap::from([("de".into(), "x".repeat(201))]),
            ..addon()
        })
        .is_some());
        for icon in [
            "../logo.svg",
            "https://example.org/icon.png",
            "icon.gif",
            ".svg",
        ] {
            assert!(
                problem(&Addon {
                    icon: Some(icon.into()),
                    ..addon()
                })
                .is_some(),
                "{icon}"
            );
        }
        assert_eq!(
            problem(&Addon {
                icon: Some("an-addon.svg".into()),
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
}
