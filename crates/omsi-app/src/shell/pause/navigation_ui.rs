//! Grouped pause navigation, using only the launcher's existing colour tokens.

use super::navigation::Category;
use super::{PauseView, plain, rail_icon};
use crate::shell::{Action, Shell};
use glam::Vec2;
use omsi_ui::kit::{ButtonKind, Ui, id_of};
use omsi_ui::paint::Align;
use omsi_ui::theme::*;
use omsi_ui::{Rect, Weight};

pub(super) fn frame_rect(size: Vec2) -> Rect {
    let w = (size.x - 24.0).max(0.0).min(1200.0);
    let h = (size.y - 24.0).max(0.0).min(820.0);
    Rect::new((size.x - w) * 0.5, (size.y - h) * 0.5, w, h)
}

fn ids<'a>(v: &'a PauseView<'a>) -> Vec<&'a str> {
    v.rail.iter().map(|line| line.0).collect()
}

fn activate(sh: &mut Shell, v: &PauseView, index: usize) {
    if !v.disabled.contains(&v.rail[index].0) {
        sh.actions.push(if v.list.is_some() {
            Action::Rail(index)
        } else {
            Action::Choose(index)
        });
    }
}

fn session_action(sh: &mut Shell, v: &PauseView, id: &str, rect: Rect, kind: ButtonKind) {
    let Some(k) = v.rail.iter().position(|line| line.0 == id) else {
        return;
    };
    let label = omsi_ui::tr(plain(&v.rail[k].1).0);
    let ui = &mut sh.ui;
    let (hovered, _, clicked) = ui.interact(id_of(&format!("pause-session-{id}")), rect);
    let t = ui.anim(
        id_of(&format!("pause-session-hover-{id}")),
        if hovered { 1.0 } else { 0.0 },
        0.08,
    );
    let primary = kind == ButtonKind::Primary;
    let fill = if primary {
        th().accent.mix(th().accent_hover, t)
    } else {
        th().danger_2.alpha(0.12 * t)
    };
    let ink = if primary {
        th().accent_ink
    } else {
        th().danger_2
    };
    // (a button is solid in every look, glass or not: the glass is the panels')
    if primary || t > 0.01 {
        ui.p().rounded(rect, corner(12.0), fill);
    }
    if primary {
        ui.p()
            .rounded_border(rect, corner(12.0), super::EDGE_WIDTH, super::visible_edge());
    }
    if !primary && t > 0.01 && th().glass.is_some() {
        // (on glass a hairline of the red sets it off, as the add-ons' own danger buttons)
        ui.p().rounded_border(
            rect,
            corner(12.0),
            1.0,
            th().danger_2.alpha(0.35 + 0.25 * t),
        );
    }
    let width = ui.width(&label, 18.0, Weight::Bold);
    let x = rect.center().x - (width + 32.0) * 0.5;
    ui.icon(
        rail_icon(id),
        Vec2::new(x + 12.0, rect.center().y),
        24.0,
        ink,
    );
    ui.text_in(
        &label,
        Rect::new(x + 32.0, rect.y, width + 2.0, rect.h),
        18.0,
        Weight::Bold,
        ink,
        Align::Left,
    );
    if v.kbd && !sh.quick_edit && v.list.is_none() && v.rail_sel == Some(k) {
        ui.p()
            .rounded_border(rect.inset(-2.0), corner(14.0), 2.0, th().accent);
    }
    if clicked {
        activate(sh, v, k);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RailEntry {
    Group(&'static str),
    Category(Category),
}

pub(super) fn rail_entries(categories: &[Category]) -> Vec<RailEntry> {
    let mut entries = Vec::new();
    if categories.contains(&Category::Home) {
        entries.push(RailEntry::Category(Category::Home));
    }
    for (title, members) in Category::GROUPS {
        let available: Vec<_> = members
            .iter()
            .copied()
            .filter(|category| categories.contains(category))
            .collect();
        if available.is_empty() {
            continue;
        }
        entries.push(RailEntry::Group(title));
        entries.extend(available.into_iter().map(RailEntry::Category));
    }
    entries
}

pub(super) fn rail(sh: &mut Shell, v: &PauseView, area: Rect) {
    let (ox, oy, rail_w, height) = (area.x, area.y, area.w, area.h);
    let ui = &mut sh.ui;
    ui.solid(area);
    if th().glass.is_some() {
        // (a glass theme: the rail a pane of its own over the picture)
        ui.surface(
            Rect::new(ox + 4.0, oy + 4.0, rail_w - 8.0, height - 8.0),
            corner(14.0),
            th().sidebar,
        );
    } else {
        ui.p().rounded(area, corner(14.0), th().sidebar);
    }
    // the game's name and version: openOMSI's icon and name; a theme's wordmark across the top,
    // or its icon and name, over openOMSI's version
    if let (Some(tex), Some(aspect)) = (sh.logo_tex, sh.logo_wide) {
        // (as wide as the rail's buttons, at most 52 high: the version under it before Resume)
        let h = 48.0f32.min((rail_w - 32.0) / aspect);
        ui.image(Rect::new(ox + 16.0, oy + 10.0, h * aspect, h), tex, 0.0);
        ui.text_in(
            &format!("openOMSI {}", crate::startup::VERSION),
            Rect::new(ox + 18.0, oy + 10.0 + h, rail_w - 36.0, 13.0),
            9.5,
            Weight::Regular,
            th().ink_muted,
            Align::Left,
        );
    } else if let Some(tex) = sh.logo_tex {
        ui.image(Rect::new(ox + 18.0, oy + 18.0, 32.0, 32.0), tex, 7.0);
    }
    match sh.logo_title.as_deref().filter(|_| sh.logo_wide.is_none()) {
        _ if sh.logo_wide.is_some() => {}
        Some(title) => {
            ui.text_in(
                title,
                Rect::new(ox + 60.0, oy + 17.0, rail_w - 72.0, 22.0),
                18.0,
                Weight::Bold,
                th().ink,
                Align::Left,
            );
            ui.text_in(
                &format!("openOMSI {}", crate::startup::VERSION),
                Rect::new(ox + 60.0, oy + 38.0, rail_w - 72.0, 18.0),
                11.0,
                Weight::Regular,
                th().ink_muted,
                Align::Left,
            );
        }
        None => {
            ui.text(
                "openOMSI",
                Vec2::new(ox + 60.0, oy + 32.0),
                18.0,
                Weight::Bold,
                th().ink,
                Align::Left,
            );
            ui.text_in(
                crate::startup::VERSION,
                Rect::new(ox + 60.0, oy + 38.0, rail_w - 72.0, 18.0),
                11.0,
                Weight::Regular,
                th().ink_muted,
                Align::Left,
            );
        }
    }
    let status = if v.summary.bus.is_empty() && v.summary.driver.is_empty() {
        omsi_ui::tr(if v.paused { "Paused" } else { "Menu" }).into_owned()
    } else {
        let session = [v.summary.bus.as_str(), v.summary.driver.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        format!(
            "{} · {session}",
            omsi_ui::tr(if v.paused { "Paused" } else { "Menu" })
        )
    };
    ui.text_in(
        &status,
        Rect::new(ox + 18.0, oy + 62.0, rail_w - 36.0, 18.0),
        11.0,
        Weight::Regular,
        th().ink_muted,
        Align::Left,
    );
    session_action(
        sh,
        v,
        "resume",
        Rect::new(ox + 16.0, oy + 88.0, rail_w - 32.0, 46.0),
        ButtonKind::Primary,
    );
    let foot = 58.0;
    let category_area = Rect::new(
        ox + 12.0,
        oy + 146.0,
        rail_w - 24.0,
        (height - 146.0 - foot).max(0.0),
    );
    let categories = Category::available(&ids(v));
    let entries = rail_entries(&categories);
    let mut picked = None;
    let mut selected_y = None;
    let selected = sh.pause_category;
    let hidden = sh.pause_hidden;
    sh.ui
        .scroll_area("pause-categories", category_area, &mut |ui, view| {
            let mut y = 0.0;
            for entry in &entries {
                match entry {
                    RailEntry::Group(title) => {
                        let label = omsi_ui::tr(title).to_uppercase();
                        ui.text_in(
                            &label,
                            Rect::new(view.x + 10.0, view.y + y + 4.0, view.w - 20.0, 14.0),
                            9.5,
                            Weight::Bold,
                            th().ink_muted,
                            Align::Left,
                        );
                        y += 24.0;
                    }
                    RailEntry::Category(category) => {
                        let rect = Rect::new(view.x, view.y + y, view.w, 38.0);
                        if selected == *category {
                            selected_y = Some(y);
                        }
                        if category_button(ui, *category, selected == *category && !hidden, rect) {
                            picked = Some(*category);
                        }
                        y += 44.0;
                    }
                }
            }
            y
        });
    if v.kbd {
        if let Some(y) = selected_y {
            sh.ui
                .scroll_to("pause-categories", y, 38.0, category_area.h);
        }
    }
    if let Some(category) = picked {
        sh.actions.push(Action::PauseCategory(category));
    }
    sh.ui.p().rect(
        Rect::new(ox + 12.0, oy + height - foot, rail_w - 24.0, 1.0),
        th().edge,
    );
    session_action(
        sh,
        v,
        "quit",
        Rect::new(ox + 16.0, oy + height - 50.0, rail_w - 32.0, 42.0),
        ButtonKind::Danger,
    );
}

fn category_button(ui: &mut Ui, category: Category, selected: bool, rect: Rect) -> bool {
    let (hovered, _, clicked) = ui.interact(id_of(&format!("pause-category-{category:?}")), rect);
    let t = ui.anim(
        id_of(&format!("pause-category-hover-{category:?}")),
        if hovered { 1.0 } else { 0.0 },
        0.08,
    );
    ui.p().rounded(
        rect,
        corner(10.0),
        if selected {
            th().accent.alpha(0.10)
        } else {
            th().surface_2.alpha(t)
        },
    );
    let color = if selected {
        th().accent
    } else {
        th().ink_2.mix(th().ink, t)
    };
    ui.icon(
        category.icon(),
        Vec2::new(rect.x + 23.0, rect.center().y),
        22.0,
        color,
    );
    ui.text_in(
        &omsi_ui::tr(category.title()),
        Rect::new(rect.x + 48.0, rect.y, (rect.w - 71.0).max(0.0), rect.h),
        17.0,
        Weight::Medium,
        color,
        Align::Left,
    );
    ui.icon(
        "chevron_right",
        Vec2::new(rect.right() - 15.0, rect.center().y),
        15.0,
        color.alpha(0.6),
    );
    clicked
}

pub(super) fn page(sh: &mut Shell, v: &PauseView, page: Rect, compact: bool) {
    let ids = ids(v);
    if !Category::available(&ids).contains(&sh.pause_category) {
        sh.pause_category = Category::Home;
    }
    let category = sh.pause_category;
    if category != Category::Home {
        sh.quick_edit = false;
    }
    let pad = if page.w < 420.0 { 14.0 } else { 26.0 };
    let width = (page.w - pad * 2.0).max(0.0);
    let x = page.x + pad;
    let right = page.right() - pad;
    let crumb = format!(
        "{} / {}",
        omsi_ui::tr("Pause menu"),
        omsi_ui::tr(category.title())
    );
    sh.ui.text_in(
        &crumb,
        Rect::new(x, page.y + 14.0, (width - 226.0).max(0.0), 18.0),
        10.0,
        Weight::Medium,
        th().ink_muted,
        Align::Left,
    );
    let state = Rect::new(right - 84.0, page.y + 10.0, 84.0, 26.0);
    sh.ui
        .p()
        .rounded(state, corner(9.0), th().accent.alpha(0.12));
    sh.ui.text_in(
        &omsi_ui::tr(if v.paused { "Paused" } else { "Menu" }),
        state,
        10.0,
        Weight::Bold,
        th().accent,
        Align::Center,
    );
    let editing = sh.quick_edit;
    let title_y = page.y + 48.0;
    let title_w = if category == Category::Home {
        (width - 92.0).max(0.0)
    } else {
        width
    };
    sh.ui.text_in(
        &omsi_ui::tr(category.title()),
        Rect::new(x, title_y, title_w, 32.0),
        23.0,
        Weight::Bold,
        th().ink,
        Align::Left,
    );
    sh.ui.text_in(
        &omsi_ui::tr(if editing {
            "Choose actions and set their order"
        } else {
            category.hint()
        }),
        Rect::new(x, title_y + 35.0, width, 22.0),
        12.0,
        Weight::Regular,
        th().ink_2,
        Align::Left,
    );
    if category == Category::Home {
        let edit = Rect::new(x + width - 78.0, title_y - 1.0, 78.0, 34.0);
        if v.keys {
            sh.ui.tooltip(
                edit,
                &format!(
                    "{} ({})",
                    omsi_ui::tr(if editing { "Done" } else { "Edit" }),
                    if editing { "Esc" } else { "E" }
                ),
            );
        }
        let edit_clicked = sh.ui.button(
            "pause-quick-edit",
            edit,
            &omsi_ui::tr(if editing { "Done" } else { "Edit" }),
            None,
            ButtonKind::Ghost,
        );
        sh.ui
            .p()
            .rounded_border(edit, corner(8.0), super::EDGE_WIDTH, super::visible_edge());
        if edit_clicked {
            sh.quick_edit = !editing;
            sh.quick_focus = 0;
            sh.last_sel = None;
        }
    }
    let mut top = title_y + 66.0;
    if compact {
        session_action(
            sh,
            v,
            "resume",
            Rect::new(x, top, width, 42.0),
            ButtonKind::Primary,
        );
        top += 50.0;
        if !editing {
            let nav_h = (page.bottom() - top - 150.0).clamp(34.0, 78.0);
            top = compact_categories(sh, &ids, Rect::new(x, top, width, nav_h), v.kbd);
        }
    }
    let footer_space = if compact { 72.0 } else { 34.0 };
    let area = Rect::new(x, top, width, (page.bottom() - footer_space - top).max(0.0));
    if editing {
        super::quick_actions_ui::editor(sh, v, area);
    } else if category == Category::Online && v.rail.iter().any(|line| line.0 == "players") {
        super::online_ui::page(sh, v, area);
    } else {
        cards(sh, v, category, area);
    }
    if compact {
        session_action(
            sh,
            v,
            "quit",
            Rect::new(x, page.bottom() - 58.0, width, 42.0),
            ButtonKind::Danger,
        );
    }
    if v.keys {
        let text = omsi_ui::tr(if editing {
            "Up / Down: select | Space: toggle | Left / Right: reorder | R: reset | Esc: done"
        } else {
            "Esc resumes  |  Left / Right: categories  |  Up / Down: actions"
        });
        let footer = Rect::new(x, page.bottom() - 20.0, width, 14.0);
        let shown = if editing && width < 750.0 {
            "↑ ↓ / Space / ← → / R / Esc"
        } else {
            &text
        };
        sh.ui.text_in(
            shown,
            footer,
            10.0,
            Weight::Regular,
            th().ink_muted,
            Align::Left,
        );
        if editing {
            sh.ui.tooltip(footer, &text);
        }
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
    sh.ui
        .scroll_area("pause-compact-categories", viewport, &mut |ui, view| {
            for (category, label, rect) in &buttons {
                let r = Rect::new(view.x + rect.x, view.y + rect.y, rect.w, rect.h);
                if ui.button(
                    &format!("pause-compact-{category:?}"),
                    r,
                    label,
                    None,
                    ButtonKind::Ghost,
                ) {
                    picked = Some(*category);
                }
                ui.p()
                    .rounded_border(r, corner(10.0), super::EDGE_WIDTH, super::visible_edge());
                if selected == *category {
                    ui.p().rounded(
                        Rect::new(r.x + 8.0, r.bottom() - 2.0, (r.w - 16.0).max(0.0), 2.0),
                        1.0,
                        th().accent,
                    );
                }
            }
            y + 34.0
        });
    if keyboard {
        if let Some((_, _, rect)) = buttons
            .iter()
            .find(|(category, _, _)| *category == selected)
        {
            sh.ui
                .scroll_to("pause-compact-categories", rect.y, rect.h, height);
        }
    }
    if let Some(category) = picked {
        sh.actions.push(Action::PauseCategory(category));
    }
    area.y + height + 10.0
}

fn cards(sh: &mut Shell, v: &PauseView, category: Category, area: Rect) {
    let indices = super::quick_actions::indices(category, v.rail, v.quick_actions);
    let home = category == Category::Home;
    let pills = home && area.h >= 90.0;
    let columns = if pills && area.w >= 520.0 { 2 } else { 1 };
    let card_height: f32 = if pills { 120.0 } else { 60.0 };
    let row_step: f32 = if pills { 134.0 } else { 68.0 };
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
                th().ink_muted,
                Align::Left,
            );
        }
        let gap = 14.0;
        let card_width = ((view.w - gap * (columns - 1) as f32) / columns as f32).max(0.0);
        let visible_height = card_height.min(area.h.max(0.0));
        for (position, &index) in indices.iter().enumerate() {
            let row = position / columns;
            let column = position % columns;
            let y = row as f32 * row_step;
            let rect = Rect::new(
                view.x + column as f32 * (card_width + gap),
                view.y + y,
                card_width,
                visible_height,
            );
            let keyboard = v.kbd && v.rail_sel == Some(index);
            if keyboard {
                focus = Some((index, y));
            }
            if action_card(ui, v, index, rect, keyboard, pills) {
                picked = Some(index);
            }
        }
        (indices.len().div_ceil(columns) as f32 * row_step - gap).max(0.0)
    });
    if let Some((index, y)) = focus {
        let next = (id_of(&key), index);
        if sh.last_sel != Some(next) {
            sh.ui.scroll_to(&key, y, card_height.min(area.h), area.h);
            sh.last_sel = Some(next);
        }
    }
    if let Some(index) = picked {
        activate(sh, v, index);
    }
}

fn action_card(
    ui: &mut Ui,
    v: &PauseView,
    index: usize,
    rect: Rect,
    keyboard: bool,
    shortcut: bool,
) -> bool {
    let (id, label, plugin) = &v.rail[index];
    let disabled = v.disabled.contains(id);
    let (hovered, _, clicked) = ui.interact(id_of(&format!("pause-card-{index}-{id}")), rect);
    let t = ui.anim(
        id_of(&format!("pause-card-hover-{index}-{id}")),
        if hovered && !disabled { 1.0 } else { 0.0 },
        0.08,
    );
    let fill = if keyboard {
        th().surface.mix(th().accent, 0.12)
    } else {
        th().surface.mix(th().surface_2, t)
    };
    let radius = if shortcut { corner(18.0) } else { corner(12.0) };
    // (a card is a button: solid in every look - the glass is the panels' and the rail's)
    ui.p().rounded(rect, radius, fill);
    ui.p()
        .rounded_border(rect, radius, super::EDGE_WIDTH, super::visible_edge());
    if keyboard {
        ui.p().rounded_border(rect, radius, 2.0, th().accent);
    }
    let danger = *id == "endduty";
    let ink = if disabled {
        th().ink_muted
    } else if danger {
        th().danger_2
    } else {
        th().ink
    };
    let icon_c = if shortcut { th().accent } else { th().ink_2 };
    let tile_size = if shortcut { 46.0 } else { 32.0 };
    let tile = Rect::new(
        rect.x + if shortcut { 18.0 } else { 12.0 },
        rect.center().y - tile_size * 0.5,
        tile_size,
        tile_size,
    );
    ui.p().rounded(
        tile,
        if shortcut { corner(14.0) } else { corner(10.0) },
        if shortcut {
            th().accent.alpha(0.10)
        } else {
            th().surface_2
        },
    );
    let icon = plugin
        .as_ref()
        .and_then(|p| p.icon.as_deref())
        .and_then(|i| omsi_ui::icons::names().find(|n| *n == i.trim()))
        .unwrap_or_else(|| {
            if *id == "plugin" {
                "extension"
            } else {
                rail_icon(id)
            }
        });
    ui.icon(
        icon,
        tile.center(),
        if shortcut { 27.0 } else { 20.0 },
        icon_c,
    );
    let label = if matches!(*id, "vehicle" | "world") {
        omsi_ui::tr("Vehicle and world")
    } else if *id == "multiplayer" {
        omsi_ui::tr(super::navigation::Category::Online.title())
    } else if *id == "plugin" {
        label.clone()
    } else {
        omsi_ui::tr(label)
    };
    let text = plain(&label).0;
    let content = if shortcut {
        Rect::new(
            rect.x + 80.0,
            rect.y + 25.0,
            (rect.w - 116.0).max(0.0),
            29.0,
        )
    } else {
        Rect::new(rect.x + 56.0, rect.y + 6.0, (rect.w - 92.0).max(0.0), 22.0)
    };
    ui.text_in(
        text,
        content,
        if shortcut { 18.0 } else { 14.0 },
        Weight::Bold,
        ink,
        Align::Left,
    );
    let hint = omsi_ui::tr(action_hint(id));
    ui.text_in(
        &hint,
        Rect::new(
            content.x,
            rect.y + if shortcut { 65.0 } else { 29.0 },
            content.w,
            if shortcut { 25.0 } else { 18.0 },
        ),
        if shortcut { 14.5 } else { 11.0 },
        Weight::Regular,
        th().ink_muted,
        Align::Left,
    );
    ui.icon(
        "chevron_right",
        Vec2::new(
            rect.right() - if shortcut { 21.0 } else { 17.0 },
            rect.center().y,
        ),
        if shortcut { 17.0 } else { 15.0 },
        th().ink_muted,
    );
    clicked && !disabled
}

fn action_hint(id: &str) -> &'static str {
    match id {
        "multiplayer" => super::navigation::Category::Online.hint(),
        "camera" => "Seat position, field of view and head movement",
        "map" => "View the city and your position",
        "streets" => "Look a street up by its name on the city map",
        "duty" => "Choose a line and a tour",
        "save" | "saveslot" | "load" => "Manage your saved situations",
        "options" | "controls" => "Adjust your game preferences",
        "vehicle" | "world" => "Adjust your vehicle and the simulation around it.",
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
