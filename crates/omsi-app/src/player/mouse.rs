//! The mouse in the cab: the switch under the cursor, its click, drag and wheel, and the
//! drag-only controls (sun blinds) driven on by themselves.

use super::*;

impl Player {
    /// The switch under a ray, if any: the `[mouseevent]` mesh the cursor points at.
    ///
    /// The ray through the cursor decides on its own wherever it hits something; only when
    /// it hits nothing are rings of rays around it tried, so that a switch a couple of
    /// pixels wide can still be caught. Letting the ring win over the middle ray (which is
    /// what happened before) meant a switch standing slightly closer than the one actually
    /// under the cursor took the click - the neighbouring toggle flipped instead.
    ///
    /// `spread` is the half-angle of those rings in radians; the window passes the angle
    /// six pixels subtend, so aiming is equally forgiving at any resolution.
    ///
    /// From outside, a switch behind another part of the bus cannot be reached through it
    /// ([`Self::reach`]); from the cab nothing hides one, as in Omsi.exe.
    pub(crate) fn pick(&self, origin: DVec3, dir: Vec3, spread: f32) -> Option<usize> {
        let (i, centre) = pick_in(&self.vehicle, origin, dir, spread, self.reach(origin, dir))?;
        if !centre && self.occlude_controls && self.control_hidden(origin, dir, None, i) {
            return None;
        }
        Some(i)
    }

    /// How far along a ray a switch of the bus or its coupled parts can be reached: from
    /// outside (`occlude_controls`) up to the nearest part, walls and windows alike; from the
    /// cab without limit (see [`reach_into`]).
    fn reach(&self, origin: DVec3, dir: Vec3) -> Reach {
        let (dir, outside) = (dir.normalize_or_zero(), self.occlude_controls);
        let v = &self.vehicle;
        let mut reach = reach_into(&v.ty, &v.mesh_props, |i| v.mesh_local_transform(i), (origin - v.position).as_vec3(), dir, outside);
        for tr in &v.trailers {
            let part = reach_into(&tr.ty, &tr.mesh_props, |i| tr.mesh_local_transform(i), (origin - tr.position).as_vec3(), dir, outside);
            reach = Reach(reach.0.min(part.0));
        }
        reach
    }

    /// The page (`[htmltexture]`, or a script texture an `.opl` plugin draws) under a ray and
    /// where it lands on it: the script texture index and `u`/`v` from 0 to 1, `v` down.
    pub(crate) fn html_hit(&self, origin: DVec3, dir: Vec3, plugins: &omsi_plugin::TexturePointer) -> Option<(usize, f32, f32)> {
        pick_html_in(&self.vehicle, origin, dir, &plugins.pages)
    }

    /// A press, release or move on a page. What the page does with it (`omsi.setVar`,
    /// `omsi.trigger`) reaches the bus at once; on a plugin's texture it waits for the plugin.
    pub(crate) fn html_pointer(&mut self, page: usize, u: f32, v: f32, kind: omsi_sim::htmltex::PointerKind, plugins: &mut omsi_plugin::TexturePointer) -> bool {
        use omsi_sim::htmltex::PointerKind as K;
        let k = match kind { K::Down => omsi_plugin::POINTER_DOWN, K::Move => omsi_plugin::POINTER_MOVE, K::Up => omsi_plugin::POINTER_UP };
        plugins.push(page, k, u, v) || self.vehicle.html_pointer(page, u, v, kind)
    }

    /// The same forgiving pick as `pick`, for the coupled sections of an articulated bus.
    pub(crate) fn pick_trailer(&self, origin: DVec3, dir: Vec3, spread: f32) -> Option<(usize, usize)> {
        let ((ti, i), centre) = pick_trailer_in(&self.vehicle, origin, dir, spread, self.reach(origin, dir))?;
        if !centre && self.occlude_controls && self.control_hidden(origin, dir, Some(ti), i) {
            return None;
        }
        Some((ti, i))
    }

    /// Exact surface under a VR pointer, including meshes without a mouse event.
    /// This runs when the mouse moves, not for every headset frame.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) fn surface_hit(&self, origin: DVec3, dir: Vec3) -> Option<DVec3> {
        let (mut nearest, nearest_control) = self.nearest_hits(origin, dir);
        if nearest_control.is_finite() { nearest = nearest_control; }
        (nearest.is_finite() && nearest < 8.0)
            .then(|| origin + (dir * nearest).as_dvec3())
    }

    /// How far along a ray the bus (any visible mesh, trailers too) is.
    pub(crate) fn body_hit(&self, origin: DVec3, dir: Vec3) -> Option<f32> {
        Some(self.nearest_hits(origin, dir).0).filter(|t| t.is_finite())
    }

    /// Whether a mesh of the vehicle is glass / transparent (windows, windshield).
    pub(crate) fn is_mesh_glass(&self, trailer: Option<usize>, i: usize) -> bool {
        let (ty, def_idx) = match trailer {
            None => (&self.vehicle.ty, self.vehicle.ty.meshes[i].def_index),
            Some(ti) => (&self.vehicle.trailers[ti].ty, self.vehicle.trailers[ti].ty.meshes[i].def_index),
        };
        let def = &ty.model.meshes[def_idx];
        let file_low = def.file.to_ascii_lowercase();
        if file_low.contains("glas") || file_low.contains("scheibe") || file_low.contains("window") || file_low.contains("fenster") {
            return true;
        }
        def.materials.iter().any(|m| m.alpha == 2)
    }

    /// How far along a ray the nearest opaque (non-glass) body part of the vehicle is.
    pub(crate) fn opaque_body_hit(&self, origin: DVec3, dir: Vec3) -> Option<f32> {
        let mut nearest = f32::INFINITY;
        let o = (origin - self.vehicle.position).as_vec3();
        for (i, mesh) in self.vehicle.ty.meshes.iter().enumerate() {
            if !self.vehicle.mesh_props[i].visible || self.is_mesh_glass(None, i) { continue; }
            let transform = self.vehicle.mesh_local_transform(i);
            if !ray_may_hit(&self.vehicle.ty, i, &transform, o, dir, 0.0) { continue; }
            if let Some(t) = omsi_geometry::ray_mesh(o, dir, &mesh.data, &transform) {
                if t > 0.02 && t < nearest { nearest = t; }
            }
        }
        for (ti, trailer) in self.vehicle.trailers.iter().enumerate() {
            let o = (origin - trailer.position).as_vec3();
            for (i, mesh) in trailer.ty.meshes.iter().enumerate() {
                if !trailer.mesh_props[i].visible || self.is_mesh_glass(Some(ti), i) { continue; }
                let transform = trailer.mesh_local_transform(i);
                if !ray_may_hit(&trailer.ty, i, &transform, o, dir, 0.0) { continue; }
                if let Some(t) = omsi_geometry::ray_mesh(o, dir, &mesh.data, &transform) {
                    if t > 0.02 && t < nearest { nearest = t; }
                }
            }
        }
        (nearest.is_finite()).then_some(nearest)
    }

    /// Seen from outside: the body of the bus (a wall, a window) is hit before the control
    /// mesh `i` (of coupled part `trailer`, or of the bus itself), so it cannot be reached.
    fn control_hidden(&self, origin: DVec3, dir: Vec3, trailer: Option<usize>, i: usize) -> bool {
        let (ty, position, xf) = match trailer {
            None => (&self.vehicle.ty, self.vehicle.position, self.vehicle.mesh_local_transform(i)),
            Some(ti) => {
                let t = &self.vehicle.trailers[ti];
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
        let nearest = self.nearest_hits(origin, dir).0;
        nearest < along - r * scale - 0.1
    }

    /// The nearest hit of a ray on the bus, and the nearest on a mesh with a mouse event
    /// (infinite: none).
    fn nearest_hits(&self, origin: DVec3, dir: Vec3) -> (f32, f32) {
        let mut nearest = f32::INFINITY;
        let mut nearest_control = f32::INFINITY;
        let vehicle = &self.vehicle;
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

    /// The part of the bus under a ray, switch or not: `(name, operable)`. Without this the
    /// HUD stayed empty over everything that is not a switch, and there was no way to tell
    /// "this is not a control" from "the cursor is not hitting anything".
    /// The flag beside it: a `[mouseevent]` mesh is under the ray, named or not - Omsi.exe
    /// shows the hand cursor over any of them (0x6f34c0 @0x6f45b4).
    pub(crate) fn hovered_part(&self, origin: DVec3, dir: Vec3, spread: f32) -> (Option<(String, bool)>, bool) {
        if let Some(i) = self.pick(origin, dir, spread) {
            let def = &self.vehicle.ty.model.meshes[self.vehicle.ty.meshes[i].def_index];
            if let Some(ev) = def.mouse_event.clone() {
                // a whole panel that can be dragged into place (the VDV dashboard's
                // `VDV_position`) is no switch to name: its name covered the whole cockpit
                // (the hand still shows over it, a door leaf or the steering column)
                let big = self.vehicle.ty.mesh_bounds.get(i).map(|b| b.1 > 0.45).unwrap_or(false);
                if big {
                    return (None, true);
                }
                return (Some((ev, true)), true);
            }
        }
        if let Some((ti, i)) = self.pick_trailer(origin, dir, spread) {
            let trailer = &self.vehicle.trailers[ti];
            let def = &trailer.ty.model.meshes[trailer.ty.meshes[i].def_index];
            if let Some(ev) = def.mouse_event.clone() {
                return (Some((ev, true)), true);
            }
        }
        (self.hovered_body_part(origin, dir), false)
    }

    /// The mesh of the bus under a ray when it is no switch, by its file's name.
    fn hovered_body_part(&self, origin: DVec3, dir: Vec3) -> Option<(String, bool)> {
        let o = (origin - self.vehicle.position).as_vec3();
        let mut best: Option<(f32, usize)> = None;
        for (i, vm) in self.vehicle.ty.meshes.iter().enumerate() {
            if !self.vehicle.mesh_props[i].visible {
                continue;
            }
            let xf = self.vehicle.mesh_local_transform(i);
            if !ray_may_hit(&self.vehicle.ty, i, &xf, o, dir, 0.0) {
                continue;
            }
            if let Some(t) = omsi_geometry::ray_mesh(o, dir, &vm.data, &xf) {
                if best.map(|(bt, _)| t < bt).unwrap_or(true) {
                    best = Some((t, i));
                }
            }
        }
        let (_, i) = best?;
        let def = &self.vehicle.ty.model.meshes[self.vehicle.ty.meshes[i].def_index];
        let name = std::path::Path::new(&def.file.replace('\\', "/"))
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| def.file.clone());
        Some((name, false))
    }

    pub(crate) fn click(&mut self, origin: DVec3, dir: Vec3, spread: f32) -> Option<usize> {
        self.release();
        if let Some(i) = self.pick(origin, dir, spread) {
            let ev = self.vehicle.ty.model.meshes[self.vehicle.ty.meshes[i].def_index]
                .mouse_event
                .clone()
                .unwrap();
            log::info!("mouse event {ev}");
            let plain = self.vehicle.trigger(&ev);
            // Momentary `[mouseevent]` buttons (Aachen ibox / ticket printer, #744) set a
            // flag in the press trigger and clear it in `_drag` / `_off`. The frame script
            // is what acts on the flag. Run that frame now, before the next redraw's
            // `_drag` (or a quick `_off`) zeroes it and the click does nothing.
            self.vehicle.update_scripts_only(0.0);
            // Clear the press flag immediately after the frame consumed it. Otherwise a
            // held click can be processed again on the next `tick` (digit keys append
            // twice: `1`→`11`, and ibox `0`/`D11` becomes `1`→`10`→`100`).
            self.clear_momentary_mouse_flag(&ev);
            self.pressed_mesh = Some(i);
            self.press_info = (plain, 0.0);
            self.auto_drag = None;
            return Some(i);
        }
        let (ti, i) = self.pick_trailer(origin, dir, spread)?;
        let ev = self.vehicle.trailers[ti].ty.model.meshes
            [self.vehicle.trailers[ti].ty.meshes[i].def_index]
            .mouse_event
            .clone()
            .unwrap();
        log::info!("trailer mouse event {ev} (part {ti})");
        self.vehicle.trigger(&ev);
        self.vehicle.update_scripts_only(0.0);
        self.clear_momentary_mouse_flag(&ev);
        self.pressed_trailer_mesh = Some((ti, i));
        Some(i)
    }

    /// Aachen ibox / ticket-printer style: press sets a flag, `_drag` / `_off` clear it.
    fn clear_momentary_mouse_flag(&mut self, ev: &str) {
        let drag = format!("{ev}_drag");
        if self.vehicle.ty.program.trigger(&drag).is_none()
            && self
                .vehicle
                .trailers
                .iter()
                .all(|t| t.ty.program.trigger(&drag).is_none())
        {
            return;
        }
        let low = ev.to_ascii_lowercase();
        // Only flag-style momentary keys (ibox `*_taste_*`, ticket printer). Rotary knobs
        // also have `_drag` but use mouse deltas; a zero `_drag` right after press would
        // disturb them.
        if !(low.contains("taste") || low.contains("ibox") || low.contains("ticket")) {
            return;
        }
        self.vehicle.host.mouse = (0.0, 0.0);
        self.vehicle.trigger(&drag);
        self.vehicle.host.mouse = (0.0, 0.0);
    }

    /// Mouse moved with the button down on a switch: OMSI fires `<event>_drag` with the
    /// movement in `mouse_x` / `mouse_y`, which is how the rotary knobs, the sun blind, the
    /// driver's window and the ignition key are operated.
    pub(crate) fn drag(&mut self, dx: f32, dy: f32) {
        let (ev, trailer) = if let Some(i) = self.pressed_mesh {
            let Some(ev) = self.vehicle.ty.model.meshes[self.vehicle.ty.meshes[i].def_index]
                .mouse_event
                .clone()
            else {
                return;
            };
            (ev, false)
        } else if let Some((ti, i)) = self.pressed_trailer_mesh {
            let Some(ev) = self.vehicle.trailers[ti].ty.model.meshes
                [self.vehicle.trailers[ti].ty.meshes[i].def_index]
                .mouse_event
                .clone()
            else {
                return;
            };
            (ev, true)
        } else {
            return;
        };
        let name = format!("{ev}_drag");
        self.press_info.1 += dx.abs() + dy.abs();
        self.vehicle.host.mouse = (dx, dy);
        self.vehicle.trigger(&name);
        self.vehicle.host.mouse = (0.0, 0.0);
        let _ = trailer;
    }

    /// One notch of the mouse wheel over a switch: `<event>_drag` with the notch as the
    /// movement, and the `_off` the script expects when the hand lets go again.
    pub(crate) fn wheel(&mut self, origin: DVec3, dir: Vec3, spread: f32, amount: f32) {
        let Some(i) = self.pick(origin, dir, spread) else {
            return;
        };
        let Some(ev) = self.vehicle.ty.model.meshes[self.vehicle.ty.meshes[i].def_index]
            .mouse_event
            .clone()
        else {
            return;
        };
        // vertical, like the wheel itself: that is the axis the knobs, the sun blind and
        // the ignition key read (mouse_x belongs to the driver's window and the light
        // rotary, which stay a drag)
        self.vehicle.host.mouse = (0.0, amount);
        self.vehicle.trigger(&format!("{ev}_drag"));
        self.vehicle.host.mouse = (0.0, 0.0);
        self.vehicle.trigger(&format!("{ev}_off"));
    }

    /// The left button let go while the right one is held: Omsi.exe sends no `_off`, so a
    /// momentary switch stays where the hand left it (a pedal held down for the steering
    /// column's adjustment, #769) until it is clicked and let go again.
    pub(crate) fn release_keeping(&mut self) {
        if let Some(i) = self.pressed_mesh.take() {
            let def = &self.vehicle.ty.model.meshes[self.vehicle.ty.meshes[i].def_index];
            log::info!("mouse event {:?}: let go with the right button held, the switch stays", def.mouse_event);
        }
        self.pressed_trailer_mesh = None;
    }

    pub(crate) fn release(&mut self) {
        self.release_mouse(true);
    }

    /// Precise headset manipulation must stop at the current value, including a
    /// tiny drag or no movement. Ordinary press/release button triggers still run.
    pub(crate) fn release_precise(&mut self) {
        self.release_mouse(false);
        self.cancel_auto_drag();
    }

    pub(crate) fn click_precise(&mut self, origin: DVec3, dir: Vec3, spread: f32) -> Option<usize> {
        self.release_precise();
        self.click(origin, dir, spread)
    }

    pub(crate) fn wheel_precise(&mut self, origin: DVec3, dir: Vec3, spread: f32, amount: f32) {
        self.cancel_auto_drag();
        self.wheel(origin, dir, spread, amount);
    }

    pub(crate) fn release_for_menu(&mut self, vr_active: bool) {
        if vr_active { self.release_precise(); } else { self.release(); }
    }

    fn cancel_auto_drag(&mut self) {
        if let Some(drag) = self.auto_drag.take() {
            self.vehicle.host.mouse = (0.0, 0.0);
            self.vehicle.trigger(&format!("{}_off", drag.ev));
        }
    }

    fn release_mouse(&mut self, allow_auto_drag: bool) {
        if let Some(i) = self.pressed_mesh.take() {
            let ty = self.vehicle.ty.clone();
            let def = &ty.model.meshes[ty.meshes[i].def_index];
            if let Some(ev) = def.mouse_event.clone() {
                self.vehicle.trigger(&format!("{ev}_off"));
                // a plain click on a control the scripts only know as a drag: dragged to its
                // other end (a click opened the NL/NG driver's door by the mouse's jitter
                // and a second click never shut it again)
                let anim = def.animations.first().map(|a| a.variable.clone()).filter(|v| !v.trim().is_empty() && v.trim().parse::<f32>().is_err());
                if let (true, Some(var)) = (
                    allow_auto_drag && auto_drag_click(&ev, self.press_info.0, self.press_info.1), anim) {
                    let now = self.vehicle.var(&var).unwrap_or(0.0);
                    let target = if now > 0.5 { 0.0 } else { 1.0 };
                    log::info!("mouse event {ev}: a click on a drag control, {var} {now:.2} -> {target}");
                    self.auto_drag = Some(AutoDrag { ev, var, target, axis: 0, sign: if target > now { -1.0 } else { 1.0 }, last: now, stalled: 0, t: 0.0 });
                }
            }
        }
        if let Some((ti, i)) = self.pressed_trailer_mesh.take() {
            if let Some(ev) = self.vehicle.trailers[ti].ty.model.meshes
                [self.vehicle.trailers[ti].ty.meshes[i].def_index]
                .mouse_event
                .clone()
            {
                self.vehicle.trigger(&format!("{ev}_off"));
            }
        }
    }
}

/// A sun blind can be left at any position, including after a very short drag. Treating
/// its drag-only click spot as a toggle turned a small adjustment into full travel.
fn auto_drag_click(event: &str, plain: bool, movement: f32) -> bool {
    let event = event.to_ascii_lowercase().replace(['_', '-'], "");
    !plain
        && movement < 4.0
        && !["rollo", "sunblind", "sonnenblende"]
            .iter()
            .any(|name| event.contains(*name))
}

#[cfg(test)]
mod roller_blind_tests {
    include!("../../../../tools/test-support/original_root.rs");
    use super::auto_drag_click;
    use crate::schedule::tests::script_test_vehicle;

    /// A blind as the stock cab scripts build it: the drag moves it while the hand holds
    /// it, the ratchet (`rastpos`) remembers the lowest point, and once let go the frame
    /// pulls it back down to that point; only the cord (`retract`) lets it roll up.
    fn blind() -> omsi_sim::VehicleInstance {
        script_test_vehicle(
            "{trigger:cp_rollo_drag}\n\
                1 (S.L.cp_rollo_dragged)\n\
                (L.L.cp_rollo_pos) (L.S.mouse_y) 100 / + 0 max 1 min (S.L.cp_rollo_pos)\n\
             {end}\n\
             {trigger:cp_rollo_off}\n0 (S.L.cp_rollo_dragged)\n{end}\n\
             {trigger:cp_rollo_retract}\n1 (S.L.cp_rollo_retract)\n{end}\n\
             {frame}\n\
                (L.L.cp_rollo_pos) (L.L.cp_rollo_rastpos) max (S.L.cp_rollo_rastpos)\n\
                (L.L.cp_rollo_dragged) !\n\
                {if}\n\
                    (L.L.cp_rollo_retract)\n\
                    {if}\n\
                        0 (S.L.cp_rollo_pos) (S.L.cp_rollo_rastpos) (S.L.cp_rollo_retract)\n\
                    {else}\n\
                        (L.L.cp_rollo_rastpos) (S.L.cp_rollo_pos)\n\
                    {endif}\n\
                {endif}\n\
             {end}\n",
            "cp_rollo_pos\ncp_rollo_rastpos\ncp_rollo_dragged\ncp_rollo_retract\n",
            "",
        )
    }

    fn drag(vehicle: &mut omsi_sim::VehicleInstance, event: &str, dy: f32) {
        vehicle.host.mouse = (0.0, dy);
        assert!(vehicle.trigger(event));
        vehicle.host.mouse = (0.0, 0.0);
    }

    #[test]
    fn a_blind_pushed_up_by_hand_falls_back_to_its_ratchet() {
        let mut vehicle = blind();
        drag(&mut vehicle, "cp_rollo_drag", 80.0);
        assert!(vehicle.trigger("cp_rollo_off"));
        vehicle.update_scripts_only(0.0);
        assert!((vehicle.var("cp_rollo_rastpos").unwrap() - 0.8).abs() < 1e-6);
        // up by hand: it follows while held...
        drag(&mut vehicle, "cp_rollo_drag", -50.0);
        vehicle.update_scripts_only(0.0);
        assert!((vehicle.var("cp_rollo_pos").unwrap() - 0.3).abs() < 1e-6);
        // ...and drops back once let go, the ratchet untouched by the drag (#2146)
        assert!(vehicle.trigger("cp_rollo_off"));
        vehicle.update_scripts_only(0.0);
        assert!((vehicle.var("cp_rollo_pos").unwrap() - 0.8).abs() < 1e-6);
        // the cord rolls it up
        assert!(vehicle.trigger("cp_rollo_retract"));
        vehicle.update_scripts_only(0.0);
        assert_eq!(vehicle.var("cp_rollo_pos"), Some(0.0));
    }

    #[test]
    fn a_short_blind_gesture_does_not_become_full_travel() {
        for event in [
            "cp_rollo",
            "cp_rollo1",
            "CP_ROLLO2",
            "cp_sun_blind",
            "cp_Sun-Blind1-1",
            "C2_CP_Sonnenblende_Seite",
        ] {
            assert!(!auto_drag_click(event, false, 0.0));
            assert!(!auto_drag_click(event, false, 3.9));
        }
        assert!(auto_drag_click("cp_Fahrertuer", false, 0.0));
        assert!(!auto_drag_click("cp_Fahrertuer", false, 4.0));
        assert!(!auto_drag_click("cp_Fahrertuer", true, 0.0));
        assert!(auto_drag_click("C2_CP_Stop_Blind", false, 0.0));
    }

    #[test]
    #[ignore = "needs the original OMSI 2 install (OMSI_ROOT)"]
    fn original_blinds_go_up_only_by_their_cord() {
        let root = original_root();
        let buses: [(&str, &[(&str, &str, &str)]); 3] = [
            (
                "Vehicles/Urbino_II/SU_18_V.bus",
                &[
                    ("cp_rollo1", "cp_rollo_pos1", "cp_rollo_rastpos1"),
                    ("cp_rollo2", "cp_rollo_pos2", "cp_rollo_rastpos2"),
                ],
            ),
            (
                "Vehicles/Solaris BVG/Urbino 18 Main.bus",
                &[
                    ("cp_rollo", "cp_rollo_pos", "cp_rollo_rastpos"),
                    ("cp_rollo1", "cp_rollo1_pos", "cp_rollo1_rastpos"),
                ],
            ),
            (
                "Vehicles/MAN_NL_NG/MAN_EN92_main.bus",
                &[("cp_rollo", "cp_rollo_pos", "cp_rollo_rastpos")],
            ),
        ];
        for (path, blinds) in buses {
            let bus = root.join(path);
            if !bus.exists() {
                eprintln!("skipped: no {}", bus.display());
                continue;
            }
            let ty = std::sync::Arc::new(omsi_sim::VehicleType::load(&root, &bus).expect(path));
            let mut vehicle = omsi_sim::VehicleInstance::new(
                ty,
                omsi_sim::VehicleHost::new(Default::default()),
            );
            vehicle.host.clock.timegap = 1.0 / 60.0;
            let program = vehicle.ty.program.clone();
            for &(blind, pos, ratchet) in blinds {
                assert!(vehicle.set_var(pos, 0.6));
                assert!(vehicle.set_var(ratchet, 0.6));
                let event = format!("{blind}_drag");
                drag(&mut vehicle, &event, -100.0);
                assert!(vehicle.var(pos).unwrap() < 0.59, "{path}: {blind} follows the hand");
                assert!(vehicle.trigger(&format!("{blind}_off")));
                for _ in 0..120 {
                    vehicle.vm.run_frame(&program, &mut vehicle.state, &mut vehicle.host);
                }
                assert!((vehicle.var(pos).unwrap() - 0.6).abs() < 1e-3, "{path}: {blind} falls back");
                assert!((vehicle.var(ratchet).unwrap() - 0.6).abs() < 1e-6, "{path}: {blind}");
            }
        }
    }
}

/// See `Player::auto_drag`.
#[derive(Debug, Clone)]
pub(crate) struct AutoDrag {
    ev: String,
    var: String,
    target: f32,
    /// The mouse axis (0 x, 1 y) and direction that move it towards the target.
    axis: usize,
    sign: f32,
    last: f32,
    stalled: u32,
    t: f32,
}

impl Player {
    /// Drag a clicked drag-only control (see `auto_drag`) a step towards its other end.
    pub(crate) fn tick_auto_drag(&mut self, dt: f32) {
        let Some(mut a) = self.auto_drag.take() else { return };
        let now = self.vehicle.var(&a.var).unwrap_or(a.last);
        a.t += dt;
        if (now - a.target).abs() < 0.03 || a.t > 2.5 {
            self.vehicle.trigger(&format!("{}_off", a.ev));
            return;
        }
        // moved away from the target, or not at all: another direction, then the other axis
        let towards = (a.target - a.last).signum();
        let moved = now - a.last;
        if a.t > 0.05 {
            if moved * towards < -1e-4 {
                a.sign = -a.sign;
            } else if moved.abs() < 1e-4 {
                a.stalled += 1;
                if a.stalled == 3 {
                    a.sign = -a.sign;
                } else if a.stalled == 6 {
                    a.axis = 1 - a.axis;
                } else if a.stalled == 9 {
                    a.sign = -a.sign;
                } else if a.stalled > 12 {
                    self.vehicle.trigger(&format!("{}_off", a.ev));
                    return;
                }
            } else {
                a.stalled = 0;
            }
        }
        a.last = now;
        let step = 900.0 * dt.min(0.05) * a.sign;
        self.vehicle.host.mouse = if a.axis == 0 { (step, 0.0) } else { (0.0, step) };
        let exists = self.vehicle.trigger(&format!("{}_drag", a.ev));
        self.vehicle.host.mouse = (0.0, 0.0);
        if exists {
            self.auto_drag = Some(a);
        }
    }
}
