//! Duties and turning them into command line arguments for the game.
use super::*;

#[derive(Deserialize, Default, Debug, Clone)]
pub struct Duty {
    pub map: String,
    pub bus: String,
    pub paint: Option<String>,
    /// The number plate (registration) the player typed: it wins over the plate the bus's
    /// `[number]` list or the map's `registrations.txt` gives it (empty: as the content says).
    #[serde(default)]
    pub plate: Option<String>,
    /// The fleet number picked from the bus's `[number]` list (none: its first).
    #[serde(default)]
    pub number: Option<String>,
    /// The variables the player chose for this bus in the launcher - its onboard computer (the
    /// IBIS unit or ticket printer its repaints name), a door type: set before the scripts'
    /// `{init}` and kept over the paint scheme's own `[setvar]`s (`--setvar`, #4077).
    #[serde(default)]
    pub setvars: Option<std::collections::BTreeMap<String, String>>,
    pub hof: Option<String>,
    pub entry: Option<i32>,
    pub line: Option<String>,
    pub tour: Option<String>,
    /// The trip of the tour to start with: its departure (HH:MM) or its place in the tour.
    #[serde(default)]
    pub trip: Option<String>,
    #[serde(default)]
    pub whole_tour: bool,
    /// HH:MM
    pub time: String,
    /// YYYY-MM-DD
    pub date: Option<String>,
    pub weather: Option<String>,
    pub traffic: Option<u32>,
    pub passengers: Option<bool>,
    pub schedule: Option<bool>,
    pub autostart: Option<bool>,
    pub comfortable_cabin_on_start: Option<bool>,
    /// Start as a pedestrian beside the bus (which is then left out): a bus is placed or
    /// taken over from the game menu later.
    #[serde(default)]
    pub on_foot: Option<bool>,
    pub profile: Option<String>,
    /// LAN play: "host", or "join:<session code | ip[:port] | port | empty = search>".
    pub lan: Option<String>,
    /// The name the other LAN players see (default: the profile).
    pub lan_name: Option<String>,
    /// Discovered web gateway to try if the initial UDP join gets no answer (Auto only).
    pub lan_ws_fallback: Option<String>,
    /// Explicit per-launch authorization for server-defined vehicle content.
    #[serde(default)]
    pub allow_server_fleet: bool,
    /// A hosted session's buses (#1464): the ones its players may drive, `;`-separated, bus
    /// files or folders. Empty: every bus.
    #[serde(default)]
    pub session_buses: Option<String>,
    /// Season override: spring / summer / autumn / winter, with a phase `-early` / `-late`
    /// (`autumn-late`; none = the middle; empty = by date).
    pub season: Option<String>,
    /// One of OMSI's tutorials (1..4): its own situation, nothing else of the duty.
    #[serde(default)]
    pub tutorial: Option<usize>,
    /// A situation file to continue (the map's `laststn.osn`): nothing else of the duty.
    #[serde(default)]
    pub situation: Option<String>,
    /// The depot bus driven: it starts with the wear it was left with (see [`depot`]).
    #[serde(default)]
    pub depot: Option<String>,
}

/// The command line a duty becomes.
pub fn duty_args(d: &Duty) -> Result<Vec<String>> {
    let root = root()?;
    duty_args_for_root(d, &root)
}

// The installation is validated by duty_args; argument tests supply their own path.
pub(crate) fn duty_args_for_root(d: &Duty, root: &Path) -> Result<Vec<String>> {
    duty_args_with_settings(d, root, &get_settings().unwrap_or_default())
}

pub(crate) fn duty_args_with_settings(d: &Duty, root: &Path, st: &Value) -> Result<Vec<String>> {
    let mut start = vec![
        "--root".into(),
        root.to_string_lossy().to_string(),
        "--no-menu".into(),
    ];
    if st
        .get("disable_plugins")
        .and_then(Value::as_bool)
        .unwrap_or(true)
    {
        start.push("--no-plugins".into());
    }
    if let Some(t) = d.tutorial {
        start.extend(["--tutorial".into(), t.to_string()]);
        return Ok(start);
    }
    if let Some(sit) = d.situation.as_deref().filter(|s| !s.trim().is_empty()) {
        let mut a = start;
        a.extend(["--situation".into(), sit.to_string()]);
        if let Some(p) = d.profile.as_deref().filter(|p| !p.trim().is_empty()) {
            a.extend(["--driver".into(), format!("Drivers/{}.odr", p.trim())]);
        }
        // the traffic and the people as for any other start: a situation file keeps the
        // vehicles the player placed, not how busy the streets are (OMSI takes that from its
        // options), and without these a continued session had the timetable buses alone -
        // no cars, nobody at the stops (#136)
        a.extend(["--traffic".into(), d.traffic.unwrap_or(30).to_string()]);
        if d.passengers.unwrap_or(true) {
            a.push("--passengers".into());
        }
        return Ok(a);
    }
    let mut a = start;
    a.extend([
        "--map".into(),
        d.map.clone(),
        "--bus".into(),
        d.bus.clone(),
        "--time".into(),
        if d.time.trim().is_empty() {
            "09:00".into()
        } else {
            d.time.trim().to_string()
        },
    ]);
    if let Some(p) = d.paint.as_deref().filter(|p| !p.trim().is_empty()) {
        a.extend(["--paint".into(), p.trim().to_string()]);
    }
    if let Some(pl) = d.plate.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        a.extend(["--plate".into(), pl.to_string()]);
    }
    if let Some(n) = d.number.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        a.extend(["--number".into(), n.to_string()]);
    }
    if let Some(vars) = d.setvars.as_ref().filter(|v| !v.is_empty()) {
        let list: Vec<String> = vars
            .iter()
            .filter(|(k, _)| !k.trim().is_empty())
            .map(|(k, v)| format!("{}={}", k.trim(), v.trim()))
            .collect();
        if !list.is_empty() {
            a.extend(["--setvar".into(), list.join(",")]);
        }
    }
    if let Some(id) = d.depot.as_deref().map(str::trim).filter(|i| !i.is_empty()) {
        a.extend(["--depot".into(), id.to_string()]);
    }
    // (a vehicle file taken for a depot from a broken ailists.cfg by older launchers is none)
    if let Some(h) = d.hof.as_deref().filter(|h| {
        !h.trim().is_empty()
            && !h.to_ascii_lowercase().contains(".bus")
            && !h.to_ascii_lowercase().contains(".ovh")
    }) {
        a.extend(["--hof".into(), h.trim().to_string()]);
    }
    // the entry point's place in the map's list; -1: the one nearest to the duty's first
    // stop (a free drive takes the list's first then)
    match d.entry {
        Some(e) if e < 0 => {
            if d.line
                .as_deref()
                .map(|l| !l.trim().is_empty())
                .unwrap_or(false)
            {
                a.push("--auto-entry".into());
            }
        }
        Some(e) => a.extend(["--entry".into(), e.to_string()]),
        None => {}
    }
    // OMSI's [useActTime] / [useActDate] / [useActYear]: the machine's clock and calendar
    // instead of the duty's (the year only on its own switch - a map's timetable is for
    // its years)
    let on = |k: &str| st.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
    let now = local_now();
    if let (true, Some((_, _, _, h, m))) = (on("use_real_time"), now) {
        if let Some(i) = a.iter().position(|x| x == "--time") {
            a[i + 1] = format!("{h:02}:{m:02}");
        }
        // (a duty picked for later starts now, not ten minutes before its first trip)
        a.push("--keep-time".into());
    }
    let mut date = d
        .date
        .as_deref()
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty());
    if let (true, Some((y, mo, dd, _, _))) = (on("use_real_date"), now) {
        let year = if on("use_real_year") {
            y
        } else {
            date.as_deref()
                .and_then(|x| x.split('-').next()?.parse::<i32>().ok())
                .unwrap_or(y)
        };
        date = Some(format!("{year:04}-{mo:02}-{dd:02}"));
    }
    if let Some(dt) = date {
        // (the year stays the duty's: Omsi.exe's [useActDate] takes the day alone from the
        // computer's calendar and leaves the year as it is, 0x708780; a map's [years] only
        // pick the year of a random start, 0x678518. Fitted to them, a chrono scenario of
        // 2021 on a map whose [years] end in 2018 was never reached, #3283)
        a.extend(["--date".into(), dt]);
    }
    if let Some(w) = d.weather.as_deref().filter(|x| !x.trim().is_empty()) {
        a.extend(["--weather".into(), w.trim().to_string()]);
    }
    a.extend(["--traffic".into(), d.traffic.unwrap_or(30).to_string()]);
    if d.passengers.unwrap_or(true) {
        a.push("--passengers".into());
    }
    // (a duty needs the timetable even with the timetable buses switched off: the game
    // then runs it for the player alone, #1762)
    let schedule = d.schedule.unwrap_or(true) || d.line.is_some();
    if schedule {
        a.push("--schedule".into());
        if d.schedule == Some(false) {
            a.push("--no-timetable-buses".into());
        }
    }
    if let (Some(l), true) = (d.line.as_deref().filter(|x| !x.trim().is_empty()), schedule) {
        a.extend(["--line".into(), l.trim().to_string()]);
        if let Some(t) = d.tour.as_deref().filter(|x| !x.trim().is_empty()) {
            a.extend(["--tour".into(), t.trim().to_string()]);
            if let Some(tr) = d.trip.as_deref().filter(|x| !x.trim().is_empty()) {
                a.extend(["--trip".into(), tr.trim().to_string()]);
                if d.whole_tour {
                    a.push("--whole-tour".into());
                }
            }
        }
    }
    if d.autostart.unwrap_or(false) {
        a.push("--autostart".into());
    }
    if d.comfortable_cabin_on_start.unwrap_or(false) {
        a.push("--comfortable-cabin-on-start".into());
    }
    if d.on_foot.unwrap_or(false) {
        a.push("--on-foot".into());
    }
    let profile = d
        .profile
        .clone()
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| load_config().profile);
    if let Some(season) = d
        .season
        .as_deref()
        .map(str::trim)
        .filter(|x| !x.is_empty() && !x.eq_ignore_ascii_case("auto"))
    {
        a.extend(["--season".into(), season.to_ascii_lowercase()]);
    }
    if let Some(lan) = d
        .lan
        .as_deref()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.eq_ignore_ascii_case("off"))
    {
        if lan.eq_ignore_ascii_case("host") {
            // 0: the default port, or the next free one when a session runs here already
            a.extend(["--lan-host".into(), "0".into()]);
            // (#1464) the buses this session drives
            if let Some(buses) = d.session_buses.as_deref().filter(|b| !b.trim().is_empty()) {
                a.extend(["--session-buses".into(), buses.trim().to_string()]);
            }
        } else if let Some(target) = lan.strip_prefix("join:") {
            let target = target.trim();
            omsi_net::describe_join(target).map_err(|e| anyhow!("LAN join: {e}"))?;
            a.extend([
                "--lan-join".into(),
                if target.is_empty() {
                    "auto".into()
                } else {
                    target.to_string()
                },
            ]);
            if let Some(url) = d.lan_ws_fallback.as_deref().filter(|url| !url.is_empty()) {
                a.extend(["--lan-ws-fallback".into(), url.to_string()]);
            }
            if d.allow_server_fleet {
                a.push("--allow-server-fleet".into());
            }
        } else {
            return Err(anyhow!(
                "LAN play: '{lan}' is neither host nor join:<code or address>"
            ));
        }
        let name = d
            .lan_name
            .clone()
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| profile.clone());
        if !name.trim().is_empty() {
            a.extend(["--lan-name".into(), name.trim().to_string()]);
        }
    }
    if !profile.trim().is_empty() {
        a.extend(["--driver".into(), format!("Drivers/{}.odr", profile.trim())]);
    }
    Ok(a)
}
