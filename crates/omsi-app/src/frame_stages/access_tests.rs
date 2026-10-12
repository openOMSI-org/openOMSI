//! The window's stages' declared groups held against what their code touches.
//!
//! A stage handed a context of its groups (`app::stage_ctx!`) is held to it by the compiler.
//! The stages that still take the whole `App` are checked here: the crate's source is read
//! (`omsi_srcscan`), each stage's `run_stage` arm gives the `App` methods it calls, and the
//! `App` fields those reach (through further `App` methods and the functions handed the
//! `App`) must belong to a group the stage declares.

use super::*;
use std::collections::BTreeSet;

/// Where an `App` field (or a `SessionState` field, as `session.<name>`) belongs: a group,
/// or `None` for what every stage may read (the arguments, the settings, the time the game
/// started and the frame's last instant, the exit under way). The same as `App::parts`.
fn field_group(field: &str) -> Option<Option<Group>> {
    use Group::*;
    Some(match field {
        "args" | "settings" | "started" | "last" | "exiting" => None,
        "input" => Some(Input),
        "menus" | "shell" => Some(Menus),
        "clock" | "paused" | "fixed_tick" => Some(Clock),
        "world" | "session.scenery" => Some(World),
        "player" => Some(Player),
        "integrations" => Some(Integrations),
        "net" => Some(Net),
        "sound" => Some(Sound),
        "camera" | "cam" | "view" => Some(View),
        "ui" | "photo" | "photo_settings" | "devtools" | "service_msg" => Some(Ui),
        "gfx" | "window" | "renderer" | "scene" | "xr" => Some(Gfx),
        "perf" => Some(Perf),
        "session.placed" | "session.seat_bus" | "session.safe_pose" | "session.safe_age" | "session.pumping" => Some(Player),
        "session.traffic" | "session.schedule" | "session.timetable_book" | "session.populate_t"
        | "session.traffic_heavy_frame" | "session.first_populate" | "pipeline" => Some(Traffic),
        "session.humans" | "session.humans_populate_t" | "session.on_foot" => Some(People),
        "session.duty" | "session.duty_places" | "session.career" | "session.journey" => Some(Duty),
        "session.world_day" | "session.clock_jump" | "session.pending_time" | "session.autosave_t" => Some(Clock),
        "session.rain" | "session.cabin_air" | "session.spray" | "session.lamps_on" | "session.envir" | "session.weather"
        | "session.wetness" | "session.cloud_drift" | "session.weather_blend" | "session.weather_cycle" | "session.metar_rx"
        | "session.metar_once" | "session.metar_next" | "session.sky" => Some(Weather),
        _ => return None,
    })
}

/// The group of a part's name in a stage context (`app::stage_ctx!`).
fn part_group(part: &str) -> Group {
    use Group::*;
    match part {
        "input" => Input,
        "menus" => Menus,
        "clock" => Clock,
        "weather" => Weather,
        "world" => World,
        "traffic" => Traffic,
        "player" => Player,
        "people" => People,
        "duty" => Duty,
        "net" => Net,
        "integrations" => Integrations,
        "sound" => Sound,
        "view" => View,
        "ui" => Ui,
        "gfx" => Gfx,
        "perf" => Perf,
        _ => panic!("no group for the part {part:?}"),
    }
}

fn src(path: &str) -> String {
    std::fs::read_to_string(format!("{}/src/{path}", env!("CARGO_MANIFEST_DIR"))).unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[test]
fn every_app_field_has_a_group() {
    let mut missing: Vec<String> = omsi_srcscan::struct_fields(&src("app.rs"), "App")
        .into_iter()
        .filter(|f| f != "session" && field_group(f).is_none())
        .collect();
    let session = omsi_srcscan::struct_fields(&src("app/groups.rs"), "SessionState");
    assert!(!session.is_empty(), "no SessionState in app/groups.rs");
    missing.extend(session.into_iter().map(|f| format!("session.{f}")).filter(|f| field_group(f).is_none()));
    assert!(missing.is_empty(), "fields without a group (frame_stages/access_tests.rs, field_group): {missing:?}");
}

#[test]
fn window_stages_touch_only_what_they_declare() {
    use crate::app_events::WINDOW_FRAME;
    let stages = WINDOW_FRAME.iter();
    let sources = omsi_srcscan::sources(std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src")));
    let code = omsi_srcscan::Code::read(&sources, "App");
    // the contexts the `App`'s methods take of it (`XCx::of(&mut self.parts())`), by name
    let contexts: std::collections::BTreeMap<String, Vec<Group>> = sources
        .iter()
        .flat_map(|(_, s)| omsi_srcscan::contexts(s))
        .map(|(name, parts)| (format!("{name}::of"), parts.iter().map(|p| part_group(p)).collect()))
        .collect();
    let arms = omsi_srcscan::match_arms(&src("app_events/frame/stages.rs"), "run_stage", "stage");
    assert!(arms.len() >= WINDOW_FRAME.len(), "run_stage's arms not found: {:?}", arms.keys());
    let mut wrong = Vec::new();
    for s in stages {
        let i = s.info();
        let arm = arms.get(&format!("{s:?}")).unwrap_or_else(|| panic!("no run_stage arm for {s:?}"));
        // (a stage handed its context, `XCx::of(&mut app.parts())`, is held to it by the
        // compiler; what its arm calls on the `App` besides is checked here as for the others)
        let (mut fields, mut entries) = code.scan("app", arm);
        entries.remove("parts");
        fields.extend(code.reach(&entries));
        let declared: BTreeSet<Group> = i.reads.iter().chain(i.writes).copied().collect();
        // (None: what every stage may read; not in the table: no field of `App`)
        let touched: BTreeSet<Group> = fields
            .iter()
            .filter_map(|f| field_group(f).flatten())
            .chain(fields.iter().filter_map(|f| contexts.get(f)).flatten().copied())
            .collect();
        let undeclared: Vec<_> = touched.difference(&declared).collect();
        if !undeclared.is_empty() {
            wrong.push(format!("{}: reaches {undeclared:?} (from {entries:?})", i.name));
        }
    }
    assert!(wrong.is_empty(), "stages reach groups they do not declare (app_events/frame/stages.rs):\n{}", wrong.join("\n"));
}

#[test]
fn the_on_screen_wheel_and_pedals_reach_the_bus_before_its_ticks() {
    use crate::app_events::{WindowStage, FRAME_BEFORE};
    assert!(FRAME_BEFORE.contains(&WindowStage::PadActions));
    let arms = omsi_srcscan::match_arms(&src("app_events/frame/stages.rs"), "run_stage", "stage");
    let arm = arms.get("PadActions").expect("no run_stage arm for PadActions");
    let pads = arm.find("frame_pad_actions(").expect("PadActions does not set the controllers' axes");
    let touch = arm.find("touch_frame(").expect("PadActions does not take the on-screen controls");
    assert!(pads < touch, "the controllers' axes overwrite the on-screen controls before the ticks");
}
