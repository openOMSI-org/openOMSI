use super::{direct_door_buttons, touch_lock_angle, DIRECT_DOOR_GROUPS};

#[test]
fn direct_door_buttons_are_independent_zero_one_and_combined_two_three() {
    assert_eq!(DIRECT_DOOR_GROUPS, [
        &["bus_doorfront0"][..],
        &["bus_doorfront1"][..],
        &["bus_doorfront2", "bus_doorfront3"][..],
    ]);
    assert_eq!(direct_door_buttons(["bus_doorfront0", "bus_doorfront1", "bus_doorfront2", "bus_doorfront3", "bus_doorfront4"]), vec![0, 1, 2]);
    assert_eq!(direct_door_buttons(["bus_doorfront3", "bus_doorfront1", "bus_doorfront4"]), vec![1, 2]);
    assert_eq!(direct_door_buttons(["BUS_DOORFRONT0", "bus_doorfront2", "bus_dooraft"]), vec![0, 2]);
    assert!(direct_door_buttons(["bus_doorfront4", "bus_doorfront5", "bus_doorfrontX", "bus_doorfront01"]).is_empty());
}

use crate::settings::Settings;

/// The cab panel's buttons stay on the screen, whatever its size: four across on a wide
/// one, fewer and wider on a phone held upright (they ran off both sides of it).
#[test]
fn the_panels_buttons_fit_the_screen() {
    for (w, h, u) in [(360.0, 800.0, 0.9), (412.0, 915.0, 1.03), (820.0, 1180.0, 1.35), (800.0, 360.0, 0.9), (320.0, 568.0, 0.85)] {
        let rects = super::panel_rects(14, w, h, 14.0 * u, 21.0 * u, u);
        assert_eq!(rects.len(), 14);
        for r in &rects {
            assert!(r.x >= 0.0 && r.right() <= w + 0.5, "{w}x{h}: {r:?} is out sideways");
            assert!(r.y >= 0.0 && r.bottom() <= h + 0.5, "{w}x{h}: {r:?} is out up or down");
        }
    }
    let portrait = super::panel_rects(14, 360.0, 800.0, 12.6, 18.9, 0.9);
    let wide = super::panel_rects(14, 820.0, 1180.0, 18.9, 28.35, 1.35);
    assert!(portrait.iter().filter(|r| r.y == portrait[0].y).count() < wide.iter().filter(|r| r.y == wide[0].y).count());
}

/// The information bar keeps to the gap between the buttons along the top (it lay under
/// them, its ends hidden, #1164), or goes under them where the gap is narrow.
#[test]
fn the_information_bar_keeps_clear_of_the_buttons_along_the_top() {
    assert_eq!(super::info_room(1280.0, 14.0, 278.0, 914.0, 56.0, 1.0), [286.0, 906.0, 14.0]);
    assert_eq!(super::info_room(720.0, 14.0, 278.0, 470.0, 56.0, 1.0), [14.0, 706.0, 64.0]);
}

#[test]
fn a_drag_scrolls_the_shell_in_pixels_at_any_screen_density() {
    // Finger up => negative wheel => the menu moves down.
    assert_eq!(super::touch_menu_scroll_lines(-84.0, 1.0), -2.0);
    assert_eq!(super::touch_menu_scroll_lines(84.0, 1.0), 2.0);
    // Twice the physical displacement at twice the scale is identical.
    assert_eq!(super::touch_menu_scroll_lines(-168.0, 2.0), -2.0);
    assert_eq!(super::touch_menu_scroll_lines(0.0, 2.0), 0.0);
}

#[test]
fn a_finger_resting_at_the_foot_of_a_pedal_presses_nothing() {
    assert_eq!(super::pedal_press(0.0, 0.15), 0.0);
    assert_eq!(super::pedal_press(0.14, 0.15), 0.0);
    assert!(super::pedal_press(0.3, 0.15) < 0.1);
    assert!(super::pedal_press(0.6, 0.15) > 0.3 && super::pedal_press(0.6, 0.15) < 0.6);
    assert_eq!(super::pedal_press(1.0, 0.15), 1.0);
    // no rest zone: from the foot up
    assert_eq!(super::pedal_press(0.0, 0.0), 0.0);
    assert!(super::pedal_press(0.05, 0.0) > 0.0);
}

#[test]
fn the_screen_wheel_turns_as_the_wheel_settings_say() {
    let deg = |s: &Settings| touch_lock_angle(s).to_degrees().round();
    // "Full lock at: OMSI": the wheel's whole rotation is the lock, half of it each way
    assert_eq!(deg(&Settings { wheel_range: 900.0, wheel_lock: 0.0, ..Default::default() }), 450.0);
    assert_eq!(deg(&Settings { wheel_range: 1800.0, wheel_lock: 0.0, ..Default::default() }), 900.0);
    // a lock set of its own wins
    assert_eq!(deg(&Settings { wheel_range: 900.0, wheel_lock: 540.0, ..Default::default() }), 270.0);
}
