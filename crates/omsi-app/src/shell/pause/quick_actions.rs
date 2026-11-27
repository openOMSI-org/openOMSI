//! Stable built-in action IDs; execution still resolves through the permission-filtered menu.

use super::navigation::Category;
use crate::app_impl::MenuLine;

pub(crate) const DEFAULT: [&str; 6] = ["tobus", "camera", "map", "duty", "save", "players"];

pub(crate) fn selected(text: &str) -> Vec<String> {
    let values = serde_json::from_str::<Vec<String>>(text)
        .unwrap_or_else(|_| DEFAULT.iter().map(|s| (*s).to_string()).collect());
    let mut out = Vec::new();
    for id in values {
        if !id.is_empty()
            && id.len() <= 128
            && !id.chars().any(char::is_control)
            && !matches!(id.as_str(), "resume" | "quit" | "plugin")
            && !out.contains(&id)
        {
            out.push(id);
        }
    }
    out
}

pub(crate) fn indices(category: Category, menu: &[MenuLine], text: &str) -> Vec<usize> {
    if category != Category::Home {
        let ids: Vec<_> = menu.iter().map(|line| line.0).collect();
        let mut indices = category.indices(&ids);
        if category == Category::World && indices.iter().any(|&i| menu[i].0 == "world") {
            indices.retain(|&i| menu[i].0 != "vehicle");
        }
        return indices;
    }
    let mut indices: Vec<_> = selected(text)
        .iter()
        .filter_map(|id| {
            menu.iter()
                .position(|line| line.0 == id && line.2.is_none())
        })
        .collect();
    if indices.iter().any(|&i| menu[i].0 == "world") {
        indices.retain(|&i| menu[i].0 != "vehicle");
    }
    indices
}

/// Selected IDs remain editable even when the current session cannot offer them.
pub(crate) fn entries(menu: &[MenuLine], text: &str) -> Vec<String> {
    let mut out = selected(text);
    for line in menu {
        if line.2.is_none()
            && !matches!(line.0, "resume" | "quit" | "plugin")
            && !out.iter().any(|id| id == line.0)
        {
            out.push(line.0.to_string());
        }
    }
    if out.iter().any(|id| id == "world") {
        out.retain(|id| id != "vehicle");
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
    if matches!(edit, Edit::Reset) {
        return "default".to_string();
    }
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
                if next < values.len() {
                    values.swap(at, next);
                }
            }
        }
    }
    serde_json::to_string(&values).unwrap()
}

#[cfg(test)]
#[path = "quick_actions_tests.rs"]
mod tests;
