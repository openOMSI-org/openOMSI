//! The bus's cameras and its meshes put where its state says.

use super::*;

impl Player {
    /// `inside`: the camera is one of the vehicle's interior cameras. `[viewpoint]` bits:
    /// 1 = visible from outside, 2 = visible from inside, 4 = visible on AI vehicles; 0 = always.
    /// `rain_quality`: the rain's (see `rain::quality_of`), for the films on the panes.
    pub(crate) fn sync_transforms(&mut self, renderer: &Renderer, scene: &mut Scene, inside: bool, rain_quality: u8) -> f64 {
        sync_vehicle_transforms(renderer, scene, &mut self.vehicle, &mut self.render, &mut self.trailer_renders, inside);
        if self.render.window_wipers.is_none() {
            self.render.window_wipers = Some(crate::window_wipers::WindowWipers::new(renderer, scene, &self.vehicle, &self.render));
        }
        let wipers = self.render.window_wipers.as_mut().unwrap();
        let start = std::time::Instant::now();
        wipers.update(renderer, scene, &self.vehicle, &self.render.instances, rain_quality);
        start.elapsed().as_secs_f64()
    }

    pub(crate) fn finish_window_paints(&mut self, renderer: &Renderer, scene: &mut Scene) {
        if let Some(wipers) = self.render.window_wipers.as_mut() {
            wipers.finish_paints(renderer, scene);
        }
    }

    /// Pose and place the driver at the wheel; `show` false hides the figure (the `driver`
    /// setting off), `mirror_only` keeps it to the mirrors (the cab view).
    pub(crate) fn sync_driver(&mut self, renderer: &Renderer, scene: &mut Scene, dt: f32, show: bool, mirror_only: bool) {
        self.sync_driver_hands(renderer, scene, dt, show, mirror_only, false, false);
    }

    /// As [`Player::sync_driver`], with the driver's hands shown in the cab view or not
    /// (Settings → "Driver's hands in the cab view").
    /// `like_omsi` sits him as OMSI 2 does, back on the seat with his hands off the wheel (Settings →
    /// "Driver sits as in OMSI 2").
    pub(crate) fn sync_driver_hands(&mut self, renderer: &Renderer, scene: &mut Scene, dt: f32, show: bool, mirror_only: bool, hands: bool, like_omsi: bool) {
        if let Some(d) = self.driver.as_mut() {
            d.show_hands_in_cab = hands;
            d.set_original_pose(like_omsi);
            d.update(renderer, scene, &self.vehicle, &self.render, dt, show, mirror_only);
        }
    }

    /// The driver's chosen camera as it is fixed in the bus (before the bus's own motion), turned
    /// by the look and the steering: what a glide between two cameras mixes.
    pub(crate) fn driver_local(&self, look: (f32, f32)) -> Option<omsi_vehicle::Camera> {
        // (no glide onto a coupled part's camera: it is not in the front's frame)
        if self.trailer_driver_camera().is_some() {
            return None;
        }
        let def = &self.vehicle.ty.def;
        let c = def.cameras_driver.get(self.driver_camera_slot()).or(def.cameras_driver.first())?;
        Some(omsi_vehicle::Camera {
            yaw: c.yaw + look.0 + self.steer_look,
            pitch: (c.pitch + look.1).clamp(-89.0, 89.0),
            ..c.clone()
        })
    }

    /// `driver_local`'s camera in the world, with the bus's pitch and bank and the head on it
    /// (the same as `camera_look` makes of the driver's camera).
    pub(crate) fn driver_world(&self, turned: &omsi_vehicle::Camera) -> Camera {
        let (eye, yaw, pitch, roll) = self.vehicle.camera_world_full(turned);
        // the driver's eye sits a touch forward of the authored seat point.
        let eye = eye + self.vehicle.body_rotation().transform_vector3(self.head_offset() + self.seat + EYE_NUDGE).as_dvec3();
        Camera { position: eye, yaw, pitch: pitch.clamp(-89.0, 89.0), roll, fov_deg: turned.fov, near: 0.1, far: 6000.0 }
    }

    /// Where a camera in the world (the walker's eyes) is in the bus's frame.
    pub(crate) fn local_of_world(&self, cam: &Camera) -> omsi_vehicle::Camera {
        let inv = self.vehicle.body_rotation().inverse();
        let pos = inv.transform_vector3((cam.position - self.vehicle.position).as_vec3());
        let f = inv.transform_vector3(cam.forward());
        omsi_vehicle::Camera {
            pos: pos.to_array(),
            yaw: f.x.atan2(f.y).to_degrees(),
            pitch: f.z.clamp(-1.0, 1.0).asin().to_degrees(),
            fov: cam.fov_deg,
            ..Default::default()
        }
    }

    pub(crate) fn camera(&self, view: &str, fallback: &Camera) -> Camera {
        self.camera_look(view, fallback, (0.0, 0.0), ORBIT_DEFAULT)
    }

    /// The outside camera must not go through walls or under the road: it stands short of
    /// the first building, wall or canopy between it and the point it orbits, and above the
    /// ground (see `camera_arm`). `dt` 0 is a single picture: the arm is where it would
    /// settle, without easing.
    pub(crate) fn camera_clipped(
        &mut self,
        mut cam: Camera,
        world: &scene::World,
        dist: f32,
        dt: f32,
    ) -> Camera {
        let def = &self.vehicle.ty.def;
        let c = def.camera_outside_center;
        let centre = camera_arm::lift_pivot(world, orbit_pivot(self.vehicle.position, self.vehicle.heading, c));
        let want = dist.clamp(ORBIT_MIN, ORBIT_MAX);
        let back = -cam.forward().as_dvec3().normalize_or_zero();
        if back.length_squared() < 0.5 {
            return cam;
        }
        let right = cam.right().as_dvec3().normalize_or_zero();
        let up = back.cross(right).normalize_or_zero();
        let t0 = Instant::now();
        let free = camera_arm::free_length(world, centre, back, right, up, want as f64) as f32;
        let len = if dt <= 0.0 {
            self.arm.reset();
            free
        } else {
            self.arm.update(want, free, centre, dt)
        };
        if camera_arm::debug_level() >= 2 {
            log::info!(
                "camera arm: yaw {:.1} want {want:.2} free {free:.2} arm {len:.2} ({:.3} ms)",
                cam.yaw,
                t0.elapsed().as_secs_f64() * 1000.0
            );
        }
        cam.position = centre + back * len as f64;
        cam
    }

    /// How many passenger cameras the bus has, its coupled parts' included.
    pub(crate) fn pax_camera_count(&self) -> usize {
        self.vehicle.ty.def.cameras_pax.len() + self.vehicle.trailers.iter().map(|t| t.ty.def.cameras_pax.len()).sum::<usize>()
    }

    /// How many driver cameras the bus has, its coupled parts' included: Omsi.exe's
    /// interior-camera keys go on from the last of one part's into the next part's
    /// (0x706278 @0x7067d1, @0x706a3c).
    pub(crate) fn driver_camera_count(&self) -> usize {
        self.vehicle.ty.def.cameras_driver.len() + self.vehicle.trailers.iter().map(|t| t.ty.def.cameras_driver.len()).sum::<usize>()
    }

    /// The chosen driver camera's place among all of the bus's (the front's from its first,
    /// then each coupled part's): the choice counts from the front's `[set_camera_std]`, and
    /// the keys only go on into the next part past the front's last camera and back from
    /// its first, as Omsi.exe's `view_interiorcam_plus/minus` do (0x706278 @0x706802 steps
    /// the vehicle's own index and crosses to the coupled one only below 0 or past its
    /// last). Counted from the standard camera over the whole bus, the key to the left of
    /// the Urbino IV's standard camera took the rear section's camera first (#2503).
    pub(crate) fn driver_camera_slot(&self) -> usize {
        driver_camera_slot(self.vehicle.ty.def.camera_std, self.vehicle.ty.def.cameras_driver.len(), self.cam_choice.0, self.driver_camera_count())
    }

    /// The driver camera chosen past the front's own: the coupled part it is on and the
    /// camera (None while one of the front's is chosen).
    pub(crate) fn trailer_driver_camera(&self) -> Option<(&omsi_sim::vehicle::TrailerPart, &omsi_vehicle::Camera)> {
        let front = self.vehicle.ty.def.cameras_driver.len();
        let mut k = self.driver_camera_slot();
        if k < front {
            return None;
        }
        k -= front;
        for t in &self.vehicle.trailers {
            if let Some(c) = t.ty.def.cameras_driver.get(k) {
                return Some((t, c));
            }
            k -= t.ty.def.cameras_driver.len();
        }
        None
    }

    /// `look`: yaw/pitch the player has turned the head (or the orbit) by; `dist`: how far
    /// the outside camera sits from the vehicle.
    pub(crate) fn camera_look(&self, view: &str, fallback: &Camera, look: (f32, f32), dist: f32) -> Camera {
        let def = &self.vehicle.ty.def;
        // `mirror<n>`: what the n-th mirror's camera sees, as it is drawn into the mirror's
        // picture (a check of the mirrors against OMSI's own `reflexion<n>.bmp`)
        if let Some(c) = view.strip_prefix("mirror").and_then(|n| n.parse::<usize>().ok()).and_then(|n| def.cameras_reflexion.get(n)) {
            let k = def.cameras_reflexion.iter().position(|x| std::ptr::eq(x, c)).unwrap_or(0);
            let aimed = crate::camera_util::mirror_view(&self.vehicle, &crate::camera_util::adjusted(c, self.mirror_shifts.get(k).copied().unwrap_or([0.0; 3]), self.mirror_fovs.get(k).copied().unwrap_or(0.0)), crate::camera_util::driver_eye(self), self.mirror_offsets.get(k).copied().unwrap_or([0.0; 2]));
            let (eye, yaw, pitch, roll) = self.vehicle.camera_world_full(&aimed);
            return Camera { position: eye, yaw, pitch, roll, fov_deg: if c.fov > 1.0 { c.fov } else { 50.0 }, near: 0.1, far: 450.0 };
        }
        let cam = match view {
            "driver" => {
                // (a coupled part's driver camera, on that part's body)
                if let Some((t, c)) = self.trailer_driver_camera() {
                    let turned = omsi_vehicle::Camera { yaw: c.yaw + look.0 + self.steer_look, pitch: (c.pitch + look.1).clamp(-89.0, 89.0), ..c.clone() };
                    let (eye, yaw, pitch, roll) = t.camera_world_full(&turned);
                    return Camera { position: eye, yaw, pitch: pitch.clamp(-89.0, 89.0), roll, fov_deg: c.fov, near: 0.1, far: 6000.0 };
                }
                def.cameras_driver
                    .get(self.driver_camera_slot())
                    .or(def.cameras_driver.first())
            }
            "pax" => {
                // the passenger cameras of every part of the bus, the front's first: an
                // articulated bus's rear section brings its own in its `.bus`
                let n = self.pax_camera_count().max(1);
                let k = self.cam_choice.1 % n;
                match def.cameras_pax.get(k) {
                    Some(c) => Some(c),
                    None => {
                        let mut k = k - def.cameras_pax.len();
                        let mut found = None;
                        for t in &self.vehicle.trailers {
                            if let Some(c) = t.ty.def.cameras_pax.get(k) {
                                // (on the part's body like the front's: `dist`, pitch and roll)
                                let turned = omsi_vehicle::Camera { yaw: c.yaw + look.0, pitch: (c.pitch + look.1).clamp(-89.0, 89.0), ..c.clone() };
                                found = Some((t.camera_world_full(&turned), c.fov));
                                break;
                            }
                            k -= t.ty.def.cameras_pax.len();
                        }
                        if let Some(((eye, yaw, pitch, roll), fov)) = found {
                            return Camera { position: eye, yaw, pitch: pitch.clamp(-89.0, 89.0), roll, fov_deg: fov, near: 0.1, far: 6000.0 };
                        }
                        def.cameras_pax.first()
                    }
                }
            }
            _ => None,
        };
        match cam {
            Some(c) => {
                // inside: the head turns, the seat does not move. The camera hangs on the
                // body as Omsi.exe hangs its driver's and passengers' cameras (0x7cf82c ->
                // 0x7edfd0, the vehicle's own matrix): it pitches and rolls with the bus, the
                // look turned in the bus's frame. Kept level, the view stood still while the
                // cab rocked about it - the "boat" (the body's own motion matches Omsi's).
                let turned = omsi_vehicle::Camera { yaw: c.yaw + look.0 + if view == "driver" { self.steer_look } else { 0.0 }, pitch: (c.pitch + look.1).clamp(-89.0, 89.0), ..c.clone() };
                let (eye, yaw, pitch, roll) = self.vehicle.camera_world_full(&turned);
                let eye = if view == "driver" { eye + self.vehicle.body_rotation().transform_vector3(self.head_offset() + self.seat + EYE_NUDGE).as_dvec3() } else { eye };
                // near 0.1 as in Omsi.exe (every view, 0x6f6aa7); with the reversed float
                // depth buffer it costs no precision out at 6 km
                Camera {
                    position: eye,
                    yaw,
                    pitch: pitch.clamp(-89.0, 89.0),
                    roll,
                    fov_deg: c.fov,
                    near: 0.1,
                    far: 6000.0,
                }
            }
            None => {
                // outside view: an orbit around the vehicle
                let c = def.camera_outside_center;
                let center = orbit_pivot(self.vehicle.position, self.vehicle.heading, c);
                let mut cam = Camera {
                    position: center,
                    yaw: self.vehicle.heading as f32 + look.0,
                    pitch: (-15.0 + look.1).clamp(-85.0, 85.0),
                    roll: 0.0,
                    fov_deg: fallback.fov_deg,
                    near: 0.1,
                    far: 6000.0,
                };
                cam.position =
                    center - (cam.forward() * dist.clamp(ORBIT_MIN, ORBIT_MAX)).as_dvec3();
                cam
            }
        }
    }
}

/// Outside-camera pivot: the `.bus` centre rotated by heading alone. Body pitch
/// and bank (suspension bounce, cornering roll) would swing the camera if they
/// reached the pivot; the view only ever yaws with the bus.
pub(crate) fn orbit_pivot(position: DVec3, heading_deg: f64, center: [f32; 3]) -> DVec3 {
    position
        + glam::Mat4::from_rotation_z((-(heading_deg as f32)).to_radians())
            .transform_point3(Vec3::new(center[0], center[1], center[2]))
            .as_dvec3()
}

/// Driver eye forward offset, currently zeroed: how OMSI 2 keeps the turning
/// head from clipping inside the seat mesh still needs figuring out (per-bus
/// mesh handling), so no global offset until then. Kept as the single knob.
pub(crate) const EYE_NUDGE: Vec3 = Vec3::ZERO;

/// Put a vehicle's meshes where its state says (animations, visibility, lights, the
/// matrix textures) - the player's bus, and the launcher's showroom bus.
pub(crate) fn sync_vehicle_transforms(
    renderer: &Renderer,
    scene: &mut Scene,
    vehicle: &mut omsi_sim::VehicleInstance,
    render: &mut scene::VehicleRender,
    trailer_renders: &mut [scene::VehicleRender],
    inside: bool,
) {
    scene::sync_vehicle_materials(renderer, scene, vehicle, render);
    scene::sync_skinned(
        renderer,
        scene,
        vehicle,
        render,
        trailer_renders,
    );
    for (i, inst) in render.instances.iter().enumerate() {
        renderer.set_transform(
            scene,
            *inst,
            vehicle.position,
            vehicle.mesh_local_transform(i),
        );
        let p = &vehicle.mesh_props[i];
        let def = &vehicle.ty.model.meshes[vehicle.ty.meshes[i].def_index];
        let vp = def.viewpoint;
        let vp_ok = vp == 0 || (inside && vp & 2 != 0) || (!inside && vp & 1 != 0);
        // the outside of the bus seen from the cab: not in the picture, but in the mirrors,
        // which look at the bus from outside (its flanks were missing from them)
        let mirror = inside && vp & 1 != 0 && vp & 2 == 0;
        let mut alpha = p.slot_alpha.clone();
        for (slot, mat) in scene.instances[*inst].materials.iter().enumerate() {
            if scene
                .materials
                .get(*mat)
                .is_some_and(|m| m.alpha == omsi_render::AlphaMode::Opaque)
            {
                if let Some(a) = alpha.get_mut(slot) {
                    *a = 1.0;
                }
            }
        }
        // (a shadow blob is left out by the renderer while it draws the sun's shadows)
        renderer.set_params(scene, *inst, &alpha, p.visible && (vp_ok || mirror), &p.slot_uv);
        renderer.set_mirror_only(scene, *inst, mirror);
        renderer.set_slot_light(scene, *inst, &p.slot_light);
        renderer.set_slot_night(scene, *inst, &p.slot_night);
        renderer.set_interior(scene, *inst, p.interior);
    }
    for (t, r) in vehicle.trailers.iter().zip(trailer_renders.iter()) {
        for (i, inst) in r.instances.iter().enumerate() {
            renderer.set_transform(scene, *inst, t.position, t.mesh_local_transform(i));
            let p = &t.mesh_props[i];
            let def = &t.ty.model.meshes[t.ty.meshes[i].def_index];
            let vp = def.viewpoint;
            let vp_ok = vp == 0 || (inside && vp & 2 != 0) || (!inside && vp & 1 != 0);
            let mirror = inside && vp & 1 != 0 && vp & 2 == 0;
            let mut alpha = p.slot_alpha.clone();
            for (slot, mat) in scene.instances[*inst].materials.iter().enumerate() {
                if scene
                    .materials
                    .get(*mat)
                    .is_some_and(|m| m.alpha == omsi_render::AlphaMode::Opaque)
                {
                    if let Some(a) = alpha.get_mut(slot) {
                        *a = 1.0;
                    }
                }
            }
            renderer.set_params(scene, *inst, &alpha, p.visible && (vp_ok || mirror), &p.slot_uv);
            renderer.set_mirror_only(scene, *inst, mirror);
            renderer.set_slot_light(scene, *inst, &p.slot_light);
            renderer.set_slot_night(scene, *inst, &p.slot_night);
            renderer.set_interior(scene, *inst, p.interior);
        }
    }
    scene::sync_vehicle_textures(renderer, scene, vehicle, render, &mut { usize::MAX });
    let mut trailers = std::mem::take(&mut vehicle.trailers);
    for (t, r) in trailers.iter_mut().zip(trailer_renders.iter_mut()) {
        scene::sync_vehicle_part(renderer, scene, &vehicle, t, r);
    }
    vehicle.trailers = trailers;
}

/// The place among all `total` driver cameras of the bus (the front's `front` first) of the
/// choice `choice` counted from the front's standard camera `std` (see
/// [`Player::driver_camera_slot`]).
fn driver_camera_slot(std: usize, front: usize, choice: usize, total: usize) -> usize {
    (std % front.max(1) + choice) % total.max(1)
}

#[cfg(test)]
mod tests {
    use super::driver_camera_slot;

    /// An articulated bus with eleven cameras in front, the standard one the second, and one
    /// in the rear section (as the Urbino IV's). The key to the left of the standard camera takes the front's
    /// first; only the one after that goes back round into the rear section's (#2503).
    #[test]
    fn the_rear_sections_camera_comes_after_the_fronts_first() {
        let slot = |choice| driver_camera_slot(1, 11, choice, 12);
        assert_eq!(slot(0), 1);
        assert_eq!(slot(11), 0, "one to the left");
        assert_eq!(slot(10), 11, "two to the left: the rear section's");
        assert_eq!(slot(1), 2, "one to the right");
        // a bus on its own as before: counted round its own cameras
        assert_eq!(driver_camera_slot(3, 5, 4, 5), 2);
        assert_eq!(driver_camera_slot(0, 0, 0, 0), 0);
    }
}
