//! The frame's order pinned: both lists stage by stage, what must come before what, and how
//! the window's frame and the offscreen run's step agree.

use super::*;
use crate::app_events::{WindowStage, WINDOW_FRAME};
use crate::offscreen::{OffscreenStage, OFFSCREEN_STEP};

fn names<S: Stage>(list: &[S]) -> Vec<&'static str> {
    list.iter().map(|s| s.info().name).collect()
}

fn pos<S: Stage + PartialEq>(list: &[S], s: S) -> usize {
    list.iter().position(|x| *x == s).unwrap_or_else(|| panic!("{s:?} is not in the list"))
}

fn roles<S: Stage>(list: &[S]) -> Vec<Role> {
    list.iter().map(|s| s.info().role).filter(|r| *r != Role::Own).collect()
}

#[test]
fn the_window_frame_runs_in_this_order() {
    assert_eq!(
        names(WINDOW_FRAME),
        [
            // once a frame
            "timing", "shell_actions", "placement", "menus", "streaming", "controllers", "mouse_drive",
            "pad_actions",
            // once per tick
            "traffic", "player_tick", "on_foot", "humans", "after_humans", "duty", "weather", "wheel_surface",
            // once a frame
            "player_view", "audio", "lan", "integrations", "view_keys", "drawn", "lights", "scripted", "photo", "ui",
            "lighting", "trace_look", "render",
        ]
    );
}

#[test]
fn the_offscreen_step_runs_in_this_order() {
    assert_eq!(
        names(OFFSCREEN_STEP),
        ["server", "clock", "traffic", "startup", "duty", "drive", "ground_gap", "humans", "lan", "surroundings", "snapshot", "record"]
    );
}

#[test]
fn every_stage_is_listed_once_under_a_name_of_its_own() {
    fn check<S: Stage + PartialEq>(list: &[S]) {
        let mut seen = std::collections::HashSet::new();
        for s in list {
            assert!(seen.insert(s.info().name), "{s:?} twice, or its name is another's");
            assert_eq!(list.iter().filter(|x| *x == s).count(), 1, "{s:?} listed twice");
        }
    }
    check(WINDOW_FRAME);
    check(OFFSCREEN_STEP);
}

#[test]
fn the_window_frame_starts_with_its_time_and_ends_with_its_picture() {
    use WindowStage::*;
    assert_eq!(WINDOW_FRAME.first(), Some(&Timing));
    assert_eq!(WINDOW_FRAME.last(), Some(&Render));
    // the start menu ends a frame before anything of a session runs
    let menus = pos(WINDOW_FRAME, Menus);
    for (k, s) in WINDOW_FRAME.iter().enumerate() {
        if matches!(s.info().phase, Phase::Sim | Phase::Input | Phase::Present | Phase::Ui) {
            assert!(k > menus, "{s:?} runs before the menus");
        }
    }
}

#[test]
fn input_comes_before_the_bus_and_the_bus_before_its_passengers() {
    use WindowStage::*;
    let w = |s| pos(WINDOW_FRAME, s);
    assert!(w(Controllers) < w(MouseDrive) && w(MouseDrive) < w(PadActions) && w(PadActions) < w(PlayerTick));
    // the passengers sit in the bus's frame: placed on this tick's pose, not the last one's
    assert!(w(PlayerTick) < w(Humans) && w(PlayerTick) < w(AfterHumans));
    // the traffic's cars the bus collides with are set before the bus moves
    assert!(w(Traffic) < w(PlayerTick));
    // the simulation's stages are the tick's; what the player does comes before them, the
    // view of them after
    use crate::app_events::{FRAME_AFTER, FRAME_BEFORE, FRAME_TICK};
    for s in [Traffic, PlayerTick, OnFoot, Humans, AfterHumans, Duty, Weather] {
        assert!(FRAME_TICK.contains(&s), "{s:?} not in the tick");
    }
    assert!(FRAME_BEFORE.contains(&PadActions) && FRAME_AFTER.contains(&PlayerView) && FRAME_AFTER.contains(&Render));
    use OffscreenStage as O;
    let o = |s| pos(OFFSCREEN_STEP, s);
    assert!(o(O::Traffic) < o(O::Drive) && o(O::Drive) < o(O::Humans));
    assert!(o(O::Startup) < o(O::Duty) && o(O::Duty) < o(O::Drive));
}

#[test]
fn the_simulation_comes_before_what_is_shown_of_it() {
    fn check<S: Stage>(list: &[S]) {
        let last_sim = list.iter().rposition(|s| s.info().phase == Phase::Sim).unwrap();
        let first_output = list.iter().position(|s| s.info().phase == Phase::Output).unwrap();
        assert!(last_sim < first_output, "a simulation stage runs after the picture started");
        let picture = list.iter().position(|s| s.info().role == Role::Picture).unwrap();
        for s in &list[picture..] {
            assert_eq!(s.info().phase, Phase::Output, "{s:?} after the picture");
        }
    }
    check(WINDOW_FRAME);
    check(OFFSCREEN_STEP);
    // the interface is drawn over the frame's own state, and the picture with it
    use WindowStage::*;
    let w = |s| pos(WINDOW_FRAME, s);
    assert!(w(Weather) < w(Ui) && w(Ui) < w(Render) && w(Lighting) < w(Render));
}

#[test]
fn what_a_stage_takes_an_earlier_stage_gave() {
    fn check<S: Stage>(list: &[S]) {
        let mut given: Vec<Carry> = Vec::new();
        for s in list {
            let i = s.info();
            for c in i.takes {
                assert!(given.contains(c), "{} takes {c:?} that no stage before it gives", i.name);
            }
            given.extend(i.gives);
        }
    }
    check(WINDOW_FRAME);
    check(OFFSCREEN_STEP);
}

#[test]
fn every_stage_says_what_it_works_on_and_how_it_is_timed() {
    fn check<S: Stage>(list: &[S]) {
        for s in list {
            let i = s.info();
            assert!(!i.writes.is_empty(), "{} writes nothing", i.name);
            if let Profile::Key(k) = i.profile {
                // (a dot marks a stage's own parts, which the stutter log leaves out of the sum)
                assert!(!k.name().contains('.') && !k.name().is_empty(), "{}: profile key {k:?}", i.name);
            }
        }
    }
    check(WINDOW_FRAME);
    check(OFFSCREEN_STEP);
}

/// The window and the offscreen run take the shared work in the same order, but for the
/// differences that were always there: the offscreen run sets its clock first (the window
/// after the people), takes the duty before the drive (the window after the people), and
/// the LAN once a step (the window once a frame, after its ticks).
#[test]
fn the_window_and_the_offscreen_run_agree_on_the_shared_order() {
    use Role::*;
    assert_eq!(roles(WINDOW_FRAME), [Traffic, Player, People, Duty, Clock, Lan, Surroundings, Picture]);
    assert_eq!(roles(OFFSCREEN_STEP), [Clock, Traffic, Duty, Player, People, Lan, Surroundings, Picture]);
    let known = [Clock, Duty, Lan];
    let shared = |list: Vec<Role>| list.into_iter().filter(|r| !known.contains(r)).collect::<Vec<_>>();
    assert_eq!(shared(roles(WINDOW_FRAME)), shared(roles(OFFSCREEN_STEP)));
    assert_eq!(shared(roles(WINDOW_FRAME)), [Traffic, Player, People, Surroundings, Picture]);
}

/// A list of stages for the runner's own tests: what ran, and where the frame stops.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Probe {
    A,
    B,
    Halt,
    C,
}

impl Stage for Probe {
    fn info(self) -> StageInfo {
        let name = match self {
            Probe::A => "a",
            Probe::B => "b",
            Probe::Halt => "halt",
            Probe::C => "c",
        };
        let profile = match self {
            Probe::A | Probe::B => Profile::Key(ProfKey::Clock),
            Probe::Halt => Profile::None,
            Probe::C => Profile::Key(ProfKey::Drive),
        };
        StageInfo { name, phase: Phase::Sim, role: Role::Own, reads: &[], writes: &[Group::Perf], takes: &[], gives: &[], profile }
    }
}

#[derive(Default)]
struct Recorder {
    ran: Vec<Probe>,
    profile: ProfileTimes,
}

impl Host<Probe> for Recorder {
    fn run_stage(&mut self, s: Probe) -> anyhow::Result<Flow> {
        self.ran.push(s);
        Ok(if s == Probe::Halt { Flow::Stop } else { Flow::Go })
    }
    fn profile(&mut self) -> &mut ProfileTimes {
        &mut self.profile
    }
}

#[test]
fn the_runner_takes_the_stages_in_order_until_one_ends_the_frame() {
    let mut r = Recorder::default();
    assert_eq!(run(&mut r, &[Probe::A, Probe::B, Probe::C]).unwrap(), Flow::Go);
    assert_eq!(r.ran, [Probe::A, Probe::B, Probe::C]);
    // two stages under one key add up; an untimed stage leaves no key
    assert_eq!(r.profile.iter().map(|(k, _)| k).collect::<Vec<_>>(), [ProfKey::Clock, ProfKey::Drive]);
    let mut r = Recorder::default();
    assert_eq!(run(&mut r, &[Probe::A, Probe::Halt, Probe::C]).unwrap(), Flow::Stop);
    assert_eq!(r.ran, [Probe::A, Probe::Halt]);
    assert_eq!(r.profile.iter().map(|(k, _)| k).collect::<Vec<_>>(), [ProfKey::Clock]);
}

#[test]
fn the_order_dump_names_every_stage() {
    let text = describe("window frame", WINDOW_FRAME);
    for n in names(WINDOW_FRAME) {
        assert!(text.contains(n), "{n} missing from the dump");
    }
    assert!(text.starts_with(&format!("window frame: {} stages", WINDOW_FRAME.len())));
}
