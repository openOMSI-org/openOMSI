//! The player: their bus, its controls and cameras.

use super::*;
use omsi_input::auto_shift::GateMemory;

mod cameras;
mod doors;
mod gears;
mod ibis;
mod mouse;
mod picking;

pub(crate) use cameras::*;
pub(crate) use doors::*;
pub(crate) use picking::*;
pub(crate) use mouse::AutoDrag;

fn indicator_toggle_action(state: &mut u8, lever: Option<u8>, want: u8) -> &'static str {
    // Scripts can cancel the lever themselves after a turn; prefer their current state.
    if let Some(lever) = lever { *state = lever; }
    if want == 3 {
        *state = if *state == 3 { 0 } else { 3 };
        "blinker_warn_toggle"
    } else if *state == want {
        *state = 0;
        "blinker_off"
    } else {
        *state = want;
        if want == 1 { "blinker_left_set" } else { "blinker_right_set" }
    }
}

/// The indicator lever to put back after a frame of the script (None: leave it): where
/// the settings keep an indicator on (`cancel` off), the script turning it off by itself
/// in its frame - a bus whose indicator cancels after a turn, on a phone's wheel after a
/// second or two (#451) - is undone. The player's own keys and clicks run as triggers
/// between the frames, so they still turn it off.
fn kept_indicator(cancel: bool, before: Option<f32>, after: Option<f32>) -> Option<f32> {
    let (before, after) = (before?.round(), after?.round());
    (!cancel && (before == 1.0 || before == 2.0) && after == 0.0).then_some(before)
}

/// Whether the view follows the steering (the setting on): only from the driver's seat as the
/// bus places it, its standard camera - a look at the ticket printer or out of the side
/// window from another of the cab's cameras turned with the wheel as well.
pub(crate) fn steering_follows(view: &str, cam_choice: (usize, usize)) -> bool {
    view == "driver" && cam_choice.0 == 0
}

pub(crate) fn steering_view_yaw(current: f32, steering: f32, dt: f32, enabled: bool, angle: f32, response: f32) -> f32 {
    let target = if enabled { steering.clamp(-1.0, 1.0) * angle.clamp(0.0, 60.0) } else { 0.0 };
    current + (target - current) * (1.0 - (-dt.max(0.0) / response.clamp(0.05, 1.0)).exp())
}

pub(crate) fn driver_head_look(
    look: (f32, f32),
    view: &str,
    pitch_deg: f32,
    vr_on: bool,
) -> (f32, f32) {
    if view == "driver" {
        (look.0, look.1 + if vr_on { 0.0 } else { pitch_deg })
    } else {
        look
    }
}

#[cfg(test)]
mod driver_head_look_tests {
    use super::driver_head_look;

    #[test]
    fn only_the_standard_driver_camera_follows_the_steering() {
        assert!(super::steering_follows("driver", (0, 0)));
        assert!(super::steering_follows("driver", (0, 2)));
        assert!(!super::steering_follows("driver", (1, 0)));
        assert!(!super::steering_follows("pax", (0, 0)));
        assert!(!super::steering_follows("outside", (0, 0)));
    }

    #[test]
    fn head_pitch_adjusts_only_non_vr_driver_view() {
        assert_eq!(
            driver_head_look((4.0, 2.0), "driver", 10.0, false),
            (4.0, 12.0)
        );
        assert_eq!(
            driver_head_look((4.0, 2.0), "driver", 10.0, true),
            (4.0, 2.0)
        );
        assert_eq!(driver_head_look((4.0, 2.0), "pax", 10.0, false), (4.0, 2.0));
        assert_eq!(
            driver_head_look((4.0, 2.0), "outside", 10.0, false),
            (4.0, 2.0)
        );
    }
}

/// The vehicle actions of the keys held whose `Inputs/keyboard.cfg` entry has the "held"
/// flag ([`omsi_content::input::KEY_HOLD`], the key list's " *"), by scan code: Omsi.exe
/// (0x6466b8) gives the vehicle their action again every frame the key stays down, not
/// only when it goes down. The stock roller blind turns while Page Up / Page Down are held
/// (`bus_rollband_up`, `_dn`: its line or destination moves by the frame's time on each
/// call); fired once, it moved by a frame's worth and no more (#1098, #1145).
#[derive(Default)]
pub(crate) struct HeldRepeat(hashbrown::HashMap<i32, (Vec<String>, bool)>);

impl HeldRepeat {
    /// The key went down with these actions (its press fires them itself).
    fn press(&mut self, scan: i32, names: Vec<String>) {
        if names.is_empty() {
            self.0.remove(&scan);
        } else {
            self.0.insert(scan, (names, true));
        }
    }

    fn release(&mut self, scan: i32) {
        self.0.remove(&scan);
    }

    /// The actions to fire again this frame: those of every key still held, but not of one
    /// that went down since the last frame - its press was this frame's call.
    fn due(&mut self) -> Vec<String> {
        let mut due = Vec::new();
        for (names, fresh) in self.0.values_mut() {
            if !std::mem::take(fresh) {
                due.extend(names.iter().cloned());
            }
        }
        due
    }
}

/// Everything about the spawned player vehicle.
pub(crate) struct Player {
    /// Which vehicle this is, for as long as the session runs (the people riding in one
    /// the player left know it by this, see `Humans::player_bus_swapped`).
    pub(crate) uid: u64,
    pub(crate) vehicle: omsi_sim::VehicleInstance,
    pub(crate) render: scene::VehicleRender,
    /// Renders of the coupled parts, parallel to `vehicle.trailers`.
    pub(crate) trailer_renders: Vec<scene::VehicleRender>,
    pub(crate) axes: omsi_sim::KeyboardAxes,
    /// A game controller's pedals and steering this frame (they win over the keys).
    pub(crate) analog: crate::controllers::Analog,
    /// The interior cameras chosen (OMSI's
    /// `view_interiorcam_minus`/`_plus`): the driver's and the passengers' camera numbers.
    pub(crate) cam_choice: (usize, usize),
    /// (scan code, modifier bits) → action name, from `Inputs/keyboard.cfg` `[vehicles]`.
    pub(crate) bindings: Vec<omsi_content::KeyBinding>,
    pub(crate) sounds: Option<omsi_audio::SoundSet>,
    /// Mesh index currently pressed with the mouse (its `[mouseevent]` gets `_off` on release).
    pub(crate) pressed_mesh: Option<usize>,
    /// Coupled-part mesh currently pressed.  Articulated buses keep the rear controls in
    /// the trailer model, while their mouse events are still handled by the lead vehicle.
    pub(crate) pressed_trailer_mesh: Option<(usize, usize)>,
    /// Set while the camera looks at the bus from outside (F3): a switch behind a wall or a
    /// window of the bus is out of reach there, and is neither named, clicked nor turned.
    pub(crate) occlude_controls: bool,
    /// The press on `pressed_mesh`: whether the script has a trigger for the click itself,
    /// and how far the mouse has been dragged since (px).
    pub(crate) press_info: (bool, f32),
    /// A control worked only by dragging (the NL/NG driver's door: `cp_Fahrertuer_drag`
    /// and no click trigger) that was clicked: it is dragged over to its other end.
    pub(crate) auto_drag: Option<AutoDrag>,
    /// Running auto-start (Shift+U).
    pub(crate) startup: Option<omsi_sim::startup::StartUp>,
    /// When the running auto-start began (one that has gone on for long is given up by the
    /// next Shift+U).
    pub(crate) startup_at: Option<std::time::Instant>,
    /// The ticket key was pressed this frame (sell the requested ticket).
    pub(crate) give_ticket: bool,
    /// OMSI's `change_give` / `change_take` keys: hand the passenger
    /// at the desk all the change owed at once / take back what lies on the change tray.
    pub(crate) give_change: bool,
    /// The triggers a controller button's door action (`door_<n>`, `doors_all`) fired, by
    /// action: let go, their `_off` is fired (see [`Player::door_key`]).
    pub(crate) door_buttons: hashbrown::HashMap<String, Vec<String>>,
    /// Parts at the end of the train coupled by hand (they can be uncoupled; an articulated
    /// bus's own rear section cannot).
    pub(crate) hand_coupled: usize,
    /// Bound to the rails (see `rail_drive`), and where on them once it is placed.
    pub(crate) rail_bound: bool,
    pub(crate) rail: Option<crate::rail_drive::RailDrive>,
    pub(crate) rail_placed_z: f64,
    /// The driver camera chosen before the timetable or ticket desk camera took over.
    pub(crate) cam_before_special: Option<usize>,
    /// The actions a key started, by its scan code: its release lets go of the same ones,
    /// whatever modifier is held by then (L pressed, Ctrl held, L let go: the release went
    /// to Ctrl+L and the L action stayed held).
    pub(crate) held_keys: hashbrown::HashMap<i32, Vec<String>>,
    /// The held ones of them that are told every frame (see [`HeldRepeat`]).
    pub(crate) held_repeat: HeldRepeat,
    /// Where the driver's head is thrown by the bus's accelerations (vehicle frame, m):
    /// OMSI's `[driverview_moving]`.
    pub(crate) head: Vec3,
    /// Its speed (m/s) and the body's turning rates of the frame before (see `move_head`).
    pub(crate) head_vel: Vec3,
    pub(crate) head_omega: Vec3,
    /// What a head does when nothing moves it: the slow sway of a standing body, kept apart
    /// from `head` so the bus's own head movement is never fed it (Settings -> idle head sway;
    /// read through `head_offset`).
    pub(crate) head_idle: crate::head_idle::HeadIdle,
    /// How far the driver's view is turned into the steering (degrees of yaw; see `move_head`).
    pub(crate) steer_look: f32,
    /// The driver's seat moved (Settings → seat position; bus frame, m).
    pub(crate) seat: Vec3,
    /// The player's turn of each mirror (yaw, pitch degrees; Ctrl+Alt+arrows in the cab).
    pub(crate) mirror_offsets: Vec<[f32; 2]>,
    /// The player's shift of each mirror (bus frame, m: across, along, up; the mirror editor).
    pub(crate) mirror_shifts: Vec<[f32; 3]>,
    /// Degrees added to each mirror camera's field of view (the mirror editor).
    pub(crate) mirror_fovs: Vec<f32>,
    /// A mirror was turned or shifted and is not saved yet.
    pub(crate) mirrors_dirty: bool,
    pub(crate) take_change: bool,
    /// Keys whose `_toggle` this bus does as `_up`/`_down` (see `action`): turned up last.
    pub(crate) toggled_up: hashbrown::HashSet<String>,
    /// H-pattern actions act as momentary gear buttons when this is enabled - those of a
    /// controller (an H shifter) only: a key stays in its gear as in OMSI (#1440).
    pub(crate) momentary_gears: bool,
    /// The actions now firing come from the keyboard (`key`).
    pub(crate) from_keyboard: bool,
    /// The settings' automated manual (#713): a gear lever's gates are worked by the
    /// engine speed (see [`Player::tick_auto_shift`]).
    pub(crate) auto_shift: bool,
    /// Seconds until the automated manual may shift again; the engine's idle speed as seen.
    pub(crate) auto_shift_wait: f32,
    /// The engine speed the governor holds it at under full throttle (rpm; 0 until seen),
    /// which the automated manual shifts up below (#1832), and the climb towards it: the
    /// highest speed of this pull and how long it has not risen.
    pub(crate) auto_shift_max: f32,
    pub(crate) auto_shift_climb: (f32, f32),
    pub(crate) auto_shift_idle: f32,
    /// The gate the gear lever was put into last (see [`GateMemory`]).
    pub(crate) gates: GateMemory,
    /// L switched the side lights on with the headlights (see
    /// [`Player::headlights_with_side_lights`]).
    pub(crate) side_lights_by_l: bool,
    /// The driver figure at the wheel (see `driver.rs`).
    pub(crate) driver: Option<crate::driver::DriverFigure>,
    /// The duty's trip to type into the IBIS once the auto-start has the electrics on:
    /// (line, terminus, the trip's stops, the stop the bus is at: its index and name).
    pub(crate) ibis_duty: Option<(String, String, Vec<String>, (usize, String))>,
    /// A destination picked by hand for the bus's roller blind, kept until the electrics are
    /// on and the auto-start is done (`schedule::turn_roller_blind`).
    pub(crate) blind_pick: Option<schedule::BlindPick>,
    /// The IBIS being typed, with the line and terminus it is typed for.
    pub(crate) ibis_typist: Option<(omsi_sim::ibis::Typist, String, String, Vec<String>)>,
    /// The duty is typed into the IBIS by itself (after Shift+U or `--autostart`): a new
    /// trip of the duty is typed too.
    pub(crate) duty_typed: bool,
    /// The plugins (by owner mark) that leave the IBIS to the driver (`duty.set_auto_ibis`):
    /// the game types the duty into it by itself (Shift+U, `--autostart`, each new trip) only
    /// while there is none. Each plugin lets go of its own hold as it goes.
    pub(crate) ibis_holds: std::collections::BTreeSet<u64>,
    /// The stop a page asked the duty to go on with (`omsi.setNextStop`), for the game to
    /// hand to the duty.
    pub(crate) html_next_stop: Option<usize>,
    /// The IBIS typing looks for its keys on a worker thread (in the window: the trials
    /// would hold a frame up to a second and a half; an offscreen run waits for them).
    pub(crate) ibis_background: bool,
    /// The outside camera's arm (how far out it is swung right now).
    pub(crate) arm: camera_arm::SpringArm,
    /// What `Z`/`X`/`C` last turned on, so a repeat press of the same key turns it back off
    /// (real OMSI's Z/X/C and Shift+numpad 4/6/5 are toggles, not one-shot "set" buttons):
    /// 0 = nothing, 1 = left, 2 = right, 3 = hazard.
    pub(crate) blinker_key_state: u8,
    /// The settings' "Indicators cancel themselves" (`blinker_cancel`): off, the script's
    /// own cancelling after a turn is undone (#451).
    pub(crate) blinker_cancel: bool,
    /// The overhead wire of the map and the trolleybus's poles drawn (see `omsi_scene::trolley_view`).
    pub(crate) trolley_view: Option<Box<omsi_scene::trolley_view::TrolleyView>>,
    /// The sparks of a rail vehicle's pantograph (see `omsi_scene::arc_fx`).
    pub(crate) panto_fx: omsi_scene::arc_fx::ArcFx,
    pub(crate) trolley_sound: crate::trolley_sound::TrolleySound,
    /// The stop announcements the game presses the IBIS on for (see `auto_announce`).
    pub(crate) announce: crate::auto_announce::AutoAnnounce,
}

// Putting a bus into service (Shift+U, `--autostart`) is `omsi_sim::startup`: it presses
// whatever the vehicle's scripts use to switch the electrics on and to crank the engine
// (found in the compiled scripts, the keys of `Inputs/keyboard.cfg` first) and watches the
// result, so a mod whose ignition key turns one notch per press of E starts as well as the
// stock buses. What is already done is left alone: pressed again (or after `--autostart`)
// the battery toggle used to switch the electrics off and every display went dead. No gear
// is selected at the end: the bus spawns in N anyway, and a D pressed while the auto-start
// was still running was thrown back to N by it.

/// OMSI's own layout drives the bus with Shift and the numpad (throttle Shift+Num 8, brake
/// Shift+Num 2, steering Shift+Num 4/6). A laptop or a Mac keyboard has no numpad at all, so
/// the arrow keys drive as well; all they do in OMSI is step through the interior cameras.
///
/// W, A, S and D drive as well, because that is what everybody reaches for - but
/// `Inputs/keyboard.cfg` gives W the wipers, S the viewpoint and **D the D of the automatic
/// gearbox**, so those three are reached by holding shift (Shift+D selects D), and the bus
/// can still be put into gear. `--drive-keys arrows` leaves W/A/S/D to OMSI entirely.
/// The driving keys of a control preset (`drive_keys` in the settings):
/// `omsi` - only the original layout of Inputs/keyboard.cfg (Shift + numpad), nothing extra;
/// `simple` - W/S/A/D and Up/Down drive (plain Left/Right keep OMSI's interior camera
/// switch, view_interiorcam_minus/plus, Omsi.exe 0x706278; A/D steer); `wasd` - W/S/A/D only;
/// `arrows` - the arrow keys only (W/S/D keep their OMSI meaning: wipers, viewpoint, gear).
pub(crate) fn fallback_action(code: KeyCode, preset: &str) -> Option<omsi_sim::EngineAction> {
    use omsi_sim::EngineAction as A;
    let preset = preset.to_ascii_lowercase();
    let (wasd, arrows) = match preset.as_str() {
        "omsi" | "original" => (false, false),
        "wasd" => (true, false),
        "arrows" => (false, true),
        _ => (true, true), // "simple" and anything unknown
    };
    Some(match code {
        KeyCode::ArrowUp if arrows => A::Throttle,
        KeyCode::ArrowDown if arrows => A::Brake,
        KeyCode::ArrowLeft if arrows && preset == "arrows" => A::SteeringLeft,
        KeyCode::ArrowRight if arrows && preset == "arrows" => A::SteeringRight,
        KeyCode::KeyW if wasd => A::Throttle,
        KeyCode::KeyS if wasd => A::Brake,
        KeyCode::KeyA if wasd => A::SteeringLeft,
        KeyCode::KeyD if wasd => A::SteeringRight,
        _ => return None,
    })
}

/// Keyboard actions of `Inputs/keyboard.cfg` that no vehicle script handles under that
/// name: the original translates them into the triggers the scripts do define (the
/// indicators are `kw_blinker_*` in every stock bus, while the key file says
/// `blinker_left_set`). Each action lists the trigger names tried in turn.
pub(crate) const ACTION_ALIASES: &[(&str, &[&str])] = &[
    (
        "blinker_left_set",
        &["kw_blinker_links", "blinker_links", "cp_blinker_links"],
    ),
    (
        "blinker_right_set",
        &["kw_blinker_rechts", "blinker_rechts", "cp_blinker_rechts"],
    ),
    (
        "blinker_off",
        &["kw_blinker_aus", "blinker_aus", "cp_blinker_aus"],
    ),
    (
        "blinker_warn_toggle",
        &["kw_blinker_warn", "blinker_warn", "cp_warnblinker_toggle"],
    ),
    (
        "parking_brake_toggle",
        &["parking_brake_mouse", "cp_feststellbremse_toggle"],
    ),
];

impl Player {
    /// Door key `n` (1 = the front door; 0 = all of them): the triggers fired. All the doors
    /// close the ones open when any is (leaving the others as they are) and else open them
    /// all; the door release switch (`bus_dooraft` of the Berlin buses) is not a door then.
    pub(crate) fn door_key(&mut self, n: usize) -> Vec<String> {
        let groups = door_keys(&self.vehicle.ty);
        let fire: Vec<String> = if n == 0 {
            let doors: Vec<&Vec<String>> = groups.iter().filter(|g| !(g.len() == 1 && g[0] == "bus_dooraft")).collect();
            let is_open = |v: &mut omsi_sim::VehicleInstance, g: &Vec<String>| {
                g.iter().any(|t| match t.split_once('|') {
                    Some((open, _)) => trigger_leaves(&v.ty.program, open).iter().any(|l| v.var(l).is_some_and(|x| x > 0.5)),
                    None => door_trigger_open(v, t) == Some(true),
                })
            };
            let open: Vec<bool> = doors.iter().map(|g| is_open(&mut self.vehicle, g)).collect();
            let any_open = open.iter().any(|o| *o);
            let mut fire = Vec::new();
            for (g, o) in doors.iter().zip(&open) {
                if !any_open || *o {
                    fire.extend(door_group_to_fire(&mut self.vehicle, g));
                }
            }
            fire
        } else {
            let Some(group) = groups.get(n - 1) else { return Vec::new() };
            let fire = door_group_to_fire(&mut self.vehicle, group);
            // the automatic rear doors of the stock Berlin buses (SD, NL): the key is their
            // release, and switched off with the doors open it shuts them now rather than
            // when the last request has lapsed ("why can I not close the rear doors at all?")
            if group.len() == 1 && group[0] == "bus_dooraft" {
                let v = &mut self.vehicle;
                let release_on = v.var("bremse_halte_sw").is_some_and(|x| x > 0.5);
                let open = v.var("doorTarget_23").is_some_and(|x| x > 0.5);
                if release_on && open && v.var("doorAftLastOpen").is_some() {
                    v.set_var("haltewunsch", 0.0);
                    v.set_var("doorAftLastOpen", 1000.0);
                }
            }
            fire
        };
        log::info!("door key {}: {}", if n == 0 { "all".to_string() } else { n.to_string() }, fire.join(" + "));
        for name in &fire {
            self.vehicle.trigger(name);
        }
        fire
    }

    /// `doors_release`: the bus's door release (`VehicleInstance::door_release_trigger`); the triggers fired.
    pub(crate) fn door_release(&mut self) -> Vec<String> {
        let fired: Vec<String> = self.vehicle.door_release_trigger().map(str::to_string).into_iter().collect();
        log::info!("door release: {}", fired.first().map_or("none on this bus", |s| s.as_str()));
        for name in &fired {
            self.vehicle.trigger(name);
        }
        fired
    }

    /// A door key let go: the `_off` of the triggers it fired (the push buttons of the
    /// automatic doors are held between the two).
    pub(crate) fn door_key_off(&mut self, fired: &[String]) {
        for name in fired {
            let off = format!("{name}_off");
            if self.vehicle.ty.program.trigger(&off).is_some() {
                self.vehicle.trigger(&off);
            }
        }
    }

    pub(crate) fn toggle_indicator(&mut self, want: u8) {
        let lever = if self.vehicle.var("lights_sw_warnblinker").is_some_and(|v| v > 0.5) {
            Some(3)
        } else {
            self.vehicle.var("lights_sw_blinker").map(|v| match v.round() as i32 { 1 => 1, 2 => 2, _ => 0 })
        };
        let action = indicator_toggle_action(&mut self.blinker_key_state, lever, want);
        self.action(action, true);
        self.action(action, false);
    }

    /// Fire a keyboard action as a script trigger, falling back to the names the stock
    /// scripts use for it. Returns whether any script block ran.
    pub(crate) fn action(&mut self, name: &str, pressed: bool) -> bool {
        if pressed {
            log::info!("action: {name}");
        }
        if name.eq_ignore_ascii_case("blinker_left_toggle") || name.eq_ignore_ascii_case("blinker_right_toggle") {
            if pressed {
                self.toggle_indicator(if name.eq_ignore_ascii_case("blinker_left_toggle") { 1 } else { 2 });
            }
            return true;
        }
        let suffix = if pressed { "" } else { "_off" };
        if let Some(gate) = crate::hpattern::resolve(&self.vehicle.ty.program, name) {
            if pressed { self.clutch_for_gate(&gate); }
            if let Some(done) = crate::hpattern::action(&mut self.vehicle, &gate, pressed, self.momentary_gears && !self.from_keyboard) { return done; }
        }
        if omsi_input::actions::is_door_release(name) {
            if pressed {
                let fired = self.door_release();
                self.door_buttons.insert("doors_release".into(), fired);
            } else if let Some(fired) = self.door_buttons.remove("doors_release") {
                self.door_key_off(&fired);
            }
            return true;
        }
        // the ticket key of Inputs/keyboard.cfg (T): sell the ticket the passenger at the
        // desk asked for, on buses whose script has no ticket printer
        if let Some(n) = door_action(name) {
            if pressed {
                let fired = self.door_key(n);
                self.door_buttons.insert(name.to_ascii_lowercase(), fired);
            } else if let Some(fired) = self.door_buttons.remove(&name.to_ascii_lowercase()) {
                self.door_key_off(&fired);
            }
            return true;
        }
        if name.eq_ignore_ascii_case("ticket_give") {
            if pressed {
                self.give_ticket = true;
            }
            return true;
        }
        if name.eq_ignore_ascii_case("change_give") || name.eq_ignore_ascii_case("change_take") {
            if pressed {
                if name.eq_ignore_ascii_case("change_give") {
                    self.give_change = true;
                } else {
                    self.take_change = true;
                }
            }
            // the bus's own script may know the key too (a change machine)
            self.vehicle.trigger(&format!("{name}{suffix}"));
            return true;
        }
        if pressed {
            self.clutch_for_gate(name);
        }
        let headlights = pressed && name.eq_ignore_ascii_case("kw_scheinwerfer_toggle");
        let lamps_before = if headlights { self.outside_lamps_lit() } else { 0 };
        if self.vehicle.trigger(&format!("{name}{suffix}")) {
            if headlights {
                self.headlights_with_side_lights(lamps_before);
            }
            return true;
        }
        // a key whose press reached the script's own trigger releases as Omsi.exe does, with
        // `<name>_off` only: an alias's `_off` (parking_brake_mouse_off) would undo it (#420)
        if !pressed && self.vehicle.ty.program.trigger(name).is_some() {
            return true;
        }
        let Some((_, aliases)) = ACTION_ALIASES
            .iter()
            .find(|(a, _)| a.eq_ignore_ascii_case(name))
        else {
            return false;
        };
        for alias in *aliases {
            if self.vehicle.trigger(&format!("{alias}{suffix}")) {
                return true;
            }
        }
        let done = self.toggle_as_steps(name, pressed);
        done
    }

    /// A trigger of `--triggers` at its time (the offscreen run's and the window's): a gate of
    /// a manual gearbox comes with the automatic clutch, as from the keys; the game's door
    /// actions (`door_<n>` / `doors_all`) as a button pressed and let go; else the script's
    /// trigger.
    pub(crate) fn fire_timed(&mut self, name: &str) {
        self.clutch_for_gate(name);
        // (the game's door actions, `door_<n>` / `doors_all` / `doors_release`, as a button
        // pressed and let go)
        if door_action(name).is_some() || omsi_input::actions::is_door_release(name) {
            self.action(name, true);
            self.action(name, false);
        } else {
            self.vehicle.trigger(name);
        }
    }

    /// OMSI's automatic clutch for a gear lever whose scripts only take a gear with the
    /// clutch pedal right down (`(L.L.clutch) 1 =`, the LiAZ and PAZ KPP) and do not read
    /// `AutoClutch` themselves: a gate chosen with a key, a phone's gear button or a
    /// controller comes with the clutch pressed, let up again as the key lets it (and held
    /// while the bus stands, see [`Player::tick`]). Without it a player with no clutch
    /// pedal - every phone, with the automatic clutch on - could not put a gear in at all
    /// (#226). Scripts that read `AutoClutch` (the Sprinters' G32) work it themselves.
    pub(crate) fn clutch_for_gate(&mut self, name: &str) {
        let Some(gate) = name.get(..5).filter(|p| p.eq_ignore_ascii_case("kw_s_")).map(|_| &name[5..]) else { return };
        let gate = gate.strip_suffix("_fest").unwrap_or(gate);
        let is_gate = gate.eq_ignore_ascii_case("r") || gate.eq_ignore_ascii_case("n") || gate.parse::<u32>().is_ok();
        if is_gate && self.vehicle.ty.program.trigger(name).is_some() {
            let to = if gate.eq_ignore_ascii_case("r") { -1 } else { gate.parse::<i32>().unwrap_or(0) };
            self.gates.engaged(to, false);
        }
        let program = &self.vehicle.ty.program;
        // (a manual gearbox only: see `Program::manual_gearbox`)
        if !is_gate || !program.manual_gearbox() || self.vehicle.host.auto_clutch < 0.5 || program.trigger(name).is_none() || program.reads_sys(omsi_script::SysVar::AutoClutch) {
            return;
        }
        self.vehicle.set_var("Clutch", 1.0);
        self.axes.clutch = 1.0;
    }

    /// The I key: all saloon light circuits on, or all off. OMSI binds one key to each
    /// circuit (Inputs/keyboard.cfg: 7 `cp_licht_untenrechts_toggle`, 8
    /// `cp_licht_oberdeck_toggle`, 9 `cp_licht_unterdeck_toggle`) and the buses wire them
    /// differently: the stock MANs light the saloon with all three, the LiAZ 5292 with the
    /// first two and nothing on the third - a shortcut that only threw
    /// `cp_licht_unterdeck` never lit the LiAZ. Every saloon switch the keyboard knows is
    /// pressed, and taken back when it moved the model's `[interiorlight]` lamps the
    /// wrong way (see [`Player::set_saloon_lights`]).
    pub(crate) fn toggle_saloon_lights(&mut self) -> String {
        let on = self.saloon_lamps_lit() == 0;
        let done = self.set_saloon_lights(on);
        log::info!("saloon lights {} ({})", if on { "on" } else { "off" }, done.join(", "));
        if self.vehicle.ty.model.interior_lights.is_empty() {
            "This bus has no saloon lights".into()
        } else if !done.is_empty() {
            (if on { "Saloon lights on" } else { "Saloon lights off" }).to_string()
        } else if on {
            // the switches are thrown, but the lamps stay dark: no current
            "Saloon light switches on - no power (battery off?)".into()
        } else {
            "Saloon lights off".into()
        }
    }

    /// How many of the model's `[interiorlight]` lamps are lit now.
    fn saloon_lamps_lit(&self) -> usize {
        self.vehicle
            .ty
            .model
            .interior_lights
            .iter()
            .filter(|l| self.vehicle.var(&l.variable).unwrap_or(0.0) > 0.5)
            .count()
    }

    /// Switch the saloon lights on or off: every saloon light switch `Inputs/keyboard.cfg`
    /// knows (`cp_…licht…`/`…light…`, not the driver's lamp) is pressed once and taken
    /// back (the variables restored) when the scripts moved the lamps the other way. Returns the
    /// switches that did what was asked.
    pub(crate) fn set_saloon_lights(&mut self, on: bool) -> Vec<String> {
        let mut switches: Vec<String> = self
            .bound_actions()
            .into_iter()
            .filter(|a| {
                let a = a.to_ascii_lowercase();
                (a.contains("licht") || a.contains("light"))
                    && a.starts_with("cp_")
                    && !a.contains("fahrer")
                    && !a.contains("driver")
            })
            .collect();
        for extra in ["interior_light_toggle", "interiorlight_toggle"] {
            if self.vehicle.ty.program.triggers.contains_key(extra) {
                switches.push(extra.to_string());
            }
        }
        let total = self.vehicle.ty.model.interior_lights.len();
        let mut done = Vec::new();
        for sw in switches {
            let before = self.saloon_lamps_lit();
            if (on && before == total) || (!on && before == 0) {
                break;
            }
            let saved = self.vehicle.state.vars.clone();
            if !self.action(&sw, true) {
                continue;
            }
            self.action(&sw, false);
            // the scripts set the lamp variables from the switches in their frame
            self.vehicle.update_scripts_only(0.0);
            let after = self.saloon_lamps_lit();
            if (on && after < before) || (!on && after > before) {
                self.vehicle.state.vars = saved;
            } else if after != before {
                done.push(sw);
            }
        }
        done
    }

    /// How many of the model's outside lamps (`[light_enh]`, `[light_enh_2]`) are lit now.
    fn outside_lamps_lit(&self) -> usize {
        let v = &self.vehicle;
        v.ty.model
            .meshes
            .iter()
            .flat_map(|m| {
                m.light_enh
                    .iter()
                    .map(|l| l.variable.as_str())
                    .chain(m.light_enh_2.iter().map(|l| l.variable.as_str()))
            })
            .filter(|n| !n.is_empty() && v.var(n).unwrap_or(0.0) > 0.5)
            .count()
    }

    /// L (`kw_scheinwerfer_toggle`) on a bus whose side and tail lights have a switch of
    /// their own (Ctrl+L, `kw_standlicht_toggle`): switching the headlights on puts those on
    /// too, and switching them off puts them out again when L put them on. The LiAZ 5292's
    /// L lit the two dipped beams and nothing else - no marker lamps on the roof, no tail
    /// lights - and nobody finds Ctrl+L. A bus whose L already lights everything (the stock
    /// MANs' rotary switch) is left as it is: pressing its side-light key would only take
    /// lamps away, and that press is undone.
    fn headlights_with_side_lights(&mut self, before: usize) {
        if !self.vehicle.ty.program.triggers.contains_key("kw_standlicht_toggle") {
            return;
        }
        self.vehicle.update_scripts_only(0.0);
        let after = self.outside_lamps_lit();
        // a press that did not do what was wanted is taken back by restoring the variables
        // as they were: pressing again is no way back on a rotary switch (the EN92's key
        // goes headlights -> side lights -> off)
        let try_press = |p: &mut Player, better: &dyn Fn(usize) -> bool| -> bool {
            let saved = p.vehicle.state.vars.clone();
            p.vehicle.trigger("kw_standlicht_toggle");
            p.vehicle.trigger("kw_standlicht_toggle_off");
            p.vehicle.update_scripts_only(0.0);
            if better(p.outside_lamps_lit()) {
                true
            } else {
                p.vehicle.state.vars = saved;
                false
            }
        };
        if after > before && !self.side_lights_by_l {
            self.side_lights_by_l = try_press(self, &|n| n > after);
        } else if after < before && after > 0 {
            // (switching the headlights off leaves the side lights on only when they were on
            // before the headlights; the lights stayed on for good when that was not known)
            if try_press(self, &|n| n < after) {
                self.side_lights_by_l = false;
            }
        }
    }

    /// A key OMSI binds to a `<switch>_toggle` on a bus whose switch only turns in steps
    /// (`<switch>_up` / `<switch>_down`): the Procity's light switch has
    /// `kw_scheinwerfer_up`/`_down` and nothing answered L (`kw_scheinwerfer_toggle`), so its
    /// headlights could not be switched on from the keyboard. Such a key turns the switch up
    /// two steps (side lights, then headlights) and, the next time, back down.
    pub(crate) fn toggle_as_steps(&mut self, name: &str, pressed: bool) -> bool {
        let Some(base) = name.strip_suffix("_toggle") else {
            return false;
        };
        let (up, down) = (format!("{base}_up"), format!("{base}_down"));
        let has = |v: &omsi_sim::VehicleInstance, t: &str| v.ty.program.triggers.contains_key(&t.to_ascii_lowercase());
        if !has(&self.vehicle, &up) || !has(&self.vehicle, &down) {
            return false;
        }
        if !pressed {
            return true;
        }
        let key = base.to_ascii_lowercase();
        // up when nothing is lit, down when something is (a switch moved by the mouse, or lit
        // from the start, went on up for ever: the lights could not be switched off again)
        let lit = self.outside_lamps_lit() > 0;
        let going_up = if base.to_ascii_lowercase().contains("schein") || base.to_ascii_lowercase().contains("licht") || base.to_ascii_lowercase().contains("light") { !lit } else { !self.toggled_up.contains(&key) };
        let step = if going_up { &up } else { &down };
        for _ in 0..2 {
            self.vehicle.trigger(step);
            self.vehicle.trigger(&format!("{step}_off"));
        }
        if going_up {
            self.toggled_up.insert(key);
        } else {
            self.toggled_up.remove(&key);
        }
        true
    }

    /// Handle a key: engine actions change the axes, everything else fires script triggers.
    pub(crate) fn key(&mut self, scan: i32, modifiers: i32, pressed: bool) {
        let names: Vec<String> = if pressed {
            let n: Vec<String> = self
                .bindings
                .iter()
                .filter(|b| b.scan_code == scan && b.matches(modifiers))
                .map(|b| b.action.clone())
                .collect();
            self.held_keys.insert(scan, n.clone());
            let repeat = self
                .bindings
                .iter()
                .filter(|b| b.scan_code == scan && b.matches(modifiers) && b.modifier & omsi_content::input::KEY_HOLD != 0)
                .filter(|b| omsi_sim::engine_action(&b.action).is_none())
                .map(|b| b.action.clone())
                .collect();
            self.held_repeat.press(scan, repeat);
            n
        } else {
            self.held_repeat.release(scan);
            match self.held_keys.remove(&scan) {
                Some(n) => n,
                None => self
                    .bindings
                    .iter()
                    .filter(|b| b.scan_code == scan && b.matches(modifiers))
                    .map(|b| b.action.clone())
                    .collect(),
            }
        };
        self.from_keyboard = true;
        for name in names {
            if let Some(a) = omsi_sim::engine_action(&name) {
                self.axes.set(a, pressed);
            } else {
                self.action(&name, pressed);
            }
        }
        self.from_keyboard = false;
    }

    /// Start the whole bus by itself (Shift+U): everything a driver does to put it into
    /// service. Returns what to show in the HUD.
    pub(crate) fn start_up(&mut self) -> String {
        if let Some(s) = self.startup.as_ref() {
            // (one going on for long - a bus whose switches never get it there - is given up
            // and begun again, rather than saying the same for ever)
            if self.startup_at.is_none_or(|t| t.elapsed().as_secs_f32() < 20.0) {
                return if s.shutting_down() { "Switching the vehicle off ..." } else { "Putting the vehicle into service ..." }.to_string();
            }
            log::info!("auto-start given up after 20 s: begun again");
        }
        let bound = self.bound_actions();
        let s = omsi_sim::startup::StartUp::new(&self.vehicle, &bound);
        let shutting_down = s.shutting_down();
        self.startup = Some(s);
        self.startup_at = Some(std::time::Instant::now());
        if shutting_down {
            "Switching the vehicle off ...".to_string()
        } else {
            "Putting the vehicle into service ...".to_string()
        }
    }

    /// An auto-start is under way putting the bus into service (not switching it off).
    pub(crate) fn putting_into_service(&self) -> bool {
        self.startup.as_ref().is_some_and(|s| !s.shutting_down())
    }

    /// The vehicle actions `Inputs/keyboard.cfg` puts on keys.
    pub(crate) fn bound_actions(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for b in &self.bindings {
            if !names.iter().any(|n| n.eq_ignore_ascii_case(&b.action)) {
                names.push(b.action.clone());
            }
        }
        names
    }

    /// Advance a running auto-start, then the IBIS typing; true while the auto-start is
    /// still going.
    pub(crate) fn tick_startup(&mut self, dt: f32) -> bool {
        self.apply_html_requests();
        self.tick_ibis(dt);
        schedule::turn_roller_blind(&mut self.vehicle, &mut self.blind_pick, self.startup.is_some());
        let Some(mut s) = self.startup.take() else {
            return false;
        };
        let bound = self.bound_actions();
        if s.tick(&mut self.vehicle, &bound, dt) {
            self.startup = Some(s);
            return true;
        }
        log::info!(
            "auto-start finished: {} (engine_n={:?})",
            if s.report.is_empty() {
                "nothing to do".to_string()
            } else {
                s.report.join(", ")
            },
            self.vehicle.var("engine_n")
        );
        if omsi_sim::startup::power_on(&self.vehicle) {
            self.saloon_lights_after_dark();
        }
        if let Some((line, terminus, stops, stop)) = self.ibis_duty.take() {
            self.type_destination(&line, &terminus, &stops, (stop.0, &stop.1));
        }
        false
    }

    /// After dark a bus put into service (Shift+U) has its saloon lit, as a driver switches it
    /// on before the first stop: every key `Inputs/keyboard.cfg` gives a saloon light switch
    /// is pressed once, and pressed again when that put out more of the model's
    /// `[interiorlight]` lamps than it lit. Before, the saloon stayed dark until the
    /// switches were found (the LiAZ has two, on 7 and 8).
    fn saloon_lights_after_dark(&mut self) {
        let light_out = !omsi_sim::Daylight::compute(&self.vehicle.host.clock, None).lamps_on;
        if light_out {
            return;
        }
        // the headlights as well (the bus was put into service at night with its saloon
        // lit and its headlights off): the light switch key once, where the headlights are
        // off (`Spot_Select` < 0, the stock scripts' "no spotlight")
        let headlights_off = self.vehicle.var("Spot_Select").is_some_and(|s| s < 0.0);
        if headlights_off && self.vehicle.ty.program.trigger("kw_scheinwerfer_toggle").is_some() {
            self.action("kw_scheinwerfer_toggle", true);
            self.action("kw_scheinwerfer_toggle", false);
            log::info!("auto-start: headlights switched on after dark");
        }
        if self.vehicle.ty.model.interior_lights.is_empty() {
            return;
        }
        let done = self.set_saloon_lights(true);
        if !done.is_empty() {
            log::info!("auto-start: saloon lights switched on after dark ({})", done.join(", "));
        }
    }

    /// The driver's head follows the bus's accelerations a little late, as a body does:
    /// forward when braking, out of a bend, down over a bump (OMSI's head movement).
    /// The driver's head on its neck, as Omsi.exe moves the driver's view (0x7e2110): a mass
    /// on a spring, thrown by what the body does where the eye is - up and down always, with
    /// the sprung seat (`VehicleInstance::driver_seat_travel`), sideways and fore and aft
    /// with `[driverview_moving]` (3000/100 and 2000/100, and only once the bus is moving),
    /// never further than 10 cm up or down (0x7e2256). (A lag of our own towards a point a
    /// hundredth of the acceleration off - a third of what the original throws the head -
    /// stood in for it.)
    pub(crate) fn move_head(&mut self, dt: f32, enabled: bool) {
        let dt = dt.clamp(0.0, 0.1);
        let a = self.vehicle.physics.accel;
        let omega = self.vehicle.rigid.as_ref().map(|rb| rb.omega).unwrap_or(Vec3::ZERO);
        let dw = omega - self.head_omega;
        self.head_omega = omega;
        if dt <= 0.0 {
            return;
        }
        let def = &self.vehicle.ty.def;
        let eye = def.cameras_driver.get(self.driver_camera_slot()).map(|c| Vec3::new(c.pos[0], c.pos[1], c.pos[2])).unwrap_or(Vec3::ZERO);
        // (1 unless a frame is longer than 1/15 s)
        let stab = (1.0 / (15.0 * dt)).min(1.0);
        let spring = |p: f32, v: f32, kick: f32, k: f32, c: f32| -> (f32, f32) {
            let v = v + kick + (-k * p - c * v) * stab * dt;
            (p + v * dt, v)
        };
        // up and down: the sprung driver's seat (`Driver_Seat_VertTransl`), the one value
        // Omsi.exe moves the seat mesh and lifts the driver's camera by (0x7e2110 keeps it at
        // +0x510; 0x7e24b2 and 0x7e24e0 put it in the camera's translation). A spring of the
        // camera's own, kicked by the body's pitching with the opposite sign to the seat's,
        // moved the view against the seat when the bus dipped under the brakes (#2550).
        // (A vehicle without the wheel physics keeps the camera's spring and the eye height
        // it has always had there: it gives no seat travel to ride on.)
        if self.vehicle.rigid.is_some() {
            self.head.z = self.vehicle.driver_seat_travel();
            self.head_vel.z = 0.0;
        } else {
            let kick = -(a.z - 9.81) * dt + dw.y * eye.x + dw.x * eye.y;
            let (mut p, mut v) = spring(self.head.z, self.head_vel.z, kick, 3000.0 / 150.0, 2000.0 / 150.0);
            if p.abs() > 0.1 {
                p = p.clamp(-0.1, 0.1);
                v = 0.0;
            }
            self.head.z = p;
            self.head_vel.z = v;
        }
        if enabled {
            let moving = self.vehicle.physics.speed.abs().min(1.0);
            let kx = (-a.x * dt + dw.y * eye.z - dw.z * eye.y) * moving;
            let ky = (-a.y * dt - dw.x * eye.z + dw.z * eye.x) * moving;
            let (px, vx) = spring(self.head.x, self.head_vel.x, kx, 3000.0 / 100.0, 2000.0 / 100.0);
            let (py, vy) = spring(self.head.y, self.head_vel.y, ky, 3000.0 / 100.0, 2000.0 / 100.0);
            self.head.x = px;
            self.head_vel.x = vx;
            self.head.y = py;
            self.head_vel.y = vy;
        } else {
            self.head.x = 0.0;
            self.head.y = 0.0;
            self.head_vel.x = 0.0;
            self.head_vel.y = 0.0;
        }
    }

    /// The head of someone who is doing nothing at all: a standing body breathes and shifts
    /// its weight, so the view is never quite still while the bus waits at a stop (Settings ->
    /// idle head sway, off by default). `strength` is how much of the sway is asked for and
    /// `pace` how fast it is to move, against the pace it is designed at.
    /// Nothing of it is written into `head`: the springs above stay the bus's business alone.
    pub(crate) fn move_head_idle(&mut self, dt: f32, strength: f32, pace: f32) {
        self.head_idle.step(dt, strength, pace);
    }

    /// Where the driver's eye is in the bus's frame: the head OMSI's head movement throws
    /// about, with the sway of a head at rest on top of it (`head` and `head_idle`, added
    /// nowhere else - `driver_world`, `camera_look` and the mirrors' `driver_eye` all ask
    /// here, so they cannot disagree).
    pub(crate) fn head_offset(&self) -> Vec3 {
        self.head + self.head_idle.offset
    }

    /// The automatic clutch of the settings for a gear lever whose scripts do not read
    /// OMSI's `AutoClutch` (the LiAZ MKPP): with a gear in and the bus slow, the clutch
    /// bites as the throttle goes down, as a driver lets it up - without it every start
    /// from a stop stalled the engine unless a clutch pedal was worked.
    pub(crate) fn auto_clutch_bite(&mut self, throttle: f32) {
        let program = &self.vehicle.ty.program;
        let has_manual_gate = program.manual_gearbox();
        // (only a gearbox that reads the clutch pedal: an automatic whose scripts answer to
        // the number keys as well had its clutch pressed at every stop, and some went to
        // neutral when it stood, #234)
        let reads_clutch = program.var("Clutch").is_some_and(|v| program.reads(v));
        if self.vehicle.host.auto_clutch < 0.5 || !has_manual_gate || !reads_clutch || program.reads_sys(omsi_script::SysVar::AutoClutch) {
            return;
        }
        let gear = self.vehicle.var("antrieb_getr_aktugang").or_else(|| self.vehicle.var("antrieb_getr_gang")).unwrap_or(0.0);
        let kmh = self.vehicle.physics.velocity_kmh().abs();
        if gear.abs() > 0.5 && kmh < 12.0 {
            // it bites as the throttle goes down and only as far as the engine keeps its
            // revs (a clutch let go at once under full throttle stalled it all the same)
            // (a script that keeps its revs under another name: by the throttle alone - taken
            // as 0 revs the clutch never bit and the bus stood with it down, #260)
            let n = self.vehicle.var("engine_n").unwrap_or(2000.0);
            let bite = ((throttle - 0.05) / 0.45).clamp(0.0, 1.0).min(((n - 850.0) / 700.0).clamp(0.0, 1.0));
            let bite = bite * bite * (3.0 - 2.0 * bite);
            let want = (1.0 - bite) * (1.0 - kmh / 12.0);
            self.axes.clutch = self.axes.clutch.max(want);
        }
    }

    /// The gear a gear lever's gates have engaged (`kw_s_1`.. buses and cars): the variable
    /// the gates store (`antrieb_getr_gang` in the stock cars, `antrieb_getr_aktugang` in the
    /// LiAZ). None without gates.
    /// A script whose variable does not follow its gates: the gate put in last (#1832).
    pub(crate) fn gate_gear(&self) -> Option<i32> {
        self.script_gate_gear().map(|read| self.gates.gear(read))
    }

    fn script_gate_gear(&self) -> Option<i32> {
        let program = &self.vehicle.ty.program;
        program.trigger("kw_s_1")?;
        let v = crate::input_script::gate_gear_var(program).and_then(|v| self.vehicle.var(&v));
        Some(v.unwrap_or(0.0).round() as i32)
    }

    /// Put a gear lever into gate `to` (-1 R, 0 N): as a driver does it, the clutch down, the
    /// gear in, the clutch let up over a second and a half as OMSI's clutch key lets it (let
    /// go at once, a bus pulling away stalled its engine). False when there is no such gate.
    pub(crate) fn shift_gate_to(&mut self, to: i32) -> bool {
        let name = match to {
            0 => "kw_s_N".to_string(),
            -1 => "kw_s_R".to_string(),
            n => format!("kw_s_{n}"),
        };
        if to < -1 || self.vehicle.ty.program.trigger(&name).is_none() {
            return false;
        }
        self.vehicle.set_var("Clutch", 1.0);
        self.axes.clutch = 1.0;
        self.vehicle.trigger(&name);
        self.vehicle.trigger(&format!("{name}_off"));
        self.gates.engaged(to, true);
        true
    }

    pub(crate) fn tick(&mut self, dt: f32, audio: Option<&omsi_audio::AudioEngine>, inside: bool, listener_follows_bus: bool) {
        self.tick_startup(dt);
        self.tick_auto_drag(dt);
        // (the script's trigger only, as Omsi.exe calls it: no alias, no log line a frame)
        for name in self.held_repeat.due() {
            if self.vehicle.ty.program.trigger(&name).is_some() {
                self.vehicle.trigger(&name);
            }
        }
        self.axes.speed_kmh = self.vehicle.physics.velocity_kmh();
        self.axes.lock_curvature = self.vehicle.ty.def.inv_min_turn_radius;
        self.axes.update(dt);
        let a = self.analog;
        // a throttle pedal takes off a brake the keys hold (`pedal_hold`), as the throttle
        // key does: a brake tapped on the keys or a wheel's button stayed on under the
        // pedal and the bus was driven against its brakes (#377)
        if a.throttle.is_some_and(|t| t > 0.05) {
            self.axes.brake = 0.0;
        }
        self.auto_clutch_bite(a.throttle.unwrap_or(0.0).max(self.axes.throttle));
        self.tick_auto_shift(dt, a.throttle.unwrap_or(0.0).max(self.axes.throttle), a.brake.unwrap_or(0.0).max(self.axes.brake));
        self.vehicle.set_controls(omsi_sim::Controls {
            throttle: a.throttle.unwrap_or(self.axes.throttle).max(self.axes.throttle),
            brake: a.brake.unwrap_or(self.axes.brake).max(self.axes.brake),
            clutch: a.clutch.unwrap_or(self.axes.clutch).max(self.axes.clutch),
            // (a wheel at rest does not hold against the keys)
            steering: match a.steering {
                Some(s) if s.abs() > 0.02 || self.axes.steering == 0.0 => s,
                _ => self.axes.steering,
            },
        });
        let lever = self.vehicle.var("lights_sw_blinker");
        self.vehicle.update(dt);
        if let Some(keep) = kept_indicator(self.blinker_cancel, lever, self.vehicle.var("lights_sw_blinker")) {
            self.vehicle.set_var("lights_sw_blinker", keep);
        }
        // OMSI_SUSP_TRACE_WINDOW=<csv>: each wheel's travel every frame of a window run
        // (the offscreen run has OMSI_SUSP_TRACE)
        if let Some(path) = omsi_cfg::flags::OMSI_SUSP_TRACE_WINDOW.os() {
            use std::io::Write;
            static TRACE: std::sync::Mutex<Option<(std::fs::File, f64)>> = std::sync::Mutex::new(None);
            let mut g = TRACE.lock().unwrap_or_else(|e| e.into_inner());
            if g.is_none() {
                if let Ok(mut f) = std::fs::File::create(&path) {
                    let _ = writeln!(f, "t,dt,z,kmh,wheel,compression,rate,load,ground_z");
                    *g = Some((f, 0.0));
                }
            }
            if let (Some((f, t)), Some(rb)) = (g.as_mut(), self.vehicle.rigid.as_ref()) {
                *t += dt as f64;
                for (k, w) in rb.wheels.iter().enumerate() {
                    let _ = writeln!(f, "{:.4},{:.4},{:.4},{:.1},{k},{:.4},{:.3},{:.0},{:.4}", t, dt, rb.position.z, self.vehicle.physics.velocity_kmh(), w.compression, w.compression_rate, w.load, w.ground_z);
                }
            }
        }
        self.sound_step(audio, inside, listener_follows_bus);
    }

    /// The bus's sounds after its step: the triggers it fired, its volume curves, the
    /// announcements (the window's frame and a recording run's step alike).
    pub(crate) fn sound_step(&mut self, audio: Option<&omsi_audio::AudioEngine>, inside: bool, listener_follows_bus: bool) {
        let fired: Vec<String> = std::mem::take(&mut self.vehicle.host.fired_triggers);
        let fired_vars: Vec<(String, Vec<f32>)> = std::mem::take(&mut self.vehicle.host.fired_trigger_vars);
        let fired_files: Vec<(String, String)> =
            std::mem::take(&mut self.vehicle.host.fired_file_triggers);
        for (t, f) in &fired_files {
            log::info!("announcement: {t} -> {f}");
        }
        if let (Some(a), Some(ss)) = (audio, self.sounds.as_mut()) {
            let xf = self.vehicle.world_transform();
            let v = &self.vehicle;
            // the camera decides which `[viewpoint]` entries are heard (the exterior engine
            // samples outside, the rain on the roof in the cab)
            // (a walker on the way in or out through a door: by degrees, see `on_foot`)
            match omsi_audio::soundset::walker_cab_amount() {
                Some(w) => ss.set_inside_amount(w),
                None => ss.set_inside(inside),
            }
            ss.set_muffled(inside);
            ss.set_listener_vehicle(listener_follows_bus);
            // how open the bus is to the outside (doors, driver's window) for every outside
            // sound heard in it - this bus's own and the traffic's: Omsi.exe reads the
            // player's bus's `Snd_OutsideVol` whatever the camera does (0 when no script
            // writes it)
            omsi_audio::soundset::set_outside_open(Some(v.var("Snd_OutsideVol").unwrap_or(0.0)));
            // (the last time a trigger fired this frame: its sounds start with that moment)
            let at_fire = |t: &str, n: &str| -> Option<f32> {
                let vals = &fired_vars.iter().rev().find(|(k, _)| k.eq_ignore_ascii_case(t))?.1;
                v.var_slot(n).and_then(|i| vals.get(i).copied())
            };
            ss.update_fired(a, &|n| v.var(n), &xf, &fired, &at_fire);
            ss.update_parts(
                a,
                &|n| v.var(n),
                &|i| v.trailers.get(i).map(|t| t.world_transform()),
                &fired,
                &at_fire,
            );
            for (t, f) in &fired_files {
                ss.play_file_trigger(a, t, f, &|n| v.var(n), &xf);
            }
            if let Some(every) = debug_sound_every() {
                static LAST: std::sync::atomic::AtomicU32 =
                    std::sync::atomic::AtomicU32::new(u32::MAX);
                let bucket = (v.host.clock.time / every as f64) as u32;
                if LAST.swap(bucket, std::sync::atomic::Ordering::Relaxed) != bucket {
                    log::info!(
                        "sound: the player's bus ({} entries, {})",
                        ss.len(),
                        if inside { "inside" } else { "outside" }
                    );
                    for line in ss.report(a, &|n| v.var(n)) {
                        log::info!("  {line}");
                    }
                    for (i, part) in &ss.parts {
                        log::info!("sound: coupled part {i} ({} entries)", part.len());
                        for line in part.report(a, &|n| v.var(n)) {
                            log::info!("  {line}");
                        }
                    }
                }
            }
        }
    }

    /// Attach the vehicle's sound configuration.
    pub(crate) fn load_sounds(&mut self, audio: &omsi_audio::AudioEngine) {
        let def = &self.vehicle.ty.def;
        if let Some(rel) = &def.sound {
            let path = omsi_cfg::resolve_path(def.dir(), rel);
            match omsi_vehicle::SoundCfg::load(&path) {
                Ok(cfg) => {
                    // the bus's own `[next_random]` sounds, chosen by its fleet number
                    let number = self.vehicle.number();
                    let all = cfg.sounds.len();
                    let cfg = cfg.chosen_for(&number);
                    let dir = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
                    let mut ss = omsi_audio::SoundSet::new(audio, &cfg, &dir);
                    log::info!(
                        "sound config {}: {} sounds ({} of {all} for fleet number '{number}')",
                        path.display(),
                        cfg.sounds.len(),
                        cfg.sounds.len()
                    );
                    // the coupled parts' own sound configurations (the engine of a pusher
                    // articulated bus is in its rear section's)
                    for (i, t) in self.vehicle.trailers.iter().enumerate() {
                        let Some(rel) = &t.ty.def.sound else { continue };
                        let path = omsi_cfg::resolve_path(t.ty.def.dir(), rel);
                        match omsi_vehicle::SoundCfg::load(&path) {
                            Ok(cfg) => {
                                let dir = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
                                log::info!(
                                    "sound config of coupled part {i} {}: {} sounds",
                                    path.display(),
                                    cfg.sounds.len()
                                );
                                ss.add_part(i, omsi_audio::SoundSet::new(audio, &cfg.chosen_for(&number), &dir));
                            }
                            Err(e) => log::warn!("{e}"),
                        }
                    }
                    // (the variables its triggered sounds' volume curves read are kept as
                    // they stand when the trigger fires, see `SoundSet::update_fired`)
                    self.vehicle.host.snapshot_triggers = ss.curve_triggers().into_iter().collect();
                    self.sounds = Some(ss);
                }
                Err(e) => log::warn!("{e}"),
            }
        }
    }

}

/// Mouse steering switched off: the wheel stays where the mouse left it and the keys go on
/// from there, as in OMSI, where the mouse and the keys turn the one wheel (#184). The
/// keys' own position was held at the middle while the mouse steered, and the wheel sprang
/// back to it.
pub(crate) fn keep_wheel(p: Option<&mut Player>) {
    if let Some(p) = p {
        p.axes.steering = p.vehicle.physics.controls.steering;
        p.axes.centering = false;
    }
}

/// OMSI's mouse steering (Omsi.exe 0x6f4284..0x6f447b): the cursor's place across the whole
/// window is the steering from full left to full right lock, divided by the speed in tens of
/// km/h once the bus is faster than 10 km/h (going backwards counts as standing). At 50 km/h
/// the same movement of the hand turns the wheels a fifth as far: the wheel "gets heavier".
/// The mouse's point may lie past the window's edges (app_impl/mouse_grab.rs): the caller
/// holds the result at the full lock.
pub(crate) fn mouse_steering(cursor_x: f32, width: f32, kmh: f32) -> f32 {
    let x = 2.0 * cursor_x / width.max(1.0) - 1.0;
    x / (kmh / 10.0).max(1.0)
}

/// How much of the way to the cursor the mouse's wheel and pedals leave behind this frame:
/// for `fade` seconds after the mouse steering was switched on they ease towards it (half
/// the way every `fade`), then within ~60 ms - the cursor comes in bursts, and taken as it
/// came the wheel moved in steps. Without `smooth` (#1092) they are where the cursor says at
/// once, as Omsi.exe sets the curvature and the pedals from it every frame.
pub(crate) fn mouse_follow(fade: f32, dt: f32, smooth: bool) -> f32 {
    if fade > 0.0 {
        (-std::f32::consts::LN_2 / fade * dt).exp()
    } else if smooth {
        (-dt / 0.06).exp()
    } else {
        0.0
    }
}

pub(crate) fn mouse_pedal_target(input: f32, strength: f32) -> f32 {
    (input.clamp(0.0, 1.0) * strength.clamp(0.25, 4.0)).min(1.0)
}

/// A mouse pedal following the cursor, `k` of the way left behind each frame. The last bit
/// is snapped: in f32 the easing stops one step short of the target (1 - 6e-8 at 60 fps), and
/// the stock gearbox scripts kick down only at a throttle of exactly 1 - the cursor at the top
/// edge never gave it.
pub(crate) fn mouse_pedal(current: f32, target: f32, k: f32) -> f32 {
    let v = target + (current - target) * k;
    if (v - target).abs() < 1e-4 { target } else { v }
}

#[cfg(test)]
mod kept_indicator_tests {
    use super::kept_indicator;

    #[test]
    fn a_kept_indicator_is_put_back_when_the_script_cancels_it() {
        // the script's own cancelling stands by default
        assert_eq!(kept_indicator(true, Some(1.0), Some(0.0)), None);
        // kept: left and right come back, the hazard lever and a change of side stay
        assert_eq!(kept_indicator(false, Some(1.0), Some(0.0)), Some(1.0));
        assert_eq!(kept_indicator(false, Some(2.0), Some(0.0)), Some(2.0));
        assert_eq!(kept_indicator(false, Some(1.0), Some(2.0)), None);
        assert_eq!(kept_indicator(false, Some(0.0), Some(0.0)), None);
        assert_eq!(kept_indicator(false, None, Some(0.0)), None);
    }
}

#[cfg(test)]
mod orbit_pivot_tests {
    use super::orbit_pivot;
    use glam::DVec3;

    #[test]
    fn pivot_yaws_with_the_bus_and_ignores_body_attitude() {
        // heading 0: the centre passes through unrotated.
        let p = orbit_pivot(DVec3::new(10.0, 20.0, 5.0), 0.0, [0.0, 0.0, 1.2]);
        assert!((p.x - 10.0).abs() < 1e-6 && (p.y - 20.0).abs() < 1e-6);
        assert!((p.z - 6.2).abs() < 1e-6, "{p:?}");
        // heading 90: the forward offset swings sideways, height untouched
        // (a body_rotation pivot would also tilt it with pitch/bank).
        let q = orbit_pivot(DVec3::ZERO, 90.0, [1.0, 2.0, 1.2]);
        assert!((q.x - 2.0).abs() < 1e-5 && (q.y + 1.0).abs() < 1e-5, "{q:?}");
        assert!((q.z - 1.2).abs() < 1e-6, "{q:?}");
    }
}

#[cfg(test)]
mod mouse_tests {
    use super::{mouse_follow, mouse_pedal, mouse_pedal_target, mouse_steering};

    /// The mouse's wheel eases after the cursor by default; with the smoothing off it is where
    /// the cursor says the same frame, as in OMSI (#1092) - only the first second after
    /// switching the mouse steering on still eases, whichever it is.
    #[test]
    fn the_wheel_follows_the_cursor_at_once_without_smoothing() {
        let dt = 1.0 / 60.0;
        let k = mouse_follow(0.0, dt, true);
        assert!(k > 0.7 && k < 0.8, "{k}");
        assert_eq!(mouse_follow(0.0, dt, false), 0.0);
        assert_eq!(mouse_pedal(0.3, 0.9, mouse_follow(0.0, dt, false)), 0.9);
        let steer = 0.9 + (0.3 - 0.9) * mouse_follow(0.0, dt, false);
        assert_eq!(steer, 0.9);
        for smooth in [true, false] {
            let k = mouse_follow(1.0, dt, smooth);
            assert!((k - 0.5f32.powf(dt)).abs() < 1e-6, "{smooth}: {k}");
        }
    }

    #[test]
    fn the_mouse_pedal_reaches_the_floor() {
        for fps in [30.0f32, 60.0, 144.0] {
            let k = (-(1.0 / fps) / 0.06f32).exp();
            let (mut t, mut b) = (0.0, 1.0);
            for _ in 0..(fps as usize) {
                t = mouse_pedal(t, 1.0, k);
                b = mouse_pedal(b, 0.0, k);
            }
            // exactly: the kickdown compares the throttle with 1
            assert_eq!(t, 1.0, "{fps} fps");
            assert_eq!(b, 0.0, "{fps} fps");
        }
        // on the way it still eases
        let k = (-(1.0 / 60.0f32) / 0.06).exp();
        let t = mouse_pedal(0.0, 1.0, k);
        assert!(t > 0.2 && t < 0.3, "{t}");
    }

    #[test]
    fn mouse_pedal_strength_moves_the_full_pedal_threshold() {
        assert_eq!(mouse_pedal_target(0.5, 1.0), 0.5);
        assert_eq!(mouse_pedal_target(0.5, 2.0), 1.0);
        assert_eq!(mouse_pedal_target(1.0, 0.5), 0.5);
        assert_eq!(mouse_pedal_target(1.0, 2.0), 1.0);
        assert_eq!(mouse_pedal_target(0.5, 0.5), 0.25);
    }

    #[test]
    fn the_mouse_steers_less_the_faster_the_bus() {
        let w = 1600.0;
        // standing and slow: the whole width is the whole lock
        assert_eq!(mouse_steering(0.0, w, 0.0), -1.0);
        assert_eq!(mouse_steering(1600.0, w, 5.0), 1.0);
        assert_eq!(mouse_steering(800.0, w, 0.0), 0.0);
        assert!((mouse_steering(1200.0, w, 10.0) - 0.5).abs() < 1e-6);
        // faster: divided by the speed in tens of km/h
        assert!((mouse_steering(1200.0, w, 50.0) - 0.1).abs() < 1e-6);
        assert!((mouse_steering(1600.0, w, 100.0) - 0.1).abs() < 1e-6);
        // backwards like standing
        assert!((mouse_steering(1200.0, w, -20.0) - 0.5).abs() < 1e-6);
        // it moves smoothly: no step anywhere across the window
        let mut last = mouse_steering(0.0, w, 30.0);
        for px in 1..=1600 {
            let s = mouse_steering(px as f32, w, 30.0);
            assert!((s - last).abs() < 0.001, "step at {px}");
            last = s;
        }
    }
}

/// The line of a depot route: the file's line column, or - when that is empty or a placeholder
/// such as "XXX" - the line of its code (`code` = line x 100 + route).
fn route_line(t: &omsi_vehicle::hof::InfoTrip) -> String {
    let raw = t.line.trim();
    let code = omsi_cfg::parse_i32(&t.code);
    if (raw.is_empty() || raw.chars().all(|c| c == 'x' || c == 'X')) && code >= 100 {
        (code / 100).to_string()
    } else {
        raw.to_string()
    }
}
#[cfg(test)]
mod preset_tests {
    use super::fallback_action;
    use omsi_sim::EngineAction as A;
    use winit::keyboard::KeyCode;

    #[test]
    fn plain_left_right_steer_only_with_the_arrows_preset() {
        assert_eq!(fallback_action(KeyCode::ArrowLeft, "simple"), None);
        assert_eq!(fallback_action(KeyCode::ArrowRight, "simple"), None);
        assert_eq!(fallback_action(KeyCode::ArrowUp, "simple"), Some(A::Throttle));
        assert_eq!(fallback_action(KeyCode::KeyA, "simple"), Some(A::SteeringLeft));
        assert_eq!(fallback_action(KeyCode::ArrowLeft, "arrows"), Some(A::SteeringLeft));
        assert_eq!(fallback_action(KeyCode::ArrowRight, "arrows"), Some(A::SteeringRight));
    }
}

#[cfg(test)]
mod indicator_tests {
    use super::indicator_toggle_action;

    #[test]
    fn repeated_presses_switch_each_side_on_then_off() {
        for (side, on) in [(1, "blinker_left_set"), (2, "blinker_right_set")] {
            let mut state = 0;
            assert_eq!(indicator_toggle_action(&mut state, None, side), on);
            assert_eq!(indicator_toggle_action(&mut state, None, side), "blinker_off");
            assert_eq!(state, 0);
        }
    }

    #[test]
    fn changing_side_and_automatic_cancellation_use_the_current_lever() {
        let mut state = 1;
        assert_eq!(indicator_toggle_action(&mut state, Some(1), 2), "blinker_right_set");
        assert_eq!(indicator_toggle_action(&mut state, Some(0), 2), "blinker_right_set");
        assert_eq!(indicator_toggle_action(&mut state, Some(2), 2), "blinker_off");
    }

    #[test]
    fn hazards_keep_their_dedicated_toggle_trigger() {
        let mut state = 0;
        assert_eq!(indicator_toggle_action(&mut state, Some(0), 3), "blinker_warn_toggle");
        assert_eq!(state, 3);
        assert_eq!(indicator_toggle_action(&mut state, Some(3), 3), "blinker_warn_toggle");
        assert_eq!(state, 0);
    }
}

#[cfg(test)]
mod held_repeat_tests {
    use super::HeldRepeat;

    /// Page Up held three frames turns the roller blind on each of them: once by the press,
    /// twice from the list; let go, no more (#1098, #1145).
    #[test]
    fn a_held_key_is_told_every_frame_after_its_press_until_let_go() {
        let mut r = HeldRepeat::default();
        assert!(r.due().is_empty());
        r.press(201, vec!["bus_rollband_up".into()]);
        assert!(r.due().is_empty(), "the press itself was this frame's call");
        assert_eq!(r.due(), vec!["bus_rollband_up".to_string()]);
        assert_eq!(r.due(), vec!["bus_rollband_up".to_string()]);
        r.release(201);
        assert!(r.due().is_empty());
        // a key without a held action is none of it
        r.press(63, Vec::new());
        r.due();
        assert!(r.due().is_empty());
    }
}

#[cfg(test)]
mod steering_view_tests {
    use super::steering_view_yaw;

    #[test]
    fn follows_both_directions_without_snapping_or_overshooting() {
        let right = steering_view_yaw(0.0, 1.0, 0.016, true, 30.0, 0.25);
        assert!(right > 0.0 && right < 30.0);
        assert_eq!(steering_view_yaw(0.0, -1.0, 0.016, true, 30.0, 0.25), -right);
        let changed = steering_view_yaw(30.0, -1.0, 0.016, true, 30.0, 0.25);
        assert!(changed < 30.0 && changed > -30.0);
    }

    #[test]
    fn smoothing_is_frame_rate_independent_even_at_five_fps() {
        let mut reference: Option<f32> = None;
        for fps in [5, 30, 60, 144] {
            let mut yaw = 0.0;
            for _ in 0..fps { yaw = steering_view_yaw(yaw, 0.8, 1.0 / fps as f32, true, 40.0, 0.25); }
            if let Some(reference) = reference { assert!((yaw - reference).abs() < 0.0001); }
            reference = Some(yaw);
        }
    }

    #[test]
    fn configurable_angle_response_and_return_to_center() {
        let slow = steering_view_yaw(0.0, 1.0, 0.1, true, 45.0, 0.5);
        let fast = steering_view_yaw(0.0, 1.0, 0.1, true, 45.0, 0.1);
        assert!(fast > slow);
        assert_eq!(steering_view_yaw(0.0, 2.0, 0.1, true, 45.0, 0.1), fast);
        let centered = steering_view_yaw(30.0, 1.0, 0.1, false, 45.0, 0.25);
        assert!(centered > 0.0 && centered < 30.0);
        assert_eq!(steering_view_yaw(30.0, 0.0, 0.0, true, 45.0, 0.25), 30.0);
    }
}
