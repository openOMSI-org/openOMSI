//! The game's own interface, drawn over the picture in Inter with a dark outline (the
//! HUD's `.oft` fonts are OMSI's and stay for the time and speed): the chat of a LAN
//! session, the name of what the cursor points at next to the cursor, and the other
//! players' name tags above their buses.
//!
//! Every text is rendered once into a small texture and kept while it is shown; the
//! overlays are rectangles in physical pixels (`Scene::overlays`).

use ab_glyph::{Font, FontVec, PxScale, ScaleFont, VariableFont};
use omsi_render::{Renderer, Scene, TextureId};

mod chat;
mod hud;
pub(crate) use hud::{SIZE_KEYS, window_size};
mod information_bar;
mod loading;
mod menu;
mod perf_panel;
mod text;
mod timetable;

pub use text::TextCache;
mod mz;
pub use information_bar::{InfoBar, InfoItem, InfoItemStyle};
use menu::*;
pub use mz::{MzState, MzView};
use text::*;

// ---------------------------------------------------------------------------------------
// the chat

/// How many rows the chat shows (a long line takes several; the lines it keeps to scroll
/// back: `omsi_lan::game::CHAT_KEEP`).
const CHAT_SHOWN: usize = 8;

/// A notification: a card over the navigator that goes by itself - from the server
/// (`notify`), the plugins, the update check, and what the game tells the driver to do
/// (`NoteWatch`).
#[derive(Clone, Debug, PartialEq)]
pub struct Notice {
    pub kind: NoticeKind,
    pub text: String,
    /// Seconds it still shows, of `total`.
    pub left: f32,
    pub total: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NoticeKind {
    Info,
    Warn,
    Alert,
    /// A new openOMSI version (`update_watch`).
    Update,
    /// Something the driver has to do (release the parking brake, start the engine, give
    /// the change): in the accent.
    Action,
    /// Something done (the trip's end, a setting saved).
    Success,
}

impl NoticeKind {
    /// The kind's colour (the launcher's: neutral for information, the accent for what to
    /// do, red for an alert, green for done, blue for an update) and its Material icon.
    pub fn look(self) -> ([u8; 3], &'static str) {
        match self {
            NoticeKind::Info => ([142, 142, 147], "info"),
            NoticeKind::Warn => ([235, 170, 60], "warning"),
            NoticeKind::Alert => ([255, 69, 58], "error"),
            NoticeKind::Update => ([47, 124, 246], "system_update"),
            NoticeKind::Action => ([235, 170, 60], "touch_app"),
            NoticeKind::Success => ([47, 168, 79], "check_circle"),
        }
    }

    /// How long a card of this kind stays when the game shows it on its own (seconds): what
    /// to do and an alert a little longer, to be read while driving.
    pub fn secs(self) -> f32 {
        match self {
            NoticeKind::Action => 7.0,
            NoticeKind::Alert => 8.0,
            NoticeKind::Warn => 6.0,
            _ => 5.0,
        }
    }
}

/// How many notifications show at once (the newest; an older one fades to make room).
pub const NOTICES_SHOWN: usize = 4;

/// How long a card takes to come in and to go (seconds).
const NOTICE_IN: f32 = 0.3;
const NOTICE_OUT: f32 = 0.4;

impl Notice {
    /// `notify`'s argument: `<id> <seconds> <info|warn|alert> <text>` → the id and the notice
    /// (seconds held to 2 .. 30; an unknown kind is information).
    pub fn parse(arg: &str) -> Option<(String, Notice)> {
        let mut it = arg.trim().splitn(4, ' ');
        let id = it.next().filter(|s| !s.is_empty())?.to_string();
        let secs: f32 = it.next()?.parse().ok().filter(|s: &f32| s.is_finite())?;
        let kind = match it.next()? {
            "warn" => NoticeKind::Warn,
            "alert" => NoticeKind::Alert,
            "action" => NoticeKind::Action,
            "success" | "ok" => NoticeKind::Success,
            _ => NoticeKind::Info,
        };
        let text = it
            .next()
            .map(str::trim)
            .filter(|t| !t.is_empty())?
            .to_string();
        let total = secs.clamp(2.0, 30.0);
        Some((
            id,
            Notice {
                kind,
                text,
                left: total,
                total,
            },
        ))
    }

    /// A notice of the game's own: `kind`'s time.
    pub fn new(kind: NoticeKind, text: impl Into<String>) -> Notice {
        let total = kind.secs();
        Notice {
            kind,
            text: text.into(),
            left: total,
            total,
        }
    }

    /// How far it is there (0 to 1, eased): coming in over `NOTICE_IN`, going over its last
    /// `NOTICE_OUT` - its opacity, and how much room it takes in the stack, so that the
    /// cards round it slide instead of jumping.
    pub fn alpha(&self) -> f32 {
        let k = (self.left / NOTICE_OUT)
            .min((self.total - self.left) / NOTICE_IN)
            .clamp(0.0, 1.0);
        k * k * (3.0 - 2.0 * k)
    }
}

/// A new notice in the list. The same text still on the screen stays its time again instead
/// of standing twice; with more than `NOTICES_SHOWN` the oldest fades out to make room.
pub fn push_notice(list: &mut Vec<Notice>, n: Notice) {
    if let Some(o) = list
        .iter_mut()
        .find(|o| o.kind == n.kind && o.text == n.text && o.left > NOTICE_OUT)
    {
        // (its age kept: it does not fade in once more)
        o.total = o.total - o.left + n.total;
        o.left = n.total;
        return;
    }
    list.push(n);
    let mut live = list.iter().filter(|o| o.left > NOTICE_OUT).count();
    for o in list.iter_mut() {
        if live <= NOTICES_SHOWN {
            break;
        }
        if o.left > NOTICE_OUT {
            o.left = NOTICE_OUT;
            live -= 1;
        }
    }
    while list.len() > NOTICES_SHOWN + 3 {
        list.remove(0);
    }
}

/// Turns what the game tells the driver every frame (why the bus does not move, the change
/// due, a message) into notifications: a line becomes a card when it starts to be true, once,
/// and again only after it was gone for `NoteWatch::REARM` seconds - the throttle let go and
/// pressed again a moment later is the same reason, not a new one.
#[derive(Default)]
pub struct NoteWatch {
    /// When each line (by its key) was last there.
    seen: hashbrown::HashMap<String, f32>,
    now: f32,
}

impl NoteWatch {
    pub const REARM: f32 = 3.0;

    /// This frame's lines: (key, text, kind). The key names the condition (its text may
    /// change with a value in it, a pressure or a sum, without being a new one).
    pub fn frame(
        &mut self,
        dt: f32,
        lines: &[(String, String, NoticeKind)],
        list: &mut Vec<Notice>,
    ) {
        self.now += dt.max(0.0);
        for (key, text, kind) in lines {
            let fresh = self
                .seen
                .get(key)
                .is_none_or(|&t| self.now - t > Self::REARM);
            self.seen.insert(key.clone(), self.now);
            if fresh && !text.trim().is_empty() {
                push_notice(list, Notice::new(*kind, text.clone()));
            }
        }
        let now = self.now;
        self.seen.retain(|_, t| now - *t <= Self::REARM);
    }
}

/// The part of a line of the game that names its condition: up to its first bracket or
/// colon ("Parking brake is on  (. releases it)", "Air pressure is low (4.1 bar ...)").
pub fn note_key(text: &str) -> String {
    text.split(['(', ':'])
        .next()
        .unwrap_or(text)
        .trim()
        .to_string()
}

/// What the chat widget needs from the session each frame.
pub struct ChatView<'a> {
    /// Every line, oldest first ("Name: text" or "* notice").
    pub lines: &'a [String],
    /// The line being typed (the input box is open).
    pub typing: Option<&'a str>,
    /// Why the last line was not sent.
    pub error: Option<&'a str>,
    /// The key that opens the line (`chat_open`, which the player can move), as it reads.
    pub open_key: Option<String>,
}

/// The chat's own state: shown or hidden (V), the scroll position and whether the cursor is
/// over it.
#[derive(Default)]
pub struct ChatWidget {
    pub hidden: bool,
    /// Lines scrolled back from the newest.
    pub scroll: usize,
    /// The chat's box on the screen (physical pixels) as drawn last: hovering over it shows
    /// the input box, a click there opens it.
    pub rect: [f32; 4],
    pub hovered: bool,
    pub caret_t: f32,
    translations: chat::TranslationCache,
}

impl ChatWidget {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.rect[0] && x <= self.rect[2] && y >= self.rect[1] && y <= self.rect[3]
    }

    /// The mouse wheel over the chat (or while typing) scrolls the history.
    pub fn wheel(&mut self, lines: usize, amount: f32) {
        let max = lines.saturating_sub(CHAT_SHOWN);
        let s = self.scroll as i32 + amount.round() as i32;
        self.scroll = s.clamp(0, max as i32) as usize;
    }
}

/// How much larger than designed the interface is drawn, on top of the screen's scale
/// `dpi`: on a window taller than 1080 logical pixels as much as it is taller (up to twice),
/// times the player's `size` (`Settings::ui_scale`). Laid out for 1080p, the texts were half
/// their size on a 4K screen at 100 % display scaling; a window of 1080 lines or fewer is
/// drawn as it always was. With `window` off (`Settings::ui_scale_window`) it does not grow
/// with the window at all.
pub fn size_factor(height_px: f32, dpi: f32, size: f32, window: bool) -> f32 {
    let grown = if window {
        (height_px / dpi.max(0.5) / 1080.0).clamp(1.0, 2.0)
    } else {
        1.0
    };
    grown * size
}

pub use omsi_ui::theme::backdrop;

/// Which menu is open: its layout follows from it.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum MenuKind {
    /// The game menu itself.
    #[default]
    Game,
    /// A settings window (options, vehicle, world): a sidebar of pages and rows with switches,
    /// sliders and buttons. A row's label is `name\u{1f}kind\u{1f}value\u{1f}description\u{1f}fraction`
    /// (kind: `s` switch, `v` slider, `c` stepper, `o` opens a list, `a` button, `i` information).
    Options,
    /// A list grouped under headings, using the settings page's section layout without its search.
    Sections,
    /// The lines of the map's timetable.
    Lines,
    /// One line's tours.
    Tours,
    /// Vehicle placement and swapping with the launcher showroom.
    Vehicles,
    /// Any other list (drivers, fleet numbers, destinations, liveries ...).
    List,
}

/// The timetable beside a list of lines or tours: a title, a line of facts and rows of
/// (what, time).
pub struct Preview {
    pub title: String,
    pub meta: String,
    pub rows: Vec<(String, String)>,
    /// The row chosen, when the rows can be chosen (the stops to start a tour from).
    pub chosen: Option<usize>,
    /// The button under the rows, when there is one.
    pub button: Option<String>,
    /// The time of the timetable the rows are of, with buttons to put it ahead (the trip of
    /// a tour to take on later), when it can be set.
    pub time: Option<String>,
}

/// The timetable window of the trip driven: its line and terminus, the tour and trip, how
/// late, the stops, and the trip after it.
pub struct TimetableView {
    pub line: String,
    pub terminus: String,
    /// "Tour 1 · Trip 17/72".
    pub meta: String,
    /// Seconds late (early: negative).
    pub delay: f64,
    pub rows: Vec<TimetableRow>,
    /// The next trip: its line and terminus, and when it leaves.
    pub next: Option<(String, String)>,
}

/// A stop of the timetable window: its name, arrival and departure ("" where there is none),
/// and 0 served, 1 next, 2 ahead.
pub struct TimetableRow {
    pub name: String,
    pub arr: String,
    pub dep: String,
    pub expected: String,
    pub state: u8,
}

/// A drop-down open over a row of a settings window: the entries, the one the keyboard is
/// on, the first one shown and the one in force now.
pub struct DropdownView<'a> {
    pub row: usize,
    pub items: Vec<&'a str>,
    pub sel: usize,
    pub top: usize,
    pub current: Option<usize>,
}

/// Everything the interface draws in a frame.
pub struct Frame<'a> {
    /// Physical pixels per logical one.
    pub scale: f32,
    /// How much larger than designed (`size_factor`): everything below is drawn this much
    /// larger on top of `scale`.
    pub ui_scale: f32,
    /// How strongly the backgrounds are drawn (`backdrop`): the menu's and the timetable's
    /// panels, the chat's box, the plates under the notes, the tooltip and the frame rate.
    pub opacity: f32,
    pub light: bool,
    pub sizes: [f32; 3],
    pub width: f32,
    pub height: f32,
    pub cursor: (f32, f32),
    /// The mouse steering's cross, while the system cursor is hidden and held (where the
    /// crosshair cursor stood before): drawn here instead.
    pub steer_cross: Option<(f32, f32)>,
    /// The tooltip goes above the cursor rather than right of it: the cursor is a finger,
    /// whose hand covers what is below and beside it.
    pub tooltip_above: bool,
    /// An OpenXR headset is drawing this frame.
    pub vr: bool,
    /// The name of what the cursor points at (a switch, a part), shown next to it.
    pub tooltip: Option<String>,
    /// The chat, when a LAN session runs and the chat is not switched off.
    pub chat: Option<ChatView<'a>>,
    /// The chat's own size on top of `ui_scale` (`Settings::chat_size`).
    pub chat_size: f32,
    /// What the driver has to act on (why the bus does not move, a passenger's wish, the
    /// change due, a service done), top left.
    pub notes: &'a [String],
    /// The frame rate, top right (the `show_fps` setting).
    pub fps: Option<f32>,
    /// The frames frame generation added to them in that second: the corner then reads `90 fps (45 + FG)`.
    pub fps_generated: f32,
    pub perf: Option<&'a crate::perf_overlay::PerfOverlay>,
    pub perf_detailed: bool,
    pub perf_touch: Option<Option<f32>>,
    /// The game stands paused.
    pub paused: bool,
    /// The game menu is open, with this line chosen (labels from `GAME_MENU`).
    pub menu: Option<(usize, &'a [(&'a str, &'a str)])>,
    /// The first line shown when a finger scrolled the menu (`App::menu_top`).
    pub menu_top: Option<f32>,
    /// A seat or view slider of the settings window held by the mouse (its line): only that
    /// line is drawn, over the undimmed picture, so the view can be seen changing (#2359).
    pub menu_peek: Option<usize>,
    /// Ids of the game menu's lines that are greyed out and cannot be chosen (the timetable
    /// without an active route).
    pub menu_disabled: &'a [&'a str],
    /// The timetable window (Insert).
    pub timetable: Option<TimetableView>,
    /// The information bar along the top, split between driving details and vehicle data.
    pub info: Option<InfoBar>,
    /// The room the information bar has along the top: from x to x, from y down (on a
    /// touch screen the gap between its buttons); none: the window's width.
    pub info_room: Option<[f32; 3]>,
    /// A tutorial page: title, text, picture, page number and count.
    pub tutorial: Option<(&'a str, &'a str, Option<&'a std::path::Path>, usize, usize)>,
    /// Name tags: a screen position (the point above a bus), the name and a second line.
    pub tags: Vec<((f32, f32), String, String, f32)>,
    /// The server's notifications, oldest first.
    pub notices: &'a [Notice],
    /// Where the navigator is on the screen (the notifications stand over it, or under it
    /// in a top corner); none: they go to the top middle.
    pub notice_anchor: Option<[f32; 4]>,
    /// The MileZero card over the navigator (setting `milezero`).
    pub milezero: Option<&'a MzView>,
    /// The navigator's own rectangle, for the MileZero card to stand by.
    pub nav_rect: Option<[f32; 4]>,
    /// What kind of menu the lines belong to.
    pub menu_kind: MenuKind,
    /// The open list's title and the small line above it (the line a tour list is of).
    pub menu_head: Option<(String, String)>,
    /// The timetable of the chosen line or tour, beside the list.
    pub menu_preview: Option<Preview>,
    /// The first stop shown of the timetable beside the tours, when the wheel has scrolled it
    /// (`None`: the stop chosen is kept in view).
    pub pane_first: Option<usize>,
    /// The pages of an open settings window (their titles) and the one shown.
    pub menu_tabs: Option<(Vec<String>, usize)>,
    /// The keyboard chose the menu's line last: that line is shown lit (else only the one
    /// under the mouse is).
    pub menu_kbd: bool,
    /// The drop-down open over a row of the settings window.
    pub dropdown: Option<DropdownView<'a>>,
}

pub struct Ui {
    origin_x: f32,
    pub text: TextCache,
    pub chat: ChatWidget,
    /// Where the game menu's lines were drawn this frame (physical pixels), for the mouse.
    pub menu_rects: Vec<[f32; 4]>,
    /// Per line of `menu_rects`, where the arrows round its value are (a list's setting,
    /// `game_lists::ADJUST`): `[from, to, plus]` - a click from `from` to `to` steps it
    /// down, one right of `plus` up.
    pub menu_arrows: Vec<Option<[f32; 3]>>,
    pub menu_scroll_thumb: Option<[f32; 4]>,
    pub menu_scroll_track: Option<[f32; 4]>,
    /// Where the controls of the settings rows were drawn (a slider's track, a stepper), one
    /// entry per line in `menu_rects`: a click there sets the value.
    pub menu_ctl: Vec<Option<[f32; 4]>>,
    /// The sidebar of a settings window: one box per page, the way back last.
    pub menu_side: Vec<[f32; 4]>,
    /// The rows of the timetable beside a line's tours (the stops to start from), the stop the
    /// first of them is, and the button that starts the trip.
    pub menu_pane: Vec<[f32; 4]>,
    pub menu_pane_start: usize,
    pub menu_pane_go: Option<[f32; 4]>,
    /// The whole timetable pane beside the tours: the wheel over it scrolls its stops.
    pub menu_pane_box: Option<[f32; 4]>,
    /// Its scroll bar when it has more stops than it shows: the track, the thumb (widened
    /// to be hit), how many stops there are and how many it shows - for the mouse to drag.
    pub menu_pane_scroll: Option<([f32; 4], [f32; 4], usize, usize)>,
    /// The two arrows beside the time of a tour: the trip before, the next one.
    pub menu_time: Vec<[f32; 4]>,
    /// The colours and positions of the menu's parts that ease to their new state (a line's
    /// light, a switch's knob ...), by what they belong to.
    anim: std::collections::HashMap<u64, f32>,
    /// Seconds since the last frame, for `ease`.
    anim_dt: f32,
    /// Overlay entries belonging to the game menu.
    pub menu_overlay_range: std::ops::Range<usize>,
    /// The pointer texture, positioned separately for each headset eye.
    pub vr_cursor_overlay: Option<usize>,
    pub vr_tooltip_overlay: Option<usize>,
    /// The first line of the menu shown (a long menu scrolls: `menu_rects[k]` is line
    /// `menu_start + k`).
    pub menu_start: usize,
    /// How many lines the menu shows at once, and how high one is (physical pixels): a
    /// finger's drag is turned into lines with it.
    pub menu_rows: usize,
    pub menu_row_h: f32,
    /// The entries of the drop-down shown (their rects), the first of them, and how many fit.
    pub dd_rects: Vec<[f32; 4]>,
    pub dd_top: usize,
    pub dd_rows: usize,
    /// The drop-down's scroll bar when it has more entries than it shows: its track and
    /// its thumb (widened to be hit), for the mouse to drag.
    pub dd_scroll: Option<([f32; 4], [f32; 4])>,
    /// Pictures shown in the interface (a tutorial page's), by file.
    images: hashbrown::HashMap<std::path::PathBuf, Option<(TextureId, u32, u32)>>,
    /// Where the information bar was drawn (within the panel, before `origin_x`), for the
    /// navigator to keep out of its way.
    pub info_rect: Option<[f32; 4]>,
    perf: perf_panel::PerfPanel,
    /// The MileZero card (see `mz.rs`).
    pub mz: MzState,
    hud: hud::Hud,
    grips: [Option<([f32; 4], f32, f32)>; 3],
    caps: [f32; 3],
    /// When the loading reached each mark of a theme's loading logo (by the loading screen's
    /// clock): they pop in from then (`omsi_ui::icons::Stage`).
    logo_marks: hashbrown::HashMap<String, f32>,
}

impl Ui {
    /// Lay out at panel resolution, then move both pixels and mouse targets to
    /// that panel's position in the spanning window.
    pub fn draw_at(&mut self, r: &Renderer, scene: &mut Scene, f: &Frame, dt: f32, origin_x: f32) {
        self.shift_hitboxes(-self.origin_x);
        let start = scene.overlays.len();
        self.draw(r, scene, f, dt);
        scene.shift_overlays(start, origin_x);
        self.shift_hitboxes(origin_x);
        self.origin_x = origin_x;
    }

    fn shift_hitboxes(&mut self, x: f32) {
        for rect in self
            .menu_rects
            .iter_mut()
            .chain(self.menu_side.iter_mut())
            .chain(self.menu_pane.iter_mut())
            .chain(self.menu_time.iter_mut())
            .chain(self.dd_rects.iter_mut())
            .chain(self.menu_ctl.iter_mut().flatten())
            .chain(self.menu_scroll_thumb.iter_mut())
            .chain(self.menu_scroll_track.iter_mut())
            .chain(self.menu_pane_go.iter_mut())
            .chain(self.menu_pane_box.iter_mut())
            .chain(self.grips.iter_mut().flatten().map(|g| &mut g.0))
        {
            rect[0] += x;
            rect[2] += x;
        }
        for arrows in self.menu_arrows.iter_mut().flatten() {
            for at in arrows {
                *at += x;
            }
        }
        if let Some((track, thumb)) = &mut self.dd_scroll {
            for rect in [track, thumb] {
                rect[0] += x;
                rect[2] += x;
            }
        }
        if let Some((track, thumb, _, _)) = &mut self.menu_pane_scroll {
            for rect in [track, thumb] {
                rect[0] += x;
                rect[2] += x;
            }
        }
        self.chat.rect[0] += x;
        self.chat.rect[2] += x;
        for rect in self
            .mz
            .rect
            .iter_mut()
            .chain(self.mz.play.iter_mut())
            .chain(self.mz.request.iter_mut())
        {
            rect[0] += x;
            rect[2] += x;
        }
    }
    pub fn new() -> Option<Ui> {
        Some(Ui {
            origin_x: 0.0,
            text: TextCache::new()?,
            chat: ChatWidget::default(),
            menu_rects: Vec::new(),
            menu_arrows: Vec::new(),
            menu_scroll_thumb: None,
            menu_scroll_track: None,
            menu_ctl: Vec::new(),
            dd_rects: Vec::new(),
            dd_top: 0,
            dd_rows: 8,
            dd_scroll: None,
            menu_side: Vec::new(),
            menu_pane: Vec::new(),
            menu_pane_start: 0,
            menu_pane_go: None,
            menu_pane_box: None,
            menu_pane_scroll: None,
            menu_time: Vec::new(),
            anim: Default::default(),
            anim_dt: 0.0,
            menu_overlay_range: 0..0,
            vr_cursor_overlay: None,
            vr_tooltip_overlay: None,
            menu_start: 0,
            menu_rows: 0,
            menu_row_h: 1.0,
            images: Default::default(),
            info_rect: None,
            perf: Default::default(),
            mz: Default::default(),
            hud: hud::Hud::new(false),
            grips: [None; 3],
            caps: [f32::MAX; 3],
            logo_marks: Default::default(),
        })
    }

    /// Draw the frame's interface: its overlays go after the HUD's in `scene.overlays`.
    #[rustfmt::skip]
    pub fn draw(&mut self, r: &Renderer, scene: &mut Scene, f: &Frame, dt: f32) {
        let s = f.scale.max(0.5) * f.ui_scale;
        self.text.backdrop = f.opacity;
        self.hud = hud::Hud::new(f.light);
        self.grips = [None; 3];
        // --- name tags above the other players' buses
        for ((x, y), name, sub, alpha) in &f.tags {
            let a = (alpha.clamp(0.0, 1.0) * 255.0) as u8;
            let l = self.text.label(r, scene, name, (19.0 * s) as u32, [255, 255, 255, 220]);
            let x0 = x - l.w as f32 * 0.5;
            let y0 = y - l.h as f32;
            if a > 0 {
                scene.overlays.push((l.tex, [x0, y0, x0 + l.w as f32, y0 + l.h as f32]));
                if !sub.is_empty() {
                    let m = self.text.label(r, scene, sub, (12.0 * s) as u32, [210, 225, 255, 200]);
                    let mx = x - m.w as f32 * 0.5;
                    scene.overlays.push((m.tex, [mx, y0 + l.h as f32 - 3.0 * s, mx + m.w as f32, y0 + l.h as f32 - 3.0 * s + m.h as f32]));
                }
            }
        }
        // the top of the windows in the corners: the notes on the left start on the line of
        // the timetable and a tutorial page on the right (14 px down, they stood higher)
        let corner_top = 60.0 * s;
        self.draw_info_bar(r, scene, f, s);
        self.draw_milezero(r, scene, f, s, dt);
        // --- the notes, top left: only the editors' keys while one is on (what the driver
        // has to do comes as a notification): one flat card, the lines in it
        let notes_bottom = {
            let px = (13.0 * s).round() as u32;
            let x0 = 16.0 * s;
            // (below the on-screen buttons of a phone, which keep their size when the
            // interface is made smaller)
            let y0 = if crate::platform::touch_controls() { (80.0 * f.scale.max(0.5) * f.ui_scale.max(1.0)).max(corner_top) } else { corner_top };
            let lines: Vec<String> = f.notes.iter().filter(|n| !n.trim().is_empty()).take(8).map(|n| clip_to(&self.text, &omsi_ui::tr(n), px as f32, f.width * 0.6)).collect();
            if lines.is_empty() {
                y0
            } else {
                let lh = (px as f32 * 1.5).round();
                let (pad_x, pad_y) = (12.0 * s, 8.0 * s);
                let w = lines.iter().map(|t| self.text.width_raw(t, px as f32)).fold(0.0f32, f32::max) + pad_x * 2.0;
                let y0 = self.below_info(x0, x0 + w, y0, s);
                let h = pad_y * 2.0 + lh * lines.len() as f32;
                self.hud_card(r, scene, [x0, y0, x0 + w, y0 + h], hud::RADIUS * s, self.text.backdrop, s);
                for (i, t) in lines.iter().enumerate() {
                    let l = self.text.label_op(r, scene, t, px, hud::text(self.hud.dim), 1.0);
                    let ly = (y0 + pad_y + lh * (i as f32 + 0.5) - l.h as f32 * 0.5).round();
                    scene.overlays.push((l.tex, [x0 + pad_x, ly, x0 + pad_x + l.w as f32, ly + l.h as f32]));
                }
                y0 + h
            }
        };
        // --- the chat, top left under the notes, as Roblox has it (from the fourth note
        // on they ran into it)
        if let Some(c) = f.chat.as_ref().filter(|_| !self.chat.hidden) {
            // (its own size on top of the interface's: Ctrl + the wheel over it)
            let s = s * f.chat_size.clamp(0.5, 3.0);
            let px = (17.0 * s) as u32;
            let lh = px as f32 * 1.35;
            let x0 = 14.0 * s;
            let y0 = (96.0 * s).max(notes_bottom + 10.0 * s);
            let width = (460.0 * s).min(f.width * 0.6);
            let open = c.typing.is_some();
            let n = c.lines.len();
            let end = n.saturating_sub(if open || self.chat.hovered { self.chat.scroll } else { 0 });
            let box_h = lh * CHAT_SHOWN as f32 + lh * 1.6;
            self.chat.rect = [x0 - 6.0 * s, y0 - 6.0 * s, x0 + width, y0 + box_h];
            self.chat.hovered = self.chat.contains(f.cursor.0, f.cursor.1);
            let show_box = open || self.chat.hovered;
            // the lines, newest at the bottom of the history area; a line too long for the box
            // goes on in rows under it, indented (the oldest line shown may lose its first rows)
            let indent = 14.0 * s;
            let mut rows: Vec<(f32, String, [u8; 4])> = Vec::new();
            {
                let tc = &self.text;
                let measure = |t: &str| tc.width_raw(t, px as f32);
                let language = omsi_ui::i18n::language();
                let mut i = end;
                while i > 0 && rows.len() < CHAT_SHOWN {
                    i -= 1;
                    let line = &c.lines[i];
                    let color = if line.starts_with("* ") { [255, 226, 140, 230] } else { [255, 255, 255, 230] };
                    let line = self.chat.translations.line(line, &language, |key| {
                        crate::_rust_i18n_try_translate(&language, key).map(|text| text.into_owned())
                    });
                    for (k, t) in wrap_rows(line.as_ref(), width, indent, &measure).into_iter().enumerate().rev() {
                        if rows.len() == CHAT_SHOWN {
                            break;
                        }
                        rows.push((if k == 0 { 0.0 } else { indent }, t, color));
                    }
                }
                rows.reverse();
            }
            let mut y = y0 + lh * (CHAT_SHOWN - rows.len()) as f32;
            for (dx, text, color) in rows {
                let l = self.text.label_raw(r, scene, &text, px, color);
                scene.overlays.push((l.tex, [x0 + dx, y, x0 + dx + l.w as f32, y + l.h as f32]));
                y += lh;
            }
            if show_box {
                // the input box: shown when the cursor is over the chat or it is typed into
                let by = y0 + lh * CHAT_SHOWN as f32 + lh * 0.2;
                let bh = lh * 1.25;
                self.hud_card(r, scene, [x0 - 4.0 * s, by, x0 + width, by + bh], bh * 0.5, self.text.backdrop, s);
                self.chat.caret_t += dt;
                let caret = if open && (self.chat.caret_t % 1.0) < 0.55 { "|" } else { "" };
                let (text, color) = match c.typing {
                    Some(t) => (format!("{t}{caret}"), hud::text(self.hud.ink)),
                    None => match &c.open_key {
                        Some(k) => (format!("Click here or press {} to chat", k), hud::text(self.hud.dim)),
                        // chat_open unbound: no "(unbound)" in the hint
                        None => ("Click here to chat".to_string(), hud::text(self.hud.dim)),
                    },
                };
                let text = clip_left(&self.text, &text, px as f32, width - 10.0 * s);
                let l = self.text.label(r, scene, &text, px, color);
                let ty = by + (bh - l.h as f32) * 0.5;
                scene.overlays.push((l.tex, [x0 + 2.0 * s, ty, x0 + 2.0 * s + l.w as f32, ty + l.h as f32]));
                if self.chat.scroll > 0 && (open || self.chat.hovered) {
                    let m = self.text.label(r, scene, &omsi_ui::tr("{n} newer below").replace("{n}", &self.chat.scroll.to_string()), (11.0 * s) as u32, [200, 200, 200, 200]);
                    scene.overlays.push((m.tex, [x0 + width - m.w as f32, by - m.h as f32, x0 + width, by]));
                }
            }
            if let Some(e) = c.error {
                let l = self.text.label(r, scene, &format!("{}: {}", omsi_ui::tr("Not sent"), omsi_ui::tr(e)), (12.0 * s) as u32, [255, 150, 150, 220]);
                let ey = y0 + box_h;
                scene.overlays.push((l.tex, [x0, ey, x0 + l.w as f32, ey + l.h as f32]));
            }
        } else {
            self.chat.hovered = false;
            self.chat.rect = [0.0; 4];
        }
        // --- the notifications: cards over the navigator (under it when it is in a top
        // corner; at the top middle without it)
        self.draw_notices(r, scene, f, s);
        if let Some(fps) = f.fps {
            // (on a small flat card, as the other plates: over a bright sky bare text was hard to read)
            let l = self.text.label_op(r, scene, &if f.fps_generated >= 1.0 { format!("{:.0} fps ({fps:.0} + FG)", fps + f.fps_generated) } else { format!("{fps:.0} fps") }, (12.0 * s).round() as u32, hud::text(self.hud.dim), 1.0);
            let x = (f.width - l.w as f32 - 14.0 * s).round();
            let y = (10.0 * s).round();
            self.hud_card(r, scene, [x - 8.0 * s, y, x + l.w as f32 + 8.0 * s, y + l.h as f32], hud::RADIUS * s, self.text.backdrop, s);
            scene.overlays.push((l.tex, [x, y, x + l.w as f32, y + l.h as f32]));
        }
        self.draw_perf(r, scene, f, s, dt);
        let tutorial_w = (420.0 * s).min(f.width * 0.42);
        // --- the timetable window, on the right (left of a tutorial page, which has the
        // same corner)
        if let Some(t) = f.timetable.as_ref() {
            let beside = if f.tutorial.is_some() { tutorial_w + 12.0 * s } else { 0.0 };
            self.draw_timetable(r, scene, f, t, corner_top, beside, s);
        }
        // --- a tutorial page, on the right
        if let Some((title, text, image, at, count)) = f.tutorial {
            let w = tutorial_w;
            let x = f.width - w - 16.0 * s;
            let mut y = self.below_info(x, x + w, corner_top, s);
            let top = y;
            let pad = 14.0 * s;
            let mut items: Vec<(TextureId, [f32; 4])> = Vec::new();
            if let Some(p) = image {
                let entry = self.images.entry(p.to_path_buf()).or_insert_with(|| {
                    omsi_texture::decode_file(p).ok().map(|img| {
                        let (iw, ih) = (img.width, img.height);
                        (r.add_texture(scene, &img, false), iw, ih)
                    })
                });
                if let Some((tex, iw, ih)) = *entry {
                    let dw = w - pad * 2.0;
                    let dh = dw * ih as f32 / iw.max(1) as f32;
                    items.push((tex, [x + pad, y + pad, x + pad + dw, y + pad + dh]));
                    y += dh + pad;
                }
            }
            y += pad * 0.6;
            let tp = (18.0 * s) as u32;
            if !title.is_empty() {
                for line in wrap(&self.text, title, tp as f32, w - pad * 2.0) {
                    let l = self.text.label_op(r, scene, &omsi_ui::tr(&line), tp, hud::text(self.hud.ink), 1.0);
                    items.push((l.tex, [x + pad, y, x + pad + l.w as f32, y + l.h as f32]));
                    y += l.h as f32;
                }
                y += 6.0 * s;
            }
            let bp = (14.0 * s) as u32;
            let max_y = f.height - 60.0 * s;
            'text: for para in text.lines() {
                for line in wrap(&self.text, para, bp as f32, w - pad * 2.0) {
                    if y > max_y {
                        break 'text;
                    }
                    let l = self.text.label_op(r, scene, &omsi_ui::tr(&line), bp, hud::text(self.hud.dim), 1.0);
                    items.push((l.tex, [x + pad, y, x + pad + l.w as f32, y + l.h as f32]));
                    y += l.h as f32 * 0.95;
                }
                y += 5.0 * s;
            }
            let tr = |t: &str| omsi_ui::tr(t).into_owned();
            let foot = format!("{} {}/{}   |   {}   |   {}   |   {}", tr("Page"), at + 1, count, tr("Enter next"), tr("Page Up back"), tr("Ctrl+T hide"));
            let l = self.text.label_op(r, scene, &foot, (12.0 * s) as u32, hud::text(self.hud.muted), 1.0);
            y += 4.0 * s;
            items.push((l.tex, [x + pad, y, x + pad + l.w as f32, y + l.h as f32]));
            y += l.h as f32 + pad;
            self.hud_card(r, scene, [x, top, x + w, y], hud::RADIUS * s, self.text.backdrop, s);
            scene.overlays.extend(items);
        }
        // --- the game menu and its lists, in the middle over a dimmed picture
        self.anim_dt = dt.clamp(0.0, 0.1);
        self.text.flat = true;
        self.draw_menu(r, scene, f);
        self.text.flat = false;
        // --- the mouse-over name, right of the cursor
        self.vr_tooltip_overlay = None;
        if let Some(t) = f.tooltip.as_ref().filter(|t| !t.is_empty()) {
            let ink = if f.vr { [237, 237, 237, 0] } else { hud::text(self.hud.ink) };
            let l = self.text.label_op(r, scene, &tooltip_label(t), (13.0 * s).round() as u32, ink, 1.0);
            let mut x = f.cursor.0.clamp(0.0, f.width) + 16.0 * s;
            let mut y = f.cursor.1 + 2.0 * s;
            if f.tooltip_above {
                x = (f.cursor.0 - l.w as f32 * 0.5).clamp(5.0 * s, (f.width - l.w as f32 - 5.0 * s).max(5.0 * s));
                y = (f.cursor.1 - 56.0 * s - l.h as f32).max(0.0);
            } else if x + l.w as f32 > f.width {
                x = (f.cursor.0.clamp(0.0, f.width) - 8.0 * s - l.w as f32).max(5.0 * s);
            }
            if y + l.h as f32 > f.height {
                y = f.height - l.h as f32;
            }
            if f.vr {
                self.vr_tooltip_overlay = Some(scene.overlays.len());
            } else {
                // (in a headset the text alone is placed in front of the eye)
                self.hud_card(r, scene, [x - 9.0 * s, y, x + l.w as f32 + 9.0 * s, y + l.h as f32], hud::RADIUS * s, 1.0, s);
            }
            scene.overlays.push((l.tex, [x, y, x + l.w as f32, y + l.h as f32]));
        }
        if let (Some((x, y)), false) = (f.steer_cross, f.vr) {
            // (the size of the system's crosshair cursor: 17 logical pixels)
            let tex = self.text.steer_cross(r, scene);
            let half = 8.5 * f.scale;
            scene.overlays.push((tex, [x - half, y - half, x + half, y + half]));
        }
        if f.vr {
            let pointer = self.text.vr_pointer(r, scene);
            self.vr_cursor_overlay = Some(scene.overlays.len());
            scene.overlays.push((pointer, [0.0, 0.0, 7.0 * s, 7.0 * s]));
        } else {
            self.vr_cursor_overlay = None;
        }
        self.text.end_frame(r, scene);
    }
}

/// The pixels (RGBA, `n`×`n`) of the steering cross: arms two pixels wide across the whole
/// picture, white, with a one-pixel dark edge so it shows on a bright sky and a dark road
/// alike, and a small gap in the middle that leaves the point itself visible.
fn steer_cross_pixels(n: usize) -> Vec<u8> {
    let mut rgba = vec![0u8; n * n * 4];
    let c = n as f32 * 0.5;
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = ((x as f32 + 0.5 - c).abs(), (y as f32 + 0.5 - c).abs());
            let (along, across) = if dx <= dy { (dy, dx) } else { (dx, dy) };
            let gap = along < 3.0;
            let px = if gap || along > c - 1.0 {
                [0, 0, 0, 0]
            } else if across <= 1.0 {
                [255, 255, 255, 255]
            } else if across <= 2.0 {
                [0, 0, 0, 200]
            } else {
                [0, 0, 0, 0]
            };
            rgba[(y * n + x) * 4..(y * n + x + 1) * 4].copy_from_slice(&px);
        }
    }
    rgba
}

impl Ui {
    /// The notifications as the navigator's cards: no edge, a soft shadow, the kind's icon in
    /// its colour, the text wrapped to as many rows as it needs. A card comes in rising a
    /// little and fading, and goes fading while the room it took closes; the newest stands
    /// on top of the stack.
    #[rustfmt::skip]
    fn draw_notices(&mut self, r: &Renderer, scene: &mut Scene, f: &Frame, s: f32) {
        if f.notices.is_empty() {
            return;
        }
        let gap = 8.0 * s;
        let px = (14.0 * s).round() as u32;
        let lh = (px as f32 * 1.45).round();
        let pad_y = 11.0 * s;
        let pad_l = 14.0 * s;
        let icon_px = (18.0 * s).round() as u32;
        let text_x = pad_l + icon_px as f32 + 10.0 * s;
        let pad_r = 16.0 * s;
        let max_text = (340.0 * s).min(f.width - 32.0 * s - text_x - pad_r).max(80.0 * s);
        // where the stack stands: over a navigator in the lower half (growing up), under one
        // in the upper half, or at the top middle; on the navigator's side of the window
        let (up, mut y, place) = match f.notice_anchor {
            Some(a) => {
                let left = (a[0] + a[2]) * 0.5 < f.width * 0.5;
                let place = if left { (a[0].max(8.0 * s), 0) } else { (a[2].min(f.width - 8.0 * s), 2) };
                if a[1] > f.height * 0.5 { (true, a[1] - gap, place) } else { (false, a[3] + gap, place) }
            }
            None => (false, self.below_info(f.width * 0.5 - 200.0 * s, f.width * 0.5 + 200.0 * s, 70.0 * s, s), (f.width * 0.5, 1)),
        };
        // (growing up, the oldest stands by the navigator and the newest on top; growing
        // down, the newest first)
        let order: Vec<Notice> = if up { f.notices.to_vec() } else { f.notices.iter().rev().cloned().collect() };
        for n in &order {
            let p = n.alpha();
            if p <= 0.0 {
                continue;
            }
            let text = omsi_ui::tr(&n.text).into_owned();
            let mut rows = {
                let tc = &self.text;
                wrap_rows(&text, max_text, 0.0, &|t: &str| tc.width_raw(t, px as f32))
            };
            // (six rows at the most; a longer text says it goes on)
            if rows.len() > 6 {
                rows.truncate(6);
                rows[5].push('…');
            }
            let tw = rows.iter().map(|t| self.text.width_raw(t, px as f32)).fold(0.0f32, f32::max);
            let w = (text_x + tw + pad_r).max(220.0 * s).min(f.width - 16.0 * s).round();
            let h = (pad_y * 2.0 + lh * rows.len() as f32).round();
            let top = if up { y - h } else { y };
            if top < 0.0 || top + h > f.height {
                break;
            }
            let x0 = match place.1 {
                0 => place.0,
                2 => place.0 - w,
                _ => place.0 - w * 0.5,
            }
            .clamp(8.0 * s, (f.width - w - 8.0 * s).max(8.0 * s))
            .round();
            // (it rises into its place as it comes, sinks as it goes)
            let ty = top + (1.0 - p) * 10.0 * s;
            let k = (p * 16.0).round() / 16.0;
            let (rgb, icon) = n.kind.look();
            self.hud_card(r, scene, [x0, ty, x0 + w, ty + h], hud::RADIUS * s, k, s);
            if let Some(t) = self.text.icon(r, scene, icon, icon_px, [rgb[0], rgb[1], rgb[2], (255.0 * k) as u8]) {
                let iy = (ty + pad_y + lh * 0.5 - icon_px as f32 * 0.5).round();
                let ix = (x0 + pad_l).round();
                scene.overlays.push((t, [ix, iy, ix + icon_px as f32, iy + icon_px as f32]));
            }
            for (i, row) in rows.iter().enumerate() {
                let l = self.text.label_op(r, scene, row, px, hud::text(self.hud.ink), k);
                let cy = ty + pad_y + lh * (i as f32 + 0.5);
                let ly = (cy - l.h as f32 * 0.5).round();
                let lx = (x0 + text_x).round();
                scene.overlays.push((l.tex, [lx, ly, lx + l.w as f32, ly + l.h as f32]));
            }
            let slot = (h + gap) * p;
            y = if up { y - slot } else { y + slot };
        }
    }
}

/// How long the loading screen stands full once its bar is at the end, and how long it then
/// takes to fade over the first pictures of the session (seconds).
const LOADING_HOLD: f32 = 0.12;
const LOADING_FADE: f32 = 0.5;

/// The loading screen's own state. The start area's tiles are counted only as each is done,
/// and most of a load the count stands at none, so the bar is driven by the time the load is
/// expected to take (`LoadTimes`): it goes along that time to `LOADING_EXPECTED_AT` and on,
/// slower, towards `LOADING_CREEP` if the load takes longer - never standing still, never going
/// back - and takes the real count whenever that is ahead. Once the world is there it runs to
/// its end in `LOADING_RUN` seconds, stands a moment, and the screen fades over the session's
/// first pictures instead of being cut away.
#[derive(Default)]
pub struct Loading {
    /// The map's name.
    pub title: String,
    /// How long this load is expected to take (seconds; 0: not known yet).
    pub expected: f32,
    /// The bar as shown (0 to 1).
    shown: f32,
    /// Seconds the loading screen has stood.
    elapsed: f32,
    /// Seconds the screen has been shown, the run and the fade too (the stripes, the tips).
    clock: f32,
    /// The tip shown first (a different one each start).
    tip0: usize,
    /// The world is there: how fast the bar runs to its end (a share a second).
    run: Option<f32>,
    /// The run has not had a frame yet: the time before it (the world being started, no
    /// frame drawn) is not run through unseen.
    run_fresh: bool,
    /// Seconds since the bar came to its end.
    out: f32,
    /// The screen was shown at all (a map without streaming starts at once: nothing fades).
    drawn: bool,
    last: Option<std::time::Instant>,
}

/// The tips under the loading screen's "Tips", one at a time.
pub const LOADING_TIPS: [&str; 9] = [
    "Shift+U puts the bus into service: the electrics, the engine and the lights in one go",
    "F1 is the driver's seat, F2 the passengers' view, F3 outside the bus and F4 the free camera",
    "Esc opens the menu: the options, the line and tour, saving and the photo mode",
    "Shift+N switches the navigator between the map, the map with the next stops, and off",
    "Insert shows the timetable of your trip",
    "Shift+M opens the city map",
    "P pauses the game; Alt+S saves the situation",
    "Shift+Y shows the information bar: the time, the speed, the temperatures and the next stop",
    "Alt+U hides the whole interface for a clear view; press it again to bring it back",
];
/// How long a tip stands (seconds).
const TIP_SECS: f32 = 6.0;

/// Where the bar stands when the expected time is up, and how far it creeps on after it.
const LOADING_EXPECTED_AT: f32 = 0.92;
const LOADING_CREEP: f32 = 0.98;
/// How long the bar takes from wherever it is to its end once the world is there (seconds).
const LOADING_RUN: f32 = 0.35;
/// The expected time of a map loaded for the first time (seconds).
pub const LOADING_FIRST_GUESS: f32 = 8.0;

/// The bar for `u` of the expected time gone by: nearly even along the time (a little quicker
/// at first, a little slower towards the end) to `LOADING_EXPECTED_AT`, then on more and more
/// slowly towards `LOADING_CREEP`.
pub fn time_progress(u: f32) -> f32 {
    let u = u.max(0.0);
    if u <= 1.0 {
        LOADING_EXPECTED_AT * u * (1.3 - 0.3 * u)
    } else {
        LOADING_EXPECTED_AT
            + (LOADING_CREEP - LOADING_EXPECTED_AT) * (1.0 - 1.0 / (1.0 + (u - 1.0) * 3.0))
    }
}

/// The bar shown after `dt` seconds on its way to `target`: an exponential approach (it
/// slows as it gets near) held between a floor speed, so that it never stands just short of
/// where it is going, and a ceiling, so that a sudden jump of the count is spread out (`ready`:
/// a quicker approach).
pub fn ease_progress(shown: f32, target: f32, ready: bool, dt: f32) -> f32 {
    let target = target.clamp(0.0, 1.0);
    let gap = target - shown;
    if gap <= 0.0 {
        return shown;
    }
    let (tau, floor, ceil) = if ready {
        (0.1, 1.0, 3.5)
    } else {
        (0.35, 0.12, 1.2)
    };
    (shown + (gap / tau).clamp(floor, ceil) * dt.clamp(0.0, 0.1)).min(target)
}

impl Loading {
    /// Seconds since the last frame of the loading screen (none before the first).
    pub fn tick(&mut self) -> f32 {
        let now = std::time::Instant::now();
        let dt = self.last.map(|t| (now - t).as_secs_f32()).unwrap_or(0.0);
        self.last = Some(now);
        dt
    }

    /// A frame of the loading screen with the start area `real` of the way loaded.
    pub fn step(&mut self, real: f32, dt: f32) {
        if !self.drawn {
            self.tip0 = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as usize)
                .unwrap_or(0);
        }
        self.drawn = true;
        let dt = dt.clamp(0.0, 1.0);
        self.clock += dt.min(0.1);
        match self.run {
            None => {
                // (by the clock: the time is what the bar goes along, and what the next load
                // of the map expects)
                self.elapsed += dt;
                let by_time = time_progress(self.elapsed / self.expected_secs());
                // (the count, when it is ahead, eased: it comes in jumps)
                let by_count =
                    ease_progress(self.shown, real.clamp(0.0, 1.0) * LOADING_CREEP, false, dt);
                self.shown = self.shown.max(by_time).max(by_count);
            }
            Some(_) if self.run_fresh => self.run_fresh = false,
            Some(rate) => {
                self.shown = (self.shown + rate * dt).min(1.0);
                if self.shown >= 1.0 {
                    // (a fade of a few steps at the least, however slow the frames are)
                    self.out += dt.min(0.1);
                }
            }
        }
    }

    fn expected_secs(&self) -> f32 {
        if self.expected > 0.5 {
            self.expected
        } else {
            LOADING_FIRST_GUESS
        }
    }

    /// The world is there: the rest of the bar in `LOADING_RUN` seconds.
    pub fn finish(&mut self) {
        if self.run.is_none() {
            self.run = Some(((1.0 - self.shown) / LOADING_RUN).max(0.5));
            self.run_fresh = true;
        }
    }

    /// Seconds the loading screen stood (the load's time, for `LoadTimes`).
    pub fn elapsed(&self) -> f32 {
        self.elapsed
    }

    /// The bar shown.
    pub fn shown(&self) -> f32 {
        self.shown
    }

    /// Seconds the screen has been shown (its stripes and tips go by it).
    pub fn clock(&self) -> f32 {
        self.clock
    }

    /// The tip shown now and how far it is faded in: a new one every `TIP_SECS`, fading
    /// out and the next in.
    pub fn tip(&self) -> (&'static str, f32) {
        let n = (self.clock / TIP_SECS) as usize;
        let t = self.clock - n as f32 * TIP_SECS;
        let k = (t / 0.4).min((TIP_SECS - t) / 0.4).clamp(0.0, 1.0);
        (LOADING_TIPS[(self.tip0 + n) % LOADING_TIPS.len()], k)
    }

    /// The screen still stands over the session's pictures: how opaque it is (none: it is
    /// gone, or it never showed).
    pub fn over_session(&self) -> Option<f32> {
        if !(self.drawn && self.run.is_some()) {
            return None;
        }
        let t = ((self.out - LOADING_HOLD) / LOADING_FADE).clamp(0.0, 1.0);
        (t < 1.0).then(|| 1.0 - t * t * (3.0 - 2.0 * t))
    }
}

/// How long the last loads of each map took (`~/.openomsi/cache/load-times.json`: the map's
/// file → the last three loads in seconds), so that the loading bar can go along the time the
/// next one will likely take.
pub struct LoadTimes;

impl LoadTimes {
    fn path() -> Option<std::path::PathBuf> {
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
        Some(
            std::path::PathBuf::from(home)
                .join(".openomsi")
                .join("cache")
                .join("load-times.json"),
        )
    }

    fn read() -> serde_json::Map<String, serde_json::Value> {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// The expected time of loading `map`: the average of its last loads (none: never loaded).
    pub fn expected(map: &str) -> Option<f32> {
        expected_of(Self::read().get(map)?)
    }

    /// `map` took `secs` to load this time.
    pub fn record(map: &str, secs: f32) {
        let Some(p) = Self::path() else { return };
        let mut all = Self::read();
        let mut last: Vec<f32> = all
            .get(map)
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_f64())
                    .map(|x| x as f32)
                    .collect()
            })
            .unwrap_or_default();
        last.push(secs);
        let keep = last.len().saturating_sub(3);
        last.drain(..keep);
        let last: Vec<f64> = last
            .iter()
            .map(|x| (*x as f64 * 100.0).round() / 100.0)
            .collect();
        all.insert(map.to_string(), serde_json::json!(last));
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(&p, serde_json::Value::Object(all).to_string()) {
            log::warn!("load times: {} not written: {e}", p.display());
        }
    }
}

/// The average of the loads kept for a map.
fn expected_of(v: &serde_json::Value) -> Option<f32> {
    let t: Vec<f32> = v
        .as_array()?
        .iter()
        .filter_map(|x| x.as_f64())
        .map(|x| x as f32)
        .filter(|x| x.is_finite() && *x > 0.0)
        .collect();
    (!t.is_empty()).then(|| t.iter().sum::<f32>() / t.len() as f32)
}

/// `text` broken into lines of at most `width` pixels, between words.
fn wrap(tc: &TextCache, text: &str, px: f32, width: f32) -> Vec<String> {
    let mut out = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let try_line = if line.is_empty() {
            word.to_string()
        } else {
            format!("{line} {word}")
        };
        if tc.width(&try_line, px) > width && !line.is_empty() {
            out.push(std::mem::take(&mut line));
            line = word.to_string();
        } else {
            line = try_line;
        }
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}

/// `text` cut at the end to fit `width` pixels ("…").
fn clip_to(tc: &TextCache, text: &str, px: f32, width: f32) -> String {
    if tc.width(text, px) <= width {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let head = |n: usize| {
        let mut t: String = chars[..n].iter().collect();
        t.push('…');
        t
    };
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        if tc.width_raw(&head(mid), px) <= width {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    head(lo)
}

/// `text` in rows that fit `width` (as `measure` measures): broken between words, and inside a
/// word too long for a row. The rows after the first fit `width - indent` (they are drawn
/// indented, under the first).
fn wrap_rows(text: &str, width: f32, indent: f32, measure: &impl Fn(&str) -> f32) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();
    let mut cur = String::new();
    let room = |rows: &Vec<String>| {
        if rows.is_empty() {
            width
        } else {
            width - indent
        }
    };
    for word in text.split(' ') {
        let joined = if cur.is_empty() {
            word.to_string()
        } else {
            format!("{cur} {word}")
        };
        if measure(&joined) <= room(&rows) {
            cur = joined;
            continue;
        }
        if !cur.is_empty() {
            rows.push(std::mem::take(&mut cur));
        }
        // the word alone, cut where a row is full (a long link, a run of letters)
        let mut w: Vec<char> = word.chars().collect();
        while w.len() > 1 && measure(&w.iter().collect::<String>()) > room(&rows) {
            let mut k = w.len() - 1;
            while k > 1 && measure(&w[..k].iter().collect::<String>()) > room(&rows) {
                k -= 1;
            }
            rows.push(w[..k].iter().collect());
            w.drain(..k);
        }
        cur = w.into_iter().collect();
    }
    if !cur.is_empty() || rows.is_empty() {
        rows.push(cur);
    }
    rows
}

/// `text` cut at the start to fit (the end of what is being typed stays visible).
fn clip_left(tc: &TextCache, text: &str, px: f32, width: f32) -> String {
    let mut t: Vec<char> = text.chars().collect();
    while t.len() > 1 && tc.width(&t.iter().collect::<String>(), px) > width {
        t.remove(0);
    }
    t.into_iter().collect()
}

/// A chat line with its bad words starred out (see `omsi_lan::game::filter_chat`).
pub fn filter_chat(text: &str) -> String {
    omsi_lan::game::filter_chat(text)
}

/// Fit all VR page buttons above the separately reserved Back button.
fn vr_settings_sidebar_step(available: f32, pages: usize, scale: f32) -> f32 {
    (available.max(0.0) / pages.max(1) as f32).min(42.0 * scale)
}

/// Mouse variants append suffixes to an official control label. Those cannot be
/// catalogue templates (they have no leading text), so translate the base separately.
fn tooltip_label(label: &str) -> std::borrow::Cow<'_, str> {
    let translated = omsi_ui::tr(label);
    if translated != label {
        return translated;
    }
    for suffix in [" (outside)", " 2"] {
        if let Some(base) = label.strip_suffix(suffix) {
            return format!("{}{}", omsi_ui::tr(base), omsi_ui::tr(suffix)).into();
        }
    }
    translated
}

#[cfg(test)]
mod tests;
