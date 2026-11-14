//! `traffic.*` and `people.*`: the AI vehicles and the people of the map.

use super::{def, multi, o, p, rec};
use crate::api::{ApiError, ApiFn, Ctx, Value};
use crate::io::{AiCar, Person, TicketAsk, TicketChange, TicketKind};

const NEW: &str = crate::api::VERSION;
/// The passengers' tickets (`people.get`, `people.set_ticket`, the `passenger_ticket` event).
const TICKETS: &str = "0.3";

fn car(c: &AiCar, from: Option<[f64; 4]>) -> Value {
    let mut r = vec![
        ("id", Value::Int(c.id as i64)),
        ("kind", c.kind.into()),
        ("name", c.name.clone().into()),
        ("x", c.pos[0].into()),
        ("y", c.pos[1].into()),
        ("z", c.pos[2].into()),
        ("heading", c.pos[3].into()),
        ("speed", c.speed_kmh.into()),
        ("max_speed", c.max_speed_kmh.into()),
        ("waiting_for", if c.why.is_empty() { Value::Nil } else { c.why.into() }),
        ("standing", Value::from(c.standing)),
        ("braking", c.braking.into()),
        ("blinker", ["off", "left", "right"].get(c.blinker as usize).copied().unwrap_or("off").into()),
        ("line", c.line.clone().into()),
        ("bounding_box", bounding_box(c)),
    ];
    if let Some(p) = from {
        r.push(("distance", (c.pos[0] - p[0]).hypot(c.pos[1] - p[1]).into()));
    }
    rec(r)
}

/// `{width, length, height, cx, cy, cz, heading, default}`: the vehicle's `[boundingbox]` and
/// the heading its axes follow.
fn bounding_box(c: &AiCar) -> Value {
    let b = c.bounding_box;
    rec(vec![
        ("width", Value::from(b[0])),
        ("length", Value::from(b[1])),
        ("height", Value::from(b[2])),
        ("cx", Value::from(b[3])),
        ("cy", Value::from(b[4])),
        ("cz", Value::from(b[5])),
        ("heading", c.box_heading.rem_euclid(360.0).into()),
        ("default", c.box_default.into()),
    ])
}

/// A person as `people.list` and `people.get` give them.
fn person(p: Person) -> Value {
    let t = p.pax_ticket.as_ref();
    rec(vec![
        ("id", Value::from(p.id)),
        ("x", p.pos[0].into()),
        ("y", p.pos[1].into()),
        ("z", p.pos[2].into()),
        ("state", p.state.into()),
        ("aboard", p.aboard.into()),
        ("ai_bus", p.in_ai_bus.map(|b| Value::Int(b as i64)).into()),
        ("stop", p.stop.map(Value::Int).into()),
        ("stop_name", p.stop_name.into()),
        ("destination", p.destination.into()),
        ("ticket", p.ticket.into()),
        ("ticket_kind", t.map(|t| t.kind.as_str()).into()),
        ("ticket_index", t.map(|t| Value::Int(t.index as i64)).into()),
        ("ticket_price", t.map(|t| t.price).into()),
        ("ticket_voice", t.and_then(|t| t.voice.clone()).into()),
        ("ticket_by", t.and_then(|t| t.by.clone()).into()),
        ("ticket_asked", t.map(|t| t.asked).into()),
        ("tag", p.tag.into()),
        ("age", p.age.into()),
        ("model", p.model.into()),
        ("voice", p.voice.into()),
        ("complaint", Value::Int(p.complaint as i64)),
    ])
}

/// A passenger's ticket as the `passenger_ticket` event gives it (the stops in the trip
/// counted from 1, as the duty's are).
pub(crate) fn ticket_ask(a: TicketAsk) -> Value {
    rec(vec![
        ("id", Value::from(a.person)),
        ("age", a.age.into()),
        ("kind", a.kind.as_str().into()),
        ("ticket", Value::Int(a.ticket as i64)),
        ("name", a.name.into()),
        ("price", a.price.into()),
        ("stop", a.stop.map(Value::Int).into()),
        ("stop_name", a.stop_name.into()),
        ("destination", a.destination.into()),
        ("stop_number", a.stop_index.map(|k| k + 1).into()),
        ("destination_number", a.destination_index.map(|k| k + 1).into()),
    ])
}

/// `people.set_ticket`'s table.
fn ticket_change(t: &Value) -> Result<TicketChange, ApiError> {
    let bad = |k: &str, want: &str| ApiError(format!("omsi.people.set_ticket: `{k}` must be {want}"));
    let field = |k: &str| t.get(k).filter(|v| !v.is_nil());
    let text = |k: &str| field(k).map(|v| v.to_text().ok_or_else(|| bad(k, "a string"))).transpose();
    Ok(TicketChange {
        kind: field("kind").map(|v| v.to_text().and_then(|s| TicketKind::parse(&s)).ok_or_else(|| bad("kind", "\"none\", \"stamp\" or \"buy\""))).transpose()?,
        ticket: field("ticket").map(|v| v.as_i64().filter(|n| (0..=255).contains(n)).map(|n| n as u8).ok_or_else(|| bad("ticket", "a ticket's number (0 to 255)"))).transpose()?,
        name: text("name")?,
        price: field("price").map(|v| v.as_f64().filter(|n| n.is_finite() && *n >= 0.0).map(|n| n as f32).ok_or_else(|| bad("price", "a number of 0 or more"))).transpose()?,
        voice: text("voice")?,
        tag: text("tag")?,
        payment: field("payment").map(|v| v.as_f64().filter(|n| n.is_finite() && *n >= 0.0).map(|n| n as f32).ok_or_else(|| bad("payment", "a number of 0 or more"))).transpose()?,
    })
}

/// The cars within `r` m of the player's bus (all, without a bus or without `r`).
fn cars(c: &mut Ctx<'_>, r: Option<f64>) -> Vec<(f64, AiCar)> {
    let io = c.io();
    let me = io.position();
    let mut list: Vec<(f64, AiCar)> = io.traffic_list().into_iter().map(|x| (me.map_or(0.0, |p| (x.pos[0] - p[0]).hypot(x.pos[1] - p[1])), x)).collect();
    if let (Some(r), Some(_)) = (r, me) {
        list.retain(|(d, _)| *d <= r);
    }
    list
}

pub static FNS: &[ApiFn] = &[
    def!("traffic.list", "traffic", [o("radius", "number")], "list of tables", "The AI vehicles (within `radius` m of the player's bus, when given): `{id, kind, name, x, y, z, heading, speed, max_speed, waiting_for, standing, braking, blinker, line, bounding_box, distance}`. `kind`: `\"car\"`, `\"taxi\"`, `\"bus\"`, `\"truck\"`, `\"timetable_bus\"`, `\"tram\"`, `\"bicycle\"`; `waiting_for` why it waits or slows (`\"lead\"`, `\"light\"`, `\"yield\"`, `\"people\"`, ...); `standing` the seconds it has stood; `bounding_box` the body's `[boundingbox]` as `{width, length, height, cx, cy, cz, heading, default}` (metres, the .bus/.ovh order; `cx`/`cy`/`cz` the box centre's offset from the vehicle's x, y, z along the body's right, forward and up, which follow `heading`; `default` when the file gives no box and the collision code's 2.5 x 12 x 3 m is used).", NEW, None, false, |c, a| {
        let r = a.opt_num(0)?;
        let me = c.io().position();
        Ok::<_, ApiError>(Value::List(cars(c, r).iter().map(|(_, x)| car(x, me)).collect()))
    }),
    def!("traffic.get", "traffic", [p("id", "integer")], "table or nil", "One AI vehicle by its id, as `traffic.list` gives them.", NEW, None, false, |c, a| {
        let id = a.int(0)? as u64;
        let me = c.io().position();
        Ok::<_, ApiError>(c.io().traffic_list().into_iter().find(|x| x.id == id).map(|x| car(&x, me)))
    }),
    def!("traffic.nearest", "traffic", [o("kind", "string")], "table or nil", "The AI vehicle nearest to the player's bus (of that `kind`, when given), with its `distance`.", NEW, None, false, |c, a| {
        let kind = a.opt_str(0)?;
        let me = c.io().position();
        let best = cars(c, None).into_iter().filter(|(_, x)| kind.as_deref().is_none_or(|k| k == x.kind)).min_by(|a, b| a.0.total_cmp(&b.0));
        Ok::<_, ApiError>(best.filter(|_| me.is_some()).map(|(_, x)| car(&x, me)))
    }),
    def!("traffic.ahead", "traffic", [o("reach", "number")], "table or nil", "The AI vehicle ahead of the player's bus within `reach` m (default 100) and 20° of its heading, with its `distance`: for a distance warning or a cruise control.", NEW, None, false, |c, a| {
        let reach = a.opt_num(0)?.unwrap_or(100.0);
        let Some(me) = c.io().position() else { return Ok(Value::Nil) };
        let h = me[3].to_radians();
        let (fx, fy) = (h.sin(), h.cos());
        let best = cars(c, Some(reach)).into_iter().filter(|(d, x)| {
            let (dx, dy) = (x.pos[0] - me[0], x.pos[1] - me[1]);
            let along = dx * fx + dy * fy;
            along > 0.0 && (dx * fy - dy * fx).abs() < along * 20f64.to_radians().tan() + 1.5 && *d > 0.5
        }).min_by(|a, b| a.0.total_cmp(&b.0));
        Ok::<_, ApiError>(best.map(|(_, x)| car(&x, Some(me))))
    }),
    def!("traffic.counts", "traffic", [], "driving, buses, asleep, parked", "The AI vehicles: driving, timetable buses among them, asleep out of range, parked.", NEW, None, true, |c, a| multi(c.io().traffic_counts().map(|n| n.map(Value::from)))),
    def!("traffic.density", "traffic", [], "cars, share", "How many cars the traffic keeps around the camera, and the share of them that do not run to a timetable (0 to 1).", NEW, None, true, |c, a| multi(c.io().traffic_density().map(|(n, s)| [Value::from(n), Value::from(s)]))),
    def!("traffic.set_density", "traffic", [p("cars", "integer"), o("share", "number")], "boolean", "Changes the traffic's amount (as the game menu's slider: 0 to 500 cars) and, when given, the share not running to a timetable; it fills up or thins out over the next seconds.", NEW, TrafficWrite, false, |c, a| {
        let n = a.int(0)?.clamp(0, 500) as usize;
        let share = a.opt_num(1)?.map(|s| s.clamp(0.0, 1.0) as f32);
        Ok::<_, ApiError>(c.io().traffic_set_density(Some(n), share))
    }),
    def!("traffic.remove", "traffic", [p("id", "integer")], "boolean", "Takes an AI vehicle off the road (a timetable bus too: its timetable forgets it); its passengers get out.", NEW, TrafficWrite, false, |c, a| {
        let id = a.int(0)? as u64;
        Ok::<_, ApiError>(c.io().traffic_remove(id))
    }),
    def!("traffic.clear", "traffic", [], "integer or nil", "Takes every car not running to a timetable off the road; how many went.", NEW, TrafficWrite, false, |c, a| c.io().traffic_clear()),
    def!("traffic.light_ahead", "traffic", [o("reach", "number")], "table or nil", "The traffic light the player's bus comes to within `reach` m (default 80): `{aspect, change_in, distance}` - `aspect` `\"red\"`, `\"red_yellow\"`, `\"green\"`, `\"green_yellow\"`, `\"yellow\"` or `\"dark\"`, `change_in` the seconds to its next change.", NEW, None, false, |c, a| {
        let r = a.opt_num(0)?.unwrap_or(80.0).clamp(1.0, 500.0);
        Ok::<_, ApiError>(c.io().traffic_light_ahead(r).map(|l| rec(vec![("aspect", l.aspect.into()), ("change_in", Value::from(l.change_in)), ("distance", l.distance.into())])))
    }),
    def!("people.counts", "people", [], "walking, waiting, riding", "The people of the map near the camera: walking, waiting at stops, riding a bus.", NEW, None, true, |c, a| multi(c.io().people_counts().map(|n| n.map(Value::from)))),
    def!("people.list", "people", [o("radius", "number")], "list of tables", "The people (within `radius` m of the player's bus, when given): `{id, x, y, z, state, aboard, ai_bus, stop, destination, ticket, complaint}`. `state`: `\"strolling\"`, `\"idle\"`, `\"standing\"`, `\"waiting\"`, `\"to_bus\"`, `\"boarding\"`, `\"riding\"`, `\"seated\"`, `\"leaving\"`, `\"to_stop\"`; `aboard` in the player's bus; `complaint` 0 to 3 (3: they leave); and the fields `people.get` describes.", NEW, None, false, |c, a| {
        let r = a.opt_num(0)?;
        let io = c.io();
        let me = io.position();
        Ok::<_, ApiError>(Value::List(io.people_list().into_iter().filter(|p| match (r, me) {
            (Some(r), Some(m)) => (p.pos[0] - m[0]).hypot(p.pos[1] - m[1]) <= r,
            _ => true,
        }).map(person).collect()))
    }),
    def!("people.get", "people", [p("id", "integer")], "table or nil", "One person by id, as `people.list` gives them, and more: `stop_name` (the timetable's name of the stop they wait or waited at), `ticket` (the name of the ticket they ask for), `ticket_kind` (`\"none\"`, `\"stamp\"`, `\"buy\"`), `ticket_index` (the ticket pack's, from 1; 0 none), `ticket_price`, `ticket_voice`, `ticket_by` (the plugin that changed it), `ticket_asked` (asked for at the cash desk already), `tag` (`people.set_ticket`'s), `age`, `model` (the `.hum` file), `voice` (its voice folder). `people.list` gives the same fields.", TICKETS, None, false, |c, a| {
        let id = a.int(0)?;
        Ok::<_, ApiError>(c.io().people_list().into_iter().find(|p| p.id as i64 == id).map(person))
    }),
    def!("people.set_ticket", "people", [p("id", "integer"), p("ticket", "table")], "true, or false and the reason", "Changes the ticket of a passenger boarding the player's bus (the `passenger_ticket` event names them): `{kind, ticket, name, price, payment, voice, tag}`, each optional. `kind` (`\"none\"`: a pass, `\"stamp\"`, `\"buy\"`) can only be changed until they are in the bus; `ticket` selects another ticket from the pack (starting at 1, with its name and price; a buyer must have a ticket: 0 is refused, and a buyer created without `ticket` gets the pack's ticket for their age). `name` and `price` set the ticket name and ticket price, respectively, until the passenger has asked at the cash desk. `payment` specifies the amount of money handed over for a ticket purchase; it does not change the ticket price and is raised to at least the ticket price. It must be set before the passenger reaches the cash desk. If the currency's denominations cannot represent the requested amount exactly, the amount handed over may be higher. `voice` selects a line from the passenger's voice folder to use instead of `Ticket_<n>_<k>` (`\"Zone_4\"` selects `Zone_4.wav`; if the file is missing, the usual line is used; `\"\"` also falls back to the usual line), until the passenger has asked at the cash desk. `tag` is plugin-defined text kept with the person (`people.get`), and can be set at any time. Returns `false` and the reason when nothing changed.", TICKETS, TrafficWrite, true, |c, a| {
        let id = a.int(0)?;
        let change = ticket_change(a.table(1)?)?;
        let by = c.state().name.clone();
        let id = u32::try_from(id).map_err(|_| ApiError("omsi.people.set_ticket: no such person".into()))?;
        Ok::<_, ApiError>(super::core::ok_or_reason(c.io().people_set_ticket(id, &change, &by)))
    }),
    def!("people.stops", "people", [], "list of tables", "The stops near the camera where people wait: `{id, name, x, y, z, waiting}`.", NEW, None, false, |c, a| {
        Value::List(c.io().people_stops().into_iter().map(|s| rec(vec![("id", Value::Int(s.id)), ("name", s.name.trim().into()), ("x", s.pos[0].into()), ("y", s.pos[1].into()), ("z", s.pos[2].into()), ("waiting", s.waiting.into())])).collect())
    }),
    def!("people.waiting", "people", [p("stop", "integer")], "integer", "How many people wait at a stop (its map object id).", NEW, None, false, |c, a| {
        let id = a.int(0)?;
        Ok::<_, ApiError>(c.io().people_stops().into_iter().find(|s| s.id == id).map_or(0, |s| s.waiting))
    }),
    def!("people.density", "people", [], "number or nil", "The people setting: 0 to 3, 1 the map's own amount.", NEW, None, false, |c, a| c.io().people_density().map(Value::from)),
    def!("people.set_density", "people", [p("value", "number")], "boolean", "Changes the people setting (0 to 3).", NEW, TrafficWrite, false, |c, a| {
        let v = a.num(0)?.clamp(0.0, 3.0) as f32;
        Ok::<_, ApiError>(c.io().people_set_density(v))
    }),
];
