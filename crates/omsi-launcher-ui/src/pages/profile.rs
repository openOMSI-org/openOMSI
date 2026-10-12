//! The Profile page: the driver's personnel file.

use super::*;
use omsi_launcher_lib as core;

// --- the pages' heads -----------------------------------------------------------------------

/// A page's icon and what it is for: the sheet's head on a desktop, the line under the bar on
/// a phone (the page itself draws neither: its name was drawn twice, in the bar and on it).
pub fn about(page: Page, controls_tab: usize) -> (&'static str, &'static str) {
    match page {
        Page::Profile => ("badge", "Everything the logbook kept, from the first duty to the last."),
        Page::Settings => ("settings", "Every change is saved at once; the game reads them when it starts."),
        Page::Controls if controls_tab == 0 => ("keyboard", "Click a key and press the new one (hold Shift, Ctrl or Alt for a combination); Escape leaves it as it is."),
        Page::Controls => ("sports_esports", "What each axis and button of a wheel, pedals or joystick does - OMSI 2's gamectrler.cfg, kept in the content folder."),
        Page::Sessions => ("sports_esports", "The games you started, and who drives with you."),
        Page::Mods => ("extension", "A bus, a map, scenery, a whole OMSI folder - as a folder or a .zip, .7z or .rar. The original OMSI 2 folder is never written to."),
        Page::Multiplayer => ("groups", "Drive with friends by a code, or on servers that are always on."),
        Page::Timetable => ("schedule", "A map's lines: their tours and when each trip leaves. Saved as the line's .ttl (in the content folder; OMSI 2's own files stay as they are)."),
        Page::Tutorials => ("help", "OMSI 2's own lessons: each opens its situation, with its pages beside the picture (Enter turns the page)."),
        Page::Setup => ("folder_open", "Where the original game and this one are."),
        Page::Editor => ("construction", "Make the map your own: lines of your own, liveries, the timetable and the map's objects."),
        Page::Lines => ("route", "Click the stops in order: the way between them is found over the roads. Saved, the line is driven by you and by the timetable's buses."),
        Page::Depots => ("departure_board", "Your own depot files: the destinations and what every display shows, the IBIS's stops and routes, special and service trips. A line chooses one, and every bus that drives it gets it."),
        Page::Livery => ("livery_fill", "Paint a bus in 3D: colours, stripes, texts, pictures and shapes, saved as a livery the game offers."),
        Page::Company => ("garage", "Your own transport company: buses, people and lines, in a time of its own."),
        Page::Drive => ("directions_bus", ""),
    }
}

/// What a page keeps in its sheet's head, right-aligned in `r`: the drivers on the service
/// record. Returns where it begins (`r`'s right edge when there is nothing).
pub fn head_tools(l: &mut Launcher, page: Page, r: Rect) -> f32 {
    match page {
        Page::Profile => driver_tools(l, r),
        Page::Company => crate::company::head_tools(l, r),
        _ => r.right(),
    }
}

// --- the drivers ----------------------------------------------------------------------------

/// The new driver's name field (one at a time: on the drivers step or on the service record).
const NEW_NAME: &str = "driver-new-name";

/// Start making a driver: the field empty, the typing in it.
pub fn open_new_driver(l: &mut Launcher) {
    l.pages.new_driver_open = true;
    l.pages.new_driver.clear();
    l.ui.focus = Some(id_of(NEW_NAME));
}

/// Make the driver whose name was typed (OMSI's own personnel file, as the game makes one)
/// and choose them.
pub fn create_driver(l: &mut Launcher) {
    let name = l.pages.new_driver.trim().to_string();
    if name.is_empty() {
        return;
    }
    match core::create_profile(&name, "M") {
        Ok(_) => {
            l.state.config.profile = name.clone();
            let _ = core::save_config(&l.state.config);
            l.pages.new_driver.clear();
            l.pages.new_driver_open = false;
            l.state.load_profiles();
            l.state.touched();
            l.state.set_status(omsi_ui::tr("Driver %{name} created.").replace("%{name}", &name), false);
        }
        Err(e) => l.state.set_status(format!("{e:#}"), true),
    }
}

/// The tile of a driver being made, after the others on the drivers step: the name typed into
/// it, Create (or Enter) makes them, the cross (or Escape) leaves it. Some(true) to make them,
/// Some(false) to give up.
pub fn new_driver_tile(ui: &mut Ui, tile: Rect, name: &mut String) -> Option<bool> {
    let focused = ui.focus == Some(id_of(NEW_NAME));
    let enter = focused && ui.input.keys.contains(&Key::Enter);
    let escape = focused && ui.input.keys.contains(&Key::Escape);
    ui.solid(tile);
    ui.p().rounded(tile, SHEET_RADIUS, PANEL);
    ui.p().rounded_border(tile, SHEET_RADIUS, 2.0, accent());
    let mono = Rect::new(tile.x + 17.0, tile.y + 17.0, 44.0, 44.0);
    ui.p().rounded(mono, RADIUS, accent().alpha(0.3));
    match name.trim().chars().next() {
        Some(c) => {
            ui.text_in(&c.to_uppercase().to_string(), mono, 18.0, Weight::Bold, TEXT, Align::Center);
        }
        None => ui.icon("add", mono.center(), 22.0, TEXT),
    }
    ui.text_in("New driver", Rect::new(mono.right() + 12.0, mono.y, tile.right() - mono.right() - 56.0, mono.h), 15.0, Weight::Bold, TEXT, Align::Left);
    let close = Rect::new(tile.right() - 42.0, tile.y + 14.0, 28.0, 28.0);
    let (hc, _, cancel) = ui.interact(id_of("driver-new-cancel"), close);
    ui.icon("close", close.center(), 18.0, if hc { TEXT } else { TEXT_DIM });
    ui.tooltip(close, "Cancel");
    ui.text_input(NEW_NAME, Rect::new(tile.x + 17.0, tile.y + 74.0, tile.w - 34.0, 34.0), name, "The new driver's name", None);
    let create = ui.button("driver-new-create", Rect::new(tile.x + 17.0, tile.y + 116.0, tile.w - 34.0, 30.0), "Create", Some("add"), ButtonKind::Primary);
    if (create || enter) && !name.trim().is_empty() {
        Some(true)
    } else if cancel || escape {
        Some(false)
    } else {
        None
    }
}

/// The drivers on this computer, right-aligned in the row `r`: whose record it is, a new one
/// (typed in place), and deleting the chosen one (pressed twice). Returns where they begin.
pub fn driver_tools(l: &mut Launcher, r: Rect) -> f32 {
    let h = r.h;
    if l.pages.new_driver_open {
        let cancel = Rect::new(r.right() - h, r.y, h, h);
        let cw = l.ui.width("Create", 13.0, Weight::Bold) + 60.0;
        let create = Rect::new(cancel.x - 8.0 - cw, r.y, cw, h);
        let fw = (create.x - 8.0 - r.x).clamp(0.0, 260.0);
        let field = Rect::new(create.x - 8.0 - fw, r.y, fw, h);
        new_driver_row(l, field, create, cancel);
        return field.x;
    }
    let dw = l.ui.width(delete_label(l), 13.0, Weight::Bold) + 56.0;
    let del = Rect::new(r.right() - dw, r.y, dw, h);
    let nw = l.ui.width("New driver", 13.0, Weight::Bold) + 56.0;
    let new = Rect::new(del.x - 8.0 - nw, r.y, nw, h);
    let sw = (new.x - 8.0 - r.x).clamp(0.0, 220.0);
    let pick = Rect::new(new.x - 8.0 - sw, r.y, sw, h);
    driver_buttons(l, (sw > 60.0).then_some(pick), new, Some("New driver"), del);
    pick.x
}

/// The same on a phone, across the page's width from `r`'s top: the driver and a new one in
/// a row, deleting under them. Returns how high it came out.
pub fn driver_tools_narrow(l: &mut Launcher, r: Rect) -> f32 {
    let h = ROW;
    if l.pages.new_driver_open {
        let cancel = Rect::new(r.right() - h, r.y, h, h);
        let create = Rect::new(cancel.x - 8.0 - h, r.y, h, h);
        let field = Rect::new(r.x, r.y, (create.x - 8.0 - r.x).max(0.0), h);
        new_driver_row(l, field, create, cancel);
        return h;
    }
    let new = Rect::new(r.right() - h, r.y, h, h);
    let pick = Rect::new(r.x, r.y, (new.x - 8.0 - r.x).max(0.0), h);
    let del = Rect::new(r.x, r.y + h + 8.0, r.w, h);
    driver_buttons(l, Some(pick), new, None, del);
    2.0 * h + 8.0
}

/// Delete the driver `name` (their personnel file): when it was the chosen one, the first
/// driver left is chosen instead (none left: the drivers step opens a new one's tile).
pub fn delete_driver(l: &mut Launcher, name: &str) {
    match core::delete_profile(name) {
        Ok(()) => {
            l.state.set_status(omsi_ui::tr("Personnel file of %{name} deleted.").replace("%{name}", name), false);
            l.state.profiles.retain(|n| n != name);
            if l.state.config.profile == name {
                l.state.profile = None;
                l.state.config.profile = l.state.profiles.first().cloned().unwrap_or_default();
                let _ = core::save_config(&l.state.config);
            }
            l.state.load_profiles();
        }
        Err(e) => l.state.set_status(format!("{e:#}"), true),
    }
}

/// The first press of "Delete this driver" still holds: a moment ago, and for the driver
/// chosen now (another one chosen since is not deleted at a single press).
fn delete_armed(l: &Launcher) -> bool {
    l.pages.confirm_delete.as_ref().is_some_and(|(name, t)| *name == l.state.config.profile && t.elapsed().as_secs() < 4)
}

/// "Delete this driver", and once pressed, what a second press does.
fn delete_label(l: &Launcher) -> &'static str {
    if delete_armed(l) {
        "Click again to delete"
    } else {
        "Delete this driver"
    }
}

/// The new driver's name, Create and the cross, where they were laid out.
fn new_driver_row(l: &mut Launcher, field: Rect, create: Rect, cancel: Rect) {
    let focused = l.ui.focus == Some(id_of(NEW_NAME));
    let enter = focused && l.ui.input.keys.contains(&Key::Enter);
    let escape = focused && l.ui.input.keys.contains(&Key::Escape);
    if l.ui.button("record-new-cancel", cancel, "", Some("close"), ButtonKind::Normal) || escape {
        l.pages.new_driver_open = false;
    }
    l.ui.tooltip(cancel, "Cancel");
    l.ui.text_input(NEW_NAME, field, &mut l.pages.new_driver, "The new driver's name", Some("person"));
    // (a narrow Create is its icon alone)
    let label = if create.w > 2.0 * create.h { "Create" } else { "" };
    if l.ui.button("record-new-create", create, label, Some("add"), ButtonKind::Primary) || enter {
        create_driver(l);
    }
    l.ui.tooltip(create, "Create");
}

/// Choosing the driver (`pick`), making a new one (`new`, its word or only its icon) and
/// deleting the chosen one, where they were laid out.
fn driver_buttons(l: &mut Launcher, pick: Option<Rect>, new: Rect, new_label: Option<&str>, del: Rect) {
    let armed = delete_armed(l);
    if l.ui.button("profile-delete", del, delete_label(l), Some("delete"), ButtonKind::Danger) {
        if armed {
            let name = l.state.config.profile.clone();
            delete_driver(l, &name);
            l.pages.confirm_delete = None;
        } else {
            l.pages.confirm_delete = Some((l.state.config.profile.clone(), std::time::Instant::now()));
        }
    }
    if l.ui.button("profile-new", new, new_label.unwrap_or(""), Some("add"), ButtonKind::Normal) {
        open_new_driver(l);
    }
    l.ui.tooltip(new, "New driver");
    let names = l.state.profiles.clone();
    let mut sel = names.iter().position(|n| *n == l.state.config.profile).unwrap_or(0);
    if let Some(pick) = pick.filter(|_| !names.is_empty()) {
        if l.ui.select("profile", pick, &mut sel, &names) {
            l.state.config.profile = names[sel].clone();
            l.pages.confirm_delete = None;
            let _ = core::save_config(&l.state.config);
            l.state.load_profile();
            l.state.touched();
        }
    }
}

/// How far a driver has come from their level towards the next (0 to 1): a level begins at
/// 250 points times the square of the one before (`omsi_launcher_lib::level_of`).
pub fn level_progress(xp: i64, level: i64, next: i64) -> f32 {
    let prev = (level - 1).max(0).pow(2) * 250;
    ((xp - prev) as f32 / (next - prev).max(1) as f32).clamp(0.0, 1.0)
}

// --- the service record ---------------------------------------------------------------------

/// A big figure: its value, unit and name, and a line under it.
struct Tile {
    value: String,
    unit: &'static str,
    label: &'static str,
    note: String,
}

/// A line of a list: what, how much, where (a record's map), and a bar (a rating).
struct Fact {
    label: &'static str,
    value: String,
    note: String,
    bar: Option<f32>,
}

/// A run in the list of the last ones.
struct Run {
    when: String,
    map: String,
    line: String,
    length: String,
    km: String,
    /// (stops on time, stops)
    on_time: Option<(i32, i32)>,
}

/// What the service record shows, worked out from the personnel file and the runs.
struct Figures {
    since: String,
    tiles: Vec<Tile>,
    /// Stops served early, on time and late.
    stops: [i32; 3],
    style: Vec<Fact>,
    records: Vec<Fact>,
    maps: Vec<(String, usize)>,
    buses: Vec<(String, usize)>,
    hours: [usize; 24],
    runs: Vec<Run>,
    /// The trips the game judged at their ends (none yet: None).
    trips: Option<Trips>,
}

/// The record's trips: what they come to (value, its colour, what it is) and the last ones.
struct Trips {
    figures: Vec<(String, Color, &'static str)>,
    last: Vec<TripLine>,
}

/// A trip in the list of the last ones.
struct TripLine {
    when: String,
    line: Option<String>,
    terminus: String,
    /// (stops on time, stops) of a trip to a timetable; None on a free drive.
    on_time: Option<(i32, i32)>,
    free: bool,
    /// The average and the worst delay, as the trip's card writes them ("+1:05").
    average: Option<String>,
    worst: Option<(String, Color)>,
}

/// How a delay is written on the record (as on the trip's card) and its colour there.
fn delay_text(d: f64) -> (String, Color) {
    let c = match crate::nav_duty::punctuality(d) {
        crate::nav_duty::Punctuality::Late => LATE.lighten(0.2),
        crate::nav_duty::Punctuality::Early => EARLY_SOFT,
        crate::nav_duty::Punctuality::OnTime => ON_TIME.lighten(0.25),
    };
    (crate::nav_duty::offset(d), c)
}

/// Stops on time out of all as a colour: green all of them, plain most, red too few.
fn on_time_colour(good: i32, all: i32) -> Color {
    if good == all {
        ON_TIME.lighten(0.25)
    } else if good * 10 >= all * 8 {
        TEXT
    } else {
        LATE.lighten(0.2)
    }
}

/// The record's trips from the profile's (None without any).
fn trips_part(p: &core::Profile, offset: i64) -> Option<Trips> {
    let t = &p.trip_totals;
    if t.trips == 0 {
        return None;
    }
    let on_time = (t.stops - t.early - t.late).max(0);
    let mut figures = vec![(t.trips.to_string(), TEXT, "Trips")];
    if t.stops > 0 {
        figures.push((format!("{:.0} %", on_time as f64 * 100.0 / t.stops as f64), on_time_colour(on_time, t.stops), "On time"));
    }
    if let Some(a) = t.average {
        let (text, c) = delay_text(a);
        figures.push((text, c, "Average delay"));
    }
    if let Some(w) = t.worst {
        let (text, c) = delay_text(w);
        figures.push((text, c, "Worst delay"));
    }
    let last = p
        .trips
        .iter()
        .take(6)
        .map(|r| TripLine {
            when: date_text(r.time, offset, false),
            line: Some(r.line.trim()).filter(|l| !l.is_empty()).map(str::to_string),
            terminus: r.terminus.trim().to_string(),
            on_time: r.timed().then(|| (r.on_time(), r.stops)),
            free: r.free,
            average: r.average.filter(|_| r.timed()).map(|a| delay_text(a).0),
            worst: r.worst.filter(|_| r.timed()).map(delay_text),
        })
        .collect();
    Some(Trips { figures, last })
}

/// "2h 05m", in the interface's language.
pub(crate) fn duration_text(hours: f64) -> String {
    let m = (hours * 60.0).round().max(0.0) as i64;
    omsi_ui::tr("%{h}h %{m}m").replace("%{h}", &(m / 60).to_string()).replace("%{m}", &format!("{:02}", m % 60))
}

/// Unix time `t` as the local (year, month, day, hour, minute), `offset` seconds from UTC.
fn local_time(t: u64, offset: i64) -> (i64, u32, u32, u32, u32) {
    let s = t as i64 + offset;
    let (days, secs) = (s.div_euclid(86400), s.rem_euclid(86400));
    // the civil date of a day number (Howard Hinnant)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    (y, m, d, (secs / 3600) as u32, (secs % 3600 / 60) as u32)
}

/// "3 October 2026" (`long`) or "3 Oct 2026, 21:40".
pub(super) fn date_text(t: u64, offset: i64, long: bool) -> String {
    let (y, m, d, h, min) = local_time(t, offset);
    let k = (m as usize).clamp(1, 12) - 1;
    if long {
        format!("{d} {} {y}", omsi_ui::tr(crate::ui::MONTHS_LONG[k]))
    } else {
        format!("{d} {} {y}, {h:02}:{min:02}", omsi_ui::tr(crate::ui::MONTHS[k]))
    }
}

fn figures(p: &core::Profile, map_name: &dyn Fn(&str) -> String, bus_name: &dyn Fn(&str) -> String, offset: i64) -> Figures {
    let r = &p.record;
    let tr = |s: &str| omsi_ui::tr(s).into_owned();
    let tile = |value: String, unit: &'static str, label: &'static str, note: String| Tile { value, unit, label, note };
    let tiles = vec![
        tile(r.runs.to_string(), "", "Duties", String::new()),
        tile(duration_text(p.hours), "", "At the wheel", String::new()),
        tile(format!("{:.0}", p.km), "km", "Driven", String::new()),
        tile(p.stops.to_string(), "", "Stops served", String::new()),
        // (a comparison only once there is something to compare: under a hundred tickets
        // "enough for one bus" is a rounding that looks bigger than it is)
        tile(format!("{:.0}", p.tickets), "", "Tickets sold", if p.tickets >= 100.0 { tr("enough to fill %{count} buses").replace("%{count}", &format!("{:.0}", (p.tickets / 100.0).round())) } else { String::new() }),
        tile(format!("{:.2}", p.cash), "", "Takings", if p.hours > 0.0 && p.cash > 0.0 { tr("%{amount} per hour").replace("%{amount}", &format!("{:.2}", p.cash / p.hours)) } else { String::new() }),
    ];
    let fact = |label: &'static str, value: String, note: String, bar: Option<f32>| Fact { label, value, note, bar };
    let rating = |v: f64| Some((v / 100.0).clamp(0.0, 1.0) as f32);
    // (jolts per hundred kilometres, not per run: a run of half an hour and one of eight are
    // not to be compared otherwise)
    let jolts = if r.km > 0.0 { format!("{:.1}", r.jolts as f64 / r.km * 100.0) } else { "-".into() };
    let style = vec![
        fact("Collisions", p.crashes.to_string(), String::new(), None),
        fact("Pedestrians hurt", p.hurt.to_string(), String::new(), None),
        fact("Jolts per 100 km", jolts, String::new(), None),
        fact("Driving style", format!("{:.0} %", p.rating_driving), String::new(), rating(p.rating_driving)),
        fact("Comfort", format!("{:.0} %", p.rating_comfort), String::new(), rating(p.rating_comfort)),
        fact("Ticket selling", format!("{:.0} %", p.rating_tickets), String::new(), rating(p.rating_tickets)),
    ];
    let mut records = Vec::new();
    if let Some(s) = &r.longest {
        records.push(fact("Longest duty", duration_text(s.seconds / 3600.0), map_name(&s.map), None));
    }
    if let Some(s) = &r.furthest {
        records.push(fact("Furthest", format!("{:.1} km", s.metres / 1000.0), map_name(&s.map), None));
    }
    if let Some(s) = &r.busiest {
        records.push(fact("Busiest duty", tr("%{n} tickets").replace("%{n}", &s.tickets.to_string()), map_name(&s.map), None));
    }
    if let Some(s) = &r.most_punctual {
        let good = (s.stops - s.early - s.late).max(0);
        records.push(fact("Closest to the timetable", tr("%{good} of %{all} stops on time").replace("%{good}", &good.to_string()).replace("%{all}", &s.stops.to_string()), map_name(&s.map), None));
    }
    let runs = p.sessions.iter().take(8).map(|s| Run {
        when: date_text(s.time, offset, false),
        map: map_name(&s.map),
        line: s.line.clone().map(|l| s.tour.as_ref().map(|t| format!("{l} / {t}")).unwrap_or(l)).unwrap_or_else(|| tr("Free drive")),
        length: duration_text(s.seconds / 3600.0),
        km: format!("{:.1} km", s.metres / 1000.0),
        on_time: (s.stops > 0).then(|| ((s.stops - s.early - s.late).max(0), s.stops)),
    }).collect();
    Figures {
        since: if r.first > 0 { tr("Driving since %{date}").replace("%{date}", &date_text(r.first, offset, true)) } else { String::new() },
        tiles,
        stops: [p.early.max(0), (p.stops - p.early - p.late).max(0), p.late.max(0)],
        style,
        records,
        maps: r.maps.iter().map(|(m, n)| (map_name(m), *n)).collect(),
        buses: r.buses.iter().map(|(b, n)| (bus_name(b), *n)).collect(),
        hours: r.hours,
        runs,
        trips: trips_part(p, offset),
    }
}

/// The service record, Omsi-Hub's "staat van dienst": the driver and their level at the head,
/// then everything the personnel file and the runs hold - in big figures, bars and short
/// lists - scrolling as a whole. Only what was measured: a figure the runs do not give is
/// left out, not made up.
pub fn profile(l: &mut Launcher, area: Rect) {
    let mut area = area;
    if crate::mobile::mobile() {
        // (a phone has no sheet head: the drivers are the page's first rows)
        let used = driver_tools_narrow(l, area) + 16.0;
        area = Rect::new(area.x, area.y + used, area.w, (area.h - used).max(0.0));
    }
    let Some(p) = l.state.profile.as_ref() else {
        l.ui.paragraph("Create a driver to start a personnel file.", Vec2::new(area.x, area.y + 8.0), area.w, 14.0, Weight::Medium, TEXT_DIM);
        return;
    };
    let maps = &l.state.maps;
    let vehicles = &l.state.vehicles;
    let map_name = |m: &str| maps.iter().find(|x| x.file == m).map(|x| if x.friendly.is_empty() { x.name.clone() } else { x.friendly.clone() }).unwrap_or_else(|| short_map(m));
    let bus_name = |b: &str| vehicles.iter().find(|v| v.file == b).map(|v| v.name.clone()).unwrap_or_else(|| b.rsplit('/').next().unwrap_or(b).trim_end_matches(".bus").to_string());
    let f = figures(p, &map_name, &bus_name, core::utc_offset());
    let (name, exists, level, frac, xp, next) = (p.name.clone(), p.exists, p.level, level_progress(p.xp, p.level, p.next_level_xp), p.xp, p.next_level_xp);
    // the head: who, since when; their level on the right, in the line's yellow (a phone:
    // the way to the next level under the name, across the page)
    let narrow = area.w < 560.0;
    let head = Rect::new(area.x, area.y, area.w, if narrow { 92.0 } else { 62.0 });
    let lw = if narrow { 120.0 } else { 220.0f32.min(area.w * 0.4) };
    let word = omsi_ui::tr("Level %{n}").replace("%{n}", &level.to_string()).to_uppercase();
    l.ui.text_in(&name, Rect::new(head.x, head.y + 2.0, head.w - lw - 20.0, 32.0), 26.0, Weight::Bold, TEXT, Align::Left);
    let under = if exists { f.since.clone() } else { omsi_ui::tr("No personnel file yet: the game writes it after the first run.").into_owned() };
    let uw = if narrow { head.w } else { head.w - lw - 20.0 };
    l.ui.text_in(&under, Rect::new(head.x, head.y + 36.0, uw, 18.0), 13.0, Weight::Regular, TEXT_DIM, Align::Left);
    let lv = Rect::new(head.right() - lw, head.y + 2.0, lw, 62.0);
    l.ui.text_in(&word, Rect::new(lv.x, lv.y, lv.w, 20.0), 15.0, Weight::Bold, LINE, Align::Right);
    let shown = l.ui.anim(id_of("record-level"), frac, 0.4);
    let bar = if narrow { Rect::new(head.x, head.y + 64.0, head.w, 5.0) } else { Rect::new(lv.x, lv.y + 28.0, lv.w, 5.0) };
    l.ui.p().rounded(bar, 2.5, HAIRLINE);
    l.ui.p().rounded(Rect::new(bar.x, bar.y, (bar.w * shown).max(5.0), bar.h), 2.5, LINE);
    let to_next = omsi_ui::tr("%{xp} of %{next} points to level %{n}").replace("%{xp}", &xp.to_string()).replace("%{next}", &next.to_string()).replace("%{n}", &(level + 1).to_string());
    let at = if narrow { Rect::new(head.x, bar.bottom() + 4.0, head.w, 18.0) } else { Rect::new(lv.x - 60.0, lv.y + 38.0, lv.w + 60.0, 18.0) };
    l.ui.text_in(&to_next, at, 12.0, Weight::Regular, TEXT_DIM, Align::Right);
    l.ui.p().rect(Rect::new(head.x, head.bottom() + 8.0, head.w, 1.0), HAIRLINE);
    // (the list begins `FIGURE_ROOM` higher than its first figures: room for one risen under
    // the mouse)
    let rest = Rect::new(area.x - 6.0, head.bottom() + 22.0 - FIGURE_ROOM, area.w + 12.0, (area.bottom() - head.bottom() - 22.0 + FIGURE_ROOM).max(0.0));
    l.ui.scroll_area("record", rest, &mut |ui, v| record_body(ui, Rect::new(v.x + 6.0, v.y, v.w - 20.0, v.h), &f));
}

/// A section of the record: its hairline box and its name; returns the room inside.
fn section(ui: &mut Ui, r: Rect, title: &str) -> Rect {
    ui.card(r);
    ui.text_in(&omsi_ui::tr(title).to_uppercase(), Rect::new(r.x + 16.0, r.y + 14.0, r.w - 32.0, 14.0), 10.5, Weight::Bold, TEXT_DIM, Align::Left);
    Rect::new(r.x + 16.0, r.y + 40.0, r.w - 32.0, (r.h - 54.0).max(0.0))
}

/// Room above the record's first figures for a figure risen under the mouse (`Ui::card_tile`).
const FIGURE_ROOM: f32 = 8.0;

/// Everything under the record's head, from `r`'s top down; returns how high it came out.
fn record_body(ui: &mut Ui, r: Rect, f: &Figures) -> f32 {
    let gap = 12.0;
    let mut y = r.y + FIGURE_ROOM;
    // the big figures, as many in a row as fit at 150 points
    // (two to a row on a phone: one each was a screen of six boxes)
    let min_w = if r.w < 400.0 { 110.0 } else { 150.0 };
    let cols = (((r.w + gap) / (min_w + gap)).floor() as usize).clamp(1, f.tiles.len().max(1));
    let tw = (r.w - gap * (cols as f32 - 1.0)) / cols as f32;
    let th = if tw < 150.0 { 80.0 } else { 92.0 };
    let value_px = if tw < 150.0 { 22.0 } else { 28.0 };
    for (k, t) in f.tiles.iter().enumerate() {
        let base = Rect::new(r.x + (k % cols) as f32 * (tw + gap), y + (k / cols) as f32 * (th + gap), tw, th);
        if !ui.rect_visible(base) {
            continue;
        }
        // (a figure is no button, but it moves under the mouse as the tiles do: it rises, the
        // light follows the mouse and its edge lights up - `Ui::card_tile`; under its shadow
        // it takes the sheet's colour, having none of its own at rest)
        ui.solid(base);
        let m = ui.card_tile(id_of(&format!("record-figure-{k}")), base, RADIUS);
        let c = m.r;
        ui.tile_shadow(&m);
        if m.hover > 0.005 {
            ui.p().rounded(c, RADIUS, PANEL.mix(HOVER, 0.5 * m.hover));
        }
        ui.tile_light(&m, 0.04);
        ui.tile_edge(&m, 1.0, HAIRLINE);
        let pad = if tw < 150.0 { 12.0 } else { 16.0 };
        let vw = ui.text_in(&t.value, Rect::new(c.x + pad, c.y + 12.0, base.w - 2.0 * pad, 34.0), value_px, Weight::Bold, TEXT, Align::Left);
        if !t.unit.is_empty() {
            ui.text_in(t.unit, Rect::new(c.x + pad + 4.0 + vw, c.y + 20.0, 40.0, 24.0), 14.0, Weight::Bold, TEXT_DIM, Align::Left);
        }
        let ly = c.bottom() - 42.0;
        ui.text_in(&omsi_ui::tr(t.label).to_uppercase(), Rect::new(c.x + pad, ly, base.w - 2.0 * pad, 14.0), 10.5, Weight::Medium, TEXT_DIM, Align::Left);
        if !t.note.is_empty() {
            ui.text_in(&t.note, Rect::new(c.x + pad, ly + 18.0, base.w - 2.0 * pad, 16.0), 12.0, Weight::Regular, TEXT_DIM, Align::Left);
        }
    }
    y += f.tiles.len().div_ceil(cols) as f32 * (th + gap) + 6.0;
    // punctuality has the most room: it is what the trade is about
    let all = f.stops.iter().sum::<i32>();
    if all > 0 {
        let s = Rect::new(r.x, y, r.w, 132.0);
        let inner = section(ui, s, "Punctuality");
        let bar = Rect::new(inner.x, inner.y, inner.w, 12.0);
        ui.p().rounded(bar, 6.0, HAIRLINE);
        let colours = [EARLY, ON_TIME, LATE];
        let mut x = bar.x;
        for (k, n) in f.stops.iter().enumerate() {
            let w = bar.w * *n as f32 / all as f32;
            if w > 0.5 {
                ui.p().rect(Rect::new(x, bar.y, w, bar.h), colours[k]);
            }
            x += w;
        }
        let mut lx = inner.x;
        for (k, word) in ["Early", "On time", "Late"].iter().enumerate() {
            ui.p().circle(Vec2::new(lx + 4.5, bar.bottom() + 22.0), 4.5, colours[k]);
            let w = ui.text_in(word, Rect::new(lx + 14.0, bar.bottom() + 13.0, 200.0, 18.0), 13.0, Weight::Regular, TEXT_DIM, Align::Left);
            let n = f.stops[k].to_string();
            let nw = ui.text_in(&n, Rect::new(lx + 20.0 + w, bar.bottom() + 13.0, 80.0, 18.0), 13.0, Weight::Bold, TEXT, Align::Left);
            lx += 20.0 + w + nw + 22.0;
        }
        let pct = f.stops[1] as f32 / all as f32 * 100.0;
        let note = omsi_ui::tr("%{pct} % of %{n} stops on time").replace("%{pct}", &format!("{pct:.0}")).replace("%{n}", &all.to_string());
        ui.text_in(&note, Rect::new(inner.x, bar.bottom() + 40.0, inner.w, 18.0), 12.5, Weight::Regular, TEXT_DIM, Align::Left);
        y = s.bottom() + gap;
    }
    // the trips the game judged as they ended: what they come to, and the last of them
    if let Some(t) = &f.trips {
        y += trips_section(ui, Rect::new(r.x, y, r.w, 0.0), t) + gap;
    }
    // how they drive and their records, side by side where there is room
    let two = r.w >= 640.0;
    let cw = if two { (r.w - gap) * 0.5 } else { r.w };
    let fact_h = |facts: &[Fact]| 54.0 + facts.iter().map(|x| if x.note.is_empty() { 30.0 } else { 46.0 }).sum::<f32>();
    let (h1, h2) = (fact_h(&f.style), if f.records.is_empty() { 0.0 } else { fact_h(&f.records) });
    let h = if two { h1.max(h2) } else { h1 };
    facts(ui, Rect::new(r.x, y, cw, h), "How you drive", &f.style);
    if !f.records.is_empty() {
        let at = if two { Rect::new(r.x + cw + gap, y, cw, h) } else { Rect::new(r.x, y + h1 + gap, cw, h2) };
        facts(ui, at, "Records", &f.records);
    }
    y += if two || f.records.is_empty() { h } else { h1 + gap + h2 } + gap;
    // where and with what, the most driven first, each bar as long as its share of the first
    if !f.maps.is_empty() {
        let n = f.maps.len().max(f.buses.len()) as f32;
        let h = 54.0 + n * 28.0;
        top(ui, Rect::new(r.x, y, cw, h), "Where you drive", &f.maps);
        let at = if two { Rect::new(r.x + cw + gap, y, cw, h) } else { Rect::new(r.x, y + h + gap, cw, h) };
        top(ui, at, "What you drive", &f.buses);
        y += if two { h } else { 2.0 * h + gap } + gap;
    }
    // when the runs end: the hour at the computer, not the game's
    if f.hours.iter().any(|n| *n > 0) {
        let s = Rect::new(r.x, y, r.w, 168.0);
        let inner = section(ui, s, "When you finish a duty");
        let most = *f.hours.iter().max().unwrap_or(&1) as f32;
        let cw = inner.w / 24.0;
        let base = inner.y + 90.0;
        for (hour, n) in f.hours.iter().enumerate() {
            let bh = (if most > 0.0 { *n as f32 / most * 90.0 } else { 0.0 }).max(2.0);
            let b = Rect::new(inner.x + hour as f32 * cw + 1.5, base - bh, cw - 3.0, bh);
            ui.p().rounded(b, 2.0, if *n > 0 { accent() } else { HAIRLINE });
            if hour % 6 == 0 {
                ui.text_in(&format!("{hour:02}"), Rect::new(b.x, base + 5.0, 30.0, 14.0), 10.0, Weight::Regular, TEXT_DIM, Align::Left);
            }
        }
        y = s.bottom() + gap;
    }
    // the last runs
    if f.runs.is_empty() {
        y += ui.paragraph("Nothing driven yet. Complete a duty and it appears here: hours, kilometres, punctuality, and how you drove.", Vec2::new(r.x, y + 4.0), r.w, 14.0, Weight::Regular, TEXT_DIM) + 8.0;
    } else {
        let rh = 34.0;
        let s = Rect::new(r.x, y, r.w, 54.0 + f.runs.len() as f32 * rh);
        let inner = section(ui, s, "Last duties");
        // (a narrow window: the date and the line make room)
        let wide = inner.w >= 620.0;
        for (k, run) in f.runs.iter().enumerate() {
            let row = Rect::new(inner.x, inner.y + k as f32 * rh, inner.w, rh);
            if k > 0 {
                ui.p().rect(Rect::new(row.x, row.y, row.w, 1.0), HAIRLINE);
            }
            let mut x = row.x;
            if wide {
                ui.text_in(&run.when, Rect::new(x, row.y, row.w * 0.24 - 12.0, rh), 13.0, Weight::Regular, TEXT_DIM, Align::Left);
                x += row.w * 0.24;
            }
            let mw = if wide { row.w * 0.28 } else { row.w - 220.0 };
            ui.text_in(&run.map, Rect::new(x, row.y, mw - 12.0, rh), 13.0, Weight::Medium, TEXT, Align::Left);
            x += mw;
            if wide {
                ui.text_in(&run.line, Rect::new(x, row.y, row.w * 0.16 - 12.0, rh), 13.0, Weight::Regular, TEXT_DIM, Align::Left);
            }
            let right = row.right();
            let (ow, kw, dw) = (64.0, 80.0, 76.0);
            if let Some((good, all)) = run.on_time {
                // (green all on time, blue some early, red some late - colour is punctuality here)
                let c = if good == all { ON_TIME.lighten(0.25) } else if good * 10 >= all * 8 { TEXT } else { LATE.lighten(0.2) };
                ui.text_in(&format!("{good}/{all}"), Rect::new(right - ow, row.y, ow, rh), 13.0, Weight::Bold, c, Align::Right);
            }
            ui.text_in(&run.km, Rect::new(right - ow - kw, row.y, kw - 8.0, rh), 13.0, Weight::Regular, TEXT_SOFT, Align::Right);
            ui.text_in(&run.length, Rect::new(right - ow - kw - dw, row.y, dw - 8.0, rh), 13.0, Weight::Regular, TEXT_SOFT, Align::Right);
        }
        y = s.bottom() + gap;
    }
    y - r.y
}

/// The record's trips in a section from `r`'s top: what they come to in a row of figures (two
/// to a row on a phone), then the last ones - the line, where to, and on the right the average
/// and the worst delay and the stops on time; returns how high it came out.
fn trips_section(ui: &mut Ui, r: Rect, t: &Trips) -> f32 {
    let (rh, head) = (34.0, 22.0);
    let n = t.figures.len().max(1);
    let cols = if r.w - 32.0 < 110.0 * n as f32 { 2.min(n) } else { n };
    let fig_h = n.div_ceil(cols) as f32 * 50.0;
    let h = 54.0 + fig_h + 12.0 + if t.last.is_empty() { 0.0 } else { head + t.last.len() as f32 * rh };
    let s = Rect::new(r.x, r.y, r.w, h);
    let inner = section(ui, s, "Trip reports");
    let fw = inner.w / cols as f32;
    for (k, (value, colour, label)) in t.figures.iter().enumerate() {
        let (x, y) = (inner.x + (k % cols) as f32 * fw, inner.y + (k / cols) as f32 * 50.0);
        ui.text_in(value, Rect::new(x, y - 2.0, fw - 12.0, 26.0), 22.0, Weight::Bold, *colour, Align::Left);
        ui.text_in(&omsi_ui::tr(label).to_uppercase(), Rect::new(x, y + 26.0, fw - 12.0, 14.0), 10.5, Weight::Medium, TEXT_DIM, Align::Left);
    }
    if t.last.is_empty() {
        return h;
    }
    // the last trips, under the names of their columns
    let wide = inner.w >= 620.0;
    let (ow, dw) = (64.0, 72.0);
    let top = inner.y + fig_h + 12.0;
    let columns: &[(&str, f32)] = if wide { &[("On time", 0.0), ("Worst", ow), ("Average", ow + dw)] } else { &[("On time", 0.0), ("Worst", ow)] };
    for (word, at) in columns {
        let cell = if *at == 0.0 { Rect::new(inner.right() - ow, top, ow, head - 6.0) } else { Rect::new(inner.right() - at - dw, top, dw - 8.0, head - 6.0) };
        ui.text_in(&omsi_ui::tr(word).to_uppercase(), cell, 10.0, Weight::Medium, TEXT_FAINT, Align::Right);
    }
    for (k, line) in t.last.iter().enumerate() {
        let row = Rect::new(inner.x, top + head + k as f32 * rh, inner.w, rh);
        ui.p().rect(Rect::new(row.x, row.y, row.w, 1.0), HAIRLINE);
        let mut x = row.x;
        if wide {
            ui.text_in(&line.when, Rect::new(x, row.y, row.w * 0.22 - 12.0, rh), 13.0, Weight::Regular, TEXT_DIM, Align::Left);
            x += row.w * 0.22;
        }
        // (the line's plate, as on the duty cards)
        if let Some(l) = &line.line {
            let pw = (ui.width(l, 12.0, Weight::Black) + 12.0).clamp(26.0, 70.0);
            let plate = Rect::new(x, row.center().y - 9.5, pw, 19.0);
            ui.p().rounded(plate, 5.0, LINE);
            ui.text_in(l, plate, 12.0, Weight::Black, ON_LINE, Align::Center);
            x += pw + 10.0;
        }
        let mut right = row.right();
        // (a free drive keeps no time: it says so where the figures would be)
        if line.free {
            ui.text_in("Free drive", Rect::new(right - ow - dw, row.y, ow + dw, rh), 12.5, Weight::Regular, TEXT_FAINT, Align::Right);
        }
        if let Some((good, all)) = line.on_time {
            ui.text_in(&format!("{good}/{all}"), Rect::new(right - ow, row.y, ow, rh), 13.0, Weight::Bold, on_time_colour(good, all), Align::Right);
        }
        right -= ow;
        if let Some((w, c)) = &line.worst {
            ui.text_in(w, Rect::new(right - dw, row.y, dw - 8.0, rh), 13.0, Weight::Medium, *c, Align::Right);
        }
        right -= dw;
        if wide {
            if let Some(a) = &line.average {
                ui.text_in(a, Rect::new(right - dw, row.y, dw - 8.0, rh), 13.0, Weight::Regular, TEXT_SOFT, Align::Right);
            }
            right -= dw;
        }
        ui.text_in(&line.terminus, Rect::new(x, row.y, (right - x - 12.0).max(0.0), rh), 13.0, Weight::Medium, TEXT, Align::Left);
    }
    h
}

/// A list of facts in a section: the name on the left, the value on the right, a bar between
/// for a rating, and where a record was set under it.
fn facts(ui: &mut Ui, r: Rect, title: &str, list: &[Fact]) {
    let inner = section(ui, r, title);
    let mut y = inner.y;
    for f in list {
        let row = Rect::new(inner.x, y, inner.w, 26.0);
        // (the value as wide as it is - "41 of 42 stops on time" - the name in what is left)
        let vw = (ui.width(&f.value, 16.0, Weight::Bold) + 4.0).min(row.w * 0.6);
        let lw = ui.text_in(f.label, Rect::new(row.x, row.y, (row.w - vw - 12.0).max(0.0), row.h), 13.0, Weight::Regular, TEXT_DIM, Align::Left);
        ui.text_in(&f.value, Rect::new(row.right() - vw, row.y, vw, row.h), 16.0, Weight::Bold, TEXT, Align::Right);
        if let Some(t) = f.bar {
            let x0 = row.x + lw.max(row.w * 0.3) + 16.0;
            let bar = Rect::new(x0, row.center().y - 3.0, (row.right() - 70.0 - x0).max(0.0), 6.0);
            ui.p().rounded(bar, 3.0, HAIRLINE);
            ui.p().rounded(Rect::new(bar.x, bar.y, (bar.w * t).max(6.0), bar.h), 3.0, accent());
        }
        y += 30.0;
        if !f.note.is_empty() {
            ui.text_in(&f.note, Rect::new(row.x, y - 6.0, row.w, 16.0), 11.5, Weight::Regular, TEXT_DIM, Align::Left);
            y += 16.0;
        }
    }
}

/// The three most driven of something, a bar each as long as its share of the first.
fn top(ui: &mut Ui, r: Rect, title: &str, rows: &[(String, usize)]) {
    let inner = section(ui, r, title);
    let most = rows.first().map(|x| x.1).unwrap_or(0).max(1) as f32;
    for (k, (name, n)) in rows.iter().enumerate() {
        let row = Rect::new(inner.x, inner.y + k as f32 * 28.0, inner.w, 22.0);
        let bw = 90.0f32.min(row.w * 0.3);
        ui.text_in(name, Rect::new(row.x, row.y, row.w - bw - 52.0, row.h), 13.0, Weight::Regular, TEXT, Align::Left);
        let bar = Rect::new(row.right() - bw - 38.0, row.center().y - 3.0, bw, 6.0);
        ui.p().rounded(bar, 3.0, HAIRLINE);
        ui.p().rounded(Rect::new(bar.x, bar.y, (bar.w * *n as f32 / most).max(6.0), bar.h), 3.0, accent());
        ui.text_in(&n.to_string(), Rect::new(row.right() - 30.0, row.y, 30.0, row.h), 13.0, Weight::Bold, TEXT, Align::Right);
    }
}

#[cfg(test)]
mod record_tests {
    use super::*;

    fn session(time: u64, map: &str, minutes: f64, km: f64, stops: i32, late: i32, tickets: i32) -> core::Session {
        core::Session { time, driver: "Luc".into(), map: map.into(), bus: "Vehicles/SD200/SD200.bus".into(), line: Some("13".into()), tour: Some("2".into()), seconds: minutes * 60.0, metres: km * 1000.0, stops, late, tickets, ..Default::default() }
    }

    fn profile(sessions: Vec<core::Session>) -> core::Profile {
        let record = core::record_of(&sessions, 0);
        core::Profile { name: "Luc".into(), file: "Drivers/Luc.odr".into(), hours: 2.5, km: 52.0, xp: 300, level: 2, next_level_xp: 1000, stops: 40, early: 2, late: 6, tickets: 140.0, cash: 210.0, crashes: 1, hurt: 0, rating_driving: 87.0, rating_comfort: 64.0, rating_tickets: 100.0, sessions, exists: true, record, trips: Vec::new(), trip_totals: Default::default() }
    }

    fn trip(time: u64, line: &str, stops: i32, early: i32, late: i32, average: f64, worst: f64) -> core::TripRun {
        core::TripRun { time, driver: "Luc".into(), line: line.into(), terminus: "Rathaus Spandau".into(), planned: stops, stops, early, late, average: Some(average), worst: Some(worst), completed: true, ..Default::default() }
    }

    fn with_trips(trips: Vec<core::TripRun>) -> core::Profile {
        core::Profile { trip_totals: core::trip_totals(&trips), trips, ..profile(Vec::new()) }
    }

    /// The trips: what they come to (the share of their stops on time, the average and the
    /// worst delay) and the last ones with theirs; a free drive is listed without them.
    #[test]
    fn the_record_shows_the_trips() {
        assert!(figures(&profile(Vec::new()), &|m| m.to_string(), &|b| b.to_string(), 0).trips.is_none(), "no trips, no section");
        let p = with_trips(vec![trip(1_000_000_300, "5", 10, 1, 1, 70.0, 250.0), trip(1_000_000_200, "5", 10, 0, 0, -10.0, -60.0), core::TripRun { time: 1_000_000_100, free: true, stops: 6, line: "13".into(), ..Default::default() }]);
        let t = figures(&p, &|m| m.to_string(), &|b| b.to_string(), 0).trips.expect("trips");
        assert_eq!(t.figures.iter().map(|x| (x.0.as_str(), x.2)).collect::<Vec<_>>(), [("3", "Trips"), ("90 %", "On time"), ("+0:30", "Average delay"), ("+4:10", "Worst delay")]);
        assert_eq!(t.last.len(), 3);
        assert_eq!((t.last[0].line.as_deref(), t.last[0].on_time, t.last[0].average.as_deref()), (Some("5"), Some((8, 10)), Some("+1:10")));
        assert_eq!(t.last[0].worst.as_ref().unwrap().0, "+4:10");
        assert_eq!(t.last[0].when, "9 Sep 2001, 01:51");
        // (the free drive: no punctuality)
        assert_eq!((t.last[2].free, t.last[2].on_time, t.last[2].worst.is_none()), (true, None, true));
        // drawn on a phone and a wide window
        let f = figures(&p, &|m| m.to_string(), &|b| b.to_string(), 0);
        let mut ui = Ui::new();
        ui.begin(Vec2::new(1400.0, 900.0), 1.0, 1.0 / 60.0);
        let without = record_body(&mut ui, Rect::new(0.0, 0.0, 1300.0, 700.0), &figures(&profile(Vec::new()), &|m| m.to_string(), &|b| b.to_string(), 0));
        let wide = record_body(&mut ui, Rect::new(0.0, 0.0, 1300.0, 700.0), &f);
        let narrow = trips_section(&mut ui, Rect::new(0.0, 0.0, 360.0, 0.0), f.trips.as_ref().unwrap());
        assert!(wide > without + 150.0, "{wide} against {without}");
        // (two figures to a row on a phone)
        assert!(narrow > trips_section(&mut ui, Rect::new(0.0, 0.0, 1300.0, 0.0), f.trips.as_ref().unwrap()));
    }

    #[test]
    fn the_record_says_what_the_file_and_the_runs_hold() {
        let p = profile(vec![session(1_000_000_000, "maps/Spandau/global.cfg", 90.0, 40.0, 30, 3, 120), session(1_000_100_000, "maps/Grundorf/global.cfg", 60.0, 12.0, 10, 0, 20)]);
        let f = figures(&p, &|m| short_map(m), &|b| b.rsplit('/').next().unwrap_or(b).to_string(), 0);
        assert_eq!(f.tiles.len(), 6);
        assert_eq!(f.tiles[0].value, "2");
        assert_eq!(f.tiles[1].value, "2h 30m");
        // a hundred and forty tickets fill a bus and a half ("enough to fill" is not said
        // under a hundred)
        assert!(f.tiles[4].note.contains('1'));
        assert_eq!(f.tiles[5].note, "84.00 per hour");
        assert_eq!(f.stops, [2, 32, 6]);
        assert_eq!(f.records.iter().map(|x| x.label).collect::<Vec<_>>(), ["Longest duty", "Furthest", "Busiest duty", "Closest to the timetable"]);
        assert_eq!(f.records[0].note, "Spandau");
        assert_eq!(f.records[3].value, "10 of 10 stops on time");
        assert_eq!(f.maps[0], ("Spandau".to_string(), 1));
        assert_eq!(f.buses[0], ("SD200.bus".to_string(), 2));
        assert_eq!(f.runs.len(), 2);
        assert_eq!(f.runs[0].line, "13 / 2");
        assert_eq!(f.runs[0].on_time, Some((27, 30)));
        assert_eq!(f.since, "Driving since 9 September 2001");
        // the ratings are bars, the counts are not
        assert_eq!(f.style.iter().filter(|x| x.bar.is_some()).count(), 3);
    }

    #[test]
    fn no_runs_no_records() {
        let p = profile(Vec::new());
        let f = figures(&p, &|m| m.to_string(), &|b| b.to_string(), 0);
        assert!(f.records.is_empty() && f.runs.is_empty() && f.maps.is_empty());
        assert_eq!(f.since, "");
        // drawn, the page says there is nothing yet instead of showing empty boxes
        let mut ui = Ui::new();
        ui.begin(Vec2::new(1200.0, 900.0), 1.0, 1.0 / 60.0);
        let h = record_body(&mut ui, Rect::new(0.0, 0.0, 1000.0, 800.0), &f);
        assert!(h > 100.0);
    }

    #[test]
    fn the_record_fits_a_phone_and_a_wide_window() {
        let p = profile((0..12).map(|k| session(1_000_000_000 + k * 3600, "maps/Spandau/global.cfg", 30.0, 10.0, 8, 1, 5)).collect());
        let f = figures(&p, &|m| short_map(m), &|b| b.to_string(), 3600);
        let mut ui = Ui::new();
        ui.begin(Vec2::new(1400.0, 900.0), 1.0, 1.0 / 60.0);
        let wide = record_body(&mut ui, Rect::new(0.0, 0.0, 1300.0, 700.0), &f);
        let narrow = record_body(&mut ui, Rect::new(0.0, 0.0, 360.0, 700.0), &f);
        // (one column on a phone: taller, nothing laid beside)
        assert!(narrow > wide, "{narrow} against {wide}");
        assert_eq!(f.runs.len(), 8, "the last eight runs, not all");
    }

    #[test]
    fn times_and_levels() {
        assert_eq!(local_time(0, 0), (1970, 1, 1, 0, 0));
        assert_eq!(local_time(1_000_000_000, 7200), (2001, 9, 9, 3, 46));
        assert_eq!(local_time(951_782_400, 0), (2000, 2, 29, 0, 0));
        assert_eq!(duration_text(129.0 + 2.0 / 60.0), "129h 02m");
        assert_eq!(level_progress(250, 2, 1000), 0.0);
        assert_eq!(level_progress(625, 2, 1000), 0.5);
        assert_eq!(level_progress(0, 1, 250), 0.0);
    }

    /// The new driver's tile: Enter makes them once a name is typed, Escape gives up.
    #[test]
    fn a_new_driver_is_made_with_enter_and_left_with_escape() {
        let tile = Rect::new(100.0, 100.0, 260.0, 160.0);
        let mut ui = Ui::new();
        let mut name = String::new();
        ui.focus = Some(id_of(NEW_NAME));
        ui.begin(Vec2::new(800.0, 600.0), 1.0, 1.0 / 60.0);
        ui.input.keys.push(Key::Enter);
        assert_eq!(new_driver_tile(&mut ui, tile, &mut name), None, "no name, nothing made");
        ui.finish();
        ui.focus = Some(id_of(NEW_NAME));
        ui.begin(Vec2::new(800.0, 600.0), 1.0, 1.0 / 60.0);
        ui.input.text.push_str("Moffel");
        assert_eq!(new_driver_tile(&mut ui, tile, &mut name), None);
        assert_eq!(name, "Moffel");
        ui.finish();
        ui.begin(Vec2::new(800.0, 600.0), 1.0, 1.0 / 60.0);
        ui.input.keys.push(Key::Enter);
        assert_eq!(new_driver_tile(&mut ui, tile, &mut name), Some(true));
        ui.finish();
        ui.focus = Some(id_of(NEW_NAME));
        ui.begin(Vec2::new(800.0, 600.0), 1.0, 1.0 / 60.0);
        ui.input.keys.push(Key::Escape);
        assert_eq!(new_driver_tile(&mut ui, tile, &mut name), Some(false));
    }

    #[test]
    fn every_page_says_what_it_is_for() {
        for (page, _, _) in crate::PAGES {
            let (icon, sub) = about(page, 0);
            assert!(!icon.is_empty());
            assert!(page == Page::Drive || !sub.is_empty(), "{page:?}");
        }
        assert_ne!(about(Page::Controls, 0).1, about(Page::Controls, 1).1);
    }
}
