//! The `impl App` methods of the running game, one module per concern (they grew up in
//! input_script.rs, which keeps the keys and the `OMSI_INPUT` script).

use super::*;

mod actions;
mod clock;
mod coupling;
mod depot;
mod editor_ops;
mod hover;
mod lan_tick;
mod menu_lists;
pub(crate) mod menus;
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
pub(crate) use menus::on_server;
pub(crate) use mouse_grab::{steer_reach as mouse_grab_reach, GrabMode, MouseGrab};
pub(crate) use view::{cab_look_yaw, chase_orbit_step, ease_look, look_key_is, look_key_of, precision_zoom_step, reset_blend, swap_cam_zoom, swap_view_look, ZOOM_INTENT, ZOOM_INTENT_F1};
