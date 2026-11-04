use crate::layouts::{Scope, Store};
use crate::{Command, Vehicle};
use omsi_sim::vehicle_api::ApiValue;
use serde_json::{json, Map, Value};

pub(crate) fn api(a: &ApiValue) -> Value {
    match a {
        ApiValue::Null => Value::Null,
        ApiValue::Bool(b) => Value::Bool(*b),
        ApiValue::Num(n) => serde_json::Number::from_f64(*n).map_or(Value::Null, Value::Number),
        ApiValue::Str(s) => Value::String(s.clone()),
        ApiValue::List(l) => Value::Array(l.iter().map(api).collect()),
        ApiValue::Map(m) => Value::Object(m.iter().map(|(k, v)| (k.clone(), api(v))).collect::<Map<_, _>>()),
    }
}

pub(crate) fn texts_message(lang: &str, texts: &[(&str, String)]) -> String {
    let map: Map<String, Value> = texts.iter().filter(|(k, v)| *k != v.as_str()).map(|(k, v)| (k.to_string(), Value::String(v.clone()))).collect();
    json!({ "t": "texts", "lang": lang, "texts": map }).to_string()
}

pub(crate) fn state(vehicle: Option<&ApiValue>, env: &ApiValue, paused: bool) -> Value {
    json!({ "vehicle": vehicle.map_or(Value::Null, api), "env": api(env), "paused": paused })
}

pub(crate) fn state_delta(old: Option<&Value>, new: &Value) -> Option<String> {
    let full = || Some(json!({ "t": "state", "full": true, "vehicle": new["vehicle"], "env": new["env"], "paused": new["paused"] }).to_string());
    let Some(old) = old else { return full() };
    let (Some(a), Some(b)) = (old["vehicle"].as_object(), new["vehicle"].as_object()) else {
        return (old != new).then(full).flatten();
    };
    let mut out = Map::new();
    let sections: Map<String, Value> = b.iter().filter(|(k, v)| a.get(*k) != Some(*v)).map(|(k, v)| (k.clone(), v.clone())).collect();
    if !sections.is_empty() {
        out.insert("vehicle".into(), Value::Object(sections));
    }
    for k in ["env", "paused"] {
        if old[k] != new[k] {
            out.insert(k.into(), new[k].clone());
        }
    }
    if out.is_empty() {
        return None;
    }
    out.insert("t".into(), "state".into());
    Some(Value::Object(out).to_string())
}

pub(crate) fn vehicle_message(v: Option<&Vehicle>, store: &Store, skins: &[String]) -> String {
    let Some(v) = v else {
        return json!({ "t": "vehicle", "key": Value::Null }).to_string();
    };
    let layout = store.resolve(&v.key, &v.folder).map(|(scope, l)| json!({ "scope": scope.name(), "data": l }));
    json!({
        "t": "vehicle",
        "key": v.key,
        "name": v.name,
        "actions": v.actions,
        "depot": v.depot.as_ref().map_or(Value::Null, api),
        "layout": layout,
        "skins": skins,
        "doors": v.doors,
        "aliases": v.aliases.iter().map(|(a, b)| json!([a, b])).collect::<Vec<_>>(),
        "saved": { "vehicle": store.has(&v.key, Scope::Vehicle), "all": store.has(&v.key, Scope::All) },
    })
    .to_string()
}

pub(crate) fn action_name(s: &str) -> Option<String> {
    let ok = !s.is_empty() && s.len() <= 64 && s.bytes().all(|c| c.is_ascii_alphanumeric() || b"_-+.".contains(&c));
    ok.then(|| s.to_string())
}

fn index(v: &Value) -> Option<usize> {
    v.get("index").and_then(Value::as_u64).and_then(|i| usize::try_from(i).ok()).filter(|i| *i < 100_000)
}

pub(crate) fn drive(v: &Value) -> Option<(f32, f32, f32)> {
    let f = |k: &str| v.get(k).and_then(Value::as_f64).filter(|x| x.is_finite()).map(|x| x as f32);
    Some((f("s")?.clamp(-1.0, 1.0), f("g")?.clamp(0.0, 1.0), f("b")?.clamp(0.0, 1.0)))
}

pub(crate) fn command(v: &Value) -> Option<Command> {
    let name = || v.get("name").and_then(Value::as_str).and_then(action_name);
    Some(match v.get("t")?.as_str()? {
        "act" => Command::Action { name: name()?, down: v.get("down").and_then(Value::as_bool)? },
        "game" => Command::Game(name()?),
        "indicator" => Command::Indicator(v.get("value").and_then(Value::as_u64).filter(|n| *n <= 3)? as u8),
        "startup" => Command::StartUp,
        "saloon" => Command::SaloonLights,
        "route" => Command::Route(index(v)?),
        "destination" => Command::Destination(index(v)?),
        "nextstop" => Command::NextStop(index(v)?),
        "clearline" => Command::ClearLine,
        "line" => {
            let l = v.get("line").and_then(Value::as_str)?.trim();
            (!l.is_empty() && l.len() <= 16 && !l.chars().any(char::is_control)).then(|| Command::Line(l.to_string()))?
        }
        _ => return None,
    })
}
