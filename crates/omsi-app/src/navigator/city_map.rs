//! The city map: the whole map from above in a large window over the game, and the
//! navigator panel's placement and dragging.

use super::*;


/// The city map: the whole map from above in a large window over the game (after ETS2's
/// map screen, but not full-screen) - every road, the trip's route, its stops with their
/// names, the bus and the traffic. Dragged to move, the wheel zooms at the cursor, Escape
/// or a click outside closes it. A click with its pin tool - or a right-click - sets one of
/// the player's own pins (`nav_pins`), which a drag moves and its card lists.
#[derive(Default)]
pub struct CityMap {
    pub open: bool,
    /// Where the window is on the screen (physical pixels).
    pub rect: [f32; 4],
    /// Centre of the view (world) and metres per pixel.
    pub(super) center: DVec2,
    pub(super) mpp: f64,
    /// The view stays on the bus until the map is dragged.
    pub(super) follow: bool,
    pub(super) drag: Option<(f32, f32)>,
    pub(super) target: Option<(TextureId, u32, u32)>,
    /// The roads mesh (buffer 3): the map version it was built for, its size and anchor.
    pub(super) roads: Option<(u64, u32, DVec2)>,
    /// The route mesh (buffer 4): the route, traffic, arrow spacing and roads versions it
    /// was built for, and its size.
    pub(super) route: ((u64, u64, u32, u64), u32),
    /// The map's extent (world), for the zoom limits.
    pub(super) extent: (DVec2, DVec2),
    /// "Centre on the bus" and zoom buttons (window pixels).
    pub(super) buttons: Vec<(Rect, u8)>,
    /// The left column: the duty sheet (`nav_duty`) or the sign-on page (`nav_signon`), each
    /// with its button in the header (a second press puts the column away). The player's
    /// choice holds while the companion's stage is the one it was made at; otherwise the
    /// sign-on page shows until the duty is signed - the map's whole view then, not a
    /// column - and the sheet after (Omsi-Hub's duty panel said "sign on first" until then).
    /// What it showed last, at which stage.
    pub(super) left: Option<(Left, Stage)>,
    pub(super) shown: Left,
    pub(super) stage: Stage,
    /// The column: where it is (window pixels, empty when not shown), how far the sheet's
    /// list is scrolled (points), the trip and stop it last scrolled to, and the interface
    /// scale it was drawn at.
    pub(super) sheet: Rect,
    pub(super) sheet_scroll: f32,
    pub(super) sheet_at: Option<(usize, usize, bool)>,
    pub(super) sheet_scale: f32,
    /// The sign-on page: its own state, where a press does what (window pixels), and the
    /// part of it that scrolls.
    pub(super) phone: crate::nav_signon::Phone,
    pub(super) phone_hits: Vec<(Rect, crate::nav_signon::Action)>,
    pub(super) phone_view: Rect,
    /// The column dragged up or down (a finger has no wheel): the cursor's height before.
    pub(super) column_drag: Option<f32>,
    /// The pin tool (the header's pin button): a click or a tap on the map sets a pin there.
    pub(super) pin_tool: bool,
    /// A press on the map with the pin tool on: where (window pixels), and whether it has
    /// moved since (a drag of the map then, no pin).
    pub(super) tap: Option<(f32, f32, bool)>,
    /// A pin being dragged to another place.
    pub(super) pin_drag: Option<PinDrag>,
    /// The pins' card (the window's own pixels; empty when not shown) and where a press on it
    /// does what.
    pub(super) card: Rect,
    pub(super) card_hits: Vec<(Rect, crate::nav_pins::CardHit)>,
}

/// A pin dragged on the city map: which, where the press began and where the cursor is now
/// (the window's own pixels), and whether it has moved far enough to be a drag.
#[derive(Debug, Clone, Copy)]
pub(super) struct PinDrag {
    pub(super) k: usize,
    pub(super) from: Vec2,
    pub(super) at: Vec2,
    pub(super) moved: bool,
}

/// The city map as painted, before it goes to the GPU: the ground, the traffic's dots, what
/// lies over the map (and the column's list, clipped to `list_clip`), the roads' and the
/// route's meshes when they were built again (the map's world, relative to `anchor`), and the
/// view they are seen through.
pub(super) struct CityPaint {
    pub(super) bg: Painter,
    pub(super) dots: Painter,
    pub(super) ui: Painter,
    pub(super) list: Painter,
    pub(super) list_clip: [f32; 4],
    pub(super) roads: Option<Vec<omsi_ui::Vertex>>,
    pub(super) route: Option<Vec<omsi_ui::Vertex>>,
    pub(super) world: Layer,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) anchor: DVec2,
}

/// What the city map's left column shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum Left {
    #[default]
    Nothing,
    Duty,
    SignOn,
}

/// What the left column shows at the companion's `stage`, with a duty or without: the
/// player's `choice` while it was made at this stage, else the sign-on page until the duty is
/// signed and then the duty sheet (nothing on a free drive). Until the duty is signed the
/// sign-on page is the map's whole view, so a driver who has no duty to sign for and has not
/// signed on gets the map (the badge's amber dot says signing on can be done).
pub(super) fn left_page(choice: Option<(Left, Stage)>, stage: Stage, duty: bool) -> Left {
    let want = match choice {
        Some((l, at)) if at == stage => l,
        _ if stage == Stage::SignOn && !duty => Left::Nothing,
        _ if stage != Stage::OnDuty => Left::SignOn,
        _ if duty => Left::Duty,
        _ => Left::Nothing,
    };
    if want == Left::Duty && !duty { Left::Nothing } else { want }
}

impl CityMap {
    /// A press on the button of `page`: that page in the left column, or the column put away
    /// when it shows already.
    pub(super) fn choose(&mut self, page: Left) {
        let next = if self.shown == page { Left::Nothing } else { page };
        self.left = Some((next, self.stage));
        self.shown = next;
    }
}

impl Navigator {
    /// Open or close the city map.
    /// Where OMSI 2's dynamic route arrows stand on the route ahead (the `nav_arrows`
    /// setting): every junction lane of the next `reach` metres with the way through it
    /// (`L`, `R`, or `dn` for straight on), and the stops of the trip ahead with their
    /// names (`busstop`). Each: a key that stays the same while it is ahead, the place,
    /// the heading, the kind and its text.
    /// `stop_pose` gives a stop object's place and heading where its tile is loaded.
    pub fn arrow_spots(&self, traffic: Option<&Network>, reach: f64, stop_pose: &dyn Fn(i64) -> Option<(DVec3, f64)>) -> Vec<(u64, DVec3, f64, &'static str, String)> {
        let mut out = Vec::new();
        if !self.arrows {
            return out;
        }
        let global = self.global.clone();
        let Some(net) = global.as_deref().or(traffic) else { return out };
        let r = &self.route;
        if !r.on_route {
            return out;
        }
        let mut acc = -(r.s as f64);
        let mut prev_end: Option<f32> = None;
        for (j, &l) in r.lanes.iter().enumerate().skip(r.progress) {
            let Some(lane) = net.lanes.get(l) else { break };
            let len = lane.length();
            if acc > reach {
                break;
            }
            let (h0, h1) = (lane.start_heading(), lane.end_heading());
            let mut d = omsi_sim::traffic::wrap_deg(h1 - h0);
            if let Some(pe) = prev_end {
                d += omsi_sim::traffic::wrap_deg(h0 - pe);
            }
            let junction = net.crossings.get(l).map(|c| !c.is_empty()).unwrap_or(false) || lane.turn != 0;
            let turn = d.abs() > 35.0 && (len < 60.0 || d.abs() > 70.0);
            if (turn || junction) && acc + len as f64 > 5.0 {
                let kind = if !turn { "dn" } else if d > 0.0 { "R" } else { "L" };
                // ten metres on from where the path begins, as OMSI puts it (the original:
                // the path's start moved by (0, 0, 10) in its own frame); the mesh hangs
                // 6-11 m over that point
                let (p, _) = lane.at(10.0f32.min(len * 0.7));
                // facing the driver coming in: the path's heading where it begins
                let h = h0;
                // the street the route goes on into (the junction's own lanes have no name)
                let text = r.lanes.iter().skip(j).take(5).find_map(|&x| self.street_of(x)).map(str::to_string).unwrap_or_default();
                out.push((l as u64, p, h as f64, kind, text));
            }
            prev_end = Some(h1);
            acc += len as f64;
        }
        // The stop's helper: Omsi.exe puts `routearrows_busstop.sco` on the stop object
        // itself, at its place and with its rotation (0x61fc04: the station record's
        // position +0x3c and quaternion +0x54) - where and how the mapper set the stop down,
        // not turned to the bus as it comes.
        for (k, (p, name, h, id)) in self.stop_spots.iter().enumerate() {
            let (p, h) = stop_pose(*id).unwrap_or((*p, *h));
            let d = (p - self.bus_at).truncate().length();
            if d < reach && k < 2 {
                out.push(((1u64 << 40) + p.x.to_bits().rotate_left(7) ^ p.y.to_bits() ^ h.to_bits(), p, h, "busstop", name.clone()));
            }
        }
        out
    }

    pub fn toggle_map(&mut self) {
        self.city.open = !self.city.open;
        if self.city.open {
            self.city.follow = true;
            if self.city.mpp <= 0.0 {
                // For repeatable offscreen GPS comparisons at a chosen zoom.
                self.city.mpp = omsi_cfg::flags::OMSI_NAV_MAP_MPP.parse::<f64>().filter(|v| v.is_finite() && (0.25..=20.0).contains(v)).unwrap_or(2.5);
            }
        }
        self.city.drag = None;
        self.city.column_drag = None;
        self.city.tap = None;
        self.city.pin_drag = None;
        // (the pin tool is put away with the map)
        if !self.city.open {
            self.city.pin_tool = false;
        }
    }

    pub fn map_open(&self) -> bool {
        self.city.open
    }

    /// Where the small navigator is on the screen, when it is shown.
    pub fn screen_rect(&self) -> Option<[f32; 4]> {
        let r = self.panel_rect;
        (self.enabled && !self.city.open && r[2] > r[0]).then_some(r)
    }

    /// The point (physical pixels) is on the small navigator.
    pub fn over_panel(&self, x: f32, y: f32) -> bool {
        let r = self.panel_rect;
        self.enabled && x >= r[0] && y >= r[1] && x < r[2] && y < r[3]
    }

    /// Where the small navigator (`size`) goes on a screen of `screen`: its corner, the top
    /// middle with the on-screen controls (`touch`), below the information bar (`info`) where
    /// they would lie over each other - unless it was dragged somewhere, which it keeps
    /// (kept inside the window), by the mouse or a finger.
    pub(super) fn panel_origin(&mut self, screen: (f32, f32), size: (f32, f32), touch: bool, info: Option<[f32; 4]>, gap: Option<[f32; 3]>) -> (f32, f32) {
        let ((sw, sh), (pw, ph)) = (screen, size);
        let margin = (sh * 0.018).max(10.0).round();
        // (with the on-screen controls the corners are theirs: the top middle)
        let right = self.corner.contains("right");
        let top = self.corner.contains("top") || touch;
        // ("top-center": a phone's, between its on-screen buttons)
        let x0 = if self.corner.contains("center") || touch { ((sw - pw) * 0.5).round() } else if right { sw - margin - pw } else { margin };
        let y0 = if top { margin } else { sh - margin - ph };
        // (below the information bar where they would lie over each other: in its rows on a
        // phone it took the navigator's place in the top middle, #1164)
        let y0 = match info {
            Some(i) if top && x0 < i[2] && x0 + pw > i[0] => y0.max((i[3] + margin * 0.5).round()),
            _ => y0,
        };
        // (and below the on-screen buttons where it is wider than the gap between them)
        let y0 = match gap {
            Some([l, r, bottom]) if top && (x0 < l || x0 + pw > r) => y0.max((bottom + margin * 0.5).round()),
            _ => y0,
        };
        // (dragged somewhere else: there - on a phone too, where it had to stay in the
        // middle it covered, #1138)
        let room = [(sw - pw).max(0.0), (sh - ph).max(0.0)];
        self.panel_room = room;
        match self.at {
            Some(a) => crate::nav_panel::place(a, pw, ph, [sw, sh]).into(),
            None => (x0, y0),
        }
    }

    /// What a press at (`x`, `y`) does on the small navigator while it is the sign-on page: a
    /// key of its keypad, a button.
    pub(super) fn panel_action(&self, x: f32, y: f32) -> Option<crate::nav_signon::Action> {
        let p = self.signing.as_ref()?;
        let local = Vec2::new(x - self.panel_rect[0], y - self.panel_rect[1]);
        p.hits.iter().find(|h| h.0.contains(local)).map(|h| h.1)
    }

    /// The mouse button went down on the small navigator: a click opens the city map, a
    /// drag moves the navigator (see [`Navigator::panel_move`]). While it is the sign-on page
    /// a press on a key or a button works it there and then (as on the city map), and is no
    /// click nor drag.
    /// A press on the board's handle shows or hides the board (the setting kept, see
    /// [`Navigator::take_board`]); on an edge or a corner a drag sizes the panel by it.
    pub fn panel_press(&mut self, x: f32, y: f32) {
        if let Some(a) = self.panel_action(x, y) {
            self.panel_drag = None;
            self.phone_act(a);
            // (drawn again at once: the digit shows in its box)
            self.drawn_at = f32::MIN;
            return;
        }
        if self.over_handle(x, y) {
            self.panel_drag = None;
            self.toggle_board();
            return;
        }
        let r = self.panel_rect;
        let grip = self.grip_at(x, y).unwrap_or(crate::nav_panel::Grip::Move);
        self.panel_drag = Some(PanelGrab { from: [x, y], rect: [r[0] - self.origin_x, r[1], r[2] - self.origin_x, r[3]], grip, moved: false });
    }

    /// What a press at (`x`, `y`) takes hold of on the small navigator: its edges only where
    /// it can be sized (not on the cockpit's display, not with the on-screen controls).
    pub(super) fn grip_at(&self, x: f32, y: f32) -> Option<crate::nav_panel::Grip> {
        let grip = crate::nav_panel::grip_at(self.panel_rect, x, y, self.edge())?;
        Some(if self.cockpit_display || crate::platform::touch_controls() { crate::nav_panel::Grip::Move } else { grip })
    }

    /// How far in from its border an edge of the small navigator can be taken hold of.
    pub(super) fn edge(&self) -> f32 {
        (7.0 * self.unit).clamp(6.0, 14.0)
    }

    /// The smallest the player can make the small navigator (pixels).
    pub(super) fn min_size(&self) -> [f32; 2] {
        let u = self.unit.max(0.5);
        [crate::nav_panel::MIN_SIZE[0] * u, crate::nav_panel::MIN_SIZE[1] * u]
    }

    /// The point (physical pixels) is on the duty board's handle.
    pub(super) fn over_handle(&self, x: f32, y: f32) -> bool {
        let Some(l) = self.layout.filter(|_| self.signing.is_none() && self.enabled && !self.city.open) else { return false };
        let local = Vec2::new(x - self.panel_rect[0], y - self.panel_rect[1]);
        l.handle.is_some_and(|h| h.inset(-4.0 * l.s).contains(local))
    }

    /// The board under the map shown or put away (its handle): kept as the player's choice,
    /// for Shift+N and the next game too.
    pub fn toggle_board(&mut self) {
        self.schedule = !self.schedule;
        self.board = self.schedule;
        self.board_changed = Some(self.board);
        self.drawn_at = f32::MIN;
    }

    /// The board wanted under the map or not (the game's settings): shown or put away at once.
    pub fn set_board(&mut self, on: bool) {
        self.board = on;
        self.schedule = on;
        self.drawn_at = f32::MIN;
    }

    /// The board was shown or put away on the panel: the `nav_board` setting to keep.
    pub fn take_board(&mut self) -> Option<bool> {
        self.board_changed.take()
    }

    /// The cursor moved with the button held on the navigator: past a few pixels the panel
    /// follows it, or the edges it holds do. True while it is being dragged.
    pub fn panel_move(&mut self, x: f32, y: f32) -> bool {
        use crate::nav_panel::{self as np, Grip};
        let Some(g) = self.panel_drag.as_mut() else { return false };
        let (dx, dy) = (x - g.from[0], y - g.from[1]);
        if !g.moved && dx.hypot(dy) < if g.grip == Grip::Move { 6.0 } else { 2.0 } {
            return false;
        }
        g.moved = true;
        let g = *g;
        match g.grip {
            Grip::Move => {
                let room = self.panel_room;
                let share = |p: f32, r: f32| if r > 0.0 { (p / r).clamp(0.0, 1.0) } else { 0.0 };
                self.at = Some([share(g.rect[0] + dx, room[0]), share(g.rect[1] + dy, room[1])]);
            }
            sides => {
                let r = np::resized(g.rect, sides, dx, dy, self.min_size(), self.window);
                let (w, h) = (r[2] - r[0], r[3] - r[1]);
                let sh = self.window[1].max(1.0);
                self.custom = Some([w / sh, h / sh]);
                self.at = Some(np::share(Vec2::new(r[0], r[1]), w, h, self.window));
                self.drawn_at = f32::MIN;
            }
        }
        true
    }

    /// The button came up after a press on the navigator: Some(true) when it was dragged
    /// (moved or sized: the place and size are then kept, [`Navigator::take_rect`]),
    /// Some(false) for a click.
    pub fn panel_release(&mut self) -> Option<bool> {
        let g = self.panel_drag.take()?;
        self.rect_changed |= g.moved;
        Some(g.moved)
    }

    /// The mouse at (`x`, `y`) (no button held, or the navigator held): what it is over on the
    /// small navigator - the handle lit, an edge marked - and the cursor for it (as
    /// `App::set_cursor_kind` has them); None where it is not over the navigator.
    pub fn hover(&mut self, x: f32, y: f32) -> Option<u8> {
        let part = if let Some(g) = self.panel_drag.filter(|g| g.moved) {
            Some(HoverPart::Grip(g.grip))
        } else if !self.over_panel(x, y) || self.city.open {
            None
        } else if self.over_handle(x, y) {
            Some(HoverPart::Handle)
        } else {
            self.grip_at(x, y).map(HoverPart::Grip)
        };
        if part != self.panel_hover {
            self.panel_hover = part;
            self.drawn_at = f32::MIN;
        }
        part.map(|p| match p {
            HoverPart::Handle => 1,
            HoverPart::Grip(crate::nav_panel::Grip::Move) if self.panel_drag.is_some_and(|g| g.moved) => 3,
            HoverPart::Grip(crate::nav_panel::Grip::Move) => 0,
            HoverPart::Grip(g) => g.cursor(),
        })
    }

    /// The `nav_rect` setting for where the navigator is and how large the player made it.
    pub fn rect_setting(&self) -> String {
        let wide = |p: [f32; 2]| [p[0] as f64, p[1] as f64];
        omsi_launcher_lib::nav_rect_text(self.at.map(wide), self.custom.map(wide))
    }

    /// The place and size from the `nav_rect` setting (a place there wins over the corner).
    pub fn set_rect(&mut self, setting: &str) {
        let (at, size) = omsi_launcher_lib::nav_rect_parts(setting);
        let narrow = |p: [f64; 2]| [p[0] as f32, p[1] as f32];
        if at.is_some() {
            self.at = at.map(narrow);
        }
        self.custom = size.map(narrow);
    }

    /// The navigator was moved or sized in the game: the `nav_rect` setting to keep.
    pub fn take_rect(&mut self) -> Option<String> {
        std::mem::take(&mut self.rect_changed).then(|| self.rect_setting())
    }

    /// The navigator as it came: in the bottom-left corner at its own size, at 100 %, the
    /// board under the map (shown again when a duty runs).
    pub fn reset_panel(&mut self) {
        self.at = None;
        self.custom = None;
        self.corner = "bottom-left".into();
        self.size = 1.0;
        self.board = true;
        self.schedule = self.duty_seen;
        self.panel_drag = None;
        self.drawn_at = f32::MIN;
    }

    pub(super) fn map_hit(&self, x: f32, y: f32) -> bool {
        let r = self.city.rect;
        x >= r[0] && y >= r[1] && x < r[2] && y < r[3]
    }

    /// A mouse press while the map is open: on it, a drag or a button; outside, it closes.
    pub fn map_press(&mut self, x: f32, y: f32) {
        if !self.map_hit(x, y) {
            self.city.open = false;
            self.city.pin_tool = false;
            return;
        }
        let local = Vec2::new(x - self.city.rect[0], y - self.city.rect[1]);
        let hit = self.city.buttons.iter().find(|(r, _)| r.contains(local)).map(|b| b.1);
        // (a pin under the press, not one under the column or the card)
        let pin = (hit.is_none() && !self.city.sheet.contains(local) && !self.city.card.contains(local) && !self.full_view()).then(|| self.pin_under(local)).flatten();
        match hit {
            Some(0) => self.city.follow = true,
            Some(1) => self.city.mpp = (self.city.mpp / 1.6).max(0.25),
            Some(2) => self.city.mpp = (self.city.mpp * 1.6).min(self.max_mpp()),
            Some(3) => self.city.open = false,
            Some(4) => self.city.choose(Left::Duty),
            Some(5) => self.city.choose(Left::SignOn),
            // (the sign-on page's whole view: the map instead, until the stage changes - the
            // badge brings the page back)
            Some(6) => self.city.choose(Left::Nothing),
            Some(7) => self.resize_by(-1),
            Some(8) => self.resize_by(1),
            // the pin tool: a click (a tap) on the map sets a pin
            Some(9) => self.city.pin_tool = !self.city.pin_tool,
            // the pins' card: its buttons (and no drag of the map under it)
            _ if self.city.card.contains(local) => {
                use crate::nav_pins::{CardHit, Op};
                if let Some(h) = self.city.card_hits.iter().find(|(r, _)| r.contains(local)).map(|h| h.1) {
                    self.pins.ops.push(match h {
                        CardHit::Remove(k) => Op::Remove(k),
                        CardHit::Up(k) => Op::Shift(k, true),
                        CardHit::Down(k) => Op::Shift(k, false),
                        CardHit::Clear => Op::Clear,
                    });
                }
            }
            // (the column is no handle to drag the map by; on the sign-on page its keys and
            // buttons work)
            _ if self.city.sheet.contains(local) => {
                let on_page = self.city.shown == Left::SignOn && self.city.phone_view.contains(local);
                match self.city.phone_hits.iter().find(|(r, _)| on_page && r.contains(local)).map(|h| h.1) {
                    Some(a) => self.phone_act(a),
                    None => self.city.column_drag = Some(y),
                }
            }
            // (the sign-on page's whole view: no map to drag under its header)
            _ if self.full_view() => {}
            // a pin: dragged to another place
            _ if pin.is_some() => self.city.pin_drag = pin.map(|k| PinDrag { k, from: local, at: local, moved: false }),
            _ => {
                self.city.drag = Some((x, y));
                // (with the pin tool a press that comes up where it went down sets a pin;
                // the header is no place for one)
                if self.city.pin_tool && local.y > 44.0 * self.city.sheet_scale.max(0.5) {
                    self.city.tap = Some((x, y, false));
                }
            }
        }
    }

    /// A right-click on the city map: a pin where it is (as a click with the pin tool), or the
    /// pin under it taken away. Not on the header, the column or the card.
    pub fn map_right_press(&mut self, x: f32, y: f32) {
        use crate::nav_pins::Op;
        if !self.city.open || !self.map_hit(x, y) || self.full_view() {
            return;
        }
        let local = Vec2::new(x - self.city.rect[0], y - self.city.rect[1]);
        if local.y < 44.0 * self.city.sheet_scale.max(0.5) || self.city.sheet.contains(local) || self.city.card.contains(local) {
            return;
        }
        match self.pin_under(local) {
            Some(k) => self.pins.ops.push(Op::Remove(k)),
            None => {
                if let Some(p) = self.map_point(x, y) {
                    self.pins.ops.push(Op::Add(p));
                }
            }
        }
    }

    /// Where a world point is in the city map's window (its own pixels).
    pub(super) fn city_spot(&self, p: DVec2) -> Vec2 {
        let r = self.city.rect;
        let (w, h) = ((r[2] - r[0]) as f64, (r[3] - r[1]) as f64);
        let d = p - self.city.center;
        Vec2::new((w * 0.5 + d.x / self.city.mpp) as f32, (h * 0.5 - d.y / self.city.mpp) as f32)
    }

    /// The pin whose marker is under `local` (the window's own pixels): a via's disc, the
    /// destination's head or its point; the one drawn on top first.
    pub(super) fn pin_under(&self, local: Vec2) -> Option<usize> {
        use crate::nav_pins::{destination_head, Role, DEST_R, VIA_R};
        let s = self.city.sheet_scale.max(0.5);
        (0..self.pins.list.len()).find(|&k| {
            let p = self.city_spot(self.pins.list[k].at);
            match self.pins.role(k) {
                Role::Destination => p.distance(local) < 10.0 * s || destination_head(p, DEST_R * s).distance(local) < (DEST_R + 4.0) * s,
                Role::Via(_) => p.distance(local) < (VIA_R + 5.0) * s,
            }
        })
    }

    /// A key for the sign-on page's keypad: it takes the digits, Backspace and Delete while
    /// it asks for the number or the code - on the city map, or on the small navigator while
    /// that is the page. True when the key was used (the caller then gives it to nothing
    /// else).
    pub fn page_key(&mut self, code: winit::keyboard::KeyCode) -> bool {
        if !self.keypad_shown() {
            return false;
        }
        let Some(a) = crate::nav_signon::key_action(code) else { return false };
        self.phone_act(a);
        self.drawn_at = f32::MIN;
        true
    }

    /// The sign-on page's keypad is on the screen: the city map's page, or the small
    /// navigator as the page, asking for the number or the code.
    pub fn keypad_shown(&self) -> bool {
        if self.city.open {
            return self.city.shown == Left::SignOn && self.city.stage == Stage::SignOn;
        }
        self.enabled && self.signing.as_ref().is_some_and(|p| p.stage == Stage::SignOn)
    }

    /// The city map is the sign-on page, its whole view (no map to point at).
    pub(super) fn full_view(&self) -> bool {
        self.city.shown == Left::SignOn && self.city.stage != Stage::OnDuty
    }

    /// Do what a press on the sign-on page asks: on the page, and of the game (through the
    /// companion, whose state the phones share).
    pub(super) fn phone_act(&mut self, a: crate::nav_signon::Action) {
        use crate::nav_signon::Call;
        let st = crate::companion::state();
        self.city.phone.follow(&st);
        let Some(call) = self.city.phone.press(a, &st) else { return };
        match call {
            Call::SignOn { number, code } => {
                let answer = crate::companion::sign_on(&number, code.as_deref());
                self.city.phone.attempted(answer);
            }
            Call::SignOff => crate::companion::sign_off(),
            Call::Free => {
                crate::companion::drive_free();
            }
            Call::Ask(r) => crate::companion::request(r),
            Call::NewCode => crate::companion::new_pairing_code(),
            Call::Forget => crate::companion::forget_devices(),
            Call::Reveal(on) => crate::companion::reveal(on),
        }
    }

    /// The world point (x, y) under the window point, when the map is open and the point
    /// is on it.
    pub fn map_point(&self, x: f32, y: f32) -> Option<DVec2> {
        if !self.city.open || !self.map_hit(x, y) || self.full_view() {
            return None;
        }
        let r = self.city.rect;
        let (w, h) = ((r[2] - r[0]) as f64, (r[3] - r[1]) as f64);
        let (lx, ly) = ((x - r[0]) as f64 - w * 0.5, (y - r[1]) as f64 - h * 0.5);
        Some(self.city.center + DVec2::new(lx, -ly) * self.city.mpp)
    }

    pub fn map_release(&mut self) {
        use crate::nav_pins::Op;
        // a pin dragged: to where it was let go (snapped onto a road in the next frame; back
        // where it was with none near)
        // (moved as far as the cursor went: it may have taken the flag by its head)
        if let Some(d) = self.city.pin_drag.take().filter(|d| d.moved) {
            let mpp = self.city.mpp;
            if let Some(pin) = self.pins.list.get_mut(d.k) {
                let from = pin.at;
                let to = from + DVec2::new((d.at.x - d.from.x) as f64 * mpp, -(d.at.y - d.from.y) as f64 * mpp);
                pin.at = to;
                self.pins.ops.push(Op::Move(d.k, to, from));
            }
        }
        // the pin tool: a click that did not drag the map sets a pin
        if let Some((x, y, false)) = self.city.tap.take() {
            if let Some(p) = self.map_point(x, y) {
                self.pins.ops.push(Op::Add(p));
            }
        }
        self.city.drag = None;
        self.city.column_drag = None;
    }

    pub fn map_move(&mut self, x: f32, y: f32) {
        // a pin dragged follows the cursor (past a few pixels: a press alone moves nothing)
        if let Some(d) = self.city.pin_drag.as_mut() {
            let local = Vec2::new(x - self.city.rect[0], y - self.city.rect[1]);
            if d.moved || local.distance(d.from) > 5.0 {
                d.moved = true;
                d.at = local;
            }
            return;
        }
        if let Some((tx, ty, moved)) = self.city.tap.as_mut() {
            if (x - *tx).hypot(y - *ty) > 8.0 {
                *moved = true;
            }
        }
        // the left column dragged: its list or page follows the cursor (kept in range when it
        // is drawn)
        if let Some(py) = self.city.column_drag {
            let scroll = if self.city.shown == Left::SignOn { &mut self.city.phone.scroll } else { &mut self.city.sheet_scroll };
            *scroll = (*scroll - (y - py)).max(0.0);
            self.city.column_drag = Some(y);
            return;
        }
        if let Some((px, py)) = self.city.drag {
            let (dx, dy) = ((x - px) as f64, (y - py) as f64);
            if dx.abs() + dy.abs() > 0.5 {
                self.city.follow = false;
            }
            self.city.center.x -= dx * self.city.mpp;
            self.city.center.y += dy * self.city.mpp;
            self.city.drag = Some((x, y));
        }
    }

    /// The wheel over the map: zoom, keeping the point under the cursor where it is.
    pub fn map_wheel(&mut self, amount: f32, x: f32, y: f32) {
        let r = self.city.rect;
        // (over the pins' card: nothing to zoom)
        if self.city.card.contains(Vec2::new(x - r[0], y - r[1])) {
            return;
        }
        // over the left column: the duty sheet's list or the sign-on page scrolls (kept in
        // range when it is drawn)
        if self.city.sheet.contains(Vec2::new(x - r[0], y - r[1])) {
            let by = amount * 48.0 * self.city.sheet_scale.max(0.5);
            let scroll = if self.city.shown == Left::SignOn { &mut self.city.phone.scroll } else { &mut self.city.sheet_scroll };
            *scroll = (*scroll - by).max(0.0);
            return;
        }
        let (w, h) = ((r[2] - r[0]) as f64, (r[3] - r[1]) as f64);
        let (lx, ly) = ((x - r[0]) as f64 - w * 0.5, (y - r[1]) as f64 - h * 0.5);
        let before = self.city.center + DVec2::new(lx, -ly) * self.city.mpp;
        let k = (1.0 - amount as f64 * 0.15).clamp(0.6, 1.6);
        self.city.mpp = (self.city.mpp * k).clamp(0.25, self.max_mpp());
        let after = self.city.center + DVec2::new(lx, -ly) * self.city.mpp;
        self.city.center += before - after;
        if amount.abs() > 0.0 && (lx.abs() > 40.0 || ly.abs() > 40.0) {
            self.city.follow = false;
        }
    }

    pub(super) fn max_mpp(&self) -> f64 {
        let (lo, hi) = self.city.extent;
        let span = (hi - lo).max_element().max(2000.0);
        let r = self.city.rect;
        span / ((r[2] - r[0]).max(200.0) as f64) * 1.2
    }

    /// Draw the city map and lay it over the picture.
    pub(super) fn city(&mut self, renderer: &Renderer, scene: &mut Scene, f: &NavFrame) {
        self.atlas.begin_frame();
        let (sw, sh) = f.screen;
        let (w, h) = ((sw * 0.8).round(), (sh * 0.82).round());
        let (x0, y0) = (((sw - w) * 0.5).round(), ((sh - h) * 0.5).round());
        self.city.rect = [x0, y0, x0 + w, y0 + h];
        let (tw, th) = (w as u32, h as u32);
        if self.gpu.is_none() {
            self.gpu = Some(Gpu::new(&renderer.device, renderer.format(), map_samples(renderer.format()), self.atlas.size));
        }
        if self.city.target.map(|t| (t.1, t.2) != (tw, th)).unwrap_or(true) {
            if let Some((t, _, _)) = self.city.target.take() {
                renderer.free_texture(scene, t);
                scene.premultiplied.remove(&t);
            }
            let t = renderer.add_render_texture(scene, tw, th);
            scene.premultiplied.insert(t);
            self.city.target = Some((t, tw, th));
        }
        // (the navigator's own size on top of the interface's: the panels, texts and buttons)
        let s = (h / 760.0).clamp(0.95, 2.0) * f.ui_scale * if self.cockpit_display { 1.0 } else { self.size };
        // the left column: the duty sheet (Omsi-Hub's live duty) or the sign-on page
        // (Omsi-Hub's phone), as `left_page` has it
        let duty = f.duty.and_then(|d| crate::nav_duty::DutyState::of(d, f.time));
        let companion = crate::companion::state();
        let left = left_page(self.city.left, companion.stage, duty.is_some());
        self.city.stage = companion.stage;
        self.city.shown = left;
        self.city.phone.follow(&companion);
        self.city.sheet_scale = s;
        // until the duty is signed the sign-on page is the whole view, the map behind it
        if self.full_view() {
            self.city_signon(renderer, scene, f, &companion, duty.as_ref(), [x0, y0, w, h], s);
            return;
        }
        let paint = self.paint_city(f, &companion, duty.as_ref(), left, w, h, s);
        self.city_to_gpu(renderer, scene, paint, [x0, y0, w, h], s);
    }

    /// The city map painted for a window `w` x `h` at scale `s`, its left column showing
    /// `left`: the roads and the route (new meshes when they changed, in the map's world
    /// relative to its anchor), the ground, the traffic, and everything over them - the stops,
    /// the street names, the player's own pins, the bus, the column, the pins' card, the
    /// header and the scale. Where a press does what is kept for [`Navigator::map_press`].
    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint_city(&mut self, f: &NavFrame, companion: &crate::companion::CompanionState, duty: Option<&crate::nav_duty::DutyState>, left: Left, w: f32, h: f32, s: f32) -> CityPaint {
        let head_h = 44.0 * s;
        let sheet = if left == Left::Nothing { Rect::default() } else { Rect::new(12.0 * s, head_h + 12.0 * s, (340.0 * s).min(w * 0.42), h - head_h - 24.0 * s) };
        self.city.sheet = sheet;
        self.city.sheet_scale = s;
        // the pins' card (`nav_pins`): in the left column when that shows nothing else, else at
        // the right edge
        let card = self.pin_card(f);
        let room = h - head_h - 48.0 * s;
        let card_rect = if !crate::nav_pins::card_shown(&card) {
            Rect::default()
        } else if sheet.w > 0.0 {
            let cw = (320.0 * s).min(w * 0.34);
            Rect::new(w - 12.0 * s - cw, head_h + 12.0 * s, cw, crate::nav_pins::card_height(&card, s, room))
        } else {
            Rect::new(12.0 * s, head_h + 12.0 * s, (340.0 * s).min(w * 0.42), crate::nav_pins::card_height(&card, s, room))
        };
        self.city.card = card_rect;
        if self.city.follow {
            // (the bus in the middle of what the sheet - or the card in its place - leaves of
            // the map)
            let side = if sheet.w > 0.0 { sheet.right() } else { card_rect.right() };
            self.city.center = f.bus.truncate() - DVec2::new(side as f64 * 0.5 * self.city.mpp, 0.0);
        }
        let global = self.global.clone();
        let net = global.as_deref().or(f.traffic.map(|t| &t.net));
        // roads of the whole map (buffer 3), once per map version
        let mut roads_verts = None;
        if let Some(n) = net {
            let version = self.global_version * 1_000_000 + n.lanes.len() as u64;
            if self.city.roads.map(|r| r.0 != version).unwrap_or(true) {
                let (mut lo, mut hi) = (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN));
                let road_lanes = road_geometry(n);
                for l in &road_lanes {
                    for p in &l.points {
                        lo = lo.min(p.truncate());
                        hi = hi.max(p.truncate());
                    }
                }
                if lo.x == f64::MAX {
                    lo = f.bus.truncate();
                    hi = lo;
                }
                let anchor = (lo + hi) * 0.5;
                self.city.extent = (lo, hi);
                let rel = |q: DVec3| Vec3::new((q.x - anchor.x) as f32, (q.y - anchor.y) as f32, 0.0);
                let mut p = Painter::new();
                for pass in 0..2 {
                    for l in &road_lanes {
                        let pts = simplify(&l.points.iter().map(|q| rel(*q)).collect::<Vec<_>>(), 0.12);
                        if pass == 0 {
                            p.ribbon(&pts, l.width + 2.0, 2.4, Color::rgba(26, 26, 26, 1.0), true);
                        } else {
                            p.ribbon(&pts, l.width, 1.4, if l.main { ROAD_MAIN } else { ROAD }, true);
                        }
                    }
                }
                self.city.roads = Some((version, p.len(), anchor));
                roads_verts = Some(p.verts);
            }
        }
        let anchor = self.city.roads.map(|r| r.2).unwrap_or(f.bus.truncate());
        let rel = |q: DVec3| Vec3::new((q.x - anchor.x) as f32, (q.y - anchor.y) as f32, 0.0);
        // the whole route (buffer 4): the part driven grey, the rest by how busy it is,
        // with arrows about 90 pixels apart (built again when the zoom changes that much)
        let mut route_verts = None;
        let every = 2f64.powf((90.0 * s as f64 * self.city.mpp).log2().round()) as f32;
        let key = (self.route.version, self.jam_version, every.to_bits(), self.city.roads.map(|r| r.0).unwrap_or(0));
        if self.city.route.0 != key {
            let mut p = Painter::new();
            if let Some(n) = net {
                let r = &self.route;
                let done = r.progress.min(r.lanes.len());
                for &l in &r.lanes[..done] {
                    let Some(lane) = n.lanes.get(l) else { continue };
                    let pts: Vec<Vec3> = lane.points.iter().map(|q| rel(*q)).collect();
                    p.ribbon(&pts, lane.width.max(3.0) + 2.0, 5.0, DRIVEN, true);
                }
                let style = RouteStyle { extra_m: 0.0, min_px: 6.0, arrows: Some((every, f32::MAX)), max_len: f32::MAX, near: None, end: r.end };
                let bus_lane = r.lanes.get(done).map(|&l| lane_from_right(n, l, f.bus)).unwrap_or(0);
                build_route(&mut p, n, &r.lanes[done..], anchor, r.s, &self.route_jam, &style, bus_lane);
            }
            self.city.route = (key, p.len());
            route_verts = Some(p.verts);
        }
        // the view: north up, `mpp` metres a pixel
        let c = self.city.center - anchor;
        let (hw, hh) = (w as f64 * 0.5 * self.city.mpp, h as f64 * 0.5 * self.city.mpp);
        let proj = Mat4::orthographic_rh((c.x - hw) as f32, (c.x + hw) as f32, (c.y - hh) as f32, (c.y + hh) as f32, -1000.0, 1000.0);
        let vp = [0.0, 0.0, w, h];
        let world = Layer { view_proj: proj, viewport: vp, clip: [0.0, 0.0, w, h], radius: 10.0 * s, opacity: 1.0, px_scale: self.city.mpp as f32 };
        let to_screen = |q: DVec3| -> Vec2 {
            let d = q.truncate() - self.city.center;
            Vec2::new((w as f64 * 0.5 + d.x / self.city.mpp) as f32, (h as f64 * 0.5 - d.y / self.city.mpp) as f32)
        };
        let win = Rect::new(0.0, 0.0, w, h);
        let mut bg = Painter::new();
        // (the opacity setting the map's ground too; the roads, names and header stay solid)
        bg.rounded(win, 10.0 * s, Color::rgba(12, 16, 26, crate::ui::backdrop(self.opacity).min(1.0)));
        // traffic: blue dots, the public transport in its colours with its line (as on the
        // small map)
        let mut dots = Painter::new();
        let vehicles = map_vehicles(f.traffic.filter(|_| self.show_ai), f.bus, f64::INFINITY);
        paint_vehicles(&mut dots, &vehicles, &rel, self.city.mpp as f32, s);
        let lines: Vec<(DVec3, Color, String, [DVec3; 2])> = vehicles.iter().filter_map(|v| v.line.clone().map(|l| (v.at, v.color, l, v.drawn_ends(self.city.mpp as f32, s)))).collect();
        // stops with their names, the bus, the frame and the header
        let mut ui = Painter::new();
        let n_stops = f.stops.len();
        // street names along their roads, where there is room (stops' names go first)
        let markers = spaced_markers(f.stops.iter().enumerate().map(|(k, st)| (k, to_screen(st.position))).filter(|(_, p)| win.contains(*p) && p.y > 50.0 * s), 20.0 * s);
        let mut taken: Vec<Rect> = markers.iter().map(|(_, p)| Rect::new(p.x - 9.0 * s, p.y - 9.0 * s, 18.0 * s, 18.0 * s)).collect();
        taken.push(Rect::new(0.0, 0.0, w, 50.0 * s));
        if sheet.w > 0.0 {
            taken.push(sheet.inset(-6.0 * s));
        }
        if card_rect.w > 0.0 {
            taken.push(card_rect.inset(-6.0 * s));
        }
        let mut stop_labels = Vec::new();
        for &(k, p) in &markers {
            if self.city.mpp >= 4.0 && k != 0 && k + 1 != n_stops { continue; }
            let st = &f.stops[k];
            let weight = if k == 0 { Weight::Bold } else { Weight::Medium };
            let name = format!("{}  {:02}:{:02}", st.name.trim(), (st.arrival / 3600.0) as i32 % 24, ((st.arrival % 3600.0) / 60.0) as i32);
            let name = self.fonts.fit(&name, 12.5 * s, weight, (300.0 * s).min(w * 0.3));
            let lw = self.fonts.width(&name, 12.5 * s, weight) + 14.0 * s;
            if let Some(r) = stop_label_rect(p, lw, s, win, &taken) {
                taken.push(r);
                stop_labels.push((k, name, r));
            }
        }
        // the public transport's line tags, above their dots, where there is room
        for (pos, color, l, ends) in &lines {
            let p = to_screen(*pos);
            if !win.contains(p) {
                continue;
            }
            let px = 11.0 * s;
            let own = f.line.as_deref().is_some_and(|o| o.trim().eq_ignore_ascii_case(l.trim()));
            let l = self.fonts.fit(l, px, Weight::Bold, 50.0 * s);
            let tw = self.fonts.width(&l, px, Weight::Bold) + crate::stop_signs::CHIP_PAD * s;
            // (above the bus's higher end, not over the bus)
            let top = ends.iter().map(|e| to_screen(*e).y).fold(p.y, f32::min);
            let r = Rect::new(p.x - tw * 0.5, top.min(p.y - 7.0 * s) - 17.0 * s, tw, 15.0 * s);
            if taken.iter().any(|o| o.x < r.right() && r.x < o.right() && o.y < r.bottom() && r.y < o.bottom()) {
                continue;
            }
            taken.push(r);
            crate::stop_signs::draw_chip(&mut ui, &mut self.atlas, &self.fonts, r, &l, px, *color, own, s);
        }
        if let (Some(st), true) = (self.streets.clone(), self.city.mpp < 3.2 && self.global.is_some()) {
            let px = 12.0 * s;
            for (q, a, id) in &st.labels {
                let p = to_screen(q.extend(0.0));
                if !win.pad(60.0 * s, 30.0 * s).contains(p) {
                    continue;
                }
                let name = &st.names[*id as usize];
                let tw = self.fonts.width(name, px, Weight::Medium);
                // (upright: turned at most a quarter either way)
                let mut ang = -*a;
                if ang > std::f32::consts::FRAC_PI_2 {
                    ang -= std::f32::consts::PI;
                } else if ang <= -std::f32::consts::FRAC_PI_2 {
                    ang += std::f32::consts::PI;
                }
                // beside the road, not on it
                let side = Vec2::new(-ang.sin(), ang.cos()) * (px * 0.9 + 2.0 * s);
                let c = p + side;
                let (hx, hy) = ((ang.cos() * tw * 0.5).abs() + (ang.sin() * px * 0.6).abs(), (ang.sin() * tw * 0.5).abs() + (ang.cos() * px * 0.6).abs());
                let bb = Rect::new(c.x - hx, c.y - hy, hx * 2.0, hy * 2.0);
                if taken.iter().any(|t| rects_overlap(t, &bb)) {
                    continue;
                }
                taken.push(bb);
                let halo = Color::rgba(15, 15, 15, 0.9);
                for o in [Vec2::new(1.0, 0.0), Vec2::new(-1.0, 0.0), Vec2::new(0.0, 1.0), Vec2::new(0.0, -1.0)] {
                    ui.text_rotated(&mut self.atlas, &self.fonts, name, px, Weight::Medium, c + o * s, ang, halo);
                }
                ui.text_rotated(&mut self.atlas, &self.fonts, name, px, Weight::Medium, c, ang, STREET);
            }
        }
        // the stops as the sign the player chose (`stop_signs`): the ones served faded under
        // everything, the ones ahead over their names (the next one's edge reaches past where
        // its name begins), far out the stops between the next and the terminus as dots
        let signs = crate::stop_signs::style();
        let far = self.city.mpp >= 4.0;
        for q in crate::stop_signs::served(f.duty, &self.stop_pos).into_iter().map(&to_screen).filter(|q| win.contains(*q) && q.y > 50.0 * s && markers.iter().all(|(_, m)| m.distance(*q) >= 14.0 * s)) {
            crate::stop_signs::draw(&mut ui, signs, crate::stop_signs::Kind::Passed, q, if far { 6.0 } else { 13.0 } * s);
        }
        for (k, name, r) in stop_labels {
            crate::stop_signs::draw_label(&mut ui, &mut self.atlas, &self.fonts, r, &name, 12.5 * s, signs, k == 0, s);
        }
        for (k, p) in markers.into_iter().rev() {
            let kind = crate::stop_signs::Kind::ahead(k, n_stops);
            let dot = far && kind == crate::stop_signs::Kind::Ahead;
            crate::stop_signs::draw(&mut ui, signs, kind, p, if dot { 6.0 } else { 13.0 } * s);
        }
        // the player's own pins (`nav_pins`): the destination's flag, the vias numbered - the
        // one being dragged where the cursor has it
        let dragged = self.city.pin_drag.filter(|d| d.moved);
        for k in (0..self.pins.list.len()).rev() {
            let here = dragged.filter(|d| d.k == k);
            let p = to_screen(self.pins.list[k].at.extend(0.0)) + here.map(|d| d.at - d.from).unwrap_or(Vec2::ZERO);
            if !win.pad(30.0 * s, 60.0 * s).contains(p) {
                continue;
            }
            let alpha = if here.is_some() { 0.8 } else { 1.0 };
            match self.pins.role(k) {
                crate::nav_pins::Role::Destination => crate::nav_pins::draw_destination(&mut ui, &mut self.atlas, p, crate::nav_pins::DEST_R * s, self.pins.arrived.is_some(), alpha),
                crate::nav_pins::Role::Via(n) => crate::nav_pins::draw_via(&mut ui, &mut self.atlas, &self.fonts, p, n, crate::nav_pins::VIA_R * s, alpha),
            }
        }
        // the other players: an arrow the way they face and their name (as on the small map)
        for pl in &f.players {
            let p = to_screen(pl.position);
            if !win.contains(p) || p.y < 44.0 * s {
                continue;
            }
            arrow(&mut ui, p, (pl.heading as f32).to_radians(), 8.5 * s, 1.3, Color::rgba(10, 10, 10, 0.9), PLAYER);
            let px = 11.0 * s;
            let name = self.fonts.fit(&pl.name, px, Weight::Bold, 140.0 * s);
            let tw = self.fonts.width(&name, px, Weight::Bold) + 9.0 * s;
            let r = Rect::new(p.x - tw * 0.5, p.y - 27.0 * s, tw, 15.0 * s);
            if taken.iter().any(|o| rects_overlap(o, &r)) {
                continue;
            }
            taken.push(r);
            ui.rounded(Rect::new(r.x - 1.0 * s, r.y - 1.0 * s, r.w + 2.0 * s, r.h + 2.0 * s), 4.5 * s, Color::rgba(10, 10, 10, 0.9));
            ui.rounded(r, 4.0 * s, PLAYER);
            ui.text_in(&mut self.atlas, &self.fonts, &name, px, Weight::Bold, r, Align::Center, LINE_TEXT);
        }
        arrow(&mut ui, to_screen(f.bus), (f.heading as f32).to_radians(), 10.0 * s, 1.3, Color::rgba(10, 10, 10, 0.9), TEXT);
        // the duty sheet: its frame and head here, its list in a layer of its own clipped to
        // it (the list scrolls)
        let mut list = Painter::new();
        let mut list_clip = [0.0; 4];
        self.city.phone_hits.clear();
        self.city.phone_view = Rect::default();
        if left == Left::SignOn {
            // the sign-on page: its head here, its items in the clipped layer (they scroll)
            if let Some(r) = self.city.phone.wants(companion) {
                crate::companion::request(r);
            }
            let qr = self.pair_qr(companion);
            let page = crate::nav_signon::page_in(companion, &self.city.phone, duty, f.time, crate::nav_signon::Room::Column, qr.as_ref());
            let view = crate::nav_signon::draw_head(&mut crate::nav_duty::Pen { p: &mut ui, atlas: &mut self.atlas, fonts: &self.fonts }, &page, sheet, s);
            let pad = crate::nav_signon::PAD * s;
            let (placed, height) = crate::nav_signon::layout(&page.items, view.w - 2.0 * pad, s, &self.fonts);
            let content = height + 18.0 * s;
            let max = (content - view.h).max(0.0);
            self.city.phone.scroll = self.city.phone.scroll.clamp(0.0, max);
            let origin = Vec2::new(view.x + pad, view.y + 6.0 * s - self.city.phone.scroll);
            crate::nav_signon::draw_items(&mut crate::nav_duty::Pen { p: &mut list, atlas: &mut self.atlas, fonts: &self.fonts }, &page.items, &placed, origin, view, s);
            if max > 0.0 {
                let thumb = (view.h * view.h / content).max(24.0 * s);
                let y = view.y + (view.h - thumb) * self.city.phone.scroll / max;
                ui.rounded(Rect::new(view.right() - 6.0 * s, y, 3.0 * s, thumb), 1.5 * s, Color::WHITE.alpha(0.18));
            }
            self.city.phone_hits = crate::nav_signon::hits(&page.items, &placed, s).into_iter().map(|(r, a)| (Rect::new(r.x + origin.x, r.y + origin.y, r.w, r.h), a)).collect();
            self.city.phone_view = view;
            list_clip = [view.x, view.y, view.right(), view.bottom()];
        }
        if let (Left::Duty, Some(d)) = (left, duty) {
            let view = crate::nav_duty::draw_sheet_head(&mut crate::nav_duty::Pen { p: &mut ui, atlas: &mut self.atlas, fonts: &self.fonts }, d, sheet, f.time, s);
            let rows = crate::nav_duty::sheet(d);
            let content = (crate::nav_duty::sheet_height(&rows) + 12.0) * s;
            let max = (content - view.h).max(0.0);
            // (it follows the duty: when the trip or the stop changes it scrolls there, a
            // third down the list, as Omsi-Hub's live duty keeps the trip under way in view)
            let key = (d.focus(), d.next_stop, d.done);
            if self.city.sheet_at != Some(key) {
                self.city.sheet_at = Some(key);
                self.city.sheet_scroll = crate::nav_duty::sheet_focus(&rows) * s - view.h * 0.3;
            }
            self.city.sheet_scroll = self.city.sheet_scroll.clamp(0.0, max);
            crate::nav_duty::draw_sheet_list(&mut crate::nav_duty::Pen { p: &mut list, atlas: &mut self.atlas, fonts: &self.fonts }, &rows, view, self.city.sheet_scroll, s);
            if max > 0.0 {
                let thumb = (view.h * view.h / content).max(24.0 * s);
                let y = view.y + (view.h - thumb) * self.city.sheet_scroll / max;
                ui.rounded(Rect::new(view.right() - 6.0 * s, y, 3.0 * s, thumb), 1.5 * s, Color::WHITE.alpha(0.18));
            }
            list_clip = [view.x, view.y, view.right(), view.bottom()];
        }
        // the pins' card, and a note of theirs under the header (in the middle of the map the
        // column and the card leave)
        self.city.card_hits.clear();
        if card_rect.w > 0.0 {
            self.city.card_hits = crate::nav_pins::draw_card(&mut crate::nav_duty::Pen { p: &mut ui, atlas: &mut self.atlas, fonts: &self.fonts }, &card, card_rect, s);
        }
        if let Some((note, t)) = self.pins.note {
            let card_left = card_rect.w > 0.0 && card_rect.x < w * 0.5;
            let lo = sheet.right().max(if card_left { card_rect.right() } else { 0.0 });
            let hi = if card_rect.w > 0.0 && !card_left { card_rect.x } else { w };
            crate::nav_pins::draw_note(&mut crate::nav_duty::Pen { p: &mut ui, atlas: &mut self.atlas, fonts: &self.fonts }, note, Vec2::new((lo + hi) * 0.5, head_h + 34.0 * s), s, (t / 0.3).min(1.0));
        }
        // header: the line and where it goes, the next stop; the size and the buttons on the
        // right (the pin tool's; the duty sheet's only on a duty; the sign-on page's always)
        let buttons: &[(&str, u8)] = if f.duty.is_some() { &[("close", 3), ("zoom_out", 2), ("zoom_in", 1), ("my_location", 0), ("location_on", 9), ("view_list", 4), ("badge", 5)] } else { &[("close", 3), ("zoom_out", 2), ("zoom_in", 1), ("my_location", 0), ("location_on", 9), ("badge", 5)] };
        self.city_head(&mut ui, f, w, s, buttons, left, companion.stage, true);
        let pad = 16.0 * s;
        // a scale bar, bottom left
        let nice = [10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0];
        let metres = nice.iter().copied().find(|m| m / self.city.mpp > 70.0 * s as f64).unwrap_or(5000.0);
        let len = (metres / self.city.mpp) as f32;
        let by = h - 22.0 * s;
        // (right of the duty sheet when it is there)
        let sx = if sheet.w > 0.0 { sheet.right() + pad } else { pad };
        ui.rect(Rect::new(sx, by, len, 2.0 * s), TEXT_DIM);
        ui.text(&mut self.atlas, &self.fonts, &if metres >= 1000.0 { format!("{:.0} km", metres / 1000.0) } else { format!("{metres:.0} m") }, 12.0 * s, Weight::Medium, Vec2::new(sx + len + 8.0 * s, by + 4.0 * s), Align::Left, TEXT_DIM);
        ui.rounded_border(win, 10.0 * s, 1.0, Color::WHITE.alpha(0.08));
        CityPaint { bg, dots, ui, list, list_clip, roads: roads_verts, route: route_verts, world, anchor }
    }

    /// The city map as [`Navigator::paint_city`] painted it, into its texture and over the
    /// picture at `win` (x, y, width, height), at scale `s`.
    pub(super) fn city_to_gpu(&mut self, renderer: &Renderer, scene: &mut Scene, paint: CityPaint, win: [f32; 4], s: f32) {
        let [x0, y0, w, h] = win;
        let (tw, th) = (w as u32, h as u32);
        let CityPaint { bg, dots, ui, list, list_clip, roads: roads_verts, route: route_verts, world, .. } = paint;
        let (n_bg, n_dots) = (bg.len(), dots.len());
        let Some((tex, _, _)) = self.city.target else { return };
        let Some(view) = renderer.texture_view(scene, tex) else { return };
        let (Some(gpu), device, queue) = (self.gpu.as_mut(), &renderer.device, &renderer.queue) else { return };
        if let Some(v) = roads_verts {
            gpu.upload(device, queue, 3, &v);
        }
        if let Some(v) = route_verts {
            gpu.upload(device, queue, 4, &v);
        }
        let mut all = bg.verts;
        all.extend(dots.verts);
        let n_ui = all.len() as u32;
        all.extend(ui.verts);
        let n_list = all.len() as u32;
        all.extend(list.verts);
        gpu.upload(device, queue, 5, &all);
        gpu.upload_atlas(queue, &mut self.atlas);
        let flat = Layer::flat([0.0, 0.0, w, h], 10.0 * s, 1.0);
        let layers = [flat, world, Layer::flat(list_clip, 0.0, 1.0)];
        let roads_n = self.city.roads.map(|r| r.1).unwrap_or(0);
        let draws = [
            Draw { buffer: 5, range: 0..n_bg, layer: 0, texture: 0 },
            Draw { buffer: 3, range: 0..roads_n, layer: 1, texture: 0 },
            Draw { buffer: 4, range: 0..self.city.route.1, layer: 1, texture: 0 },
            Draw { buffer: 5, range: n_bg..n_bg + n_dots, layer: 1, texture: 0 },
            Draw { buffer: 5, range: n_ui..n_list, layer: 0, texture: 0 },
            Draw { buffer: 5, range: n_list..all.len() as u32, layer: 2, texture: 0 },
        ];
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("city map") });
        gpu.render(device, queue, &mut enc, &view, (tw, th), Some(wgpu::Color::TRANSPARENT), &layers, &draws);
        queue.submit([enc.finish()]);
        scene.overlays.push((tex, [x0, y0, x0 + w, y0 + h]));
    }

    /// The city map's header along the top of a view `w` pixels wide (opaque: the route and
    /// the stops showed through behind its text): the line and where it goes, the next stop
    /// (`next_stop`: on the map, not over the sign-on page), the navigator's size with its -
    /// and +, and the `buttons` on the right - lit for what shows (`left`), the sign-on
    /// page's with an amber dot while the duty is not signed and the page is put away. The
    /// buttons go in `self.city.buttons`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn city_head(&mut self, ui: &mut Painter, f: &NavFrame, w: f32, s: f32, buttons: &[(&str, u8)], left: Left, stage: Stage, next_stop: bool) {
        let head = Rect::new(0.0, 0.0, w, 44.0 * s);
        ui.rect(head, Color::rgba(20, 26, 38, 0.97));
        let pad = 16.0 * s;
        let wd = words(f.language);
        let title = match (&f.line, &f.terminus) {
            (Some(l), Some(t)) => format!("{}  ›  {}", l.trim(), t.trim()),
            _ => wd.no_duty.to_string(),
        };
        let title_w = ui.text_in(&mut self.atlas, &self.fonts, &title, 15.0 * s, Weight::Bold, Rect::new(pad, head.y, w * 0.4, head.h), Align::Left, TEXT);
        let bs = 30.0 * s;
        self.city.buttons.clear();
        let mut bx = w - pad - bs;
        for &(icon, id) in buttons {
            let r = Rect::new(bx, head.y + (head.h - bs) * 0.5, bs, bs);
            let on = (id == 0 && self.city.follow) || (id == 4 && left == Left::Duty) || (id == 5 && left == Left::SignOn) || (id == 9 && self.city.pin_tool);
            ui.rounded(r, 5.0 * s, if on { crate::nav_duty::accent() } else { Color::rgba(36, 44, 62, 1.0) });
            ui.icon(&mut self.atlas, icon, r.center(), 18.0 * s, TEXT);
            // (an amber dot while the duty is not signed and the page is put away)
            if id == 5 && !on && stage != Stage::OnDuty {
                ui.circle(Vec2::new(r.right() - 4.0 * s, r.y + 4.0 * s), 4.0 * s, crate::nav_duty::NOW);
            }
            self.city.buttons.push((r, id));
            bx -= bs + 8.0 * s;
        }
        // the navigator's size: - 100 % +, left of the buttons (Ctrl + the wheel does it too)
        let text = size_text(self.size);
        let tw = self.fonts.width("200 %", 12.5 * s, Weight::Bold).max(self.fonts.width(&text, 12.5 * s, Weight::Bold)) + 12.0 * s;
        let pill = Rect::new(bx + bs - 8.0 * s - (2.0 * bs + tw), head.y + (head.h - bs) * 0.5, 2.0 * bs + tw, bs);
        ui.rounded(pill, 5.0 * s, Color::rgba(36, 44, 62, 1.0));
        let (minus, plus) = (Rect::new(pill.x, pill.y, bs, bs), Rect::new(pill.right() - bs, pill.y, bs, bs));
        let ends = (omsi_launcher_lib::nav_scale(Some(0.0)) as f32, omsi_launcher_lib::nav_scale(Some(9.0)) as f32);
        for (r, icon, id, end) in [(minus, "remove", 7u8, ends.0), (plus, "add", 8, ends.1)] {
            // (greyed at the end of the range)
            let ink = if (self.size - end).abs() < 1e-3 { TEXT_DIM.alpha(0.5) } else { TEXT };
            ui.icon(&mut self.atlas, icon, r.center(), 18.0 * s, ink);
            self.city.buttons.push((r, id));
        }
        let lit = (0.0..SIZE_NOTE).contains(&(self.time - self.size_at));
        ui.text_in(&mut self.atlas, &self.fonts, &text, 12.5 * s, Weight::Bold, Rect::new(minus.right(), pill.y, tw, bs), Align::Center, if lit { TEXT } else { TEXT_DIM });
        // the next stop - or the player's own destination, or the diversion's next via
        let text = if !next_stop {
            None
        } else if let Some((t, c)) = self.pins_head() {
            Some((t, c))
        } else {
            f.stops.first().map(|st| {
                let d = self.next_dist.map(|d| if d >= 1000.0 { format!("{:.1} km", d / 1000.0) } else { format!("{:.0} m", d) }).unwrap_or_default();
                (format!("{}  |  {}  |  {:02}:{:02}", st.name.trim(), d, (st.arrival / 3600.0) as i32 % 24, ((st.arrival % 3600.0) / 60.0) as i32), TEXT_DIM)
            })
        };
        if let Some((t, c)) = text {
            // (up to the size and the buttons, not under them)
            let x = pad + title_w + 24.0 * s;
            let room = (pill.x - 16.0 * s - x).max(0.0);
            ui.text_in(&mut self.atlas, &self.fonts, &t, 13.5 * s, Weight::Medium, Rect::new(x, head.y, room.min(w * 0.45), head.h), Align::Left, c);
        }
    }

    /// The pins' card as the city map shows it (`nav_pins::draw_card`): each pin with how far
    /// along the route and how long to it, to the destination when there too; with no pins
    /// (the tool on) a diversion's when the navigator follows a trip.
    pub(super) fn pin_card(&self, f: &NavFrame) -> crate::nav_pins::Card {
        use crate::nav_pins as pins;
        let rows: Vec<pins::CardRow> = self
            .pins
            .list
            .iter()
            .enumerate()
            .map(|(k, p)| {
                let dist = self.pins.dist.get(k).copied().flatten();
                pins::CardRow { role: self.pins.role(k), name: p.name.clone(), dist, eta: dist.map(|d| pins::eta(d, self.speed_avg)) }
            })
            .collect();
        let total = rows.last().filter(|_| !self.pins.diversion()).and_then(|r| r.dist).map(|d| {
            let e = pins::eta(d, self.speed_avg);
            (d, e, f.time + e)
        });
        let on_trip = !self.route.key.is_empty() && self.route.key != DEST_KEY;
        pins::Card { diversion: self.pins.diversion() || (self.pins.list.is_empty() && on_trip), rows, total, arrived: self.pins.arrived.is_some(), tool: self.city.pin_tool }
    }

    /// What the city map's header says of the player's own pins: on a free drive the
    /// destination with how far and how long; on a duty the diversion and its next via.
    pub(super) fn pins_head(&self) -> Option<(String, Color)> {
        use crate::nav_pins as pins;
        let first = self.pins.list.first()?;
        if self.pins.arrived.is_some() {
            return Some((format!("{}  ·  {}", self.pins.list.last()?.name, omsi_ui::tr("You have arrived")), ON_TIME));
        }
        let dist = |k: usize| self.pins.dist.get(k).copied().flatten();
        if self.pins.diversion() {
            let d = dist(0).map(|d| format!("  ·  {}", pins::distance_text(d))).unwrap_or_default();
            return Some((format!("{}  ·  {}{d}", omsi_ui::tr("Diversion"), first.name), crate::nav_duty::NOW));
        }
        let last = self.pins.list.len() - 1;
        let parts = match dist(last) {
            Some(d) => format!("  ·  {}  ·  {}", pins::distance_text(d), pins::eta_text(pins::eta(d, self.speed_avg))),
            None => String::new(),
        };
        Some((format!("{}{parts}", self.pins.list[last].name), pins::PIN_LIGHT))
    }

    /// The city map's whole view as the sign-on page, painted for a window `w` x `h` at scale
    /// `s`: its background, the cards (to be clipped to the body under the header, which they
    /// scroll in) and the header with the scroll bar; and the body. Where a press does what is
    /// kept for [`Navigator::map_press`].
    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint_city_signon(&mut self, f: &NavFrame, companion: &crate::companion::CompanionState, duty: Option<&crate::nav_duty::DutyState>, w: f32, h: f32, s: f32) -> (Painter, Painter, Painter, Rect) {
        use crate::nav_signon as signon;
        let head_h = (44.0 * s).round();
        let body = Rect::new(0.0, head_h, w, (h - head_h).max(0.0));
        // (the whole body is the page's: a press there is no drag of the map, the wheel and a
        // drag scroll it)
        self.city.sheet = body;
        if let Some(r) = self.city.phone.wants(companion) {
            crate::companion::request(r);
        }
        let qr = self.pair_qr(companion);
        let page = signon::page_in(companion, &self.city.phone, duty, f.time, signon::Room::Full, qr.as_ref());
        let full = signon::full(&page, body, s, &self.fonts);
        let max = (full.height - body.h).max(0.0);
        self.city.phone.scroll = self.city.phone.scroll.clamp(0.0, max);
        // (scrolled by whole pixels: the QR code's modules stay sharp)
        let scroll = self.city.phone.scroll.round();
        let mut bg = Painter::new();
        bg.rounded(Rect::new(0.0, 0.0, w, h), 10.0 * s, Color::rgba(12, 16, 26, crate::ui::backdrop(self.opacity).min(1.0)));
        // (a soft light behind the cards: the page is what the view is about now)
        bg.radial(body.center(), body.w.min(body.h) * 0.5, crate::nav_duty::accent().alpha(0.08), Color::rgba(12, 16, 26, 0.0));
        let mut list = Painter::new();
        signon::draw_full(&mut crate::nav_duty::Pen { p: &mut list, atlas: &mut self.atlas, fonts: &self.fonts }, &page, &full, scroll, body, s);
        self.city.phone_hits = signon::full_hits(&page, &full, scroll, s);
        self.city.phone_view = body;
        let mut ui = Painter::new();
        if max > 0.0 {
            let thumb = (body.h * body.h / full.height).max(24.0 * s);
            let y = body.y + (body.h - thumb) * scroll / max;
            ui.rounded(Rect::new(body.right() - 8.0 * s, y, 3.0 * s, thumb), 1.5 * s, Color::WHITE.alpha(0.18));
        }
        // the header: the duty's line, the size, the map instead of the page, closing
        self.city_head(&mut ui, f, w, s, &[("close", 3), ("map", 6)], Left::SignOn, companion.stage, false);
        ui.rounded_border(Rect::new(0.0, 0.0, w, h), 10.0 * s, 1.0, Color::WHITE.alpha(0.08));
        (bg, list, ui, body)
    }

    /// The city map as the sign-on page, its whole view (while the duty waits to be signed
    /// for): the header, and under it the page in a card with the devices' card beside it -
    /// scrolled with the wheel or a drag when they are taller than the view. The window is
    /// at `win` (x, y, width, height), its interface at scale `s`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn city_signon(&mut self, renderer: &Renderer, scene: &mut Scene, f: &NavFrame, companion: &crate::companion::CompanionState, duty: Option<&crate::nav_duty::DutyState>, win: [f32; 4], s: f32) {
        let [x0, y0, w, h] = win;
        let (bg, list, ui, body) = self.paint_city_signon(f, companion, duty, w, h, s);
        let (tw, th) = (w as u32, h as u32);
        let Some((tex, _, _)) = self.city.target else { return };
        let Some(view) = renderer.texture_view(scene, tex) else { return };
        let (Some(gpu), device, queue) = (self.gpu.as_mut(), &renderer.device, &renderer.queue) else { return };
        let n_bg = bg.len();
        let mut all = bg.verts;
        let n_list = all.len() as u32;
        all.extend(list.verts);
        let n_ui = all.len() as u32;
        all.extend(ui.verts);
        gpu.upload(device, queue, 5, &all);
        gpu.upload_atlas(queue, &mut self.atlas);
        let layers = [Layer::flat([0.0, 0.0, w, h], 10.0 * s, 1.0), Layer::flat([body.x, body.y, body.right(), body.bottom()], 0.0, 1.0)];
        let draws = [
            Draw { buffer: 5, range: 0..n_bg, layer: 0, texture: 0 },
            Draw { buffer: 5, range: n_list..n_ui, layer: 1, texture: 0 },
            Draw { buffer: 5, range: n_ui..all.len() as u32, layer: 0, texture: 0 },
        ];
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("city map sign-on") });
        gpu.render(device, queue, &mut enc, &view, (tw, th), Some(wgpu::Color::TRANSPARENT), &layers, &draws);
        queue.submit([enc.finish()]);
        scene.overlays.push((tex, [x0, y0, x0 + w, y0 + h]));
    }
}
