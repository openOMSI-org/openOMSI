use super::*;

const READING: &str = "The street names are still being read from the map";

pub(super) fn street_rows(names: Option<Vec<String>>) -> Vec<(String, String)> {
    let Some(mut names) = names else { return vec![(omsi_ui::tr(READING).into_owned(), "back".into())] };
    if names.is_empty() {
        return vec![(omsi_ui::tr("This map has no street names on its signs").into_owned(), "back".into())];
    }
    names.sort_by(|a, b| natural(a, b));
    names.into_iter().map(|n| (n.clone(), format!("street {n}"))).collect()
}

pub(super) fn show_street(app: &mut App, name: &str) -> Option<ListKind> {
    if app.menus.navigator.as_mut().is_some_and(|n| n.show_street(name)) {
        app.close_game_menu();
        return None;
    }
    Some(ListKind::Streets)
}

pub(crate) fn refresh_streets_when_read(app: &mut App) {
    if matches!(app.menus.list_kind, Some(ListKind::Streets))
        && app.menus.admin_list.as_ref().is_some_and(|l| l.iter().any(|(t, _)| *t == omsi_ui::tr(READING)))
        && app.menus.navigator.as_ref().is_some_and(|n| n.street_names().is_some())
    {
        app.refresh_list();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use winit::keyboard::KeyCode;

    fn app_with_streets() -> App {
        let mut app = crate::assemble_app(crate::cli::Args::parse_from(["openomsi"]), crate::settings::Settings::default());
        app.menus.navigator = Some(omsi_navigator::Navigator::new(true, 0.85, "bottom-left"));
        app.menus.game_menu = Some(0);
        app.open_list(ListKind::Streets);
        app
    }

    fn start_search(app: &mut App) {
        assert_eq!(run(app, &ListKind::Streets, "search"), Some(ListKind::Streets));
        app.open_list(ListKind::Streets);
        app.route_edit_text("haupt");
        assert!(crate::platform::TEXT_INPUT_ON.with(|t| t.get()));
    }

    fn typing_over(app: &App) -> bool {
        !crate::platform::TEXT_INPUT_ON.with(|t| t.get()) && app.menus.menu_edit.is_none() && !app.menus.menu_edit_search
    }

    #[test]
    fn streets_are_listed_by_name_and_picked_by_name() {
        let rows = street_rows(Some(vec!["Zeppelinstr.".into(), "Am Hafen".into(), "Breite Str.".into()]));
        assert_eq!(rows, [("Am Hafen", "street Am Hafen"), ("Breite Str.", "street Breite Str."), ("Zeppelinstr.", "street Zeppelinstr.")].map(|(a, b)| (a.to_string(), b.to_string())));
        assert_eq!(rows.iter().filter(|(l, _)| search_matches(l, "", "str")).count(), 2);
    }

    #[test]
    fn a_map_still_being_read_is_not_a_map_without_street_names() {
        assert_ne!(street_rows(None), street_rows(Some(Vec::new())));
        assert!(street_rows(Some(Vec::new())).iter().all(|(_, a)| a == "back"));
    }

    #[test]
    fn a_street_that_cannot_be_shown_keeps_its_list_open() {
        let mut app = app_with_streets();
        assert_eq!(run(&mut app, &ListKind::Streets, "street Am Hafen"), Some(ListKind::Streets));
        assert!(app.menus.game_menu.is_some());
        assert!(!app.menus.navigator.as_ref().is_some_and(|n| n.map_open()));
    }

    #[test]
    fn the_keyboard_goes_away_when_a_search_is_set_or_dropped() {
        let mut app = app_with_streets();
        start_search(&mut app);
        app.chooser_key(KeyCode::Enter);
        assert!(typing_over(&app));
        assert_eq!(app.menus.menu_search, "haupt");
        start_search(&mut app);
        app.chooser_key(KeyCode::Escape);
        assert!(typing_over(&app));
        start_search(&mut app);
        app.route_edit_text("\n");
        assert!(typing_over(&app));
        start_search(&mut app);
        app.open_list(ListKind::Numbers);
        assert!(typing_over(&app));
    }

    #[test]
    fn closing_the_menu_while_a_search_is_typed_gives_the_keys_back_to_the_game() {
        let mut app = app_with_streets();
        start_search(&mut app);
        app.close_game_menu();
        assert!(typing_over(&app));
        assert!(app.menus.menu_search.is_empty());
    }
}
