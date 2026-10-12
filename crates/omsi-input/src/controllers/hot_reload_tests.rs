use super::*;

fn wheel() -> DeviceCfg {
    let mut d = DeviceCfg { name: "Test wheel".into(), second: "0".into(), ff_scale: Some((0.65, 1.25)), ff_invert: Some(true), ..Default::default() };
    d.axes[0] = Some((Func::Steering, true));
    d.axes[1] = Some((Func::Throttle, true));
    d.axes[5] = Some((Func::Brake, false));
    d.axis_flags[0] = 2 | 8 | 0x10;
    d.buttons = vec![("horn".into(), "7".into()), ("kw_s_1_fest".into(), "0".into()), ("blinker_warn_toggle".into(), "0".into())];
    d.latching = vec![2];
    d
}

fn no_hardware() -> Devices {
    Devices {
        gilrs: None,
        #[cfg(all(target_os = "linux", target_pointer_width = "64"))]
        calibration_wheel: None,
        #[cfg(all(target_os = "linux", target_pointer_width = "64"))]
        ff_wheels: Default::default(),
        #[cfg(all(target_os = "linux", target_pointer_width = "64"))]
        button_devices: crate::evdev_buttons::ButtonDevices::new(),
        #[cfg(windows)]
        di: None,
        #[cfg(target_os = "macos")]
        gc: crate::mac_game_controller::GcPads::new(),
        #[cfg(target_os = "macos")]
        hid: None,
        #[cfg(target_os = "macos")]
        hid_axes: Vec::new(),
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        hats: Vec::new(),
        pad_buttons: Vec::new(),
    }
}

#[test]
fn hot_reload_keeps_device_owner_and_ffb_runtime_state() {
    let mut c = Controllers::with_devices(no_hardware(), vec![wheel()]);
    c.ff_t = 12.0;
    c.ff_lateral = 0.25;
    c.ff_bump = 0.75;
    c.ff_bump_age = 0.125;
    c.ff_micro = Micro { roll: 10.0, rpm: 850.0, crank: 0.1, firing: 0.2, mass: 0.3 };
    c.ff_vib = ScriptVib { amp: 0.4, period: 2.0, t: 0.12 };
    c.ff_rumble = 0.6;
    c.steer = Some(("Test wheel".into(), 0.2, 0.1, true));
    c.settled = vec![0.3, 0.4];
    let devices = std::ptr::addr_of!(c.devices);
    c.held.event(&c.cfg, "Test wheel", 1, true, &mut c.actions);
    let mut cfg = c.configuration();
    cfg[0].ff_invert = Some(false);
    c.install_cfg(cfg);
    assert_eq!(std::ptr::addr_of!(c.devices), devices);
    assert_eq!((c.ff_t, c.ff_lateral, c.ff_bump, c.ff_bump_age), (12.0, 0.25, 0.75, 0.125));
    assert_eq!(c.steer, Some(("Test wheel".into(), 0.2, 0.1, true)));
    assert_eq!(c.settled, vec![0.3, 0.4]);
    assert_eq!((c.ff_micro.roll, c.ff_micro.rpm, c.ff_micro.crank, c.ff_micro.firing, c.ff_micro.mass), (10.0, 850.0, 0.1, 0.2, 0.3));
    assert_eq!((c.ff_vib.amp, c.ff_vib.period, c.ff_vib.t, c.ff_rumble), (0.4, 2.0, 0.12, 0.6));
    assert_eq!(c.actions.last(), Some(&("kw_s_1_fest".into(), false)));
    assert_eq!(c.configuration()[0].ff_invert, Some(false));
}

#[test]
fn a_device_nobody_set_up_steers_only_once_its_axis_moved() {
    let mut moved = Vec::new();
    // idle at its centre (or a little off it): it does not steer, so the arrow keys
    // switch the interior camera as with no device at all
    assert!(!free_axis_steers(&mut moved, "4 axes, 25 buttons, joystick", 0.0));
    assert!(!free_axis_steers(&mut moved, "4 axes, 25 buttons, joystick", -0.1));
    // turned: it steers, also when back at the centre
    assert!(free_axis_steers(&mut moved, "4 axes, 25 buttons, joystick", 0.4));
    assert!(free_axis_steers(&mut moved, "4 axes, 25 buttons, joystick", 0.0));
    assert!(!free_axis_steers(&mut moved, "another joystick", 0.0));
}

#[test]
fn hot_reload_editing_suppresses_input_and_releases_once() {
    let mut c = Controllers::with_devices(no_hardware(), vec![wheel()]);
    c.held.event(&c.cfg, "Test wheel", 0, true, &mut c.actions);
    c.set_editing(true);
    c.set_editing(true);
    c.poll();
    assert!(c.editing);
    assert_eq!(c.actions, vec![("horn".into(), true), ("horn".into(), false)]);
    c.set_editing(false);
    assert!(!c.editing);
}

#[test]
fn a_latching_switch_stays_in_through_the_menu_and_the_focus() {
    let cfg = vec![DeviceCfg { name: "Test wheel".into(), buttons: vec![(String::new(), String::new()), ("bus_stopbrake".into(), String::new()), ("bus_horn".into(), String::new())], latching: vec![1], ..Default::default() }];
    let mut held = HeldButtons::default();
    let mut actions = Vec::new();
    held.event(&cfg, "Test wheel", 1, true, &mut actions);
    held.event(&cfg, "Test wheel", 2, true, &mut actions);
    actions.clear();
    held.release_momentary(&mut actions);
    assert_eq!(actions, vec![("bus_horn".to_string(), false)]);
    actions.clear();
    // switched out later: its release and the latch's way back still come
    held.event(&cfg, "Test wheel", 1, false, &mut actions);
    assert_eq!(actions.first(), Some(&("bus_stopbrake".to_string(), false)));
}

#[test]
fn hot_reload_releases_original_held_gear_and_does_not_press_new_binding() {
    let mut held = HeldButtons::default();
    let mut cfg = vec![wheel()];
    let mut actions = Vec::new();
    held.event(&cfg, "Test wheel", 1, true, &mut actions);
    cfg[0].buttons[1].0 = "horn".into();
    held.release(&mut actions);
    held.event(&cfg, "Test wheel", 1, false, &mut actions);
    assert_eq!(actions, vec![("kw_s_1_fest".into(), true), ("kw_s_1_fest".into(), false)]);
    held.event(&cfg, "Test wheel", 1, true, &mut actions);
    assert_eq!(actions.last(), Some(&("horn".into(), true)));
}

#[test]
fn hot_reload_does_not_toggle_latching_switch_on_menu_entry() {
    let mut held = HeldButtons::default();
    let cfg = vec![wheel()];
    let mut actions = Vec::new();
    held.event(&cfg, "Test wheel", 2, true, &mut actions);
    held.event(&cfg, "Test wheel", 2, true, &mut actions); // duplicate down
    held.release(&mut actions);
    held.event(&cfg, "Test wheel", 2, false, &mut actions);
    assert_eq!(actions, vec![("blinker_warn_toggle".into(), true), ("blinker_warn_toggle".into(), false)]);
    // A physical release during driving still performs the existing latching behaviour.
    held.event(&cfg, "Test wheel", 2, true, &mut actions);
    held.event(&cfg, "Test wheel", 2, false, &mut actions);
    assert_eq!(actions.len(), 6);
}

#[test]
fn hot_reload_release_uses_mapping_at_press_even_after_external_change() {
    let mut held = HeldButtons::default();
    let mut cfg = vec![wheel()];
    let mut actions = Vec::new();
    held.event(&cfg, "Test wheel", 0, true, &mut actions);
    cfg[0].buttons[0].0 = "door".into();
    held.event(&cfg, "Test wheel", 0, false, &mut actions);
    assert_eq!(actions, vec![("horn".into(), true), ("horn".into(), false)]);
}

#[test]
fn hot_reload_preserves_axis_flags_button_metadata_and_per_device_ffb() {
    let cfg = vec![wheel(), DeviceCfg { name: "Separate pedals".into(), second: "12".into(), ff_scale: Some((1.0, 1.0)), ..Default::default() }];
    assert_eq!(parse_cfg(&cfg_text(&cfg)), cfg);
    assert_eq!(cfg[0].axis_flags[0], 26);
    let (steer, physical) = wheel_steering(0.6, true, 0, 0.1, 1.0);
    assert!((physical + 0.6).abs() < 1e-6);
    assert!((steer + 0.5 / 0.9).abs() < 1e-6);
}

#[test]
fn hot_reload_mixed_devices_keep_the_selected_steering_kind() {
    let mut out = Analog::default();
    set_steering(&mut out, 0.75, false);
    set_steering(&mut out, 0.0, true);
    assert_eq!(out.steering, Some(0.75));
    assert!(!out.stick);
    set_steering(&mut out, -0.9, true);
    assert_eq!(out.steering, Some(-0.9));
    assert!(out.stick);
    set_steering(&mut out, -0.9, false);
    assert!(out.stick); // identical readings keep the original source
}

#[test]
fn hot_reload_gamepad_vibration_uses_its_own_device_scale() {
    let mut cfg = vec![wheel(), DeviceCfg { name: "Xbox Controller".into(), ff_scale: Some((1.0, 0.5)), ..Default::default() }];
    assert_eq!(rumble_scale(&cfg, "Xbox Controller"), 0.5);
    cfg[1].ff_scale = Some((1.0, 1.5));
    assert_eq!(rumble_scale(&cfg, "Xbox Controller"), 1.5);
    assert_eq!(rumble_scale(&cfg, "Test wheel"), 1.25);
    assert_eq!(rumble_scale(&cfg, "Unconfigured gamepad"), 1.0);
}

#[test]
fn hot_reload_custom_gamepad_layout_owns_analog_controls() {
    let mut cfg = vec![DeviceCfg { name: "Xbox Controller".into(), second: "0".into(), ..Default::default() }];
    cfg[0].buttons.push(("horn".into(), "0".into()));
    assert!(!custom_gamepad_axes(&cfg, "Xbox Controller"));
    cfg[0].axes[0] = Some((Func::Steering, false));
    assert!(custom_gamepad_axes(&cfg, "Xbox Controller"));
    let mut axes = vec![(0, 0.2), (1, -0.1)];
    gamepad_triggers(&mut axes, 0.0, 1.0);
    assert_eq!(axes[2..], [(6, -1.0), (7, 1.0)]);
    gamepad_triggers(&mut axes, 1.0, 0.0);
    assert_eq!(axes.len(), 4);
}

#[test]
fn hot_reload_save_replaces_existing_file_and_rejects_invalid_input() {
    let dir = std::env::temp_dir().join(format!("openomsi-controllers-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("gamectrler.cfg");
    let mut cfg = vec![wheel()];
    save_cfg_to(&path, &cfg).unwrap();
    cfg[0].ff_invert = Some(false);
    save_cfg_to(&path, &cfg).unwrap();
    assert_eq!(parse_cfg(&std::fs::read_to_string(&path).unwrap()), cfg);
    let original = std::fs::read(&path).unwrap();
    cfg[0].buttons[0].0 = "horn\n[ctrl]".into();
    assert!(save_cfg_to(&path, &cfg).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert!(save_cfg_to(&path.join("not-a-directory.cfg"), &[]).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_forces_go_to_the_wheel_whichever_steering_device_is_listed_first() {
    let shifter = Some(("Shifter".to_string(), 0.0, false));
    let wheel = Some(("Test wheel".to_string(), 0.1, true));
    assert!(takes_feedback(&None, false));
    assert!(takes_feedback(&shifter, true));
    assert!(!takes_feedback(&shifter, false));
    assert!(!takes_feedback(&wheel, false));
    assert!(!takes_feedback(&wheel, true));
}

#[test]
fn only_a_connected_force_feedback_wheel_set_up_to_steer_counts_as_the_wheel() {
    let connected = |name: &str, gamepad: bool, ff: bool| Connected { name: name.into(), hardware_id: None, axes: vec![(0, 0.0)], gamepad, ff, ff_capable: ff, buttons: 0 };
    let mut pad = wheel();
    pad.name = "Xbox Controller".into();
    assert!(wheel_set_up(&[wheel()], &[connected("Test wheel", false, true)]));
    assert!(!wheel_set_up(&[wheel()], &[connected("Test wheel", false, false)]));
    assert!(!wheel_set_up(&[wheel()], &[connected("Shifter", false, true)]));
    assert!(!wheel_set_up(&[pad], &[connected("Xbox Controller", true, true)]));
    assert!(!wheel_set_up(&[DeviceCfg { name: "Test wheel".into(), ..Default::default() }], &[connected("Test wheel", false, true)]));
}
