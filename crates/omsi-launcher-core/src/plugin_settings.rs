//! The settings a Lua plugin declares, before the game has run it (#4292).
//!
//! The game writes `data/settings.schema.json` when a plugin calls `omsi.plugin.settings`.
//! A freshly installed plugin has not called it yet, so the launcher cannot show its settings
//! button. The declaration is a literal table in `main.lua` (the catalog's plugins too), so it
//! can be read without running the plugin: only the call's arguments are evaluated, in a Lua
//! state with no globals but a stub `omsi.plugin.settings`, and an instruction limit. Anything
//! else (a declaration built at runtime, a compiled `.oop`, a malformed table) leaves the
//! settings hidden until the game has run the plugin and written the real schema.

use mlua::{ChunkMode, HookTriggers, Lua, Result as LuaResult, Value as LuaValue, VmState};
use serde_json::{Map, Value, json};
use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;

/// How much of `main.lua` may hold a declaration, and how long its evaluation may take.
const MAX_EXPRESSION: usize = 32 * 1024;
const MAX_INSTRUCTIONS: u32 = 20_000;
const MAX_DEPTH: u8 = 8;

/// The schema the launcher's form and `save_settings_in` understand, or None when the source
/// has no literal declaration.
pub fn from_source(src: &str) -> Option<Value> {
    let expression = call_expression(src)?;
    let lua = Lua::new();
    // (no `os`, `io`, `require`, `load`...: the expression can only build values, and the
    // single global left is the stub it calls)
    let globals = lua.globals();
    let names: Vec<LuaValue> = globals
        .pairs::<LuaValue, LuaValue>()
        .filter_map(|p| p.ok().map(|(k, _)| k))
        .collect();
    for name in names {
        let _ = globals.raw_set(name, LuaValue::Nil);
    }
    let seen = Rc::new(RefCell::new(None));
    let capture = {
        let seen = seen.clone();
        lua.create_function(move |_, (spec, title): (LuaValue, Option<LuaValue>)| {
            let spec = to_json(&spec, 0)?;
            let title = match title {
                Some(LuaValue::String(s)) => Some(s.to_string_lossy().to_string()),
                _ => None,
            };
            *seen.borrow_mut() = Some((spec, title));
            Ok(LuaValue::Nil)
        })
        .ok()?
    };
    let plugin = lua.create_table().ok()?;
    plugin.set("settings", capture).ok()?;
    let omsi = lua.create_table().ok()?;
    omsi.set("plugin", plugin).ok()?;
    globals.set("omsi", omsi).ok()?;
    let used = Rc::new(Cell::new(0));
    {
        let used = used.clone();
        lua.set_hook(
            HookTriggers::new().every_nth_instruction(512),
            move |_, _| {
                used.set(used.get() + 512);
                (used.get() <= MAX_INSTRUCTIONS)
                    .then_some(VmState::Continue)
                    .ok_or_else(|| mlua::Error::runtime("plugin settings declaration too long"))
            },
        );
    }
    let chunk = format!("return {expression}");
    let _: LuaValue = lua.load(&chunk).set_mode(ChunkMode::Text).eval().ok()?;
    let (spec, title) = seen.borrow().clone()?;
    sanitize(spec, title)
}

/// The declaration of a folder plugin (`main.lua`) or of a loose `.lua` file.
pub fn from_file(path: &Path) -> Option<Value> {
    let file = if path.is_dir() {
        path.join("main.lua")
    } else {
        path.to_path_buf()
    };
    let text = std::fs::read_to_string(file).ok()?;
    from_source(&text)
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Ident,
    Dot,
    Open,
    Close,
    Other,
}

struct Token {
    kind: Kind,
    start: usize,
    end: usize,
}

fn long_bracket(bytes: &[u8], at: usize) -> Option<usize> {
    if bytes.get(at) != Some(&b'[') {
        return None;
    }
    let mut n = 0;
    while bytes.get(at + 1 + n) == Some(&b'=') {
        n += 1;
    }
    (bytes.get(at + 1 + n) == Some(&b'[')).then_some(n)
}

fn skip_long(bytes: &[u8], at: usize, n: usize) -> usize {
    let close = format!("]{}]", "=".repeat(n));
    let from = at + n + 2;
    bytes[from..]
        .windows(close.len())
        .position(|w| w == close.as_bytes())
        .map(|p| from + p + close.len())
        .unwrap_or(bytes.len())
}

/// Identifiers and the brackets that matter, with comments and strings skipped: a `)` inside
/// a label or a comment is not the end of the call.
fn tokens(src: &str) -> Vec<Token> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let kind = match b[i] {
            c if c.is_ascii_whitespace() => {
                i += 1;
                continue;
            }
            b'-' if b.get(i + 1) == Some(&b'-') => {
                i += 2;
                if let Some(n) = long_bracket(b, i) {
                    i = skip_long(b, i, n);
                } else {
                    while i < b.len() && b[i] != b'\n' {
                        i += 1;
                    }
                }
                continue;
            }
            b'"' | b'\'' => {
                let quote = b[i];
                i += 1;
                while i < b.len() && b[i] != quote {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                i = (i + 1).min(b.len());
                continue;
            }
            b'[' if long_bracket(b, i).is_some() => {
                let n = long_bracket(b, i).unwrap();
                i = skip_long(b, i, n);
                continue;
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                let start = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                out.push(Token {
                    kind: Kind::Ident,
                    start,
                    end: i,
                });
                continue;
            }
            b'.' => Kind::Dot,
            b'(' => Kind::Open,
            b')' => Kind::Close,
            _ => Kind::Other,
        };
        let start = i;
        i += 1;
        out.push(Token {
            kind,
            start,
            end: i,
        });
    }
    out
}

/// The text of `omsi.plugin.settings(...)` or `plugin.settings(...)`, without any other code
/// from the plugin.
fn call_expression(src: &str) -> Option<String> {
    let t = tokens(src);
    for (i, w) in t.windows(4).enumerate() {
        let named =
            |tok: &Token, name: &str| tok.kind == Kind::Ident && &src[tok.start..tok.end] == name;
        if !named(&w[0], "plugin")
            || w[1].kind != Kind::Dot
            || !named(&w[2], "settings")
            || w[3].kind != Kind::Open
        {
            continue;
        }
        let mut start = w[0].start;
        if i >= 2 && named(&t[i - 2], "omsi") && t[i - 1].kind == Kind::Dot {
            start = t[i - 2].start;
        }
        let mut depth = 0;
        for tok in &t[i + 3..] {
            if tok.kind == Kind::Open {
                depth += 1;
            } else if tok.kind == Kind::Close {
                depth -= 1;
                if depth == 0 {
                    let text = &src[start..tok.end];
                    return (text.len() <= MAX_EXPRESSION).then(|| text.to_string());
                }
            }
        }
        return None;
    }
    None
}

fn to_json(v: &LuaValue, depth: u8) -> LuaResult<Value> {
    if depth > MAX_DEPTH {
        return Err(mlua::Error::runtime("settings table nested too deeply"));
    }
    Ok(match v {
        LuaValue::Nil
        | LuaValue::LightUserData(_)
        | LuaValue::UserData(_)
        | LuaValue::Function(_)
        | LuaValue::Thread(_)
        | LuaValue::Other(_)
        | LuaValue::Error(_) => Value::Null,
        LuaValue::Boolean(b) => Value::Bool(*b),
        LuaValue::Integer(n) => Value::Number((*n).into()),
        LuaValue::Number(n) => serde_json::Number::from_f64(*n)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        LuaValue::String(s) => Value::String(s.to_string_lossy().to_string()),
        LuaValue::Table(t) => {
            let len = t.raw_len() as usize;
            let mut array = Vec::new();
            let mut object = Map::new();
            let mut is_array = len > 0;
            for pair in t.clone().pairs::<LuaValue, LuaValue>() {
                let (k, v) = pair?;
                let value = to_json(&v, depth + 1)?;
                match k {
                    LuaValue::Integer(n) if n >= 1 => {
                        if n as usize > len {
                            is_array = false;
                        }
                        array.push((n as usize, value));
                    }
                    LuaValue::String(s) => {
                        is_array = false;
                        object.insert(s.to_string_lossy().to_string(), value);
                    }
                    _ => is_array = false,
                }
            }
            if is_array {
                array.sort_by_key(|(k, _)| *k);
                Value::Array(array.into_iter().map(|(_, v)| v).collect())
            } else {
                Value::Object(object)
            }
        }
    })
}

/// Only the fields the settings form uses, in the shape the game's own schema has.
fn sanitize(spec: Value, title: Option<String>) -> Option<Value> {
    // `plugin.settings` takes the list itself; the game's saved schema wraps it.
    let list = if let Some(array) = spec.as_array() {
        array
    } else {
        spec.as_object()?.get("settings")?.as_array()?
    };
    let mut settings = Vec::new();
    for s in list {
        let s = s.as_object()?;
        let key = s.get("key")?.as_str()?.to_string();
        let ty = s.get("type")?.as_str()?;
        if !matches!(ty, "bool" | "number" | "text" | "choice") {
            continue;
        }
        let mut m = Map::new();
        m.insert("key".into(), Value::String(key.clone()));
        m.insert(
            "label".into(),
            Value::String(
                s.get("label")
                    .and_then(Value::as_str)
                    .unwrap_or(&key)
                    .to_string(),
            ),
        );
        m.insert("type".into(), Value::String(ty.to_string()));
        match ty {
            "number" => {
                for field in ["min", "max", "step"] {
                    if let Some(n) = s.get(field).and_then(Value::as_f64) {
                        m.insert(field.into(), json!(n));
                    }
                }
            }
            "choice" => {
                let choices = s.get("choices")?.as_array()?;
                if choices.is_empty() || !choices.iter().all(|c| c.as_str().is_some()) {
                    continue;
                }
                m.insert("choices".into(), Value::Array(choices.clone()));
            }
            _ => {}
        }
        if let Some(default) = s.get("default") {
            m.insert("default".into(), default.clone());
        }
        settings.push(Value::Object(m));
    }
    if settings.is_empty() {
        return None;
    }
    let title = title
        .or_else(|| {
            spec.as_object()
                .and_then(|o| o.get("title"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default();
    Some(json!({ "title": title, "settings": settings }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_literal_declaration_is_read_without_running_the_plugin() {
        let src = r#"
-- a comment with ) and plugin.settings( in it
local s = omsi.plugin.settings({
  { key = "limit", type = "number", label = "Warn above (km/h)", default = 50, min = 20, max = 120, step = 5 },
  { key = "only_on_duty", type = "bool", label = "Only while on a duty", default = false },
}, "Speed warning")
omsi.on("duty_start", function() print("not run") end)
"#;
        let schema = from_source(src).unwrap();
        assert_eq!(schema["title"], "Speed warning");
        assert_eq!(schema["settings"][0]["key"], "limit");
        assert_eq!(schema["settings"][0]["max"], 120.0);
        assert_eq!(schema["settings"][1]["type"], "bool");
    }

    #[test]
    fn a_declaration_built_at_runtime_is_left_to_the_game() {
        let src = r#"local spec = build(); local s = omsi.plugin.settings(spec, "Late")"#;
        assert!(from_source(src).is_none());
    }

    #[test]
    fn a_declaration_cannot_call_the_game_apis() {
        let src = r#"local s = omsi.plugin.settings({ { key = "x", type = "text", default = os.execute("true") } })"#;
        assert!(from_source(src).is_none());
    }
}
