//! The window's frame as lists of stages, and what takes them: the `App` with the frame's
//! event loop and the values one stage hands to the next (see `frame_stages`). The frame runs
//! [`FRAME_BEFORE`] once, [`FRAME_TICK`] once per tick of the fixed simulation step
//! (`omsi_sim::tick`, 120 a second, with the tick's time) and [`FRAME_AFTER`] once (see
//! `ticked`); [`WINDOW_FRAME`] is the three in that order, as a frame of one tick runs them.

use super::*;
use crate::frame_stages::{self, Carry, Flow, Group, Phase, Profile, Role, StageInfo};

/// The window's stages, in the order of [`WINDOW_FRAME`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowStage {
    Timing,
    ShellActions,
    Placement,
    Menus,
    Streaming,
    Traffic,
    Controllers,
    MouseDrive,
    PadActions,
    /// The player's bus's step, once per tick.
    PlayerTick,
    /// The player's bus drawn between its last two ticks, and the view from it.
    PlayerView,
    Audio,
    Lan,
    OnFoot,
    Humans,
    AfterHumans,
    Duty,
    Integrations,
    ViewKeys,
    Drawn,
    Weather,
    Lights,
    Scripted,
    WheelSurface,
    Photo,
    Ui,
    Lighting,
    TraceLook,
    Render,
}

/// Once a frame, before the ticks: the session's housekeeping, the graphics device and the
/// frame's time (none: the session ends), what the pause menu's clicks of the last frame
/// asked for, a vehicle chosen in the menu (read on a worker meanwhile, put down once it is
/// ready), the start menu and the map's first area still loading (the frame ends there), the
/// tiles streamed in, and what the player does.
pub(crate) const FRAME_BEFORE: &[WindowStage] = {
    use WindowStage::*;
    &[Timing, ShellActions, Placement, Menus, Streaming, Controllers, MouseDrive, PadActions]
};

/// Once per tick, with the tick's time. The player's vehicle moves before the passengers are
/// placed: they sit in the bus frame, and placing them on the pose of the tick before made
/// everyone aboard tremble at speed (a quarter of a metre behind the seat, every frame).
pub(crate) const FRAME_TICK: &[WindowStage] = {
    use WindowStage::*;
    &[Traffic, PlayerTick, OnFoot, Humans, AfterHumans, Duty, Weather, WheelSurface]
};

/// Once a frame, after the ticks, with the frame's time: the view of the simulation, the
/// sounds, the LAN, the interface (with the photo mode's camera and panel) and the picture.
pub(crate) const FRAME_AFTER: &[WindowStage] = {
    use WindowStage::*;
    &[PlayerView, Audio, Lan, Integrations, ViewKeys, Drawn, Lights, Scripted, Photo, Ui, Lighting, TraceLook, Render]
};

const FRAME_LEN: usize = FRAME_BEFORE.len() + FRAME_TICK.len() + FRAME_AFTER.len();

/// The frame's three lists one after the other (what the order's tests and dump take).
pub(crate) const WINDOW_FRAME: &[WindowStage] = &{
    let lists = [FRAME_BEFORE, FRAME_TICK, FRAME_AFTER];
    let mut out = [WindowStage::Timing; FRAME_LEN];
    let (mut n, mut l) = (0, 0);
    while l < lists.len() {
        let mut k = 0;
        while k < lists[l].len() {
            out[n] = lists[l][k];
            n += 1;
            k += 1;
        }
        l += 1;
    }
    out
};

const fn stage(
    name: &'static str,
    phase: Phase,
    role: Role,
    reads: &'static [Group],
    writes: &'static [Group],
    profile: Profile,
) -> StageInfo {
    StageInfo { name, phase, role, reads, writes, takes: &[Carry::Time], gives: &[], profile }
}

impl frame_stages::Stage for WindowStage {
    fn info(self) -> StageInfo {
        use Group as G;
        use Phase as P;
        use crate::profile_keys::ProfKey as K;
        use WindowStage::*;
        match self {
            Timing => StageInfo {
                takes: &[],
                gives: &[Carry::Time],
                ..stage("timing", P::Housekeeping, Role::Own, &[G::Clock, G::Gfx, G::Perf, G::View, G::Net, G::Input, G::Menus, G::Weather, G::Traffic, G::Player, G::People, G::Duty, G::Ui], &[G::Clock, G::Gfx, G::Perf, G::View, G::Net, G::World], Profile::None)
            },
            ShellActions => StageInfo { takes: &[], ..stage("shell_actions", P::Housekeeping, Role::Own, &[G::Menus, G::Input, G::Traffic, G::People, G::Duty, G::Net, G::Integrations, G::Sound, G::View, G::Ui, G::Gfx, G::Perf], &[G::Menus, G::Player, G::World, G::Clock, G::Weather], Profile::Key(K::Shell)) },
            Placement => StageInfo { takes: &[], ..stage("placement", P::Housekeeping, Role::Own, crate::app_impl::PlacementCx::READS, crate::app_impl::PlacementCx::WRITES, Profile::Key(K::Placement)) },
            Menus => stage("menus", P::Housekeeping, Role::Own, &[G::Menus, G::Input, G::World, G::Clock, G::Weather, G::Traffic, G::People, G::Duty, G::Integrations, G::Sound, G::View, G::Ui, G::Perf], &[G::Menus, G::Input, G::Gfx, G::Net, G::World, G::Player], Profile::None),
            Streaming => StageInfo { takes: &[], ..stage("streaming", P::Sim, Role::Own, crate::app::StreamingCx::READS, crate::app::StreamingCx::WRITES, Profile::Key(K::Streaming)) },
            Traffic => stage("traffic", P::Sim, Role::Traffic, super::road::TrafficCx::READS, super::road::TrafficCx::WRITES, Profile::Key(K::Traffic)),
            Controllers => StageInfo { gives: &[Carry::Controls], ..stage("controllers", P::Input, Role::Own, &[G::Input, G::Player, G::Menus, G::Clock, G::Weather, G::World, G::Traffic, G::People, G::Duty, G::Net, G::Integrations, G::View, G::Gfx], &[G::Input, G::Ui], Profile::Key(K::Player)) },
            MouseDrive => StageInfo { takes: &[Carry::Time, Carry::Controls], gives: &[Carry::Controls], ..stage("mouse_drive", P::Input, Role::Own, &[G::Input, G::Player, G::View, G::Menus, G::Clock, G::World, G::People, G::Duty, G::Net, G::Integrations, G::Ui, G::Gfx], &[G::Input, G::View], Profile::Key(K::Player)) },
            PadActions => StageInfo { takes: &[Carry::Time, Carry::Controls], ..stage("pad_actions", P::Input, Role::Own, &[G::Clock, G::Weather, G::World, G::Traffic, G::People, G::Duty, G::Net, G::Integrations, G::Ui, G::Gfx, G::Perf], &[G::Input, G::Player, G::Menus, G::View], Profile::Key(K::Player)) },
            PlayerTick => stage("player_tick", P::Sim, Role::Player, super::driving::PlayerStepCx::READS, super::driving::PlayerStepCx::WRITES, Profile::Key(K::Player)),
            PlayerView => stage("player_view", P::Present, Role::Own, &[G::Player, G::View, G::Clock, G::Input, G::World, G::Menus, G::People, G::Duty, G::Net, G::Sound, G::Ui, G::Perf], &[G::Player, G::View, G::Gfx], Profile::Key(K::Player)),
            Audio => stage("audio", P::Present, Role::Own, super::driving::AudioCx::READS, super::driving::AudioCx::WRITES, Profile::Key(K::Player)),
            Lan => stage("lan", P::Sim, Role::Lan, &[G::Net, G::Player, G::Clock, G::Duty, G::Input, G::Menus, G::Weather, G::World, G::Integrations, G::View, G::Ui], &[G::Net, G::Traffic, G::People, G::Gfx, G::Sound], Profile::Key(K::Lan)),
            OnFoot => stage("on_foot", P::Sim, Role::Own, crate::on_foot::OnFootCx::READS, crate::on_foot::OnFootCx::WRITES, Profile::Key(K::OnFoot)),
            Humans => stage("humans", P::Sim, Role::People, super::people::HumansCx::READS, super::people::HumansCx::WRITES, Profile::Key(K::Humans)),
            AfterHumans => stage("after_humans", P::Sim, Role::Own, super::people::AfterHumansCx::READS, super::people::AfterHumansCx::WRITES, Profile::Key(K::Service)),
            Duty => stage("duty", P::Sim, Role::Duty, &[G::Duty, G::Player, G::People, G::Clock, G::World, G::Net, G::Input, G::Menus, G::Traffic, G::Sound, G::View, G::Gfx], &[G::Duty, G::Player, G::Integrations, G::Ui, G::Net], Profile::Key(K::Duty)),
            Integrations => stage("integrations", P::Sim, Role::Own, &[G::Integrations, G::Player, G::Duty, G::World, G::Net, G::Input, G::Clock, G::Weather, G::Traffic, G::People, G::Sound, G::View, G::Ui, G::Gfx, G::Perf], &[G::Integrations, G::Player, G::Menus, G::Duty], Profile::Key(K::Integrations)),
            ViewKeys => stage("view_keys", P::Input, Role::Own, super::view_keys::ViewKeysCx::READS, super::view_keys::ViewKeysCx::WRITES, Profile::Key(K::ViewKeys)),
            Drawn => stage("drawn", P::Present, Role::Own, &[G::Clock, G::Net, G::World, G::People, G::Traffic, G::Player, G::View, G::Gfx], &[G::Player, G::Traffic, G::People, G::View, G::Gfx], Profile::Key(K::Player)),
            Weather => StageInfo { gives: &[Carry::Daylight], ..stage("weather", P::Sim, Role::Clock, &[G::Clock, G::Weather, G::World, G::Net, G::Ui, G::Perf], &[G::Clock, G::Weather, G::Traffic, G::Player, G::Gfx], Profile::Key(K::Weather)) },
            Lights => StageInfo { takes: &[Carry::Time, Carry::Daylight], ..stage("lights", P::Present, Role::Surroundings, super::surroundings::LightsCx::READS, super::surroundings::LightsCx::WRITES, Profile::Key(K::LightsAndRain)) },
            Scripted => StageInfo { takes: &[Carry::Time, Carry::Daylight], ..stage("scripted", P::Present, Role::Own, super::surroundings::ScriptedCx::READS, super::surroundings::ScriptedCx::WRITES, Profile::Key(K::Scripted)) },
            WheelSurface => StageInfo { takes: &[], ..stage("wheel_surface", P::Sim, Role::Own, super::WheelSurfaceCx::READS, super::WheelSurfaceCx::WRITES, Profile::Key(K::WheelSurface)) },
            Photo => stage("photo", P::Ui, Role::Own, crate::photo::PhotoCx::READS, crate::photo::PhotoCx::WRITES, Profile::Key(K::Photo)),
            Ui => StageInfo { gives: &[Carry::VrNavDisplay], ..stage("ui", P::Ui, Role::Own, &[G::Menus, G::Player, G::Duty, G::People, G::Integrations, G::Input, G::World, G::Clock, G::Weather, G::Traffic, G::Net, G::Sound, G::View, G::Perf], &[G::Ui, G::Gfx, G::Menus], Profile::Inner) },
            Lighting => StageInfo { takes: &[Carry::Time, Carry::Daylight], gives: &[Carry::Lighting], ..stage("lighting", P::Output, Role::Own, super::present::LightingCx::READS, super::present::LightingCx::WRITES, Profile::Key(K::Lighting)) },
            TraceLook => StageInfo { takes: &[], ..stage("trace_look", P::Output, Role::Own, super::TraceLookCx::READS, super::TraceLookCx::WRITES, Profile::None) },
            Render => StageInfo { takes: &[Carry::Time, Carry::Lighting, Carry::VrNavDisplay], ..stage("render", P::Output, Role::Picture, &[G::Gfx, G::View, G::Ui, G::Input, G::Menus, G::Clock, G::World, G::Traffic, G::People, G::Duty, G::Net], &[G::Gfx, G::Perf, G::Integrations, G::Player], Profile::Inner) },
        }
    }
}

const NO_TIME: &str = "the timing stage comes first";

/// The window's frame as the runner takes it: the `App`, the event loop, and what the
/// stages hand on (each `Carry` of the stages' `gives`).
pub(crate) struct WindowFrame<'a> {
    app: &'a mut App,
    event_loop: &'a ActiveEventLoop,
    time: Option<FrameTime>,
    controls: Option<(crate::controllers::Analog, Vec<(String, bool)>)>,
    daylight: Option<omsi_sim::Daylight>,
    vr_nav_display: Option<crate::vr_navigator::Display>,
    lighting: Option<omsi_render::Lighting>,
    /// While the simulation's stages run: the tick's time, which they take instead of the
    /// frame's (see `ticked`).
    tick_dt: Option<f32>,
}

impl<'a> WindowFrame<'a> {
    pub(crate) fn new(app: &'a mut App, event_loop: &'a ActiveEventLoop) -> Self {
        WindowFrame { app, event_loop, time: None, controls: None, daylight: None, vr_nav_display: None, lighting: None, tick_dt: None }
    }

    /// The `App` between the frame's lists.
    pub(super) fn app(&mut self) -> &mut App {
        self.app
    }

    /// The frame's time as the simulation would take it (after the timing stage).
    pub(super) fn frame_dt(&self) -> f32 {
        self.time.as_ref().expect(NO_TIME).dt
    }

    /// The tick's time the simulation's stages take from now on (none: the frame's again).
    pub(super) fn set_tick_dt(&mut self, dt: Option<f32>) {
        self.tick_dt = dt;
    }

    pub(super) fn daylight(&self) -> Option<omsi_sim::Daylight> {
        self.daylight
    }

    pub(super) fn set_daylight(&mut self, d: omsi_sim::Daylight) {
        self.daylight = Some(d);
    }
}

impl frame_stages::Host<WindowStage> for WindowFrame<'_> {
    fn run_stage(&mut self, stage: WindowStage) -> anyhow::Result<Flow> {
        use WindowStage::*;
        let app = &mut *self.app;
        let el = self.event_loop;
        let times = self.time.as_ref().map(|t| (t.dt, t.raw_dt));
        let tick_dt = self.tick_dt;
        let dt = || tick_dt.unwrap_or_else(|| times.expect(NO_TIME).0);
        match stage {
            Timing => match app.frame_timing(el) {
                Some(t) => self.time = Some(t),
                None => return Ok(Flow::Stop),
            },
            ShellActions => app.apply_shell_actions(el),
            Placement => crate::app_impl::placement(crate::app_impl::PlacementCx::of(&mut app.parts())),
            Menus => {
                if !app.frame_menus(el, dt()) {
                    return Ok(Flow::Stop);
                }
            }
            Streaming => crate::app::streaming(crate::app::StreamingCx::of(&mut app.parts())),
            Traffic => super::road::traffic(super::road::TrafficCx::of(&mut app.parts()), dt()),
            Controllers => self.controls = Some(app.frame_controllers(dt())),
            MouseDrive => {
                let (analog, actions) = self.controls.take().expect("the controllers stage comes first");
                self.controls = Some((app.frame_mouse_drive(dt(), analog), actions));
            }
            PadActions => {
                let (analog, actions) = self.controls.take().expect("the controllers stage comes first");
                app.frame_pad_actions(analog, actions);
                app.touch_frame(dt());
            }
            PlayerTick => super::driving::player_step(super::driving::PlayerStepCx::of(&mut app.parts()), dt()),
            PlayerView => app.frame_player_view(dt()),
            Audio => super::driving::audio(super::driving::AudioCx::of(&mut app.parts()), dt()),
            Lan => app.frame_lan(dt()),
            OnFoot => super::people::on_foot(crate::on_foot::OnFootCx::of(&mut app.parts()), dt()),
            Humans => super::people::humans(super::people::HumansCx::of(&mut app.parts()), dt()),
            AfterHumans => super::people::after_humans(super::people::AfterHumansCx::of(&mut app.parts()), dt()),
            Duty => app.frame_duty(dt()),
            Integrations => app.frame_integrations(el, dt()),
            ViewKeys => super::view_keys::view_keys(super::view_keys::ViewKeysCx::of(&mut app.parts()), dt()),
            Drawn => app.frame_drawn(),
            Weather => self.daylight = Some(app.frame_weather(dt())),
            Lights => {
                let daylight = self.daylight.expect(NO_DAYLIGHT);
                let behind = app.drawn_behind();
                let sim = app.player_to_drawn_pose();
                super::surroundings::lights(super::surroundings::LightsCx::of(&mut app.parts()), dt(), daylight, behind);
                app.player_back_to_sim_pose(sim);
            }
            Scripted => super::surroundings::scripted(super::surroundings::ScriptedCx::of(&mut app.parts()), dt(), self.daylight.expect(NO_DAYLIGHT)),
            WheelSurface => super::wheel_surface(super::WheelSurfaceCx::of(&mut app.parts())),
            Photo => crate::photo::photo(crate::photo::PhotoCx::of(&mut app.parts()), times.expect(NO_TIME).1),
            Ui => self.vr_nav_display = app.frame_ui(dt()),
            Lighting => {
                let daylight = self.daylight.expect(NO_DAYLIGHT);
                let sim = app.player_to_drawn_pose();
                self.lighting = Some(super::present::lighting(super::present::LightingCx::of(&mut app.parts()), dt(), daylight));
                app.player_back_to_sim_pose(sim);
            }
            TraceLook => super::trace_look(super::TraceLookCx::of(&mut app.parts())),
            Render => {
                let lighting = self.lighting.take().expect("the lighting stage comes first");
                // (OMSI_SIM_THREAD: the steps the stages left run beside the picture, back
                // before the frame is counted - see `sim_thread`)
                app.start_sim_thread();
                let sim = app.player_to_drawn_pose();
                app.frame_render(el, self.time.as_ref().expect(NO_TIME), &lighting, self.vr_nav_display.take());
                app.player_back_to_sim_pose(sim);
                app.join_sim_thread();
            }
        }
        Ok(Flow::Go)
    }

    fn profile(&mut self) -> &mut crate::profile_keys::ProfileTimes {
        &mut self.app.perf.profile
    }
}

const NO_DAYLIGHT: &str = "the weather stage comes first";
