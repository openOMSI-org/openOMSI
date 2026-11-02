//! Regression tests for the extracted player controls.

use super::*;

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
