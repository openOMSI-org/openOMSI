//! The destination form (the quick menu's destination tile, OMSI's `Tform_setline`): the
//! route number and two free lines of text for the displays, "Select" to set them and
//! "Clear display" to leave the displays empty. A first line that names one of the depot
//! file's destinations (and no second line) picks that destination, its roller blind
//! included; any other text becomes a destination of its own (`Hof::with_free_text`), which
//! the matrix displays show through their scripts.

use super::{button, card, grow_row, input, note, text, title};
use omsi_plugin::api::Value;
use omsi_plugin::ui::{Anchor, Panel, Weight};

/// What is typed into the form.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct DestForm {
    pub route: String,
    pub lines: [String; 2],
    pub(super) said: Option<String>,
}

impl DestForm {
    pub(crate) fn panel(&self, light: bool) -> Panel {
        let help = self.said.as_deref().map(str::to_owned).unwrap_or_else(|| omsi_ui::tr("A destination of the depot file is found by its name; any other text is shown as it is typed.").into_owned());
        let children = vec![
            title(&omsi_ui::tr("Destination display")),
            text(&omsi_ui::tr("Route number"), 13.0, Weight::Medium),
            input("route", &self.route, "137", 12),
            text(&omsi_ui::tr("Destination, two lines"), 13.0, Weight::Medium),
            input("line1", &self.lines[0], &omsi_ui::tr("First line"), 60),
            input("line2", &self.lines[1], &omsi_ui::tr("Second line (may stay empty)"), 60),
            // (the depot file's destinations to pick from, as OMSI's form lists them, #2219)
            grow_row(vec![button("list", &omsi_ui::tr("Choose from the depot file"), Some("view_list"))]),
            grow_row(vec![button("clear", &omsi_ui::tr("Clear display"), Some("delete")), button("select", &omsi_ui::tr("Select"), Some("check")), button("close", &omsi_ui::tr("Close"), Some("close"))]),
            note(&help, light),
        ];
        card(Anchor::Center, 0.0, 0.0, 420.0, 236, light, children)
    }
}

/// The form opened, filled with what the bus shows.
pub(crate) fn open(app: &mut crate::App) {
    let Some(p) = app.player.as_ref() else {
        app.service_msg = Some(("No vehicle to set a destination for".into(), 3.0));
        return;
    };
    let route = crate::game_lists::destination_line(&p.vehicle);
    let shown = p.vehicle.host.hof.as_deref().and_then(|hof| {
        let ti = crate::schedule::shown_destination(&p.vehicle, hof, p.blind_pick.as_ref())?;
        hof.termini.get(ti).map(|t| t.menu_name())
    });
    let mut form = DestForm { route, ..Default::default() };
    if let Some(name) = shown.filter(|n| n != omsi_vehicle::hof::FREE_TEXT) {
        form.lines[0] = name;
    }
    app.menus.game_panels.dest = Some(form);
}

/// A field typed into.
pub(crate) fn change(app: &mut crate::App, id: &str, value: &Value) {
    let (Some(f), Value::Str(s)) = (app.menus.game_panels.dest.as_mut(), value) else { return };
    match id {
        "route" => f.route = s.clone(),
        "line1" => f.lines[0] = s.clone(),
        "line2" => f.lines[1] = s.clone(),
        _ => {}
    }
}

/// The destination of the depot file that `lines` name, if they name one: the first line
/// as a destination's name (or its first text), with no second line.
pub(crate) fn named(hof: &omsi_vehicle::Hof, lines: &[String; 2]) -> Option<usize> {
    let want = lines[0].trim().to_lowercase();
    if want.is_empty() || !lines[1].trim().is_empty() {
        return None;
    }
    hof.termini.iter().position(|t| t.texture_id != omsi_vehicle::hof::FREE_TEXT && (t.menu_name().trim().to_lowercase() == want || t.display_name().trim().to_lowercase() == want))
}

pub(crate) fn click(app: &mut crate::App, id: &str) {
    match id {
        "close" => app.menus.game_panels.dest = None,
        "list" => {
            app.menus.game_panels.dest = None;
            app.open_list(crate::game_lists::ListKind::Destinations);
            if app.menus.chooser.is_some() && app.menus.game_menu.is_none() {
                app.open_game_menu();
            }
        }
        "select" | "clear" => {
            let Some(form) = app.menus.game_panels.dest.clone() else { return };
            let clear = id == "clear";
            let said = set(app, &form, clear);
            if let Some(f) = app.menus.game_panels.dest.as_mut() {
                f.said = Some(said);
                if clear {
                    *f = DestForm { said: f.said.take(), ..Default::default() };
                }
            }
        }
        // (Enter in a field is a click on it: as "Select")
        "route" | "line1" | "line2" => click(app, "select"),
        _ => {}
    }
}

/// Set the bus's displays to `form` (or empty them); what was done, in words.
fn set(app: &mut crate::App, form: &DestForm, clear: bool) -> String {
    if !clear && !form.route.trim().is_empty() {
        crate::game_lists::set_route_by_hand(app, &form.route);
    }
    let Some(p) = app.player.as_mut() else { return String::new() };
    if clear {
        // (the route number off the matrix and the rollers as well)
        for name in ["SetLineTo", "Matrix_Nr", "Linie"] {
            if let Some(i) = p.vehicle.ty.program.str_var(name) {
                p.vehicle.state.str_vars[i as usize] = String::new();
            }
        }
    }
    let Some(hof) = p.vehicle.host.hof.clone() else {
        return omsi_ui::tr("This bus has no depot file (.hof) with destinations").into_owned();
    };
    let line = crate::game_lists::destination_line(&p.vehicle);
    let lines = if clear { [String::new(), String::new()] } else { form.lines.clone() };
    if !clear && lines.iter().all(|l| l.trim().is_empty()) {
        return omsi_ui::tr("Route number set").into_owned();
    }
    match named(&hof, &lines) {
        Some(ti) => {
            p.set_destination_by_hand(&hof, &line, ti);
            format!("{} {}", omsi_ui::tr("Destination:"), hof.termini[ti].menu_name())
        }
        None => {
            let (free, ti) = hof.with_free_text([&lines[0], &lines[1]]);
            let free = std::sync::Arc::new(free);
            p.vehicle.host.hof = Some(free.clone());
            p.set_destination_by_hand(&free, &line, ti);
            log::info!("destination display set to free text: {:?} / {:?}", lines[0], lines[1]);
            if clear { omsi_ui::tr("Display cleared").into_owned() } else { omsi_ui::tr("Destination set as typed").into_owned() }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_vehicle::hof::{Hof, Terminus};

    #[test]
    fn a_destination_is_found_by_its_name_alone() {
        let hof = Hof { termini: vec![Terminus { code: 12, texture_id: "Rathaus Spandau".into(), strings: vec!["Rathaus".into()], ..Default::default() }], ..Default::default() };
        assert_eq!(named(&hof, &["rathaus spandau".into(), String::new()]), Some(0));
        assert_eq!(named(&hof, &["Rathaus".into(), " ".into()]), Some(0));
        assert_eq!(named(&hof, &["Rathaus".into(), "via Altstadt".into()]), None, "two lines: free text");
        assert_eq!(named(&hof, &["Sonderfahrt".into(), String::new()]), None);
        assert_eq!(named(&hof, &[String::new(), String::new()]), None);
    }

    #[test]
    fn traditional_chinese_localizes_navigator_distances_and_destination_form() {
        crate::with_test_language("ZHT", || {
            assert_eq!(omsi_ui::units::Units::Metric.distance(160.0), "160 米");
            assert_eq!(omsi_ui::tr("<1 min"), "<1 分鐘");
            assert_eq!(omsi_ui::tr("Nothing to couple: back up to a trailer's coupling (within 2.5 m, in line)"), "沒有可連接的物件：請倒車至拖車掛鉤處（2.5 米以內，成一直線）");

            let panel = format!("{:?}", DestForm::default().panel(false));
            for text in ["目的地顯示", "路線編號", "目的地(兩行)", "第一行", "第二行(可留空)", "從車廠檔案選擇", "清除顯示", "選擇", "關閉", "車庫檔中的目的地依名稱尋找;其他文字依輸入內容顯示。"] {
                assert!(panel.contains(text), "destination form is missing translated text: {text}");
            }
        });
    }
}
