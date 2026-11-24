use super::*;

#[cfg(test)]
mod axis_shape_tests {
    use super::axis_shape;
    #[test]
    fn characteristics_match_omsi() {
        assert!((axis_shape(0.75, 24) - 0.625).abs() < 1e-6);
        assert!((axis_shape(0.5, 24) - 0.5).abs() < 1e-6);
        assert!((axis_shape(0.5, 8) - 0.25).abs() < 1e-6);
        assert!((axis_shape(0.5, 4) - (std::f32::consts::PI / 4.0).sin()).abs() < 1e-6);
        assert!((axis_shape(0.3, 0) - 0.3).abs() < 1e-6);
        assert_eq!(axis_shape(1.0, 2), 1.0);
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_gilrs_event_that_panics_is_skipped() {
        let mut queue = vec![Some(2), None, Some(1)];
        let mut next = || match queue.pop().unwrap() {
            None => panic!("index out of bounds: the len is 0 but the index is 0"),
            ev => ev,
        };
        assert_eq!(super::next_event_caught(&mut next), Some(1));
        assert_eq!(super::next_event_caught(&mut next), Some(2));
    }

    #[test]
    fn latching_switches_switch_back_and_stay_in_the_file() {
        assert_eq!(super::latch_release("blinker_left_set"), "blinker_off");
        assert_eq!(super::latch_release("Blinker_Right_Set"), "blinker_off");
        assert_eq!(
            super::latch_release("parking_brake_set"),
            "parking_brake_release"
        );
        assert_eq!(
            super::latch_release("blinker_warn_toggle"),
            "blinker_warn_toggle"
        );
        let text = "[ctrl]\r\nAER0 Truck Simulator Gear\r\n0\r\n\r\n[buttons]\r\n2\r\nblinker_warn_toggle\r\n0\r\n\r\n0\r\n\r\n[openOMSI.Latching]\r\n1\r\n";
        let devices = super::parse_cfg(text);
        assert_eq!(devices[0].latching, vec![0]);
        let again = super::parse_cfg(&super::cfg_text(&devices));
        assert_eq!(again[0].latching, vec![0]);
        assert_eq!(again[0].buttons, devices[0].buttons);
        assert!(super::cfg_text(&devices).contains("[openOMSI.Latching]\r\n1\r\n"));
    }

    #[test]
    fn look_axes_are_kept_in_the_file_and_rest_at_the_centre() {
        // (openOMSI's own numbers after OMSI's five, written back as read)
        for f in [super::Func::LookX, super::Func::LookY] {
            assert_eq!(super::Func::from_code(super::Func::code(Some(f))), Some(f));
        }
        assert_eq!(
            super::Func::LABELS.len() as i32,
            super::Func::code(Some(super::Func::LookY)) + 2
        );
        assert_eq!(super::look_axis(0.1), 0.0);
        assert_eq!(super::look_axis(1.0), 1.0);
        assert_eq!(super::look_axis(-1.0), -1.0);
        assert!(super::look_axis(0.5) > 0.4 && super::look_axis(0.5) < 0.5);
    }

    #[test]
    fn a_stick_leaves_its_dead_zone_from_nothing() {
        use super::stick_deadzone;
        assert_eq!(stick_deadzone(0.05), 0.0);
        assert_eq!(stick_deadzone(-0.08), 0.0);
        assert!(stick_deadzone(0.0801) < 0.001);
        assert!((stick_deadzone(1.0) - 1.0).abs() < 1e-6);
        assert!((stick_deadzone(-1.0) + 1.0).abs() < 1e-6);
        assert!(stick_deadzone(-0.5) < 0.0);
    }

    #[test]
    fn stick_steering_is_smoothed_at_any_frame_rate() {
        use super::smooth_toward;
        assert_eq!(smooth_toward(0.0, 0.7, 0.016, 0.0), 0.7);
        // never past the target
        assert!(smooth_toward(0.0, 0.5, 1.0, 0.12) <= 0.5);
        // two 8 ms frames land where one 16 ms frame does
        let twice = smooth_toward(smooth_toward(0.0, 1.0, 0.008, 0.12), 1.0, 0.008, 0.12);
        assert!((twice - smooth_toward(0.0, 1.0, 0.016, 0.12)).abs() < 1e-5);
        // a stick wobbling 0.03 either side of 0.2 every frame: the wheel holds near 0.2
        let mut v = 0.2;
        for i in 0..240 {
            v = smooth_toward(v, if i % 2 == 0 { 0.23 } else { 0.17 }, 1.0 / 60.0, 0.12);
        }
        assert!((v - 0.2).abs() < 0.01, "{v}");
    }

    #[test]
    fn names() {
        assert!(super::names_match(
            "Logitech G25 Racing Wheel USB",
            "Logitech G25 Racing Wheel"
        ));
        assert!(!super::names_match("", "x"));
        assert!(!super::names_match("Logitech G25 Racing Wheel", ""));
        assert!(!super::names_match("Logitech G25 Racing Wheel", " ... "));
        assert!(!super::names_match("", ""));
        assert!(super::names_match("Кнопочная панель", "кнопочная  панель"));
        assert!(!super::names_match("Кнопочная панель", "Руль"));
    }

    #[test]
    fn only_real_xbox_pads_are_read_as_direct_input_pads() {
        assert!(super::xbox_direct_input_pad(Some((0x045e, 0x0b13)), false));
        assert!(!super::xbox_direct_input_pad(Some((0x046d, 0xc261)), true));
        assert!(!super::xbox_direct_input_pad(Some((0x046d, 0xc261)), false));
        assert!(!super::xbox_direct_input_pad(Some((0x045e, 0x0034)), true));
        assert!(!super::xbox_direct_input_pad(Some((0x045e, 0x0b13)), true));
        assert!(!super::xbox_direct_input_pad(None, false));
    }

    #[test]
    fn an_xbox_named_ff_wheel_stays_visible_in_the_launcher() {
        assert!(super::include_di_device(
            "G920 Driving Force Racing Wheel for Xbox One",
            true,
            false,
            super::SystemPadMatch::Named,
        ));
        assert!(!super::include_di_device(
            "Controller (Xbox One)",
            false,
            false,
            super::SystemPadMatch::Named,
        ));
    }

    #[test]
    fn xbox_gamepad_with_direct_input_twin_is_not_claimed_by_direct_input() {
        let is_claimed_by_di = |name: &str, ff_capable: bool| {
            super::include_di_device(name, ff_capable, false, super::SystemPadMatch::Named)
        };
        assert!(!is_claimed_by_di(
            "Controller (XBOX 360 For Windows)",
            false
        ));
        assert!(!is_claimed_by_di("Xbox Wireless Controller", false));
        assert!(is_claimed_by_di(
            "Logitech G29 Driving Force Racing Wheel",
            true
        ));
        assert!(is_claimed_by_di("HORI Racing Wheel APEX", false));
        assert!(is_claimed_by_di(
            "G920 Driving Force Racing Wheel for Xbox One",
            true
        ));
    }

    #[test]
    fn a_dualsense_on_macos_is_one_device() {
        let pad = ["PS5 Controller", "DualSense Wireless Controller"];
        let id = Some((0x054c, 0x0ce6));
        // the HID device under the system's name, or with the same ids, is gilrs's pad
        assert!(super::same_hid_device(
            "DualSense Wireless Controller",
            None,
            pad,
            id
        ));
        assert!(super::same_hid_device(
            "Wireless Controller",
            id,
            ["PS5 Controller", "PS5 Controller"],
            id
        ));
        assert!(!super::same_hid_device(
            "HORI Truck Control System",
            Some((0x0f0d, 0x01b7)),
            pad,
            id
        ));
        assert!(!super::same_hid_device(
            "Thrustmaster T.16000M",
            None,
            pad,
            None
        ));
    }

    #[test]
    fn analog_triggers_are_no_buttons_on_macos() {
        // L2 and R2 (Rx, Ry), sliders, the simulation page's pedals and the hat
        for code in [0x10033, 0x10034, 0x10036, 0x10039, 0x200C4, 0x200C5] {
            assert!(super::mac_axis_code(code), "{code:#x}");
        }
        // the button page, and a generic desktop D-pad button
        for code in [0x90001, 0x9000e, 0x10090] {
            assert!(!super::mac_axis_code(code), "{code:#x}");
        }
    }

    #[test]
    fn a_hat_switch_is_read_with_up_negative() {
        // macOS: the hat's two axes; an SDL mapping's D-pad Y comes turned round
        assert_eq!(super::hat_axis(0x10039, 1.0, false), Some((0, 1.0)));
        assert_eq!(super::hat_axis(0x1003A, 1.0, true), Some((1, -1.0)));
        assert_eq!(super::hat_axis(0x1003A, 1.0, false), Some((1, 1.0)));
        // Linux: ABS_HAT0X .. ABS_HAT3Y; D-pad Y mapped by SDL/gilrs comes turned round
        assert_eq!(super::hat_axis(0x30010, -1.0, false), Some((0, -1.0)));
        assert_eq!(super::hat_axis(0x30011, 1.0, true), Some((1, -1.0)));
        assert_eq!(super::hat_axis(0x30011, 1.0, false), Some((1, 1.0)));
        assert_eq!(super::hat_axis(0x30017, 1.0, false), Some((7, 1.0)));
        assert_eq!(super::hat_axis(0x30018, 1.0, false), None);
        assert_eq!(super::hat_axis(0x10030, 1.0, false), None);
    }

    #[test]
    fn system_gamepad_buttons_work_alongside_direct_input_wheels() {
        assert!(super::use_gilrs_buttons(true, true));
        assert!(!super::use_gilrs_buttons(true, false));
        assert!(super::use_gilrs_buttons(false, false));
    }

    #[test]
    fn wheels_are_not_treated_as_system_gamepads() {
        assert!(!super::is_system_gamepad(
            "Logitech G29 Driving Force Racing Wheel",
            true
        ));
        assert!(!super::is_system_gamepad("Logitech Driving Force GT", true));
        assert!(!super::is_system_gamepad(
            "Logitech Driving Force GT",
            false
        ));
        assert!(!super::is_system_gamepad("Thrustmaster T300RS", true));
        assert!(!super::is_system_gamepad("Thrustmaster T300RS", false));

        assert!(super::is_system_gamepad("Xbox 360 Controller", false));
        assert!(super::is_system_gamepad("Controller (Xbox One)", false));
        assert!(super::is_system_gamepad("Xbox Series X Controller", false));
        assert!(!super::is_system_gamepad(
            "G920 Driving Force Racing Wheel for Xbox One",
            true
        ));
    }

    #[test]
    fn matched_wgi_twins_drop_only_their_direct_input_twin_and_keep_wheels_and_unmatched_pads() {
        let flydigi_id = Some((0x045e, 0x028e));
        let second_id = Some((0x20d6, 0xa711));
        let g920_id = Some((0x046d, 0xc262));
        let tmx_id = Some((0x044f, 0xb67f));
        let gilrs_pads = [
            ("Flydigi Direwolf 4", flydigi_id, true),
            (
                "G920 Driving Force Racing Wheel for Xbox One",
                g920_id,
                false,
            ),
        ];

        let flydigi_match = super::match_system_pad("Flydigi Direwolf 4", flydigi_id, gilrs_pads);
        assert_eq!(flydigi_match, super::SystemPadMatch::Wgi);
        assert!(!super::include_di_device(
            "Flydigi Direwolf 4",
            true,
            true,
            flydigi_match
        ));
        assert!(!super::include_di_device(
            "Flydigi Direwolf 4",
            false,
            true,
            flydigi_match
        ));
        assert!(super::is_system_gamepad_kind(
            "Flydigi Direwolf 4",
            true,
            false
        ));

        let second_match =
            super::match_system_pad("Generic DirectInput Only Pad", second_id, gilrs_pads);
        assert_eq!(second_match, super::SystemPadMatch::None);
        assert!(super::include_di_device(
            "Generic DirectInput Only Pad",
            false,
            true,
            second_match
        ));
        assert!(super::include_di_device(
            "Generic DirectInput Only Pad",
            true,
            true,
            second_match
        ));

        let g920_match = super::match_system_pad(
            "G920 Driving Force Racing Wheel for Xbox One",
            g920_id,
            gilrs_pads,
        );
        assert_eq!(g920_match, super::SystemPadMatch::Named);
        assert!(super::include_di_device(
            "G920 Driving Force Racing Wheel for Xbox One",
            true,
            false,
            g920_match
        ));
        assert!(super::include_di_device(
            "G920 Driving Force Racing Wheel for Xbox One",
            true,
            true,
            g920_match
        ));

        let tmx_match =
            super::match_system_pad("Thrustmaster TMX Racing Wheel", tmx_id, gilrs_pads);
        assert_eq!(tmx_match, super::SystemPadMatch::None);
        assert!(super::include_di_device(
            "Thrustmaster TMX Racing Wheel",
            true,
            true,
            tmx_match
        ));
        assert!(!super::is_system_gamepad_kind(
            "Logitech G29 Driving Force Racing Wheel",
            false,
            true
        ));

        let xbox_wgi_pads = [
            ("Xbox One Game Controller", Some((0x045e, 0x02ea)), true),
            ("Xbox Wireless Controller", Some((0x045e, 0x0b13)), true),
        ];
        let xbox_di_match = super::match_system_pad(
            "Controller (Xbox One For Windows)",
            Some((0x045e, 0x02ff)),
            xbox_wgi_pads,
        );
        assert_eq!(xbox_di_match, super::SystemPadMatch::Wgi);
        assert!(!super::include_di_device(
            "Controller (Xbox One For Windows)",
            false,
            true,
            xbox_di_match
        ));
        let bt_di_match = super::match_system_pad(
            "Bluetooth XINPUT compatible input device",
            Some((0x045e, 0x0b13)),
            xbox_wgi_pads,
        );
        assert_eq!(bt_di_match, super::SystemPadMatch::Wgi);
        assert!(!super::include_di_device(
            "Bluetooth XINPUT compatible input device",
            false,
            true,
            bt_di_match
        ));
        assert_eq!(
            super::match_system_pad("Generic DirectInput Only Pad", second_id, xbox_wgi_pads),
            super::SystemPadMatch::None
        );
        assert_ne!(
            super::match_system_pad(
                "G920 Driving Force Racing Wheel for Xbox One",
                g920_id,
                xbox_wgi_pads
            ),
            super::SystemPadMatch::Wgi
        );

        let gilrs_edges = [("Xbox One Game Controller".to_string(), 0usize, true)];
        let xinput_edges = [
            (gilrs::Button::South, 0usize, true),
            (gilrs::Button::East, 1usize, false),
        ];
        let mut out = Vec::new();
        let mut pad_buttons = Vec::new();
        super::push_xinput_edges(
            &gilrs_edges,
            "Xbox One Game Controller",
            xinput_edges,
            &mut out,
            &mut pad_buttons,
        );
        assert_eq!(
            out,
            vec![("Xbox One Game Controller".to_string(), 1, false)]
        );
        assert_eq!(
            pad_buttons,
            vec![(
                "Xbox One Game Controller".to_string(),
                gilrs::Button::East,
                Some(1),
                false
            )]
        );
    }

    #[test]
    fn binding_one_pad_button_keeps_the_defaults_of_the_others() {
        use super::{DeviceCfg, preset_applies};
        let bound = |n: usize, action: &str| {
            let mut d = DeviceCfg {
                name: "Pad".into(),
                ..Default::default()
            };
            d.buttons.resize(n + 1, (String::new(), "0".into()));
            d.buttons[n].0 = action.to_string();
            d
        };
        // a pad nobody set up, or one with no button bound: every default drives
        assert!(preset_applies(None, Some(0)));
        assert!(preset_applies(
            Some(&DeviceCfg {
                name: "Pad".into(),
                ..Default::default()
            }),
            Some(0)
        ));
        // button 8 (the left stick's click) bound: the other buttons keep their defaults...
        let d = bound(8, "horn");
        assert!(preset_applies(Some(&d), Some(0)));
        assert!(preset_applies(Some(&d), Some(5)));
        // ...and the bound one drives what the player chose, not its default
        assert!(!preset_applies(Some(&d), Some(8)));
        // a second action on the same button counts as bound as well
        let mut d = bound(8, "");
        d.more.push((3, "door_2".into()));
        assert!(!preset_applies(Some(&d), Some(3)));
        assert!(preset_applies(Some(&d), Some(0)));
        // a button whose number is unknown keeps the old rule
        assert!(!preset_applies(Some(&bound(8, "horn")), None));
        assert!(preset_applies(Some(&bound(8, "")), None));
    }
}

#[cfg(test)]
mod slot_tests {
    use super::*;

    #[test]
    fn a_missing_axis_does_not_shift_the_rest() {
        // X and Rz only (a wheel with one pedal axis): Rz stays slot 5
        let axes = if cfg!(target_os = "macos") {
            vec![(0x1_0030, 0.5), (0x1_0035, -1.0)]
        } else {
            vec![(0x3_0000, 0.5), (0x3_0005, -1.0)]
        };
        let got = di_slots(&axes);
        assert!(got.contains(&(0, 0.5)), "{got:?}");
        if !cfg!(windows) {
            assert!(got.contains(&(5, -1.0)), "{got:?}");
        }
    }

    /// Linux: a macro keypad's volume knob, a tablet's or a touch code or a hat takes no slot - the stray axis
    /// was X and steered the bus (#868). ABS_WHEEL still takes a free one.
    #[test]
    fn no_controller_axis_takes_no_slot_on_linux() {
        assert!(controller_abs(0x3_0000) && controller_abs(0x3_0008) && controller_abs(0x3_000A));
        // (ABS_MISC and on: a device's vendor-defined axes)
        assert!(controller_abs(0x3_0028) && controller_abs(0x3_002E));
        for code in [
            0x3_0010, 0x3_0017, 0x3_0018, 0x3_001C, 0x3_0020, 0x3_0021, 0x3_002F, 0x3_0035,
        ] {
            assert!(!controller_abs(code), "{code:#x}");
        }
        if cfg!(target_os = "linux") {
            assert!(di_slots(&[(0x3_0021, -1.0), (0x3_0020, 0.3), (0x3_0010, 1.0)]).is_empty());
            assert_eq!(di_slots(&[(0x3_0008, 0.2), (0x3_0020, -1.0)]), [(0, 0.2)]);
            assert_eq!(di_slots(&[(0x3_0028, 0.5)]), [(0, 0.5)]);
        }
    }

    #[test]
    fn two_devices_merge_by_the_larger_movement() {
        let mut s = None;
        set(&mut s, 0.0);
        set(&mut s, 0.4);
        set(&mut s, -0.1);
        assert_eq!(s, Some(0.4));
    }
}

#[cfg(test)]
mod cfg_tests {
    #[test]
    fn feedback_keeps_the_physical_position_inside_the_steering_deadzone() {
        for (axis, deadzone) in [(0.2, 0.3), (0.01, 0.02)] {
            let (steering, position) = super::wheel_steering(axis, false, 0, deadzone, 1.0);
            assert_eq!(steering, 0.0);
            assert_eq!(position, axis);
            let f = super::FfInput {
                on: true,
                kmh: 30.0,
                dt: 0.016,
                ..Default::default()
            };
            let mut t = 0.0;
            let force = super::wheel_force(&f, position, position, &mut t, 1.0, 0.0);
            assert!(force < 0.0, "axis={axis}: {force}");
        }
    }

    #[test]
    fn steering_range_and_reversal_preserve_the_feedback_position() {
        let (steering, position) = super::wheel_steering(0.5, false, 0, 0.1, 2.0);
        assert!((steering - 8.0 / 9.0).abs() < 1e-6);
        assert_eq!(position, 0.5);
        let (steering, position) = super::wheel_steering(0.5, true, 0, 0.1, 2.0);
        assert!((steering + 8.0 / 9.0).abs() < 1e-6);
        assert_eq!(position, -0.5);
        let (steering, position) = super::wheel_steering(0.5, false, 0, 0.0, 0.25);
        assert_eq!(steering, 0.125);
        assert_eq!(position, 0.5);
    }

    #[test]
    fn axis_characteristics_change_bus_steering_but_not_feedback_position() {
        let (steering, position) = super::wheel_steering(0.5, false, 24, 0.0, 1.0);
        assert_eq!(steering, 0.25);
        assert_eq!(position, 0.5);
        let (steering, position) = super::wheel_steering(0.5, true, 24, 0.0, 1.0);
        assert_eq!(steering, -0.25);
        assert_eq!(position, -0.5);
        let (steering, position) = super::wheel_steering(0.4, false, 2, 0.0, 1.0);
        assert!((steering - 0.5).abs() < 1e-6);
        assert_eq!(position, 0.4);
    }

    #[test]
    fn force_feedback_scales_are_saved_per_controller() {
        let devices = vec![
            super::DeviceCfg {
                name: "Wheel A".into(),
                ff_scale: Some((2.0, 0.5)),
                ..Default::default()
            },
            super::DeviceCfg {
                name: "Wheel B".into(),
                ff_scale: Some((0.75, 1.25)),
                ..Default::default()
            },
        ];
        let saved = super::parse_cfg(&super::cfg_text(&devices));
        assert_eq!(saved[0].ff_scale, Some((2.0, 0.5)));
        assert_eq!(saved[1].ff_scale, Some((0.75, 1.25)));
    }

    #[test]
    fn force_feedback_direction_is_saved_and_applied_per_wheel() {
        let devices = vec![
            super::DeviceCfg {
                name: "Wheel A".into(),
                ff_invert: Some(true),
                ..Default::default()
            },
            super::DeviceCfg {
                name: "Wheel B".into(),
                ff_invert: Some(false),
                ..Default::default()
            },
            super::DeviceCfg {
                name: "Uncalibrated".into(),
                ..Default::default()
            },
        ];
        let saved = super::parse_cfg(&super::cfg_text(&devices));
        assert_eq!(saved[0].ff_invert, Some(true));
        assert_eq!(saved[1].ff_invert, Some(false));
        assert_eq!(saved[2].ff_invert, None);
        assert!(super::feedback_inverted(Some(&saved[0]), false, false));
        assert!(!super::feedback_inverted(Some(&saved[1]), true, false));
        assert!(super::feedback_inverted(Some(&saved[2]), true, false));
        assert!(!super::feedback_inverted(Some(&saved[0]), false, true));
        assert!(super::feedback_inverted(Some(&saved[1]), true, true));
        let mut reversed = super::DeviceCfg::default();
        reversed.axes[0] = Some((super::Func::Steering, true));
        assert!(!super::calibrated_steering_reversed(Some(&reversed)));
        reversed.ff_invert = Some(false);
        assert!(super::calibrated_steering_reversed(Some(&reversed)));
    }

    #[test]
    fn exact_wheel_configuration_beats_a_shorter_alias() {
        let devices = vec![
            super::DeviceCfg {
                name: "G920".into(),
                ff_scale: Some((0.0, 0.0)),
                ..Default::default()
            },
            super::DeviceCfg {
                name: "G920 Driving Force Racing Wheel for Xbox One".into(),
                ff_scale: Some((1.2, 0.6)),
                ..Default::default()
            },
        ];
        let matched =
            super::find_device_cfg(&devices, "G920 Driving Force Racing Wheel for Xbox One")
                .unwrap();
        assert_eq!(matched.ff_scale, Some((1.2, 0.6)));
    }

    #[test]
    fn a_reversed_steering_axis_reverses_its_motor_force_too() {
        let mut wheel = super::DeviceCfg::default();
        wheel.axes[0] = Some((super::Func::Steering, true));
        wheel.axes[1] = Some((super::Func::Throttle, true));
        assert!(super::force_axis_reversed(Some(&wheel), Some(0)));
        assert!(!super::force_axis_reversed(Some(&wheel), Some(1)));
        assert!(!super::force_axis_reversed(Some(&wheel), None));
    }

    #[test]
    fn the_stock_file_round_trips() {
        let Ok(bytes) = std::fs::read("../../../OMSI 2 Original/Inputs/gamectrler.cfg") else {
            return;
        };
        let text = omsi_cfg::codepage::decode(&bytes);
        let devs = super::parse_cfg(&text);
        assert!(
            devs.iter().any(|d| d.name.contains("G25")),
            "{:?}",
            devs.iter().map(|d| &d.name).collect::<Vec<_>>()
        );
        let again = super::parse_cfg(&super::cfg_text(&devs));
        assert_eq!(again, devs);
    }
}

#[cfg(test)]
mod button_tests {
    #[test]
    fn an_analog_trigger_is_no_numbered_button_on_linux() {
        assert!(
            super::evdev_axis_code((3 << 16) | 0x02),
            "ABS_Z, a pad's left trigger"
        );
        assert!(
            super::evdev_axis_code((3 << 16) | 0x05),
            "ABS_RZ, its right trigger"
        );
        assert!(
            !super::evdev_axis_code((1 << 16) | 0x138),
            "BTN_TL2 is a key"
        );
        let mut slots = vec![(0, 0.0), (1, 0.0)];
        super::gamepad_triggers(&mut slots, 0.5, 1.0);
        assert_eq!(slots, vec![(0, 0.0), (1, 0.0), (6, 0.0), (7, 1.0)]);
    }

    #[test]
    fn a_wheel_with_buttons_past_the_table_counts_them_in_order() {
        let moza = super::key_bitmap_buttons(
            "ffffffff ffffffffffffffff ffff000000000000 0 0 0 0 ffff00000000 0 0 0 0",
        );
        assert_eq!(moza.len(), 128);
        if cfg!(target_os = "linux") {
            assert_eq!(super::button_index(&moza, 0x120), Some(0));
            assert_eq!(super::button_index(&moza, 0x12f), Some(15));
            assert_eq!(super::button_index(&moza, 0x270), Some(16));
            assert_eq!(super::button_index(&moza, 0x2c0), Some(96));
            let pad: Vec<u32> = vec![0x130, 0x131, 0x133, 0x134];
            assert_eq!(super::button_index(&pad, 0x133), None);
        }
        // a button box of BTN_TRIGGER_HAPPY codes only: from 0, no sixteen ghosts (#1879)
        let panel: Vec<u32> = (0x2c0..0x2c8).collect();
        assert_eq!(super::button_index(&panel, 0x2c0), Some(0));
        assert_eq!(super::button_count(&panel), 8);
    }

    #[test]
    fn a_device_lists_as_many_buttons_as_its_highest_number() {
        let moza = super::key_bitmap_buttons(
            "ffffffff ffffffffffffffff ffff000000000000 0 0 0 0 ffff00000000 0 0 0 0",
        );
        assert_eq!(super::button_count(&moza), 128);
        if cfg!(target_os = "linux") {
            assert_eq!(super::button_count(&[0x130, 0x131, 0x133, 0x134]), 5);
        }
        assert_eq!(super::button_count(&[]), 0);
    }

    #[test]
    fn buttons_count_as_directinput_does() {
        if cfg!(target_os = "macos") {
            assert_eq!(super::code_button(0x9_0001), Some(0));
            assert_eq!(super::code_button(0x9_0010), Some(15));
        }
        if cfg!(target_os = "linux") {
            assert_eq!(super::code_button(0x1_0120), Some(0));
            assert_eq!(super::code_button(0x1_02c0), Some(16));
        }
        if cfg!(windows) {
            assert_eq!(super::code_button(3), Some(3));
            // an analog trigger (WGI axis code) is no numbered button
            assert_eq!(super::code_button(0x1_0004), None);
            // the D-pad is the first hat, not dropped
            assert_eq!(super::code_button(u32::MAX - 3), Some(super::HAT_BUTTONS));
            assert_eq!(super::code_button(u32::MAX), Some(super::HAT_BUTTONS + 3));
        }
    }
}

#[cfg(test)]
mod device_kind_tests {
    #[test]
    fn a_mapped_constant_force_wheel_is_not_a_gamepad() {
        assert!(super::mapped_device_is_gamepad(true, false));
        assert!(!super::mapped_device_is_gamepad(true, true));
        assert!(!super::mapped_device_is_gamepad(false, true));
    }
}

#[cfg(test)]
mod stick_deadzone_tests {
    use super::stick_deadzone_with;

    #[test]
    fn the_dead_zone_is_the_settings_and_the_rest_of_the_stick_still_reaches_one() {
        assert_eq!(stick_deadzone_with(0.14, 0.15), 0.0);
        assert!(stick_deadzone_with(0.3, 0.15) > 0.0);
        assert!((stick_deadzone_with(-1.0, 0.15) + 1.0).abs() < 1e-6);
        assert_eq!(stick_deadzone_with(0.05, 0.0), 0.05);
    }
}

#[cfg(test)]
mod stick_steering_tests {
    #[test]
    fn an_idle_device_nobody_set_up_does_not_hold_the_sticks_steering() {
        assert!(super::stick_steers(None, false, 0.0));
        assert!(super::stick_steers(Some(0.0), false, -0.6));
        assert!(super::stick_steers(Some(0.0), false, 0.0));
        assert!(super::stick_steers(Some(0.02), false, 0.0));
        assert!(!super::stick_steers(Some(0.8), false, 0.3));
        assert!(!super::stick_steers(Some(0.0), true, 1.0));
    }
}

#[cfg(test)]
mod right_stick_look_tests {
    #[test]
    fn disabling_automatic_look_keeps_driving_controls_and_assigned_look() {
        for assigned in [[0.0, 0.0], [0.5, -0.25]] {
            let mut analog = super::Analog {
                steering: Some(0.3),
                stick: true,
                throttle: Some(0.8),
                brake: Some(0.2),
                clutch: Some(0.4),
                look: assigned,
            };
            analog.apply_default_gamepad_look(false, 1.0, -1.0);
            assert_eq!(analog.look, assigned);
            assert_eq!(analog.steering, Some(0.3));
            assert!(analog.stick);
            assert_eq!(analog.throttle, Some(0.8));
            assert_eq!(analog.brake, Some(0.2));
            assert_eq!(analog.clutch, Some(0.4));
        }
    }

    #[test]
    fn enabled_automatic_look_keeps_dead_zone_direction_and_assigned_axes() {
        let mut analog = super::Analog::default();
        analog.apply_default_gamepad_look(true, 0.1, -0.1);
        assert_eq!(analog.look, [0.0, 0.0]);
        analog.apply_default_gamepad_look(true, 1.0, -1.0);
        assert_eq!(analog.look, [1.0, 1.0]);
        analog.look = [0.25, -0.5];
        analog.apply_default_gamepad_look(true, -1.0, 1.0);
        assert_eq!(analog.look, [0.25, -0.5]);
    }
}
