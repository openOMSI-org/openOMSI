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
