//! What another player's game may ask of this one: working a switch of the bus by its
//! `[mouseevent]` name (`app_impl::remote_switches`), and finding the switch of another
//! player's bus under a ray. Apart from `player.rs` so that file does not grow.

use super::*;
use super::mouse::auto_drag_click;

impl Player {
    /// Whether a switch of this name is one of the bus's (or a coupled section's) own
    /// `[mouseevent]`s - what another player may ask this game to press (`press_event`).
    pub(crate) fn has_mouse_event(&self, ev: &str) -> bool {
        self.mouse_event_def(ev).is_some()
    }

    /// The mesh definition (of the bus or one of its sections) that carries the switch `ev`.
    fn mouse_event_def(&self, ev: &str) -> Option<&omsi_model::MeshDef> {
        let named = |d: &&omsi_model::MeshDef| d.mouse_event.as_deref().is_some_and(|m| m.eq_ignore_ascii_case(ev));
        self.vehicle
            .ty
            .model
            .meshes
            .iter()
            .find(named)
            .or_else(|| self.vehicle.trailers.iter().find_map(|t| t.ty.model.meshes.iter().find(named)))
    }

    /// Another player (on foot, at this bus) presses the switch `ev`: what a click on it does
    /// for the player at the wheel, without the ray - the other game found the switch. True
    /// when the script has a trigger for the press itself (`release_event` wants it back).
    pub(crate) fn press_event(&mut self, ev: &str) -> bool {
        log::info!("mouse event {ev} (pressed by another player)");
        let plain = self.vehicle.trigger(ev);
        self.repair_roller_blind(ev);
        // as `click`: a momentary key acts in the frame that follows the press
        self.vehicle.update_scripts_only(0.0);
        self.clear_momentary_mouse_flag(ev);
        plain
    }

    /// The other player moves the mouse with the switch held: `<event>_drag`, as `drag`.
    pub(crate) fn drag_event(&mut self, ev: &str, dx: f32, dy: f32) {
        let name = format!("{ev}_drag");
        self.vehicle.host.mouse = (dx, dy);
        self.vehicle.trigger(&name);
        self.repair_roller_blind(&name);
        self.vehicle.host.mouse = (0.0, 0.0);
    }

    /// The other player lets go of the switch: `<event>_off`, and the click on a control the
    /// scripts only know as a drag, as `release`. `plain` is what `press_event` answered,
    /// `moved` the mouse's way (logical pixels) while it was held.
    pub(crate) fn release_event(&mut self, ev: &str, plain: bool, moved: f32) {
        self.vehicle.trigger(&format!("{ev}_off"));
        let anim = self
            .mouse_event_def(ev)
            .and_then(|d| d.animations.first())
            .map(|a| a.variable.clone())
            .filter(|v| !v.trim().is_empty() && v.trim().parse::<f32>().is_err());
        if let (true, Some(var)) = (auto_drag_click(ev, plain, moved), anim) {
            let now = self.vehicle.var(&var).unwrap_or(0.0);
            let target = if now > 0.5 { 0.0 } else { 1.0 };
            log::info!("mouse event {ev}: a click on a drag control by another player, {var} {now:.2} -> {target}");
            self.auto_drag = Some(AutoDrag { ev: ev.to_string(), var, target, axis: 0, sign: if target > now { -1.0 } else { 1.0 }, last: now, stalled: 0, t: 0.0 });
        }
    }
}

/// The switch of a bus under a ray - the `[mouseevent]` name of the mesh the ray points at,
/// with what is in front of it (a wall seen from outside) taken into account. This is
/// `Player::pick` / `pick_trailer` for a vehicle that is not the player's own: another
/// player's bus, whose switches a player on foot works by telling its driver's game which
/// event was pressed (see `lan_command`).
pub(crate) fn pick_event_in(vehicle: &omsi_sim::VehicleInstance, origin: DVec3, dir: Vec3, spread: f32) -> Option<String> {
    let main = pick_in(vehicle, origin, dir, spread)
        .filter(|&i| !control_hidden_in(vehicle, origin, dir, None, i))
        .and_then(|i| vehicle.ty.model.meshes[vehicle.ty.meshes[i].def_index].mouse_event.clone());
    if main.is_some() {
        return main;
    }
    pick_trailer_in(vehicle, origin, dir, spread)
        .filter(|&(ti, i)| !control_hidden_in(vehicle, origin, dir, Some(ti), i))
        .and_then(|(ti, i)| {
            let t = &vehicle.trailers[ti];
            t.ty.model.meshes[t.ty.meshes[i].def_index].mouse_event.clone()
        })
}

/// Seen from outside: the body of a bus (a wall, a window) is hit before the control mesh
/// `i` (of coupled part `trailer`, or of the bus itself), so it cannot be reached.
pub(super) fn control_hidden_in(vehicle: &omsi_sim::VehicleInstance, origin: DVec3, dir: Vec3, trailer: Option<usize>, i: usize) -> bool {
    let (ty, position, xf) = match trailer {
        None => (&vehicle.ty, vehicle.position, vehicle.mesh_local_transform(i)),
        Some(ti) => {
            let t = &vehicle.trailers[ti];
            (&t.ty, t.position, t.mesh_local_transform(i))
        }
    };
    let Some(&(c, r)) = ty.mesh_bounds.get(i) else {
        return false;
    };
    let dir = dir.normalize_or_zero();
    let o = (origin - position).as_vec3();
    let scale = xf
        .x_axis
        .truncate()
        .length()
        .max(xf.y_axis.truncate().length())
        .max(xf.z_axis.truncate().length());
    let along = (xf.transform_point3(c) - o).dot(dir);
    let nearest = nearest_hits_in(vehicle, origin, dir).0;
    nearest < along - r * scale - 0.1
}

/// The nearest hit of a ray on the bus, and the nearest on a mesh with a mouse event
/// (infinite: none).
pub(super) fn nearest_hits_in(vehicle: &omsi_sim::VehicleInstance, origin: DVec3, dir: Vec3) -> (f32, f32) {
    let mut nearest = f32::INFINITY;
    let mut nearest_control = f32::INFINITY;
    let o = (origin - vehicle.position).as_vec3();
    for (i, mesh) in vehicle.ty.meshes.iter().enumerate() {
        if !vehicle.mesh_props[i].visible { continue; }
        let transform = vehicle.mesh_local_transform(i);
        if !ray_may_hit(&vehicle.ty, i, &transform, o, dir, 0.0) { continue; }
        if let Some(t) = omsi_geometry::ray_mesh(o, dir, &mesh.data, &transform) {
            if t > 0.02 {
                if t < nearest { nearest = t; }
                if vehicle.ty.model.meshes[mesh.def_index].mouse_event.is_some()
                    && t < nearest_control { nearest_control = t; }
            }
        }
    }
    for trailer in &vehicle.trailers {
        let o = (origin - trailer.position).as_vec3();
        for (i, mesh) in trailer.ty.meshes.iter().enumerate() {
            if !trailer.mesh_props[i].visible { continue; }
            let transform = trailer.mesh_local_transform(i);
            if !ray_may_hit(&trailer.ty, i, &transform, o, dir, 0.0) { continue; }
            if let Some(t) = omsi_geometry::ray_mesh(o, dir, &mesh.data, &transform) {
                if t > 0.02 {
                    if t < nearest { nearest = t; }
                    if trailer.ty.model.meshes[mesh.def_index].mouse_event.is_some()
                        && t < nearest_control { nearest_control = t; }
                }
            }
        }
    }
    (nearest, nearest_control)
}
