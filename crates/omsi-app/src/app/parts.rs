//! `App` split into the parts of the game the frame's stages declare (`frame_stages::Group`):
//! one borrow per group, taken apart from the whole `App` at once, so that a stage handed
//! only its groups (a context made by [`stage_ctx!`]) can touch nothing else, and the
//! compiler holds the stage to what it declares.
//!
//! [`App::parts`] names every field of `App` and of `SessionState` without `..`: a new field
//! does not build until it is given its group here (and in the access test's table,
//! `frame_stages/access_tests.rs`, for the stages that still take the whole `App`).

// (a group's fields are all here, also those no stage context takes yet)
#![allow(dead_code)]

use super::*;

/// What every stage may read: the command line, the settings, when the game started, the
/// frame's last instant, the exit under way. (The stages that change them take the `App`.)
pub(crate) struct Common<'p> {
    pub(crate) args: &'p Args,
    pub(crate) settings: &'p settings::Settings,
    pub(crate) started: Instant,
    pub(crate) last: Instant,
    pub(crate) exiting: bool,
}

/// `Group::Input`.
pub(crate) type InputPart<'p> = &'p mut InputState;

/// `Group::Menus`: the start menu, the game menu, the pause menu.
pub(crate) struct MenusPart<'p> {
    pub(crate) menus: &'p mut MenuState,
    pub(crate) shell: &'p mut crate::shell::Shell,
}

/// `Group::Clock`: the sim clock, the pause, the day the world shows, the clock's jumps.
pub(crate) struct ClockPart<'p> {
    pub(crate) clock: &'p mut omsi_sim::SimClock,
    pub(crate) paused: &'p mut bool,
    pub(crate) world_day: &'p mut Option<(i32, Option<String>)>,
    pub(crate) clock_jump: &'p mut f64,
    pub(crate) pending_time: &'p mut Option<f64>,
    pub(crate) autosave_t: &'p mut f64,
    /// The fixed step's accumulator, when it is on (`ticked`).
    pub(crate) fixed_tick: &'p mut omsi_sim::tick::SimTicks,
}

/// `Group::Weather`: the weather, the daylight's envir, the wet roads, the cabin air, the spray.
pub(crate) struct WeatherPart<'p> {
    pub(crate) rain: &'p mut rain::Rain,
    pub(crate) cabin_air: &'p mut crate::condensation::CabinAir,
    pub(crate) spray: &'p mut puddles::Spray,
    pub(crate) lamps_on: &'p mut Option<bool>,
    pub(crate) envir: &'p mut Option<omsi_content::Envir>,
    pub(crate) weather: &'p mut Option<omsi_content::weather::Weather>,
    pub(crate) wetness: &'p mut f32,
    pub(crate) cloud_drift: &'p mut [f64; 4],
    pub(crate) weather_blend: &'p mut Option<crate::weather_cycle::Blend>,
    pub(crate) weather_cycle: &'p mut Option<crate::weather_cycle::Cycle>,
    pub(crate) metar_rx: &'p mut Option<std::sync::mpsc::Receiver<Option<omsi_content::weather::Weather>>>,
    pub(crate) metar_once: &'p mut bool,
    pub(crate) metar_next: &'p mut f64,
    pub(crate) sky: &'p mut crate::weather_setup::SkyState,
}

/// `Group::World`: the map and its scenery objects' simulation.
pub(crate) struct WorldPart<'p> {
    pub(crate) world: &'p mut Option<Arc<World>>,
    pub(crate) scenery: &'p mut omsi_sim::scenery::SceneryState,
}

/// `Group::Traffic`: the AI traffic and the timetable.
pub(crate) struct TrafficPart<'p> {
    pub(crate) traffic: &'p mut Option<traffic::Traffic>,
    pub(crate) schedule: &'p mut Option<schedule::Schedule>,
    pub(crate) timetable_book: &'p mut Option<schedule::Schedule>,
    pub(crate) populate_t: &'p mut f32,
    pub(crate) traffic_heavy_frame: &'p mut bool,
    pub(crate) first_populate: &'p mut bool,
    /// The simulation's own thread and what it has under way (`sim_thread`).
    pub(crate) pipeline: &'p mut crate::sim_thread::Pipeline,
}

/// `Group::Player`: the player's vehicle and the vehicles placed.
pub(crate) struct PlayerPart<'p> {
    pub(crate) player: &'p mut Option<Player>,
    pub(crate) placed: &'p mut Vec<Player>,
    pub(crate) seat_bus: &'p mut String,
    pub(crate) safe_pose: &'p mut Option<(glam::DVec3, f64)>,
    pub(crate) safe_age: &'p mut f32,
    pub(crate) pumping: &'p mut Option<Pumping>,
}

/// `Group::People`: the people and the player on foot.
pub(crate) struct PeoplePart<'p> {
    pub(crate) humans: &'p mut Option<humans::Humans>,
    pub(crate) humans_populate_t: &'p mut f32,
    pub(crate) on_foot: &'p mut Option<crate::on_foot::OnFoot>,
    pub(crate) passenger_question: &'p mut Option<crate::app::PassengerQuestion>,
    pub(crate) passenger_dialogs: &'p mut u64,
}

/// `Group::Duty`: the duty, the career, the journey.
pub(crate) struct DutyPart<'p> {
    pub(crate) duty: &'p mut Option<schedule::PlayerDuty>,
    pub(crate) duty_places: &'p mut bool,
    pub(crate) career: &'p mut career::Career,
    pub(crate) journey: &'p mut Option<crate::journey::Journey>,
}

/// `Group::Net`.
pub(crate) type NetPart<'p> = &'p mut NetState;

/// `Group::Integrations`.
pub(crate) type IntegrationsPart<'p> = &'p mut Integrations;

/// `Group::Sound`.
pub(crate) type SoundPart<'p> = &'p mut SoundState;

/// `Group::View`: the camera and the views.
pub(crate) struct ViewPart<'p> {
    pub(crate) camera: &'p mut Option<Camera>,
    pub(crate) cam: &'p mut ViewState,
    pub(crate) view: &'p mut String,
}

/// `Group::Ui`: the interface, the photo mode, the developer tools, the service message.
pub(crate) struct UiPart<'p> {
    pub(crate) ui: &'p mut Option<ui::Ui>,
    pub(crate) photo: &'p mut Option<crate::photo::Photo>,
    pub(crate) photo_settings: &'p mut Option<crate::photo::Settings>,
    pub(crate) devtools: &'p mut Option<Box<crate::devtools::DevTools>>,
    pub(crate) service_msg: &'p mut Option<(String, f32)>,
}

/// `Group::Gfx`: what draws the picture.
pub(crate) struct GfxPart<'p> {
    pub(crate) gfx: &'p mut GfxState,
    pub(crate) window: &'p mut Option<Arc<Window>>,
    pub(crate) renderer: &'p mut Option<Renderer>,
    pub(crate) scene: &'p mut Option<Scene>,
    pub(crate) xr: &'p mut VrState,
}

/// `Group::Perf`.
pub(crate) type PerfPart<'p> = &'p mut PerfState;

/// The whole `App`, a borrow per group (see the module's notes).
pub(crate) struct Parts<'p> {
    pub(crate) common: Common<'p>,
    pub(crate) input: InputPart<'p>,
    pub(crate) menus: MenusPart<'p>,
    pub(crate) clock: ClockPart<'p>,
    pub(crate) weather: WeatherPart<'p>,
    pub(crate) world: WorldPart<'p>,
    pub(crate) traffic: TrafficPart<'p>,
    pub(crate) player: PlayerPart<'p>,
    pub(crate) people: PeoplePart<'p>,
    pub(crate) duty: DutyPart<'p>,
    pub(crate) net: NetPart<'p>,
    pub(crate) integrations: IntegrationsPart<'p>,
    pub(crate) sound: SoundPart<'p>,
    pub(crate) view: ViewPart<'p>,
    pub(crate) ui: UiPart<'p>,
    pub(crate) gfx: GfxPart<'p>,
    pub(crate) perf: PerfPart<'p>,
}

impl App {
    /// The `App` taken apart by group (see the module's notes).
    pub(crate) fn parts(&mut self) -> Parts<'_> {
        let App {
            args, gfx, window, renderer, xr, scene, camera, player, session, menus, world, cam, ui,
            perf, sound, clock, started, view, input, last, paused, integrations, net, service_msg,
            settings, exiting, shell, photo, photo_settings, devtools, pipeline, fixed_tick,
        } = self;
        let SessionState {
            placed, world_day, traffic, schedule, timetable_book, humans, duty, duty_places, rain,
            cabin_air, spray, lamps_on, populate_t, humans_populate_t, traffic_heavy_frame,
            first_populate, envir, weather, clock_jump, seat_bus, on_foot, safe_pose, safe_age,
            pending_time, autosave_t, pumping, career, journey, wetness, cloud_drift, weather_blend,
            weather_cycle, metar_rx, metar_once, metar_next, sky, scenery, passenger_question,
            passenger_dialogs, accident_review: _, accident_reason: _, red_light_watch: _, road_offence_cool: _,
            wrong_way_watch: _,
            priority_offence_cool: _, courtesy_watch: _,
        } = session;
        Parts {
            common: Common { args, settings, started: *started, last: *last, exiting: *exiting },
            input,
            menus: MenusPart { menus, shell },
            clock: ClockPart { clock, paused, world_day, clock_jump, pending_time, autosave_t, fixed_tick },
            weather: WeatherPart {
                rain, cabin_air, spray, lamps_on, envir, weather, wetness, cloud_drift,
                weather_blend, weather_cycle, metar_rx, metar_once, metar_next, sky,
            },
            world: WorldPart { world, scenery },
            traffic: TrafficPart { traffic, schedule, timetable_book, populate_t, traffic_heavy_frame, first_populate, pipeline },
            player: PlayerPart { player, placed, seat_bus, safe_pose, safe_age, pumping },
            people: PeoplePart { humans, humans_populate_t, on_foot, passenger_question, passenger_dialogs },
            duty: DutyPart { duty, duty_places, career, journey },
            net,
            integrations,
            sound,
            view: ViewPart { camera, cam, view },
            ui: UiPart { ui, photo, photo_settings, devtools, service_msg },
            gfx: GfxPart { gfx, window, renderer, scene, xr },
            perf,
        }
    }
}

/// The type of a group's part (`stage_ctx!`).
macro_rules! part_ty {
    (input, $p:lifetime) => { $crate::app::InputPart<$p> };
    (menus, $p:lifetime) => { $crate::app::MenusPart<$p> };
    (clock, $p:lifetime) => { $crate::app::ClockPart<$p> };
    (weather, $p:lifetime) => { $crate::app::WeatherPart<$p> };
    (world, $p:lifetime) => { $crate::app::WorldPart<$p> };
    (traffic, $p:lifetime) => { $crate::app::TrafficPart<$p> };
    (player, $p:lifetime) => { $crate::app::PlayerPart<$p> };
    (people, $p:lifetime) => { $crate::app::PeoplePart<$p> };
    (duty, $p:lifetime) => { $crate::app::DutyPart<$p> };
    (net, $p:lifetime) => { $crate::app::NetPart<$p> };
    (integrations, $p:lifetime) => { $crate::app::IntegrationsPart<$p> };
    (sound, $p:lifetime) => { $crate::app::SoundPart<$p> };
    (view, $p:lifetime) => { $crate::app::ViewPart<$p> };
    (ui, $p:lifetime) => { $crate::app::UiPart<$p> };
    (gfx, $p:lifetime) => { $crate::app::GfxPart<$p> };
    (perf, $p:lifetime) => { $crate::app::PerfPart<$p> };
}

/// The group of a part's name (`stage_ctx!`).
macro_rules! part_group {
    (input) => { $crate::frame_stages::Group::Input };
    (menus) => { $crate::frame_stages::Group::Menus };
    (clock) => { $crate::frame_stages::Group::Clock };
    (weather) => { $crate::frame_stages::Group::Weather };
    (world) => { $crate::frame_stages::Group::World };
    (traffic) => { $crate::frame_stages::Group::Traffic };
    (player) => { $crate::frame_stages::Group::Player };
    (people) => { $crate::frame_stages::Group::People };
    (duty) => { $crate::frame_stages::Group::Duty };
    (net) => { $crate::frame_stages::Group::Net };
    (integrations) => { $crate::frame_stages::Group::Integrations };
    (sound) => { $crate::frame_stages::Group::Sound };
    (view) => { $crate::frame_stages::Group::View };
    (ui) => { $crate::frame_stages::Group::Ui };
    (gfx) => { $crate::frame_stages::Group::Gfx };
    (perf) => { $crate::frame_stages::Group::Perf };
}

/// A stage's context: the groups it reads (shared borrows) and writes (mutable borrows) of
/// the `App`'s [`Parts`], and nothing else but [`Common`]; its `READS` and `WRITES` are the
/// stage's declaration (`StageInfo::reads`/`writes`), so what a stage declares is what it
/// can touch.
///
/// `stage_ctx!(WheelSurfaceCx { reads: [world], writes: [player] });`
macro_rules! stage_ctx {
    ($(#[$m:meta])* $name:ident { reads: [$($r:ident),* $(,)?], writes: [$($w:ident),* $(,)?] $(,)? }) => {
        $(#[$m])*
        #[allow(dead_code)]
        pub(crate) struct $name<'a, 'p> {
            pub(crate) common: &'a $crate::app::Common<'p>,
            $(pub(crate) $r: &'a $crate::app::part_ty!($r, 'p),)*
            $(pub(crate) $w: &'a mut $crate::app::part_ty!($w, 'p),)*
        }

        // (a context a helper takes, not a stage, has no declaration to give)
        #[allow(dead_code)]
        impl<'a, 'p> $name<'a, 'p> {
            /// The groups the stage reads only.
            pub(crate) const READS: &'static [$crate::frame_stages::Group] = &[$($crate::app::part_group!($r)),*];
            /// The groups the stage writes (and may read).
            pub(crate) const WRITES: &'static [$crate::frame_stages::Group] = &[$($crate::app::part_group!($w)),*];

            pub(crate) fn of(p: &'a mut $crate::app::Parts<'p>) -> Self {
                $name { common: &p.common, $($r: &p.$r,)* $($w: &mut p.$w,)* }
            }
        }
    };
}

/// A narrower context taken out of a wider one for a helper: the helper's own `stage_ctx!`
/// struct, its parts reborrowed from `$cx` (a context, or a reference to one). The lists are
/// the helper context's; a part missing in `$cx`, or written there and only read in `$cx`,
/// does not build.
///
/// `narrow!(cx => JumpCx { reads: [world], writes: [clock, traffic, gfx] })`
macro_rules! narrow {
    ($cx:expr => $name:ident { reads: [$($r:ident),* $(,)?], writes: [$($w:ident),* $(,)?] $(,)? }) => {
        $name { common: &*$cx.common, $($r: &*$cx.$r,)* $($w: &mut *$cx.$w,)* }
    };
}

pub(crate) use {narrow, part_group, part_ty, stage_ctx};

/// Every group has its part (a group without one could not be handed to a stage).
#[cfg(test)]
#[test]
fn every_group_has_a_part() {
    use crate::frame_stages::Group::*;
    let all = [Input, Menus, Clock, Weather, World, Traffic, Player, People, Duty, Net, Integrations, Sound, View, Ui, Gfx, Perf];
    stage_ctx!(Everything {
        reads: [],
        writes: [input, menus, clock, weather, world, traffic, player, people, duty, net, integrations, sound, view, ui, gfx, perf]
    });
    assert_eq!(Everything::WRITES, all);
    assert!(Everything::READS.is_empty());
}
