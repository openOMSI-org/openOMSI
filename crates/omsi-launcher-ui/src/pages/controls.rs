//! The Controls page: key bindings, game controllers and their setup wizard.

use super::*;
use omsi_launcher_lib as core;

// --- controls ---------------------------------------------------------------------------------

/// The game's own actions a controller's button can be given, besides the bus's: the doors
/// and gears of any bus, looking round while held, the bus radio while held, the cameras
/// and the views - both of OMSI's view resets, the one view's (C) and every view's (Space),
/// which a controller could not bring back to the first camera (#1167) - the main menu,
/// which Esc opens and a controller has no Esc for - and head tracking's switch and the
/// reset of the head's middle.
pub(super) const PAD_GAME_ACTIONS: [&str; 29] = ["head_tracking_recenter", "head_tracking_toggle", "doors_all", "door_4", "door_3", "door_2", "door_1", "gear_up", "gear_down", "view_look_left", "view_look_right", "view_look_up", "view_look_down", "view_reset_direction", "view_reset_all_directions", "view_interiorcam_plus", "view_interiorcam_minus", "view_toggle_viewpoint", "view_toggle_interior", "view_set_driver", "view_set_passenger", "view_set_outside", "sim_pause", "open_menu", "screenshot", "quicksave", "toggel_mouse_ctrl", "toggel_ctrler", "voice_radio"];

pub(super) fn action_text(names: &omsi_input::describe::ControlNames, a: &str) -> String {
    known_action(a).unwrap_or_else(|| names.control(a))
}

pub(super) fn control_names(l: &Launcher) -> &'static omsi_input::describe::ControlNames {
    omsi_input::describe::names(std::path::Path::new(&l.state.config.root), l.state.settings.get("language").and_then(|x| x.as_str()).unwrap_or("ENG"))
}

fn known_action(a: &str) -> Option<String> {
    if let Some(gear) = a.strip_prefix("kw_s_").and_then(|s| s.strip_suffix("_fest")) {
        return Some(omsi_ui::tr("Gear %{gear} (H-pattern)").replace("%{gear}", gear));
    }
    let known: &[(&str, &str)] = &[
        ("throttle", "Throttle"),
        ("brake", "Brake"),
        ("throttle_amplify", "Throttle (full, kickdown)"),
        ("clutch", "Clutch"),
        ("steering_left", "Steer left"),
        ("steering_right", "Steer right"),
        ("steering_neutral", "Steering to centre"),
        ("parking_brake_toggle", "Parking brake"),
        ("blinker_left_set", "Indicator left"),
        ("blinker_right_set", "Indicator right"),
        ("blinker_left_toggle", "Indicator left (toggle)"),
        ("blinker_right_toggle", "Indicator right (toggle)"),
        ("blinker_off", "Indicators off"),
        ("blinker_warn_toggle", "Hazard lights"),
        ("gear_up", "Gear up (manual gearbox)"),
        ("gear_down", "Gear down (manual gearbox)"),
        ("horn", "Horn"),
        ("kw_scheinwerfer_toggle", "Headlights"),
        ("kw_standlicht_toggle", "Sidelights"),
        ("kw_fernlicht_toggle", "High beam"),
        ("kw_m_enginestart", "Starter"),
        ("kw_wipermode_up", "Wipers (next mode)"),
        ("cp_batterietrennschalter_toggle", "Battery / ignition"),
        ("automatic_D", "Gear D"),
        ("automatic_N", "Gear N"),
        ("automatic_R", "Gear R"),
        ("bus_doorfront0", "Front door (leaf 1)"),
        ("bus_doorfront1", "Front door (leaf 2)"),
        ("bus_dooraft", "Release rear doors"),
        ("door_1", "Door 1 (front), any bus"),
        ("door_2", "Door 2, any bus"),
        ("door_3", "Door 3, any bus"),
        ("door_4", "Door 4, any bus"),
        ("doors_all", "All doors, any bus"),
        ("ticket_give", "Sell the requested ticket"),
        ("view_set_driver", "Driver's view"),
        ("view_set_passenger", "Passenger view"),
        ("view_set_outside", "Outside view"),
        ("view_toggle_viewpoint", "Next view"),
        ("view_toggle_interior", "Cabin and outside, one key"),
        ("head_tracking_recenter", "Head tracking: reset the head position"),
        ("head_tracking_toggle", "Head tracking on / off"),
        ("vr_recenter", "VR: Reset view"),
        ("vr_toggle_desktop_mirror", "VR: Monitor preview"),
        ("vr_toggle_mode", "VR: Switch VR / desktop"),
        ("vr_toggle_navigator", "VR: Toggle navigator"),
        ("vr_position_navigator", "VR: Position navigator"),
        ("exit", "Quit"),
        ("chat_open", "Multiplayer: write in the chat"),
        ("chat_toggle", "Multiplayer: show / hide the chat"),
        ("voice_radio", "Multiplayer: bus radio (hold)"),
        ("sim_pause", "Pause"),
        ("open_menu", "Open / close the main menu"),
        ("screenshot", "Screenshot"),
        ("photo_mode", "Photo mode"),
        ("quicksave", "Quicksave"),
        ("toggel_mouse_ctrl", "Toggle mouse steering"),
        ("toggel_ctrler", "Toggle game controllers"),
    ];
    known.iter().find(|k| k.0 == a).map(|k| k.1.to_string())
}

pub fn controls(l: &mut Launcher, area: Rect) {
    let mut tab = l.pages.controls_tab;
    if l.ui.chips("controls-tab", Rect::new(area.x, area.y, area.w, 34.0), &mut tab, &["Keyboard", "Game controllers"]) {
        l.pages.controls_tab = tab;
    }
    let body = Rect::new(area.x, area.y + 50.0, area.w, (area.h - 50.0).max(0.0));
    if l.pages.controls_tab == 1 {
        game_controllers(l, body);
        return;
    }
    l.pages.pads.cancel_feedback_test();
    // a key pressed while one binding waits for it
    if let (Some((sec, idx)), Some(code)) = (l.pages.capturing, l.ui.input.raw_key) {
        use winit::keyboard::KeyCode as K;
        if code == K::Escape {
            l.pages.capturing = None;
        } else if !matches!(code, K::ShiftLeft | K::ShiftRight | K::ControlLeft | K::ControlRight | K::AltLeft | K::AltRight | K::SuperLeft | K::SuperRight) {
            match omsi_input::keys::dik_code(code) {
                Some(scan) => {
                    let m = l.ui.input.binding_modifiers() as i64;
                    let section = ["vehicles", "game"][sec];
                    let vr_binding = l.state.keybindings.get(section).and_then(|a| a.as_array())
                        .and_then(|a| a.get(idx)).and_then(|b| b.get("action"))
                        .and_then(|a| a.as_str()).is_some_and(|a| a.starts_with("vr_"));
                    if let Some(b) = l.state.keybindings.get_mut(section).and_then(|a| a.as_array_mut()).and_then(|a| a.get_mut(idx)) {
                        // (the entry's "held" bit is the action's, not the key's: it stays)
                        let hold = b.get("modifier").and_then(|x| x.as_i64()).unwrap_or(0) & omsi_content::input::KEY_HOLD as i64;
                        b["scan_code"] = json!(scan);
                        b["modifier"] = json!(m | hold);
                    }
                    save_keys(l, vr_binding);
                }
                None => l.state.set_status(omsi_ui::tr("%{key} has no DirectInput scan code the game understands.").replace("%{key}", &format!("{code:?}")), true),
            }
            l.pages.capturing = None;
        }
        l.ui.input.raw_key = None;
        l.ui.input.raw_chord = 0;
    }
    // The keys below are the ones the game uses only with "Custom controls" (Settings →
    // Driving keys); the ready-made layouts keep W A S D / the arrows for driving. Say so,
    // with the switch right here - and changing a key switches by itself (see `save_keys`).
    let preset = l.state.settings.get("drive_keys").and_then(|v| v.as_str()).unwrap_or("simple").to_string();
    let body = if preset != "omsi" {
        let name = omsi_ui::tr(match preset.as_str() {
            "wasd" => "W A S D only",
            "arrows" => "Arrow keys only",
            _ => "W A S D + arrows",
        });
        let bw = if body.w < 700.0 { 150.0 } else { 200.0 };
        let text = omsi_ui::tr("Driving keys: %{name} (Settings). Those keys drive the bus and win over the list below. Change any key here and your own layout (Custom controls) is used from then on.").replace("%{name}", &name);
        // (a phone: the button under the words, not beside them in a column one word wide)
        let narrow = body.w < 520.0;
        let tw = if narrow { body.w - 56.0 } else { body.w - bw - 70.0 };
        let th = l.ui.paragraph_height(&text, tw, 12.5, Weight::Medium);
        let bar = Rect::new(body.x, body.y, body.w, if narrow { th + 22.0 + 46.0 } else { (th + 22.0).max(54.0) });
        l.ui.p().rounded(bar, 8.0, accent().alpha(0.1));
        l.ui.p().rounded_border(bar, 8.0, 1.0, accent().alpha(0.45));
        let ty = if narrow { bar.y + 11.0 } else { bar.center().y - th * 0.5 };
        l.ui.icon("info", Vec2::new(bar.x + 22.0, if narrow { ty + 9.0 } else { bar.center().y }), 20.0, accent());
        l.ui.paragraph(&text, Vec2::new(bar.x + 42.0, ty), tw, 12.5, Weight::Medium, TEXT_SOFT);
        let button = if narrow { Rect::new(bar.x + 42.0, bar.bottom() - 46.0, bar.w - 52.0, 36.0) } else { Rect::new(bar.right() - bw - 10.0, bar.center().y - 18.0, bw, 36.0) };
        if l.ui.button("kb-use-custom", button, "Use these keys", Some("keyboard"), ButtonKind::Primary) {
            use_custom_keys(l);
        }
        let used = bar.h + 12.0;
        Rect::new(body.x, body.y + used, body.w, body.h - used)
    } else {
        body
    };
    let names = control_names(l);
    let sections = [("Driving & the bus", "The bus's own keys", "vehicles"), ("The game", "Menus, views, pausing", "game")];
    // (a phone: one list at a time, the chips choosing which, each row in two lines - side by
    // side the lists were too narrow to show what a key does)
    let narrow = body.w < 640.0;
    let (body, half) = if narrow {
        let mut k = l.pages.kb_section;
        if l.ui.chips("kb-section", Rect::new(body.x, body.y, body.w, 34.0), &mut k, &[sections[0].0, sections[1].0]) {
            l.pages.kb_section = k;
        }
        (Rect::new(body.x, body.y + 48.0, body.w, (body.h - 48.0).max(0.0)), body.w)
    } else {
        (body, (body.w - GAP * 2.0) * 0.5)
    };
    for (sec, (title, sub, key)) in sections.iter().enumerate() {
        if narrow && sec != l.pages.kb_section {
            continue;
        }
        let r = if narrow { body } else { Rect::new(body.x + sec as f32 * (half + GAP * 2.0), body.y, half, body.h) };
        l.ui.card(r);
        let inner = l.ui.heading(Rect::new(r.x + 18.0, r.y + 14.0, r.w - 36.0, r.h - 28.0), title, Some(if sec == 0 { "directions_bus" } else { "sports_esports" }));
        l.ui.text_in(sub, Rect::new(inner.x, inner.y - 6.0, inner.w, 18.0), 12.0, Weight::Regular, TEXT_DIM, Align::Left);
        let mut filter = std::mem::take(&mut l.pages.kb_filter[sec]);
        // "Add binding": the picker of every action (keybind_picker.rs) - OMSI's events and
        // the installed buses' own triggers, which the game's section has no use for (there
        // a name typed in the filter is added, below)
        let add_w = if sec != 0 { 0.0 } else if inner.w < 500.0 { 104.0 } else { 124.0 };
        let filter_w = if sec != 0 { inner.w } else { (inner.w - add_w - GAP).max(120.0) };
        l.ui.text_input(&format!("kb-filter-{sec}"), Rect::new(inner.x, inner.y + 18.0, filter_w, 34.0), &mut filter, "Filter...", Some("search"));
        if sec == 0 && l.ui.button(&format!("kb-add-{sec}"), Rect::new(inner.right() - add_w, inner.y + 18.0, add_w, 34.0), "Add binding", Some("add"), ButtonKind::Normal) {
            l.pages.kb_picker = Some(sec);
            l.pages.kb_picker_filter.clear();
            l.pages.kb_picker_source_filter.clear();
            l.pages.kb_action_options = None;
            l.pages.kb_filtered_options = None;
            l.pages.capturing = None;
        }
        l.pages.kb_filter[sec] = filter.clone();
        let q = filter.to_lowercase();

        let list: Vec<(usize, String, i64, i64)> = l.state.keybindings.get(*key).and_then(|a| a.as_array()).map(|a| a.iter().enumerate().map(|(i, b)| (i, b.get("action").and_then(|x| x.as_str()).unwrap_or("").to_string(), b.get("scan_code").and_then(|x| x.as_i64()).unwrap_or(0), b.get("modifier").and_then(|x| x.as_i64()).unwrap_or(0))).collect()).unwrap_or_default();
        let mut shown: Vec<(usize, String, String, bool)> = list
            .iter()
            .filter(|(_, a, s, m)| q.is_empty() || matches(&action_text(names, a), &q) || a.to_lowercase().contains(&q) || omsi_input::keys::key_name(*s, *m).to_lowercase().contains(&q))
            .map(|(i, a, s, m)| {
                let clash = *s != 0 && list.iter().any(|(j, _, s2, m2)| j != i && s2 == s && m2 == m);
                (*i, action_text(names, a), omsi_input::keys::key_name(*s, *m), clash)
            })
            .collect();
        if sec == 1 {
            shown.sort_by_key(|(_, label, _, _)| !label.starts_with("VR:"));
        }
        let capturing = l.pages.capturing;
        // what the row's buttons asked: a key to be pressed, cleared, or the entry removed;
        // `more` another key for an entry's action (#854)
        #[derive(Clone, Copy)]
        enum RowAsked { Capture, Clear, Remove }
        let mut clicked: Option<(usize, RowAsked)> = None;
        let mut more: Option<usize> = None;
        let time = l.ui.time;
        // a name the list does not have (a bus's own trigger a mod's readme gives a key, the
        // Urbanway's `ASS_toggle`): added to the list as a key of its own, as an [entry]
        // added to OMSI's keyboard.cfg by hand is (#854)
        let new_action = filter.trim();
        let mut list_top = inner.y + 62.0;
        if shown.is_empty() && new_action.len() > 1 && new_action.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            if l.ui.button(&format!("kb-add-custom-{sec}"), Rect::new(inner.x, list_top, inner.w, 36.0), &omsi_ui::tr("Add \"%{action}\" and give it a key").replace("%{action}", new_action), Some("add"), ButtonKind::Normal) {
                if let Some(a) = l.state.keybindings.get_mut(*key).and_then(|a| a.as_array_mut()) {
                    a.push(json!({ "action": new_action, "scan_code": 0, "modifier": 0 }));
                    l.pages.capturing = Some((sec, a.len() - 1));
                }
                // (the filter stays: the new row is the one it shows, waiting for its key)
            }
            list_top += 44.0;
        }
        l.ui.scroll_area(&format!("kb-{sec}"), Rect::new(inner.x - 6.0, list_top, inner.w + 12.0, inner.bottom() - list_top), &mut |ui, v| {
            let rh = if narrow { 74.0 } else { 40.0 };
            for (row, (i, label, keyn, clash)) in shown.iter().enumerate() {
                let rr = Rect::new(v.x + 6.0, v.y + row as f32 * rh, v.w - 16.0, rh - 4.0);
                if rr.bottom() < v.y - 50.0 {
                    continue;
                }
                ui.p().rounded(rr, 8.0, Color::WHITE.alpha(0.03));
                // (the key's row: on a phone the line under the name)
                let keys = if narrow { Rect::new(rr.x, rr.y + 34.0, rr.w, 36.0) } else { Rect::new(rr.x, rr.y, rr.w, rr.h) };
                let label_r = if narrow { Rect::new(rr.x + 12.0, rr.y + 2.0, rr.w - 24.0, 32.0) } else { Rect::new(rr.x + 12.0, rr.y, rr.w - 270.0, rr.h) };
                ui.text_in(label, label_r, 13.0, Weight::Medium, TEXT_SOFT, Align::Left);
                let kw = if narrow { (keys.w - 134.0).max(60.0) } else { 150.0 };
                // another key for the same action (OMSI's file may give one action
                // several [entry]s; several actions on one key need nothing more than the
                // same key pressed for each)
                let pr = Rect::new(keys.right() - 102.0 - kw, keys.y + 5.0, 26.0, keys.h - 10.0);
                let (hp, _, cp) = ui.interact(id_of(&format!("kb-{sec}-{i}-more")), pr);
                ui.icon("add", pr.center(), 16.0, if hp { accent() } else { TEXT_FAINT });
                if ui.hover(pr) {
                    ui.tooltip(pr, "Add another key for this action");
                }
                if cp {
                    more = Some(*i);
                }
                let kr = Rect::new(keys.right() - 70.0 - kw, keys.y + 5.0, kw, keys.h - 10.0);
                let waiting = capturing == Some((sec, *i));
                let id = id_of(&format!("kb-{sec}-{i}"));
                let (h, _, c) = ui.interact(id, kr);
                if c {
                    clicked = Some((*i, RowAsked::Capture));
                }
                let base = if waiting { accent().alpha(0.25 + 0.15 * (time * 6.0).sin().abs()) } else if *clash { DANGER.alpha(0.22) } else { Color::WHITE.alpha(if h { 0.12 } else { 0.07 }) };
                ui.p().rounded(kr, 6.0, base);
                ui.p().rounded_border(kr, 6.0, 1.0, if waiting { accent() } else if *clash { DANGER } else { Color::WHITE.alpha(0.1) });
                ui.text_in(if waiting { "press a key..." } else { keyn }, kr, 12.0, Weight::Bold, if *clash { DANGER.lighten(0.3) } else { TEXT }, Align::Center);
                let dr = Rect::new(keys.right() - 64.0, keys.y + 5.0, 26.0, keys.h - 10.0);
                let (hd, _, cd) = ui.interact(id_of(&format!("kb-{sec}-{i}-clear")), dr);
                ui.icon("delete", dr.center(), 16.0, if hd { TEXT } else { TEXT_FAINT });
                if ui.hover(dr) {
                    ui.tooltip(dr, "Clear the key (the entry stays)");
                }
                if cd {
                    clicked = Some((*i, RowAsked::Clear));
                }
                let xr = Rect::new(keys.right() - 32.0, keys.y + 5.0, 26.0, keys.h - 10.0);
                let (hx, _, cx) = ui.interact(id_of(&format!("kb-{sec}-{i}-remove")), xr);
                ui.icon("close", xr.center(), 16.0, if hx { DANGER } else { TEXT_FAINT });
                if ui.hover(xr) {
                    ui.tooltip(xr, "Remove the entry");
                }
                if cx {
                    clicked = Some((*i, RowAsked::Remove));
                }
            }
            shown.len() as f32 * rh
        });
        match clicked {
            Some((i, asked @ (RowAsked::Clear | RowAsked::Remove))) => {
                let vr_binding = l.state.keybindings.get(*key).and_then(|a| a.as_array())
                    .and_then(|a| a.get(i)).and_then(|b| b.get("action"))
                    .and_then(|a| a.as_str()).is_some_and(|a| a.starts_with("vr_"));
                let entries = l.state.keybindings.get_mut(*key).and_then(|a| a.as_array_mut());
                match (asked, entries) {
                    (RowAsked::Clear, Some(a)) => if let Some(b) = a.get_mut(i) {
                        b["scan_code"] = json!(0);
                        b["modifier"] = json!(0);
                    },
                    // (the entry out of the file; one waiting for its key below it moves up)
                    (_, Some(a)) if i < a.len() => {
                        a.remove(i);
                        l.pages.capturing = match l.pages.capturing {
                            Some((s, k)) if s == sec && k == i => None,
                            Some((s, k)) if s == sec && k > i => Some((s, k - 1)),
                            other => other,
                        };
                    }
                    _ => {}
                }
                save_keys(l, vr_binding);
            }
            Some((i, RowAsked::Capture)) => l.pages.capturing = Some((sec, i)),
            None => {}
        }
        if let Some(i) = more {
            if let Some(a) = l.state.keybindings.get_mut(*key).and_then(|a| a.as_array_mut()) {
                if let Some(b) = a.get(i).cloned() {
                    // (the held bit is the action's: it goes with it)
                    let hold = b.get("modifier").and_then(|x| x.as_i64()).unwrap_or(0) & omsi_content::input::KEY_HOLD as i64;
                    a.insert(i + 1, json!({ "action": b.get("action").cloned().unwrap_or(json!("")), "scan_code": 0, "modifier": hold }));
                    l.pages.capturing = Some((sec, i + 1));
                }
            }
        }
    }
    if !l.state.keybindings_error.is_empty() {
        let e = l.state.keybindings_error.clone();
        l.ui.text_in(&e, Rect::new(body.x, body.bottom() + 4.0, body.w, 18.0), 12.0, Weight::Medium, DANGER, Align::Left);
    }
}

/// Hide empty slots beyond the physical buttons without changing the saved controller file.
fn shown_button_count(buttons: &[(String, String)], physical: usize, revealed: Option<usize>) -> usize {
    physical
        .max(buttons.iter().rposition(|(action, _)| !action.trim().is_empty()).map(|i| i + 1).unwrap_or(0))
        .max(revealed.map(|i| i + 1).unwrap_or(0))
}

/// The game controllers tab (see `PadsView`).
fn game_controllers(l: &mut Launcher, body: Rect) {
    use omsi_input::controllers::{DeviceCfg, Func};
    let names = control_names(l);
    if l.pages.controller_action_choices.is_none() {
        l.pages.controller_action_choices = Some(std::sync::Arc::new(controller_action_choices(names, &l.state.keybindings)));
    }
    let choices = std::sync::Arc::clone(l.pages.controller_action_choices.as_ref().unwrap());
    let (actions, labels) = choices.as_ref();
    let hwnd = l.window.as_deref().and_then(omsi_input::controllers::window_handle);
    let pv = &mut l.pages.pads;
    if pv.io.is_none() {
        pv.io = Some(omsi_input::controllers::Devices::new(hwnd, false));
    }
    if pv.devices.is_none() {
        let root = std::path::PathBuf::from(&l.state.config.root);
        pv.devices = Some(omsi_input::controllers::read_cfg(&root));
    }
    // what the devices do now (and a button pressed while one is awaited)
    let mut pressed: Vec<(String, usize)> = Vec::new();
    let mut connected: Vec<omsi_input::controllers::Connected> = Vec::new();
    if let Some(io) = pv.io.as_mut() {
        for (name, n, down) in io.poll() {
            if down {
                pressed.push((name, n));
            }
        }
        connected = io.connected();
    }
    let devices = pv.devices.get_or_insert_with(Vec::new);
    let mut body = body;
    if !l.state.settings.get("momentary_gears").and_then(|v| v.as_bool()).unwrap_or(false) && omsi_input::hpattern::has_held_bindings(devices) {
        let height = l.ui.paragraph("H-pattern gears are assigned. Enable return to neutral under Settings → Driving → Game controllers if your shifter has no neutral button.", Vec2::new(body.x + 12.0, body.y + 8.0), body.w - 24.0, 12.5, Weight::Regular, TEXT_DIM);
        body.y += height + 20.0;
        body.h -= height + 20.0;
    }
    // devices connected but not set up yet can be added
    let list_w = (body.w * 0.32).min(360.0);
    let left = Rect::new(body.x, body.y, list_w, body.h);
    let right = Rect::new(body.x + list_w + GAP * 2.0, body.y, body.w - list_w - GAP * 2.0, body.h);
    l.ui.card(left);
    let inner = l.ui.heading(Rect::new(left.x + 18.0, left.y + 14.0, left.w - 36.0, left.h - 28.0), "Devices", Some("sports_esports"));
    let list_r = Rect::new(inner.x - 6.0, inner.y, inner.w + 12.0, inner.h - 108.0);
    let offs: Vec<String> = l.state.settings.get("ctrl_off").and_then(|v| v.as_str()).unwrap_or("").split('|').map(str::to_string).filter(|s| !s.is_empty()).collect();
    let (sel, add) = device_list(&mut l.ui, list_r, devices, &connected, &offs, pv.selected);
    if sel != pv.selected {
        release_feedback(&mut pv.io, &mut pv.feedback_test);
        pv.selected = sel;
        pv.capturing = false;
        pv.revealed_button = None;
        pv.wizard = None;
        pv.confirm_remove = None;
    }
    if let Some(name) = add {
        // (a device of buttons only - a gear shifter, a button box - has no axes for the
        // assistant: its buttons are given their keys on its page)
        let axes = !pv.io.as_ref().is_some_and(|io| io.buttons_only(&name));
        devices.push(DeviceCfg { name, second: "0".into(), ..Default::default() });
        pv.selected = devices.len() - 1;
        pv.revealed_button = None;
        pv.dirty = true;
        // a new device starts with the assistant
        if axes {
            pv.wizard = Some(Wizard { step: 0, rest: [None; 8], at: Vec::new(), error: None, calibration: None, ff_choice: None, test_strength: omsi_input::ffb_calibration::PULSE_FORCE });
        }
    }
    // the dead zone (a setting of the game's)
    let dz_r = Rect::new(inner.x, inner.bottom() - 98.0, inner.w, 34.0);
    let mut dz = l.state.settings.get("ctrl_deadzone").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
    if l.ui.slider("pad-dz", dz_r, &mut dz, 0.0, 0.3, 0.01, "Dead zone", &|v| format!("{:.0} %", v * 100.0)) {
        l.state.settings["ctrl_deadzone"] = json!(dz);
        l.state.settings_dirty = 0.3;
    }
    let save_r = Rect::new(inner.x, inner.bottom() - 48.0, inner.w, 40.0);
    if l.ui.button("pad-save", save_r, if pv.dirty { "Save" } else { "Saved" }, Some("save"), if pv.dirty { ButtonKind::Primary } else { ButtonKind::Normal }) && pv.dirty {
        match save_gamectrler(devices) {
            Ok(p) => {
                pv.dirty = false;
                l.state.set_status(omsi_ui::tr("Game controllers saved to %{path}").replace("%{path}", &p.display().to_string()), false);
            }
            Err(e) => l.state.set_status(omsi_ui::tr("Not saved: %{error}").replace("%{error}", &e.to_string()), true),
        }
    }
    // the device shown
    l.ui.card(right);
    let Some(d) = devices.get_mut(pv.selected) else { return };
    let inner = l.ui.heading(Rect::new(right.x + 18.0, right.y + 14.0, right.w - 36.0, right.h - 28.0), &d.name.clone(), Some("tune"));
    let live_dev = omsi_input::controllers::find_connected(&connected, &d.name);
    let live: Vec<(usize, f32)> = live_dev.map(|c| c.axes.clone()).unwrap_or_default();
    // every button the device has gets its line (DirectInput says how many)
    if let Some(n) = live_dev.map(|c| c.buttons).filter(|n| *n > d.buttons.len()) {
        d.buttons.resize(n, (String::new(), "0".into()));
    }
    // the assistant, over the device's page
    if let Some(w) = pv.wizard.as_mut() {
        let done = if w.step == WIZARD_STEPS.len() {
            feedback_setup(&mut l.ui, inner, w, d, &live, live_dev, &mut pv.io, &mut pv.feedback_test, hwnd,
                           l.state.settings.get("ff_invert").and_then(|v| v.as_bool()).unwrap_or(false))
        } else {
            wizard(&mut l.ui, inner, w, d, &live, live_dev.is_some(), live_dev.is_some_and(|c| c.ff_capable && !c.gamepad))
        };
        match done {
            Some(true) => {
                release_feedback(&mut pv.io, &mut pv.feedback_test);
                pv.wizard = None;
                pv.dirty = true;
                l.state.set_status("Set up: press Save to keep it (the buttons can be given their keys below).", false);
            }
            Some(false) => {
                release_feedback(&mut pv.io, &mut pv.feedback_test);
                pv.wizard = None;
            }
            None => {}
        }
        return;
    }
    // this device on or off (a second listing of the same wheel, a device not to be used)
    {
        let offs: Vec<String> = l.state.settings.get("ctrl_off").and_then(|v| v.as_str()).unwrap_or("").split('|').map(str::to_string).filter(|s| !s.is_empty()).collect();
        let mut on = !offs.iter().any(|o| o.eq_ignore_ascii_case(&d.name));
        if l.ui.toggle("pad-on", Rect::new(inner.right() - 400.0, inner.y - 36.0, 170.0, 30.0), &mut on, "Use this device") {
            let mut offs: Vec<String> = offs.into_iter().filter(|o| !o.eq_ignore_ascii_case(&d.name)).collect();
            if !on {
                offs.push(d.name.clone());
            }
            l.state.settings["ctrl_off"] = json!(offs.join("|"));
            l.state.settings_dirty = 0.3;
        }
    }
    let buttons_only = pv.io.as_ref().is_some_and(|io| io.buttons_only(&d.name));
    if !buttons_only && l.ui.button("pad-wizard", Rect::new(inner.right() - 220.0, inner.y - 36.0, 220.0, 30.0), "Set up step by step", Some("touch_app"), ButtonKind::Normal) {
        pv.wizard = Some(Wizard { step: 0, rest: [None; 8], at: Vec::new(), error: None, calibration: None, ff_choice: None, test_strength: omsi_input::ffb_calibration::PULSE_FORCE });
    }
    const AXES: [&str; 8] = ["X axis", "Y axis", "Z axis", "X rotation", "Y rotation", "Z rotation", "Slider 1", "Slider 2"];
    let funcs: Vec<String> = Func::LABELS.iter().map(|s| s.to_string()).collect();
    let mut dirty = false;
    let lit = pv.last_pressed.filter(|(_, t)| t.elapsed().as_secs_f32() < 4.0).map(|(b, _)| b);
    // Some OMSI configs contain hundreds of empty trailing slots (the G920 report had
    // 131 entries for 18 physical buttons). Keep them on disk, but do not fill the UI with them.
    let shown_buttons = shown_button_count(&d.buttons, live_dev.map(|c| c.buttons).unwrap_or(0), pv.revealed_button);
    // (the axes, then every button of the device: the list scrolls - it stopped at the ten
    // buttons that fitted)
    let list = Rect::new(inner.x - 6.0, inner.y, inner.w + 12.0, inner.h - 50.0);
    let mut buttons_start_y = 0.0;
    l.ui.scroll_area("pad-detail", list, &mut |ui, v| {
        let x0 = v.x + 6.0;
        let w = v.w - 16.0;
        let (mut y, ff_dirty) = force_feedback_rows(ui, d, live_dev, &l.state.settings, x0, w, v.y);
        if ff_dirty {
            dirty = true;
        }
        let lab_w = if w < 520.0 { 84.0 } else { 110.0 };
        let inv_w = 110.0;
        let shp_w = 140.0;
        let sel_w = (w - lab_w - inv_w - shp_w - 60.0 - 4.0 * GAP).clamp(120.0, 200.0);
        let bar_w = (w - lab_w - sel_w - inv_w - shp_w - 4.0 * GAP).max(30.0);
        let shapes: Vec<String> = omsi_input::controllers::AXIS_SHAPES.iter().map(|s| s.0.to_string()).collect();
        // (a device of buttons only has no axes to give a function)
        for a in (0..8).filter(|_| !buttons_only) {
            let r = Rect::new(x0, y, w, ROW);
            ui.label(Rect::new(r.x, r.y, lab_w, r.h), AXES[a]);
            let bar = Rect::new(r.x + lab_w + GAP, r.y + 12.0, bar_w, r.h - 24.0);
            ui.p().rounded(bar, 4.0, Color::WHITE.alpha(0.06));
            if let Some((_, v)) = live.iter().find(|(k, _)| *k == a) {
                let x = bar.x + (v.clamp(-1.0, 1.0) + 1.0) * 0.5 * bar.w;
                ui.p().rounded(Rect::new(x - 2.0, bar.y - 3.0, 4.0, bar.h + 6.0), 2.0, accent());
            }
            let mut sel = (Func::code(d.axes[a].map(|x| x.0)) + 1) as usize;
            if ui.select(&format!("pad-axis-{a}"), Rect::new(bar.right() + GAP, r.y, sel_w, r.h), &mut sel, &funcs) {
                let inv = d.axes[a].map(|x| x.1).unwrap_or(false);
                d.axes[a] = Func::from_code(sel as i32 - 1).map(|f| (f, inv));
                dirty = true;
            }
            let mut inv = d.axes[a].map(|x| x.1).unwrap_or(false);
            if d.axes[a].is_some() && ui.toggle(&format!("pad-inv-{a}"), Rect::new(bar.right() + GAP + sel_w + GAP, r.y, inv_w, r.h), &mut inv, "Reversed") {
                if let Some(x) = d.axes[a].as_mut() {
                    x.1 = inv;
                }
                dirty = true;
            }
            if d.axes[a].is_some() {
                // the characteristic: the curve bits of the flags, the range extension kept
                let curve = d.axis_flags[a] & (4 | 8 | 0x10);
                let mut shp = omsi_input::controllers::AXIS_SHAPES.iter().position(|s| s.1 == curve).unwrap_or(0);
                if ui.select(&format!("pad-shape-{a}"), Rect::new(bar.right() + GAP + sel_w + GAP + inv_w + GAP, r.y, shp_w, r.h), &mut shp, &shapes) {
                    d.axis_flags[a] = (d.axis_flags[a] & !(4 | 8 | 0x10)) | omsi_input::controllers::AXIS_SHAPES[shp].1;
                    dirty = true;
                }
            }
            y += ROW + 6.0;
        }
        // buttons: their key actions
        y += 10.0;
        ui.text_in("Buttons", Rect::new(x0, y, w, 20.0), 14.0, Weight::Bold, TEXT, Align::Left);
        y += 26.0;
        buttons_start_y = y - v.y;
        let cols = if w < 560.0 { 1usize } else { 2 };
        let cw = (w - GAP * (cols - 1) as f32) / cols as f32;
        let per_row = ROW + 4.0;
        let rows = shown_buttons.div_ceil(cols);
        let latching = &mut d.latching;
        for (b, (act, _)) in d.buttons.iter_mut().take(shown_buttons).enumerate() {
            let (col, row) = (b / rows.max(1), b % rows.max(1));
            let r = Rect::new(x0 + col as f32 * (cw + GAP), y + row as f32 * per_row, cw, ROW);
            let label = button_label(b);
            if lit == Some(b) {
                ui.p().rounded(Rect::new(r.x - 4.0, r.y - 2.0, r.w + 8.0, r.h + 4.0), 6.0, accent().alpha(0.28));
            }
            ui.label(Rect::new(r.x, r.y, 90.0, r.h), &label);
            // (a latching switch - a turn signal lever, a lit hazard button - also switches
            // when it comes out)
            let latch_w = 104.0;
            let mut sel = actions.iter().position(|a| a.eq_ignore_ascii_case(act)).unwrap_or(0);
            if ui.select(&format!("pad-btn-{b}"), Rect::new(r.x + 90.0, r.y, r.w - 90.0 - latch_w - GAP, r.h), &mut sel, &labels) {
                *act = if sel == 0 { String::new() } else { actions[sel].clone() };
                dirty = true;
            }
            let mut latched = latching.contains(&b);
            if ui.toggle(&format!("pad-latch-{b}"), Rect::new(r.right() - latch_w, r.y, latch_w, r.h), &mut latched, "Latching") {
                latching.retain(|x| *x != b);
                if latched {
                    latching.push(b);
                    latching.sort_unstable();
                }
                dirty = true;
            }
        }
        y += rows as f32 * per_row;
        y - v.y + 8.0
    });
    if dirty {
        pv.dirty = true;
    }
    // a button pressed on the device: its line (added up to it)
    if let Some((name, n)) = pressed.into_iter().find(|(name, _)| omsi_input::controllers::names_match(&d.name, name)) {
        if omsi_input::controllers::names_match(&d.name, &name) && n < omsi_input::controllers::HAT_BUTTONS + 16 {
            while d.buttons.len() <= n {
                d.buttons.push((String::new(), "0".into()));
                pv.dirty = true;
            }
            pv.capturing = false;
            pv.revealed_button = Some(n);
            let cols = if list.w - 16.0 < 560.0 { 1usize } else { 2 };
            let rows = shown_buttons.max(n + 1).div_ceil(cols).max(1);
            let row = n % rows;
            l.ui.scroll_to("pad-detail", buttons_start_y + row as f32 * (ROW + 4.0), ROW, list.h);
            pv.last_pressed = Some((n, std::time::Instant::now()));
            let now = d.buttons.get(n).map(|b| b.0.clone()).filter(|a| !a.is_empty());
            let label = button_label(n);
            let text = match now {
                Some(a) => omsi_ui::tr("%{device}: %{button} - %{action} (lit in the list: choose another there)").replace("%{action}", &omsi_ui::tr(&action_text(names, &a))),
                None => omsi_ui::tr("%{device}: %{button} - nothing yet (lit in the list: choose what it does)").into_owned(),
            };
            l.state.set_status(text.replace("%{device}", &name).replace("%{button}", &label), false);
        }
    }
    let add_r = Rect::new(inner.x, inner.bottom() - 40.0, 260.0, 36.0);
    if l.ui.button("pad-add-button", add_r, if pv.capturing { "Press a button on the device..." } else { "Add a button" }, Some("add"), ButtonKind::Normal) {
        pv.capturing = !pv.capturing;
    }
    // a device no longer used (a wheel sold, one that came along in OMSI's own file) leaves
    // the list on a second click; Save keeps it so, and a connected one can be set up again
    // from the list (#636)
    let armed = pv.confirm_remove.is_some_and(|t| t.elapsed().as_secs() < 4);
    let remove_r = Rect::new(inner.right() - 260.0, inner.bottom() - 40.0, 260.0, 36.0);
    if l.ui.button("pad-remove", remove_r, if armed { "Click again to remove" } else { "Remove this device" }, Some("delete"), ButtonKind::Danger) {
        if armed {
            release_feedback(&mut pv.io, &mut pv.feedback_test);
            let name = remove_device(devices, &mut pv.selected);
            pv.capturing = false;
            pv.revealed_button = None;
            pv.last_pressed = None;
            pv.confirm_remove = None;
            pv.dirty = true;
            l.state.set_status(format!("{name} removed: press Save to keep it so."), false);
        } else {
            pv.confirm_remove = Some(std::time::Instant::now());
        }
    }
}

/// The list of devices on the controllers tab: those set up (lit when connected, `off` when
/// switched off) and a button for each connected one not set up. The line chosen (`sel`
/// unless one was clicked) and the device to set up, if one was asked for.
fn device_list(ui: &mut Ui, list_r: Rect, devices: &[omsi_input::controllers::DeviceCfg], connected: &[omsi_input::controllers::Connected], offs: &[String], mut sel: usize) -> (usize, Option<String>) {
    let mut add: Option<String> = None;
    ui.scroll_area("pad-list", list_r, &mut |ui, v| {
        let mut y = v.y;
        for (i, d) in devices.iter().enumerate() {
            let on = connected.iter().any(|c| omsi_input::controllers::names_match(&d.name, &c.name));
            let switched_off = offs.iter().any(|o| o.eq_ignore_ascii_case(&d.name));
            let r = Rect::new(v.x + 6.0, y, v.w - 12.0, 44.0);
            if ui.row(&format!("pad-{i}"), r, sel == i) {
                sel = i;
            }
            ui.text_in(&d.name, Rect::new(r.x + 12.0, r.y, r.w - 40.0, r.h), 13.0, Weight::Medium, if on { TEXT } else { TEXT_DIM }, Align::Left);
            if switched_off {
                ui.text_in("off", Rect::new(r.right() - 40.0, r.y, 30.0, r.h), 11.5, Weight::Bold, TEXT_FAINT, Align::Right);
            } else {
                ui.icon(if on { "check_circle" } else { "remove" }, Vec2::new(r.right() - 18.0, r.center().y), 16.0, if on { OK } else { TEXT_FAINT });
            }
            y += 48.0;
        }
        for c in connected.iter().filter(|c| !devices.iter().any(|d| omsi_input::controllers::names_match(&d.name, &c.name))) {
            let r = Rect::new(v.x + 6.0, y, v.w - 12.0, 38.0);
            if ui.button(&format!("pad-add-{}", c.name), r, &omsi_ui::tr("Set up %{device}").replace("%{device}", &c.name), Some("add"), ButtonKind::Primary) {
                add = Some(c.name.clone());
            }
            y += 44.0;
        }
        if devices.is_empty() && connected.is_empty() {
            y += 4.0 + ui.paragraph("No game controller is connected, and none is set up. Connect a wheel, pedals or a joystick; a gamepad works without setting up (left stick steers, the triggers are the pedals).", Vec2::new(v.x + 6.0, y + 4.0), v.w - 12.0, 13.0, Weight::Regular, TEXT_DIM);
        }
        y - v.y
    });
    (sel, add)
}

/// The force feedback rows at the top of a device's page (a wheel that has it, or one set
/// up with it): steering force, vibration and, but for a gamepad, the inverted force. Where
/// the next row starts, and whether something changed.
#[allow(clippy::too_many_arguments)]
fn force_feedback_rows(ui: &mut Ui, d: &mut omsi_input::controllers::DeviceCfg, live_dev: Option<&omsi_input::controllers::Connected>, settings: &serde_json::Value, x0: f32, w: f32, mut y: f32) -> (f32, bool) {
    let mut dirty = false;
    if live_dev.is_some_and(|c| c.ff_capable) || d.ff_invert.is_some() {
        let (mut steering_force, mut vibration) = d.ff_scale.unwrap_or((1.0, 1.0));
        if ui.slider("pad-ff-steering", Rect::new(x0, y, w, ROW), &mut steering_force, 0.0, 2.0, 0.05, "Steering force", &|v| format!("{:.0}%", v * 100.0)) {
            d.ff_scale = Some((steering_force, vibration));
            dirty = true;
        }
        y += ROW + 6.0;
        if ui.slider("pad-ff-vibration", Rect::new(x0, y, w, ROW), &mut vibration, 0.0, 2.0, 0.05, "Vibration", &|v| format!("{:.0}%", v * 100.0)) {
            d.ff_scale = Some((steering_force, vibration));
            dirty = true;
        }
        y += ROW + 20.0;
        if !live_dev.is_some_and(|c| c.gamepad) {
            let mut invert = d.ff_invert.unwrap_or_else(|| settings.get("ff_invert").and_then(|v| v.as_bool()).unwrap_or(false));
            if ui.toggle("pad-ff-invert", Rect::new(x0, y, w, ROW), &mut invert, "Invert force feedback") {
                d.ff_invert = Some(invert);
                dirty = true;
            }
            y += ROW + 20.0;
        }
    }
    (y, dirty)
}

/// Take the device shown (`selected`) out of the list; the one below it (or the last) is
/// shown next. Its name.
fn remove_device(devices: &mut Vec<omsi_input::controllers::DeviceCfg>, selected: &mut usize) -> String {
    let name = devices.remove(*selected).name;
    *selected = (*selected).min(devices.len().saturating_sub(1));
    name
}

/// A device's button `b` as its line names it: a hat's direction, or the button's number.
fn button_label(b: usize) -> String {
    match b.checked_sub(crate::controllers::HAT_BUTTONS) {
        Some(h) => omsi_ui::tr(["Hat %{n} up", "Hat %{n} right", "Hat %{n} down", "Hat %{n} left"][h % 4]).replace("%{n}", &(h / 4 + 1).to_string()),
        None => omsi_ui::tr("Button %{n}").replace("%{n}", &(b + 1).to_string()),
    }
}

/// The steps of the set-up assistant (see `Wizard`): what the player is asked each time.
const WIZARD_STEPS: [(&str, &str); 5] = [
    ("Let go of everything", "Take your hands off the wheel and your feet off the pedals (the wheel in the middle), then press Next."),
    ("Steering", "Turn the wheel (or move the stick) all the way to the LEFT and hold it there, then press Next."),
    ("Throttle", "Press the throttle pedal all the way down and hold it, then press Next. No pedals: Skip."),
    ("Brake", "Press the brake pedal all the way down and hold it, then press Next. No brake pedal: Skip."),
    ("Clutch", "Press the clutch pedal all the way down and hold it, then press Next. No clutch: Skip."),
];

/// One frame of the assistant in `r`; Some(true) when it has set the device up, Some(false)
/// when the player gave up.
fn wizard(ui: &mut Ui, r: Rect, w: &mut Wizard, d: &mut omsi_input::controllers::DeviceCfg, live: &[(usize, f32)], connected: bool, feedback: bool) -> Option<bool> {
    let (title, text) = WIZARD_STEPS[w.step];
    ui.text_in(&omsi_ui::tr("Step %{n} of %{total}: %{title}").replace("%{n}", &(w.step + 1).to_string()).replace("%{total}", &WIZARD_STEPS.len().to_string()).replace("%{title}", &omsi_ui::tr(title)), Rect::new(r.x, r.y, r.w, 26.0), 17.0, Weight::Bold, TEXT, Align::Left);
    let mut y = r.y + 34.0;
    y += ui.paragraph(text, Vec2::new(r.x, y), r.w, 13.5, Weight::Regular, TEXT_SOFT) + 10.0;
    if !connected {
        y += ui.paragraph("The device is not connected: plug it in (the list on the left marks it green).", Vec2::new(r.x, y), r.w, 13.0, Weight::Medium, DANGER);
    }
    if let Some(e) = &w.error {
        y += ui.paragraph(e, Vec2::new(r.x, y), r.w, 13.0, Weight::Medium, DANGER) + 6.0;
    }
    // the axes as they stand, so that the player sees the device answer
    for (k, v) in live {
        let bar = Rect::new(r.x + 90.0, y + 8.0, (r.w - 100.0).max(40.0), 8.0);
        ui.text_in(["X", "Y", "Z", "Rx", "Ry", "Rz", "Slider 1", "Slider 2"][*k], Rect::new(r.x, y, 84.0, 24.0), 12.0, Weight::Medium, TEXT_DIM, Align::Left);
        ui.p().rounded(bar, 4.0, Color::WHITE.alpha(0.06));
        let x = bar.x + (v.clamp(-1.0, 1.0) + 1.0) * 0.5 * bar.w;
        ui.p().rounded(Rect::new(x - 2.0, bar.y - 4.0, 4.0, bar.h + 8.0), 2.0, accent());
        y += 26.0;
    }
    let now = |live: &[(usize, f32)]| {
        let mut a = [None; 8];
        for (k, v) in live {
            a[*k] = Some(*v);
        }
        a
    };
    let by = r.bottom() - 40.0;
    if ui.button("wiz-cancel", Rect::new(r.x, by, 120.0, 36.0), "Cancel", None, ButtonKind::Ghost) {
        return Some(false);
    }
    let skip = w.step >= 2 && ui.button("wiz-skip", Rect::new(r.right() - 260.0, by, 110.0, 36.0), "Skip", None, ButtonKind::Normal);
    let next = ui.button("wiz-next", Rect::new(r.right() - 140.0, by, 140.0, 36.0), if w.step + 1 == WIZARD_STEPS.len() && !feedback { "Finish" } else { "Next" }, Some("chevron_right"), ButtonKind::Primary);
    if !(next || skip) {
        return None;
    }
    let cur = now(live);
    w.error = None;
    if w.step == 0 {
        if live.is_empty() {
            w.error = Some("The device shows no axis yet: move the wheel and the pedals a little, let go, and press Next again.".into());
            return None;
        }
        w.rest = cur;
    } else if skip {
        w.at.push([None; 8]);
    } else {
        // the axis that moved most since everything was let go (one taken before is not
        // taken again - but the throttle's axis may turn out to be the brake's too)
        let used: Vec<usize> = w.at.iter().filter_map(|a| moved_most(&w.rest, a, &[]).map(|m| m.0)).collect();
        let exclude: Vec<usize> = if w.step == 3 { used.iter().copied().take(1).collect() } else { used.clone() };
        match moved_most(&w.rest, &cur, &exclude) {
            Some(_) => w.at.push(cur),
            None => {
                w.error = Some("Nothing moved far enough. Hold it all the way, then press Next (or Skip).".into());
                return None;
            }
        }
    }
    w.step += 1;
    if w.step < WIZARD_STEPS.len() {
        return None;
    }
    let axes = wizard_result(&w.rest, &w.at);
    if feedback && axes.iter().any(|a| matches!(a, Some((omsi_input::controllers::Func::Steering, _)))) {
        return None;
    }
    d.axes = axes;
    Some(true)
}

fn feedback_setup(
    ui: &mut Ui, r: Rect, w: &mut Wizard, d: &mut omsi_input::controllers::DeviceCfg,
    live: &[(usize, f32)], device: Option<&omsi_input::controllers::Connected>,
    io: &mut Option<omsi_input::controllers::Devices>, active: &mut bool, hwnd: Option<isize>, global_invert: bool,
) -> Option<bool> {
    let axes = wizard_result(&w.rest, &w.at);
    let axis = axes.iter().position(|a| matches!(a, Some((omsi_input::controllers::Func::Steering, _))));
    ui.text_in("Force feedback direction", Rect::new(r.x, r.y, r.w, 26.0), 17.0, Weight::Bold, TEXT, Align::Left);
    let warning = "INJURY RISK: TAKE YOUR HANDS OFF THE WHEEL. Keep hands and fingers clear before starting and throughout the test.";
    let warning_height = ui.paragraph_height(warning, r.w - 58.0, 14.5, Weight::Bold) + 20.0;
    let warning_rect = Rect::new(r.x, r.y + 34.0, r.w, warning_height);
    ui.p().rounded(warning_rect, 6.0, DANGER.alpha(0.15));
    ui.p().rounded_border(warning_rect, 6.0, 1.5, DANGER);
    ui.icon("warning", Vec2::new(warning_rect.x + 22.0, warning_rect.center().y), 26.0, DANGER);
    ui.paragraph(warning, Vec2::new(warning_rect.x + 44.0, warning_rect.y + 10.0), warning_rect.w - 58.0, 14.5, Weight::Bold, DANGER);
    let body_y = warning_rect.bottom() + 12.0;
    ui.scroll_area("wiz-ff-body", Rect::new(r.x, body_y, r.w, r.bottom() - 56.0 - body_y), &mut |ui, r| {
        let mut y = r.y;
        y += ui.paragraph("The test applies two short forces in opposite directions. Finish and press Save to keep the detected direction for this wheel.", Vec2::new(r.x, y), r.w, 13.5, Weight::Regular, TEXT_SOFT) + 16.0;
        if let Some((started, test)) = w.calibration.as_mut() {
            if *active {
                let position = axis.and_then(|a| live.iter().find(|(k, _)| *k == a).map(|(_, x)| *x));
                if let Some(force) = test.update(started.elapsed().as_secs_f32(), position) {
                    if !device.zip(axis).zip(io.as_mut()).is_some_and(|((device, axis), io)| io.calibration_pulse(&device.name, axis, force)) {
                        test.fail("Force feedback is unavailable. Choose the direction manually.");
                    }
                }
                if let Some(result) = test.result {
                    release_feedback(io, active);
                    if let Ok(invert) = result {
                        w.ff_choice = Some(invert);
                    }
                }
            }
            let (message, color) = match test.result {
                Some(Ok(false)) => ("Direction detected: normal", OK),
                Some(Ok(true)) => ("Direction detected: inverted", OK),
                Some(Err(message)) => (message, DANGER),
                None => ("Testing: keep your hands off the wheel...", TEXT_SOFT),
            };
            y += ui.paragraph(message, Vec2::new(r.x, y), r.w, 13.0, Weight::Medium, color) + 12.0;
        }
        if !*active {
            ui.slider("wiz-ff-strength", Rect::new(r.x, y, r.w, ROW), &mut w.test_strength,
                      omsi_input::ffb_calibration::PULSE_FORCE, omsi_input::ffb_calibration::MAX_PULSE_FORCE, 0.01,
                      "Test strength", &|v| format!("{:.0}%", v * 100.0));
            y += ROW + 8.0;
            y += ui.paragraph("If the wheel barely moves, increase Test strength and retry. Keep your hands clear.", Vec2::new(r.x, y), r.w, 13.0, Weight::Regular, TEXT_DIM) + 10.0;
            if ui.button("wiz-ff-test", Rect::new(r.x, y, 180.0, 36.0), "Start test", Some("play_arrow"), ButtonKind::Primary) {
                if device.is_none() || axis.is_none() {
                    w.error = Some("The wheel is unavailable. Reconnect it and try again.".into());
                } else {
                    *io = None;
                    *io = Some(omsi_input::controllers::Devices::new(hwnd, true));
                    *active = true;
                    w.error = None;
                    log::info!("FFB calibration: device {}, raw steering axis {:?}, test strength {:.0}%", device.unwrap().name, axis, w.test_strength * 100.0);
                    w.calibration = Some((std::time::Instant::now(), omsi_input::ffb_calibration::Calibration::new(w.test_strength)));
                }
            }
            y += 48.0;
            let mut invert = w.ff_choice.or(d.ff_invert).unwrap_or(global_invert);
            if ui.toggle("wiz-ff-manual", Rect::new(r.x, y, r.w, ROW), &mut invert, "Invert force feedback") {
                w.ff_choice = Some(invert);
            }
            y += ROW + 8.0;
            y += ui.paragraph("If detection is inconclusive, retry or choose the direction manually. You can change it later on this device's page.", Vec2::new(r.x, y), r.w, 13.0, Weight::Regular, TEXT_DIM) + 8.0;
        }
        if let Some(error) = &w.error {
            y += ui.paragraph(error, Vec2::new(r.x, y), r.w, 13.0, Weight::Medium, DANGER) + 8.0;
        }
        y - r.y
    });
    let by = r.bottom() - 40.0;
    if ui.button("wiz-cancel", Rect::new(r.x, by, 120.0, 36.0), "Cancel", None, ButtonKind::Ghost) {
        return Some(false);
    }
    if !*active && ui.button("wiz-ff-finish", Rect::new(r.right() - 140.0, by, 140.0, 36.0), "Finish", Some("check"), ButtonKind::Primary) {
        d.axes = axes;
        d.ff_invert = Some(w.ff_choice.or(d.ff_invert).unwrap_or(global_invert));
        return Some(true);
    }
    None
}

/// The axes the assistant found: `rest` where everything rested, `at` where the axes stood
/// with the wheel turned left, the throttle, the brake and the clutch pressed (all None: that
/// step skipped).
fn wizard_result(rest: &[Option<f32>; 8], at: &[[Option<f32>; 8]]) -> [Option<(omsi_input::controllers::Func, bool)>; 8] {
    use omsi_input::controllers::Func;
    let mut axes: [Option<(Func, bool)>; 8] = [None; 8];
    let steer = at.first().and_then(|a| moved_most(rest, a, &[]));
    if let Some((k, delta)) = steer {
        // turned left the value falls: else the axis runs the other way
        axes[k] = Some((Func::Steering, delta > 0.0));
    }
    let taken: Vec<usize> = steer.map(|s| vec![s.0]).unwrap_or_default();
    let pedal = |i: usize, ex: &[usize]| at.get(i).and_then(|a| moved_most(rest, a, ex));
    let throttle = pedal(1, &taken);
    let brake = pedal(2, &taken);
    match (throttle, brake) {
        // one axis for both (pedals on a single axis): the throttle towards the raw maximum
        // (as in Omsi.exe), else the axis is reversed
        (Some((kt, dt)), Some((kb, db))) if kt == kb && dt * db < 0.0 => axes[kt] = Some((Func::ThrottleBrake, dt < 0.0)),
        _ => {
            // a pedal pressed goes towards 1
            if let Some((k, dl)) = throttle {
                axes[k] = Some((Func::Throttle, dl < 0.0));
            }
            if let Some((k, dl)) = brake.filter(|b| Some(b.0) != throttle.map(|t| t.0)) {
                axes[k] = Some((Func::Brake, dl < 0.0));
            }
        }
    }
    let mut ex = taken.clone();
    ex.extend(throttle.map(|t| t.0));
    ex.extend(brake.map(|t| t.0));
    if let Some((k, dl)) = pedal(3, &ex) {
        axes[k] = Some((Func::Clutch, dl < 0.0));
    }
    axes
}

/// The axis that moved most from `rest` to `now` (at least a sixth of its travel), not one of
/// `exclude`: (slot, how far, signed).
fn moved_most(rest: &[Option<f32>; 8], now: &[Option<f32>; 8], exclude: &[usize]) -> Option<(usize, f32)> {
    (0..8)
        .filter(|k| !exclude.contains(k))
        .filter_map(|k| Some((k, now[k]? - rest[k].unwrap_or(0.0))))
        .filter(|(_, d)| d.abs() > 0.33)
        .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
}

/// Write the devices to the content folder's `Inputs/gamectrler.cfg` (OMSI 2's own is only
/// read; the game takes the content folder's first).
fn save_gamectrler(devices: &[omsi_input::controllers::DeviceCfg]) -> Result<std::path::PathBuf, String> {
    let candidate = core::content_dir().unwrap_or_else(core::data_dir).join("Inputs");
    let dir = if (candidate.exists() || std::fs::create_dir_all(&candidate).is_ok()) && omsi_cfg::is_writable(&candidate) {
        candidate
    } else {
        core::data_dir().join("Inputs")
    };
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let p = dir.join("gamectrler.cfg");
    std::fs::write(&p, omsi_input::controllers::cfg_text(devices)).map_err(|e| e.to_string())?;
    omsi_cfg::content_changed();
    Ok(p)
}

/// Settings → Driving keys: "Custom controls", the keys of the Controls page.
fn use_custom_keys(l: &mut Launcher) -> bool {
    if l.state.settings.get("drive_keys").and_then(|v| v.as_str()) == Some("omsi") {
        return false;
    }
    l.state.settings["drive_keys"] = json!("omsi");
    l.state.settings_dirty = 0.3;
    l.state.set_status("Driving keys: Custom controls - the game uses the keys of this page.", false);
    true
}

fn save_keys(l: &mut Launcher, vr_binding: bool) {
    match core::save_keybindings(&l.state.keybindings) {
        Ok(()) => {
            l.state.keybindings_error.clear();
            if let Ok(k) = core::get_keybindings() {
                l.state.keybindings = k;
            }
            l.pages.kb_action_options = None;
            l.pages.kb_filtered_options = None;
            l.pages.controller_action_choices = None;
            // a key changed is a key the player wants to use: with a ready-made layout it
            // would be ignored wherever that layout has a key of its own
            if !vr_binding && use_custom_keys(l) {
                l.state.set_status("Key bindings saved; Driving keys switched to Custom controls so the game uses them.", false);
            } else {
                l.state.set_status("Key bindings saved.", false);
            }
        }
        Err(e) => {
            l.state.keybindings_error = format!("{e:#}");
            l.state.set_status(format!("{e:#}"), true);
        }
    }
}

#[cfg(test)]
mod wizard_tests {
    fn feedback_wizard() -> super::Wizard {
        super::Wizard {
            step: super::WIZARD_STEPS.len(), rest: [Some(0.0); 8],
            at: vec![[Some(-1.0), None, None, None, None, None, None, None], [None; 8], [None; 8], [None; 8]],
            error: None, calibration: None, ff_choice: None, test_strength: omsi_input::ffb_calibration::PULSE_FORCE,
        }
    }

    fn click_feedback(name: &str, w: &mut super::Wizard, d: &mut omsi_input::controllers::DeviceCfg) -> Option<bool> {
        use super::*;
        let mut ui = Ui::new();
        let size = Vec2::new(900.0, 700.0);
        let area = Rect::new(20.0, 20.0, 700.0, 600.0);
        let mut io = None;
        let mut active = false;
        ui.begin(size, 1.0, 0.016);
        feedback_setup(&mut ui, area, w, d, &[], None, &mut io, &mut active, None, false);
        let rect = ui.drawn[&id_of(name)];
        ui.input.mouse = rect.center();
        ui.input.pressed = true;
        ui.input.down = true;
        ui.begin(size, 1.0, 0.016);
        feedback_setup(&mut ui, area, w, d, &[], None, &mut io, &mut active, None, false);
        ui.input.pressed = false;
        ui.input.down = false;
        ui.input.released = true;
        ui.begin(size, 1.0, 0.016);
        let done = feedback_setup(&mut ui, area, w, d, &[], None, &mut io, &mut active, None, false);
        assert!(!active);
        assert!(io.is_none());
        done
    }

    #[test]
    fn manual_direction_is_only_applied_on_finish_and_cancel_preserves_the_device() {
        let original = omsi_input::controllers::DeviceCfg { ff_invert: Some(true), ..Default::default() };
        let mut device = original.clone();
        let mut w = feedback_wizard();
        assert_eq!(click_feedback("wiz-ff-manual", &mut w, &mut device), None);
        assert_eq!(w.ff_choice, Some(false));
        assert_eq!(device, original);
        assert_eq!(click_feedback("wiz-cancel", &mut w, &mut device), Some(false));
        assert_eq!(device, original);
        assert_eq!(click_feedback("wiz-ff-finish", &mut w, &mut device), Some(true));
        assert_eq!(device.ff_invert, Some(false));
        assert_eq!(device.axes[0], Some((Func::Steering, false)));
    }

    #[test]
    fn disconnected_wheel_cannot_start_a_hardware_test() {
        let mut device = omsi_input::controllers::DeviceCfg::default();
        let mut w = feedback_wizard();
        assert_eq!(click_feedback("wiz-ff-test", &mut w, &mut device), None);
        assert!(w.error.is_some());
        assert!(w.calibration.is_none());
    }

    #[test]
    fn cancelling_feedback_releases_io_and_invalidates_the_test() {
        let mut pads = super::PadsView::default();
        pads.feedback_test = true;
        pads.wizard = Some(super::Wizard {
            step: super::WIZARD_STEPS.len(), rest: [None; 8], at: Vec::new(), error: None,
            calibration: Some((std::time::Instant::now(), omsi_input::ffb_calibration::Calibration::new(omsi_input::ffb_calibration::PULSE_FORCE))), ff_choice: None, test_strength: omsi_input::ffb_calibration::PULSE_FORCE,
        });
        pads.cancel_feedback_test();
        assert!(!pads.feedback_test);
        assert!(pads.io.is_none());
        let test = &pads.wizard.as_ref().unwrap().calibration.as_ref().unwrap().1;
        assert!(test.result.unwrap().is_err());
        assert_eq!(pads.wizard.as_ref().unwrap().ff_choice, None);
    }

    use omsi_input::controllers::Func;

    #[test]
    fn h_pattern_gears_have_a_clear_name() {
        assert_eq!(super::known_action("kw_s_1_fest").as_deref(), Some("Gear 1 (H-pattern)"));
        assert_eq!(super::known_action("kw_s_R_fest").as_deref(), Some("Gear R (H-pattern)"));
    }

    #[test]
    fn empty_saved_button_slots_do_not_fill_the_controller_list() {
        let mut buttons = vec![(String::new(), "0".to_string()); 131];
        buttons[10].0 = "horn".to_string();
        assert_eq!(super::shown_button_count(&buttons, 18, None), 18);
        assert_eq!(super::shown_button_count(&buttons, 18, Some(128)), 129);
    }

    #[test]
    fn a_wheel_with_three_pedals() {
        // X the wheel; Y throttle, Z brake, Rz clutch - pedals reading 1 up, -1 down (as the
        // G25's run, "reversed")
        let rest = [Some(0.0), Some(1.0), Some(1.0), None, None, Some(1.0), None, None];
        let mut left = rest;
        left[0] = Some(-1.0);
        let mut thr = rest;
        thr[1] = Some(-1.0);
        let mut brk = rest;
        brk[2] = Some(-1.0);
        let mut clu = rest;
        clu[5] = Some(-1.0);
        let a = super::wizard_result(&rest, &[left, thr, brk, clu]);
        assert_eq!(a[0], Some((Func::Steering, false)));
        assert_eq!(a[1], Some((Func::Throttle, true)));
        assert_eq!(a[2], Some((Func::Brake, true)));
        assert_eq!(a[5], Some((Func::Clutch, true)));
    }

    #[test]
    fn pedals_on_one_axis_and_a_wheel_the_other_way() {
        let rest = [Some(0.0), Some(0.0), None, None, None, None, None, None];
        let a = super::wizard_result(&rest, &[[Some(0.9), Some(0.0), None, None, None, None, None, None], [Some(0.0), Some(-1.0), None, None, None, None, None, None], [Some(0.0), Some(1.0), None, None, None, None, None, None], [None; 8]]);
        assert_eq!(a[0], Some((Func::Steering, true)));
        assert_eq!(a[1], Some((Func::ThrottleBrake, true)));
    }
}

#[cfg(test)]
mod pad_action_tests {
    /// Every game action a button can be given is one the game carries out from a
    /// controller (app_events: `view_look_*` / `voice_radio` while held, the gears by name,
    /// the rest through `is_game_action`, the doors through `Player::action`) - Space's
    /// reset of every view among them (#1167).
    #[test]
    fn a_button_can_reset_every_view() {
        assert!(super::PAD_GAME_ACTIONS.contains(&"view_reset_all_directions"));
        assert!(super::PAD_GAME_ACTIONS.contains(&"voice_radio"));
        assert_eq!(super::known_action("voice_radio").as_deref(), Some("Multiplayer: bus radio (hold)"));
        // the main menu, which a controller has no Esc for
        assert!(super::PAD_GAME_ACTIONS.contains(&"open_menu"));
        assert_eq!(super::known_action("open_menu").as_deref(), Some("Open / close the main menu"));
        for a in super::PAD_GAME_ACTIONS {
            let held = a.starts_with("view_look_") || a == "voice_radio";
            let handled = held
                || omsi_input::actions::is_game_action(a)
                || a.starts_with("gear_")
                || omsi_input::actions::door_action(a).is_some();
            assert!(handled, "{a}");
        }
        // held pad actions are game actions so they never reach the bus script
        assert!(omsi_input::actions::is_game_action("voice_radio"));
        assert!(omsi_input::actions::is_game_action("view_look_left"));
    }
}

#[cfg(test)]
mod pad_remove_tests {
    use omsi_input::controllers::{cfg_text, parse_cfg};

    /// A device taken out of the list is gone from the file Save writes, and the next one is
    /// shown - the one below it, or above it when it was the last (#636).
    #[test]
    fn a_removed_device_leaves_the_file() {
        let mut devices = parse_cfg("[ctrl]\r\nSideWinder Joystick\r\n0\r\n\r\n[ctrl]\r\nLogitech G25 Racing Wheel USB\r\n1\r\n\r\n[ctrl]\r\nMOZA R3 Base\r\n0\r\n");
        let mut sel = 1;
        assert_eq!(super::remove_device(&mut devices, &mut sel), "Logitech G25 Racing Wheel USB");
        assert_eq!(sel, 1);
        let names = |text: &str| parse_cfg(text).into_iter().map(|d| d.name).collect::<Vec<_>>();
        assert_eq!(names(&cfg_text(&devices)), ["SideWinder Joystick", "MOZA R3 Base"]);
        assert_eq!(super::remove_device(&mut devices, &mut sel), "MOZA R3 Base");
        assert_eq!(sel, 0);
        assert_eq!(super::remove_device(&mut devices, &mut sel), "SideWinder Joystick");
        assert_eq!(sel, 0);
        assert!(names(&cfg_text(&devices)).is_empty());
    }
}
