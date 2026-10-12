use super::*;

#[test]
fn only_the_g923_has_its_force_flipped() {
    assert!(force_flipped_for(Some((0x046D, 0xC266)))); // G923 PlayStation/PC
    assert!(force_flipped_for(Some((0x046D, 0xC26E)))); // G923 Xbox
    assert!(!force_flipped_for(Some((0x046D, 0xC24F)))); // G29
    assert!(!force_flipped_for(Some((0x044F, 0xB66E)))); // Thrustmaster T300
    assert!(!force_flipped_for(Some(VJOY_HARDWARE_ID)));
    assert!(!force_flipped_for(None));
}

#[test]
fn failed_opens_back_off_to_half_a_minute() {
    let secs: Vec<u64> = (0..8).map(|t| retry_delay(t).as_secs()).collect();
    assert_eq!(secs, [1, 2, 4, 8, 16, 30, 30, 30]);
}

#[test]
fn disappearing_device_releases_buttons_and_hats() {
    let mut state = RawState::default();
    state.buttons[7] = 0x80;
    state.pov[1] = 9000;
    let events = state_release_events("wheel", &state);
    assert!(events.contains(&("wheel".to_string(), 7, false)));
    assert!(events.contains(&(
        "wheel".to_string(),
        crate::controllers::HAT_BUTTONS + 5,
        false
    )));
    assert_eq!(events.len(), 2);
}

#[test]
fn poll_failure_never_reads_stale_state_and_retries_after_acquire() {
    let calls = RefCell::new(Vec::new());
    let mut attempt = 0;
    let result = read_with_recovery(
        || {
            calls.borrow_mut().push("poll");
            attempt += 1;
            if attempt == 1 {
                return Err("poll lost");
            }
            calls.borrow_mut().push("state");
            Ok(42)
        },
        || {
            calls.borrow_mut().push("acquire");
            true
        },
    );
    assert_eq!(result, Ok(42));
    assert_eq!(*calls.borrow(), ["poll", "acquire", "poll", "state"]);
}

#[test]
fn state_failure_and_poll_failure_share_bounded_recovery() {
    for stage in ["poll", "state"] {
        let mut reads = 0;
        let mut acquires = 0;
        let result: Result<(), _> = read_with_recovery(
            || {
                reads += 1;
                Err(stage)
            },
            || {
                acquires += 1;
                true
            },
        );
        assert_eq!(result, Err(stage));
        assert_eq!((reads, acquires), (2, 1));
    }
    let mut reads = 0;
    let result: Result<(), _> = read_with_recovery(
        || {
            reads += 1;
            Err("lost")
        },
        || false,
    );
    assert_eq!(result, Err("lost"));
    assert_eq!(reads, 1);
}

#[test]
fn hotplug_targets_exact_device_path_even_with_the_same_product() {
    let removed = vec!["hid#vid_046d&pid_c29b#first".to_string()];
    assert!(removed_path(Some("HID#VID_046D&PID_C29B#FIRST"), &removed));
    assert!(!removed_path(
        Some("hid#vid_046d&pid_c29b#second"),
        &removed
    ));
    assert!(!removed_path(Some("hid#keyboard"), &removed));
    assert!(!removed_path(None, &removed));
}

#[test]
fn invalid_state_releases_shared_wheel_shifter_and_cannot_drive() {
    let mut state = RawState::default();
    state.axes[0] = RANGE;
    state.axes[1] = RANGE;
    state.buttons[0] = 0x80; // wheel rim
    state.buttons[8] = 0x80; // G27-style shifter in the same device
    state.buttons[127] = 0x80;
    state.pov[3] = 18000;
    let mut valid = true;
    let mut events = Vec::new();
    assert_eq!(state_axes(&state, valid, &[true; 8])[0], (0, 1.0));
    invalidate_state("G27", &mut state, &mut valid, &mut events);
    assert!(!valid);
    assert!(state_axes(&state, valid, &[true; 8]).is_empty());
    assert_eq!(events.len(), 4);
    for button in [0, 8, 127] {
        assert!(events.contains(&("G27".into(), button, false)));
    }
    invalidate_state("G27", &mut state, &mut valid, &mut events);
    assert_eq!(events.len(), 4); // no repeated releases
                                 // The first valid reconnect sample compares against an empty snapshot.
    assert_eq!(state.buttons[8], 0);
}

#[test]
fn simultaneous_gate_change_releases_before_pressing_in_both_directions() {
    for (old, new) in [(1, 2), (2, 1)] {
        let mut events = vec![("wheel".into(), new, true), ("wheel".into(), old, false)];
        order_button_events(&mut events);
        assert_eq!(
            events,
            [("wheel".into(), old, false), ("wheel".into(), new, true)]
        );
    }
}

#[test]
fn button_offsets_follow_instances_not_enumeration_order() {
    let objects = [7, 1, 7].map(|n| InputObject {
        guid: GUID_Button,
        ty: DIDFT_BUTTON | (n << 8),
        flags: 0,
    });
    let (format, _, _) = format_objects(&objects);
    assert_eq!(format.iter().map(|o| o.dwOfs).collect::<Vec<_>>(), [55, 49]);
}

/// The window that hears of devices plugged in or out opens (it is a thread's own).
#[test]
fn notification_window_opens() {
    let w = std::thread::spawn(|| {
        notification_window().map(|w| unsafe { DestroyWindow(w).is_ok() })
    })
    .join()
    .unwrap();
    assert_eq!(w, Some(true));
}

#[test]
fn controller_device_filter_accepts_controllers_and_rejects_mice_keyboards_and_vendor_devices()
{
    assert!(is_controller_device(DI8DEVTYPE_JOYSTICK, 0, 0));
    assert!(is_controller_device(DI8DEVTYPE_GAMEPAD, 0, 0));
    assert!(is_controller_device(DI8DEVTYPE_DRIVING, 0, 0));
    assert!(is_controller_device(DI8DEVTYPE_FLIGHT, 0, 0));
    assert!(is_controller_device(DI8DEVTYPE_DEVICECTRL, 0, 0));

    assert!(!is_controller_device(DI8DEVTYPE_KEYBOARD, 0, 0));
    assert!(!is_controller_device(DI8DEVTYPE_MOUSE, 0, 0));
    assert!(!is_controller_device(DI8DEVTYPE_SCREENPOINTER, 0, 0));

    // Generic devices (DI8DEVTYPE_DEVICE) with game usages (Arduino button boxes):
    assert!(is_controller_device(DI8DEVTYPE_DEVICE, 0x01, 0x04)); // Joystick
    assert!(is_controller_device(DI8DEVTYPE_DEVICE, 0x01, 0x05)); // Gamepad
    assert!(is_controller_device(DI8DEVTYPE_DEVICE, 0x01, 0x08)); // Multi-axis
    assert!(is_controller_device(DI8DEVTYPE_DEVICE, 0x02, 0x01)); // Simulation controls

    // Generic devices with consumer/vendor/power/undefined usages (Razer, Logitech mice/keyboards macro collections):
    assert!(!is_controller_device(DI8DEVTYPE_DEVICE, 0x0C, 0x01)); // Consumer / media keys
    assert!(!is_controller_device(DI8DEVTYPE_DEVICE, 0x01, 0x80)); // System control
    assert!(!is_controller_device(DI8DEVTYPE_DEVICE, 0x01, 0x00)); // Undefined
    assert!(!is_controller_device(DI8DEVTYPE_DEVICE, 0xFF00, 0x01)); // Vendor specific
}

/// A wheel's force goes to its X axis first, even where the driver lists another axis as
/// its first actuator (the MOZA R5's Y); a device without an X axis gets its actuators.
#[test]
fn the_wheel_force_is_tried_on_the_x_axis_first() {
    let mut has = [false; 8];
    has[0] = true;
    has[1] = true;
    assert_eq!(force_axes(&has, &[4, 0]), vec![0, 4]);
    assert_eq!(force_axes(&has, &[4]), vec![0, 4]);
    assert_eq!(force_axes(&has, &[]), vec![0]);
    let only_z = [false, false, true, false, false, false, false, false];
    assert_eq!(force_axes(&only_z, &[8]), vec![8]);
}

#[test]
fn button_box_without_axes_formats_successfully() {
    let objects: Vec<InputObject> = (0..16)
        .map(|n| InputObject {
            guid: GUID_Button,
            ty: DIDFT_BUTTON | (n << 8),
            flags: 0,
        })
        .collect();
    let (objs, has_axis, ff_axes) = format_objects(&objects);
    assert_eq!(objs.len(), 16);
    assert_eq!(has_axis, [false; 8]);
    assert!(ff_axes.is_empty());
    for (i, obj) in objs.iter().enumerate() {
        assert_eq!(obj.dwOfs, (48 + i) as u32);
    }
}
