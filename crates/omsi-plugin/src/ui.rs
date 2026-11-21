//! `omsi.ui`: on-screen panels and notifications of the plugins (see docs/PLUGINS.md).
//!
//! A plugin describes a panel as a table once and replaces it (or patches one element of it,
//! `ui.update`) when its content changes; the game lays it out and draws it every frame. This
//! module is the part every game shares: the tables read into [`Panel`]s (checked and held to
//! the limits here, so a plugin learns at once what is wrong), which plugin owns what, the
//! notifications, whether the panels have the mouse, the clicks and the changes of sliders,
//! boxes and fields waiting for their plugin, the panels dragged and the field being typed
//! into. Drawing and hit-testing are the game's (`omsi-app`'s `plugin_ui`).

use crate::api::Value;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

mod menu;
pub use menu::{MAX_MENU_ITEMS, MAX_MENU_LABEL, MenuItem};
mod theme;
pub use theme::{ThemeInfo, themes_for_load};
mod dock;
pub use dock::Dock;
mod read;
use read::patch;
pub use read::{ToastSpec, panel_from_source, parse_panel, parse_toast};

/// `omsi.ui.version`: raised when something is added a plugin may want to test for.
/// 2: images, checkboxes, sliders, text fields, tabs, charts, tables, draggable panels.
/// 3: the navigator's board (`anchor = "navigator"`, `ui.dock`).
/// 4: boxes, stop lists, bars of several parts, a badge's weight, a table's column
/// alignments and coloured cells.
/// 5: panels that take a click without the mouse (`grab`).
/// 6: a navigator panel that fills the navigator's whole frame (`frame`), and the navigator's
/// map as an element of it (`map`).
/// 7: the interface themes (`ui.themes`, `ui.theme`, `ui.set_theme`).
pub const VERSION: i64 = 7;
/// Most panels of one plugin.
pub const MAX_PANELS: usize = 16;
/// Most elements of one panel, those in rows included.
pub const MAX_ELEMENTS: usize = 200;
/// Longest text, in characters.
pub const MAX_TEXT: usize = 500;
/// Most notifications of one plugin on the screen at once.
pub const MAX_TOASTS: usize = 8;
/// Longest panel or element id, in characters.
pub const MAX_ID: usize = 64;
/// Most values of a chart, cells of a table.
pub const MAX_POINTS: usize = 512;
/// Rows in rows in rows...: deeper is an error (a table that holds itself would never end).
const MAX_DEPTH: usize = 8;
/// Clicks and changes kept for plugins that do not run (the game paused): the oldest go.
const MAX_CLICKS: usize = 32;

/// The panels of every plugin, shared by the plugins and the game.
pub type SharedUi = Rc<RefCell<UiState>>;

/// An sRGB colour with straight alpha, as `"#RRGGBB"` or `"#RRGGBBAA"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba(pub [u8; 4]);

impl Rgba {
    pub fn parse(s: &str) -> Option<Rgba> {
        let hex = s.trim().strip_prefix('#')?;
        if !(hex.len() == 6 || hex.len() == 8) || !hex.is_ascii() {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        let a = if hex.len() == 8 { byte(6)? } else { 255 };
        Some(Rgba([byte(0)?, byte(2)?, byte(4)?, a]))
    }
}

/// Where on the screen a panel is placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Anchor {
    #[default]
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
    /// In the navigator's board, under its map, as wide as the navigator: `x`, `y`, `width`
    /// and the card's own look are left aside.
    Navigator,
}

impl Anchor {
    fn parse(s: &str) -> Option<Anchor> {
        Some(match s {
            "top_left" => Anchor::TopLeft,
            "top" => Anchor::Top,
            "top_right" => Anchor::TopRight,
            "left" => Anchor::Left,
            "center" => Anchor::Center,
            "right" => Anchor::Right,
            "bottom_left" => Anchor::BottomLeft,
            "bottom" => Anchor::Bottom,
            "bottom_right" => Anchor::BottomRight,
            "navigator" => Anchor::Navigator,
            _ => return None,
        })
    }

    /// Where the anchor is across and down the screen: 0, a half or 1 of each.
    pub fn factors(self) -> (f32, f32) {
        match self {
            Anchor::TopLeft => (0.0, 0.0),
            Anchor::Top => (0.5, 0.0),
            Anchor::TopRight => (1.0, 0.0),
            Anchor::Left => (0.0, 0.5),
            Anchor::Center => (0.5, 0.5),
            Anchor::Right => (1.0, 0.5),
            Anchor::BottomLeft => (0.0, 1.0),
            Anchor::Bottom => (0.5, 1.0),
            Anchor::BottomRight => (1.0, 1.0),
            // (not placed on the screen: the navigator says where it goes)
            Anchor::Navigator => (0.0, 0.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Weight {
    #[default]
    Regular,
    Medium,
    Bold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

/// How a row's children share a width they do not fill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RowAlign {
    #[default]
    Start,
    Center,
    End,
    /// The room left between the children.
    Between,
}

/// A panel: a card of elements stacked top to bottom. Sizes are logical pixels (the game
/// scales them with its interface).
#[derive(Debug, Clone, PartialEq)]
pub struct Panel {
    pub anchor: Anchor,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub padding: f32,
    pub gap: f32,
    /// None: the game's own card colour.
    pub background: Option<Rgba>,
    pub radius: f32,
    /// A stripe along the left edge.
    pub accent: Option<Rgba>,
    pub visible: bool,
    /// A click anywhere on it is a `ui_click` with no element.
    pub clickable: bool,
    /// While the panels have the mouse it can be moved by dragging it (where it has nothing
    /// clickable); the place it is moved to stays when it is set again.
    pub draggable: bool,
    /// A click on it is its own even while the panels do not have the mouse (a card the
    /// driver clicks away or keeps): the cab behind it gets none.
    pub grab: bool,
    /// With `anchor = "navigator"`: it fills the navigator's whole frame, not only the board
    /// under its map; the map shows only where the panel has a `map` element.
    pub frame: bool,
    pub children: Vec<Element>,
}

impl Default for Panel {
    fn default() -> Panel {
        Panel {
            anchor: Anchor::TopLeft,
            x: 0.0,
            y: 0.0,
            width: 300.0,
            padding: 12.0,
            gap: 6.0,
            background: None,
            radius: 12.0,
            accent: None,
            visible: true,
            clickable: false,
            draggable: false,
            grab: false,
            frame: false,
            children: Vec::new(),
        }
    }
}

/// One element of a panel or a row (those with `visible = false` are left out when read).
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub id: Option<String>,
    pub color: Option<Rgba>,
    /// In a row: takes the width the others leave.
    pub grow: bool,
    pub clickable: bool,
    pub kind: Kind,
}

impl Element {
    /// Whether a click on it is a `ui_click`: a button's or a tile's always, others' with an id and
    /// `clickable = true`.
    pub fn takes_clicks(&self) -> bool {
        matches!(
            self.kind,
            Kind::Button { .. } | Kind::Tile { off: false, .. }
        ) || (self.clickable && self.id.is_some())
    }

    /// Whether the mouse changes its value (a checkbox, a slider, a field, tabs).
    pub fn is_control(&self) -> bool {
        self.id.is_some()
            && matches!(
                self.kind,
                Kind::Checkbox { .. }
                    | Kind::Slider { .. }
                    | Kind::Input { .. }
                    | Kind::Tabs { .. }
            )
    }

    /// Its value as `ui.value` and `ui_change` give it.
    pub fn value(&self) -> Value {
        match &self.kind {
            Kind::Checkbox { checked, .. } => Value::Bool(*checked),
            Kind::Slider { value, .. } => Value::Num(*value as f64),
            Kind::Input { text, .. } => Value::Str(text.clone()),
            Kind::Tabs { selected, .. } => Value::Int(*selected as i64 + 1),
            _ => Value::Nil,
        }
    }
}

/// How a chart draws its values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChartStyle {
    #[default]
    Line,
    Bars,
}

/// Where a stop of a `stops` list is for the bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StopState {
    /// Behind it.
    Done,
    /// The one it heads for, or stands at.
    Now,
    #[default]
    Ahead,
}

/// A stop of a `stops` list: its name and time, when the bus will be there (in its colour),
/// where it is for the bus and a tag after its name ("terminus").
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Stop {
    pub name: String,
    pub time: String,
    pub expected: String,
    pub expected_color: Option<Rgba>,
    pub state: StopState,
    pub tag: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Text {
        text: String,
        size: f32,
        weight: Weight,
        align: Align,
        wrap: bool,
    },
    /// A Material Symbols name of the game's icons; one it has not draws nothing.
    Icon {
        name: String,
        size: f32,
    },
    Row {
        children: Vec<Element>,
        gap: f32,
        align: RowAlign,
    },
    /// `segments` (shares and colours, side by side): a bar of several parts in place of
    /// `value`'s one.
    Bar {
        value: f32,
        height: f32,
        background: Option<Rgba>,
        segments: Vec<(f32, Rgba)>,
    },
    Badge {
        text: String,
        text_color: Option<Rgba>,
        weight: Weight,
    },
    Divider,
    Space {
        size: f32,
    },
    Button {
        text: String,
        icon: Option<String>,
    },
    Tile {
        text: String,
        icon: String,
        focus: bool,
        off: bool,
    },
    /// A picture of the plugin's folder (PNG, JPEG, BMP, TGA); `path` None: no such file.
    Image {
        path: Option<PathBuf>,
        width: f32,
        height: f32,
    },
    Checkbox {
        text: String,
        checked: bool,
    },
    Slider {
        value: f32,
        min: f32,
        max: f32,
        step: f32,
    },
    /// A text field: clicked, it takes what is typed until Enter, Esc or a click elsewhere.
    Input {
        text: String,
        placeholder: String,
        max: usize,
    },
    Tabs {
        tabs: Vec<String>,
        selected: usize,
    },
    /// Values over time, as a line or as bars; `min`/`max` None: the values' own.
    Chart {
        values: Vec<f32>,
        min: Option<f32>,
        max: Option<f32>,
        height: f32,
        style: ChartStyle,
        fill: bool,
    },
    /// Columns of texts: the header row bold; `widths` shares of the width (none: alike),
    /// `aligns` each column's alignment (none: left), `inks` a colour of each cell's own
    /// (row by row; none: the element's).
    Table {
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
        widths: Vec<f32>,
        size: f32,
        aligns: Vec<Align>,
        inks: Vec<Vec<Option<Rgba>>>,
    },
    /// Its children stacked in a tinted box with a stripe on the left (the element's colour).
    Box {
        children: Vec<Element>,
        gap: f32,
    },
    /// Stops on a rail: the ones behind the bus grey, the one it heads for marked, a time on
    /// the right (and before it when the bus will be there).
    Stops {
        stops: Vec<Stop>,
        size: f32,
    },
    /// The navigator's map, in a panel that fills its frame: `height` (0: as high as the
    /// navigator's own map, 0.8 times its width), `cards` the navigator's cards over it (the
    /// next turn, the clock, the speed, the next stop). Elsewhere empty room.
    Map {
        height: f32,
        cards: bool,
    },
}

/// A notification card of `omsi.ui.toast`.
#[derive(Debug, Clone, PartialEq)]
pub struct Toast {
    pub owner: u64,
    /// Tells the game's drawings of toasts apart (none is ever used twice).
    pub serial: u64,
    pub text: String,
    pub title: Option<String>,
    pub icon: Option<String>,
    pub color: Option<Rgba>,
    pub seconds: f32,
    /// Seconds it has been shown.
    pub age: f32,
}

/// A panel and who made it.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelEntry {
    pub owner: u64,
    pub id: String,
    pub panel: Panel,
    /// Changes whenever the panel does: the game lays it out again only then.
    pub revision: u64,
    /// Where it was dragged to, from where its table puts it (logical pixels).
    pub moved: (f32, f32),
}

/// A click on a panel, for its plugin.
#[derive(Debug, Clone, PartialEq)]
pub struct Click {
    pub owner: u64,
    pub panel: String,
    pub element: Option<String>,
}

/// A new value of a checkbox, slider, field or tabs, for its plugin (`ui_change`).
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub owner: u64,
    pub panel: String,
    pub element: String,
    pub value: Value,
}

/// The field being typed into.
#[derive(Debug, Clone, PartialEq)]
pub struct Typing {
    pub owner: u64,
    pub panel: String,
    pub element: String,
}

/// Everything the plugins show, and what the game tells them back.
#[derive(Debug)]
pub struct UiState {
    panels: Vec<PanelEntry>,
    toasts: Vec<Toast>,
    clicks: Vec<Click>,
    changes: Vec<Change>,
    /// The plugin that gave the panels the mouse, while they have it.
    focus: Option<u64>,
    /// Whether the panels are on the screen (the game hides them under its menus).
    shown: bool,
    /// Logical width, height, and physical pixels per logical one.
    screen: [f32; 3],
    serial: u64,
    typing: Option<Typing>,
    /// The plugins' lines in the pause menu, in the order they were first set.
    menu: Vec<MenuItem>,
    /// Lines chosen, for their plugins' next frame.
    menu_picks: Vec<(u64, String)>,
    /// The navigator's board (None: the navigator shows none).
    dock: Option<Dock>,
    /// The game's interface themes, the one in use, and the one a plugin asked for.
    themes: Vec<ThemeInfo>,
    theme_now: String,
    theme_ask: Option<String>,
}

impl Default for UiState {
    fn default() -> Self {
        let (themes, theme_now) = theme::for_load();
        // (until the game has drawn a frame: a 1080p screen)
        UiState {
            panels: Vec::new(),
            toasts: Vec::new(),
            clicks: Vec::new(),
            changes: Vec::new(),
            focus: None,
            shown: true,
            screen: [1920.0, 1080.0, 1.0],
            serial: 0,
            typing: None,
            menu: Vec::new(),
            menu_picks: Vec::new(),
            dock: None,
            themes,
            theme_now,
            theme_ask: None,
        }
    }
}

impl UiState {
    fn next_serial(&mut self) -> u64 {
        self.serial += 1;
        self.serial
    }

    fn find(&mut self, owner: u64, id: &str) -> Option<&mut PanelEntry> {
        self.panels
            .iter_mut()
            .find(|p| p.owner == owner && p.id == id)
    }

    /// Create or replace a plugin's panel. The same panel again changes nothing (a plugin
    /// that sets its panels on a timer redraws nothing while they stay the same).
    pub fn set(&mut self, owner: u64, id: &str, panel: Panel) -> Result<(), String> {
        check_id("the panel id", id)?;
        if let Some(i) = self
            .panels
            .iter()
            .position(|p| p.owner == owner && p.id == id)
        {
            if self.panels[i].panel != panel {
                let revision = self.next_serial();
                let e = &mut self.panels[i];
                e.panel = panel;
                e.revision = revision;
            }
            return Ok(());
        }
        if self.panels.iter().filter(|p| p.owner == owner).count() >= MAX_PANELS {
            return Err(format!("a plugin has at most {MAX_PANELS} panels"));
        }
        let revision = self.next_serial();
        self.panels.push(PanelEntry {
            owner,
            id: id.to_string(),
            panel,
            revision,
            moved: (0.0, 0.0),
        });
        Ok(())
    }

    /// Remove a plugin's panel; false when it had none of that id.
    pub fn remove(&mut self, owner: u64, id: &str) -> bool {
        let n = self.panels.len();
        self.panels.retain(|p| !(p.owner == owner && p.id == id));
        self.clicks.retain(|c| !(c.owner == owner && c.panel == id));
        self.changes
            .retain(|c| !(c.owner == owner && c.panel == id));
        if self
            .typing
            .as_ref()
            .is_some_and(|t| t.owner == owner && t.panel == id)
        {
            self.typing = None;
        }
        self.panels.len() != n
    }

    /// Remove every panel of a plugin.
    pub fn clear(&mut self, owner: u64) {
        self.panels.retain(|p| p.owner != owner);
        self.clicks.retain(|c| c.owner != owner);
        self.changes.retain(|c| c.owner != owner);
        if self.typing.as_ref().is_some_and(|t| t.owner == owner) {
            self.typing = None;
        }
    }

    /// What goes when a plugin stops (or is loaded again, or switched off): its panels and
    /// clicks, and the mouse when it was the one that asked for it (the cursor stayed on
    /// panels nobody would answer any more). Its notifications run their time: they say
    /// something that happened, and go by themselves.
    pub fn remove_owner(&mut self, owner: u64) {
        self.clear(owner);
        if self.focus == Some(owner) {
            self.focus = None;
        }
        self.menu.retain(|m| m.owner != owner);
        self.menu_picks.retain(|m| m.0 != owner);
    }

    /// The ids of a plugin's panels.
    pub fn ids(&self, owner: u64) -> Vec<String> {
        self.panels
            .iter()
            .filter(|p| p.owner == owner)
            .map(|p| p.id.clone())
            .collect()
    }

    /// A plugin's panel.
    pub fn get(&self, owner: u64, id: &str) -> Option<&PanelEntry> {
        self.panels.iter().find(|p| p.owner == owner && p.id == id)
    }

    /// Show or hide a panel; false when there is none.
    pub fn set_visible(&mut self, owner: u64, id: &str, on: bool) -> bool {
        let serial = self.next_serial();
        match self.find(owner, id) {
            Some(e) => {
                if e.panel.visible != on {
                    e.panel.visible = on;
                    e.revision = serial;
                }
                true
            }
            None => false,
        }
    }

    /// Change keys of one element of a panel (`ui.update`): what an element of its type
    /// takes, read as `set` reads them. Err when there is no such panel or element.
    pub fn update(
        &mut self,
        owner: u64,
        id: &str,
        element: &str,
        fields: &Value,
        folder: Option<&Path>,
    ) -> Result<(), String> {
        let serial = self.next_serial();
        let e = self
            .find(owner, id)
            .ok_or_else(|| format!("no panel \"{id}\""))?;
        let el = find_element(&mut e.panel.children, element)
            .ok_or_else(|| format!("no element \"{element}\" in panel \"{id}\""))?;
        let before = el.clone();
        patch(el, fields, folder)?;
        if *el != before {
            e.revision = serial;
        }
        Ok(())
    }

    /// Whether `element` of a plugin's panel is a text field (a click on it starts typing).
    pub fn is_field(&self, owner: u64, id: &str, element: &str) -> bool {
        self.panels
            .iter()
            .find(|p| p.owner == owner && p.id == id)
            .and_then(|e| find_in(&e.panel.children, element))
            .is_some_and(|e| matches!(e.kind, Kind::Input { .. }))
    }

    /// The value of a checkbox, slider, field or tabs.
    pub fn value(&self, owner: u64, id: &str, element: &str) -> Option<Value> {
        let e = self
            .panels
            .iter()
            .find(|p| p.owner == owner && p.id == id)?;
        find_in(&e.panel.children, element).map(Element::value)
    }

    /// Show a notification; a plugin's oldest makes room when it has `MAX_TOASTS` already
    /// (a plugin that sends one per event is not told off for a busy minute).
    pub fn toast(&mut self, owner: u64, spec: ToastSpec) {
        if let Some(oldest) = self.toasts.iter().position(|t| t.owner == owner) {
            if self.toasts.iter().filter(|t| t.owner == owner).count() >= MAX_TOASTS {
                self.toasts.remove(oldest);
            }
        }
        let serial = self.next_serial();
        let ToastSpec {
            text,
            title,
            icon,
            color,
            seconds,
        } = spec;
        self.toasts.push(Toast {
            owner,
            serial,
            text,
            title,
            icon,
            color,
            seconds,
            age: 0.0,
        });
    }

    /// The panels have the mouse (`on`, asked by plugin `owner`) or let it go. They cannot
    /// have it while the game hides them. Returns the new state.
    pub fn set_focus(&mut self, owner: u64, on: bool) -> bool {
        self.focus = (on && self.shown).then_some(owner);
        if self.focus.is_none() {
            self.typing = None;
        }
        self.focus.is_some()
    }

    pub fn focused(&self) -> bool {
        self.focus.is_some()
    }

    /// The game takes the mouse back (Esc).
    pub fn release_focus(&mut self) {
        self.focus = None;
        self.typing = None;
    }

    /// The game shows the panels or hides them (its menus open); hidden, they let the mouse go.
    pub fn set_shown(&mut self, shown: bool) {
        self.shown = shown;
        if !shown {
            self.focus = None;
            self.typing = None;
        }
    }

    pub fn set_screen(&mut self, width: f32, height: f32, scale: f32) {
        self.screen = [width, height, scale];
    }

    /// `omsi.ui.screen()`: logical width, height, and the interface's scale.
    pub fn screen(&self) -> [f32; 3] {
        self.screen
    }

    pub fn panels(&self) -> &[PanelEntry] {
        &self.panels
    }

    /// The notifications on the screen, oldest first.
    pub fn toasts(&self) -> &[Toast] {
        &self.toasts
    }

    /// The game's time passes for the notifications: those over go.
    pub fn tick(&mut self, dt: f32) {
        for t in self.toasts.iter_mut() {
            t.age += dt.max(0.0);
        }
        self.toasts.retain(|t| t.age < t.seconds);
    }

    /// A click on a panel (`element`: the element's id, None for the panel itself), sent to
    /// its plugin in the plugin's next frame. A checkbox clicked is ticked or unticked, a tab
    /// chosen, a field starts taking the keys: each a change for the plugin.
    pub fn click(&mut self, owner: u64, panel: &str, element: Option<&str>) {
        if let Some(el) = element {
            if self.control_click(owner, panel, el) {
                return;
            }
        }
        if self.clicks.len() >= MAX_CLICKS {
            self.clicks.remove(0);
        }
        self.clicks.push(Click {
            owner,
            panel: panel.to_string(),
            element: element.map(str::to_string),
        });
    }

    /// A click on a control; false when `el` is none.
    fn control_click(&mut self, owner: u64, panel: &str, el: &str) -> bool {
        let serial = self.next_serial();
        let Some(e) = self.find(owner, panel) else {
            return false;
        };
        let Some(c) = find_element(&mut e.panel.children, el) else {
            return false;
        };
        let value = match &mut c.kind {
            Kind::Checkbox { checked, .. } => {
                *checked = !*checked;
                Value::Bool(*checked)
            }
            Kind::Input { .. } => {
                self.typing = Some(Typing {
                    owner,
                    panel: panel.to_string(),
                    element: el.to_string(),
                });
                return true;
            }
            _ => return false,
        };
        e.revision = serial;
        self.push_change(owner, panel, el, value);
        true
    }

    fn push_change(&mut self, owner: u64, panel: &str, element: &str, value: Value) {
        // (a slider dragged: only its latest value waits)
        if let Some(c) = self
            .changes
            .iter_mut()
            .find(|c| c.owner == owner && c.panel == panel && c.element == element)
        {
            c.value = value;
            return;
        }
        if self.changes.len() >= MAX_CLICKS {
            self.changes.remove(0);
        }
        self.changes.push(Change {
            owner,
            panel: panel.to_string(),
            element: element.to_string(),
            value,
        });
    }

    /// The game sets a control's value (a slider at `fraction` of its range, a tab chosen):
    /// changed, it is a change for the plugin.
    pub fn set_control(&mut self, owner: u64, panel: &str, element: &str, fraction: f32) {
        let serial = self.next_serial();
        let Some(e) = self.find(owner, panel) else {
            return;
        };
        let Some(c) = find_element(&mut e.panel.children, element) else {
            return;
        };
        let value = match &mut c.kind {
            Kind::Slider {
                value,
                min,
                max,
                step,
            } => {
                let mut v = *min + fraction.clamp(0.0, 1.0) * (*max - *min);
                if *step > 0.0 {
                    v = (*min + ((v - *min) / *step).round() * *step)
                        .clamp(min.min(*max), max.max(*min));
                }
                if v == *value {
                    return;
                }
                *value = v;
                Value::Num(v as f64)
            }
            Kind::Tabs { tabs, selected } => {
                let n = tabs.len().max(1);
                let i = ((fraction.clamp(0.0, 0.9999) * n as f32) as usize).min(n - 1);
                if i == *selected {
                    return;
                }
                *selected = i;
                Value::Int(i as i64 + 1)
            }
            _ => return,
        };
        e.revision = serial;
        self.push_change(owner, panel, element, value);
    }

    /// The field being typed into, if any.
    pub fn typing(&self) -> Option<&Typing> {
        self.typing.as_ref()
    }

    /// Text typed into the field (`None`: one character rubbed out); a change for its plugin.
    pub fn type_text(&mut self, text: Option<&str>) {
        let Some(t) = self.typing.clone() else { return };
        let serial = self.next_serial();
        let Some(e) = self.find(t.owner, &t.panel) else {
            return;
        };
        let Some(c) = find_element(&mut e.panel.children, &t.element) else {
            return;
        };
        let Kind::Input {
            text: have, max, ..
        } = &mut c.kind
        else {
            return;
        };
        match text {
            Some(s) => {
                for ch in s.chars().filter(|c| !c.is_control()) {
                    if have.chars().count() < *max {
                        have.push(ch);
                    }
                }
            }
            None => {
                have.pop();
            }
        }
        let value = Value::Str(have.clone());
        e.revision = serial;
        self.push_change(t.owner, &t.panel, &t.element, value);
    }

    /// Typing ends (Enter, Esc, a click elsewhere): `submit` sends `ui_submit` for Enter.
    pub fn stop_typing(&mut self, submit: bool) {
        let Some(t) = self.typing.take() else { return };
        if submit {
            if self.clicks.len() >= MAX_CLICKS {
                self.clicks.remove(0);
            }
            // (Enter in a field is a click on it: the plugin takes the text)
            self.clicks.push(Click {
                owner: t.owner,
                panel: t.panel,
                element: Some(t.element),
            });
        }
    }

    /// A panel dragged by `dx`, `dy` logical pixels.
    pub fn drag(&mut self, owner: u64, id: &str, dx: f32, dy: f32) {
        if let Some(e) = self.find(owner, id) {
            e.moved.0 += dx;
            e.moved.1 += dy;
        }
    }

    /// The clicks waiting for a plugin.
    pub fn take_clicks(&mut self, owner: u64) -> Vec<Click> {
        if self.clicks.is_empty() {
            return Vec::new();
        }
        let (mine, rest) = std::mem::take(&mut self.clicks)
            .into_iter()
            .partition(|c| c.owner == owner);
        self.clicks = rest;
        mine
    }

    /// The changes waiting for a plugin.
    pub fn take_changes(&mut self, owner: u64) -> Vec<Change> {
        if self.changes.is_empty() {
            return Vec::new();
        }
        let (mine, rest) = std::mem::take(&mut self.changes)
            .into_iter()
            .partition(|c| c.owner == owner);
        self.changes = rest;
        mine
    }
}

fn find_element<'a>(list: &'a mut [Element], id: &str) -> Option<&'a mut Element> {
    for e in list.iter_mut() {
        if e.id.as_deref() == Some(id) {
            return Some(e);
        }
        if let Kind::Row { children, .. } | Kind::Box { children, .. } = &mut e.kind {
            if let Some(f) = find_element(children, id) {
                return Some(f);
            }
        }
    }
    None
}

fn find_in<'a>(list: &'a [Element], id: &str) -> Option<&'a Element> {
    for e in list {
        if e.id.as_deref() == Some(id) {
            return Some(e);
        }
        if let Kind::Row { children, .. } | Kind::Box { children, .. } = &e.kind {
            if let Some(f) = find_in(children, id) {
                return Some(f);
            }
        }
    }
    None
}

fn check_id(what: &str, id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err(format!("{what} is empty"));
    }
    if id.chars().count() > MAX_ID {
        return Err(format!("{what} is longer than {MAX_ID} characters"));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
