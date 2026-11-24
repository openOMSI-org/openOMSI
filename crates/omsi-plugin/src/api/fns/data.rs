//! Game data, queried independently of the bus's script execution.
use super::{def, o, p, rec};
use crate::api::{ApiError, ApiFn, Value};
use crate::data::{DataQuery, QueryType, QueryValue};
const NEW: &str = "0.3.40";

fn ty(t: QueryType) -> &'static str {
    match t {
        QueryType::Number => "number",
        QueryType::String => "string",
        QueryType::Rgba => "rgba",
    }
}
pub static FNS: &[ApiFn] = &[
    def!(
        "game.queries",
        "data",
        [],
        "list of tables",
        "Read-only game data getters: `{name, args = {{name, type}, ...}, result}`. Names and zero-based indices follow the M.V. getter names, but plugins read the game data directly without executing scripts. `rgba` means four results in the range 0–255.",
        NEW,
        None,
        false,
        |c, a| Value::List(
            DataQuery::ALL
                .iter()
                .map(|q| rec(vec![
                    ("name", q.name().into()),
                    (
                        "args",
                        Value::List(
                            q.args()
                                .iter()
                                .map(|(n, t)| rec(vec![
                                    ("name", (*n).into()),
                                    ("type", ty(*t).into())
                                ]))
                                .collect()
                        )
                    ),
                    ("result", ty(q.result()).into())
                ]))
                .collect()
        )
    ),
    def!(
        "game.query",
        "data",
        [
            p("name", "string"),
            o("args", "table"),
            o("object", "integer")
        ],
        "value(s), or nil and reason",
        "Read a getter listed by `game.queries` from the player's vehicle's HOF, timetable, passengers, tickets, radio, fonts or textures. Arguments are an ordered list (empty by default). No script callback is invoked. `STReadPixel` returns RGBA without changing the bus's drawing colour. Supply a loaded scripted map object's id as `object` to read its data, including `GetArrBus*` on departure boards, without needing a player vehicle. Object ids can be found through `map.objects_near`. Without `object`, reads use the player's vehicle. Unknown names, mutating operations and invalid arguments raise an error; no vehicle returns nil and a reason.",
        NEW,
        None,
        true,
        |c, a| {
            let q = DataQuery::from_name(&a.str(0)?)
                .ok_or_else(|| ApiError("unknown read-only game data query".into()))?;
            let values = match a.get(1) {
                Value::Nil => &[][..],
                Value::List(l) => l.as_slice(),
                Value::Map(m) if m.is_empty() => &[],
                _ => return Err(ApiError("args must be an ordered list".into())),
            };
            let args: Vec<_> = values
                .iter()
                .map(|v| match v {
                    Value::Int(n) => Ok(QueryValue::Number(*n as f32)),
                    Value::Num(n) if n.is_finite() && (*n as f32).is_finite() => {
                        Ok(QueryValue::Number(*n as f32))
                    }
                    Value::Str(s) => Ok(QueryValue::String(s.clone())),
                    _ => Err(ApiError(
                        "query arguments must be finite numbers or strings".into(),
                    )),
                })
                .collect::<Result<_, _>>()?;
            q.validate(&args).map_err(ApiError)?;
            Ok::<_, ApiError>(Value::List(
                match c.io().data_query(q, &args, a.opt_int(2)?) {
                    Ok(v) => v
                        .into_iter()
                        .map(|v| match v {
                            QueryValue::Number(n) => Value::Num(n as f64),
                            QueryValue::String(s) => Value::Str(s),
                        })
                        .collect(),
                    Err(e) => vec![Value::Nil, e.into()],
                },
            ))
        }
    ),
];
