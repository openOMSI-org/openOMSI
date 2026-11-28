use super::*;
use clap::Parser;

fn app() -> App {
    crate::assemble_app(crate::Args::parse_from(["openomsi"]), Default::default())
}

#[test]
fn headset_mouse_moves_reach_the_held_control() {
    let mut app = app();
    app.input.cursor = (400.0, 300.0);
    app.input.dragging = true;
    app.xr.vr_cursor_physical = Some((100.0, 100.0));
    app.on_vr_cursor_moved(102.0, 97.0);
    assert_eq!(app.input.cursor, (402.0, 297.0));
    assert_eq!(app.input.drag_delta, (2.0, -3.0));
    // Multiple moves before the frame are accumulated, never replaced.
    app.on_vr_cursor_moved(103.0, 98.0);
    assert_eq!(app.input.drag_delta, (3.0, -2.0));
}

#[test]
fn cursor_warp_does_not_turn_the_control_or_move_the_virtual_pointer() {
    let mut app = app();
    app.input.cursor = (400.0, 300.0);
    app.input.dragging = true;
    app.xr.vr_cursor_physical = Some((990.0, 700.0));
    app.xr.vr_cursor_warp_pending = Some((500.0, 400.0));
    app.on_vr_cursor_moved(500.0, 400.0);
    assert_eq!(app.input.cursor, (400.0, 300.0));
    assert_eq!(app.input.drag_delta, (0.0, 0.0));
    app.on_vr_cursor_moved(503.0, 398.0);
    assert_eq!(app.input.drag_delta, (3.0, -2.0));
    app.input.dragging = false;
    app.on_vr_cursor_moved(506.0, 399.0);
    assert_eq!(app.input.drag_delta, (3.0, -2.0));
}
