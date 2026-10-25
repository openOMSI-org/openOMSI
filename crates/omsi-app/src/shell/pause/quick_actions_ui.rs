//! Shortcut editing uses saved IDs; row clicks never execute game actions.
use super::*;
use super::quick_actions::{self, Edit};

fn label(id: &str, menu: &[crate::app_impl::MenuLine]) -> String {
    let label = menu.iter().find(|line| line.0 == id && line.2.is_none()).map(|line| line.1.as_ref())
        .or_else(|| crate::app_impl::GAME_MENU.iter().find(|line| line.0 == id).map(|line| line.1))
        .unwrap_or(match id {
            "tobus" => "Back to my bus", "players" => "Player list...",
            "admin" => "Administration...", "copycode" => "Copy server code", _ => id,
        });
    plain(&omsi_ui::tr(label)).0.to_string()
}

pub(super) fn editor(sh: &mut Shell, v: &PauseView, area: Rect) {
    let entries = quick_actions::entries(v.rail, v.quick_actions);
    let chosen = quick_actions::selected(v.quick_actions);
    let mut edits = Vec::new();
    let mut picked = None;
    let focus = sh.quick_focus.min(entries.len().saturating_sub(1));
    let keyboard = v.kbd && !sh.ui.hover(area);
    let row_h = 62.0;
    let scroll_rect = Rect::new(area.x, area.y, area.w, (area.h - 54.0).max(0.0));
    sh.ui.p().rounded(area, 10.0, SURFACE);
    sh.ui.scroll_area("pause-quick-editor", scroll_rect, &mut |ui, view| {
        for (i, id) in entries.iter().enumerate() {
            let rect = Rect::new(view.x + 8.0, view.y + i as f32 * row_h + 4.0, (view.w - 16.0).max(0.0), row_h - 8.0);
            let available = v.rail.iter().any(|line| line.0 == id && line.2.is_none()) && !v.disabled.contains(&id.as_str());
            let on = chosen.contains(id);
            if keyboard && focus == i { ui.p().rounded_border(rect, 6.0, 2.0, ACCENT); }
            let field = Rect::new(rect.x, rect.y, (rect.w - 88.0).max(0.0), rect.h);
            let (hover, _, clicked) = ui.interact(id_of(&format!("pause-quick-toggle-{id}")), field);
            if hover { ui.p().rounded(field, 6.0, SURFACE_2); }
            let check = Rect::new(field.x + 8.0, field.center().y - 10.0, 20.0, 20.0);
            ui.p().rounded_border(check, 3.0, 1.0, if on { ACCENT } else { INK_MUTED });
            if on { ui.icon("check", check.center(), 18.0, ACCENT); }
            let text = label(id, v.rail);
            let name = Rect::new(field.x + 38.0, field.y, (field.w - 44.0).max(0.0), if available { field.h } else { 30.0 });
            ui.text_in(&text, name, 15.0, Weight::Medium, if available { INK } else { INK_MUTED }, Align::Left);
            ui.tooltip(field, &text);
            if !available { ui.text_in(&omsi_ui::tr("Currently unavailable"), Rect::new(name.x, field.y + 29.0, name.w, 18.0), 11.0, Weight::Regular, INK_MUTED, Align::Left); }
            if clicked { edits.push(Edit::Toggle(id.clone())); picked = Some(i); }
            for (down, icon, tip, x) in [(false, "expand_less", "Up", rect.right() - 84.0), (true, "expand_more", "Down", rect.right() - 40.0)] {
                let r = Rect::new(x, rect.y + 7.0, 36.0, 40.0);
                let at = chosen.iter().position(|v| v == id);
                let enabled = at.is_some_and(|at| if down { at + 1 < chosen.len() } else { at > 0 });
                if enabled {
                    if ui.button(&format!("pause-quick-move-{id}-{down}"), r, "", Some(icon), ButtonKind::Ghost) {
                        edits.push(Edit::Move(id.clone(), down)); picked = Some(i);
                    }
                    ui.tooltip(r, &omsi_ui::tr(tip));
                } else { ui.icon(icon, r.center(), 18.0, INK_MUTED.alpha(0.3)); }
            }
        }
        entries.len() as f32 * row_h + 8.0
    });
    if keyboard {
        let next = (id_of("pause-quick-editor"), focus);
        if sh.last_sel != Some(next) {
            sh.ui.scroll_to("pause-quick-editor", focus as f32 * row_h, row_h, scroll_rect.h);
            sh.last_sel = Some(next);
        }
    }
    if let Some(i) = picked { sh.quick_focus = i; }
    let w = ((area.w - 24.0) * 0.5).max(0.0);
    let y = area.bottom() - 46.0;
    if sh.ui.button("pause-quick-reset", Rect::new(area.x + 8.0, y, w, 38.0), &omsi_ui::tr("Reset to defaults"), None, ButtonKind::Ghost) { edits.push(Edit::Reset); }
    if sh.ui.button("pause-quick-done", Rect::new(area.right() - 8.0 - w, y, w, 38.0), &omsi_ui::tr("Done"), Some("check"), ButtonKind::Primary) { sh.quick_edit = false; }
    sh.actions.extend(edits.into_iter().map(Action::QuickActions));
}

#[cfg(test)]
mod tests {
    #[test]
    fn quick_actions_reorder_icons_are_available() {
        for icon in ["expand_less", "expand_more", "check"] {
            assert!(omsi_ui::icons::svg(icon).is_some(), "missing editor icon: {icon}");
        }
    }
}
