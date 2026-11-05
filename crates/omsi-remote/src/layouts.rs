use serde_json::Value;
use std::path::{Path, PathBuf};

pub(crate) const MAX_LAYOUT: usize = 256 * 1024;
const ALL: &str = "_all";
const MOD_FILE: &str = "remote.json";

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Scope {
    Vehicle,
    All,
    Mod,
}

impl Scope {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Scope::Vehicle => "vehicle",
            Scope::All => "all",
            Scope::Mod => "mod",
        }
    }

    pub(crate) fn parse(s: &str) -> Option<Scope> {
        match s {
            "vehicle" => Some(Scope::Vehicle),
            "all" => Some(Scope::All),
            _ => None,
        }
    }
}

pub(crate) struct Store {
    dir: PathBuf,
}

pub(crate) fn file_name(key: &str) -> String {
    let mut s: String = key.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '.' { c } else { '_' }).collect();
    s = s.trim_matches('.').to_string();
    if s.is_empty() || s == ALL {
        s = "vehicle".into();
    }
    s.truncate(120);
    format!("{s}.json")
}

const MAX_GRID: i64 = 16;
const MAX_PAGES: usize = 12;
const MAX_WIDGETS: usize = 96;
const MAX_TEXT: usize = 120;

fn read(path: &Path) -> Option<Value> {
    if std::fs::metadata(path).ok()?.len() > MAX_LAYOUT as u64 {
        return None;
    }
    let v: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    clean(&v)
}

fn int(v: Option<&Value>, lo: i64, hi: i64) -> Option<i64> {
    v.and_then(Value::as_f64).filter(|x| x.is_finite()).map(|x| (x.round() as i64).clamp(lo, hi))
}

fn colour(v: &Value) -> bool {
    v.as_str().is_some_and(|s| matches!(s.len(), 4 | 7 | 9) && s.starts_with('#') && s[1..].bytes().all(|c| c.is_ascii_hexdigit()))
}

fn text(v: &Value) -> bool {
    v.as_str().is_some_and(|s| s.chars().count() <= MAX_TEXT && !s.chars().any(char::is_control))
}

fn grid(v: Option<&Value>) -> Option<Value> {
    let a = v?.as_array()?;
    Some(Value::from(vec![int(a.first(), 1, MAX_GRID)?, int(a.get(1), 1, MAX_GRID)?]))
}

fn place(v: Option<&Value>) -> Option<Value> {
    let a = v?.as_array()?;
    let x = int(a.first(), 0, MAX_GRID - 1)?;
    let y = int(a.get(1), 0, MAX_GRID - 1)?;
    let w = int(a.get(2), 1, MAX_GRID - x)?;
    let h = int(a.get(3), 1, MAX_GRID - y)?;
    Some(Value::from(vec![x, y, w, h]))
}

fn clean_widget(w: &Value) -> Option<Value> {
    let src = w.as_object()?;
    let mut out = serde_json::Map::new();
    for (k, v) in src {
        let keep = match k.as_str() {
            "pos" => {
                let mut pos = serde_json::Map::new();
                for o in ["l", "p"] {
                    if let Some(p) = place(v.get(o)) {
                        pos.insert(o.into(), p);
                    }
                }
                out.insert(k.clone(), Value::Object(pos));
                continue;
            }
            "color" | "lampColor" | "pictoColor" => colour(v),
            "max" => v.as_f64().is_some_and(|x| x.is_finite() && x > 0.0 && x < 1e6),
            "dp" => v.as_u64().is_some_and(|x| x <= 6),
            "lamps" => v.as_array().is_some_and(|a| a.len() <= 32 && a.iter().all(text)),
            "do" => v.as_object().is_some_and(|d| d.values().all(|x| text(x) || x.as_f64().is_some_and(f64::is_finite))),
            "skin" | "real" => v.is_boolean(),
            _ => text(v),
        };
        if keep {
            out.insert(k.clone(), v.clone());
        }
    }
    out.get("kind").is_some().then(|| Value::Object(out))
}

pub(crate) fn clean(v: &Value) -> Option<Value> {
    if v.get("v").and_then(Value::as_u64) != Some(1) {
        return None;
    }
    let pages: Vec<Value> = v
        .get("pages")?
        .as_array()?
        .iter()
        .take(MAX_PAGES)
        .filter_map(|p| {
            let p = p.as_object()?;
            let widgets: Vec<Value> = p.get("widgets")?.as_array()?.iter().take(MAX_WIDGETS).filter_map(clean_widget).collect();
            let mut g = serde_json::Map::new();
            for o in ["l", "p"] {
                if let Some(x) = grid(p.get("grid").and_then(|g| g.get(o))) {
                    g.insert(o.into(), x);
                }
            }
            let name = p.get("name").filter(|n| text(n)).cloned().unwrap_or_else(|| Value::from(""));
            Some(serde_json::json!({ "name": name, "grid": g, "widgets": widgets }))
        })
        .collect();
    let mut out = serde_json::json!({ "v": 1, "pages": pages });
    if let Some(t) = v.get("theme").filter(|t| text(t)) {
        out["theme"] = t.clone();
    }
    if let Some(a) = v.get("accent").filter(|a| colour(a)) {
        out["accent"] = a.clone();
    }
    if let Some(s) = v.get("skins").filter(|s| s.is_boolean()) {
        out["skins"] = s.clone();
    }
    Some(out)
}

impl Store {
    pub(crate) fn new(dir: PathBuf) -> Store {
        Store { dir }
    }

    fn path(&self, key: &str, scope: Scope) -> PathBuf {
        match scope {
            Scope::All => self.dir.join(format!("{ALL}.json")),
            _ => self.dir.join(file_name(key)),
        }
    }

    pub(crate) fn resolve(&self, key: &str, folder: &Path) -> Option<(Scope, Value)> {
        read(&self.path(key, Scope::Vehicle))
            .map(|v| (Scope::Vehicle, v))
            .or_else(|| read(&folder.join(MOD_FILE)).map(|v| (Scope::Mod, v)))
            .or_else(|| read(&self.path(key, Scope::All)).map(|v| (Scope::All, v)))
    }

    pub(crate) fn has(&self, key: &str, scope: Scope) -> bool {
        self.path(key, scope).is_file()
    }

    pub(crate) fn save(&self, key: &str, scope: Scope, layout: &Value) -> bool {
        let Some(layout) = clean(layout) else { return false };
        let Ok(text) = serde_json::to_string_pretty(&layout) else { return false };
        if text.len() > MAX_LAYOUT {
            return false;
        }
        let _ = std::fs::create_dir_all(&self.dir);
        let path = self.path(key, scope);
        static WRITES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let tmp = path.with_extension(format!("{}.tmp", WRITES.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
        match std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, &path)) {
            Ok(()) => true,
            Err(e) => {
                log::warn!("remote cockpit: cannot write {}: {e}", path.display());
                false
            }
        }
    }

    pub(crate) fn reset(&self, key: &str, scope: Scope) {
        let _ = std::fs::remove_file(self.path(key, scope));
    }
}
