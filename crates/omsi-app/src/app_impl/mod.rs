//! The `impl App` methods of the running game, one module per concern (they grew up in
//! input_script.rs, which keeps the keys and the `OMSI_INPUT` script).

use super::*;

mod actions;
mod clock;
mod coupling;
mod depot;
mod editor_ops;
mod hover;
pub(crate) use clock::{real_time_locked, shift_clock, timetable_after_clock_jump, JumpCx, ShiftClockCx};
pub(crate) use hover::{hud_cursor, hud_size, sight_extent};
pub(crate) use vehicles::{placement, PlacementCx};
pub(crate) mod lan_tick;
mod menu_lists;
mod menus;
mod mouse;
mod mouse_grab;
mod saves;
mod shell_link;
mod vehicles;
mod view;
mod voice_tick;
mod weather;

pub(crate) use actions::gate_gear_var;
pub(crate) use menu_lists::dropdown_top_at;
pub(crate) use menus::{on_server, MenuLine};
pub(crate) use mouse_grab::{steer_reach as mouse_grab_reach, GrabMode, MouseGrab};
pub(crate) use view::{cab_look_yaw, chase_orbit_step, ease_look, look_key_is, look_key_of, precision_zoom_step, reset_blend, swap_cam_zoom, swap_view_look, sync_view_look, zoom_view, ZOOM_INTENT, ZOOM_INTENT_F1};
