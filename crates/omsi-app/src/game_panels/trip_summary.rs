//! The trip's summary when it ends: a table of its stops - planned, actual, how far off,
//! the odometer there - with the stops served and the kilometres driven, to copy or to keep
//! in the Journeys folder beside the journey's log. Settings → Gameplay → "Trip summary"
//! turns it off; the quick menu's statistics tile shows the last one again.

use super::{button, card, divider, el, grow_row, note, row, text, title};
use crate::journey::{hms, TripSummary};
use omsi_plugin::ui::{Anchor, Element, Kind, Panel, Rgba, RowAlign, Weight};
use omsi_ui::tr;

const ACCENT: Rgba = Rgba([232, 160, 48, 255]);
const DIM: Rgba = Rgba([142, 142, 142, 255]);
const UNLIT: Rgba = Rgba([62, 62, 62, 255]);
const OK: Rgba = Rgba([104, 190, 118, 255]);
const WARN: Rgba = Rgba([232, 170, 70, 255]);
const DANGER: Rgba = Rgba([222, 78, 68, 255]);

fn icon(name: &str, size: f32, color: Rgba) -> Element {
    Element { color: Some(color), ..el(None, Kind::Icon { name: name.to_string(), size }) }
}

fn label(t: &str) -> Element {
    Element { color: Some(DIM), ..text(&tr(t), 12.0, Weight::Medium) }
}

fn grade(percent: Option<f64>) -> Rgba {
    let Some(percent) = percent else { return DIM };
    if percent >= 80.0 {
        OK
    } else if percent >= 50.0 {
        WARN
    } else {
        DANGER
    }
}

fn columns(children: Vec<Element>) -> Element {
    let children = children.into_iter().map(|c| Element { grow: true, ..c }).collect();
    el(None, Kind::Row { children, gap: 24.0, align: RowAlign::Start })
}

fn head(lines: &str) -> String {
    let line = |l: &str| match l.strip_prefix("Driver ") {
        Some(rest) => format!("{} {rest}", tr("Driver")),
        None if l.starts_with("Line ") => tr(l).into_owned(),
        None => l.to_string(),
    };
    lines.lines().map(line).collect::<Vec<_>>().join("\n")
}

fn stars(lit: u8) -> Element {
    let children = (0..5).map(|k| icon("star", 18.0, if k < lit { ACCENT } else { UNLIT })).collect();
    el(None, Kind::Row { children, gap: 2.0, align: RowAlign::End })
}

pub(crate) struct TripPanel {
    summary: TripSummary,
    /// What the last button did (copied, saved where).
    said: Option<String>,
}

impl TripPanel {
    pub(crate) fn new(summary: TripSummary) -> TripPanel {
        TripPanel { summary, said: None }
    }

    pub(crate) fn panel(&self) -> Panel {
        let s = &self.summary;
        let mut children = vec![
            row(vec![icon("directions_bus", 22.0, ACCENT), Element { grow: true, ..title(&tr("Trip summary")) }, stars(s.stars())], RowAlign::Start),
            text(&tr(&s.title), 14.0, Weight::Medium),
            note(&head(&s.head)),
        ];
        if let Some(mood) = s.mood() {
            children.push(row(vec![icon("forum", 18.0, ACCENT), text(&tr(mood), 14.0, Weight::Medium)], RowAlign::Start));
        }
        children.push(divider());
        let mut tiles = vec![("location_on", format!("{} / {}", s.served.0, s.served.1), "stops served"), ("schedule", format!("{} / {}", s.on_time, s.served.0), "on time")];
        if let Some(km) = s.km {
            tiles.push(("route", format!("{km:.1} km"), "distance"));
        }
        if let Some(d) = s.driven {
            tiles.push(("timer", hms(d), "time driven"));
        }
        if let Some(r) = s.rating {
            tiles.push(("groups", r.passengers.to_string(), "passengers"));
        }
        children.push(grow_row(tiles.iter().map(|(i, v, _)| row(vec![icon(i, 18.0, ACCENT), text(v, 17.0, Weight::Bold)], RowAlign::Start)).collect()));
        children.push(grow_row(tiles.iter().map(|(_, _, l)| label(l)).collect()));
        if let Some(r) = s.rating {
            children.push(divider());
            let gauges = [("speed", Some(r.driving), "driving"), ("airport_shuttle", r.comfort_rated(), "comfort"), ("receipt_long", r.tickets_rated(), "ticket selling")];
            children.push(columns(
                gauges
                    .iter()
                    .map(|(i, p, l)| row(vec![icon(i, 16.0, ACCENT), Element { grow: true, ..label(l) }, Element { color: Some(grade(*p)), ..text(&p.map(|p| format!("{p:.0} %")).unwrap_or_else(|| "-".into()), 13.0, Weight::Bold) }], RowAlign::Start))
                    .collect(),
            ));
            children.push(columns(gauges.iter().map(|(_, p, _)| Element { color: Some(grade(*p)), ..el(None, Kind::Bar { value: p.map_or(0.0, |p| (p / 100.0).clamp(0.0, 1.0) as f32), height: 6.0, background: None, segments: Vec::new() }) }).collect()));
        }
        children.push(divider());
        let rows: Vec<Vec<String>> = s.rows.iter().map(|r| r.to_vec()).collect();
        children.push(el(
            None,
            Kind::Table {
                columns: ["Bus stop", "Plan", "Actual", "Diff", "Odometer"].map(|c| tr(c).into_owned()).to_vec(),
                rows,
                widths: vec![0.36, 0.15, 0.15, 0.16, 0.18],
                size: 13.0,
                aligns: Vec::new(),
                inks: Vec::new(),
            },
        ));
        children.push(grow_row(vec![button("copy", &tr("Copy"), Some("content_copy")), button("save", &tr("Save"), Some("save")), button("close", &tr("Close"), Some("close"))]));
        if let Some(said) = &self.said {
            children.push(note(said));
        }
        children.push(row(vec![note(&tr("Settings → Gameplay → Trip summary turns it off."))], RowAlign::Start));
        card(Anchor::Center, 0.0, 0.0, 620.0, 236, children)
    }

    /// About how much room the card takes at scale 1 (width, height), so that a small screen
    /// (a phone) can draw it smaller instead of past its edges.
    pub(crate) fn extent(&self) -> (f32, f32) {
        let table = (self.summary.rows.len() + 1) as f32 * 13.0 * crate::plugin_ui::LINE + 3.0;
        (650.0, table + 380.0)
    }
}

/// The text on the system's clipboard; false when there is none to be had.
pub(crate) fn to_clipboard(text: &str) -> bool {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        thread_local! {
            // (kept alive: on X11 the text is gone when the clipboard is dropped)
            static CLIPBOARD: std::cell::RefCell<Option<arboard::Clipboard>> = const { std::cell::RefCell::new(None) };
        }
        CLIPBOARD.with(|c| {
            let mut c = c.borrow_mut();
            if c.is_none() {
                *c = arboard::Clipboard::new().ok();
            }
            c.as_mut().is_some_and(|cb| cb.set_text(text.to_string()).is_ok())
        })
    }
    #[cfg(target_os = "ios")]
    {
        crate::apple::set_clipboard(text)
    }
    #[cfg(target_os = "android")]
    {
        let _ = text;
        false
    }
}

pub(crate) fn click(app: &mut crate::App, id: &str) {
    let Some(t) = app.menus.game_panels.trip.as_mut() else { return };
    match id {
        "copy" => {
            t.said = Some(if to_clipboard(&t.summary.text()) { omsi_ui::tr("Copied to the clipboard").into_owned() } else { omsi_ui::tr("The clipboard cannot be reached").into_owned() });
        }
        "save" => {
            t.said = Some(match t.summary.save() {
                Ok(p) => format!("{} {}", omsi_ui::tr("Saved:"), p.display()),
                Err(e) => format!("{} {e}", omsi_ui::tr("Not saved:")),
            });
        }
        "close" => app.menus.game_panels.trip = None,
        _ => {}
    }
}
