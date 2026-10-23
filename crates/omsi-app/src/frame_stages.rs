//! The frame as a declared, ordered list of stages, and the one runner that takes them.
//!
//! The window's frame (`app_events::frame`, `WINDOW_FRAME`) and the offscreen run's step
//! (`offscreen::stages`, `OFFSCREEN_STEP`) are each a list of stages in the order they run.
//! Every stage says what it is ([`StageInfo`]): its name, its phase, the role it plays (so the
//! two lists can be held against each other), the parts of the game it reads and writes, the
//! values it takes from and gives to the stages after it, and the key OMSI_PROFILE times it
//! under. [`run`] takes a list stage by stage, times each one, and stops where a stage ends
//! the frame (the start menu, a server told to stop). `OMSI_FRAME_ORDER=1` logs both lists.
//!
//! Nothing here is an engine: a stage is a plain method of the window's `App` or of the
//! offscreen run, and the list is the order they were always called in.
//!
//! What a window stage declares is checked: a stage handed a context of its groups
//! (`app::stage_ctx!`, taken from `App::parts`) can touch nothing else, and its `reads` and
//! `writes` are that context's; for a stage that still takes the whole `App`, the access
//! test (`access_tests`) reads the crate's source and fails when the stage reaches a field
//! of a group it does not declare.

use std::time::Instant;

use crate::profile_keys::{ProfKey, ProfileTimes};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod access_tests;

/// Where in the frame a stage belongs, roughly in the order of the frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Phase {
    /// The frame's time, the session's housekeeping, the menus that end a frame early.
    Housekeeping,
    /// Keys, controllers, the mouse: what the player does.
    Input,
    /// The simulation: the traffic, the buses, the people, the clock and the weather.
    Sim,
    /// What the renderer shows of the simulation, the sounds and the lights of this frame.
    Present,
    /// The interface over the picture.
    Ui,
    /// The picture drawn and shown (or written), the frame counted.
    Output,
}

/// The parts of the game a stage reads or writes: the window's `App` field groups, and the
/// offscreen run's fields of the same name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Group {
    /// `App::input`, the controllers, the input script.
    Input,
    /// `App::menus`, `App::shell`: the start menu, the game menu, the pause menu.
    Menus,
    /// `App::clock` (the offscreen run's `run_clock`).
    Clock,
    /// The weather, the daylight, the roads' wetness, the cabin air, the spray.
    Weather,
    /// `App::world` and its streamed tiles.
    World,
    /// `session.traffic` and `session.schedule`: the AI and the timetable.
    Traffic,
    /// `App::player` and the vehicles placed.
    Player,
    /// `session.humans`, the player on foot.
    People,
    /// `session.duty`, `session.career`, `session.journey`.
    Duty,
    /// `App::net`: the LAN session and the other players (a dedicated server's administration).
    Net,
    /// `App::integrations`: the plugins, Discord, Steam.
    Integrations,
    /// `App::sound`.
    Sound,
    /// `App::camera`, `App::cam`, `App::view`.
    View,
    /// `App::ui`, the HUD, the photo mode's panel.
    Ui,
    /// `App::gfx`, `App::renderer`, `App::scene`: what is drawn.
    Gfx,
    /// `App::perf`, the logs and the files a run writes.
    Perf,
}

/// The values a stage hands to the stages after it in the same frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Carry {
    /// The frame's time (`FrameTime`).
    Time,
    /// The controllers' axes and their buttons' actions.
    Controls,
    /// The frame's daylight.
    Daylight,
    /// The VR navigator's display.
    VrNavDisplay,
    /// The lighting the picture is drawn with.
    Lighting,
}

/// How OMSI_PROFILE times a stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Profile {
    /// The runner times the whole stage under this key (stages may share one; a key never
    /// has a dot: those are a stage's own parts, `traffic.tick`).
    Key(ProfKey),
    /// The stage times its own parts under keys of its own (the picture: acquire, render,
    /// present, ...).
    Inner,
    /// Not timed: the stage that takes the frame's time itself.
    None,
}

/// What a stage is (see the module's notes).
#[derive(Debug, Clone, Copy)]
pub(crate) struct StageInfo {
    pub(crate) name: &'static str,
    pub(crate) phase: Phase,
    /// What the stage does, as the other list knows it too (the same role in the window and
    /// the offscreen run).
    pub(crate) role: Role,
    /// The groups the stage reads (a stage handed a context: only reads; one that takes the
    /// whole `App`: everything its code reaches but `writes`).
    pub(crate) reads: &'static [Group],
    /// The groups the stage changes (and may read).
    pub(crate) writes: &'static [Group],
    pub(crate) takes: &'static [Carry],
    pub(crate) gives: &'static [Carry],
    pub(crate) profile: Profile,
}

/// The roles the window's and the offscreen run's stages share, for holding their orders
/// against each other; `Own` for what only one of them does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    Clock,
    Traffic,
    Player,
    Duty,
    People,
    Lan,
    Surroundings,
    Picture,
    Own,
}

/// Whether the frame goes on after a stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Flow {
    Go,
    /// The frame ends here (nothing after this stage runs).
    Stop,
}

/// A list's stages: a plain enum whose variants say what they are.
pub(crate) trait Stage: Copy + std::fmt::Debug + 'static {
    fn info(self) -> StageInfo;
}

/// What takes a list's stages: the window's frame, the offscreen run's step.
pub(crate) trait Host<S: Stage> {
    fn stopping(&self) -> bool { false }
    fn run_stage(&mut self, stage: S) -> anyhow::Result<Flow>;
    /// Where the stages' times go (`App::perf.profile`, the offscreen run's own).
    fn profile(&mut self) -> &mut ProfileTimes;
}

/// The runner: `list`'s stages in their order, each timed under its profile key, until one
/// ends the frame.
pub(crate) fn run<S: Stage, H: Host<S>>(host: &mut H, list: &[S]) -> anyhow::Result<Flow> {
    if host.stopping() {
        return Ok(Flow::Stop);
    }
    for &stage in list {
        let t = Instant::now();
        let flow = host.run_stage(stage)?;
        if let Profile::Key(k) = stage.info().profile {
            host.profile().add(k, t.elapsed().as_secs_f64());
        }
        if flow == Flow::Stop {
            return Ok(Flow::Stop);
        }
    }
    Ok(Flow::Go)
}

/// A list as text, a stage a line (`OMSI_FRAME_ORDER`).
pub(crate) fn describe<S: Stage>(title: &str, list: &[S]) -> String {
    let mut out = format!("{title}: {} stages", list.len());
    for (k, s) in list.iter().enumerate() {
        let i = s.info();
        let profile = match i.profile {
            Profile::Key(k) => k.name().to_string(),
            Profile::Inner => "(own parts)".into(),
            Profile::None => "-".into(),
        };
        out.push_str(&format!(
            "\n  {:>2} {:<16} {:<12} {:<12} reads {:?} writes {:?} takes {:?} gives {:?} profile {profile}",
            k + 1,
            i.name,
            format!("{:?}", i.phase),
            format!("{:?}", i.role),
            i.reads,
            i.writes,
            i.takes,
            i.gives,
        ));
    }
    out
}

/// `OMSI_FRAME_ORDER=1`: both lists in the log, once.
pub(crate) fn log_order() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    if omsi_cfg::flags::OMSI_FRAME_ORDER.is_set() {
        ONCE.call_once(|| {
            log::info!("{}", describe("window frame", crate::app_events::WINDOW_FRAME));
            log::info!("{}", describe("offscreen step", crate::offscreen::OFFSCREEN_STEP));
        });
    }
}
