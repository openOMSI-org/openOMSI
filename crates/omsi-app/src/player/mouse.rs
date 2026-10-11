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
    pub(crate) fn pick(&self, origin: DVec3, dir: Vec3, spread: f32) -> Option<usize> {
        let i = pick_in(&self.vehicle, origin, dir, spread)?;
        if self.occlude_controls && self.control_hidden(origin, dir, None, i) {
            return None;
        }
        Some(i)
    }

    /// The page (`[htmltexture]`) under a ray and where it lands on it: the script texture
    /// index and `u`/`v` from 0 to 1 across the page, `v` down from the top.
    pub(crate) fn html_hit(&self, origin: DVec3, dir: Vec3) -> Option<(usize, f32, f32)> {
        pick_html_in(&self.vehicle, origin, dir)
    }

    /// A press, release or move on a page. What the page does with it (`omsi.setVar`,
    /// `omsi.trigger`) reaches the bus at once.
    pub(crate) fn html_pointer(&mut self, page: usize, u: f32, v: f32, kind: omsi_sim::htmltex::PointerKind) -> bool {
        self.vehicle.html_pointer(page, u, v, kind)
    }

    /// The same forgiving pick as `pick`, for the coupled sections of an articulated bus.
    pub(crate) fn pick_trailer(&self, origin: DVec3, dir: Vec3, spread: f32) -> Option<(usize, usize)> {
        let (ti, i) = pick_trailer_in(&self.vehicle, origin, dir, spread)?;
        if self.occlude_controls && self.control_hidden(origin, dir, Some(ti), i) {
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
        control_hidden_in(&self.vehicle, origin, dir, trailer, i)
    }

    /// The nearest hit of a ray on the bus, and the nearest on a mesh with a mouse event
    /// (infinite: none).
    fn nearest_hits(&self, origin: DVec3, dir: Vec3) -> (f32, f32) {
        nearest_hits_in(&self.vehicle, origin, dir)
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
            self.repair_roller_blind(&ev);
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
        self.repair_roller_blind(&ev);
        self.vehicle.update_scripts_only(0.0);
        self.clear_momentary_mouse_flag(&ev);
        self.pressed_trailer_mesh = Some((ti, i));
        Some(i)
    }

    /// Aachen ibox / ticket-printer style: press sets a flag, `_drag` / `_off` clear it.
    pub(super) fn clear_momentary_mouse_flag(&mut self, ev: &str) {
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
        self.repair_roller_blind(&name);
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
        self.repair_roller_blind(&format!("{ev}_drag"));
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
        if let Some(i) = self.pressed_mesh.take() {
            let ty = self.vehicle.ty.clone();
            let def = &ty.model.meshes[ty.meshes[i].def_index];
            if let Some(ev) = def.mouse_event.clone() {
                self.vehicle.trigger(&format!("{ev}_off"));
                // a plain click on a control the scripts only know as a drag: dragged to its
                // other end (a click opened the NL/NG driver's door by the mouse's jitter
                // and a second click never shut it again)
                let anim = def.animations.first().map(|a| a.variable.clone()).filter(|v| !v.trim().is_empty() && v.trim().parse::<f32>().is_err());
                if let (true, Some(var)) = (auto_drag_click(&ev, self.press_info.0, self.press_info.1), anim) {
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
pub(super) fn auto_drag_click(event: &str, plain: bool, movement: f32) -> bool {
    let event = event.to_ascii_lowercase().replace(['_', '-'], "");
    !plain
        && movement < 4.0
        && !["rollo", "sunblind", "sonnenblende"]
            .iter()
            .any(|name| event.contains(*name))
}

/// Keep the ratchet of the blind the drag script actually moved. Mods number either the
/// blind (`cp_rollo1_pos`) or its position (`cp_rollo_pos1`); the event name alone cannot
/// identify the variable. Retraction already updates the ratchet in the frame script.
pub(super) fn repair_roller_blind(vehicle: &mut omsi_sim::VehicleInstance, event: &str) {
    let event = event.to_ascii_lowercase();
    if !event.contains("rollo") || !event.ends_with("_drag") {
        return;
    }
    let program = &vehicle.ty.program;
    let Some(block) = program.trigger(&event) else {
        return;
    };
    fn walk(
        program: &omsi_script::Program,
        block: omsi_script::BlockId,
        seen: &mut hashbrown::HashSet<omsi_script::BlockId>,
        pairs: &mut Vec<(omsi_script::VarId, omsi_script::VarId)>,
    ) {
        if !seen.insert(block) || seen.len() > 64 {
            return;
        }
        let Some(block) = program.blocks.get(block as usize) else {
            return;
        };
        for op in &block.ops {
            match op {
                omsi_script::Op::Store(pos) => {
                    let Some(name) = program.var_names.get(*pos as usize) else {
                        continue;
                    };
                    let name = name.to_ascii_lowercase();
                    let Some((blind, number)) = name.rsplit_once("_pos") else {
                        continue;
                    };
                    if !blind.contains("rollo") || !number.chars().all(|c| c.is_ascii_digit()) {
                        continue;
                    }
                    let Some(ratchet) = program.var(&format!("{blind}_rastpos{number}")) else {
                        continue;
                    };
                    if !pairs.contains(&(*pos, ratchet)) {
                        pairs.push((*pos, ratchet));
                    }
                }
                omsi_script::Op::Macro(block) => walk(program, *block, seen, pairs),
                _ => {}
            }
        }
    }
    let mut pairs = Vec::new();
    walk(program, block, &mut hashbrown::HashSet::new(), &mut pairs);
    for (pos, ratchet) in pairs {
        vehicle.state.vars[ratchet as usize] = vehicle.state.vars[pos as usize];
    }
}

#[cfg(test)]
mod roller_blind_tests {
    include!("../../../../tools/test-support/original_root.rs");
    use super::{auto_drag_click, repair_roller_blind};
    use crate::schedule::tests::script_test_vehicle;

    fn blinds() -> omsi_sim::VehicleInstance {
        // Synthetic controls with both numbering conventions, deliberately different
        // from their event names. The simple frame stands in for a retaining ratchet.
        script_test_vehicle(
            "{trigger:cp_rollo_drag}\n\
                (L.L.cp_rollo_pos) (L.S.mouse_y) 100 / + (S.L.cp_rollo_pos)\n\
             {end}\n\
             {trigger:cp_rollo1_drag}\n(M.L.side)\n{end}\n\
             {macro:side}\n\
                (L.L.cp_rollo_pos1) (L.S.mouse_y) 100 / + (S.L.cp_rollo_pos1)\n\
             {end}\n\
             {trigger:cp_rollo2_drag}\n\
                (L.L.cp_rollo2_pos) (L.S.mouse_y) 100 / + (S.L.cp_rollo2_pos)\n\
             {end}\n\
             {trigger:window_drag}\n\
                (L.L.window_pos) (L.S.mouse_y) 100 / + (S.L.window_pos)\n\
             {end}\n\
             {frame}\n\
                (L.L.cp_rollo_rastpos) (L.L.cp_rollo_pos) max (S.L.cp_rollo_pos)\n\
                (L.L.cp_rollo_rastpos1) (L.L.cp_rollo_pos1) max (S.L.cp_rollo_pos1)\n\
                (L.L.cp_rollo2_rastpos) (L.L.cp_rollo2_pos) max (S.L.cp_rollo2_pos)\n\
             {end}\n",
            "cp_rollo_pos\ncp_rollo_rastpos\ncp_rollo_pos1\ncp_rollo_rastpos1\n\
             cp_rollo2_pos\ncp_rollo2_rastpos\nwindow_pos\n",
            "",
        )
    }

    fn drag(vehicle: &mut omsi_sim::VehicleInstance, event: &str, dy: f32) {
        vehicle.host.mouse = (0.0, dy);
        assert!(vehicle.trigger(event));
        repair_roller_blind(vehicle, event);
        vehicle.host.mouse = (0.0, 0.0);
    }

    #[test]
    fn numbered_blind_keeps_partial_position_without_moving_another_blind() {
        for (event, pos, ratchet) in [
            ("CP_ROLLO1_DRAG", "cp_rollo_pos1", "cp_rollo_rastpos1"),
            ("cp_rollo2_drag", "cp_rollo2_pos", "cp_rollo2_rastpos"),
        ] {
            let mut vehicle = blinds();
            vehicle.set_var("cp_rollo_pos", 0.4);
            vehicle.set_var("cp_rollo_rastpos", 0.7);
            vehicle.set_var(pos, 0.8);
            vehicle.set_var(ratchet, 0.8);
            drag(&mut vehicle, event, -30.0);
            assert!((vehicle.var(pos).unwrap() - 0.5).abs() < 1e-6);
            assert_eq!(vehicle.var(ratchet), vehicle.var(pos));
            assert_eq!(vehicle.var("cp_rollo_pos"), Some(0.4));
            assert_eq!(vehicle.var("cp_rollo_rastpos"), Some(0.7));
            // Pausing the hand delivers another drag with no movement.
            drag(&mut vehicle, event, 0.0);
            vehicle.update_scripts_only(0.0);
            assert!((vehicle.var(pos).unwrap() - 0.5).abs() < 1e-6);
        }
    }

    #[test]
    fn canonical_blind_still_keeps_its_position() {
        let mut vehicle = blinds();
        vehicle.set_var("cp_rollo_pos", 0.8);
        vehicle.set_var("cp_rollo_rastpos", 0.8);
        vehicle.set_var("cp_rollo_pos1", 0.3);
        vehicle.set_var("cp_rollo_rastpos1", 0.6);
        drag(&mut vehicle, "cp_rollo_drag", -30.0);
        assert_eq!(vehicle.var("cp_rollo_rastpos"), vehicle.var("cp_rollo_pos"));
        assert_eq!(vehicle.var("cp_rollo_rastpos1"), Some(0.6));
        vehicle.update_scripts_only(0.0);
        assert!((vehicle.var("cp_rollo_pos").unwrap() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn retract_and_unrelated_events_leave_ratchets_to_their_scripts() {
        let mut vehicle = blinds();
        vehicle.set_var("cp_rollo_pos", 0.2);
        vehicle.set_var("cp_rollo_rastpos", 0.6);
        vehicle.set_var("cp_rollo_rastpos1", 0.8);
        for event in [
            "cp_rollo_retract",
            "cp_rollo_retract1",
            "cp_rollo1_retract",
            "cp_rollo1_retract_off",
            "cp_rollo1_off",
            "missing_rollo_drag",
        ] {
            repair_roller_blind(&mut vehicle, event);
            assert_eq!(vehicle.var("cp_rollo_rastpos"), Some(0.6));
            assert_eq!(vehicle.var("cp_rollo_rastpos1"), Some(0.8));
        }
        drag(&mut vehicle, "window_drag", 20.0);
        assert_eq!(vehicle.var("window_pos"), Some(0.2));
        assert_eq!(vehicle.var("cp_rollo_rastpos"), Some(0.6));
        assert_eq!(vehicle.var("cp_rollo_rastpos1"), Some(0.8));
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
    fn original_blinds_keep_small_upward_adjustments_after_release() {
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
                for &(_, other_pos, other_ratchet) in blinds {
                    assert!(vehicle.set_var(other_pos, 0.3));
                    assert!(vehicle.set_var(other_ratchet, 0.3));
                }
                assert!(vehicle.set_var(pos, 0.6));
                assert!(vehicle.set_var(ratchet, 0.6));
                let event = format!("{blind}_drag");
                drag(&mut vehicle, &event, -2.0);
                let adjusted = vehicle.var(pos).unwrap();
                assert!(adjusted < 0.6 && adjusted > 0.59, "{path}: {blind}");
                drag(&mut vehicle, &event, 0.0);
                assert!(vehicle.trigger(&format!("{blind}_off")));
                for _ in 0..60 {
                    vehicle
                        .vm
                        .run_frame(&program, &mut vehicle.state, &mut vehicle.host);
                }
                assert!(
                    (vehicle.var(pos).unwrap() - adjusted).abs() < 1e-6,
                    "{path}: {blind}"
                );
                assert!(
                    (vehicle.var(ratchet).unwrap() - adjusted).abs() < 1e-6,
                    "{path}: {blind}"
                );
                for &(other, other_pos, other_ratchet) in blinds {
                    if other != blind {
                        assert!((vehicle.var(other_pos).unwrap() - 0.3).abs() < 1e-6);
                        assert!((vehicle.var(other_ratchet).unwrap() - 0.3).abs() < 1e-6);
                    }
                }
                assert!(!auto_drag_click(blind, false, 2.0));
            }
        }
    }
}

/// See `Player::auto_drag`.
#[derive(Debug, Clone)]
pub(crate) struct AutoDrag {
    pub(super) ev: String,
    pub(super) var: String,
    pub(super) target: f32,
    /// The mouse axis (0 x, 1 y) and direction that move it towards the target.
    pub(super) axis: usize,
    pub(super) sign: f32,
    pub(super) last: f32,
    pub(super) stalled: u32,
    pub(super) t: f32,
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
        self.repair_roller_blind(&format!("{}_drag", a.ev));
        self.vehicle.host.mouse = (0.0, 0.0);
        if exists {
            self.auto_drag = Some(a);
        }
    }
}
