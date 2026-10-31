use std::collections::HashMap;
use std::fmt::Display;
use omsi_cfg::template::Index;
use std::sync::{Arc, Mutex, OnceLock};

pub static TABLES: &str = include_str!("../../omsi-app/locales/app.yml");

static PINNED: OnceLock<&'static str> = OnceLock::new();

fn unquote(s: &str) -> Option<String> {
    let inner = s.trim().strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                out.extend(u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32));
            }
            Some(o @ ('"' | '\\' | '/')) => out.push(o),
            Some(o) => out.extend(['\\', o]),
            None => out.push('\\'),
        }
    }
    Some(out)
}

pub fn table(text: &str, lang: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut key: Option<String> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("  ") {
            if let (Some(k), Some((l, value))) = (&key, rest.split_once(": ")) {
                if l.trim() == lang {
                    out.extend(unquote(value).map(|v| (k.clone(), v)));
                }
            }
        } else if let Some(k) = line.strip_suffix(':') {
            key = unquote(k);
        } else if !line.trim().is_empty() && !line.trim_start().starts_with('#') {
            key = None;
        }
    }
    out
}

pub fn pin_language(code: &str) {
    let _ = PINNED.set(crate::language_iso(code));
}

fn language() -> &'static str {
    static SEEN: Mutex<Option<(Option<std::time::SystemTime>, &'static str)>> = Mutex::new(None);
    if let Some(l) = PINNED.get() {
        return l;
    }
    if cfg!(test) {
        return "";
    }
    let at = std::fs::metadata(crate::data_dir().join("settings.cfg")).and_then(|m| m.modified()).ok();
    let mut g = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    match *g {
        Some((t, l)) if t == at => l,
        _ => {
            let l = crate::language_iso(crate::content_language());
            *g = Some((at, l));
            l
        }
    }
}

pub fn tr(text: &str) -> String {
    type Words = (HashMap<String, String>, Index);
    static LOADED: Mutex<Option<(&'static str, Arc<Words>)>> = Mutex::new(None);
    let lang = language();
    if lang.is_empty() {
        return text.to_string();
    }
    let words = {
        let mut g = LOADED.lock().unwrap_or_else(|e| e.into_inner());
        match g.as_ref() {
            Some((l, t)) if *l == lang => t.clone(),
            _ => {
                let t = table(TABLES, lang);
                let keys: Vec<String> = t.keys().filter(|k| k.contains('{')).cloned().collect();
                let t = Arc::new((t, Index::new(keys)));
                *g = Some((lang, t.clone()));
                t
            }
        }
    };
    let (t, index) = &*words;
    t.get(text).cloned().or_else(|| index.templated(text, |k| t.get(k).cloned())).unwrap_or_else(|| text.to_string())
}

pub fn tr_said(text: &str) -> String {
    for key in ["the host turned us away: {reason}", "the host sent you away: {reason}", "Host's mods could not be fetched: {e}"] {
        let (head, hole) = key.rsplit_once('{').unwrap_or((key, ""));
        if let Some(value) = text.strip_prefix(head) {
            return tr(key).replace(&format!("{{{hole}"), &tr(value));
        }
    }
    tr(text)
}

pub fn fill(text: String, vars: &[(&str, &dyn Display)]) -> String {
    vars.iter().fold(text, |s, (k, v)| s.replace(&format!("{{{k}}}"), &v.to_string()))
}

pub fn trf(text: &str, vars: &[(&str, &dyn Display)]) -> String {
    fill(tr(text), vars)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_language_is_read_from_the_tables() {
        let yml = "_version: 2\n\n\"Mon\":\n  fr: \"lun.\"\n  de: \"Mo\"\n\"a \\\"b\\\"\":\n  fr: \"c\\n\"\n";
        let fr = super::table(yml, "fr");
        assert_eq!(fr.get("Mon").map(String::as_str), Some("lun."));
        assert_eq!(fr.get("a \"b\"").map(String::as_str), Some("c\n"));
        assert_eq!(super::table(yml, "de").len(), 1);
        assert!(super::table(super::TABLES, "fr").contains_key("Mon"));
    }

    /// The built-up snow switch's label and help (the Graphics page, the game's Settings)
    /// are read in every language, each once in the tables.
    #[test]
    fn the_built_up_snow_switch_is_read_in_every_language() {
        let texts = [
            "Built-up snow on roads and roofs",
            "While it snows, snow builds up on the roads and on the vehicles' roofs, with ruts and tyre tracks, and thaws again; off: the weather's own snow on road, as in OMSI 2",
        ];
        let langs = "de fr ru uk be kk pl cs hu es pt pt-pt it nl tr ja zh-tw ko th vi id ms tl zh hi ca sv sr ka";
        for lang in langs.split(' ') {
            let table = super::table(super::TABLES, lang);
            for text in texts {
                let got = table.get(text).map(String::as_str).unwrap_or("");
                assert!(!got.is_empty() && got != text, "{lang}: {text}");
            }
        }
        // (by lines: a Windows checkout has the tables with CRLF endings)
        for text in texts {
            let key = format!("\"{text}\":");
            assert_eq!(super::TABLES.lines().filter(|l| *l == key).count(), 1, "{text}");
        }
    }
}
