use serde_json::{json, Value};

pub struct Turn {
    pub direction: i32,
    pub angle: f32,
    pub metres: f32,
    pub street: Option<String>,
}

pub struct NavInfo {
    pub anchor: [f64; 2],
    pub route: Vec<[f32; 2]>,
    pub turn: Option<Turn>,
    pub stop: Option<(String, f32)>,
    pub stops: Vec<[f32; 2]>,
    pub street: Option<String>,
    pub limit: Option<f32>,
    pub on_route: bool,
}

fn round(v: f32) -> f64 {
    (v as f64 * 10.0).round() / 10.0
}

fn line(points: &[[f32; 2]]) -> Value {
    Value::Array(points.iter().flat_map(|p| [json!(round(p[0])), json!(round(p[1]))]).collect())
}

pub fn roads_message(anchor: [f64; 2], roads: &[(Vec<[f32; 2]>, f32)]) -> String {
    let roads: Vec<Value> = roads.iter().map(|(pts, width)| json!({ "w": round(*width), "p": line(pts) })).collect();
    json!({ "t": "roads", "anchor": anchor, "roads": roads }).to_string()
}

pub fn nav_message(n: &NavInfo) -> String {
    json!({
        "t": "nav",
        "anchor": n.anchor,
        "route": line(&n.route),
        "turn": n.turn.as_ref().map(|t| json!({ "dir": t.direction, "angle": round(t.angle), "m": round(t.metres), "street": t.street })),
        "stop": n.stop.as_ref().map(|(name, m)| json!({ "name": name, "m": round(*m) })),
        "stops": line(&n.stops),
        "street": n.street,
        "limit": n.limit.map(round),
        "onRoute": n.on_route,
    })
    .to_string()
}
