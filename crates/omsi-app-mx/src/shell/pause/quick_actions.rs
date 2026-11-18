//! Stable built-in action IDs; execution still resolves through the permission-filtered menu.

use crate::app_impl::MenuLine;
use super::navigation::Category;

pub(crate) const DEFAULT: [&str; 6] = ["tobus", "camera", "map", "duty", "save", "players"];

pub(crate) fn selected(text: &str) -> Vec<String> {
    let values = serde_json::from_str::<Vec<String>>(text)
        .unwrap_or_else(|_| DEFAULT.iter().map(|s| (*s).to_string()).collect());
    let mut out = Vec::new();
    for id in values {
        if !id.is_empty() && id.len() <= 128 && !id.chars().any(char::is_control)
            && !matches!(id.as_str(), "resume" | "quit" | "plugin") && !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

pub(crate) fn indices(category: Category, menu: &[MenuLine], text: &str) -> Vec<usize> {
    if category != Category::Home {
        return category.indices(&menu.iter().map(|line| line.0).collect::<Vec<_>>());
    }
    selected(text).iter().filter_map(|id| menu.iter().position(|line| line.0 == id && line.2.is_none())).collect()
}

/// Selected IDs remain editable even when the current session cannot offer them.
pub(crate) fn entries(menu: &[MenuLine], text: &str) -> Vec<String> {
    let mut out = selected(text);
    for line in menu {
        if line.2.is_none() && !matches!(line.0, "resume" | "quit" | "plugin") && !out.iter().any(|id| id == line.0) {
            out.push(line.0.to_string());
        }
    }
    out
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Edit {
    Toggle(String),
    Move(String, bool),
    Reset,
}

pub(crate) fn change(text: &str, edit: &Edit) -> String {
    let mut values = selected(text);
    if matches!(edit, Edit::Reset) { return "default".to_string(); }
    match edit {
        Edit::Reset => unreachable!(),
        Edit::Toggle(id) => {
            if let Some(at) = values.iter().position(|v| v == id) {
                values.remove(at);
            } else {
                values.push(id.clone());
            }
        }
        Edit::Move(id, down) => {
            if let Some(at) = values.iter().position(|v| v == id) {
                let next = if *down { at + 1 } else { at.saturating_sub(1) };
                if next < values.len() { values.swap(at, next); }
            }
        }
    }
    serde_json::to_string(&values).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quick_actions_defaults_and_explicit_empty_are_distinct() {
        assert_eq!(selected("default"), ["tobus", "camera", "map", "duty", "save", "players"]);
        assert!(selected("[]").is_empty());
        assert_eq!(selected("broken"), selected("default"));
    }

    #[test]
    fn quick_actions_keep_unavailable_ids_but_remove_duplicates_and_session_controls() {
        assert_eq!(selected(r#"["save","future-action","save","resume","quit","plugin"]"#), ["save", "future-action"]);
    }
}

#[cfg(test)]
mod editing_tests {
    use super::*;
    fn menu() -> Vec<MenuLine> {
        ["resume", "camera", "save", "world", "quit"].into_iter().map(|id| (id, id.into(), None)).collect()
    }
    #[test]
    fn quick_actions_first_edit_keeps_other_defaults() {
        let text = change("default", &Edit::Toggle("camera".into()));
        assert_eq!(selected(&text), ["tobus", "map", "duty", "save", "players"]);
    }
    #[test]
    fn quick_actions_custom_order_resolves_only_original_available_indices() {
        assert_eq!(indices(Category::Home, &menu(), r#"["world","admin","save"]"#), [3, 2]);
        assert_eq!(indices(Category::Settings, &menu(), "[]"), [1]);
    }
    #[test]
    fn quick_actions_reorder_does_not_wrap_and_reset_restores_defaults() {
        let text = r#"["save","camera"]"#;
        assert_eq!(selected(&change(text, &Edit::Move("save".into(), false))), ["save", "camera"]);
        assert_eq!(selected(&change(text, &Edit::Move("save".into(), true))), ["camera", "save"]);
        assert_eq!(selected(&change(text, &Edit::Move("camera".into(), true))), ["save", "camera"]);
        assert_eq!(change(text, &Edit::Reset), "default");
        assert_eq!(entries(&menu(), r#"["admin","save"]"#), ["admin", "save", "camera", "world"]);
    }
}
