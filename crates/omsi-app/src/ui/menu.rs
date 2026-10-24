//! The game menu and its lists, drawn with the launcher's look.

use super::*;

// ---------------------------------------------------------------------------------------
// the game menu and its lists: flat near-black greys and one amber accent, as the launcher

/// How long a fade of the menu takes (a line lit, a switch turned over): a short moment.
pub(super) const FADE_SECS: f32 = 0.15;
pub(super) const BAR_SECS: f32 = 0.19;

// (the launcher's values, see `launcher/theme.rs`: neutral greys, no tint)
pub(super) const PANEL: [u8; 4] = [22, 22, 22, 255];
pub(super) const PANEL_ALT: [u8; 4] = [31, 31, 31, 255];
pub(super) const BORDER: [u8; 4] = [255, 255, 255, 15];
pub(super) const ACCENT: [u8; 4] = [232, 160, 48, 255];
pub(super) const ACCENT_SOFT: [u8; 4] = [232, 160, 48, 34];
pub(super) const DANGER: [u8; 4] = [222, 78, 68, 255];
/// A line under the mouse, and the line chosen: the launcher's `HOVER` and `SELECTED`, as
/// solid greys. (They were white at 6 % and 9 %, which the overlays blend in linear light:
/// on the 22-grey card that came out 73 and 92, twice as light as the launcher's 38 and 44.)
pub(super) const LIT: [u8; 4] = [38, 38, 38, 255];
pub(super) const SELECTED: [u8; 4] = [44, 44, 44, 255];
/// "End the session" under the mouse: the danger colour at 14 % on the card, solid.
pub(super) const LIT_DANGER: [u8; 4] = [50, 30, 28, 255];
/// A stepper's or a button's field: the launcher's `FIELD`.
pub(super) const CHIP: [u8; 4] = [31, 31, 31, 255];
/// A switch's track when off, a slider's empty track, their knob (the launcher's).
pub(super) const TRACK_OFF: [u8; 4] = [62, 62, 62, 255];
pub(super) const SLIDER_TRACK: [u8; 4] = [58, 58, 58, 255];
pub(super) const KNOB: [u8; 4] = [240, 240, 240, 255];
/// A scroll bar's thumb, idle and with the mouse over its list (the launcher's white at
/// 10 % and 35 %; no track, no accent).
pub(super) const THUMB: [u8; 4] = [255, 255, 255, 26];
pub(super) const THUMB_HOT: [u8; 4] = [255, 255, 255, 90];
/// The small capitals over a title (the launcher's section headings: 11 px, bold, dim).
pub(super) const DIM: [u8; 4] = [142, 142, 142, 0];
/// The accent's fill under the mouse, and the ink on it (the launcher's primary button).
pub(super) const ACCENT_HOT: [u8; 4] = [246, 182, 84, 255];
pub(super) const ON_ACCENT: [u8; 4] = [18, 14, 8, 0];
/// Text colours (alpha 0: no outline on the flat panel).
pub(super) const WHITE: [u8; 4] = [236, 236, 236, 0];
pub(super) const SOFT: [u8; 4] = [200, 200, 200, 0];
pub(super) const MUTED: [u8; 4] = [142, 142, 142, 0];
pub(super) const AMBER: [u8; 4] = [255, 200, 110, 0];

/// The ink of a greyed-out line and its small hint.
pub(super) const OFF_INK: [u8; 4] = [96, 96, 96, 0];
pub(super) const OFF_HINT: [u8; 4] = [96, 96, 96, 0];

/// The card's radius, a line's, and the inset of lines from the card's edge and of their
/// text from the line's edge (all times the scale).
pub(super) const CARD_R: f32 = 8.0;
pub(super) const ROW_R: f32 = 6.0;
pub(super) const PAD: f32 = 12.0;
pub(super) const TEXT_IN: f32 = 16.0;

/// `v` (0 to 1) in eighths.
pub(super) fn quant(v: f32) -> f32 {
    (v.clamp(0.0, 1.0) * 8.0).round() / 8.0
}

/// The colour `t` (0 to 1) of the way from `a` to `b`.
pub(super) fn mix(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    [m(a[0], b[0]), m(a[1], b[1]), m(a[2], b[2]), m(a[3], b[3])]
}

/// `c` with its opacity times `k` (0 to 1).
pub(super) fn fade(c: [u8; 4], k: f32) -> [u8; 4] {
    [c[0], c[1], c[2], (c[3] as f32 * k.clamp(0.0, 1.0)).round() as u8]
}

/// `c` as a text colour (no outline).
pub(super) fn txt(c: [u8; 4]) -> [u8; 4] {
    [c[0], c[1], c[2], 0]
}

/// The game menu's size: the screen's scale times the interface size (`Frame::ui_scale`, the
/// "Interface size" setting and the growth with tall windows, as the rest of the HUD has it),
/// held where the settings window (880 x 600 as designed) would no longer fit the window -
/// but never below the screen's scale alone, the size it always had.
pub(super) fn menu_scale(f: &Frame) -> f32 {
    menu_scale_for(f.scale, f.ui_scale, f.width, f.height, f.vr)
}

pub(super) fn menu_scale_for(scale: f32, ui_scale: f32, width: f32, height: f32, vr: bool) -> f32 {
    let base = scale.max(0.5);
    if vr {
        return base;
    }
    let fit = (width / (880.0 + 24.0)).min(height * 0.94 / 600.0);
    (base * ui_scale.max(0.5)).min(fit.max(base))
}

/// A list line `"<code>  <name>"` (a number, maybe padded in front, two spaces, a name): its
/// code and name.
pub(super) fn code_and_name(label: &str) -> Option<(&str, &str)> {
    let (code, name) = label.trim_start().split_once("  ")?;
    (!code.is_empty() && code.chars().all(|c| c.is_ascii_digit()) && !name.trim().is_empty()).then(|| (code, name.trim()))
}

/// The cursor is in `rect`.
pub(super) fn over_rect(c: (f32, f32), rect: [f32; 4]) -> bool {
    c.0 >= rect[0] && c.0 <= rect[2] && c.1 >= rect[1] && c.1 <= rect[3]
}

/// `text` clipped to `width` pixels in the bold weight (with "...").
pub(super) fn clip_bold(tc: &TextCache, text: &str, px: f32, width: f32) -> String {
    let text = &*omsi_ui::tr(text);
    if tc.width_bold(text, px) <= width {
        return text.to_string();
    }
    let mut out: String = text.chars().collect();
    while !out.is_empty() && tc.width_bold(&format!("{out}..."), px) > width {
        out.pop();
    }
    format!("{}...", out.trim_end())
}

/// The distance of the point (`px`, `py`) from the rounded rectangle at (`x0`, `y0`) of
/// `w` x `h` with corners of `rad`: negative inside.
pub(super) fn rr_dist(px: f32, py: f32, x0: f32, y0: f32, w: f32, h: f32, rad: f32) -> f32 {
    let (cx, cy) = (x0 + w * 0.5, y0 + h * 0.5);
    let qx = (px - cx).abs() - (w * 0.5 - rad);
    let qy = (py - cy).abs() - (h * 0.5 - rad);
    (qx.max(0.0) * qx.max(0.0) + qy.max(0.0) * qy.max(0.0)).sqrt() + qx.max(qy).min(0.0) - rad
}

/// A line that opens another list ends in dots: the text without them, and whether it did.
pub(super) fn strip_more(label: &str) -> (&str, bool) {
    match label.strip_suffix("...").or_else(|| label.strip_suffix('…')) {
        Some(t) => (t.trim_end(), true),
        None => (label, false),
    }
}

impl Ui {
    /// `text` at `x`, its middle on `cy`; returns its width.
    pub(super) fn put(&mut self, r: &Renderer, scene: &mut Scene, text: &str, px: u32, color: [u8; 4], x: f32, cy: f32) -> f32 {
        let l = self.text.label(r, scene, text, px, color);
        let y = cy - l.h as f32 * 0.5;
        scene.overlays.push((l.tex, [x, y, x + l.w as f32, y + l.h as f32]));
        l.w as f32
    }

    /// The value that eases to `target` (a part of the menu changing its colour or place):
    /// each frame it goes `speed` per second of the way (exponentially). One value per `key`;
    /// the first time it is asked for it is there already.
    pub(super) fn ease(&mut self, key: (u8, &str, usize), target: f32, speed: f32) -> f32 {
        // (`speed`: the share of the whole way it goes in a second: it takes 1 / speed seconds)
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut h);
        let dt = self.anim_dt;
        let v = self.anim.entry(h.finish()).or_insert(target);
        let step = speed * dt;
        if (target - *v).abs() <= step {
            *v = target;
        } else if target > *v {
            *v += step;
        } else {
            *v -= step;
        }
        *v
    }

    /// `ease`, in eighths: a colour or a light that fades is a texture made once per step
    /// (a plate or a text of that colour), so a fade goes in a few steps, not in hundreds.
    pub(super) fn easeq(&mut self, key: (u8, &str, usize), target: f32, speed: f32) -> f32 {
        quant(self.ease(key, target, speed))
    }

    /// `text` ending at `right`, its middle on `cy`; returns its width.
    pub(super) fn put_right(&mut self, r: &Renderer, scene: &mut Scene, text: &str, px: u32, color: [u8; 4], right: f32, cy: f32) -> f32 {
        let l = self.text.label(r, scene, text, px, color);
        let y = cy - l.h as f32 * 0.5;
        scene.overlays.push((l.tex, [right - l.w as f32, y, right, y + l.h as f32]));
        l.w as f32
    }

    /// A scroll bar's thumb in `track` for `shown` of `n` lines from `first`, as the
    /// launcher's: no track, a thin white bar that lightens while the mouse is over its list
    /// (`hot`), at least 28 px long. Returns the rect drawn.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn thumb(&mut self, r: &Renderer, scene: &mut Scene, track: [f32; 4], first: usize, shown: usize, n: usize, hot: bool, s: f32) -> [f32; 4] {
        let th = track[3] - track[1];
        let n = n.max(1) as f32;
        let len = (th * shown as f32 / n).max((28.0 * s).min(th));
        let t0 = track[1] + (th - len) * (first as f32 / (n - shown as f32).max(1.0)).clamp(0.0, 1.0);
        let thumb = [track[0], t0, track[2], t0 + len];
        let a = self.easeq((16, "thumb", track[0] as usize), if hot { 1.0 } else { 0.0 }, 1.0 / FADE_SECS);
        self.text.rounded(r, scene, thumb, 2.0 * s, mix(THUMB, THUMB_HOT, a));
        thumb
    }

    /// The accent bar at the left edge of a line, growing from its middle as `k` goes 0 to 1.
    pub(super) fn accent_bar(&mut self, r: &Renderer, scene: &mut Scene, rect: [f32; 4], k: f32, danger: bool, s: f32) {
        let full = (rect[3] - rect[1] - 20.0 * s).max(10.0 * s);
        let bh = full * (0.4 + 0.6 * k);
        let by = (rect[1] + rect[3]) * 0.5 - bh * 0.5;
        self.text.rounded(r, scene, [rect[0], by, rect[0] + 2.0 * s, by + bh], 1.0 * s, fade(if danger { DANGER } else { ACCENT }, k));
    }

    /// The header of the card at (`x`, `y`) of `w` wide: what the list is of, small and in
    /// capitals, the title large under it, a hairline under both. Its text starts where the
    /// text of the lines does. Returns the middle of the header.
    pub(super) fn menu_header(&mut self, r: &Renderer, scene: &mut Scene, x: f32, y: f32, w: f32, header_h: f32, title: &str, sub: &str, s: f32) -> f32 {
        let left = x + (PAD + TEXT_IN) * s;
        // (the game's name in the accent; the list names above are in capitals)
        // (as the launcher's section headings: small bold capitals, dim; the title bold)
        let eyebrow = if sub.is_empty() { "OPENOMSI".to_string() } else { sub.to_uppercase() };
        let room = w - (PAD + TEXT_IN) * 2.0 * s;
        let eyebrow = clip_bold(&self.text, &eyebrow, 11.0 * s, room);
        let e = self.text.label(r, scene, &eyebrow, (11.0 * s) as u32 | BOLD, DIM);
        let title = clip_bold(&self.text, title, 22.0 * s, room);
        let t = self.text.label(r, scene, &title, (22.0 * s) as u32 | BOLD, WHITE);
        let band = header_h - 6.0 * s;
        let top = y + (band - (e.h as f32 + t.h as f32 - 2.0 * s)) * 0.5;
        scene.overlays.push((e.tex, [left, top, left + e.w as f32, top + e.h as f32]));
        let ty = top + e.h as f32 - 2.0 * s;
        scene.overlays.push((t.tex, [left, ty, left + t.w as f32, ty + t.h as f32]));
        y + band * 0.5
    }

    /// A pill (`cap`: a key cap) of `fill` with `text` in it, ending at `right`; returns
    /// its left edge.
    pub(super) fn chip(&mut self, r: &Renderer, scene: &mut Scene, text: &str, px: u32, color: [u8; 4], fill: [u8; 4], cap: bool, right: f32, cy: f32, s: f32) -> f32 {
        let l = self.text.label(r, scene, text, px, color);
        let (cw, ch) = (l.w as f32 + 18.0 * s, l.h as f32 + 6.0 * s);
        let x0 = right - cw;
        if cap {
            // (a key cap: a fine light edge round it)
            self.text.rounded(r, scene, [x0 - 1.0, cy - ch * 0.5 - 1.0, right + 1.0, cy + ch * 0.5 + 1.0], 5.0 * s, [96, 96, 96, 255]);
        }
        self.text.rounded(r, scene, [x0, cy - ch * 0.5, right, cy + ch * 0.5], if cap { 4.0 * s } else { ROW_R * s }, fill);
        let (tx, ty) = (x0 + 9.0 * s, cy - l.h as f32 * 0.5);
        scene.overlays.push((l.tex, [tx, ty, tx + l.w as f32, ty + l.h as f32]));
        x0
    }

    /// The game menu and its lists (options, lines, tours ...): a card in the middle of a
    /// dimmed picture. Options are settings lines with switches and values; lines and tours
    /// are bigger lines with the timetable of the chosen one beside them.
    pub(super) fn draw_menu(&mut self, r: &Renderer, scene: &mut Scene, f: &Frame) {
        self.menu_rects.clear();
        self.menu_ctl.clear();
        self.menu_side.clear();
        self.menu_pane.clear();
        self.menu_pane_start = 0;
        self.menu_pane_go = None;
        self.menu_pane_box = None;
        self.menu_pane_scroll = None;
        self.menu_time.clear();
        self.menu_scroll_thumb = None;
        self.menu_scroll_track = None;
        self.dd_rects.clear();
        self.dd_scroll = None;
        let overlay_start = scene.overlays.len();
        let Some((sel, items)) = f.menu else {
            self.menu_overlay_range = overlay_start..overlay_start;
            self.anim.clear();
            return;
        };
        // a settings window has its own layout
        if f.menu_kind == MenuKind::Options && f.menu_tabs.is_some() {
            self.draw_settings(r, scene, f, sel, items);
            self.menu_overlay_range = overlay_start..scene.overlays.len();
            return;
        }
        // the pause menu itself: the launcher's rail (not in a headset, whose menu is a card)
        if f.menu_kind == MenuKind::Game && f.menu_head.is_none() && !f.vr {
            self.draw_game_menu(r, scene, f, sel, items);
            self.menu_overlay_range = overlay_start..scene.overlays.len();
            return;
        }
        let s = menu_scale(f);
        let kind = f.menu_kind;
        // the picture dimmed behind the menu (the first overlay: the headset's menu takes it
        // for the backdrop)
        let dim = self.text.plate(r, scene, 6);
        let sep = self.text.plate(r, scene, 9);
        scene.overlays.push((dim, [0.0, 0.0, f.width, f.height]));
        // key hints only where there is a keyboard
        let keys = !f.vr && !crate::platform::touch_controls();
        // the timetable beside the list, where the window is wide enough
        // VR uses the same timetable pane and hit rectangles as the desktop:
        // hiding it also removes trip-time selection and the start-trip button.
        let preview = f.menu_preview.as_ref().filter(|_| f.width >= 760.0 * s);
        let timetable_kind = matches!(kind, MenuKind::Lines | MenuKind::Tours);
        let pane_w = if preview.is_some() { (if timetable_kind { 320.0 } else { 340.0 }) * s } else { 0.0 };
        let want = if timetable_kind { 360.0 * s } else { 380.0 * s };
        let w = (want + pane_w).min(f.width - 24.0 * s).max(200.0 * s);
        let list_w = w - pane_w;
        let header_h = 72.0 * s;
        let pad = PAD * s;
        let tin = TEXT_IN * s;
        // as many lines as fit at a readable height; a longer menu scrolls (the wheel, the
        // arrow keys), the chosen line kept in view
        // (lines and tours: a card of one fixed height, a share of the screen's)
        // "Back" of lines, tours and the other lists stands alone under the list (not a line of
        // it: no scrolling down to it), and their card keeps one size
        let back_txt = omsi_ui::tr("Back").into_owned();
        let back_footer = (timetable_kind || kind == MenuKind::List) && items.last().is_some_and(|&(id, l)| id == "back" && l == back_txt.as_str());
        let nl = items.len() - back_footer as usize;
        let foot_h = if back_footer { 48.0 * s } else { 0.0 };
        let fixed_h = matches!(kind, MenuKind::Lines | MenuKind::Tours | MenuKind::List).then(|| if f.vr { f.height * 0.60 } else { (520.0 * s).min(f.height * 0.94) });
        let room = match fixed_h {
            Some(fh) => fh - header_h - pad - foot_h,
            None => f.height * (if f.vr { 0.60 } else { 0.92 }) - header_h - pad - 8.0 * s,
        };
        let base = match (f.vr, kind) {
            (true, _) => 40.0,
            (_, MenuKind::Lines) => 44.0,
            (_, MenuKind::Tours) => 40.0,
            _ => 42.0,
        } * s;
        let row_h = base.min(room / nl.max(1) as f32).max(34.0 * s);
        let rows = ((room / row_h).floor() as usize).clamp(1, nl.max(1));
        let sel_l = sel.min(nl.saturating_sub(1));
        let start = match (nl > rows, f.menu_top) {
            (false, _) => 0,
            (true, Some(top)) => (top.max(0.0).round() as usize).min(nl - rows),
            (true, None) => sel_l.saturating_sub(rows / 2).min(nl - rows),
        };
        self.menu_start = start;
        self.menu_rows = rows;
        self.menu_row_h = row_h;
        let px = (((if timetable_kind { 14.0 } else { 16.0 }) * s).min(row_h * 0.45)) as u32;
        let mut h = header_h + row_h * rows as f32 + pad + foot_h;
        if let Some(fh) = fixed_h {
            // (lines and tours: one size, whatever the list holds, like the settings window)
            h = fh;
        } else if preview.is_some() {
            h = h.max((420.0 * s).min(f.height * 0.92));
        }
        let x = ((f.width - w) * 0.5).round();
        let y = ((f.height - h) * 0.5).round();
        let list_r = x + list_w;
        let radius = CARD_R * s;
        // the card: a soft shadow, a 1 px hairline round it, the flat panel
        self.text.shadow(r, scene, [x, y, x + w, y + h], radius, 28.0 * s, 10.0 * s, 110);
        self.text.rounded(r, scene, [x - 1.0, y - 1.0, x + w + 1.0, y + h + 1.0], radius + 1.0, BORDER);
        self.text.rounded(r, scene, [x, y, x + w, y + h], radius, PANEL);
        // the header: what the list is of (or the game's name), and the title
        let (title, sub): (String, String) = match f.menu_head.as_ref() {
            Some((t, u)) => (t.clone(), u.clone()),
            None => {
                let t = if f.paused { "Paused" } else { "Menu" };
                (t.to_string(), String::new())
            }
        };
        self.menu_header(r, scene, x, y, w, header_h, &title, &sub, s);
        // the scroll bar: where the lines shown lie in the whole menu
        let scrolls = nl > rows;
        if scrolls {
            let top = y + header_h;
            let track = [list_r - 12.0 * s, top, list_r - 8.0 * s, top + row_h * rows as f32 - 4.0 * s];
            self.menu_scroll_track = Some(track);
            let hot = over_rect(f.cursor, [x, y, list_r, y + h]);
            let thumb = self.thumb(r, scene, track, start, rows, nl, hot, s);
            // (a wider grip than the drawn thumb: three pixels are hard to hit)
            self.menu_scroll_thumb = Some([thumb[0] - 6.0 * s, thumb[1], thumb[2] + 6.0 * s, thumb[3]]);
        }
        // (the line under the mouse is the one lit; the keyboard's choice only while the
        // mouse is off the lines - both lit at once read as two choices)
        let over = |rect: [f32; 4]| f.cursor.0 >= rect[0] && f.cursor.0 <= rect[2] && f.cursor.1 >= rect[1] && f.cursor.1 <= rect[3];
        // (the whole list: in the gaps between the lines the keyboard's choice, the top line,
        // lit up for a moment as the mouse went down the list)
        let any_hovered = over([x, y, list_r, y + h]);
        let right = list_r - if scrolls { 20.0 * s } else { pad };
        let line_pre = format!("{} ", omsi_ui::tr("Line"));
        let tour_pre = format!("{} ", omsi_ui::tr("Tour"));
        // (all line signs as wide as the widest, so that the texts after them line up)
        let mut sign_w = 48.0 * s;
        if kind == MenuKind::Lines {
            for &(_, label) in items.iter() {
                if let Some(n) = label.strip_prefix(line_pre.as_str()).and_then(|rest| rest.rsplit_once("  (")).map(|(n, _)| n) {
                    sign_w = sign_w.max(self.text.width(n, 15.0 * s) + 22.0 * s);
                }
            }
        }
        // (a list of codes and names - the destinations, `"{code:>3}  {name}"` - in two
        // columns: the codes right-aligned in one as wide as the widest, the names after it.
        // Padded with spaces, the proportional font left them ragged.)
        let mut code_w = 0.0f32;
        if kind == MenuKind::List {
            for &(_, label) in items[..nl].iter() {
                if let Some((c, _)) = code_and_name(label) {
                    code_w = code_w.max(self.text.width(c, px as f32));
                }
            }
        }
        for (k, &(id, label)) in items[..nl].iter().enumerate().skip(start).take(rows) {
            let ry = y + header_h + row_h * (k - start) as f32;
            let gap = 4.0 * s;
            let rect = [x + pad, ry, right, ry + row_h - gap];
            // (a greyed-out line is never lit: not by the mouse, not by the keyboard)
            let off = kind == MenuKind::Game && f.menu_disabled.contains(&id);
            let lit = !off && (over(rect) || (k == sel && f.menu_kbd && !any_hovered));
            let danger = id == "quit";
            let is_back = id == "back" && label == back_txt.as_str();
            // a thin line between the groups: the everyday lines apart from the rarer ones
            // (none in the other lists: "Back" there stands apart under them, without a line)
            let apart = k > start
                && match kind {
                MenuKind::Game => matches!(id, "save" | "admin" | "quit"),
                MenuKind::Lines => id == "free",
                _ => false,
            };
            if apart {
                let sy = (ry - 2.0 * s).round();
                scene.overlays.push((sep, [x + pad + tin, sy, right - tin, sy + 1.0]));
            }
            // (the light of the line eases in and out)
            let glow = self.easeq((7, id, k), if lit { 1.0 } else { 0.0 }, 1.0 / FADE_SECS);
            // (the tour chosen stays marked, whatever the mouse is over)
            let active = kind == MenuKind::Tours && k == sel && !is_back;
            let a_act = self.easeq((14, id, k), if active { 1.0 } else { 0.0 }, 1.0 / FADE_SECS);
            let bar = self.easeq((8, id, k), if lit || active { 1.0 } else { 0.0 }, 1.0 / BAR_SECS);
            if a_act > 0.0 {
                self.text.rounded(r, scene, rect, ROW_R * s, fade(SELECTED, a_act));
            }
            if glow > 0.0 {
                self.text.rounded(r, scene, rect, ROW_R * s, fade(if danger { LIT_DANGER } else { LIT }, glow));
            }
            if bar > 0.0 {
                // the accent bar at the left edge of the lit line, growing from its middle
                self.accent_bar(r, scene, rect, bar, danger, s);
            }
            let ink = if off {
                OFF_INK
            } else {
                if danger {
                    mix([232, 138, 128, 0], [255, 176, 166, 0], glow)
                } else {
                    mix(SOFT, WHITE, glow.max(a_act))
                }
            };
            let cy = (rect[1] + rect[3]) * 0.5;
            let lx = rect[0] + tin;
            let rx = rect[2] - tin;
            let mut done = false;
            match kind {
                // a line of the timetable: its number on a solid sign, like a line sign on a bus
                MenuKind::Lines => {
                    let parsed = label.strip_prefix(line_pre.as_str()).and_then(|rest| rest.rsplit_once("  (")).map(|(n, t)| (n, t.trim_end_matches(')')));
                    if let Some((name, info)) = parsed {
                        let bl = self.text.label(r, scene, name, (15.0 * s) as u32, ON_ACCENT);
                        let (bw, bh) = (sign_w, 28.0 * s);
                        let bx = lx;
                        let sign = mix(mix(ACCENT, [150, 104, 30, 255], 0.30), ACCENT_HOT, glow);
                        self.text.rounded(r, scene, [bx, cy - bh * 0.5, bx + bw, cy + bh * 0.5], 7.0 * s, sign);
                        let (lx0, ly0) = (bx + (bw - bl.w as f32) * 0.5, cy - bl.h as f32 * 0.5);
                        scene.overlays.push((bl.tex, [lx0, ly0, lx0 + bl.w as f32, ly0 + bl.h as f32]));
                        let tx = bx + bw + 14.0 * s;
                        let d = 22.0 * s;
                        self.text.rounded(r, scene, [rx - d, cy - d * 0.5, rx, cy + d * 0.5], d * 0.5, fade(ACCENT, 0.10 + 0.30 * glow));
                        let aw = self.text.width("›", (px + 2) as f32);
                        self.put(r, scene, "›", px + 2, mix(MUTED, ACCENT_HOT, glow), rx - d * 0.5 - aw * 0.5, cy - 1.0 * s);
                        let info = clip_to(&self.text, info, px as f32, rx - d - 12.0 * s - tx);
                        self.put(r, scene, &info, px, ink, tx, cy);
                        done = true;
                    }
                }
                MenuKind::List if code_w > 0.0 => {
                    if let Some((code, name)) = code_and_name(label) {
                        self.put_right(r, scene, code, px, mix(MUTED, ACCENT_HOT, glow), lx + code_w, cy);
                        let nx = lx + code_w + 12.0 * s;
                        let name = clip_to(&self.text, name, px as f32, rx - nx);
                        self.put(r, scene, &name, px, ink, nx, cy);
                        done = true;
                    }
                }
                // a tour: its name (the time is chosen beside the list)
                MenuKind::Tours => {
                    if let Some(rest) = label.strip_prefix(tour_pre.as_str()) {
                        let num = rest.split_once("  ").map(|(n, _)| n).unwrap_or(rest).trim();
                        let name = clip_to(&self.text, &format!("{tour_pre}{num}"), px as f32, rx - lx);
                        self.put(r, scene, &name, px, ink, lx, cy);
                        done = true;
                    }
                }
                _ => {}
            }
            if !done {
                let (text, more) = strip_more(label);
                let mut avail = rx - lx;
                if is_back {
                    let plain = matches!(kind, MenuKind::Lines | MenuKind::Tours);
                    if plain {
                        self.text.rounded(r, scene, rect, ROW_R * s, fade(LIT, 0.55));
                    }
                    let idle = if plain { SOFT } else { MUTED };
                    self.put(r, scene, &format!("‹  {text}"), px, if lit { WHITE } else { idle }, lx, cy);
                } else {
                    if off {
                        // (why the line cannot be chosen)
                        let cw = self.put_right(r, scene, "No active route", (12.0 * s) as u32, OFF_HINT, rx, cy);
                        avail -= cw + 10.0 * s;
                    }
                    if more {
                        // (the line opens another list)
                        let cw = self.put_right(r, scene, "›", px + 6, if lit { WHITE } else { MUTED }, rx, cy);
                        avail -= cw + 10.0 * s;
                    } else if id == "resume" && keys {
                        // ("Resume" shows the key that does the same)
                        let left = self.chip(r, scene, "Esc", (12.0 * s) as u32, WHITE, [52, 52, 52, 255], true, rx, cy, s);
                        avail = left - 10.0 * s - lx;
                    }
                    let text = clip_to(&self.text, text, px as f32, avail);
                    self.put(r, scene, &text, px, ink, lx, cy);
                }
            }
            self.menu_rects.push(rect);
        }
        if back_footer {
            // ("Back" pinned to the bottom of the list: the rects of the lines
            // out of view are empty ones, so that a rect stays at the index of its line)
            let bk = items.len() - 1;
            let fr = [x + pad, y + h - pad - 36.0 * s, right, y + h - pad];
            let lit = over(fr) || (sel == bk && f.menu_kbd && !any_hovered);
            let glow = self.easeq((7, "back", bk), if lit { 1.0 } else { 0.0 }, 1.0 / FADE_SECS);
            self.text.rounded(r, scene, fr, ROW_R * s, fade(LIT, 0.55 + 0.45 * glow));
            if glow > 0.0 {
                self.accent_bar(r, scene, fr, glow, false, s);
            }
            let (text, _) = strip_more(back_txt.as_str());
            self.put(r, scene, &format!("‹  {text}"), px, mix(SOFT, WHITE, glow), fr[0] + tin, (fr[1] + fr[3]) * 0.5);
            while self.menu_rects.len() < bk - start {
                self.menu_rects.push([-1.0e9; 4]);
            }
            self.menu_rects.push(fr);
        }
        // the timetable of the chosen line or tour, beside the list
        if let Some(p) = preview {
            self.draw_menu_preview(r, scene, f, p, sep, [x, y, w, h], list_r, header_h, s);
        }
        self.menu_overlay_range = overlay_start..scene.overlays.len();
    }

    /// The timetable of the chosen line or tour in its pane beside the menu's list (card
    /// `[x, y, w, h]`, the list ending at `list_r`): the trip's times between arrows, then the
    /// stops to start from with the button that starts the trip, or the tour's trips.
    #[allow(clippy::too_many_arguments)]
    fn draw_menu_preview(&mut self, r: &Renderer, scene: &mut Scene, f: &Frame, p: &Preview, sep: TextureId, card: [f32; 4], list_r: f32, header_h: f32, s: f32) {
        let [x, y, w, h] = card;
        let pad = PAD * s;
        let tin = TEXT_IN * s;
        let over = |rect: [f32; 4]| f.cursor.0 >= rect[0] && f.cursor.0 <= rect[2] && f.cursor.1 >= rect[1] && f.cursor.1 <= rect[3];
        let (px0, py0) = (list_r + 4.0 * s, y + header_h);
        let (px1, py1) = (x + w - pad, y + h - pad);
        self.text.rounded(r, scene, [px0 - 1.0, py0 - 1.0, px1 + 1.0, py1 + 1.0], CARD_R * s + 1.0, BORDER);
        self.text.rounded(r, scene, [px0, py0, px1, py1], CARD_R * s, PANEL_ALT);
        let pad = tin;
        let inner = px1 - px0 - pad * 2.0;
        let mut cy = py0 + 28.0 * s;
        let head = clip_to(&self.text, &p.title, 16.0 * s, inner);
        self.put(r, scene, &head, (16.0 * s) as u32, WHITE, px0 + pad, cy);
        cy += 22.0 * s;
        let meta = clip_to(&self.text, &p.meta, 11.0 * s, inner);
        self.put(r, scene, &meta, (11.0 * s) as u32, MUTED, px0 + pad, cy);
        cy += 18.0 * s;
        scene.overlays.push((sep, [px0 + pad, cy.round(), px1 - pad, cy.round() + 1.0]));
        let rpx = (13.0 * s) as u32;
        let lh = 24.0 * s;
        let mut top = cy + 10.0 * s;
        // the tour, and the time of the trip, each between arrows that step through them
        let nav_rows: Vec<(&String, f32, usize)> = p.time.iter().map(|t| (t, 20.0f32, 0usize)).collect();
        for (time, fs, base) in nav_rows {
            let bh = 30.0 * s;
            let bw = 46.0 * s;
            let by = top;
            let right = px1 - pad;
            let left = px0 + pad;
            for j in 0..2usize {
                let rect = if j == 0 { [left, by, left + bw, by + bh] } else { [right - bw, by, right, by + bh] };
                let a = self.easeq((14, "time", j + base), if over(rect) { 1.0 } else { 0.0 }, 1.0 / FADE_SECS);
                self.text.rounded(r, scene, rect, ROW_R * s, mix([232, 160, 48, 60], ACCENT, a * 0.7));
                let col = mix(ACCENT_HOT, [18, 14, 8, 255], a);
                let (cx, cy) = ((rect[0] + rect[2]) * 0.5, (rect[1] + rect[3]) * 0.5);
                // (a real arrow: a shaft and a head of stacked strips)
                let dir = if j == 0 { -1.0 } else { 1.0 };
                let (half, head_w, head_h) = (9.0 * s, 7.0 * s, 7.0 * s);
                let t = (2.0 * s).max(2.0);
                self.text.rounded(r, scene, [cx - half, cy - t * 0.5, cx + half, cy + t * 0.5], 0.0, col);
                let tip = cx + dir * half;
                let strips = 7;
                for k in 0..strips {
                    // (strip k, from the head's base towards the tip, narrowing)
                    let bx = tip - dir * head_w * (1.0 - k as f32 / strips as f32);
                    let ex = tip - dir * head_w * (1.0 - (k as f32 + 1.0) / strips as f32);
                    let h = head_h * (1.0 - (k as f32 + 0.5) / strips as f32);
                    self.text.rounded(r, scene, [bx.min(ex), cy - h, bx.max(ex), cy + h], 0.0, col);
                }
                self.menu_time.push(rect);
            }
            let mid_l = left + bw + 10.0 * s;
            let mid_r = right - bw - 10.0 * s;
            let time = clip_to(&self.text, time, fs * s, mid_r - mid_l);
            let tw = self.text.width(&time, fs * s);
            self.put(r, scene, &time, (fs * s) as u32, if base == 0 { AMBER } else { WHITE }, mid_l + (mid_r - mid_l - tw) * 0.5, by + bh * 0.5);
            top = by + bh + 8.0 * s;
        }
        let n = p.rows.len();
        if let (Some(chosen), Some(button)) = (p.chosen, p.button.as_ref()) {
            // the stops to start from: the one chosen marked, a click chooses another
            let go_h = 34.0 * s;
            let go = [px0 + pad, py1 - 12.0 * s - go_h, px1 - pad, py1 - 12.0 * s];
            let fit = (((go[1] - 10.0 * s) - top) / lh).floor().max(1.0) as usize;
            let first = match f.pane_first {
                Some(p) if n > fit => p.min(n - fit),
                _ if n > fit => chosen.saturating_sub(fit / 2).min(n - fit),
                _ => 0,
            };
            self.menu_pane_start = first;
            self.menu_pane_box = Some([px0, py0, px1, py1]);
            // (a long list of stops: a thin scroll bar at the pane's edge)
            if n > fit {
                let hot = over([px0, py0, px1, py1]);
                let track = [px1 - 7.0 * s, top, px1 - 3.0 * s, go[1] - 10.0 * s];
                let thumb = self.thumb(r, scene, track, first, fit, n, hot, s);
                self.menu_pane_scroll = Some((track, [thumb[0] - 6.0 * s, thumb[1], thumb[2] + 3.0 * s, thumb[3]], n, fit));
            }
            let time_w = p.rows.iter().skip(first).take(fit).map(|row| self.text.width(&row.1, rpx as f32)).fold(0.0f32, f32::max);
            for (i, (what, when)) in p.rows.iter().enumerate().skip(first).take(fit) {
                let ry = top + lh * (i - first) as f32 + lh * 0.5;
                let rect = [px0 + 8.0 * s, ry - lh * 0.5, px1 - 8.0 * s, ry + lh * 0.5];
                let on = i == chosen;
                let hov = over(rect) && !on;
                let a_on = self.easeq((11, "stop", i), if on { 1.0 } else { 0.0 }, 1.0 / FADE_SECS);
                let a_hov = self.easeq((12, "stop", i), if hov { 1.0 } else { 0.0 }, 1.0 / FADE_SECS);
                if a_hov > 0.0 {
                    self.text.rounded(r, scene, rect, ROW_R * s, fade(LIT, a_hov));
                }
                if a_on > 0.0 {
                    self.text.rounded(r, scene, rect, ROW_R * s, fade(SELECTED, a_on));
                    self.text.rounded(r, scene, [rect[0], rect[1] + 7.0 * s, rect[0] + 2.0 * s, rect[3] - 7.0 * s], 1.0 * s, fade(ACCENT, a_on));
                }
                self.put_right(r, scene, when, rpx, AMBER, px1 - pad, ry);
                let what = clip_to(&self.text, what, rpx as f32, inner - time_w - 14.0 * s);
                self.put(r, scene, &what, rpx, mix(SOFT, WHITE, a_on), px0 + pad, ry);
                self.menu_pane.push(rect);
            }
            // the button that starts the trip
            let a_go = self.easeq((13, "go", 0), if over(go) { 1.0 } else { 0.0 }, 1.0 / FADE_SECS);
            self.text.rounded(r, scene, go, ROW_R * s, mix(ACCENT, ACCENT_HOT, a_go));
            let l = self.text.label(r, scene, button, (14.0 * s) as u32 | BOLD, ON_ACCENT);
            let (gx, gy) = (go[0] + (go[2] - go[0] - l.w as f32) * 0.5, (go[1] + go[3]) * 0.5 - l.h as f32 * 0.5);
            scene.overlays.push((l.tex, [gx, gy, gx + l.w as f32, gy + l.h as f32]));
            self.menu_pane_go = Some(go);
        } else {
            let fit = ((py1 - 12.0 * s - top) / lh).floor().max(1.0) as usize;
            // (a list too long for the pane ends in how many more there are)
            let shown = if n > fit { fit.saturating_sub(1) } else { n };
            let time_w = p.rows.iter().take(shown).map(|row| self.text.width(&row.1, rpx as f32)).fold(0.0f32, f32::max);
            let tp = format!("{} ", omsi_ui::tr("Tour"));
            // (the tour numbers in tiles of one width, the destination after them)
            let tile_w = p.rows.iter().take(shown).filter_map(|row| row.0.strip_prefix(tp.as_str()).and_then(|x| x.split_once("  ›  ").map(|(n, _)| n).or(Some(x)))).map(|n| self.text.width(n.trim(), rpx as f32) + 16.0 * s).fold(30.0 * s, f32::max);
            for (i, (what, when)) in p.rows.iter().take(shown).enumerate() {
                let ry = top + lh * i as f32 + lh * 0.5;
                self.put_right(r, scene, when, rpx, AMBER, px1 - pad, ry);
                if let Some(rest) = what.strip_prefix(tp.as_str()) {
                    let (num, dest) = rest.split_once("  ›  ").unwrap_or((rest, ""));
                    let th = lh - 6.0 * s;
                    self.text.rounded(r, scene, [px0 + pad, ry - th * 0.5, px0 + pad + tile_w, ry + th * 0.5], 5.0 * s, ACCENT_SOFT);
                    let nw = self.text.width(num.trim(), rpx as f32);
                    self.put(r, scene, num.trim(), rpx, txt(ACCENT), px0 + pad + (tile_w - nw) * 0.5, ry);
                    let dx = px0 + pad + tile_w + 10.0 * s;
                    let dest = clip_to(&self.text, dest, rpx as f32, px1 - pad - time_w - 14.0 * s - dx);
                    self.put(r, scene, &dest, rpx, SOFT, dx, ry);
                } else {
                    let what = clip_to(&self.text, what, rpx as f32, inner - time_w - 14.0 * s);
                    self.put(r, scene, &what, rpx, SOFT, px0 + pad, ry);
                }
            }
            if n > shown {
                let ry = top + lh * shown as f32 + lh * 0.5;
                self.put(r, scene, &omsi_ui::tr("+{} more").replacen("{}", &(n - shown).to_string(), 1), (12.0 * s) as u32, MUTED, px0 + pad, ry);
            }
        }
    }
}

/// The launcher's rail and its separator line (`launcher/theme.rs` `RAIL`, `EDGE`).
pub(super) const RAIL: [u8; 4] = [18, 18, 18, 255];
pub(super) const EDGE: [u8; 4] = [255, 255, 255, 15];

/// The Material icon of a line of the game menu (the launcher's icon set).
pub(super) fn game_menu_icon(id: &str) -> &'static str {
    match id {
        "resume" => "play_arrow",
        "tobus" => "directions_bus",
        "options" => "settings",
        "controls" => "sports_esports",
        "camera" => "videocam",
        "vehicle" => "airport_shuttle",
        "world" => "partly_cloudy_day",
        "map" => "map",
        "duty" => "departure_board",
        "skipstop" => "near_me",
        "endduty" => "sports_score",
        "save" => "save",
        "saveslot" => "add",
        "load" => "history",
        "shot" => "photo_camera",
        "admin" => "key",
        "quit" => "logout",
        _ => "chevron_right",
    }
}

impl Ui {
    /// The pause menu as the launcher's rail: a full-height panel on the left over the
    /// dimmed picture - the game's name and version on top, "Resume" as the launcher's
    /// amber primary button, the other lines as its pages (an icon, the name, the chosen one
    /// on a lighter grey with the accent bar), "End the session" in the danger colour at the
    /// end. A menu longer than the screen scrolls, as the card did.
    pub(super) fn draw_game_menu(&mut self, r: &Renderer, scene: &mut Scene, f: &Frame, sel: usize, items: &[(&str, &str)]) {
        let s = menu_scale(f);
        let dim = self.text.plate(r, scene, 6);
        scene.overlays.push((dim, [0.0, 0.0, f.width, f.height]));
        let keys = !crate::platform::touch_controls();
        // (a phone draws to its edges, round the camera's hole in the screen: on the rail's
        // side in one of the two landscape turns - its contents keep clear of it)
        let cut = if crate::platform::MOBILE || omsi_cfg::flags::OMSI_MOBILE.is_set() { 18.0 * s } else { 0.0 };
        let rail_w = (264.0 * s + cut).min(f.width * 0.86).round();
        // the rail: flat, a hairline at its edge, a soft shadow over the picture
        self.text.shadow(r, scene, [-40.0 * s, -40.0 * s, rail_w, f.height + 40.0 * s], 0.0, 30.0 * s, 0.0, 120);
        self.text.rounded(r, scene, [0.0, 0.0, rail_w, f.height], 0.0, RAIL);
        self.text.rounded(r, scene, [rail_w - 1.0, 0.0, rail_w, f.height], 0.0, EDGE);
        // the game's name and version, as at the top of the launcher's rail
        let bx = 24.0 * s + cut;
        let name_w = self.put(r, scene, "openOMSI", (20.0 * s) as u32 | BOLD, WHITE, bx, 34.0 * s);
        self.put(r, scene, crate::startup::VERSION, (12.0 * s) as u32, MUTED, bx, 54.0 * s);
        // what state the game is in, a small amber tag beside the name
        let tag = if f.paused { "PAUSED" } else { "MENU" };
        let tl = self.text.label(r, scene, tag, (10.0 * s) as u32 | BOLD, txt(ACCENT));
        let (tw, th) = (tl.w as f32 + 12.0 * s, tl.h as f32 + 4.0 * s);
        let tx = (bx + name_w + 10.0 * s).min(rail_w - tw - 12.0 * s);
        let ty = 34.0 * s - th * 0.5;
        self.text.rounded(r, scene, [tx, ty, tx + tw, ty + th], 4.0 * s, ACCENT_SOFT);
        scene.overlays.push((tl.tex, [tx + 6.0 * s, ty + 2.0 * s, tx + 6.0 * s + tl.w as f32, ty + 2.0 * s + tl.h as f32]));
        // the lines: as many as fit at the launcher's 42 px pitch (down to 32), the rest
        // scrolled to
        let n = items.len();
        let top = 80.0 * s;
        let foot = if keys { 40.0 * s } else { 12.0 * s };
        let room = (f.height - top - foot).max(32.0 * s);
        let pitch = (42.0 * s).min(room / n.max(1) as f32).max(32.0 * s);
        let rows = ((room / pitch).floor() as usize).clamp(1, n.max(1));
        let start = match (n > rows, f.menu_top) {
            (false, _) => 0,
            (true, Some(t)) => (t.max(0.0).round() as usize).min(n - rows),
            (true, None) => sel.min(n.saturating_sub(1)).saturating_sub(rows / 2).min(n - rows),
        };
        self.menu_start = start;
        self.menu_rows = rows;
        self.menu_row_h = pitch;
        let scrolls = n > rows;
        let x0 = 12.0 * s + cut;
        let x1 = rail_w - if scrolls { 20.0 * s } else { 12.0 * s };
        if scrolls {
            let track = [rail_w - 10.0 * s, top, rail_w - 6.0 * s, top + pitch * rows as f32 - 4.0 * s];
            self.menu_scroll_track = Some(track);
            let hot = over_rect(f.cursor, [0.0, 0.0, rail_w, f.height]);
            let thumb = self.thumb(r, scene, track, start, rows, n, hot, s);
            self.menu_scroll_thumb = Some([thumb[0] - 6.0 * s, thumb[1], thumb[2] + 6.0 * s, thumb[3]]);
        }
        let any_hovered = over_rect(f.cursor, [0.0, top, rail_w, top + pitch * rows as f32]);
        let px = (13.5 * s).round() as u32;
        let icon_px = (18.0 * s).round() as u32;
        for (k, &(id, label)) in items.iter().enumerate().skip(start).take(rows) {
            let ry = top + pitch * (k - start) as f32;
            let rect = [x0, ry, x1, ry + pitch - 4.0 * s];
            let off = f.menu_disabled.contains(&id);
            let hov = !off && over_rect(f.cursor, rect);
            let chosen = !off && k == sel && f.menu_kbd && !any_hovered;
            let lit = hov || chosen;
            let danger = id == "quit";
            let primary = id == "resume";
            // a hairline between the groups: the everyday lines, saving, the way out
            if k > start && matches!(id, "save" | "admin" | "quit") {
                let sy = (ry - 2.0 * s).round();
                self.text.rounded(r, scene, [x0 + 8.0 * s, sy, x1 - 8.0 * s, sy + 1.0], 0.0, EDGE);
            }
            let glow = self.easeq((7, id, k), if lit { 1.0 } else { 0.0 }, 1.0 / FADE_SECS);
            let cy = (rect[1] + rect[3]) * 0.5;
            let (ink, icon_ink) = if primary {
                // the launcher's primary button: amber, dark bold text, lighter under the mouse
                self.text.rounded(r, scene, rect, 6.0 * s, mix(ACCENT, ACCENT_HOT, glow));
                ([18, 14, 8, 0], [18, 14, 8, 255])
            } else {
                if glow > 0.0 {
                    let fill = if danger { LIT_DANGER } else if chosen { SELECTED } else { LIT };
                    self.text.rounded(r, scene, rect, 6.0 * s, fade(fill, glow));
                    self.accent_bar(r, scene, rect, glow, danger, s);
                }
                let c = if off {
                    OFF_INK
                } else if danger {
                    mix([222, 110, 100, 0], [255, 176, 166, 0], glow)
                } else {
                    mix(MUTED, WHITE, glow)
                };
                (c, [c[0], c[1], c[2], 255])
            };
            if let Some(t) = self.text.icon(r, scene, game_menu_icon(id), icon_px, icon_ink) {
                let (ix, iy) = ((rect[0] + 20.0 * s - icon_px as f32 * 0.5).round(), (cy - icon_px as f32 * 0.5).round());
                scene.overlays.push((t, [ix, iy, ix + icon_px as f32, iy + icon_px as f32]));
            }
            let (text, more) = strip_more(label);
            let lx = rect[0] + 40.0 * s;
            let mut right = rect[2] - 12.0 * s;
            if more {
                if let Some(t) = self.text.icon(r, scene, "chevron_right", icon_px, fade(icon_ink, if off { 1.0 } else { 0.55 + 0.45 * glow })) {
                    let (ix, iy) = ((right - icon_px as f32 + 4.0 * s).round(), (cy - icon_px as f32 * 0.5).round());
                    scene.overlays.push((t, [ix, iy, ix + icon_px as f32, iy + icon_px as f32]));
                }
                right -= icon_px as f32;
            } else if primary && keys {
                // ("Resume" shows the key that does the same)
                let left = self.chip(r, scene, "Esc", (11.0 * s) as u32, [18, 14, 8, 0], [255, 214, 150, 255], false, right + 4.0 * s, cy, s);
                right = left - 6.0 * s;
            } else if off {
                let cw = self.put_right(r, scene, "No active route", (11.0 * s) as u32, OFF_HINT, right, cy);
                right -= cw + 8.0 * s;
            }
            let size = if primary { (14.0 * s).round() as u32 | BOLD } else { px };
            let text = clip_to(&self.text, text, (size & !BOLD) as f32, right - lx);
            self.put(r, scene, &text, size, ink, lx, cy);
            self.menu_rects.push(rect);
        }
        // the keys, quietly at the bottom, where there is a keyboard
        if keys {
            let hint = omsi_ui::tr("Esc resumes  |  P pauses").into_owned();
            let hint = clip_to(&self.text, &hint, 11.0 * s, rail_w - bx * 2.0);
            self.put(r, scene, &hint, (11.0 * s) as u32, OFF_INK, bx, f.height - 20.0 * s);
        }
    }
}

impl Ui {
    /// A settings window (options, vehicle, world): a sidebar of pages on the left, the rows
    /// of the page shown on the right. A row is `name\u{1f}kind\u{1f}value\u{1f}description\u{1f}fraction`:
    /// `s` a switch (value "on"/"off"), `v` a slider (value text, fraction of the way),
    /// `c` a stepper (value between arrows), `o` opens a list, `a` a button (its text is the
    /// value), `i` information.
    pub(super) fn draw_settings(&mut self, r: &Renderer, scene: &mut Scene, f: &Frame, sel: usize, items: &[(&str, &str)]) {
        let s = menu_scale(f);
        // (a held seat slider: the window is drawn as always - its places stay where they
        // are for the mouse - and then all but that line is taken away again, see `menu_peek`)
        let begin = scene.overlays.len();
        let mut peek: Option<(std::ops::Range<usize>, [f32; 4])> = None;
        let dim = self.text.plate(r, scene, 6);
        let sep = self.text.plate(r, scene, 9);
        scene.overlays.push((dim, [0.0, 0.0, f.width, f.height]));
        let none: Vec<String> = Vec::new();
        let (titles, active) = match f.menu_tabs.as_ref() {
            Some((t, a)) => (t, *a),
            None => (&none, 0),
        };
        // a window of a fixed size (as far as the screen allows): the pages differ in how many
        // rows they have, the window does not - a long page scrolls inside it
        let want_side = if titles.is_empty() { 0.0 } else { 200.0 * s };
        let w = (want_side + 680.0 * s).min(f.width - 24.0 * s).max(260.0 * s);
        let side_w = want_side.min(w * 0.38);
        let header_h = 72.0 * s;
        let pad = PAD * s;
        let tin = TEXT_IN * s;
        let fixed_h = (if f.vr { 440.0 } else { 600.0 }) * s;
        let h = fixed_h.min(f.height * (if f.vr { 0.70 } else { 0.94 })).max(220.0 * s);
        let room = h - header_h - pad;
        let row_h = ((if f.vr { 54.0 } else { 62.0 }) * s).min(room).max(30.0 * s);
        let rows = ((room / row_h).floor() as usize).clamp(1, items.len().max(1));
        let start = match (items.len() > rows, f.menu_top) {
            (false, _) => 0,
            (true, Some(top)) => (top.max(0.0).round() as usize).min(items.len() - rows),
            (true, None) => sel.saturating_sub(rows / 2).min(items.len() - rows),
        };
        self.menu_start = start;
        self.menu_rows = rows;
        self.menu_row_h = row_h;
        let x = ((f.width - w) * 0.5).round();
        let y = ((f.height - h) * 0.5).round();
        let radius = CARD_R * s;
        self.text.shadow(r, scene, [x, y, x + w, y + h], radius, 28.0 * s, 10.0 * s, 110);
        self.text.rounded(r, scene, [x - 1.0, y - 1.0, x + w + 1.0, y + h + 1.0], radius + 1.0, BORDER);
        self.text.rounded(r, scene, [x, y, x + w, y + h], radius, PANEL);
        // the header: the game's name small, the title large
        let (title, sub): (String, String) = match f.menu_head.as_ref() {
            Some((t, u)) => (t.clone(), u.clone()),
            None => ("Options".to_string(), String::new()),
        };
        self.menu_header(r, scene, x, y, w, header_h, &title, &sub, s);
        let over = |rect: [f32; 4]| f.cursor.0 >= rect[0] && f.cursor.0 <= rect[2] && f.cursor.1 >= rect[1] && f.cursor.1 <= rect[3];
        // the sidebar: the pages, and the way back at its foot
        if side_w > 0.0 {
            let (sx0, sx1) = (x + pad, x + side_w);
            self.text.rounded(r, scene, [sx0 - 1.0, y + header_h - 1.0, sx1 + 1.0, y + h - pad + 1.0], CARD_R * s + 1.0, BORDER);
            self.text.rounded(r, scene, [sx0, y + header_h, sx1, y + h - pad], CARD_R * s, PANEL_ALT);
            let inset = 6.0 * s;
            let bottom = y + h - pad - inset;
            let pages_top = y + header_h + inset;
            let step = if f.vr {
                vr_settings_sidebar_step(bottom - 38.0 * s - inset - pages_top, titles.len(), s)
            } else {
                42.0 * s
            };
            let page_h = (step - 4.0 * s).max(1.0);
            let spx = (15.0 * s).min(page_h * 0.6).max(1.0) as u32;
            for (i, title) in titles.iter().enumerate() {
                let top = pages_top + i as f32 * step;
                let rect = [sx0 + inset, top, sx1 - inset, top + page_h];
                let on = i == active;
                let hov = over(rect) && !on;
                // (the page chosen and the page under the mouse fade in and out)
                let a_on = self.easeq((1, title.as_str(), 0), if on { 1.0 } else { 0.0 }, 1.0 / FADE_SECS);
                let a_hov = self.easeq((2, title.as_str(), 0), if hov { 1.0 } else { 0.0 }, 1.0 / FADE_SECS);
                let a_bar = self.easeq((9, title.as_str(), 0), if on { 1.0 } else { 0.0 }, 1.0 / BAR_SECS);
                if a_hov > 0.0 {
                    self.text.rounded(r, scene, rect, ROW_R * s, fade(LIT, a_hov));
                }
                if a_on > 0.0 {
                    self.text.rounded(r, scene, rect, ROW_R * s, fade(SELECTED, a_on));
                }
                if a_bar > 0.0 {
                    self.accent_bar(r, scene, rect, a_bar, false, s);
                }
                let ink = mix(mix(MUTED, SOFT, a_hov), WHITE, a_on);
                let text = clip_to(&self.text, title, spx as f32, rect[2] - rect[0] - tin * 2.0);
                self.put(r, scene, &text, spx, ink, rect[0] + tin, (rect[1] + rect[3]) * 0.5);
                self.menu_side.push(rect);
            }
            let rect = [sx0 + inset, bottom - 38.0 * s, sx1 - inset, bottom];
            let a_back = self.easeq((3, "back", 0), if over(rect) { 1.0 } else { 0.0 }, 1.0 / FADE_SECS);
            if a_back > 0.0 {
                self.text.rounded(r, scene, rect, ROW_R * s, fade(LIT, a_back));
            }
            let back = format!("‹  {}", omsi_ui::tr("Back"));
            self.put(r, scene, &back, spx, mix(MUTED, WHITE, a_back), rect[0] + tin, (rect[1] + rect[3]) * 0.5);
            self.menu_side.push(rect);
        }
        // the scroll bar of a long page
        let scrolls = items.len() > rows;
        let cx0 = x + side_w + if side_w > 0.0 { 8.0 * s } else { pad };
        let cx1 = x + w - pad - if scrolls { 8.0 * s } else { 0.0 };
        if scrolls {
            let top = y + header_h;
            let track = [x + w - 12.0 * s, top, x + w - 8.0 * s, top + row_h * rows as f32 - 4.0 * s];
            self.menu_scroll_track = Some(track);
            let hot = over_rect(f.cursor, [x + side_w, y + header_h, x + w, y + h]);
            let thumb = self.thumb(r, scene, track, start, rows, items.len(), hot, s);
            self.menu_scroll_thumb = Some([thumb[0] - 6.0 * s, thumb[1], thumb[2] + 6.0 * s, thumb[3]]);
        }
        // the rows
        let any_hovered = over([x + side_w, y + header_h, x + w, y + h]);
        let px = ((15.0 * s).min(row_h * 0.34)) as u32;
        // (the light of the row above: the line between two rows is hidden when either is lit)
        let mut prev_a = 0.0f32;
        for (k, &(id, label)) in items.iter().enumerate().skip(start).take(rows) {
            let ry = y + header_h + row_h * (k - start) as f32;
            let rect = [cx0, ry, cx1, ry + row_h - 4.0 * s];
            let row_from = scene.overlays.len();
            // (lit by the mouse over it; by the keyboard's choice only when the keyboard chose)
            let lit = f.dropdown.is_none() && (over(rect) || (k == sel && f.menu_kbd && !any_hovered));
            // the light of the row eases in and out
            let a = self.easeq((4, id, k), if lit { 1.0 } else { 0.0 }, 1.0 / FADE_SECS);
            let a_bar = self.easeq((10, id, k), if lit { 1.0 } else { 0.0 }, 1.0 / BAR_SECS);
            if a > 0.0 {
                self.text.rounded(r, scene, rect, ROW_R * s, fade(LIT, a));
            }
            if a_bar > 0.0 {
                self.accent_bar(r, scene, rect, a_bar, false, s);
            }
            if k > start && a.max(prev_a) < 0.5 {
                let sy = (rect[1] - 2.0 * s).round();
                scene.overlays.push((sep, [rect[0] + tin, sy, rect[2] - tin, sy + 1.0]));
            }
            prev_a = a;
            let ink = mix(SOFT, WHITE, a);
            let cy = (rect[1] + rect[3]) * 0.5;
            let nx = rect[0] + tin;
            let rx = rect[2] - tin;
            let mut parts = label.split('\u{1f}');
            let name = parts.next().unwrap_or("");
            let kind = parts.next().unwrap_or("a");
            let value = parts.next().unwrap_or("");
            let desc = parts.next().unwrap_or("");
            let frac: Option<f32> = parts.next().and_then(|p| p.parse().ok());
            let mut ctl: Option<[f32; 4]> = None;
            let left: f32 = match kind {
                // a switch
                "s" => {
                    let on = value == "on";
                    // (the launcher's switch - 34 x 18, a white knob 6 px smaller than the
                    // track - a tenth larger for the menu's larger text)
                    let (tw, th) = (38.0 * s, 20.0 * s);
                    let tx = rx - tw;
                    let track = [tx, cy - th * 0.5, tx + tw, cy + th * 0.5];
                    // (the knob slides over and the track changes its colour)
                    let t = self.ease((5, id, k), if on { 1.0 } else { 0.0 }, 1.0 / FADE_SECS);
                    let tq = quant(t);
                    self.text.rounded(r, scene, track, th * 0.5, mix(TRACK_OFF, ACCENT, tq));
                    let kn = th - 6.0 * s;
                    let kx = tx + 3.0 * s + (tw - kn - 6.0 * s) * t;
                    self.text.rounded(r, scene, [kx, cy - kn * 0.5, kx + kn, cy + kn * 0.5], kn * 0.5, KNOB);
                    tx
                }
                // a slider: the track with its knob, the value right of it
                "v" => {
                    let vw = 64.0 * s;
                    self.put_right(r, scene, value, (14.0 * s) as u32, mix(SOFT, WHITE, a), rx, cy);
                    let tw = (200.0 * s).min((rx - nx) * 0.45);
                    let x1 = rx - vw - 10.0 * s;
                    let x0 = x1 - tw;
                    let th = 4.0 * s;
                    self.text.rounded(r, scene, [x0, cy - th * 0.5, x1, cy + th * 0.5], th * 0.5, SLIDER_TRACK);
                    // (the knob glides to its place and grows a little under the mouse)
                    let fr = self.ease((6, id, k), frac.unwrap_or(0.0).clamp(0.0, 1.0), 2.0 / FADE_SECS);
                    let fx = x0 + tw * fr;
                    if fx - x0 >= 1.0 {
                        self.text.rounded(r, scene, [x0, cy - th * 0.5, fx, cy + th * 0.5], th * 0.5, ACCENT);
                    }
                    let kn = (13.0 + 2.0 * a) * s;
                    self.text.rounded(r, scene, [fx - kn * 0.5, cy - kn * 0.5, fx + kn * 0.5, cy + kn * 0.5], kn * 0.5, KNOB);
                    ctl = Some([x0, rect[1], x1, rect[3]]);
                    x0
                }
                // a stepper: the value between two arrows, the left half steps back
                "c" => {
                    let text = format!("‹  {value}  ›");
                    let l = self.chip(r, scene, &text, (13.0 * s) as u32, mix(SOFT, WHITE, a), mix(CHIP, SELECTED, a), false, rx, cy, s);
                    ctl = Some([l, rect[1], rx, rect[3]]);
                    l
                }
                // opens another list
                "o" => {
                    let cw = self.put_right(r, scene, "›", px + 6, mix(MUTED, WHITE, a), rx, cy);
                    if value.is_empty() {
                        rx - cw
                    } else {
                        // (the value now, as the stepper showed it)
                        let vw = self.put_right(r, scene, value, (14.0 * s) as u32, SOFT, rx - cw - 8.0 * s, cy);
                        rx - cw - 8.0 * s - vw
                    }
                }
                "i" => {
                    let cw = self.put_right(r, scene, value, (14.0 * s) as u32, SOFT, rx, cy);
                    rx - cw
                }
                // a value being typed: lit in the accent
                "E" => self.chip(r, scene, value, (13.0 * s) as u32, AMBER, ACCENT_SOFT, false, rx, cy, s),
                // a button
                _ => {
                    if value.is_empty() {
                        rx
                    } else {
                        self.chip(r, scene, value, (13.0 * s) as u32, mix(SOFT, AMBER, a), mix(CHIP, ACCENT_SOFT, a), false, rx, cy, s)
                    }
                }
            };
            let avail = left - 16.0 * s - nx;
            if desc.is_empty() {
                let n = clip_to(&self.text, name, px as f32, avail);
                self.put(r, scene, &n, px, ink, nx, cy);
            } else {
                let n = clip_to(&self.text, name, px as f32, avail);
                self.put(r, scene, &n, px, ink, nx, cy - 9.0 * s);
                let dpx = (12.0 * s) as u32;
                let d = clip_to(&self.text, desc, dpx as f32, avail);
                self.put(r, scene, &d, dpx, MUTED, nx, cy + 10.0 * s);
            }
            if f.menu_peek == Some(k) {
                peek = Some((row_from..scene.overlays.len(), rect));
            }
            self.menu_rects.push(rect);
            self.menu_ctl.push(ctl);
        }
        if let Some((row, rect)) = peek {
            let line: Vec<_> = scene.overlays.drain(row).collect();
            scene.overlays.truncate(begin);
            let m = 6.0 * s;
            let card = [rect[0] - m, rect[1] - m, rect[2] + m, rect[3] + m];
            self.text.shadow(r, scene, card, CARD_R * s, 18.0 * s, 6.0 * s, 110);
            self.text.rounded(r, scene, [card[0] - 1.0, card[1] - 1.0, card[2] + 1.0, card[3] + 1.0], CARD_R * s + 1.0, BORDER);
            self.text.rounded(r, scene, card, CARD_R * s, PANEL);
            scene.overlays.extend(line);
            return;
        }
        // a drop-down over a row (the weather preset, the clouds): the entries under the
        // row's value as a select's in a page - over it where there is no room under
        if let Some(dd) = f.dropdown.as_ref().filter(|d| d.row >= start && d.row < start + rows && !d.items.is_empty()) {
            let ry = y + header_h + row_h * (dd.row - start) as f32;
            let row_b = ry + row_h - 4.0 * s;
            let item_h = (36.0 * s).min(row_h);
            let inner = 4.0 * s;
            let below = y + h - pad - row_b - 4.0 * s;
            let above = ry - (y + header_h) - 4.0 * s;
            let want = dd.items.len().min(8);
            let fits = |room: f32| (((room - 2.0 * inner) / item_h).floor().max(0.0) as usize).min(want);
            let down = fits(below) >= want || fits(below) >= fits(above);
            let n_vis = (if down { fits(below) } else { fits(above) }).max(1);
            let ph = n_vis as f32 * item_h + 2.0 * inner;
            let pw = (300.0 * s).min(cx1 - cx0);
            let (px1, py0) = (cx1 - 6.0 * s, if down { row_b + 4.0 * s } else { ry - 4.0 * s - ph });
            let px0 = px1 - pw;
            let panel = [px0, py0, px1, py0 + ph];
            let top = dd.top.min(dd.items.len() - n_vis.min(dd.items.len()));
            self.dd_top = top;
            self.dd_rows = n_vis;
            let rad = (CARD_R * s).min(10.0 * s);
            self.text.shadow(r, scene, panel, rad, 18.0 * s, 6.0 * s, 150);
            // (the launcher's popup: the field's grey, a hairline round it)
            self.text.rounded(r, scene, [panel[0] - 1.0, panel[1] - 1.0, panel[2] + 1.0, panel[3] + 1.0], rad + 1.0, BORDER);
            self.text.rounded(r, scene, panel, rad, PANEL_ALT);
            let more = dd.items.len() > n_vis;
            let dpx = (14.0 * s) as u32;
            let tin = TEXT_IN * s;
            for i in 0..n_vis {
                let idx = top + i;
                let rect = [px0 + inner, py0 + inner + i as f32 * item_h, px1 - inner - if more { 8.0 * s } else { 0.0 }, py0 + inner + (i + 1) as f32 * item_h];
                let cur = dd.current == Some(idx);
                let hot = over(rect) || (idx == dd.sel && f.menu_kbd && !over(panel));
                let a = self.easeq((15, "dropdown", idx), if hot { 1.0 } else { 0.0 }, 1.0 / FADE_SECS);
                if cur {
                    self.text.rounded(r, scene, rect, ROW_R * s, LIT);
                }
                if a > 0.0 {
                    self.text.rounded(r, scene, rect, ROW_R * s, fade(SELECTED, a));
                }
                if cur {
                    self.accent_bar(r, scene, rect, 1.0, false, s);
                }
                let text = clip_to(&self.text, dd.items[idx], dpx as f32, rect[2] - rect[0] - tin * 2.0);
                self.put(r, scene, &text, dpx, if cur { WHITE } else { mix(SOFT, WHITE, a) }, rect[0] + tin, (rect[1] + rect[3]) * 0.5);
                self.dd_rects.push(rect);
            }
            if more {
                let track = [px1 - 8.0 * s, py0 + inner, px1 - 4.0 * s, py0 + ph - inner];
                let thumb = self.thumb(r, scene, track, top, n_vis, dd.items.len(), over(panel), s);
                self.dd_scroll = Some((track, [thumb[0] - 6.0 * s, thumb[1], thumb[2] + 4.0 * s, thumb[3]]));
            }
        }
    }
}
