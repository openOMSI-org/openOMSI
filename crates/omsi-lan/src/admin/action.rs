//! An administration action - a line of the Administration menu, or what an admin sends a
//! dedicated server as `admin <action>` - read once into what it means (`Action::parse`), and
//! what it asks of the session itself (`act_on_session`): the same whoever administers it,
//! the host's own game or a dedicated server. What it asks of a game's own world - its
//! clock, its weather, its traffic, its own bus - is that game's (the app's `admin`, the
//! server's `ServerAdmin`).

use super::{beside, finite, notice_targets, parse_duty};
use glam::DVec3;
use omsi_net::LanSession;

/// Whose buses a service is for.
#[derive(Debug, Clone, PartialEq)]
pub enum Whom {
    /// Every player's, and the bus of the game that administers, if it has one.
    All,
    Player(u32),
}

/// What an administration does with the weather.
#[derive(Debug, Clone, PartialEq)]
pub enum WeatherAction {
    Next,
    /// The weather cycle on or off (a dedicated server takes it for `Next`).
    Cycle,
    /// This weather file (each side checks it is one it may choose).
    Set(String),
}

/// An administration action, from the text of a line (see the app's `admin` for the list).
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// A player sent away (`ban`: for the session), with what they read (`kick <id> [reason]`).
    Kick { id: u32, ban: bool, why: Option<String> },
    /// A player brought beside the bus of whoever asks.
    Bring(u32),
    /// Whoever asks goes beside a player's bus.
    Goto(u32),
    /// Everybody brought beside the bus of whoever asks, one behind the other.
    BringAll,
    /// The clock moved (s, at most a day either way).
    Time(f64),
    /// The clock's speed (x1 .. x30).
    Speed(f64),
    /// The clock set to this time of day (s).
    Clock(f64),
    Weather(WeatherAction),
    /// A chat line to everybody.
    Say(String),
    /// A word for one player only.
    Tell { id: u32, text: String },
    /// A duty for one player: the argument of the `duty` command (`admin::duty_arg`).
    Duty { id: u32, arg: String },
    /// A player's duty taken back.
    DutyOff(u32),
    /// A notice: for whom (`all` or a player's number), and the notice itself (`<id>
    /// <seconds> <info|warn|alert> <text>`).
    Notify { who: String, notice: String },
    /// The AI traffic: its argument (`clear`, a density, or the next step).
    Traffic(String),
    /// The passenger density of a dedicated server (`passengers <factor>`, as its
    /// `server.cfg`'s `passenger_density`).
    PassengerDensity(f32),
    /// A service (`repair`, `refuel`, `wash`).
    Service { kind: String, whom: Whom },
    /// A player's bus put back on its wheels.
    Unstick(u32),
}

impl Action {
    /// The action of a line of the menu (`kick 3`, `service repair all`, ...), or None.
    pub fn parse(text: &str) -> Option<Action> {
        let (verb, arg) = text.split_once(' ').unwrap_or((text, ""));
        let id = || arg.trim().parse::<u32>().ok();
        Some(match verb {
            "kick" | "ban" => {
                let mut words = arg.trim().splitn(2, ' ');
                let id = words.next()?.parse().ok()?;
                let why = words.next().map(str::trim).filter(|w| !w.is_empty()).map(str::to_string);
                Action::Kick { id, ban: verb == "ban", why }
            }
            "bring" => Action::Bring(id()?),
            "goto" => Action::Goto(id()?),
            "bringall" => Action::BringAll,
            "time" => Action::Time(finite(arg)?.clamp(-86400.0, 86400.0)),
            "speed" => Action::Speed(finite(arg)?.clamp(1.0, 30.0)),
            "clock" => Action::Clock(finite(arg)?.rem_euclid(86400.0)),
            "weather" => Action::Weather(match arg.trim().split_once(' ').map(|(a, b)| (a, b.trim())).unwrap_or((arg.trim(), "")) {
                ("cycle", _) => WeatherAction::Cycle,
                ("set", file) => WeatherAction::Set(file.to_string()),
                _ => WeatherAction::Next,
            }),
            "say" => Action::Say(arg.to_string()),
            "tell" => {
                let (who, text) = arg.trim().split_once(' ')?;
                Action::Tell { id: who.parse().ok()?, text: text.trim().to_string() }
            }
            "duty" => {
                let (who, rest) = arg.trim().split_once(' ')?;
                parse_duty(rest)?;
                Action::Duty { id: who.parse().ok()?, arg: rest.trim().to_string() }
            }
            "duty-off" => Action::DutyOff(id()?),
            "notify" => {
                let (who, notice) = arg.trim().split_once(' ')?;
                Action::Notify { who: who.to_string(), notice: notice.trim().to_string() }
            }
            "traffic" => Action::Traffic(arg.trim().to_string()),
            "passengers" => Action::PassengerDensity(crate::server_cfg::passenger_density(arg).ok()?),
            "service" => {
                let (kind, who) = arg.split_once(' ').unwrap_or((arg, "all"));
                if !matches!(kind, "repair" | "refuel" | "wash") {
                    return None;
                }
                let whom = if who == "all" { Whom::All } else { Whom::Player(who.trim().parse().ok()?) };
                Action::Service { kind: kind.to_string(), whom }
            }
            "unstick" => Action::Unstick(id()?),
            _ => return None,
        })
    }
}

/// Who administers: the host's own game (`id` None) or a dedicated server's admin.
pub struct Admin<'a> {
    pub id: Option<u32>,
    /// Where their bus stands (for `bring`, `bringall`).
    pub at: Option<(DVec3, f64)>,
    /// What a player sent away reads when no reason is given.
    pub sent_away: &'a str,
    /// Whether a notice is one a game can show.
    pub notice_ok: &'a dyn Fn(&str) -> bool,
}

/// What `act_on_session` left to the game that administers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Done {
    /// All of it was the session's.
    Session,
    /// The session's part is done; the action is for the administering game itself too (a
    /// notice for its own screen, a service for everybody's bus, its own included).
    AlsoHere,
    /// None of it was the session's: the game's own world (`goto`, the clock, the weather,
    /// the traffic).
    World,
}

/// What `action` asks of the session itself - players sent away, brought, told to take or
/// leave a duty, given a service, a notice or a word - done the same whoever administers.
pub fn act_on_session(lan: &mut LanSession, action: &Action, admin: &Admin) -> Done {
    let teleport = |lan: &mut LanSession, id: u32, (x, y): (f64, f64), pos: DVec3, h: f64| lan.command(id, &format!("teleport {x:.2} {y:.2} {:.2} {h:.1}", pos.z));
    match action {
        Action::Kick { id, ban, why } => {
            let why = why.as_deref().unwrap_or(if *ban { "sent away for this session" } else { admin.sent_away });
            lan.kick(*id, why, *ban);
        }
        Action::Bring(id) => {
            if let Some((pos, h)) = admin.at {
                teleport(lan, *id, beside(pos, h, 8.0), pos, h);
            }
        }
        Action::BringAll => {
            if let Some((pos, h)) = admin.at {
                let ids: Vec<u32> = lan.peers().map(|p| p.pose.id).filter(|id| *id != lan.my_id && Some(*id) != admin.id).collect();
                for (k, id) in ids.iter().enumerate() {
                    teleport(lan, *id, beside(pos, h, 5.0 * (k as f64 + 1.0)), pos, h);
                }
            }
        }
        Action::Say(text) => {
            let _ = lan.say(text);
        }
        Action::Tell { id, text } => {
            if let Err(e) = lan.say_to(*id, "Admin (private)", text) {
                log::info!("admin: tell {id}: {e}");
            }
        }
        Action::Duty { id, arg } => lan.command(*id, &format!("duty {arg}")),
        Action::DutyOff(id) => lan.command(*id, "duty-off"),
        Action::Notify { who, notice } => {
            if !(admin.notice_ok)(notice) {
                return Done::Session;
            }
            let (ids, here) = notice_targets(who, lan.peers().map(|p| p.pose.id), lan.my_id);
            for id in ids {
                lan.command(id, &format!("notify {notice}"));
            }
            if here {
                return Done::AlsoHere;
            }
        }
        Action::Service { kind, whom } => {
            let ids: Vec<u32> = match whom {
                Whom::All => lan.peers().map(|p| p.pose.id).filter(|id| *id != lan.my_id).collect(),
                Whom::Player(id) => vec![*id],
            };
            for id in ids {
                lan.command(id, &format!("service {kind}"));
            }
            if *whom == Whom::All {
                return Done::AlsoHere;
            }
        }
        Action::Unstick(id) => lan.command(*id, "unstick"),
        Action::Goto(_) | Action::Time(_) | Action::Speed(_) | Action::Clock(_) | Action::Weather(_) | Action::Traffic(_) | Action::PassengerDensity(_) => return Done::World,
    }
    Done::Session
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every line of the menu reads as the action it names; numbers held to their ranges, and
    /// a line that is not one is no action.
    #[test]
    fn a_line_reads_as_its_action() {
        assert_eq!(Action::parse("kick 3"), Some(Action::Kick { id: 3, ban: false, why: None }));
        assert_eq!(Action::parse("ban 4 spam"), Some(Action::Kick { id: 4, ban: true, why: Some("spam".into()) }));
        assert_eq!(Action::parse("time 999999"), Some(Action::Time(86400.0)));
        assert_eq!(Action::parse("speed 0.5"), Some(Action::Speed(1.0)));
        assert_eq!(Action::parse("clock 90000"), Some(Action::Clock(3600.0)));
        assert_eq!(Action::parse("time NaN"), None, "NaN would stop everybody's clock");
        assert_eq!(Action::parse("weather cycle"), Some(Action::Weather(WeatherAction::Cycle)));
        assert_eq!(Action::parse("weather set Weather/Rain.owt"), Some(Action::Weather(WeatherAction::Set("Weather/Rain.owt".into()))));
        assert_eq!(Action::parse("weather next"), Some(Action::Weather(WeatherAction::Next)));
        assert_eq!(Action::parse("service wash all"), Some(Action::Service { kind: "wash".into(), whom: Whom::All }));
        assert_eq!(Action::parse("service repair 7"), Some(Action::Service { kind: "repair".into(), whom: Whom::Player(7) }));
        assert_eq!(Action::parse("service paint all"), None);
        assert_eq!(Action::parse("duty 5 12 A 1 0"), Some(Action::Duty { id: 5, arg: "12 A 1 0".into() }));
        assert_eq!(Action::parse("duty 5 12"), None, "not a duty's argument");
        assert_eq!(Action::parse("tell 2 hello there"), Some(Action::Tell { id: 2, text: "hello there".into() }));
        assert_eq!(Action::parse("notify all 9 5 info Hi"), Some(Action::Notify { who: "all".into(), notice: "9 5 info Hi".into() }));
        assert_eq!(Action::parse("passengers 1.5"), Some(Action::PassengerDensity(1.5)));
        assert_eq!(Action::parse("passengers 0"), Some(Action::PassengerDensity(0.0)));
        for bad in ["passengers", "passengers -1", "passengers 4", "passengers NaN", "passengers on"] {
            assert_eq!(Action::parse(bad), None, "{bad}");
        }
        assert_eq!(Action::parse("goto x"), None);
        assert_eq!(Action::parse("dance"), None);
    }
}
