//! The bus company's welcome and tutorial (Luc: "een welkomstscherm voor de busbedrijfmodus,
//! met een tutorial"): the first time a driver opens the company, its tour starts by itself -
//! a welcome card with what the mode is, then (once a company runs) its pages one by one, each
//! with its part lit (`tour::start_company`). It can be skipped; where it was left is kept per
//! driver in `~/.openomsi/company-tour.json`, and the "?" in the company's bar goes on from
//! there (or from the start, once it was seen to the end). A tour that only showed the welcome
//! and the founding goes on through the pages by itself once the company is founded.
//!
//! When the company's pages were tabs, the tour had its stops in another order. A driver who
//! saw that tour is shown the new start screen once, from its tiles on (`LAYOUT`), and a stop
//! where it was left then is found again in the new order (`resume_at`).

use super::super::Launcher;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// What a driver has seen of the company's tour.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Seen {
    /// It was shown (the welcome at least).
    pub seen: bool,
    /// Where it was left when it was skipped (its stop, 0: the start).
    pub at: usize,
    /// Gone through to its end.
    pub done: bool,
    /// The welcome was shown before there was a company: its pages are still to come.
    pub pages_to_come: bool,
    /// The company's layout it was seen in (`LAYOUT`; 0: its pages as tabs, before the start
    /// screen).
    pub layout: u32,
}

/// The company's layout now: 1, the start screen with a tile for every page.
pub const LAYOUT: u32 = 1;

/// The stop of the tour of the tabs' days (with a company: the welcome, the clock, the tabs,
/// then the pages) in the start screen's tour (the welcome, the tiles, what the company has,
/// the clock, the way round, then the pages).
fn from_tabs(at: usize) -> usize {
    match at {
        0 => 0,
        1 => 3,
        2 => 1,
        k => k + 2,
    }
}

fn file() -> PathBuf {
    omsi_launcher_lib::data_dir().join("company-tour.json")
}

fn read_all<T: serde::de::DeserializeOwned>(path: &Path) -> BTreeMap<String, T> {
    std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn write_all<T: Serialize>(path: &Path, all: &BTreeMap<String, T>) {
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if let Err(e) = std::fs::write(path, serde_json::to_vec_pretty(all).unwrap_or_default()) {
        log::warn!("company tour: not kept: {e}");
    }
}

/// What a driver has seen (nothing yet: the default).
pub(crate) fn of(profile: &str) -> Seen {
    read_all(&file()).get(profile.trim()).cloned().unwrap_or_default()
}

fn keep(profile: &str, s: Seen) {
    let path = file();
    let mut all = read_all(&path);
    all.insert(profile.trim().to_string(), s);
    write_all(&path, &all);
}

/// The tour was left at its stop `at`: gone through (`completed`) or skipped.
pub(crate) fn left(profile: &str, at: usize, completed: bool) {
    let mut s = of(profile);
    s.seen = true;
    s.layout = LAYOUT;
    if completed {
        // (a tour of the welcome and the founding alone is not the pages')
        if !s.pages_to_come {
            s.done = true;
        }
        s.at = 0;
    } else {
        s.at = at;
    }
    keep(profile, s);
}

/// What starting the tour now means for a driver who has seen `s`, with a company running or
/// not: (start it by itself, at which stop, the pages still to come after it).
pub fn on_open(s: &Seen, company: bool) -> Option<(usize, bool)> {
    if !s.seen {
        return Some((0, !company));
    }
    if s.pages_to_come && company {
        // (the welcome was seen: on with the pages)
        return Some((1, false));
    }
    if s.layout < LAYOUT && company {
        // (seen when the pages were tabs: the start screen shown once, from its tiles)
        return Some((1, false));
    }
    None
}

/// Where the "?" starts the tour: where it was left, else from the start.
pub fn resume_at(s: &Seen) -> usize {
    if s.done {
        0
    } else if s.layout < LAYOUT {
        from_tabs(s.at)
    } else {
        s.at
    }
}

/// Once a frame on the company's screen: the tour started by itself the first time (not on a
/// phone, not over a dialog, not while another tour runs).
pub(super) fn frame(l: &mut Launcher) {
    if super::super::mobile::mobile() || super::super::tour::active(l) || l.company.companies.is_none() || l.company.dialog.is_some() {
        return;
    }
    let profile = l.state.config.profile.clone();
    if l.company.tutorial_for.as_deref() == Some(profile.as_str()) {
        return;
    }
    l.company.tutorial_for = Some(profile.clone());
    let company = l.company.company.is_some() && l.company.wizard.is_none();
    let mut s = of(&profile);
    let Some((at, to_come)) = on_open(&s, company) else { return };
    s.seen = true;
    s.pages_to_come = to_come;
    if company {
        s.layout = LAYOUT;
    }
    keep(&profile, s);
    super::super::tour::start_company(l, at);
}

/// A company was founded: the tour may go on with its pages (looked at again next frame).
pub(super) fn founded(l: &mut Launcher) {
    l.company.tutorial_for = None;
}

/// The "?" in the company's bar: the tour from where it was left.
pub(super) fn ask(l: &mut Launcher) {
    let profile = l.state.config.profile.clone();
    let s = of(&profile);
    super::super::tour::start_company(l, resume_at(&s));
}

// --- RosterFlow's tutorial ----------------------------------------------------------------------
//
// RosterFlow (the planning page) has a tour and lessons of its own (`planning::rf_tutorial`,
// Luc: "build a tutorial for the new RosterFlow: I no longer understand how it works, let alone
// other players"). What a driver has seen of them is kept beside the company's tour, in
// `~/.openomsi/rosterflow-tour.json`; a file of before, or one without a field, reads as
// nothing seen (`#[serde(default)]`). The tour itself (`tour::start_flow`) reaches RosterFlow's
// tabs and drawings through here.

/// What a driver has seen of RosterFlow's tour and lessons.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct FlowSeen {
    /// The tour was shown (it started by itself once).
    pub seen: bool,
    /// Where it was left when it was skipped (its stop among all of them).
    pub at: usize,
    /// Gone through to its end.
    pub done: bool,
    /// The lessons done to their end (their ids).
    pub lessons: Vec<String>,
}

fn flow_file() -> PathBuf {
    omsi_launcher_lib::data_dir().join("rosterflow-tour.json")
}

/// What a driver has seen of RosterFlow's tutorial (nothing yet: the default).
pub(crate) fn flow_of(profile: &str) -> FlowSeen {
    read_all(&flow_file()).get(profile.trim()).cloned().unwrap_or_default()
}

pub(crate) fn flow_keep(profile: &str, s: FlowSeen) {
    let path = flow_file();
    let mut all = read_all(&path);
    all.insert(profile.trim().to_string(), s);
    write_all(&path, &all);
}

/// RosterFlow's tour was left at its stop `at`: gone through (`completed`) or skipped.
pub(crate) fn rf_left(profile: &str, at: usize, completed: bool) {
    let mut s = flow_of(profile);
    s.seen = true;
    if completed {
        s.done = true;
        s.at = 0;
    } else {
        s.at = at;
    }
    flow_keep(profile, s);
}

/// Where RosterFlow's tour goes on: where it was left, from the start once seen to its end.
pub(crate) fn flow_resume_at(s: &FlowSeen, stops: usize) -> usize {
    if s.done || s.at + 1 >= stops {
        0
    } else {
        s.at
    }
}

/// A lesson done to its end, kept (its tick in the "?" menu).
pub(crate) fn flow_lesson_done(profile: &str, id: &str) {
    let mut s = flow_of(profile);
    if !s.lessons.iter().any(|x| x == id) {
        s.lessons.push(id.to_string());
        flow_keep(profile, s);
    }
}

/// The company's page RosterFlow is on, and RosterFlow's tabs (the planning's views).
pub(crate) const RF_TAB: usize = 5;
pub(crate) const RF_ROSTER: usize = super::planning::RF_ROSTER;
pub(crate) const RF_VEHICLES: usize = super::planning::RF_VEHICLES;
pub(crate) const RF_DRIVERS: usize = super::planning::RF_DRIVERS;
pub(crate) const RF_TIMEPLAN: usize = super::planning::RF_TIMEPLAN;

/// RosterFlow's tab and the timeplan's screen now.
pub(crate) fn rf_view(l: &Launcher) -> (usize, usize) {
    super::planning::rf_view(l)
}

/// RosterFlow on tab `view` (and the timeplan on screen `screen`), as the tour shows it.
pub(crate) fn rf_set_view(l: &mut Launcher, view: usize, screen: Option<usize>) {
    super::planning::rf_set_view(l, view, screen);
}

/// One of RosterFlow's drawings for the tour's cards (0: the chain, 1: the symbols) in `r`, `a`
/// of it seen, `t` seconds since it came (None: standing still).
pub(crate) fn rf_picture(ui: &mut super::super::ui::Ui, which: u8, r: omsi_ui::Rect, a: f32, t: Option<f32>) {
    super::planning::rf_picture(ui, which, r, a, t);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rosterflows_tutorial_is_kept_per_driver_and_old_files_still_read() {
        // nothing seen: the tour from its start; skipped: on from there; done: from the start
        let new = FlowSeen::default();
        assert!(!new.seen);
        assert_eq!(flow_resume_at(&new, 30), 0);
        let skipped = FlowSeen { seen: true, at: 7, ..Default::default() };
        assert_eq!(flow_resume_at(&skipped, 30), 7);
        assert_eq!(flow_resume_at(&FlowSeen { done: true, ..skipped.clone() }, 30), 0);
        assert_eq!(flow_resume_at(&FlowSeen { at: 29, ..skipped.clone() }, 30), 0);
        // a file of before (no lessons in it) and an empty one
        let old: FlowSeen = serde_json::from_str(r#"{ "seen": true, "at": 3 }"#).unwrap();
        assert_eq!((old.at, old.done, old.lessons.len()), (3, false, 0));
        let empty: FlowSeen = serde_json::from_str("{}").unwrap();
        assert_eq!(empty, FlowSeen::default());
        // per driver, beside the company's tour
        let dir = std::env::temp_dir().join(format!("omsi_flow_tour_{}", std::process::id()));
        let path = dir.join("rosterflow-tour.json");
        let _ = std::fs::remove_dir_all(&dir);
        let mut all: BTreeMap<String, FlowSeen> = read_all(&path);
        all.insert("Luc".into(), FlowSeen { seen: true, lessons: vec!["move-trip".into()], ..Default::default() });
        write_all(&path, &all);
        let back: BTreeMap<String, FlowSeen> = read_all(&path);
        assert_eq!(back.get("Luc").map(|s| s.lessons.clone()), Some(vec!["move-trip".to_string()]));
        assert!(!back.contains_key("Anna"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_tour_starts_once_and_goes_on_where_it_was_left() {
        // a driver new to the company: the welcome (and the founding, without a company)
        let new = Seen::default();
        assert_eq!(on_open(&new, false), Some((0, true)));
        assert_eq!(on_open(&new, true), Some((0, false)));
        // seen, the company founded since: on with the pages, past the welcome
        let welcomed = Seen { seen: true, pages_to_come: true, ..Default::default() };
        assert_eq!(on_open(&welcomed, false), None);
        assert_eq!(on_open(&welcomed, true), Some((1, false)));
        // seen and skipped: not by itself again; the "?" goes on where it was left
        let skipped = Seen { seen: true, at: 5, layout: LAYOUT, ..Default::default() };
        assert_eq!(on_open(&skipped, true), None);
        assert_eq!(resume_at(&skipped), 5);
        // seen to the end: the "?" from the start
        assert_eq!(resume_at(&Seen { seen: true, at: 5, done: true, layout: LAYOUT, ..Default::default() }), 0);
    }

    #[test]
    fn a_tour_seen_in_the_tabs_days_shows_the_start_screen_once() {
        // (a file of before: no layout in it)
        let old: Seen = serde_json::from_str(r#"{ "seen": true, "at": 3, "done": false, "pages_to_come": false }"#).unwrap();
        assert_eq!(old.layout, 0);
        // the company open: the tour by itself, from the start screen's tiles (after the welcome)
        assert_eq!(on_open(&old, true), Some((1, false)));
        // the founding still to come: nothing yet
        assert_eq!(on_open(&old, false), None);
        // where it was left is found again in the new order: the overview stays the overview
        assert_eq!(resume_at(&old), 5);
        assert_eq!((from_tabs(0), from_tabs(1), from_tabs(2), from_tabs(10)), (0, 3, 1, 12));
        // seen to the end then: the "?" from the start all the same
        assert_eq!(resume_at(&Seen { done: true, ..old.clone() }), 0);
        // once seen in the new layout, not again
        assert_eq!(on_open(&Seen { layout: LAYOUT, ..old }, true), None);
    }

    #[test]
    fn what_was_seen_is_kept_per_driver() {
        let dir = std::env::temp_dir().join(format!("omsi_company_tour_{}", std::process::id()));
        let path = dir.join("company-tour.json");
        let _ = std::fs::remove_dir_all(&dir);
        let mut all: BTreeMap<String, Seen> = read_all(&path);
        assert!(all.is_empty());
        all.insert("Luc".into(), Seen { seen: true, at: 3, ..Default::default() });
        write_all(&path, &all);
        let back: BTreeMap<String, Seen> = read_all(&path);
        assert_eq!(back.get("Luc").map(|s| s.at), Some(3));
        assert!(!back.contains_key("Anna"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
