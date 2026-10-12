//! RosterFlow's scrollbars (Optibus's): a slim rounded thumb on a faint track, the thumb
//! brighter under the mouse and while it is dragged. The thumb is as long as the part shown;
//! it is dragged, and a click on the track beside it moves a page that way. The Vehicles and
//! Drivers Gantt has one along the day (once zoomed in) and one down the rows, the timeplan
//! one along its stops, the Roster tab's lists one each (`area`).

use super::super::super::ui::{id_of, Ui};
use super::rosterflow::Pal;
use omsi_ui::{Color, Rect};

/// A bar's thickness, the track's room included.
pub(super) const BAR: f32 = 12.0;

/// A bar's colours: the track, the thumb, the thumb under the mouse or held.
#[derive(Clone, Copy)]
pub(super) struct Look {
    pub track: Color,
    pub thumb: Color,
    pub hot: Color,
}

/// RosterFlow's bar in its palette (dark or light).
pub(super) fn look(p: &Pal) -> Look {
    if p.light {
        Look { track: p.text.alpha(0.06), thumb: p.text.alpha(0.24), hot: p.text.alpha(0.44) }
    } else {
        Look { track: Color::WHITE.alpha(0.05), thumb: Color::WHITE.alpha(0.22), hot: Color::WHITE.alpha(0.42) }
    }
}

/// A scrollbar in `track` for a view of `view` out of `total`, scrolled to `pos` (0 to
/// `total - view`). Draws nothing and returns None when everything is in view; else returns
/// where it was dragged or paged to.
#[allow(clippy::too_many_arguments)]
pub(super) fn bar(ui: &mut Ui, look: &Look, name: &str, track: Rect, vertical: bool, pos: f32, view: f32, total: f32) -> Option<f32> {
    if total <= view + 0.5 || view <= 0.0 {
        return None;
    }
    let max = total - view;
    let (start, len) = if vertical { (track.y, track.h) } else { (track.x, track.w) };
    let thumb_len = (len * view / total).max(28.0f32.min(len * 0.5)).min(len);
    let room = (len - thumb_len).max(1.0);
    let at = start + room * (pos / max).clamp(0.0, 1.0);
    let thumb = if vertical { Rect::new(track.x, at, track.w, thumb_len) } else { Rect::new(at, track.y, thumb_len, track.h) };
    let id = id_of(name);
    let (_, _, paged) = ui.interact(id ^ 0x5c71, track);
    let (over, held, _) = ui.interact(id ^ 0x5c72, thumb);
    let m = if vertical { ui.input.mouse.y } else { ui.input.mouse.x };
    // (where on the thumb it was taken: it stays under the mouse)
    let grab = id ^ 0x5c73;
    if over && ui.input.pressed {
        ui.scroll.insert(grab, m - at);
    }
    let mut out = None;
    if held {
        let g = ui.scroll.get(&grab).copied().unwrap_or(thumb_len * 0.5);
        out = Some(((m - g - start) / room).clamp(0.0, 1.0) * max);
    } else if paged && !over {
        let page = view * 0.9;
        out = Some(if m < at { (pos - page).max(0.0) } else { (pos + page).min(max) });
    }
    let hot = ui.anim(id ^ 0x5c74, if over || held { 1.0 } else { 0.0 }, 0.1);
    let thick = if vertical { track.w } else { track.h };
    let inner = if vertical { Rect::new(track.x + 3.0, track.y + 2.0, thick - 6.0, track.h - 4.0) } else { Rect::new(track.x + 2.0, track.y + 3.0, track.w - 4.0, thick - 6.0) };
    let r = (thick - 6.0) * 0.5;
    ui.p().rounded(inner, r, look.track);
    let t = if vertical { Rect::new(inner.x, thumb.y + 2.0, inner.w, (thumb.h - 4.0).max(8.0)) } else { Rect::new(thumb.x + 2.0, inner.y, (thumb.w - 4.0).max(8.0), inner.h) };
    ui.p().rounded(t, r, look.thumb.mix(look.hot, hot));
    out
}

/// A list that scrolls in `r` with RosterFlow's bar on its right (the launcher's
/// `scroll_area` as RosterFlow draws it: `scroll_to` finds it by the same name). `body`
/// draws the rows given the room moved by the scroll and returns their height.
pub(super) fn area(ui: &mut Ui, look: &Look, name: &str, r: Rect, body: &mut dyn FnMut(&mut Ui, Rect) -> f32) {
    let id = id_of(name);
    let off = ui.scroll.get(&id).copied().unwrap_or(0.0);
    ui.push_clip(r, 0.0);
    let content = body(ui, Rect::new(r.x, r.y - off, r.w, r.h));
    ui.pop_clip();
    let max = (content - r.h).max(0.0);
    let mut target = ui.scroll.get(&(id ^ 0xabc)).copied().unwrap_or(off);
    if max > 0.0 && ui.hover(r) && !ui.over_popup() && ui.input.wheel.y.abs() > 0.0 && !ui.input.ctrl {
        target -= ui.input.wheel.y * 42.0;
        ui.input.wheel.y = 0.0;
    }
    let track = Rect::new(r.right() - BAR, r.y, BAR, r.h);
    let mut now_too = false;
    if let Some(to) = bar(ui, look, name, track, true, off, r.h, content) {
        target = to;
        now_too = true;
    }
    target = target.clamp(0.0, max);
    ui.scroll.insert(id ^ 0xabc, target);
    let k = 1.0 - (-ui.dt / 0.07).exp();
    let now = if now_too { target } else { off + (target - off) * k };
    ui.scroll.insert(id, if (now - target).abs() < 0.3 { target } else { now });
}
