//! The `impl App` methods of the running game, one module per concern (they grew up in
//! input_script.rs, which keeps the keys and the `OMSI_INPUT` script).

use super::*;

mod actions;
mod clock;
mod coupling;
mod depot;
mod editor_ops;
mod hover;
pub(crate) use clock::{
    JumpCx, ShiftClockCx, real_time_locked, restart_timetable, shift_clock, time_speed,
    timetable_after_clock_jump,
};
pub(crate) use hover::{hud_cursor, hud_size, sight_extent, triple_rig};
pub(crate) use vehicles::{PlacementCx, placement, placement_args};
mod accident;
pub(crate) mod lan_duties;
pub(crate) mod lan_tick;
mod menu_lists;
mod menus;
mod mouse;
mod mouse_grab;
mod quick_actions;
mod room_world;
pub(crate) mod rooms;
mod saves;
mod shell_link;
mod vehicles;
mod view;
mod voice_tick;
mod weather;

pub(crate) use actions::gate_gear_var;
pub(crate) use menu_lists::dropdown_top_at;
pub(crate) use menus::{GAME_MENU, MenuLine, on_server};
pub(crate) use mouse_grab::{GrabMode, MouseGrab, steer_reach as mouse_grab_reach};
pub(crate) use view::{
    ZOOM_INTENT, ZOOM_INTENT_F1, ZoomHold, cab_look_yaw, chase_orbit_step, drawn_fov, ease_look,
    look_key_is, look_key_of, precision_zoom_step, reset_blend, swap_cam_zoom, swap_view_look,
    sync_view_look, with_zoom_hold, zoom_view,
};
