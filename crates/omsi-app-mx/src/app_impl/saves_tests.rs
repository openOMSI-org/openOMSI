use super::{NAME_MAX, newer_quicksave, slot_target};
use crate::game_lists::ListKind;
use omsi_launcher_lib::SavedSituation;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use winit::keyboard::KeyCode;

fn app() -> crate::App {
    crate::assemble_app(
        <crate::cli::Args as clap::Parser>::parse_from(["openomsi"]),
        crate::settings::Settings::default(),
    )
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("openomsi-saves-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn touch(file: &PathBuf) {
    std::fs::write(file, "[name]\n").unwrap();
}

fn slot(file: PathBuf, name: &str, chosen: bool) -> SavedSituation {
    SavedSituation {
        file,
        name: name.into(),
        saved: 0,
        slot: true,
        chosen,
    }
}

fn first_action(app: &crate::App) -> String {
    app.menus.admin_list.as_ref().unwrap()[0].1.clone()
}

fn keyboard_on() -> bool {
    crate::platform::TEXT_INPUT_ON.with(|t| t.get())
}

#[test]
fn a_typed_name_is_edited_and_escape_cancels_it() {
    let mut app = app();
    app.open_list(ListKind::SaveSlots);
    assert_eq!(first_action(&app), "named");

    app.chooser_pick(0);
    assert_eq!(app.menus.list_kind, Some(ListKind::SaveSlots));
    assert_eq!(app.menus.menu_edit.as_deref(), Some(""));
    assert!(keyboard_on());

    app.route_edit_text("Night run!");
    app.chooser_key(KeyCode::Backspace);
    assert_eq!(app.menus.menu_edit.as_deref(), Some("Night run"));
    assert!(
        app.menus.admin_list.as_ref().unwrap()[0]
            .0
            .contains("Night run_")
    );

    app.route_edit_text(&"x".repeat(2 * NAME_MAX));
    assert_eq!(
        app.menus.menu_edit.as_ref().unwrap().chars().count(),
        NAME_MAX
    );

    app.chooser_key(KeyCode::Escape);
    assert_eq!(app.menus.menu_edit, None);
    assert!(!keyboard_on());
    assert_eq!(app.menus.list_kind, Some(ListKind::SaveSlots));
    assert_eq!(first_action(&app), "named");
    assert!(app.service_msg.is_none());
}

#[test]
fn enter_ignores_a_blank_name_and_closes_the_menu_on_a_typed_one() {
    let mut app = app();
    app.menus.game_menu = Some(0);
    app.open_list(ListKind::SaveSlots);
    app.chooser_pick(0);
    app.route_edit_text("   ");
    app.chooser_key(KeyCode::Enter);
    assert_eq!(app.menus.menu_edit, None);
    assert_eq!(app.menus.list_kind, Some(ListKind::SaveSlots));
    assert_eq!(app.menus.game_menu, Some(0));

    app.chooser_pick(0);
    app.route_edit_text("Night run");
    app.chooser_key(KeyCode::NumpadEnter);
    assert_eq!(app.menus.menu_edit, None);
    assert!(!keyboard_on());
    assert_eq!(app.menus.game_menu, None);
}

#[test]
fn a_new_slot_takes_the_first_number_free_in_both_folders() {
    let dir = temp("new");
    let fallback = temp("new-fallback");
    touch(&dir.join("Slot 1.osn"));
    touch(&fallback.join("Slot 2.osn"));

    let (n, out, named) = slot_target(&dir, Some(fallback.as_path()), None, None).unwrap();
    assert_eq!(
        (n.as_str(), out, named),
        ("3", dir.join("Slot 3.osn"), None)
    );

    let (_, _, named) = slot_target(
        &dir,
        Some(fallback.as_path()),
        None,
        Some("  Night\u{7} run \n".into()),
    )
    .unwrap();
    assert_eq!(named.as_deref(), Some("Night run"));

    let (_, _, named) = slot_target(&dir, None, None, Some("y".repeat(2 * NAME_MAX))).unwrap();
    assert_eq!(named.unwrap().chars().count(), NAME_MAX);

    let (n, _, named) = slot_target(&dir, None, None, Some("   ".into())).unwrap();
    assert_eq!((n.as_str(), named), ("2", None));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&fallback);
}

#[test]
fn saving_over_a_slot_keeps_its_file_number_and_chosen_name() {
    let dir = temp("over");
    let third = dir.join("Slot 3.osn");
    touch(&third);

    let kept = slot_target(
        &dir,
        None,
        Some(slot(third.clone(), "Depot run", true)),
        None,
    );
    assert_eq!(
        kept,
        Some(("3".into(), third.clone(), Some("Depot run".into())))
    );

    let auto = slot_target(
        &dir,
        None,
        Some(slot(third.clone(), "Slot 3: bus, 08:00", false)),
        None,
    );
    assert_eq!(auto, Some(("3".into(), third.clone(), None)));

    let renamed = slot_target(
        &dir,
        None,
        Some(slot(third.clone(), "Depot run", true)),
        Some("Evening".into()),
    );
    assert_eq!(renamed, Some(("3".into(), third, Some("Evening".into()))));

    let evening = dir.join("Evening.osn");
    let (n, out, _) = slot_target(
        &dir,
        None,
        Some(slot(evening.clone(), "Evening", true)),
        None,
    )
    .unwrap();
    assert_eq!((n.as_str(), out), ("Evening", evening));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_newer_quicksave_of_the_two_folders_is_loaded() {
    let dir = temp("quick");
    let main = dir.join("main.osn");
    let fallback = dir.join("fallback.osn");
    assert_eq!(newer_quicksave(main.clone(), Some(fallback.clone())), None);

    touch(&main);
    assert_eq!(
        newer_quicksave(main.clone(), Some(fallback.clone())),
        Some(main.clone())
    );
    assert_eq!(newer_quicksave(main.clone(), None), Some(main.clone()));

    touch(&fallback);
    let later = SystemTime::now() + Duration::from_secs(60);
    std::fs::File::options()
        .write(true)
        .open(&fallback)
        .unwrap()
        .set_modified(later)
        .unwrap();
    assert_eq!(
        newer_quicksave(main.clone(), Some(fallback.clone())),
        Some(fallback.clone())
    );

    std::fs::remove_file(&main).unwrap();
    assert_eq!(
        newer_quicksave(main, Some(fallback.clone())),
        Some(fallback)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_row_of_load_a_situation_asks_for_its_file() {
    let quick = PathBuf::from("Situations/quicksave.osn");
    let depot = PathBuf::from("maps/Grundorf/Saves/Slot 1.osn");
    let rows = crate::game_lists::load_rows(
        Some(quick.clone()),
        vec![slot(depot.clone(), "Depot run", true)],
    );
    let actions: Vec<&str> = rows.iter().map(|r| r.1.as_str()).collect();
    assert_eq!(
        actions,
        [
            format!("load {}", quick.to_string_lossy()),
            format!("load {}", depot.to_string_lossy())
        ]
    );
    assert_eq!(rows[1].0, "Depot run");

    let mut app = app();
    app.menus.game_menu = Some(0);
    app.menus.list_kind = Some(ListKind::LoadSaves);
    app.menus.admin_list = Some(rows);
    app.menus.chooser = Some(1);
    app.chooser_pick(1);
    assert_eq!(app.menus.load_request, Some(depot));
    assert_eq!(app.menus.game_menu, None);

    let empty = crate::game_lists::load_rows(None, Vec::new());
    assert_eq!(empty.len(), 1);
    assert_eq!(empty[0].1, "back");
    let mut app = self::app();
    app.menus.list_kind = Some(ListKind::LoadSaves);
    app.menus.admin_list = Some(empty);
    app.chooser_pick(0);
    assert_eq!(app.menus.load_request, None);
}

#[test]
fn the_plugin_load_command_loads_the_quicksave_or_says_there_is_none() {
    let quick = PathBuf::from("Situations/quicksave.osn");
    let mut app = app();
    app.integrations.plugin_command = true;
    app.menus.game_menu = Some(0);
    app.choose_load(Some(quick.clone()));
    assert_eq!(app.menus.load_request, Some(quick));
    assert_eq!(app.menus.game_menu, None);
    assert!(app.service_msg.is_none());

    let mut app = self::app();
    app.integrations.plugin_command = true;
    app.choose_load(None);
    assert_eq!(app.menus.load_request, None);
    assert!(
        app.service_msg
            .as_ref()
            .is_some_and(|m| m.0.starts_with("No quicksave yet"))
    );

    let mut app = self::app();
    app.choose_load(None);
    assert_eq!(app.menus.list_kind, Some(ListKind::LoadSaves));
    assert_eq!(app.menus.load_request, None);
}
