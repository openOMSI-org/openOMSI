//! `cable.*`: lines a plugin lays in the 3D world - overhead wires, ropes - drawn as thin
//! round tubes, lit and shaded as the scene round them (see `crate::api::cables`).

use super::{def, o, p};
use crate::api::{ApiError, ApiFn, Value};
use crate::api::cables::{Cable, CableFrame, MAX_POINTS};

const NEW: &str = crate::api::VERSION;

/// Thickness of a cable when none is given: a 12 mm contact wire.
const DEFAULT_RADIUS: f32 = 0.006;
const DEFAULT_COLOR: [f32; 4] = [0.08, 0.08, 0.08, 1.0];

fn err(msg: impl Into<String>) -> ApiError {
    ApiError(format!("omsi.cable.set: {}", msg.into()))
}

/// The points: a flat list `{x1, y1, z1, x2, y2, z2, ...}` or a list of `{x, y, z}`.
fn points(v: &Value) -> Result<Vec<[f64; 3]>, ApiError> {
    let items = v.items();
    if !v.is_table() {
        return Err(err("points must be a table"));
    }
    let mut out = Vec::new();
    if items.first().is_some_and(Value::is_table) {
        for (i, p) in items.iter().enumerate() {
            let xyz = p.items();
            let n = |k: usize, key: &str| xyz.get(k).or_else(|| p.get(key)).and_then(Value::as_f64);
            match (n(0, "x"), n(1, "y"), n(2, "z")) {
                (Some(x), Some(y), Some(z)) => out.push([x, y, z]),
                _ => return Err(err(format!("points[{}] must be {{x, y, z}}", i + 1))),
            }
        }
    } else {
        if items.len() % 3 != 0 {
            return Err(err("a flat list of points has three numbers each"));
        }
        for (i, c) in items.chunks(3).enumerate() {
            match (c[0].as_f64(), c[1].as_f64(), c[2].as_f64()) {
                (Some(x), Some(y), Some(z)) => out.push([x, y, z]),
                _ => return Err(err(format!("point {} is not three numbers", i + 1))),
            }
        }
    }
    if out.len() < 2 {
        return Err(err("a cable needs two points at least"));
    }
    if out.len() > MAX_POINTS {
        return Err(err(format!("at most {MAX_POINTS} points a cable")));
    }
    if out.iter().flatten().any(|c| !c.is_finite()) {
        return Err(err("the points must be finite numbers"));
    }
    Ok(out)
}

fn options(v: &Value) -> Result<(f32, [f32; 4], CableFrame), ApiError> {
    let mut radius = DEFAULT_RADIUS;
    let mut color = DEFAULT_COLOR;
    let mut frame = CableFrame::World;
    if v.is_nil() {
        return Ok((radius, color, frame));
    }
    if let Some(r) = v.get("radius").filter(|r| !r.is_nil()) {
        radius = r.as_f64().filter(|r| *r > 0.0 && *r <= 1.0).ok_or_else(|| err("radius is a number of metres, over 0 and at most 1"))? as f32;
    }
    if let Some(c) = v.get("color").filter(|c| !c.is_nil()) {
        let n: Vec<f64> = c.items().iter().filter_map(Value::as_f64).collect();
        if !(n.len() == 3 || n.len() == 4) || n.len() != c.items().len() {
            return Err(err("color is {r, g, b} or {r, g, b, a}, each 0 to 1"));
        }
        color = [n[0] as f32, n[1] as f32, n[2] as f32, n.get(3).copied().unwrap_or(1.0) as f32].map(|x| x.clamp(0.0, 1.0));
    }
    match v.get("frame").and_then(Value::as_str) {
        None | Some("world") => {}
        Some("bus") => frame = CableFrame::Bus,
        Some(other) => return Err(err(format!("frame is \"world\" or \"bus\", not \"{other}\""))),
    }
    Ok((radius, color, frame))
}

pub(crate) static FNS: &[ApiFn] = &[
    def!("cable.set", "scene", [p("id", "string"), p("points", "table"), o("options", "table")], "boolean", "Lays the cable `id` or moves it: a thin round tube through `points` (a flat list `{x1, y1, z1, x2, ...}` or a list of `{x, y, z}`, metres), lit and shaded as the scene. `options`: `radius` (m, default 0.006), `color` (`{r, g, b}` or `{r, g, b, a}`, 0 to 1, default dark grey) and `frame`: `\"world\"` (the map's coordinates, z up; the default) or `\"bus\"` (the player's bus's model frame: x right, y forward, z up, so a rope tied to the bus moves with it as drawn). Set it again whenever it moves; `false` (and the cable stays as it was) when the plugin would have more than 4096 cables, or more than 65536 points in all its cables together.", NEW, Ui, false, |c, a| {
        let id = a.str(0)?;
        let pts = points(a.get(1))?;
        let (radius, color, frame) = options(a.get(2))?;
        let s = c.state();
        let owner = s.owner;
        Ok::<_, ApiError>(s.hub.cables.borrow_mut().set(owner, &id, Cable { points: pts, radius, color, frame }))
    }),
    def!("cable.remove", "scene", [p("id", "string")], "boolean", "Takes the cable `id` away; `true` when there was one.", NEW, Ui, false, |c, a| {
        let id = a.str(0)?;
        let s = c.state();
        let owner = s.owner;
        Ok::<_, ApiError>(s.hub.cables.borrow_mut().remove(owner, &id))
    }),
    def!("cable.clear", "scene", [], "integer", "Takes every cable of the plugin away; how many went. They go by themselves when the plugin stops.", NEW, Ui, false, |c, a| {
        let s = c.state();
        let owner = s.owner;
        s.hub.cables.borrow_mut().remove_owner(owner) as i64
    }),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn list(v: &[f64]) -> Value {
        Value::List(v.iter().map(|x| Value::Num(*x)).collect())
    }

    #[test]
    fn points_come_flat_or_as_triples() {
        let flat = points(&list(&[0.0, 0.0, 6.0, 30.0, 0.0, 6.2])).unwrap();
        let triples = points(&Value::List(vec![list(&[0.0, 0.0, 6.0]), list(&[30.0, 0.0, 6.2])])).unwrap();
        assert_eq!(flat, triples);
        assert!(points(&list(&[0.0, 0.0])).is_err());
        assert!(points(&list(&[0.0, 0.0, 6.0])).is_err());
    }

    #[test]
    fn options_have_defaults_and_say_what_is_wrong() {
        assert_eq!(options(&Value::Nil).unwrap(), (DEFAULT_RADIUS, DEFAULT_COLOR, CableFrame::World));
        let o = Value::map([("radius", Value::Num(0.01)), ("frame", Value::Str("bus".into())), ("color", list(&[1.0, 0.5, 0.0]))]);
        assert_eq!(options(&o).unwrap(), (0.01, [1.0, 0.5, 0.0, 1.0], CableFrame::Bus));
        assert!(options(&Value::map([("frame", Value::Str("sky".into()))])).is_err());
        assert!(options(&Value::map([("radius", Value::Num(-1.0))])).is_err());
    }
}
