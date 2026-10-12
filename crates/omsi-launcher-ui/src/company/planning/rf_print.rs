//! RosterFlow's prints: the bus graph (a row per vehicle: its trips as bars in the line's
//! colour, the pull-outs and pull-ins black, its duties as a band under it) and the duty graph
//! (a row per duty: its signing, trips, travel and custom work, its vehicles under it), laid
//! out as Optibus's "Bus Graph" (`co::rosterpdf`) and saved where the player chooses.

use super::super::super::Launcher;
use super::rf_gantt::{self as gantt, block_label};
use super::rosterflow::{Cached, DAY_NAMES};
use omsi_launcher_lib::company::network::TourOfDay;
use omsi_launcher_lib::company::rosterflow::{self as rf, Board, EvKind};
use omsi_launcher_lib::company::rosterpdf::{self as pdf, GraphItem, GraphRow, ItemKind};
use omsi_launcher_lib::company::{self as co, Company};

/// A block's trips as a graph's items.
fn block_items(c: &Company, board: &Board, t: &TourOfDay, range: std::ops::Range<usize>) -> Vec<GraphItem> {
    let colour = rf::line_colour(c, &t.line);
    let n = t.trips.len();
    let mut out = Vec::new();
    for j in range {
        let x = &t.trips[j];
        let kind = if x.empty && (j == 0 || j + 1 == n) {
            ItemKind::Pull
        } else if x.empty || !x.counts() {
            ItemKind::Deadhead
        } else {
            ItemKind::Trip { inbound: !gantt::outbound(&board.tours, &t.line, x) }
        };
        let label = if x.line.trim().is_empty() { t.number.trim().to_string() } else { x.line.trim().to_string() };
        out.push(GraphItem { from: x.dep, to: x.arr, kind, label, colour, start_stop: pdf::stop_letters(&x.from), end_stop: pdf::stop_letters(x.end()) });
    }
    out
}

/// The bus graph's rows: a vehicle each, its duties under it.
pub(super) fn bus_rows(c: &Company, board: &Board) -> Vec<GraphRow> {
    let line_at = |l: &str| c.lines.iter().position(|x| x.name.eq_ignore_ascii_case(l)).unwrap_or(usize::MAX);
    let mut idx: Vec<usize> = (0..board.tours.len()).collect();
    idx.sort_by_key(|&i| (line_at(&board.tours[i].line), board.tours[i].from(), i));
    idx.into_iter()
        .map(|i| {
            let t = &board.tours[i];
            let under = co::network::duties_of(t)
                .iter()
                .enumerate()
                .map(|(k, r)| {
                    let (a, b) = rf::span_of(&c.planning.flow.prefs, t, r);
                    let label = co::studio::duty_of(&board.duties, i, k).and_then(|di| board.shaped.iter().position(|s| s.duty == Some(di))).map(|row| board.shaped[row].label(row)).unwrap_or_default();
                    (a, b, label)
                })
                .collect();
            GraphRow { name: block_label(t), sub: omsi_ui::tr("Line %{n}").replace("%{n}", &t.number), sub2: format!("({:.1} km)", t.km()), items: block_items(c, board, t, 0..t.trips.len()), under }
        })
        .collect()
}

/// The duty graph's rows: a duty each, its vehicles under it.
pub(super) fn duty_rows(c: &Company, board: &Board) -> Vec<GraphRow> {
    let p = &c.planning.flow.prefs;
    board
        .shaped
        .iter()
        .enumerate()
        .map(|(row, s)| {
            let mut items: Vec<GraphItem> = Vec::new();
            for e in &s.events {
                let (kind, label) = match e.kind {
                    EvKind::SignOn | EvKind::SignOff => (ItemKind::Sign, String::new()),
                    EvKind::Travel(m) => (ItemKind::Travel, m.letter().to_string()),
                    EvKind::Custom(k) => (ItemKind::Custom, s.custom.get(k).map(|x| omsi_ui::tr(&x.kind).into_owned()).unwrap_or_default()),
                };
                items.push(GraphItem { from: e.from, to: e.to, kind, label, colour: [255, 255, 255], start_stop: String::new(), end_stop: String::new() });
            }
            let mut under = Vec::new();
            for sp in &s.spans {
                let t = &board.tours[sp.tour];
                let r = co::network::duties_of(t).get(sp.duty).cloned().unwrap_or(0..0);
                items.extend(block_items(c, board, t, r));
                under.push((sp.from, sp.to, block_label(t)));
            }
            items.sort_by_key(|x| x.from);
            GraphRow { name: s.label(row), sub: omsi_ui::tr(s.kind_name(p)).into_owned(), sub2: format!("({} {})", omsi_ui::tr("paid"), rf::hm(s.paid)), items, under }
        })
        .collect()
}

/// Print the bus graph (`bus`) or the duty graph of the day shown into a PDF file the player
/// chooses where to keep; it is shown in the file browser.
pub(super) fn print(l: &mut Launcher, k: &Cached, board: &Board, date: &str, bus: bool) {
    let c: &Company = &k.view;
    let rows = if bus { bus_rows(c, board) } else { duty_rows(c, board) };
    let title = omsi_ui::tr(if bus { "Bus Graph" } else { "Duty Graph" }).into_owned();
    let info = vec![
        omsi_ui::tr("Depot: %{d}").replace("%{d}", c.depot.trim_end_matches(".hof")),
        omsi_ui::tr("Service: %{s}").replace("%{s}", &omsi_ui::tr(DAY_NAMES[board.weekday as usize % 7])),
        omsi_ui::tr("Date: %{d}").replace("%{d}", date),
    ];
    let printed = format!("{}: {}  ·  {}", omsi_ui::tr("Printed"), c.date, c.name);
    let bytes = pdf::graph(&title, &info, &printed, &omsi_ui::tr("Page"), &rows);
    let name = format!("{}-{}.pdf", if bus { "bus-graph" } else { "duty-graph" }, date);
    // (a folder given in OMSI_ROSTERFLOW_PDF takes the file without asking: the screenshot
    // runs check the print so)
    if let Ok(dir) = std::env::var("OMSI_ROSTERFLOW_PDF") {
        let path = std::path::Path::new(&dir).join(&name);
        if std::fs::write(&path, &bytes).is_ok() {
            l.state.set_status(omsi_ui::tr("Saved: %{file}").replace("%{file}", &path.display().to_string()), false);
        }
        return;
    }
    #[cfg(not(target_os = "android"))]
    {
        let Some(path) = rfd::FileDialog::new().set_title(omsi_ui::tr("Save the graph as a PDF file").as_ref()).set_file_name(&name).add_filter("PDF", &["pdf"]).save_file() else { return };
        match std::fs::write(&path, &bytes) {
            Ok(()) => {
                crate::platform::reveal(&path);
                l.state.set_status(omsi_ui::tr("Saved: %{file}").replace("%{file}", &path.display().to_string()), false);
            }
            Err(e) => l.state.set_status(format!("{}: {e}", omsi_ui::tr("Could not save the file")), true),
        }
    }
    #[cfg(target_os = "android")]
    {
        let _ = (name, bytes);
        l.state.set_status(omsi_ui::tr("Printing is for the computer.").into_owned(), true);
    }
}
