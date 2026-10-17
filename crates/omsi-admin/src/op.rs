//! The operations an admin asks for, read once and checked (`Op::from_json`): numbers in
//! their ranges, texts cleaned and cut to size, a player named by number *and* connection
//! (`Target`), so that an operation meant for one player never reaches whoever has that
//! number later. Text is never read as a command: a reason or a chat line is a value of its
//! operation and nothing else.

use crate::perm::Perm;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A player as an admin picked them: the number the session gave them and the connection it
/// was given to (`omsi_net::Peer::conn`). A player who left and came back, or another one
/// with the same number, is another connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Target {
    pub id: u32,
    pub conn: u64,
}

impl Target {
    /// `<id>#<conn>`, as the game's Administration menu sends it.
    pub fn parse_tag(s: &str) -> Option<Target> {
        let (id, conn) = s.trim().split_once('#')?;
        Some(Target { id: id.parse().ok()?, conn: conn.parse().ok()? })
    }

    pub fn tag(&self) -> String {
        format!("{}#{}", self.id, self.conn)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum WeatherOp {
    /// The next installed weather.
    Next,
    /// This weather file (`Weather/….owt`; whether it is installed, the server checks).
    Set(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrafficOp {
    Density(usize),
    /// Every AI car off the road (the timetable's buses stay).
    Clear,
}

/// An administration operation.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    /// The player sent away, with what they read.
    Kick { target: Target, reason: Option<String> },
    /// Sent away and kept out for the rest of this server run (by the game's nonce and the
    /// address it came from: no identity that outlives the run, see SERVER.md).
    Ban { target: Target, reason: Option<String> },
    /// A chat line to everybody.
    Say(String),
    /// A word for one player only.
    Tell { target: Target, text: String },
    /// The clock set to this time of day (s, 0 .. 86400).
    Clock(f64),
    /// The clock moved (s, at most a day either way).
    Shift(f64),
    /// The clock's speed (x1 .. x30).
    Speed(f64),
    Weather(WeatherOp),
    Traffic(TrafficOp),
    /// The passenger density (see `passenger_density`).
    PassengerDensity(f32),
}

/// The longest reason a player sent away reads.
pub const MAX_REASON: usize = 120;
/// The longest chat line (the session's own limit).
pub const MAX_TEXT: usize = 160;

/// The highest passenger density (the game's own passengers setting goes to 3 as well).
pub const PASSENGER_DENSITY_MAX: f32 = 3.0;

/// A passenger density as `server.cfg` and the admins give it: a number from 0 to
/// `PASSENGER_DENSITY_MAX`. Anything else is refused with the reason, never bent into range:
/// a typo should not quietly fill the map or empty it.
pub fn passenger_density(text: &str) -> Result<f32, String> {
    let v = text.trim().parse::<f32>().map_err(|_| format!("'{}' is not a number", text.trim().chars().take(20).collect::<String>()))?;
    density_ok(v)
}

fn density_ok(v: f32) -> Result<f32, String> {
    if !v.is_finite() {
        return Err("not a finite number".into());
    }
    if v < 0.0 {
        return Err("cannot be negative".into());
    }
    if v > PASSENGER_DENSITY_MAX {
        return Err(format!("at most {PASSENGER_DENSITY_MAX}"));
    }
    Ok(v)
}

/// A weather file an admin may choose: a `Weather/….owt` path, nothing above it.
pub fn weather_file_ok(file: &str) -> bool {
    let f = file.replace('\\', "/").to_ascii_lowercase();
    f.starts_with("weather/") && f.ends_with(".owt") && !f.contains("..") && f.matches('/').count() == 1
}

/// A text as an operation keeps it: no control characters (a line break is no second
/// command, a tab no column), runs of spaces as one, at most `max` characters.
pub fn clean(text: &str, max: usize) -> String {
    let spaced: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    spaced.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(max).collect::<String>().trim_end().to_string()
}

impl Op {
    /// What the operation needs.
    pub fn perm(&self) -> Perm {
        match self {
            Op::Kick { .. } | Op::Ban { .. } | Op::Say(_) | Op::Tell { .. } => Perm::Moderate,
            Op::Clock(_) | Op::Shift(_) | Op::Speed(_) | Op::Weather(_) | Op::Traffic(_) | Op::PassengerDensity(_) => Perm::World,
        }
    }

    /// Its name in the API (`"op"`) and in the records.
    pub fn verb(&self) -> &'static str {
        match self {
            Op::Kick { .. } => "kick",
            Op::Ban { .. } => "ban",
            Op::Say(_) => "say",
            Op::Tell { .. } => "tell",
            Op::Clock(_) => "clock",
            Op::Shift(_) => "shift",
            Op::Speed(_) => "speed",
            Op::Weather(_) => "weather",
            Op::Traffic(_) => "traffic",
            Op::PassengerDensity(_) => "passenger_density",
        }
    }

    pub fn target(&self) -> Option<Target> {
        match self {
            Op::Kick { target, .. } | Op::Ban { target, .. } | Op::Tell { target, .. } => Some(*target),
            _ => None,
        }
    }

    /// One line for the records (the texts as they are kept: cleaned, short).
    pub fn summary(&self) -> String {
        let why = |r: &Option<String>| r.as_deref().map(|r| format!(": {r}")).unwrap_or_default();
        match self {
            Op::Kick { target, reason } => format!("send away player {}{}", target.id, why(reason)),
            Op::Ban { target, reason } => format!("send away player {} for this run{}", target.id, why(reason)),
            Op::Say(t) => format!("say: {t}"),
            Op::Tell { target, text } => format!("tell player {}: {text}", target.id),
            Op::Clock(s) => format!("clock {:02}:{:02}", (*s as i64) / 3600, (*s as i64) / 60 % 60),
            Op::Shift(s) => format!("clock {:+.0} min", s / 60.0),
            Op::Speed(f) => format!("time speed x{f}"),
            Op::Weather(WeatherOp::Next) => "next weather".into(),
            Op::Weather(WeatherOp::Set(f)) => format!("weather {f}"),
            Op::Traffic(TrafficOp::Density(n)) => format!("traffic {n}"),
            Op::Traffic(TrafficOp::Clear) => "clear the AI traffic".into(),
            Op::PassengerDensity(d) => format!("passenger density {d}"),
        }
    }

    /// The operation of an API request's body (`{"op": "kick", "target": {"id": 3, "conn":
    /// 17}, "reason": "…"}`, see SERVER.md for each), checked; the reason it is refused
    /// otherwise. `traffic_max`: the highest traffic density the game allows.
    pub fn from_json(v: &Value, traffic_max: usize) -> Result<Op, String> {
        let verb = v.get("op").and_then(Value::as_str).ok_or("no \"op\"")?;
        let num = |k: &str| -> Result<f64, String> {
            let x = v.get(k).and_then(Value::as_f64).ok_or_else(|| format!("\"{k}\": a number is needed"))?;
            if x.is_finite() {
                Ok(x)
            } else {
                Err(format!("\"{k}\": not a finite number"))
            }
        };
        let text = |k: &str, max: usize| -> Option<String> { v.get(k).and_then(Value::as_str).map(|t| clean(t, max)).filter(|t| !t.is_empty()) };
        let target = || -> Result<Target, String> {
            let t = v.get("target").ok_or("\"target\": the player ({\"id\", \"conn\"}) is needed")?;
            let id = t.get("id").and_then(Value::as_u64).filter(|id| *id <= u32::MAX as u64).ok_or("\"target.id\": a player number is needed")?;
            let conn = t.get("conn").and_then(Value::as_u64).ok_or("\"target.conn\": the player's connection is needed (from the players' list)")?;
            Ok(Target { id: id as u32, conn })
        };
        let in_range = |k: &str, lo: f64, hi: f64| -> Result<f64, String> {
            let x = num(k)?;
            if (lo..=hi).contains(&x) {
                Ok(x)
            } else {
                Err(format!("\"{k}\": from {lo} to {hi}"))
            }
        };
        Ok(match verb {
            "kick" => Op::Kick { target: target()?, reason: text("reason", MAX_REASON) },
            "ban" => Op::Ban { target: target()?, reason: text("reason", MAX_REASON) },
            "say" => Op::Say(text("text", MAX_TEXT).ok_or("\"text\": nothing to say")?),
            "tell" => Op::Tell { target: target()?, text: text("text", MAX_TEXT).ok_or("\"text\": nothing to say")? },
            "clock" => {
                let s = in_range("seconds", 0.0, 86_400.0)?;
                Op::Clock(s % 86_400.0)
            }
            "shift" => Op::Shift(in_range("seconds", -86_400.0, 86_400.0)?),
            "speed" => Op::Speed(in_range("factor", 1.0, 30.0)?),
            "weather" => match v.get("file").and_then(Value::as_str) {
                Some(f) if weather_file_ok(f) => Op::Weather(WeatherOp::Set(f.replace('\\', "/"))),
                Some(_) => return Err("\"file\": a Weather/….owt file".into()),
                None if v.get("next").and_then(Value::as_bool) == Some(true) => Op::Weather(WeatherOp::Next),
                None => return Err("\"file\" or \"next\": true is needed".into()),
            },
            "traffic" => {
                if v.get("clear").and_then(Value::as_bool) == Some(true) {
                    Op::Traffic(TrafficOp::Clear)
                } else {
                    let n = in_range("density", 0.0, traffic_max as f64)?;
                    if n.fract() != 0.0 {
                        return Err("\"density\": a whole number".into());
                    }
                    Op::Traffic(TrafficOp::Density(n as usize))
                }
            }
            "passenger_density" => Op::PassengerDensity(density_ok(num("value")? as f32).map_err(|e| format!("\"value\": {e}"))?),
            other => return Err(format!("unknown operation '{}'", clean(other, 32))),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_density_is_a_number_from_0_to_the_maximum() {
        assert_eq!(passenger_density("1"), Ok(1.0));
        assert_eq!(passenger_density(" 0.5 "), Ok(0.5));
        assert_eq!(passenger_density("0"), Ok(0.0));
        assert_eq!(passenger_density("3"), Ok(PASSENGER_DENSITY_MAX));
        for bad in ["", "abc", "-0.1", "3.01", "50", "NaN", "inf", "-inf", "1e40", "1;say hi"] {
            assert!(passenger_density(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn only_a_weather_file() {
        assert!(weather_file_ok("Weather/#CAVOK.owt"));
        assert!(weather_file_ok("weather\\Bodennebel.OWT"));
        assert!(!weather_file_ok("Weather/../server.cfg"));
        assert!(!weather_file_ok("Weather/sub/x.owt"));
        assert!(!weather_file_ok("maps/x.owt"));
        assert!(!weather_file_ok("Weather/x.cfg"));
    }

    /// A text stays one value: line breaks and other control characters are spaces, and it
    /// is cut to size; markup is kept as text (the panels show it as text, never as HTML).
    #[test]
    fn a_text_is_one_clean_value() {
        assert_eq!(clean("spam\nkick 3\r\n", 120), "spam kick 3");
        assert_eq!(clean("  a \t b  ", 120), "a b");
        assert_eq!(clean("<script>alert(1)</script>", 120), "<script>alert(1)</script>");
        assert_eq!(clean(&"x".repeat(500), 120).len(), 120);
        assert_eq!(clean("\u{7}\u{1b}[31m", 120), "[31m");
    }

    #[test]
    fn a_target_tag_goes_and_comes_back() {
        let t = Target { id: 3, conn: 17 };
        assert_eq!(Target::parse_tag(&t.tag()), Some(t));
        for bad in ["3", "#17", "3#", "x#1", "3#-1", "3 17"] {
            assert_eq!(Target::parse_tag(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_request_reads_as_its_operation() {
        let t = Target { id: 3, conn: 17 };
        let op = |v: Value| Op::from_json(&v, 120);
        assert_eq!(op(json!({"op": "kick", "target": {"id": 3, "conn": 17}, "reason": " spam\nkick 4 "})), Ok(Op::Kick { target: t, reason: Some("spam kick 4".into()) }));
        assert_eq!(op(json!({"op": "kick", "target": {"id": 3, "conn": 17}})), Ok(Op::Kick { target: t, reason: None }));
        assert_eq!(op(json!({"op": "ban", "target": {"id": 3, "conn": 17}, "reason": ""})), Ok(Op::Ban { target: t, reason: None }));
        assert_eq!(op(json!({"op": "say", "text": "hello"})), Ok(Op::Say("hello".into())));
        assert_eq!(op(json!({"op": "clock", "seconds": 30600})), Ok(Op::Clock(30600.0)));
        assert_eq!(op(json!({"op": "speed", "factor": 2})), Ok(Op::Speed(2.0)));
        assert_eq!(op(json!({"op": "weather", "file": "Weather/#CAVOK.owt"})), Ok(Op::Weather(WeatherOp::Set("Weather/#CAVOK.owt".into()))));
        assert_eq!(op(json!({"op": "weather", "next": true})), Ok(Op::Weather(WeatherOp::Next)));
        assert_eq!(op(json!({"op": "traffic", "density": 40})), Ok(Op::Traffic(TrafficOp::Density(40))));
        assert_eq!(op(json!({"op": "traffic", "clear": true})), Ok(Op::Traffic(TrafficOp::Clear)));
        assert_eq!(op(json!({"op": "passenger_density", "value": 1.5})), Ok(Op::PassengerDensity(1.5)));
        // what is refused, with the reason
        for bad in [
            json!({"op": "kick", "target": {"id": 3}}),
            json!({"op": "kick", "target": 3}),
            json!({"op": "kick"}),
            json!({"op": "say", "text": "  "}),
            json!({"op": "clock", "seconds": -1}),
            json!({"op": "clock", "seconds": "08:00"}),
            json!({"op": "speed", "factor": 0.5}),
            json!({"op": "speed", "factor": 31}),
            json!({"op": "weather", "file": "Weather/../server.cfg"}),
            json!({"op": "weather"}),
            json!({"op": "traffic", "density": 121}),
            json!({"op": "traffic", "density": 2.5}),
            json!({"op": "passenger_density", "value": -1}),
            json!({"op": "passenger_density", "value": 4}),
            json!({"op": "passenger_density"}),
            json!({"op": "promote", "role": "admin"}),
            json!({"no": "op"}),
        ] {
            assert!(op(bad.clone()).is_err(), "{bad}");
        }
    }

    #[test]
    fn each_operation_needs_its_permission() {
        let t = Target { id: 1, conn: 1 };
        assert_eq!(Op::Kick { target: t, reason: None }.perm(), Perm::Moderate);
        assert_eq!(Op::Say("x".into()).perm(), Perm::Moderate);
        assert_eq!(Op::PassengerDensity(1.0).perm(), Perm::World);
        assert_eq!(Op::Weather(WeatherOp::Next).perm(), Perm::World);
        assert_eq!(Op::Traffic(TrafficOp::Clear).perm(), Perm::World);
    }
}
