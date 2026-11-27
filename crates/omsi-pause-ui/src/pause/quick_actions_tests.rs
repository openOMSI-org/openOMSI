use super::*;

mod tests {
    use super::*;

    #[test]
    fn quick_actions_defaults_and_explicit_empty_are_distinct() {
        assert_eq!(
            selected("default"),
            ["tobus", "camera", "map", "duty", "save", "players"]
        );
        assert!(selected("[]").is_empty());
        assert_eq!(selected("broken"), selected("default"));
    }

    #[test]
    fn quick_actions_keep_unavailable_ids_but_remove_duplicates_and_session_controls() {
        assert_eq!(
            selected(r#"["save","future-action","save","resume","quit","plugin"]"#),
            ["save", "future-action"]
        );
    }
}

#[cfg(test)]
mod editing_tests {
    use super::*;
    fn menu() -> Vec<MenuLine> {
        ["resume", "camera", "save", "world", "quit"]
            .into_iter()
            .map(|id| (id, id.into(), None))
            .collect()
    }
    #[test]
    fn quick_actions_first_edit_keeps_other_defaults() {
        let text = change("default", &Edit::Toggle("camera".into()));
        assert_eq!(selected(&text), ["tobus", "map", "duty", "save", "players"]);
    }
    #[test]
    fn quick_actions_custom_order_resolves_only_original_available_indices() {
        assert_eq!(
            indices(Category::Home, &menu(), r#"["world","admin","save"]"#),
            [3, 2]
        );
        assert_eq!(indices(Category::Settings, &menu(), "[]"), [1]);
    }

    #[test]
    fn world_category_uses_one_combined_vehicle_and_world_settings_entry() {
        let menu = vec![
            ("resume", "Resume".into(), None),
            ("vehicle", "Vehicle options...".into(), None),
            ("world", "World options...".into(), None),
        ];
        assert_eq!(indices(Category::World, &menu, "default"), [2]);
        assert_eq!(
            indices(Category::Home, &menu, r#"["vehicle","world"]"#),
            [2]
        );
        assert_eq!(entries(&menu, r#"["vehicle","world"]"#), ["world"]);
    }
    #[test]
    fn quick_actions_reorder_does_not_wrap_and_reset_restores_defaults() {
        let text = r#"["save","camera"]"#;
        assert_eq!(
            selected(&change(text, &Edit::Move("save".into(), false))),
            ["save", "camera"]
        );
        assert_eq!(
            selected(&change(text, &Edit::Move("save".into(), true))),
            ["camera", "save"]
        );
        assert_eq!(
            selected(&change(text, &Edit::Move("camera".into(), true))),
            ["save", "camera"]
        );
        assert_eq!(change(text, &Edit::Reset), "default");
        assert_eq!(
            entries(&menu(), r#"["admin","save"]"#),
            ["admin", "save", "camera", "world"]
        );
    }
}
