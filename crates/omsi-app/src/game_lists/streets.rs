use super::*;

pub(super) fn street_rows(names: Option<Vec<String>>) -> Vec<(String, String)> {
    let Some(mut names) = names else { return vec![(omsi_ui::tr("The street names are still being read from the map").into_owned(), "back".into())] };
    if names.is_empty() {
        return vec![(omsi_ui::tr("This map has no street names on its signs").into_owned(), "back".into())];
    }
    names.sort_by(|a, b| natural(a, b));
    names.into_iter().map(|n| (n.clone(), format!("street {n}"))).collect()
}

pub(super) fn show_street(app: &mut App, name: &str) {
    if app.menus.navigator.as_mut().is_some_and(|n| n.show_street(name)) {
        app.close_game_menu();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
