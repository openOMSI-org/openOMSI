//! Renderer-specific pause screens, isolated from the game application.

use std::borrow::Cow;
use std::ops::{Deref, DerefMut};

use glam::Vec2;
use omsi_ui::Rect;
use omsi_ui::kit::Ui;

pub mod pause;

pub type MenuLine = (
    &'static str,
    Cow<'static, str>,
    Option<omsi_plugin::ui::MenuItem>,
);

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Choose(usize),
    BusSelect(usize),
    PauseCategory(pause::navigation::Category),
    QuickActions(pause::quick_actions::Edit),
    OnlineAction(u8, String),
    Rail(usize),
    Control(usize, f32),
    Date(String),
    Side(usize),
    Pane(usize),
    Dropdown(usize),
    CloseDropdown,
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum MenuKind {
    #[default]
    Game,
    Options,
    Sections,
    Lines,
    Tours,
    Vehicles,
    List,
}

pub struct Preview {
    pub title: String,
    pub meta: String,
    pub rows: Vec<(String, String)>,
    pub chosen: Option<usize>,
    pub button: Option<String>,
    pub time: Option<String>,
}

pub struct DropdownView<'a> {
    pub row: usize,
    pub items: Vec<&'a str>,
    pub sel: usize,
    pub top: usize,
    pub current: Option<usize>,
}

pub struct ListView<'a> {
    pub kind: MenuKind,
    pub head: Option<(String, String)>,
    pub badge: Option<usize>,
    pub tabs: Option<(Vec<String>, usize)>,
    pub items: Vec<(&'a str, &'a str)>,
    pub sel: usize,
    pub preview: Option<&'a Preview>,
    pub pane_first: Option<usize>,
    pub dropdown: Option<DropdownView<'a>>,
    pub key: String,
}

pub struct PauseView<'a> {
    pub paused: bool,
    pub quick_actions: &'a str,
    pub rail: &'a [MenuLine],
    pub rail_sel: Option<usize>,
    pub disabled: &'a [&'a str],
    pub list: Option<ListView<'a>>,
    pub kbd: bool,
    pub keys: bool,
    pub summary: Summary,
    pub online: omsi_multiplayer_ui::View,
    pub version: &'a str,
    pub catalog: &'a [(&'static str, &'static str)],
    pub selectable: &'a [&'a str],
}

pub const HEADING: &str = "#";

#[derive(Default)]
pub struct Summary {
    pub bus: String,
    pub driver: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PreviewView {
    #[default]
    Outside,
    Passenger,
    Driver,
}

impl PreviewView {
    pub fn name(self) -> &'static str {
        match self {
            Self::Outside => "outside",
            Self::Passenger => "passenger",
            Self::Driver => "driver",
        }
    }
}

#[derive(Default)]
pub struct BusPreview {
    pub busy: bool,
    pub error: Option<String>,
    pub title: String,
    pub meta: String,
    pub views: Vec<&'static str>,
    pub view: PreviewView,
    pub texture: Option<usize>,
    pub ready: bool,
    pub drag: Option<Vec2>,
    pub orbit: (f32, f32),
    pub zoom: f32,
}

pub struct State {
    pub pause_category: pause::navigation::Category,
    pub settings_search: omsi_settings_ui::SearchState,
    pub settings_sections: std::collections::HashMap<String, usize>,
    pub pause_online: omsi_multiplayer_ui::State,
    pub quick_edit: bool,
    pub pause_hidden: bool,
    pub quick_focus: usize,
    pub last_sel: Option<(u64, usize)>,
    pub dd_anchor: Option<Rect>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            pause_category: Default::default(),
            settings_search: Default::default(),
            settings_sections: Default::default(),
            pause_online: Default::default(),
            quick_edit: false,
            pause_hidden: true,
            quick_focus: 0,
            last_sel: None,
            dd_anchor: None,
        }
    }
}

pub struct Context<'a> {
    pub ui: &'a mut Ui,
    pub state: &'a mut State,
    pub actions: Vec<Action>,
    pub bus_preview: &'a mut BusPreview,
    pub logo_tex: Option<usize>,
    pub logo_title: Option<String>,
    pub logo_wide: Option<f32>,
}

impl<'a> Context<'a> {
    pub fn new(
        ui: &'a mut Ui,
        state: &'a mut State,
        bus_preview: &'a mut BusPreview,
        logo_tex: Option<usize>,
        logo_title: Option<String>,
        logo_wide: Option<f32>,
    ) -> Self {
        Self {
            ui,
            state,
            actions: Vec::new(),
            bus_preview,
            logo_tex,
            logo_title,
            logo_wide,
        }
    }
}

impl Deref for Context<'_> {
    type Target = State;
    fn deref(&self) -> &State {
        self.state
    }
}

impl DerefMut for Context<'_> {
    fn deref_mut(&mut self) -> &mut State {
        self.state
    }
}

pub type Shell<'a> = Context<'a>;
