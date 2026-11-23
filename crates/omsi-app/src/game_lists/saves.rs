use super::*;

pub(crate) fn load_rows(
    quicksave: Option<std::path::PathBuf>,
    slots: Vec<omsi_launcher_lib::SavedSituation>,
) -> Vec<(String, String)> {
    let out: Vec<(String, String)> = quicksave
        .map(|q| (omsi_ui::tr("Quicksave").into_owned(), q))
        .into_iter()
        .chain(slots.into_iter().map(|s| (s.name, s.file)))
        .map(|(name, f)| (name, format!("load {}", f.to_string_lossy())))
        .collect();
    if out.is_empty() {
        vec![(
            omsi_ui::tr("No saved situation of this map yet").into_owned(),
            "back".into(),
        )]
    } else {
        out
    }
}

pub(super) fn run_saves(app: &mut App, kind: &ListKind, verb: &str, arg: &str) -> Option<ListKind> {
    if matches!(kind, ListKind::LoadSaves) {
        if verb == "load" && !arg.is_empty() {
            app.close_game_menu();
            app.menus.load_request = Some(std::path::PathBuf::from(arg));
        }
        return None;
    }
    match verb {
        "named" => match app.menus.menu_edit.take() {
            Some(t) if !t.trim().is_empty() => {
                crate::platform::text_input(app.window.as_deref(), false);
                app.save_slot_to(None, Some(t));
            }
            _ => {
                app.menus.menu_edit = Some(String::new());
                crate::platform::text_input(app.window.as_deref(), true);
                return Some(ListKind::SaveSlots);
            }
        },
        "new" => app.save_slot_to(None, None),
        "over" => app.save_slot_to(Some(std::path::PathBuf::from(arg)), None),
        _ => return Some(ListKind::SaveSlots),
    }
    app.close_game_menu();
    None
}
