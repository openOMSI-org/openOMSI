//! The game menu's own windows besides the administration (see `admin`), as OMSI has them
//! in its menus: the options that can change while driving, the line and tour to drive,
//! the driver whose personnel file the run goes into, and the bus's fleet number. Each is a
//! list of (label, action) lines in the menu's chooser; choosing a line does it and shows
//! the list again (or the next one: a line's tours).

use crate::App;
/// The cloud types of OMSI's weather (`Weather/clouds.cfg`): the name in a weather file and
/// the name shown.
use crate::weather_setup::CLOUD_KINDS;

/// Which list the chooser shows.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ListKind {
    Admin,
    /// The options of the game, on this page of the settings window.
    Options(usize),
    /// What can be done with the vehicle, on this page.
    Vehicle(usize),
    /// The clock, the weather and the traffic, on this page.
    World(usize),
    /// Entry points for keyboard and game controller configuration.
    Controls,
    /// Vehicle bindings and game/camera bindings on separate pages.
    Keyboard(usize),
    ControllerDevices(usize),
    Controller(String, usize),
    ControllerAxis(String, usize),
    ControllerButtons(String),
    ControllerButtonSettings(String, usize),
    ControllerButton(String, usize),
    ControllerCapture(String),
    /// OMSI's KY_ vehicle events that can be added to the keyboard.
    Events,
    Lines,
    /// A line's tours; the stop chosen in the timetable beside them to start from: (the
    /// tour's number, the stop as `Schedule::tour_stops` lists them), none: the default.
    Tours(String, Option<(String, usize, usize)>),
    Drivers,
    Numbers,
    /// The termini of the bus's depot file, for its destination display.
    Destinations,
    /// The route numbers (lines) for the destination display: the depot file's and the
    /// map timetable's.
    RouteNumbers,
    /// The depot files (.hof) of the bus driven.
    Hofs,
    /// The map's entry points (the launcher's "Start at"), to put the bus at.
    Spots,
    /// Placing a vehicle: its manufacturer, then its type (the manufacturer's key), its
    /// livery, then its depot file (bus file; bus file and livery).
    PlaceMaker,
    PlaceType(String),
    PlaceLivery(String),
    PlaceHof(String, String),
}

/// The lists with a search line on top: the vehicles to place, the depot files and the
/// destinations.
pub(crate) fn searchable(kind: &ListKind) -> bool {
    matches!(kind, ListKind::Hofs | ListKind::Destinations | ListKind::PlaceMaker | ListKind::PlaceType(_) | ListKind::PlaceHof(..))
}

/// The search typed (while it is) or set for the open list.
fn search_query(app: &App) -> &str {
    if app.menus.menu_edit_search {
        app.menus.menu_edit.as_deref().unwrap_or("")
    } else {
        &app.menus.menu_search
    }
}

/// A line of a list matches a search when its label or its action (a file's path) holds
/// every word of it, whatever the case.
fn search_matches(label: &str, action: &str, query: &str) -> bool {
    let haystack = format!("{label} {action}").to_lowercase();
    query.split_whitespace().all(|word| haystack.contains(&word.to_lowercase()))
}

/// A vehicle file of the menu's list (`Vehicles/...`) as its definition.
fn bus_def(app: &App, bus: &str) -> Option<omsi_vehicle::Vehicle> {
    let path = crate::spawn::player_bus_path(&app.args.root, bus).ok()?;
    omsi_vehicle::Vehicle::load(&path).ok()
}

/// The route numbers to choose from: the lines of the depot file's routes (their own
/// line, or the route code without its last two digits) and of the map's timetable.
fn route_numbers(app: &App) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(hof) = app.player.as_ref().and_then(|p| p.vehicle.host.hof.clone()) {
        for t in &hof.info_trips {
            let code = t.code.trim();
            let l = if !t.line.trim().is_empty() { t.line.trim().to_string() } else if code.len() > 2 && code.chars().all(|c| c.is_ascii_digit()) { code[..code.len() - 2].trim_start_matches('0').to_string() } else { String::new() };
            // A route number is display text, not necessarily an alphanumeric IBIS code:
            // Brazilian matrices use values such as "-10". Keep the HOF spelling intact.
            let l = l.trim().to_string();
            if !l.is_empty() && !out.contains(&l) {
                out.push(l);
            }
        }
    }
    if let Some(sch) = crate::duty_book::timetable(app) {
        for l in &sch.data.lines {
            let n = l.name.trim().to_string();
            if !n.is_empty() && !out.contains(&n) {
                out.push(n);
            }
        }
    }
    out.sort_by(|a, b| natural(a, b));
    out
}

/// Whether a line can be represented by the numeric IBIS line/suffix variables openOMSI
/// already knows how to encode. Everything else must stay as display text: turning "-10"
/// or "EXP" into a number loses information.
fn numeric_ibis_line(line: &str) -> bool {
    let line = line.trim();
    if line.is_empty() {
        return false;
    }
    if line.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    let mut chars = line.chars();
    if let Some(first) = chars.next() {
        let rest: String = chars.collect();
        if matches!(first.to_ascii_uppercase(), 'E' | 'S' | 'A' | 'D' | 'C' | 'B' | 'U' | 'M' | 'N' | 'X')
            && !rest.is_empty()
            && rest.chars().all(|c| c.is_ascii_digit())
        {
            return true;
        }
    }
    let digits: String = line.chars().take_while(|c| c.is_ascii_digit()).collect();
    let suffix = &line[digits.len()..];
    !digits.is_empty()
        && suffix.chars().count() == 1
        && matches!(suffix.chars().next().unwrap().to_ascii_uppercase(), 'E' | 'U' | 'N' | 'S' | 'M')
}

/// The route number on the bus's IBIS and display, as picked or typed in the destination
/// list (the destination stays: the one on the display now, else the first -
/// `schedule::shown_destination`).
pub(crate) fn set_route_by_hand(app: &mut App, line: &str) {
    let line = line.trim();
    if line.is_empty() {
        return;
    }
    if let Some(p) = app.player.as_mut() {
        if numeric_ibis_line(line) {
            let hof = p.vehicle.host.hof.clone();
            if let Some(hof) = hof.as_deref() {
                if let Some(ti) = crate::schedule::shown_destination(&p.vehicle, hof, p.blind_pick.as_ref()) {
                    p.set_destination_by_hand(hof, line, ti);
                }
            }
        } else {
            // OMSI's route-number field is also used as arbitrary display text. Do not
            // force symbols/unknown letters through IBIS_LinieKurs: that would turn "-10"
            // into line 0 (and matrix scripts would then blank or rewrite the rear sign).
            for name in ["SetLineTo", "Matrix_Nr", "Linie"] {
                if let Some(i) = p.vehicle.ty.program.str_var(name) {
                    p.vehicle.state.str_vars[i as usize] = line.to_string();
                }
            }
        }
        log::info!(
            "route number set by hand: {line} (IBIS_LinieKurs {:?}, Matrix_Nr {:?})",
            p.vehicle.var("IBIS_LinieKurs"),
            p.vehicle.str_var("Matrix_Nr")
        );
        app.service_msg = Some((format!("Route {line}"), 3.0));
    }
}

/// The route number the bus shows: its matrix's (`Matrix_Nr`), else the one it was set to
/// (`SetLineTo`), else its IBIS's line number.
fn line_shown(v: &omsi_sim::VehicleInstance) -> Option<String> {
    [v.str_var("Matrix_Nr"), v.str_var("SetLineTo")]
        .into_iter()
        .map(|s| s.trim().to_string())
        .find(|s| !s.is_empty())
        .or_else(|| ibis_line_number(v))
}

fn ibis_line_number(v: &omsi_sim::VehicleInstance) -> Option<String> {
    v.var("IBIS_LinieKurs").filter(|l| *l > 0.0).map(|l| format!("{}", l as i64))
}

/// The line a destination picked from the list is set with: the route number the bus
/// shows, when the IBIS can take it - its matrix's (`Matrix_Nr`), a roller blind's
/// (`SetLineTo`, which its crank sets) - else the IBIS's own number. Taken from
/// `IBIS_LinieKurs`, the IBIS's number without its letter, a pick made 92E into 92 on the
/// IBIS and the matrix. Not `SetLineTo` on any other bus: no script of its own writes it,
/// only an earlier pick, so a line typed on the IBIS since went back to that pick's.
pub(crate) fn destination_line(v: &omsi_sim::VehicleInstance) -> String {
    let blind = crate::schedule::has_roller_blind(v).then(|| v.str_var("SetLineTo"));
    [Some(v.str_var("Matrix_Nr")), blind]
        .into_iter()
        .flatten()
        .map(|s| s.trim().to_string())
        .find(|s| !s.is_empty())
        .filter(|l| numeric_ibis_line(l))
        .or_else(|| ibis_line_number(v))
        .unwrap_or_default()
}

/// Names in older packs often use underscores as spaces.
fn bus_label(name: &str) -> String {
    name.replace('_', " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Numbers inside names sort as numbers (DL9 before DL10), case does not matter.
fn bus_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (a, b) = (a.to_lowercase(), b.to_lowercase());
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let x: String = std::iter::from_fn(|| a.next_if(|c| c.is_ascii_digit())).collect();
                let y: String = std::iter::from_fn(|| b.next_if(|c| c.is_ascii_digit())).collect();
                let (x, y) = (x.trim_start_matches('0'), y.trim_start_matches('0'));
                let order = x.len().cmp(&y.len()).then_with(|| x.cmp(y));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(x), Some(y)) => {
                let order = x.cmp(&y);
                if order != Ordering::Equal {
                    return order;
                }
                a.next();
                b.next();
            }
        }
    }
}

/// The vehicles of the place list as (manufacturer's key, manufacturer, type, path): on a
/// server only those it offers (#1183).
fn place_vehicles(app: &App, unknown: &str) -> Vec<(String, String, String, String)> {
    let offered = crate::lan::server_offers();
    app.menus.vehicle_list
        .iter()
        .filter(|(_, path)| offered.as_deref().is_none_or(|o| crate::lan::offers(o, path)))
        .map(|(name, path)| {
            let (maker, ty) = app.menus.vehicle_meta.get(path).cloned().unwrap_or_default();
            let maker = bus_label(&maker);
            let ty = if ty.trim().is_empty() { bus_label(name) } else { bus_label(&ty) };
            let shown = if maker.is_empty() { unknown.to_string() } else { maker.clone() };
            (maker.to_lowercase(), shown, ty, path.clone())
        })
        .collect()
}

/// The paint schemes of a vehicle file, by name (without loading its meshes).
fn liveries(def: &omsi_vehicle::Vehicle) -> Vec<String> {
    let Some(m) = def.model.as_ref() else { return Vec::new() };
    let mp = omsi_cfg::resolve_path(def.dir(), m);
    let Ok(model) = omsi_model::Model::load(&mp) else { return Vec::new() };
    let mut names: Vec<String> = model.ctc.iter().flat_map(|c| omsi_sim::vehicle::load_paint_schemes(&omsi_cfg::resolve_path(def.dir(), &c.path))).map(|s| s.name).collect();
    names.dedup();
    names
}

/// A depot file's name for the lists: its `[name]`, and the file.
fn hof_label(p: &std::path::Path) -> String {
    let file = p.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    match omsi_vehicle::Hof::load(p).ok().map(|h| h.name).filter(|n| !n.trim().is_empty()) {
        Some(n) if !file.to_ascii_lowercase().starts_with(&n.trim().to_ascii_lowercase()) => format!("{}  ({file})", n.trim()),
        _ => file,
    }
}

/// The time speeds, traffic amounts and passenger shares the options step through.
const SPEEDS: [f64; 5] = [1.0, 2.0, 4.0, 8.0, 15.0];
pub(crate) const TRAFFIC: [usize; 7] = [0, 10, 20, 30, 50, 80, 120];
/// The most traffic anything may ask for: the menu's top step, the server's `traffic <n>`
/// and server.cfg's `traffic` alike (#1327).
pub(crate) const TRAFFIC_MAX: usize = TRAFFIC[TRAFFIC.len() - 1];
const PAX: [f32; 6] = [0.25, 0.5, 0.75, 1.0, 1.5, 2.0];
const VOLUME: [f32; 6] = [0.0, 0.2, 0.4, 0.6, 0.8, 1.0];
/// The pedal strengths the options step through (see `settings::pedal_curve`).
const PEDAL: [f32; 7] = [0.5, 0.7, 0.85, 1.0, 1.25, 1.5, 2.0];

/// The next of `steps` after `now` (round to the first).
pub(crate) fn next_step<T: PartialOrd + Copy>(steps: &[T], now: T) -> T {
    steps.iter().copied().find(|s| *s > now).unwrap_or(steps[0])
}

/// A list line that heads the lines under it: not chosen, not run.
pub(crate) const HEADING: &str = "#";
/// The end of the action of a line whose value Left and Right step down and up (and the
/// arrows drawn round its value): the action is run with `-` or `+` in its place, and with
/// it as it is on Enter (see `App::chooser_adjust`).
pub(crate) const ADJUST: &str = " ±";

pub(crate) fn keyboard_actions(app: &App) -> Vec<String> {
    let cfg = keyboard_cfg(app);
    cfg.vehicles.into_iter().chain(cfg.game).map(|b| b.action).collect()
}

fn keyboard_cfg(app: &App) -> omsi_content::KeyboardCfg {
    omsi_content::KeyboardCfg::load(&crate::startup::keyboard_cfg(&app.args.root))
        .unwrap_or_default()
        .with_game_defaults()
        .with_vr_defaults()
}

fn write_keyboard_cfg(_app: &App, cfg: &omsi_content::KeyboardCfg) -> Result<(), String> {
    let path = omsi_cfg::own_input("keyboard.cfg").ok_or_else(|| "No home folder was found to keep the key bindings in".to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("cfg.tmp");
    cfg.save(&tmp).map_err(|e| e.to_string())?;
    omsi_content::KeyboardCfg::load(&tmp).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(())
}

impl App {
    fn install_keyboard_cfg(&mut self, cfg: omsi_content::KeyboardCfg) {
        let runtime = cfg.with_game_defaults().with_vr_defaults();
        self.input.game_keys = runtime.game.clone();
        if let Some(p) = self.player.as_mut() {
            p.bindings = runtime.vehicles;
        }
        self.input.own_keys = crate::startup::own_keys(&self.args.root);
        self.input.own_shift = crate::startup::own_bindings(&self.args.root, omsi_content::input::KEY_SHIFT);
        // A key changed by the player must win over the ready-made W/A/S/D or arrow
        // presets just as a key changed on the launcher's Controls page does.
        self.args.drive_keys = "omsi".into();
        self.settings.drive_keys = "omsi".into();
        remember_setting("drive_keys", "omsi");
    }

    pub(crate) fn begin_key_capture(&mut self, game: bool, index: usize) {
        self.menus.key_capture = Some((game, index));
        self.service_msg = Some(("Press a key for this action (Delete clears it, Esc cancels)".into(), 5.0));
        self.refresh_list();
    }

    pub(crate) fn cancel_key_capture(&mut self) {
        if self.menus.key_capture.take().is_some() {
            self.service_msg = Some(("Key change cancelled".into(), 2.0));
            self.refresh_list();
        }
    }

    pub(crate) fn apply_key_capture(&mut self, scan: Option<i32>, chord: i32) {
        let Some((game, index)) = self.menus.key_capture else { return };
        let mut cfg = keyboard_cfg(self);
        let list = if game { &mut cfg.game } else { &mut cfg.vehicles };
        let Some(binding) = list.get_mut(index) else {
            self.menus.key_capture = None;
            self.service_msg = Some(("That key entry no longer exists".into(), 3.0));
            self.refresh_list();
            return;
        };
        let hold = binding.modifier & omsi_content::input::KEY_HOLD;
        binding.scan_code = scan.unwrap_or(0);
        binding.modifier = if scan.is_some() { hold | chord } else { hold };
        let action = binding.action.clone();
        match write_keyboard_cfg(self, &cfg) {
            Ok(()) => {
                self.menus.key_capture = None;
                self.install_keyboard_cfg(cfg);
                let key = scan.map(|s| crate::keys::key_name(s as i64, (hold | chord) as i64)).unwrap_or_else(|| "(unbound)".into());
                self.service_msg = Some((format!("{}: {key}", crate::describe::names(&self.args.root, &self.settings.language).control(&action)), 3.0));
                self.refresh_list();
            }
            Err(e) => self.service_msg = Some((format!("Key binding was not saved: {e}"), 5.0)),
        }
    }

    pub(crate) fn add_key_event(&mut self, action: &str) {
        let action = action.trim();
        if action.is_empty() {
            return;
        }
        let mut cfg = keyboard_cfg(self);
        cfg.vehicles.push(omsi_content::KeyBinding { action: action.to_string(), scan_code: 0, modifier: 0 });
        let index = cfg.vehicles.len() - 1;
        match write_keyboard_cfg(self, &cfg) {
            Ok(()) => {
                self.install_keyboard_cfg(cfg);
                self.menus.key_capture = Some((false, index));
                self.service_msg = Some((format!("KY_{action} added. Press its key now."), 5.0));
            }
            Err(e) => self.service_msg = Some((format!("Event was not added: {e}"), 5.0)),
        }
    }
}

fn keyboard_pages(app: &App) -> Vec<Page> {
    let cfg = keyboard_cfg(app);
    let names = crate::describe::names(&app.args.root, &app.settings.language);
    let mut vehicle = vec![opens("Add a vehicle event...", "Choose a control supplied by the bus or its mods", "key_events")];
    let mut game = Vec::new();
    for (is_game, bindings, rows) in [(false, &cfg.vehicles, &mut vehicle), (true, &cfg.game, &mut game)] {
        let mut bindings: Vec<_> = bindings.iter().enumerate().collect();
        bindings.sort_by_key(|(_, b)| names.control(&b.action).to_lowercase());
        for (i, b) in bindings {
            let key = if app.menus.key_capture == Some((is_game, i)) { "press a key...".into() }
                else { crate::keys::key_name(b.scan_code as i64, b.modifier as i64) };
            rows.push((row(&names.control(&b.action), 'a', &key, "Enter to change; Delete clears; Esc cancels", None),
                format!("keybind {} {i}", if is_game { "g" } else { "v" })));
        }
    }
    vec![("Driving and bus", vehicle), ("Game and camera", game)]
}

pub(crate) fn items(app: &App, kind: &ListKind) -> Vec<(String, String)> {
    let tr = |t: &str| omsi_ui::tr(t).into_owned();
    let mut out: Vec<(String, String)> = Vec::new();
    match kind {
        ListKind::Admin => return crate::admin::items(app),
        ListKind::Options(_) | ListKind::Vehicle(_) | ListKind::World(_) => {
            let Some((mut pages, tab)) = pages_of(app, kind) else { return out };
            if pages.is_empty() {
                return vec![(row("Nothing to set here", 'i', "", "", None), "noop".to_string())];
            }
            return pages.swap_remove(tab).1;
        }
        ListKind::ControllerDevices(_) | ListKind::Controller(..) | ListKind::ControllerAxis(..) | ListKind::ControllerButtons(_) | ListKind::ControllerButtonSettings(..) | ListKind::ControllerButton(..) | ListKind::ControllerCapture(_) => return crate::game_controller_menu::items(app, kind),
        ListKind::Controls => {
            return vec![
                opens("Keyboard", "Change driving, vehicle, game and camera key bindings", "keyboard"),
                opens("Game controllers", "Configure wheels, pedals, gamepads and force feedback", "controllers"),
            ];
        }
        ListKind::Keyboard(tab) => {
            return keyboard_pages(app).swap_remove((*tab).min(1)).1;
        }
        ListKind::Events => {
            let names = crate::describe::names(&app.args.root, &app.settings.language);
            let mut events = names.events();
            if let Some(p) = app.player.as_ref() {
                for action in p.vehicle.ty.program.trigger_names() {
                    if !events.iter().any(|(a, _)| a.eq_ignore_ascii_case(&action)) {
                        events.push((action.clone(), names.control(&action)));
                    }
                }
            }
            events.sort_by(|a, b| a.1.to_ascii_lowercase().cmp(&b.1.to_ascii_lowercase()).then_with(|| a.0.to_ascii_lowercase().cmp(&b.0.to_ascii_lowercase())));
            for (action, label) in events {
                out.push((format!("{label}  |  KY_{action}"), format!("key_event {action}")));
            }
            if out.is_empty() {
                out.push(("No vehicle events were found".into(), "back".into()));
            }
            return out;
        }
        ListKind::Lines => {
            if let Some(sch) = crate::duty_book::timetable(app) {
                let mut lines: Vec<&omsi_timetable::Line> = sch.data.lines.iter().filter(|l| l.user_allowed && l.tours.iter().any(|t| tour_listed(sch, &l.name, t, app.clock.time))).collect();
                lines.sort_by(|a, b| natural(&a.name, &b.name));
                for l in lines {
                    out.push((format!("{} {}  ({} {})", tr("Line"), l.name, l.tours.iter().filter(|t| tour_listed(sch, &l.name, t, app.clock.time)).count(), tr("tours")), format!("line {}", l.name)));
                }
            }
            if out.is_empty() {
                out.push((tr("No timetable on this map"), "back".into()));
            }
        }
        ListKind::Tours(line, _) => {
            if let Some(l) = crate::duty_book::timetable(app).and_then(|s| s.data.lines.iter().find(|l| l.name == *line)) {
                for t in sorted_tours(l).into_iter().filter(|t| crate::duty_book::timetable(app).is_some_and(|s| tour_listed(s, line, t, app.clock.time))) {
                    // (the tours in order of the time they start)
                    out.push((format!("{} {}", tr("Tour"), t.number.trim()), format!("tour {}\u{1}{}", line, t.number)));
                }
            }
        }
        ListKind::Drivers => {
            for name in driver_names(app) {
                let mark = if app.session.career.path.as_ref().and_then(|p| p.file_stem()).is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case(&name)) { format!("  {}", tr("(now)")) } else { String::new() };
                out.push((format!("{name}{mark}"), format!("driver {name}")));
            }
        }
        ListKind::Destinations => {
            if let Some(p) = app.player.as_ref().filter(|p| p.vehicle.host.hof.is_some()) {
                let now = line_shown(&p.vehicle).unwrap_or_else(|| "-".into());
                out.push((format!("{}: {now}...", tr("Route number")), "routes".into()));
            }
            if let Some(hof) = app.player.as_ref().and_then(|p| p.vehicle.host.hof.clone()) {
                let mut termini: Vec<(String, i32, usize)> = hof
                    .termini
                    .iter()
                    .enumerate()
                    .map(|(i, t)| (t.menu_name(), t.code, i))
                    .collect();
                // (alphabetical, by name; picked by its row: several may share a name or a code)
                termini.sort_by_key(|(name, ..)| name.trim().to_lowercase());
                for (name, code, i) in termini {
                    out.push((format!("{:>3}  {}", code, name.trim()), format!("dest {i}")));
                }
            }
            if out.is_empty() {
                out.push((tr("This bus has no depot file (.hof) with destinations"), "back".into()));
            }
        }
        ListKind::RouteNumbers => {
            // any route number, typed as on OMSI's own field (#836): the scripts that read
            // it (a bus that switches its functions by route number) take what is typed
            match app.menus.menu_edit.as_ref() {
                Some(t) => out.push((format!("{}: {t}_  ({})", tr("Route number"), tr("Enter sets it, Esc cancels")), "route_type".into())),
                None => out.push((format!("{}...", tr("Type a route number")), "route_type".into())),
            }
            for l in route_numbers(app) {
                out.push((format!("{} {l}", tr("Route")), format!("route {l}")));
            }
            if out.len() == 1 {
                out.push((tr("No route numbers in the depot file or the timetable"), "back".into()));
            }
        }
        ListKind::Hofs => {
            if let Some(p) = app.player.as_ref() {
                let now = p.vehicle.host.hof.as_ref().map(|h| h.path.clone());
                let mut files: Vec<(String, std::path::PathBuf)> = omsi_vehicle::hof::depot_files(p.vehicle.ty.def.dir()).into_iter().map(|f| (hof_label(&f), f)).collect();
                // (alphabetical)
                files.sort_by_key(|(label, _)| label.to_lowercase());
                for (label, f) in files {
                    let mark = if now.as_ref() == Some(&f) { format!("  {}", tr("(now)")) } else { String::new() };
                    out.push((format!("{label}{mark}"), format!("hof {}", f.to_string_lossy())));
                }
            }
            if out.is_empty() {
                out.push((tr("This bus has no depot files (.hof)"), "back".into()));
            }
        }
        ListKind::Spots => {
            if let Some(w) = app.world.as_ref() {
                // (the map's entry points, as the launcher's "Start at" lists them)
                for (i, e) in w.global.entry_points.iter().enumerate() {
                    let label = if e.name.trim().is_empty() { format!("{} {}", tr("entry"), e.index + 1) } else { e.name.trim().to_string() };
                    out.push((label, format!("spot {i}")));
                }
            }
            if out.is_empty() {
                out.push((tr("This map has no entry points"), "back".into()));
            }
        }
        ListKind::PlaceMaker => {
            let all = place_vehicles(app, &tr("Unknown manufacturer"));
            let mut groups: Vec<(String, String, Vec<&(String, String, String, String)>)> = Vec::new();
            for v in &all {
                match groups.iter_mut().find(|g| g.0 == v.0) {
                    Some(g) => g.2.push(v),
                    None => groups.push((v.0.clone(), v.1.clone(), vec![v])),
                }
            }
            groups.sort_by(|a, b| bus_cmp(&a.1, &b.1).then_with(|| a.0.cmp(&b.0)));
            let searching = !search_query(app).trim().is_empty();
            for (key, name, vs) in groups {
                if searching {
                    // (searched for: every type, under its manufacturer's name)
                    for v in vs {
                        out.push((format!("{name}  |  {}", v.2), format!("bus {}", v.3)));
                    }
                } else if vs.len() == 1 {
                    // (a manufacturer with one type: that type at once)
                    out.push((format!("{name}  |  {}", vs[0].2), format!("bus {}", vs[0].3)));
                } else {
                    out.push((format!("{name}  ({} {})", vs.len(), tr("models")), format!("maker {key}")));
                }
            }
        }
        ListKind::PlaceType(key) => {
            let all = place_vehicles(app, &tr("Unknown manufacturer"));
            let mut types: Vec<(String, String)> = all.iter().filter(|v| v.0 == *key).map(|v| (v.2.clone(), v.3.clone())).collect();
            types.sort_by(|a, b| bus_cmp(&a.0, &b.0).then_with(|| a.1.cmp(&b.1)));
            // (a type name used twice: with its pack's folder, then with its file)
            let same = |t: &[(String, String)], n: &str| t.iter().filter(|x| x.0.to_lowercase() == n.to_lowercase()).count();
            let counts: Vec<usize> = types.iter().map(|t| same(&types, &t.0)).collect();
            for (t, n) in types.iter().zip(counts) {
                let mut label = t.0.clone();
                if n > 1 {
                    let parts: Vec<&str> = t.1.split('/').collect();
                    let folder = parts.get(1).copied().unwrap_or_default();
                    let file = parts.last().copied().unwrap_or_default().rsplit_once('.').map(|x| x.0).unwrap_or_default();
                    label = format!("{label}  |  {}  |  {}", bus_label(folder), bus_label(file));
                }
                out.push((label, format!("bus {}", t.1)));
            }
        }
        ListKind::PlaceLivery(bus) => {
            out.push((tr("Random livery"), "livery ".into()));
            let mut names = bus_def(app, bus).map(|d| liveries(&d)).unwrap_or_default();
            // (alphabetical)
            names.sort_by_key(|n| n.to_lowercase());
            names.dedup();
            for n in names {
                out.push((n.clone(), format!("livery {n}")));
            }
        }
        ListKind::PlaceHof(bus, _) => {
            out.push((tr("The map's depot file"), "placehof ".into()));
            if let Some(d) = bus_def(app, bus) {
                let mut files: Vec<(String, String)> = omsi_vehicle::hof::depot_files(d.dir())
                    .into_iter()
                    .map(|f| (hof_label(&f), f.file_name().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default()))
                    .collect();
                // (alphabetical)
                files.sort_by_key(|(label, _)| label.to_lowercase());
                for (label, name) in files {
                    out.push((label, format!("placehof {name}")));
                }
            }
        }
        ListKind::Numbers => {
            if let Some(p) = app.player.as_ref() {
                let mut numbers = fleet_numbers(&p.vehicle);
                // (in order of the numbers)
                numbers.sort_by(|a, b| natural(&a.0, &b.0));
                for (n, reg) in numbers {
                    out.push((if reg.is_empty() { n.clone() } else { format!("{n}  ({reg})") }, format!("number {n}\u{1}{reg}")));
                }
            }
            if out.is_empty() {
                out.push((tr("This bus has no list of fleet numbers"), "back".into()));
            }
        }
    }
    if searchable(kind) {
        let query = search_query(app);
        out.retain(|(label, action)| search_matches(label, action, query));
        let label = if app.menus.menu_edit_search {
            format!("{}: {query}_  ({})", tr("Search..."), tr("Enter sets it, Esc cancels"))
        } else if query.is_empty() {
            tr("Search...")
        } else {
            format!("{}: {query}", tr("Search..."))
        };
        out.insert(0, (label, "search".into()));
    }
    out.push((tr("Back"), "back".into()));
    out
}

/// What the open list tells the interface besides its lines: the layout it wants, its title
/// and the small line above it, and - for lines and tours - the timetable of the chosen one.
pub(crate) fn menu_extras(
    kind: Option<&ListKind>,
    list: Option<&[(String, String)]>,
    sel: Option<usize>,
    schedule: Option<&crate::schedule::Schedule>,
    now: f64,
) -> (crate::ui::MenuKind, Option<(String, String)>, Option<crate::ui::Preview>) {
    use crate::ui::{MenuKind, Preview};
    let Some(sel) = sel else { return (MenuKind::Game, None, None) };
    let tr = |t: &str| omsi_ui::tr(t).into_owned();
    // (the menu's own labels, translated, without their dots)
    let title = |t: &str| tr(t).trim_end_matches("...").trim_end_matches('…').trim_end().to_string();
    let head = |t: &str| Some((title(t), String::new()));
    let hm = |m: f32| format!("{:02}:{:02}", (m / 60.0) as i32 % 24, (m % 60.0) as i32);
    // a trip's line and terminus
    let trip_of = |name: &str| -> (String, String) {
        schedule
            .and_then(|s| s.data.trips.iter().find(|x| x.name.eq_ignore_ascii_case(name)))
            .map(|x| (x.line.trim().to_string(), x.terminus.trim().to_string()))
            .unwrap_or_default()
    };
    let action = list.and_then(|l| l.get(sel)).map(|x| x.1.as_str()).unwrap_or("");
    // (no kind: the vehicle chooser)
    let Some(kind) = kind else { return (MenuKind::List, head("Place a vehicle..."), None) };
    match kind {
        ListKind::Options(_) => (MenuKind::Options, head("Options..."), None),
        ListKind::Vehicle(_) => (MenuKind::Options, head("Vehicle options..."), None),
        ListKind::World(_) => (MenuKind::Options, head("World options..."), None),
        ListKind::Controls => (MenuKind::Options, head("Controls..."), None),
        ListKind::Keyboard(_) => (MenuKind::Options, head("Keyboard"), None),
        ListKind::ControllerDevices(_) => (MenuKind::Options, head("Game controllers"), None),
        ListKind::Controller(name, _) => (MenuKind::Options, Some((name.clone(), String::new())), None),
        ListKind::ControllerAxis(name, a) => (MenuKind::Options, Some((format!("{} | Axis {}", name, a + 1), String::new())), None),
        ListKind::ControllerButtons(name) => (MenuKind::List, Some(("Choose a button".into(), name.clone())), None),
        ListKind::ControllerButtonSettings(name, b) => (MenuKind::Options, Some((crate::game_controller_menu::button_label(*b), name.clone())), None),
        ListKind::ControllerButton(_, b) => (MenuKind::List, Some((format!("{}: choose an action", crate::game_controller_menu::button_label(*b)), String::new())), None),
        ListKind::ControllerCapture(_) => (MenuKind::List, head("Assign a physical button"), None),
        ListKind::Events => (MenuKind::List, Some((tr("Add event"), String::new())), None),
        ListKind::Lines => {
            let preview = action.strip_prefix("line ").and_then(|name| {
                let line = schedule?.data.lines.iter().find(|l| l.name == name)?;
                let rows = sorted_tours(line)
                    .into_iter()
                    .filter(|t| schedule.is_some_and(|s| tour_listed(s, &line.name, t, now)))
                    .map(|t| {
                        // (the trip and the time the tour has from the game's time on)
                        let next = schedule.and_then(|s| s.tour_stops_from(&line.name, &t.number, now).first().cloned());
                        let end = match (schedule, next.as_ref()) {
                            (Some(s), Some(n)) => tour_trip_name(s, t, n.0).map(|name| trip_of(&name).1).unwrap_or_default(),
                            _ => t.trips.first().map(|tt| trip_of(&tt.trip).1).unwrap_or_default(),
                        };
                        let what = if end.is_empty() { format!("{} {}", tr("Tour"), t.number.trim()) } else { format!("{} {}  ›  {}", tr("Tour"), t.number.trim(), end) };
                        let when = match next {
                            Some(n) => hm((n.3 / 60.0) as f32),
                            None => t.trips.first().map(|tt| hm(tt.departure)).unwrap_or_default(),
                        };
                        (what, when)
                    })
                    .collect();
                Some(Preview { title: format!("{} {}", tr("Line"), line.name), meta: format!("{} {}", line.tours.iter().filter(|t| schedule.is_some_and(|s| tour_listed(s, &line.name, t, now))).count(), tr("tours")), rows, chosen: None, button: None, time: None })
            });
            (MenuKind::Lines, head("Line and tour..."), preview)
        }
        ListKind::Tours(line_name, pick) => {
            // (the line number of the trip of a tour shown: a tour may run trips of several lines)
            let tour_line = |ln: &str, num: &str| -> Option<String> {
                let sch = schedule?;
                let line = sch.data.lines.iter().find(|l| l.name == ln)?;
                let tour = line.tours.iter().find(|t| t.number == num)?;
                let n_trips = sch.tour_trip_count(ln, num);
                let trip = pick.as_ref().filter(|p| p.0 == num).map(|p| p.2).unwrap_or_else(|| sch.tour_trip_now(ln, num, now)).min(n_trips.saturating_sub(1));
                let stops = sch.tour_trip_stops(ln, num, trip);
                stops.first().and_then(|s| tour_trip_name(sch, tour, s.0)).map(|n| trip_of(&n).0).filter(|l| !l.is_empty())
            };
            let preview = action.strip_prefix("tour ").and_then(|rest| rest.split_once('\u{1}')).and_then(|(ln, num)| {
                let sch = schedule?;
                let line = sch.data.lines.iter().find(|l| l.name == ln)?;
                let tour = line.tours.iter().find(|t| t.number == num)?;
                let n_trips = sch.tour_trip_count(ln, num);
                let trip = pick.as_ref().filter(|p| p.0 == num).map(|p| p.2).unwrap_or_else(|| sch.tour_trip_now(ln, num, now)).min(n_trips.saturating_sub(1));
                let stops = sch.tour_trip_stops(ln, num, trip);
                let at = stops.first().map(|s| s.3).unwrap_or_else(|| tour_start(tour).unwrap_or(0.0));
                let chosen = pick.as_ref().filter(|p| p.0 == num).map(|p| p.1).unwrap_or(0).min(stops.len().saturating_sub(1));
                let rows = stops.iter().map(|s| (s.2.trim().to_string(), hm((s.3 / 60.0) as f32))).collect();
                let trip_line = tour_line(ln, num).unwrap_or_else(|| line_sign(schedule, line));
                Some(Preview {
                    title: format!("{} {}", tr("Tour"), num.trim()),
                    meta: format!("{} {}  |  {} {}/{}  |  {}", tr("Line"), trip_line, tr("Trip"), trip + 1, n_trips.max(1), tr("Choose the stop to start from")),
                    rows,
                    chosen: Some(chosen),
                    button: Some(tr("Start trip")),
                    // (the time of the trip: between the arrows that step through the tour's trips)
                    time: Some(hm((at / 60.0) as f32)),
                })
            });
            let chosen_line = action.strip_prefix("tour ").and_then(|rest| rest.split_once('\u{1}')).and_then(|(ln, num)| tour_line(ln, num));
            let sign = chosen_line.or_else(|| schedule.and_then(|s| s.data.lines.iter().find(|l| l.name == *line_name)).map(|l| line_sign(schedule, l))).unwrap_or_else(|| line_name.clone());
            (MenuKind::Tours, Some((title("Line and tour..."), format!("{} {}", tr("Line"), sign))), preview)
        }
        ListKind::Drivers => (MenuKind::List, head("Driver..."), None),
        ListKind::Numbers => (MenuKind::List, head("Fleet number..."), None),
        ListKind::Destinations => (MenuKind::List, head("Destination display..."), None),
        ListKind::RouteNumbers => (MenuKind::List, Some((tr("Route number"), String::new())), None),
        ListKind::Hofs => (MenuKind::List, head("Depot file (HOF)..."), None),
        ListKind::Spots => (MenuKind::List, head("Teleport to a start point..."), None),
        ListKind::PlaceMaker | ListKind::PlaceType(_) | ListKind::PlaceLivery(_) | ListKind::PlaceHof(..) => (MenuKind::List, head("Place a vehicle..."), None),
        ListKind::Admin => (MenuKind::List, Some((tr("Administration"), String::new())), None),
    }
}

/// Do a line of the list; returns the list to show next (None: back to the menu).
pub(crate) fn run(app: &mut App, kind: &ListKind, action: &str) -> Option<ListKind> {
    run_move(app, kind, action, Move::Next)
}

/// Do a line of the list, a setting changed as `mv` says (Enter and a click on a line are
/// `Move::Next`; the arrows and a click on a slider or a stepper the others).
pub(crate) fn run_move(app: &mut App, kind: &ListKind, action: &str, mv: Move) -> Option<ListKind> {
    if crate::game_controller_menu::is_controller_list(Some(kind)) {
        return crate::game_controller_menu::run(app, kind, action, mv);
    }
    if action == "search" && searchable(kind) {
        app.menus.menu_edit = Some(app.menus.menu_search.clone());
        app.menus.menu_edit_search = true;
        return Some(kind.clone());
    }
    if action == "back" {
        return match kind {
            ListKind::Tours(..) => Some(ListKind::Lines),
            ListKind::Events => Some(ListKind::Keyboard(0)),
            ListKind::Keyboard(_) => Some(ListKind::Controls),
            ListKind::PlaceType(_) | ListKind::PlaceLivery(_) | ListKind::PlaceHof(..) => Some(ListKind::PlaceMaker),
            _ => None,
        };
    }
    let (verb, arg) = action.split_once(' ').unwrap_or((action, ""));
    match kind {
        ListKind::ControllerDevices(_) | ListKind::Controller(..) | ListKind::ControllerAxis(..) | ListKind::ControllerButtons(_) | ListKind::ControllerButtonSettings(..) | ListKind::ControllerButton(..) | ListKind::ControllerCapture(_) => unreachable!("controller lists handled above"),
        ListKind::Admin => {
            crate::admin::run(app, action);
            Some(ListKind::Admin)
        }
        ListKind::Options(_) | ListKind::World(_) => {
            if verb == "noop" || option_do(app, verb, arg, mv) {
                return Some(kind.clone());
            }
            // (the arrows and a click on a stepper only change values: no buttons)
            let step = matches!(mv, Move::Next);
            match verb {
                "vr_nav_edit" if step && app.vr_active() && app.player.is_some() => {
                    app.start_vr_nav_edit();
                    return None;
                }
                "vr_nav_reset" if step && app.vr_active() && app.player.is_some() => {
                    app.vr_nav_adjust("reset", 1.0);
                    app.menus.list_dirty = true;
                }
                // (the preset, the clouds and the precipitation are picked from a drop-down: `App::chooser_pick`)
                "weather" | "cloudkind" | "precipkind" | "metar_src" | "sel" | "preset" | "gfxprofile" | "reset" => {}
                "metar_icao_edit" if step => {
                    if app.menus.menu_edit_icao { app.apply_icao_edit(); } else { app.start_icao_edit(); }
                }
                // the exact time: Enter starts typing it, and sets it when typed
                "time_edit" if step => {
                    if app.menus.menu_edit.is_some() {
                        app.apply_time_edit();
                    } else if app.net.lan.as_ref().is_some_and(|l| l.role == omsi_net::Role::Client) {
                        app.service_msg = Some(("In a LAN session the host sets the clock".into(), 3.0));
                    } else if app.real_time_locked() {
                        app.service_msg = Some(("The time cannot be changed while the real-time sync is on".into(), 3.0));
                    } else {
                        app.menus.menu_edit = Some(String::new());
                    }
                }
                "view_editor" if step => { crate::game_panels::view_editor::open(app); return None; }
                "seat_reset" if step => {
                    // (this bus's views as its .bus file has them again)
                    app.settings.seat = [0.0; 3];
                    app.settings.seat_pitch_deg = 0.0;
                    remember_bus_seat(app);
                }
                "clock_ontime" if step => {
                    if app.net.lan.as_ref().is_some_and(|l| l.role == omsi_net::Role::Client) {
                        app.service_msg = Some(("In a LAN session the host sets the clock".into(), 3.0));
                    } else if let Some(d) = app.player.as_ref().map(|p| p.vehicle.host.tt_delay as f64).filter(|d| d.abs() >= 1.0) {
                        // (the delay as it is now, not as the button was drawn: a second click
                        // would otherwise move the clock by the old amount again)
                        app.shift_clock(-d);
                        if let Some(p) = app.player.as_mut() {
                            p.vehicle.host.tt_delay = 0.0;
                        }
                    }
                }
                "clock_set" | "clock_shift" if step => {
                    if app.net.lan.as_ref().is_some_and(|l| l.role == omsi_net::Role::Client) {
                        app.service_msg = Some(("In a LAN session the host sets the clock".into(), 3.0));
                    } else if let Ok(secs) = arg.trim().parse::<f64>() {
                        let by = if verb == "clock_set" { secs - app.clock.time } else { secs };
                        app.shift_clock(by);
                    }
                }
                "traffic_clear" if step => {
                    crate::admin::clear_ai_traffic(app);
                }
                other if step => {
                    app.page_action(other);
                    return None;
                }
                _ => {}
            }
            Some(kind.clone())
        }
        ListKind::Vehicle(_) => {
            if matches!(mv, Move::Next) {
                app.page_action(verb);
                return None;
            }
            Some(kind.clone())
        }
        ListKind::Controls => {
            if !matches!(mv, Move::Next) { return Some(kind.clone()); }
            match verb {
                "keyboard" => Some(ListKind::Keyboard(0)),
                "controllers" => Some(ListKind::ControllerDevices(0)),
                _ => Some(kind.clone()),
            }
        }
        ListKind::Keyboard(_) => {
            if !matches!(mv, Move::Next) { return Some(kind.clone()); }
            match verb {
                "key_events" => Some(ListKind::Events),
                "keybind" => {
                    let mut p = arg.split_whitespace();
                    let game = p.next() == Some("g");
                    if let Some(index) = p.next().and_then(|n| n.parse::<usize>().ok()) {
                        app.begin_key_capture(game, index);
                    }
                    Some(kind.clone())
                }
                _ => Some(kind.clone()),
            }
        }
        ListKind::Events => {
            if matches!(mv, Move::Next) && verb == "key_event" {
                app.add_key_event(arg);
                Some(ListKind::Keyboard(0))
            } else {
                Some(ListKind::Events)
            }
        }
        ListKind::Lines => match verb {
            "line" => {
                Some(ListKind::Tours(arg.to_string(), None))
            }
            "free" => {
                end_duty(app);
                None
            }
            _ => None,
        },
        ListKind::Tours(_, pick) => {
            if let Some((line, tour)) = arg.split_once('\u{1}') {
                let chosen = pick.as_ref().filter(|p| p.0 == tour).map(|p| p.1).unwrap_or(0);
                let trip = pick.as_ref().filter(|p| p.0 == tour).map(|p| p.2).unwrap_or_else(|| crate::duty_book::timetable(app).map(|s| s.tour_trip_now(line, tour, app.clock.time)).unwrap_or(0));
                start_duty_at(app, line, tour, trip, chosen);
            }
            None
        }
        ListKind::Drivers => {
            switch_driver(app, arg);
            Some(ListKind::Drivers)
        }
        ListKind::Hofs => {
            if let Some(p) = app.player.as_mut() {
                match omsi_vehicle::Hof::load(std::path::Path::new(arg)) {
                    Ok(h) => {
                        let name = h.name.clone();
                        p.vehicle.host.hof = Some(std::sync::Arc::new(h));
                        app.service_msg = Some((format!("Depot file: {}", name.trim()), 3.0));
                    }
                    Err(e) => app.service_msg = Some((format!("Depot file: {e}"), 4.0)),
                }
            }
            None
        }
        ListKind::Spots => {
            if app.net.lan.as_ref().is_some_and(|l| l.role == omsi_net::Role::Client) {
                app.service_msg = Some(("In a LAN session only the host moves vehicles on the map".into(), 4.0));
                return None;
            }
            let found = app.world.clone().and_then(|w| {
                let ep = arg.trim().parse::<usize>().ok().and_then(|i| w.global.entry_points.get(i))?;
                // (the entry points of tiles that are not loaded come from the map index)
                w.index();
                w.entry_point_place(ep).map(|(pos, rot)| (pos, rot[0]))
            });
            match found {
                Some((pos, heading)) => {
                    crate::admin::teleport(app, pos, heading);
                    app.service_msg = Some(("The bus stands at the start point".into(), 3.0));
                    app.service_event("teleport", app.menu_by(), None);
                }
                None => app.service_msg = Some(("That start point is not in the map".into(), 3.0)),
            }
            None
        }
        ListKind::PlaceMaker | ListKind::PlaceType(_) if verb == "maker" => Some(ListKind::PlaceType(arg.to_string())),
        ListKind::PlaceMaker | ListKind::PlaceType(_) => Some(ListKind::PlaceLivery(arg.to_string())),
        ListKind::PlaceLivery(bus) => Some(ListKind::PlaceHof(bus.clone(), arg.to_string())),
        ListKind::PlaceHof(bus, paint) => {
            let (bus, paint, hof) = (bus.clone(), paint.clone(), arg.trim().to_string());
            app.close_game_menu();
            app.place_vehicle(&bus, Some(paint).filter(|p| !p.is_empty()), Some(hof).filter(|h| !h.is_empty()));
            None
        }
        ListKind::Destinations if verb == "routes" => Some(ListKind::RouteNumbers),
        ListKind::RouteNumbers if verb == "route_type" => {
            // the first press starts typing, the next one (Enter) sets what is typed
            match app.menus.menu_edit.take() {
                Some(t) => {
                    set_route_by_hand(app, &t);
                    None
                }
                None => {
                    app.menus.menu_edit = Some(String::new());
                    Some(ListKind::RouteNumbers)
                }
            }
        }
        ListKind::RouteNumbers => {
            set_route_by_hand(app, arg);
            None
        }
        ListKind::Destinations => {
            if let Some(p) = app.player.as_mut() {
                let hof = p.vehicle.host.hof.clone();
                let ti: usize = arg.trim().parse().unwrap_or(usize::MAX);
                if let Some((hof, t)) = hof.as_ref().and_then(|h| h.termini.get(ti).map(|t| (h, t))) {
                    // (the line on the IBIS stays; only the destination changes)
                    let line = destination_line(&p.vehicle);
                    let name = t.menu_name();
                    p.set_destination_by_hand(hof, &line, ti);
                    log::info!("destination display set by hand: {} {} (terminus code now {:?})", t.code, name.trim(), p.vehicle.var("IBIS_TerminusCode"));
                    app.service_msg = Some((format!("Destination: {}", name.trim()), 3.0));
                }
            }
            None
        }
        ListKind::Numbers => {
            if let (Some((n, reg)), Some(p)) = (arg.split_once('\u{1}'), app.player.as_mut()) {
                let v = &mut p.vehicle;
                if let Some(i) = v.ty.program.str_var("number") {
                    v.state.str_vars[i as usize] = n.to_string();
                }
                if !reg.is_empty() {
                    if let Some(i) = v.ty.program.str_var("ident") {
                        v.state.str_vars[i as usize] = reg.to_string();
                    }
                }
                app.service_msg = Some((format!("Fleet number {n}"), 3.0));
            }
            None
        }
    }
}

// the settings windows (options, vehicle, world), one concern a file
mod rows;
mod options;
mod dropdowns;
mod menu_pages;
mod duty;
pub(crate) use rows::*;
pub(crate) use options::*;
pub(crate) use dropdowns::*;
pub(crate) use menu_pages::*;
pub(crate) use duty::*;

#[cfg(test)]
mod tests {
    #[test]
    fn searches_model_and_hof_names_and_paths_without_losing_their_actions() {
        assert!(super::search_matches("Volvo BRT", "bus Vehicles/Pack/biarticulado.bus", "VOLVO biarticulado"));
        assert!(super::search_matches("Curitiba", "hof Vehicles/Bus/curitiba.hof", "curitiba .hof"));
        assert!(!super::search_matches("Curitiba", "hof curitiba.hof", "recife"));
        assert!(super::search_matches("Anything", "bus x.bus", "  "));
    }

    /// The options page names the preset a file holds after the launcher saved it, in every
    /// graphics mode, and "Custom" once one of its values is changed.
    #[test]
    fn the_preset_is_recognised_in_every_mode() {
        for mode in ["vanilla", "vanilla_plus", "enhanced", "enhanced_plus"] {
            for (i, (_, preset)) in omsi_launcher_lib::graphics_presets_for(mode).into_iter().enumerate() {
                let mut file = omsi_launcher_lib::settings_from_text(None);
                omsi_launcher_lib::apply_graphics_profile(&preset, &mut file);
                let mut file = omsi_launcher_lib::settings_from_text(Some(&omsi_launcher_lib::settings_to_text(&file, None)));
                assert_eq!(super::preset_now(&file), Some(i), "{mode}: {i}");
                file["mirror_size"] = serde_json::json!(1024);
                assert_eq!(super::preset_now(&file), None);
            }
        }
    }

    /// A destination picked from the list keeps the route number the bus shows, its letter
    /// too (92E, IBIS 92 and suffix 10).
    #[test]
    fn a_destination_picked_from_the_list_keeps_the_lines_letter() {
        let mut v = crate::schedule::tests::script_test_vehicle("{frame}\n{end}\n", "IBIS_LinieKurs\nIBIS_Linie_Suffix\nIBIS_Linie_Complex\nIBIS_TerminusCode\n", "Matrix_Nr\nSetLineTo\n");
        let set_str = |v: &mut omsi_sim::VehicleInstance, name: &str, s: &str| {
            let i = v.ty.program.str_var(name).unwrap();
            v.state.str_vars[i as usize] = s.to_string();
        };
        v.set_var("IBIS_LinieKurs", 92.0);
        v.set_var("IBIS_Linie_Suffix", 10.0);
        v.set_var("IBIS_Linie_Complex", 9210.0);
        set_str(&mut v, "Matrix_Nr", "92E");
        assert_eq!(super::destination_line(&v), "92E");
        let hof = omsi_vehicle::Hof { termini: vec![omsi_vehicle::hof::Terminus { code: 211, texture_id: "U Rathaus Spandau".into(), strings: vec!["RATHAUS SPANDAU".into()], ..Default::default() }], ..Default::default() };
        let line = super::destination_line(&v);
        crate::schedule::set_player_destination_at(&mut v, &hof, &line, 0, &[]);
        assert_eq!((v.var("IBIS_LinieKurs"), v.var("IBIS_Linie_Suffix"), v.var("IBIS_Linie_Complex"), v.var("IBIS_TerminusCode")), (Some(92.0), Some(10.0), Some(9210.0), Some(211.0)));
        // a route number the IBIS cannot take is left to the display: the IBIS keeps its own
        set_str(&mut v, "Matrix_Nr", "-10");
        assert_eq!(super::destination_line(&v), "92");
        // nothing shown: the IBIS's number
        set_str(&mut v, "Matrix_Nr", "   ");
        set_str(&mut v, "SetLineTo", "");
        assert_eq!(super::destination_line(&v), "92");
        // no matrix line, the line of an earlier pick left in SetLineTo and another typed on
        // the IBIS since: the IBIS's (only a roller blind shows SetLineTo)
        set_str(&mut v, "Matrix_Nr", "");
        set_str(&mut v, "SetLineTo", "5");
        v.set_var("IBIS_LinieKurs", 145.0);
        assert_eq!(super::destination_line(&v), "145");
        let mut blind = crate::schedule::tests::script_test_vehicle("{trigger:rollband_sync}\n{end}\n", "IBIS_LinieKurs\n", "SetLineTo\n");
        set_str(&mut blind, "SetLineTo", "  5");
        blind.set_var("IBIS_LinieKurs", 145.0);
        assert_eq!(super::destination_line(&blind), "5");
    }

    #[test]
    fn escape_fov_updates_the_active_projection_and_can_restore_geometry() {
        let mut settings = crate::settings::Settings::default();
        assert_eq!(super::set_camera_fov(&mut settings, 50.0).0, "fov");
        settings.triple.enabled = true;
        assert_eq!(
            super::set_camera_fov(&mut settings, 75.0).0,
            "triple_fov_deg"
        );
        assert_eq!(settings.fov, 50.0);
        assert_eq!(settings.triple.fov_deg, 75.0);
        super::set_camera_fov(&mut settings, 0.0);
        assert_eq!(settings.triple.fov_deg, 0.0);
    }
    /// The game menu offers the 16x anisotropic filtering the launcher does (#669): set
    /// there, it showed as a bare "16" here and could not be chosen again.
    #[test]
    fn sixteen_x_anisotropy_can_be_chosen_in_the_game_menu() {
        let file = serde_json::json!({ "anisotropy": 16 });
        let (options, at, _) = super::select_state(&file, "anisotropy");
        assert_eq!(at.map(|i| options[i]), Some(("16", "16x")));
    }

    #[test]
    fn steps_wrap_round() {
        assert_eq!(super::next_step(&super::SPEEDS, 1.0), 2.0);
        assert_eq!(super::next_step(&super::SPEEDS, 15.0), 1.0);
        assert_eq!(super::next_step(&super::TRAFFIC, 35), 50);
    }

    #[test]
    fn mouse_pedal_strength_steps_include_percentage_range_and_omsi_default() {
        let steps = super::steps_of("mouse_pedal_strength").unwrap();
        assert_eq!(steps.first(), Some(&0.5));
        assert_eq!(steps.last(), Some(&2.0));
        assert!(steps.contains(&1.0));
    }

    #[test]
    fn lines_sort_as_numbers() {
        let mut v = vec!["13N", "5", "137", "N30", "92"];
        v.sort_by(|a, b| super::natural(a, b));
        assert_eq!(v, vec!["5", "13N", "92", "137", "N30"]);
    }

    #[test]
    fn symbols_are_not_forced_through_numeric_ibis_lines() {
        assert!(super::numeric_ibis_line("10"));
        assert!(super::numeric_ibis_line("10E"));
        assert!(super::numeric_ibis_line("X10"));
        assert!(!super::numeric_ibis_line("-10"));
        assert!(!super::numeric_ibis_line("10-"));
        assert!(!super::numeric_ibis_line("EXP"));
    }
}
