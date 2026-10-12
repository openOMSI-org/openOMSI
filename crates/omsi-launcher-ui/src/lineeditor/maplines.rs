//! The lines of the map's own in the line editor (Luc: "bestaande lijnen van de kaarten"):
//! listed apart under the player's lines, each opened as a line to edit as one of his own
//! (`core::lines::import_map_line`: its trips' stops per direction, the map's own way between
//! them, its departures in the map's tours, its destinations, number and depot group). Saved,
//! the game drives the edited line instead of the map's (it is written as the map's own line
//! file, in openOMSI's content folder); "Restore the map's original" brings the map's back
//! exactly and drops the edit. Working for the bus company, the list has the map's lines the
//! company runs, and a changed route is approved anew (`core::company::ownline::
//! map_change_fee`).

use super::*;
use omsi_timetable::{Line, TimetableData};

/// The height of the list's heading over the map's lines.
pub(super) const HEAD_H: f32 = 30.0;

/// A line of the map's timetable: its `.ttl`'s name, the number its trips show most, where it
/// goes and how many tours it has.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct MapRow {
    pub ttl: String,
    pub number: String,
    pub termini: Vec<String>,
    pub tours: usize,
}

/// A line of the map's as the list shows it: its row, the plate's number and colour (an edited
/// line's own), and whether the player edited it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Shown {
    pub ttl: String,
    pub number: String,
    pub label: String,
    pub colour: String,
    pub edited: bool,
}

/// The map's own lines of the timetable the game reads (the player's own files left out), in
/// the order of their names (2 before 10).
pub(super) fn map_rows(tt: &TimetableData) -> Vec<MapRow> {
    let mut out: Vec<MapRow> = tt
        .lines
        .iter()
        .filter(|l| !reg::is_own_file(&l.name))
        .map(|l| {
            let mut numbers: Vec<(String, usize)> = Vec::new();
            let mut termini: Vec<String> = Vec::new();
            for tt_trip in l.tours.iter().flat_map(|t| t.trips.iter()) {
                let Some(trip) = tt.trip(&tt_trip.trip) else { continue };
                let n = trip.line.trim().to_string();
                if !n.is_empty() {
                    match numbers.iter_mut().find(|x| x.0 == n) {
                        Some(x) => x.1 += 1,
                        None => numbers.push((n, 1)),
                    }
                }
                let to = trip.terminus.trim().to_string();
                if !to.is_empty() && !termini.contains(&to) && termini.len() < 4 {
                    termini.push(to);
                }
            }
            numbers.sort_by(|a, b| b.1.cmp(&a.1));
            MapRow { ttl: l.name.clone(), number: numbers.first().map(|x| x.0.clone()).unwrap_or_else(|| l.name.clone()), termini, tours: l.tours.len() }
        })
        .collect();
    out.sort_by_key(|r| natural(&r.ttl));
    out
}

/// A name as the lists order it: its leading number first ("2" before "10"), then the rest.
fn natural(s: &str) -> (u64, String) {
    let digits: String = s.trim().chars().take_while(|c| c.is_ascii_digit()).collect();
    (digits.parse().unwrap_or(u64::MAX), s.trim().to_lowercase())
}

/// The map's lines the list shows: all of them - working for the bus company, those it runs -
/// each edited one by its own number and colour.
pub(super) fn shown(v: &LineEditorView) -> Vec<Shown> {
    v.map_rows
        .iter()
        .filter(|r| v.company.as_ref().is_none_or(|fc| fc.runs.iter().any(|n| n.trim().eq_ignore_ascii_case(r.ttl.trim()))))
        .map(|r| {
            let line = v.reg.map_line(&r.ttl);
            let edited = line.is_some_and(|l| Some(l.id) != v.fresh);
            let number = line.map(|l| l.number.trim().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| r.number.clone());
            // (its name where it says more than the number, else where it goes)
            let label = if r.ttl.trim().eq_ignore_ascii_case(number.trim()) || r.ttl.trim().eq_ignore_ascii_case(r.number.trim()) { r.termini.join(" – ") } else { r.ttl.clone() };
            let label = line.map(|l| l.name.trim().to_string()).filter(|n| edited && !n.is_empty() && !n.eq_ignore_ascii_case(&number)).unwrap_or(label);
            let colour = line.filter(|_| edited).map(|l| l.colour.clone()).unwrap_or_else(|| reg::MAP_COLOUR.to_string());
            Shown { ttl: r.ttl.clone(), number, label, colour, edited }
        })
        .collect()
}

/// The list's part of the map's lines (`r` from its top): a heading, then a row a line - its
/// plate, its name or where it goes, and "edited" on one the player changed. Returns the line
/// clicked.
pub(super) fn draw_rows(ui: &mut Ui, r: Rect, rows: &[Shown], selected: Option<&str>) -> Option<String> {
    let mut pick = None;
    let x = r.x + 6.0;
    let w = r.w - 12.0;
    ui.p().rect(Rect::new(x, r.y + 6.0, w, 1.0), HAIRLINE);
    ui.text_in(&omsi_ui::tr("Lines of the map").to_uppercase(), Rect::new(x + 2.0, r.y + 10.0, w - 40.0, 16.0), 10.5, Weight::Bold, TEXT_DIM, Align::Left);
    ui.text_in(&rows.len().to_string(), Rect::new(x + w - 40.0, r.y + 10.0, 38.0, 16.0), 10.5, Weight::Bold, TEXT_FAINT, Align::Right);
    let edited_word = omsi_ui::tr("edited").to_uppercase();
    for (i, row) in rows.iter().enumerate() {
        let rr = Rect::new(x, r.y + HEAD_H + i as f32 * 40.0, w, 36.0);
        if !ui.rect_visible(rr) {
            continue;
        }
        let on = selected.is_some_and(|s| s.eq_ignore_ascii_case(&row.ttl));
        if ui.row(&format!("le-map-line-{}", row.ttl), rr, on) {
            pick = Some(row.ttl.clone());
        }
        let pw = super::super::ownlines::plate(ui, Vec2::new(rr.x + 8.0, rr.y + 7.0), &row.number, &row.colour, 22.0);
        let badge_w = if row.edited { ui.width(&edited_word, 10.0, Weight::Bold) + 16.0 } else { 0.0 };
        let tx = rr.x + 8.0 + pw + 10.0;
        ui.text_in(&row.label, Rect::new(tx, rr.y, rr.right() - tx - badge_w - 6.0, rr.h), 12.5, Weight::Medium, if on { on_accent() } else { TEXT_SOFT }, Align::Left);
        if row.edited {
            ui.badge(Vec2::new(rr.right() - badge_w + 4.0, rr.y + 10.0), &edited_word, if on { on_accent() } else { accent_2() });
        }
    }
    pick
}

/// A line of the map's chosen in the list: its edit when the player has one, else the map's
/// line opened (once the map's stops are read). What to say, if anything.
pub(super) fn pick(v: &mut LineEditorView, ttl: &str) -> Option<(String, bool)> {
    if let Some(id) = v.reg.map_line(ttl).map(|l| l.id) {
        if v.sel != Some(id) {
            v.leave_fresh();
            (v.sel, v.dir, v.sel_stop, v.delete_armed) = (Some(id), 0, None, false);
            v.table_reset();
        }
        return None;
    }
    v.leave_fresh();
    if v.stops.is_empty() {
        v.opening = Some(ttl.to_string());
        return Some(("Reading the map's roads: the line opens in a moment".into(), false));
    }
    open(v, ttl)
}

/// The timetable line `ttl` as the map has it: its own file where the line editor wrote over it
/// (kept beside it, `<name>.ttl.orig`), else the timetable's.
fn original_line(tt: &TimetableData, ttl: &str) -> Option<Line> {
    let line = tt.lines.iter().find(|l| l.name.trim().eq_ignore_ascii_case(ttl.trim()))?;
    let orig = PathBuf::from(format!("{}.orig", line.path.display()));
    if orig.is_file() {
        if let Ok(f) = omsi_cfg::CfgFile::read(&orig) {
            return Some(Line { name: line.name.clone(), path: line.path.clone(), ..Line::parse(&f) });
        }
    }
    Some(line.clone())
}

/// The map's line `ttl` as the line editor opens it (`core::lines::import_map_line`): its
/// stops where the map's network has them, the depot file's own IBIS routes, the legs the map
/// has no way for routed over the roads. With how many legs were routed and how many of the
/// map's trips could not be taken.
fn import(v: &mut LineEditorView, ttl: &str) -> Option<(LineDesign, usize, usize)> {
    let tt = v.tt.clone()?;
    let line = original_line(&tt, ttl)?;
    let tiles = v.raw_tiles.clone();
    let stops = &v.stops;
    // (a stop by its id on the tile its `Busstops.cfg` entry names: ids repeat across tiles)
    let stop_of = |id: i64, name: &str| -> StopRef {
        let tile = tt.bus_stops.iter().find(|b| b.object_id == id).and_then(|b| usize::try_from(b.group).ok()).and_then(|k| tiles.get(k)).map(|t| [t.0, t.1]);
        let found = stops.iter().find(|s| s.id == id && Some(s.tile) == tile).or_else(|| stops.iter().find(|s| s.id == id));
        match found {
            Some(s) => StopRef { name: if name.trim().is_empty() { s.name.clone() } else { name.trim().to_string() }, ..s.clone() },
            None => StopRef { id, name: if name.trim().is_empty() { format!("Stop {id}") } else { name.trim().to_string() }, tile: tile.unwrap_or_default(), ..Default::default() },
        }
    };
    let mut l = reg::import_map_line(&tt, &line, &tiles, &stop_of)?;
    // the depot file's own IBIS route of each direction, kept while its stops are the map's
    if let Some(depot) = v.depot_of(&l.ai_group.clone()) {
        let number = l.number.clone();
        for d in &mut l.directions {
            let names: Vec<&str> = d.stops.iter().map(|s| s.name.as_str()).collect();
            d.map_route = depot.route_of(&d.number_or(&number), &names).unwrap_or(0);
        }
    }
    // the legs the map has no way for: the router's
    if let Some((_, router)) = v.router.as_ref() {
        for d in l.directions.iter_mut().filter(|d| d.legs.iter().any(|g| !g.ok)) {
            route_direction(router, d);
        }
    }
    let routed = l.directions.iter().flat_map(|d| &d.legs).filter(|g| g.map_way.is_none()).count();
    let dropped = line.tours.iter().map(|t| t.trips.len()).sum::<usize>().saturating_sub(l.table.len());
    Some((l, routed, dropped))
}

/// The map's line `ttl` opened as a line to edit: in the registry (saved only when changed and
/// saved), shown. What to say.
pub(super) fn open(v: &mut LineEditorView, ttl: &str) -> Option<(String, bool)> {
    let Some((l, routed, dropped)) = import(v, ttl) else {
        return Some((omsi_ui::tr("Line %{n} of the map runs no trip the timetable has: it cannot be opened").replace("%{n}", ttl), true));
    };
    let number = l.number.trim().to_string();
    let id = v.reg.add_map_line(l).id;
    if let Some(x) = v.reg.line(id) {
        v.running.insert(id, x.clone());
    }
    (v.sel, v.dir, v.sel_stop, v.delete_armed, v.restore_armed) = (Some(id), 0, None, false, false);
    v.fresh = Some(id);
    v.table_reset();
    v.shapes_for = None;
    v.revision += 1;
    let mut text = omsi_ui::tr("Line %{n} of the map is open: change it as one of your own. Saved, the game drives it instead of the map's original.").replace("%{n}", &number);
    if routed > 0 {
        text = format!("{text} {}", omsi_ui::tr("%{n} leg(s) the map has no way for are routed over the roads.").replace("%{n}", &routed.to_string()));
    }
    if dropped > 0 {
        text = format!("{text} {}", omsi_ui::tr("%{n} of its trips could not be taken: they have fewer than two stops the timetable knows.").replace("%{n}", &dropped.to_string()));
    }
    crate::mt::protect([text.as_str()]);
    Some((text, dropped > 0))
}

/// The direction to show of `line` (`current`): Outbound and Return as two halves of `r` - a
/// line of the map's with more trips than two in a list, each by its trip's ends. The one chosen.
pub(super) fn direction_choice(ui: &mut Ui, line: &LineDesign, id: &str, r: Rect, current: usize) -> Option<usize> {
    if line.directions.len() <= 2 {
        let mut d = current;
        return ui.segmented(id, r, &mut d, &["Outbound", "Return"]).then_some(d);
    }
    let labels: Vec<String> = line
        .directions
        .iter()
        .enumerate()
        .map(|(k, d)| {
            let what = match k {
                0 => omsi_ui::tr("Outbound").into_owned(),
                1 => omsi_ui::tr("Return").into_owned(),
                _ => omsi_ui::tr("Variant %{n}").replace("%{n}", &(k + 1).to_string()),
            };
            let from = d.stops.first().map(|s| s.name.trim().to_string()).unwrap_or_default();
            let shown = if d.number.trim().is_empty() { String::new() } else { format!("{} · ", d.number.trim()) };
            format!("{what}: {shown}{from} – {}", d.destination())
        })
        .collect();
    crate::mt::protect(labels.iter().map(String::as_str));
    let mut d = current.min(labels.len() - 1);
    ui.select(id, r, &mut d, &labels).then_some(d)
}

/// The foot of the left panel for a line of the map's: save it (working for the company,
/// paying the route's approval when its stops changed: `fee`, with the company's cash), drive
/// it, and put the map's original back (two presses).
#[allow(clippy::too_many_arguments)]
pub(super) fn foot(ui: &mut Ui, v: &mut LineEditorView, state: &mut super::super::state::State, body: Rect, foot: f32, problems: &[reg::Problem], fee: Option<(Cents, Cents)>, status: &mut Option<(String, bool)>) {
    let company = v.company.is_some();
    let why: Option<String> = if let Some(p) = problems.first() {
        Some(omsi_ui::tr(p.text).replace("%{n}", &p.n.to_string()))
    } else {
        fee.filter(|(f, cash)| f > cash).map(|(_, cash)| omsi_ui::tr("The company has %{cash}: not enough for the change.").replace("%{cash}", &super::super::company::eur(cash)))
    };
    if let Some(w) = &why {
        let h = ui.paragraph_height(w, body.w, 11.5, Weight::Regular);
        ui.paragraph(w, Vec2::new(body.x, foot - ROW - 14.0 - h), body.w, 11.5, Weight::Regular, WARN);
    }
    // (opened and not changed: nothing of it to save)
    let dirty = v.dirty && v.fresh != v.sel;
    let label = match fee {
        Some((f, _)) if f > 0 && dirty => omsi_ui::tr("Pay %{amount} and save").replace("%{amount}", &super::super::company::eur(f)),
        _ if dirty => omsi_ui::tr("Save line").into_owned(),
        _ => omsi_ui::tr("Saved").into_owned(),
    };
    let sw = if company { body.w } else { body.w - ROW - GAP };
    if ui.button("le-save", Rect::new(body.x, foot - ROW - 8.0, sw, ROW), &label, Some("save"), if dirty && why.is_none() { ButtonKind::Primary } else { ButtonKind::Normal }) && dirty {
        match (why.clone(), company) {
            (Some(w), _) => *status = Some((w, true)),
            (None, true) => v.company_act = Some(CompanyAct::SaveMap),
            (None, false) => *status = Some(save_map(v, state)),
        }
    }
    if !company && ui.icon_button("le-drive", Vec2::new(body.right() - ROW * 0.5, foot - 8.0 - ROW * 0.5), 15.0, "play_arrow", "Drive it") {
        *status = Some(drive_it(v, state));
    }
    // (opened and not changed: the map's original is what runs, nothing to restore)
    if v.fresh.is_some() && v.fresh == v.sel {
        return;
    }
    let restore_label = if v.restore_armed { "Press again: the map's original back" } else { "Restore the map's original" };
    let rr = Rect::new(body.x, foot, body.w, ROW);
    let pressed = ui.button("le-restore", rr, restore_label, Some("restart_alt"), ButtonKind::Normal);
    ui.tooltip(rr, "Your edit goes: the game runs the map's own line again, exactly as the map has it");
    if pressed {
        if !v.restore_armed {
            v.restore_armed = true;
            *status = Some(("Press again: the edit goes and the game runs the map's own line as before".into(), false));
        } else if company {
            v.company_act = Some(CompanyAct::RestoreMap);
        } else {
            *status = Some(restore(v, state));
        }
    }
}

/// A line of the map's saved (`save`), said as such.
fn save_map(v: &mut LineEditorView, state: &mut super::super::state::State) -> (String, bool) {
    let problems = v.line().map(reg::problems).unwrap_or_default();
    let number = v.line().map(|x| x.number.trim().to_string()).unwrap_or_default();
    let (text, err) = save(v, state, &problems);
    if err || !problems.is_empty() {
        return (text, err);
    }
    v.restore_armed = false;
    (omsi_ui::tr("Line %{n} saved: the game drives it instead of the map's original (the next game start picks it up)").replace("%{n}", &number), false)
}

/// The edit of the shown line of the map's dropped: the map's own line file back as it was, the
/// registry and the timetable written again. What to say.
fn restore(v: &mut LineEditorView, state: &mut super::super::state::State) -> (String, bool) {
    let Some(line) = v.line().cloned() else { return (String::new(), false) };
    let Some(ttl) = line.map.as_ref().map(|m| m.ttl.clone()) else { return (String::new(), false) };
    v.reg.restore_map_line(&ttl);
    v.running.remove(&line.id);
    v.fresh = None;
    v.sel = v.first_shown();
    (v.dir, v.sel_stop, v.delete_armed, v.restore_armed, v.dirty) = (0, None, false, false, false);
    v.table_reset();
    v.shapes_for = None;
    v.revision += 1;
    let prepared = v.prepare_depots();
    if let Err(e) = reg::save_registry(&v.reg_path, &v.reg) {
        return (format!("{}: {e}", omsi_ui::tr("Not restored")), true);
    }
    let Some(content) = core::content_dir() else { return ("Taken out of your lines, but there is no content folder to put the map's line back into".into(), true) };
    if let Err(e) = reg::export_to_map(&content, &v.map_dir, &v.reg) {
        return (format!("{}: {e}", omsi_ui::tr("The timetable files could not be written")), true);
    }
    saved(v);
    if let Some(e) = v.write_depots(&content, &prepared) {
        return (format!("{}: {e}", omsi_ui::tr("A depot file could not be written (displays and IBIS)")), true);
    }
    texts_written(&v.reg_path, &mut v.reg, true);
    omsi_cfg::content_changed();
    state.load_lines();
    (omsi_ui::tr("Line %{n} is the map's own again: the game runs its original as before").replace("%{n}", &ttl), false)
}

/// After the timetable was written: each line of the map's as the game runs it now, and the
/// map's lines read again.
pub(super) fn saved(v: &mut LineEditorView) {
    // (the one opened and not changed stays out of the timetable as it was: `save`)
    let fresh = v.fresh;
    v.running.retain(|id, _| Some(*id) == fresh);
    for l in v.reg.lines.iter().filter(|l| l.is_map_line()) {
        v.running.insert(l.id, l.clone());
    }
    omsi_cfg::content_changed();
    let tt = TimetableData::load(&v.map_dir);
    v.map_rows = map_rows(&tt);
    v.tt = Some(Arc::new(tt));
}

/// What saving the shown line of the map's costs the company that runs it (`ownline::
/// map_change_fee`, against the line as the game runs it now), with the company's cash. None:
/// not working for a company, or no line of the map's shown.
pub(super) fn company_fee(l: &Launcher) -> Option<(Cents, Cents)> {
    let v = &l.pages.lines;
    v.company.as_ref()?;
    let c = l.company.company.as_ref()?;
    let line = v.line().filter(|x| x.is_map_line())?;
    let old = v.running.get(&line.id).unwrap_or(line);
    Some((ownline::map_change_fee(c, old, line).unwrap_or(0), c.cash))
}

/// The company's line of the map's saved (its changed route paid for) or put back as the map
/// has it: the company first (`ownline::apply_map_change`: the fee, its line, the tours to plan
/// anew), then the registry and the timetable.
pub(super) fn company_act(l: &mut Launcher, restore_it: bool, status: &mut Option<(String, bool)>) {
    let v = &mut l.pages.lines;
    let Some(line) = v.line().cloned() else { return };
    let Some(ttl) = line.map.as_ref().map(|m| m.ttl.clone()) else { return };
    let old = v.running.get(&line.id).cloned().unwrap_or_else(|| line.clone());
    // (put back: the map's own line, as the change's other end)
    let new = if restore_it {
        match import(v, &ttl) {
            Some((orig, _, _)) => orig,
            None => line.clone(),
        }
    } else {
        line.clone()
    };
    let Some(paid) = super::super::company::act(l, |c| ownline::apply_map_change(c, &ttl, &old, &new, restore_it)) else { return };
    let Launcher { pages, state, .. } = l;
    let v = &mut pages.lines;
    let (text, err) = if restore_it { restore(v, state) } else { save_map(v, state) };
    super::super::company::reload_timetable(l);
    let number = line.number.trim().to_string();
    *status = Some(if !err && paid > 0 {
        (omsi_ui::tr("Line %{n} saved: its new route is approved for %{amount}.").replace("%{n}", &number).replace("%{amount}", &super::super::company::eur(paid)), false)
    } else {
        (text, err)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_timetable::{BusStopEntry, StnLink, StnLinkEntry, Tour, TourTrip, Trip, TripProfile};

    /// A map's `TTData` in `dir`: line 7 out and back over two links each, its short run 7E,
    /// three tours, and another line 12 - as OMSI's editor has them.
    fn timetable(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        let stops: String = [(1, "Bahnhof"), (2, "Kirche"), (3, "Markt"), (4, "Markt"), (5, "Kirche"), (6, "Bahnhof")].iter().map(|(id, n)| BusStopEntry { name: n.to_string(), group: 0, object_id: *id, params: [0.0; 3] }.to_text()).collect();
        std::fs::write(dir.join("Busstops.cfg"), stops).unwrap();
        let link = |a: i64, b: i64| StnLink { length: 400.0, from_id: a, to_id: b, params: [2.0, 2.0, 1.0, 1.0, 0.0, 0.0], entries: vec![StnLinkEntry { values: [a as f64 * 10.0, 0.0, 0.0, 400.0, -1.0, 0.0, 0.0] }] }.to_text("a", "b");
        std::fs::write(dir.join("StnLinks.cfg"), [link(1, 2), link(2, 3), link(4, 5), link(5, 6)].concat()).unwrap();
        let trip = |name: &str, to: &str, line: &str, stations: &[i64]| {
            let t = Trip { terminus: to.into(), line: line.into(), stations: stations.to_vec(), profiles: vec![TripProfile { name: "standard".into(), factor: 4.0, ..Default::default() }], ..Default::default() };
            std::fs::write(dir.join(format!("{name}.ttp")), t.to_text()).unwrap();
        };
        trip("7_out", "Markt", "7", &[1, 2, 3]);
        trip("7_back", "Bahnhof", "7", &[4, 5, 6]);
        trip("7_short", "Kirche", "7E", &[1, 2]);
        trip("12_a", "Markt", "12", &[1, 3]);
        let tt = |trip: &str, departure: f32| TourTrip { trip: trip.into(), profile: 0, departure };
        let line = |name: &str, tours: Vec<Tour>| std::fs::write(dir.join(format!("{name}.ttl")), Line { name: name.into(), user_allowed: true, priority: 1, tours, ..Default::default() }.to_text()).unwrap();
        line(
            "7",
            vec![
                Tour { number: "1".into(), ai_group: "Busses".into(), extra: "799".into(), trips: vec![tt("7_out", 360.0), tt("7_back", 370.0)] },
                Tour { number: "2".into(), ai_group: "Busses".into(), extra: "992".into(), trips: vec![tt("7_out", 480.0)] },
                Tour { number: "3".into(), ai_group: "Busses".into(), extra: "799".into(), trips: vec![tt("7_short", 400.0)] },
            ],
        );
        line("12", vec![Tour { number: "1".into(), ai_group: "Busses".into(), extra: "1023".into(), trips: vec![tt("12_a", 600.0)] }]);
    }

    /// The line editor on that map, its stops read (as the map's network gives them).
    fn view(base: &Path) -> LineEditorView {
        timetable(&base.join("TTData"));
        let tt = TimetableData::load(base);
        let stops = [(1, "Bahnhof"), (2, "Kirche"), (3, "Markt"), (4, "Markt"), (5, "Kirche"), (6, "Bahnhof")].iter().map(|(id, n)| StopRef { tile: [0, 0], id: *id, name: n.to_string(), at: [*id as f64 * 100.0, 0.0], ..Default::default() }).collect();
        LineEditorView { map_rows: map_rows(&tt), tt: Some(Arc::new(tt)), raw_tiles: vec![(0, 0)], stops, map_dir: base.to_path_buf(), ..Default::default() }
    }

    #[test]
    fn a_line_of_the_map_is_listed_apart_and_opened_as_a_line_to_edit() {
        let base = std::env::temp_dir().join(format!("openomsi-maplines-open-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let mut v = view(&base);
        // the map's lines, by their names and numbers, none of them edited
        assert_eq!(v.map_rows.iter().map(|r| (r.ttl.as_str(), r.number.as_str(), r.tours)).collect::<Vec<_>>(), [("7", "7", 3), ("12", "12", 1)]);
        let rows = shown(&v);
        assert_eq!(rows[0].label, "Markt – Bahnhof – Kirche");
        assert!(rows.iter().all(|r| !r.edited && r.colour == reg::MAP_COLOUR));
        // opened: its stops, its directions, its departures - not changed, so not the player's yet
        let said = pick(&mut v, "7").unwrap();
        assert!(!said.1, "{}", said.0);
        let l = v.line().unwrap();
        assert!(l.is_map_line() && l.directions.len() == 3 && l.table.len() == 4);
        assert_eq!(l.directions[0].stops.iter().map(|s| s.id).collect::<Vec<_>>(), [1, 2, 3]);
        assert_eq!(l.directions[0].stops[1].at, [200.0, 0.0], "where the map's network has it");
        assert!(v.fresh == v.sel && !v.dirty && !shown(&v)[0].edited);
        assert!(!v.shows(v.line().unwrap()), "not among the player's own lines");
        // left as it came: it goes again
        v.leave_fresh();
        assert!(v.reg.map_line("7").is_none() && v.sel.is_none());
        // changed: the player's edit, marked so
        pick(&mut v, "7");
        v.line_mut().unwrap().colour = "#e5484d".into();
        v.touched();
        assert!(v.fresh.is_none());
        let rows = shown(&v);
        assert!(rows[0].edited && rows[0].colour == "#e5484d" && !rows[1].edited);
        // chosen again from the list: the edit, not a new import
        let id = v.sel;
        v.leave_fresh();
        pick(&mut v, "12");
        assert!(pick(&mut v, "7").is_none() && v.sel == id && v.reg.lines.len() == 1, "line 12 left as it came");
        // before the map's stops are read: opened once they are
        v.stops.clear();
        assert!(pick(&mut v, "12").is_some() && v.opening.as_deref() == Some("12"));
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A line of the map's opened and not changed is no edit: a save of the registry leaves it
    /// out, and driving it drives the map's own line and trip as they run. Changed, it waits
    /// for its save; saved, the Drive page follows the files it is written as.
    #[test]
    fn a_line_of_the_map_opened_and_not_changed_is_neither_saved_nor_driven_as_an_edit() {
        let base = std::env::temp_dir().join(format!("openomsi-maplines-fresh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let mut v = view(&base);
        pick(&mut v, "7");
        assert_eq!(drive_target(&v), Some(("7".to_string(), "7_out".to_string())), "the map's own");
        // saved meanwhile (another line's save): not in the registry written
        let path = base.join("lines").join("map.json");
        without_fresh(&mut v, |v| reg::save_registry(&path, &v.reg)).unwrap();
        assert!(reg::load_registry(&path).map_line("7").is_none());
        assert!(v.reg.map_line("7").is_some() && v.fresh == v.sel, "still open");
        // changed: the player's edit, to be saved before it is driven
        v.line_mut().unwrap().name = "Ring".into();
        v.touched();
        assert_eq!(drive_target(&v), None);
        without_fresh(&mut v, |v| reg::save_registry(&path, &v.reg)).unwrap();
        assert_eq!(reg::load_registry(&path).map_line("7").map(|l| l.name.as_str()), Some("Ring"));
        v.dirty = false;
        assert_eq!(drive_target(&v), Some(("7".to_string(), "oo_map_7_a".to_string())), "the edit's files, under the map's line");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn working_for_the_company_the_list_has_the_map_lines_it_runs() {
        let base = std::env::temp_dir().join(format!("openomsi-maplines-company-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let mut v = view(&base);
        let fc = |runs: Vec<String>| ForCompany { id: "c".into(), name: "Stadtbus".into(), map: String::new(), colours: Default::default(), depot: String::new(), start: None, stop_lines: None, cache: None, details: false, runs, start_map: None };
        v.company = Some(fc(vec!["12".into()]));
        assert_eq!(shown(&v).iter().map(|r| r.ttl.as_str()).collect::<Vec<_>>(), ["12"]);
        v.company = Some(fc(Vec::new()));
        assert!(shown(&v).is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A line of the map's of more trips than two chooses its direction in a list; an own
    /// line keeps its two halves.
    #[test]
    fn more_directions_than_two_are_chosen_in_a_list() {
        use super::super::super::ui::id_of;
        let base = std::env::temp_dir().join(format!("openomsi-maplines-dirs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let mut v = view(&base);
        pick(&mut v, "7");
        let mut ui = Ui::new();
        ui.begin(Vec2::new(400.0, 300.0), 1.0, 1.0 / 60.0);
        assert_eq!(direction_choice(&mut ui, v.line().unwrap(), "le-dir", Rect::new(0.0, 0.0, 300.0, 40.0), 0), None);
        assert!(ui.drawn.contains_key(&id_of("le-dir")), "a list");
        let own = LineDesign { directions: vec![Direction::default(), Direction::default()], ..Default::default() };
        direction_choice(&mut ui, &own, "le-dir-own", Rect::new(0.0, 50.0, 300.0, 40.0), 0);
        assert!(!ui.drawn.contains_key(&id_of("le-dir-own")) && ui.drawn.contains_key(&(id_of("le-dir-own") ^ 11)), "two halves");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn the_map_lines_texts_are_translated() {
        let keys = [
            "Lines of the map",
            "edited",
            "%{n} m · as the map drives it",
            "Reading the map's roads: the line opens in a moment",
            "Line %{n} of the map runs no trip the timetable has: it cannot be opened",
            "Line %{n} of the map is open: change it as one of your own. Saved, the game drives it instead of the map's original.",
            "%{n} leg(s) the map has no way for are routed over the roads.",
            "%{n} of its trips could not be taken: they have fewer than two stops the timetable knows.",
            "Your edit goes: the game runs the map's own line again, exactly as the map has it",
            "Variant %{n}",
            "Restore the map's original",
            "Press again: the map's original back",
            "Press again: the edit goes and the game runs the map's own line as before",
            "Line %{n} saved: the game drives it instead of the map's original (the next game start picks it up)",
            "Not restored",
            "Taken out of your lines, but there is no content folder to put the map's line back into",
            "Line %{n} is the map's own again: the game runs its original as before",
            "Edit the line in the line editor: its route, timetable and displays",
            "The table holds the map's departures, each in the tour the map runs it in: change them there. A change to the patterns below makes that group of days anew from them.",
        ];
        for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
            for k in keys {
                assert!(crate::_rust_i18n_try_translate(lang, k).is_some(), "{lang}: {k}");
            }
        }
    }
}
