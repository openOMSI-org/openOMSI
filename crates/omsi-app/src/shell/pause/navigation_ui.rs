//! Grouped pause navigation, using only the launcher's existing colour tokens.

use super::navigation::Category;
use super::{PauseView, plain, rail_icon};
use crate::shell::{Action, Shell};
use glam::Vec2;
use omsi_ui::kit::{ButtonKind, Ui, id_of};
use omsi_ui::paint::Align;
use omsi_ui::theme::*;
use omsi_ui::{Rect, Weight};

fn ids<'a>(v: &'a PauseView<'a>) -> Vec<&'a str> {
    v.rail.iter().map(|line| line.0).collect()
}

fn activate(sh: &mut Shell, v: &PauseView, index: usize) {
    if !v.disabled.contains(&v.rail[index].0) {
        sh.actions.push(if v.list.is_some() { Action::Rail(index) } else { Action::Choose(index) });
    }
}

fn session_action(sh: &mut Shell, v: &PauseView, id: &str, rect: Rect, kind: ButtonKind) {
    let Some(k) = v.rail.iter().position(|line| line.0 == id) else { return };
    let label = omsi_ui::tr(plain(&v.rail[k].1).0);
    let ui = &mut sh.ui;
    let (hovered, _, clicked) = ui.interact(id_of(&format!("pause-session-{id}")), rect);
    let t = ui.anim(id_of(&format!("pause-session-hover-{id}")), if hovered { 1.0 } else { 0.0 }, 0.08);
    let primary = kind == ButtonKind::Primary;
    let fill = if primary { ACCENT.mix(ACCENT_HOVER, t) } else { SURFACE_2.mix(DANGER_2.alpha(0.15), t) };
    let ink = if primary { ACCENT_INK } else { DANGER_2 };
    ui.p().rounded(rect, 12.0, fill);
    let width = ui.width(&label, 18.0, Weight::Bold);
    let x = rect.center().x - (width + 32.0) * 0.5;
    ui.icon(rail_icon(id), Vec2::new(x + 12.0, rect.center().y), 24.0, ink);
    ui.text_in(&label, Rect::new(x + 32.0, rect.y, width + 2.0, rect.h), 18.0, Weight::Bold, ink, Align::Left);
    if v.kbd && !sh.quick_edit && v.list.is_none() && v.rail_sel == Some(k) {
        ui.p().rounded_border(rect.inset(-2.0), 14.0, 2.0, ACCENT);
    }
    if clicked {
        activate(sh, v, k);
    }
}

pub(super) fn rail(sh: &mut Shell, v: &PauseView) {
    let size = sh.ui.size;
    let r = Rect::new(0.0, 0.0, RAIL_W, size.y);
    let ui = &mut sh.ui;
    ui.solid(r);
    ui.p().rect(r, SIDEBAR);
    if let Some(tex) = sh.logo_tex {
        ui.image(Rect::new(18.0, 18.0, 32.0, 32.0), tex, 7.0);
    }
    ui.text("openOMSI", Vec2::new(60.0, 32.0), 18.0, Weight::Bold, INK, Align::Left);
    ui.text_in(crate::startup::VERSION, Rect::new(60.0, 38.0, RAIL_W - 72.0, 18.0), 11.0, Weight::Regular, INK_MUTED, Align::Left);
    session_action(sh, v, "resume", Rect::new(16.0, 78.0, RAIL_W - 32.0, 54.0), ButtonKind::Primary);
    sh.ui.text_in(&omsi_ui::tr("Pause menu"), Rect::new(26.0, 144.0, RAIL_W - 42.0, 20.0), 11.0, Weight::Medium, INK_MUTED, Align::Left);
    let show_driver = size.y >= 440.0;
    let foot = if show_driver { 138.0 } else { 76.0 };
    let area = Rect::new(12.0, 176.0, RAIL_W - 24.0, (size.y - 176.0 - foot).max(0.0));
    let categories = Category::available(&ids(v));
    let mut picked = None;
    let selected = sh.pause_category;
    let hidden = sh.pause_hidden;
    sh.ui.scroll_area("pause-categories", area, &mut |ui, view| {
        for (i, category) in categories.iter().copied().enumerate() {
            let rect = Rect::new(view.x, view.y + i as f32 * 60.0, view.w, 52.0);
            if category_button(ui, category, selected == category && !hidden, rect) {
                picked = Some(category);
            }
        }
        categories.len() as f32 * 60.0
    });
    if v.kbd {
        if let Some(row) = categories.iter().position(|category| *category == selected) {
            sh.ui.scroll_to("pause-categories", row as f32 * 60.0, 52.0, area.h);
        }
    }
    if let Some(category) = picked {
        sh.actions.push(Action::PauseCategory(category));
    }
    session_action(sh, v, "quit", Rect::new(16.0, size.y - foot + 6.0, RAIL_W - 32.0, 54.0), ButtonKind::Danger);
    if show_driver {
        driver(sh, v, size.y);
    }
}

fn driver(sh: &mut Shell, v: &PauseView, height: f32) {
    let ui = &mut sh.ui;
    let chip = Rect::new(16.0, height - 66.0, RAIL_W - 32.0, 52.0);
    ui.p().rounded(chip, 8.0, SURFACE);
    let av = Vec2::new(chip.x + 24.0, chip.center().y);
    ui.p().circle(av, 16.0, SURFACE_3);
    let initial = v.summary.driver.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into());
    ui.text_in(&initial, Rect::new(av.x - 16.0, av.y - 16.0, 32.0, 32.0), 13.0, Weight::Bold, INK_2, Align::Center);
    ui.text_in(&v.summary.driver, Rect::new(chip.x + 50.0, chip.y + 7.0, chip.w - 58.0, 19.0), 13.0, Weight::Bold, INK, Align::Left);
    ui.text_in(&v.summary.bus, Rect::new(chip.x + 50.0, chip.y + 27.0, chip.w - 58.0, 17.0), 11.0, Weight::Regular, INK_MUTED, Align::Left);
}

fn category_button(ui: &mut Ui, category: Category, selected: bool, rect: Rect) -> bool {
    let (hovered, _, clicked) = ui.interact(id_of(&format!("pause-category-{category:?}")), rect);
    let t = ui.anim(id_of(&format!("pause-category-hover-{category:?}")), if hovered { 1.0 } else { 0.0 }, 0.08);
    ui.p().rounded(rect, 10.0, if selected { ACCENT.alpha(0.10) } else { SURFACE_2.alpha(t) });
    let color = if selected { ACCENT } else { INK_2.mix(INK, t) };
    ui.icon(category.icon(), Vec2::new(rect.x + 23.0, rect.center().y), 22.0, color);
    ui.text_in(&omsi_ui::tr(category.title()), Rect::new(rect.x + 48.0, rect.y, (rect.w - 71.0).max(0.0), rect.h), 17.0, Weight::Medium, color, Align::Left);
    ui.icon("chevron_right", Vec2::new(rect.right() - 15.0, rect.center().y), 15.0, color.alpha(0.6));
    clicked
}

pub(super) fn page(sh: &mut Shell, v: &PauseView, page: Rect, compact: bool) {
    let ids = ids(v);
    if !Category::available(&ids).contains(&sh.pause_category) {
        sh.pause_category = Category::Home;
    }
    let category = sh.pause_category;
    if category != Category::Home { sh.quick_edit = false; }
    let pad = if page.w < 420.0 { 18.0 } else { 32.0 };
    let width = (page.w - pad * 2.0).clamp(0.0, 1400.0);
    let x = page.x + pad;
    let tight = compact && page.h < 420.0;
    if !tight {
        let header = Rect::new(x, 28.0, width, 22.0);
        sh.ui.text_in(&omsi_ui::tr("Pause menu"), header, 11.0, Weight::Medium, INK_MUTED, Align::Left);
        sh.ui.text_in(&omsi_ui::tr(if v.paused { "Paused" } else { "Menu" }), header, 11.0, Weight::Medium, ACCENT, Align::Right);
    }
    let short = compact && page.h < 640.0;
    let mut top = if compact { 86.0 } else { 52.0 };
    if compact {
        session_action(sh, v, "resume", Rect::new(x, if tight { 16.0 } else { 66.0 }, width, 46.0), ButtonKind::Primary);
        top = if tight {
            70.0
        } else if short {
            118.0
        } else {
            126.0
        };
    }
    let editing = sh.quick_edit;
    let title_w = if category == Category::Home { (width - 98.0).max(0.0) } else { width };
    sh.ui.text_in(&omsi_ui::tr(category.title()), Rect::new(x, top, title_w, 46.0), if category == Category::Home && width < 420.0 { 26.0 } else { 34.0 }, Weight::Bold, INK, Align::Left);
    if !short {
        sh.ui.text_in(&omsi_ui::tr(if sh.quick_edit { "Choose actions and set their order" } else { category.hint() }), Rect::new(x, top + 54.0, width, 28.0), 16.0, Weight::Regular, INK_2, Align::Left);
    }
    if category == Category::Home {
        let edit_rect = Rect::new(x + width - 90.0, top + 4.0, 90.0, 38.0);
        if v.keys { sh.ui.tooltip(edit_rect, &format!("{} ({})", omsi_ui::tr(if editing { "Done" } else { "Edit" }), if editing { "Esc" } else { "E" })); }
        if sh.ui.button("pause-quick-edit", edit_rect, &omsi_ui::tr(if editing { "Done" } else { "Edit" }), None, ButtonKind::Ghost) {
            sh.quick_edit = !editing;
            sh.quick_focus = 0;
            sh.last_sel = None;
        }
    }
    top += if tight {
        44.0
    } else if short {
        46.0
    } else {
        98.0
    };
    let footer_h = if compact { 94.0 } else { 48.0 };
    if compact && !sh.quick_edit {
        let nav_h = (page.h - top - footer_h - 106.0).clamp(34.0, 82.0);
        top = compact_categories(sh, &ids, Rect::new(x, top, width, nav_h), v.kbd);
    }
    let area = Rect::new(x, top, width, (page.h - top - footer_h).max(0.0));
    if sh.quick_edit { super::quick_actions_ui::editor(sh, v, area); } else { cards(sh, v, category, area); }
    if compact {
        session_action(sh, v, "quit", Rect::new(x, page.h - 84.0, width, 54.0), ButtonKind::Danger);
    }
    if v.keys {
        let text = omsi_ui::tr(if sh.quick_edit { "Up / Down: select | Space: toggle | Left / Right: reorder | R: reset | Esc: done" } else { "Esc resumes  |  Left / Right: categories  |  Up / Down: actions" });
        let footer = Rect::new(x, page.h - 27.0, width, 18.0);
        let shown = if sh.quick_edit && width < 750.0 { "↑ ↓ / Space / ← → / R / Esc" } else { &text };
        sh.ui.text_in(shown, footer, 12.0, Weight::Regular, INK_MUTED, Align::Left);
        if sh.quick_edit { sh.ui.tooltip(footer, &text); }
    }
}

fn compact_categories(sh: &mut Shell, ids: &[&str], area: Rect, keyboard: bool) -> f32 {
    let mut x = 0.0;
    let mut y = 0.0;
    let mut buttons = Vec::new();
    for category in Category::available(ids) {
        let label = omsi_ui::tr(category.title());
        let w = (sh.ui.width(&label, 12.0, Weight::Medium) + 24.0).min(area.w);
        if x + w > area.w && x > 0.0 {
            x = 0.0;
            y += 40.0;
        }
        buttons.push((category, label, Rect::new(x, y, w, 34.0)));
        x += w + 4.0;
    }
    let height = (y + 34.0).min(area.h);
    let viewport = Rect::new(area.x, area.y, area.w, height);
    let selected = sh.pause_category;
    let mut picked = None;
    sh.ui.scroll_area("pause-compact-categories", viewport, &mut |ui, view| {
        for (category, label, rect) in &buttons {
            let r = Rect::new(view.x + rect.x, view.y + rect.y, rect.w, rect.h);
            if ui.button(&format!("pause-compact-{category:?}"), r, label, None, ButtonKind::Ghost) {
                picked = Some(*category);
            }
            if selected == *category {
                ui.p().rounded(Rect::new(r.x + 8.0, r.bottom() - 2.0, (r.w - 16.0).max(0.0), 2.0), 1.0, ACCENT);
            }
        }
        y + 34.0
    });
    if keyboard {
        if let Some((_, _, rect)) = buttons.iter().find(|(category, _, _)| *category == selected) {
            sh.ui.scroll_to("pause-compact-categories", rect.y, rect.h, height);
        }
    }
    if let Some(category) = picked {
        sh.actions.push(Action::PauseCategory(category));
    }
    area.y + height + 10.0
}

fn cards(sh: &mut Shell, v: &PauseView, category: Category, area: Rect) {
    let indices = super::quick_actions::indices(category, v.rail, v.quick_actions);
    let cols = if area.w >= 520.0 { 2 } else { 1 };
    let gap = 14.0;
    let w = ((area.w - gap * (cols - 1) as f32) / cols as f32).max(0.0);
    let key = format!("pause-cards-{category:?}");
    let mut picked = None;
    let mut focus = None;
    sh.ui.scroll_area(&key, area, &mut |ui, view| {
        if indices.is_empty() {
            ui.text_in(
                &omsi_ui::tr("No actions available in this category."),
                Rect::new(view.x, view.y, view.w, 40.0),
                13.0,
                Weight::Regular,
                INK_MUTED,
                Align::Left,
            );
        }
        for (position, &index) in indices.iter().enumerate() {
            let y = (position / cols) as f32 * 136.0;
            let rect = Rect::new(view.x + (position % cols) as f32 * (w + gap), view.y + y, w, 120.0);
            let keyboard = v.kbd && v.rail_sel == Some(index);
            if keyboard {
                focus = Some((index, y));
            }
            if action_card(ui, v, index, rect, keyboard, category == Category::Home) {
                picked = Some(index);
            }
        }
        indices.len().div_ceil(cols) as f32 * 136.0
    });
    if let Some((index, y)) = focus {
        let next = (id_of(&key), index);
        if sh.last_sel != Some(next) {
            sh.ui.scroll_to(&key, y, 120.0, area.h);
            sh.last_sel = Some(next);
        }
    }
    if let Some(index) = picked {
        activate(sh, v, index);
    }
}

fn action_card(ui: &mut Ui, v: &PauseView, index: usize, rect: Rect, keyboard: bool, shortcut: bool) -> bool {
    let (id, label, plugin) = &v.rail[index];
    let disabled = v.disabled.contains(id);
    let (hovered, _, clicked) = ui.interact(id_of(&format!("pause-card-{index}-{id}")), rect);
    let t = ui.anim(id_of(&format!("pause-card-hover-{index}-{id}")), if hovered && !disabled { 1.0 } else { 0.0 }, 0.08);
    let fill = if keyboard { SURFACE.mix(ACCENT, 0.12) } else { SURFACE.mix(SURFACE_2, t) };
    ui.p().rounded(rect, 18.0, fill);
    if keyboard {
        ui.p().rounded_border(rect, 18.0, 2.0, ACCENT);
    }
    let danger = *id == "endduty";
    let ink = if disabled {
        INK_MUTED
    } else if danger {
        DANGER_2
    } else {
        INK
    };
    let icon_c = if shortcut { ACCENT } else { INK_2 };
    let tile = Rect::new(rect.x + 18.0, rect.y + 37.0, 46.0, 46.0);
    ui.p().rounded(tile, 14.0, if shortcut { ACCENT.alpha(0.10) } else { SURFACE_2 });
    let icon = plugin
        .as_ref()
        .and_then(|p| p.icon.as_deref())
        .and_then(|i| omsi_ui::icons::names().find(|n| *n == i.trim()))
        .unwrap_or_else(|| if *id == "plugin" { "extension" } else { rail_icon(id) });
    ui.icon(icon, tile.center(), 27.0, icon_c);
    let label = if *id == "plugin" { label.clone() } else { omsi_ui::tr(label) };
    let text = plain(&label).0;
    let content = Rect::new(rect.x + 80.0, rect.y + 26.0, (rect.w - 112.0).max(0.0), 29.0);
    ui.text_in(text, content, 18.0, Weight::Bold, ink, Align::Left);
    let hint = omsi_ui::tr(action_hint(id));
    ui.text_in(&hint, Rect::new(content.x, rect.y + 65.0, content.w, 25.0), 14.5, Weight::Regular, INK_MUTED, Align::Left);
    ui.icon("chevron_right", Vec2::new(rect.right() - 19.0, rect.center().y), 17.0, INK_MUTED);
    clicked && !disabled
}

fn action_hint(id: &str) -> &'static str {
    match id {
        "camera" => "Seat position, field of view and head movement",
        "map" => "View the city and your position",
        "duty" => "Choose a line and a tour",
        "save" | "saveslot" | "load" => "Manage your saved situations",
        "options" | "controls" => "Adjust your game preferences",
        "vehicle" | "world" => "Adjust the current simulation",
        "plugin" => "Open this add-on's tools",
        "skipstop" | "endduty" => "Manage the current tour",
        "tobus" => "Return to your own vehicle",
        "photo" | "shot" => "Capture the current scene",
        "players" | "copycode" | "admin" => "Manage the multiplayer session",
        _ => "Open this tool",
    }
}

#[cfg(test)]
#[path = "navigation_ui_tests.rs"]
mod tests;
