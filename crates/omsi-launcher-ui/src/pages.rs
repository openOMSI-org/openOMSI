//! The launcher's other pages: the driver's profile, the settings, the key bindings, the
//! running games, the mods and where things are.

use super::state::{fmt_bytes, hhmm, short_map};
use super::theme::*;
use super::ui::{id_of, matches, ButtonKind, Ui};
use super::{Launcher, Page};
use glam::Vec2;
use omsi_launcher_lib as core;
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};
use serde_json::{json, Value};

// the "Add binding" picker of the keyboard page: OMSI's events and the installed buses' own
// triggers, searchable
mod keybind_picker;
pub use keybind_picker::{key_actions, keybind_picker};
use keybind_picker::{controller_action_choices, KeyActionOption};

#[derive(Default)]
pub struct PagesView {
    pub new_driver: String,
    pub confirm_delete: Option<std::time::Instant>,
    /// The "reset every setting" dialog is open.
    pub confirm_reset: bool,
    pub kb_filter: [String; 2],
    /// The keyboard action picker: which section it adds to, and its search query.
    pub kb_picker: Option<usize>,
    pub kb_picker_filter: String,
    pub kb_picker_source_filter: String,
    /// The complete bus-file provenance list shown from an action row.
    pub kb_source_action: Option<(String, Vec<String>)>,
    /// (section, index) of the binding waiting for a key.
    pub capturing: Option<(usize, usize)>,
    pub drop_hover: bool,
    /// The Mods page's list: its search, its filter (see `MOD_FILTERS`), and the mod whose
    /// deletion is asked about.
    pub mod_search: String,
    pub mod_filter: usize,
    pub mod_confirm: Option<String>,
    /// The mods opened to show what they hold, folder by folder.
    pub mod_open: std::collections::HashSet<String>,
    pub setup_root: Option<String>,
    pub setup_game: Option<String>,
    /// The Controls page's tab: 0 the keyboard, 1 the game controllers.
    pub controls_tab: usize,
    /// Sources indexed by action name, filled as installed vehicle scripts are scanned.
    pub kb_script_actions: Option<std::collections::HashMap<String, Vec<String>>>,
    pub kb_script_actions_rx: Option<std::sync::mpsc::Receiver<omsi_input::describe::ScriptActionScanUpdate>>,
    kb_script_actions_root: Option<std::path::PathBuf>,
    kb_script_cache_loaded: bool,
    kb_script_scan_actions: Option<std::collections::HashMap<String, Vec<String>>>,
    kb_script_scan_paths: Vec<String>,
    kb_source_paths: Vec<String>,
    kb_source_path_set: std::collections::HashSet<String>,
    kb_source_suggestions: Option<(String, Vec<String>)>,
    pub kb_script_scan: (usize, usize, String),
    kb_script_total_buses: usize,
    pub kb_script_scan_complete: bool,
    kb_action_options: Option<Vec<KeyActionOption>>,
    kb_filtered_options: Option<(String, String, Vec<KeyActionOption>)>,
    controller_action_choices: Option<std::sync::Arc<(Vec<String>, Vec<String>)>>,
    /// The Settings page's tab (see `SETTINGS_TABS`).
    pub settings_tab: usize,
    pub pads: PadsView,
    pub tt: super::timetable::TimetableView,
}

/// The game controllers tab: the devices `gamectrler.cfg` sets up, the ones connected now,
/// the one shown, a button being waited for.
#[derive(Default)]
pub struct PadsView {
    pub io: Option<omsi_input::controllers::Devices>,
    /// The set-up assistant, while it runs.
    pub wizard: Option<Wizard>,
    feedback_test: bool,
    pub devices: Option<Vec<omsi_input::controllers::DeviceCfg>>,
    pub selected: usize,
    /// Waiting for a button of the shown device to be pressed (to add its binding).
    pub capturing: bool,
    /// A button found through "Add a button", kept visible even past the highlight.
    pub revealed_button: Option<usize>,
    pub dirty: bool,
    /// The button last pressed on the shown device and when: its line is lit, so that one
    /// sees which it is and what it does, and can give it an action there.
    pub last_pressed: Option<(usize, std::time::Instant)>,
    /// "Remove this device" clicked once, and when: a second click removes it.
    confirm_remove: Option<std::time::Instant>,
}

/// The set-up assistant of a device: the player lets go of everything, then turns the wheel
/// to the left and presses each pedal in turn; what moved most each time is that control
/// (and which way it runs), as OMSI's options dialog has the player choose by hand.
pub struct Wizard {
    pub step: usize,
    /// Where each axis rests, and where it stood at each step (left, throttle, brake,
    /// clutch).
    pub rest: [Option<f32>; 8],
    pub at: Vec<[Option<f32>; 8]>,
    pub error: Option<String>,
    calibration: Option<(std::time::Instant, omsi_input::ffb_calibration::Calibration)>,
    ff_choice: Option<bool>,
    test_strength: f32,
}

impl PadsView {
    pub(super) fn cancel_feedback_test(&mut self) {
        release_feedback(&mut self.io, &mut self.feedback_test);
        if let Some((_, test)) = self.wizard.as_mut().and_then(|w| w.calibration.as_mut()) {
            if test.result.is_none() {
                test.fail("The test was interrupted. Please try again.");
            }
        }
    }

    /// Give every controller handle up before handing the hardware to the game. The normal
    /// controller list uses non-exclusive DirectInput too, not only the force-feedback test.
    pub(super) fn release_io(&mut self) {
        self.cancel_feedback_test();
        self.io = None;
    }
}

fn release_feedback(io: &mut Option<omsi_input::controllers::Devices>, active: &mut bool) {
    if *active {
        *io = None;
        *active = false;
    }
}

// the pages, one file each (what they reach of the launcher besides `Launcher`)
use super::{mobile, state};
mod profile;
mod settings;
mod controls;
mod sessions;
mod mods;
mod setup;
pub use profile::*;
pub use settings::*;
pub use controls::*;
pub use sessions::*;
pub use mods::*;
pub use setup::*;
