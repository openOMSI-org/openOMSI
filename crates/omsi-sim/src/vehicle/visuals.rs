//! Animations, material properties, skinned meshes and the drawn tyres after the scripts ran.

use super::*;

impl VehicleInstance {
    /// Brightest `[interiorlight]` of the vehicle right now (0..1), for the passengers.
    pub fn interior_light(&self) -> f32 {
        self.ty
            .model
            .interior_lights
            .iter()
            .map(|il| {
                il.variable
                    .trim()
                    .parse::<f32>()
                    .ok()
                    .or_else(|| self.var(&il.variable))
                    .unwrap_or(0.0)
                    .clamp(0.0, 1.0)
                    * il.range.clamp(0.0, 1.0)
            })
            .fold(0.0, f32::max)
    }

    /// The `[smoothskin]` meshes (the bellows of an articulated bus).
    pub fn skinned_meshes(&self) -> Vec<usize> {
        (0..self.ty.meshes.len())
            .filter(|&i| !self.ty.meshes[i].skin.is_empty())
            .collect()
    }

    /// The transforms a skinned mesh's shape depends on (its own and its bones'): when they
    /// are unchanged, so is the shape.
    pub fn skin_key(&self, i: usize) -> Vec<Mat4> {
        skin_key(&self.ty, i, &self.mesh_transforms)
    }

    /// Skinned mesh `i` as its bones hold it now (see [`skin_vertices`]).
    pub fn skinned(&mut self, i: usize) -> Option<(Vec<Vec3>, Vec<Vec3>)> {
        if self.skin_rest.len() != self.animators.len() {
            self.skin_rest = rest_transforms(&self.animators, self.state.vars.len());
        }
        let plan = self.skin_plans.get(&self.ty, i)?;
        skin_vertices(&self.ty, i, &self.mesh_transforms, &self.skin_rest, plan)
    }

    /// Seat the drawn tyres on the hubs the physics has: a model whose suspension
    /// animation lifts its wheels by less (or more) than the spring is compressed - the
    /// LiAZ 5292 turns them about pivots across the bus, and at rest they stood 4 cm in the
    /// asphalt, like flat tyres - has each tyre mesh moved up or down to where its hub is.
    /// Only small corrections (under 8 cm) and only axles the model moves with
    /// `Axle_Suspension`: a larger mismatch is the model's own business.
    pub(super) fn seat_wheels(&mut self) {
        let Some(rb) = self.rigid.as_ref() else { return };
        if self.wheel_seats.is_none() {
            let mut seats = Vec::new();
            for (i, vm) in self.ty.meshes.iter().enumerate() {
                let def = &self.ty.model.meshes[vm.def_index];
                let Some(an) = def.animations.iter().find(|a| a.variable.to_ascii_lowercase().starts_with("wheel_rotation_")) else { continue };
                let v = an.variable.to_ascii_lowercase();
                let rest = v.trim_start_matches("wheel_rotation_");
                let mut parts = rest.split('_');
                let (Some(Ok(axle)), Some(side)) = (parts.next().map(|a| a.parse::<usize>()), parts.next()) else { continue };
                if !self.ty.suspension_axles.contains(&axle) {
                    continue;
                }
                let k = axle * 2 + if side.starts_with('r') { 1 } else { 0 };
                if k >= rb.wheels.len() {
                    continue;
                }
                // (the hub is where the wheel turns about: its `origin_trans`, or the mesh's
                // own pivot for `origin_from_mesh`. Measured at the .o3d's pivot, a tyre without
                // one - the NEOMAN's right front, at the model's origin - was measured at a point
                // swinging round the hub as the wheel turned: it was pushed up and down by
                // centimetres while its hub cap stayed, and the cap "rolled off" the tyre.)
                let mut hub = None;
                for o in &an.origins {
                    match o {
                        omsi_model::AnimOrigin::Trans(t) => hub = Some(hub.unwrap_or(Vec3::ZERO) + Vec3::from(*t)),
                        omsi_model::AnimOrigin::FromMesh => hub = Some(vm.pivot.w_axis.truncate()),
                        _ => {}
                    }
                }
                seats.push((i, k, hub.unwrap_or(vm.pivot.w_axis.truncate())));
            }
            self.wheel_seats = Some(seats);
        }
        let seats = self.wheel_seats.as_ref().unwrap();
        for &(i, k, pivot) in seats {
            let comp = rb.wheels[k].compression.clamp(-crate::rigid::DROOP, crate::rigid::BUMP);
            let drawn = self.mesh_transforms[i].transform_point3(pivot).z;
            let dz = pivot.z + comp - drawn;
            if omsi_cfg::flags::OMSI_DEBUG_SEAT.is_set() {
                log::info!("seat mesh {i} wheel {k}: comp {:.4} drawn {:.4} pivot {:.4} dz {:.4}", comp, drawn, pivot.z, dz);
            }
            // (every frame, however small: a dead band of 3 mm had the correction switch on
            // and off as the travel crossed it, and the tyre ticked up and down by that much)
            if dz.abs() < 0.08 {
                self.mesh_transforms[i] = Mat4::from_translation(Vec3::new(0.0, 0.0, dz)) * self.mesh_transforms[i];
            }
        }
    }

    /// Animations and per-mesh material properties after the scripts ran.
    pub(super) fn update_visuals(&mut self, dt: f32) {
        // coupled vehicles come first, because they write the joint's angles this vehicle's
        // plates and bellows turn with (a frame late, the joint lagged behind the rear section)
        if !std::mem::take(&mut self.trailers_stepped) {
            self.update_trailers(dt);
        }
        // a raised pantograph is shown where its head meets the wire, not at the full travel
        // the script's variable says: shown that value for the animations only
        let shown = self.pantograph_shown();
        let raw = shown.map(|(id, v)| (id as usize, std::mem::replace(&mut self.state.vars[id as usize], v)));
        for (i, a) in self.animators.iter_mut().enumerate() {
            self.mesh_transforms[i] = a.update(dt, &self.state.vars);
        }
        if let Some((id, v)) = raw {
            self.state.vars[id] = v;
        }
        crate::anim::apply_parents(&self.animators, &mut self.mesh_transforms);
        self.seat_wheels();
        if let Some(want) = omsi_cfg::flags::OMSI_DEBUG_ANIM.var() {
            for (i, m) in self.ty.meshes.iter().enumerate() {
                let file = &self.ty.model.meshes[m.def_index].file;
                if file
                    .to_ascii_lowercase()
                    .contains(&want.to_ascii_lowercase())
                {
                    let a = &self.animators[i];
                    let vals: Vec<(String, f32, f32, [f32; 3], [f32; 3])> = a
                        .anims
                        .iter()
                        .map(|(d, o, st)| {
                            (
                                d.variable.clone(),
                                st.var
                                    .map(|v| self.state.vars[v as usize])
                                    .unwrap_or(f32::NAN),
                                st.value,
                                o.x_axis.truncate().to_array(),
                                o.w_axis.truncate().to_array(),
                            )
                        })
                        .collect();
                    log::info!(
                        "anim {file}: {vals:?} transform {:?}",
                        self.mesh_transforms[i]
                            .to_cols_array()
                            .map(|v| (v * 100.0).round() / 100.0)
                    );
                }
            }
        }
        self.props_plan.refresh(&self.ty, &self.var_index);
        self.props_plan
            .apply(&self.state.vars, &mut self.mesh_props);
        // Dirt overlays are engine-owned layers. A model may declare Dirt_Norm or
        // Dirt_Wiped in its cfg without putting the name in the script varlist; the
        // generic alphascale fallback is 1.0 in that case, which makes Dreck.tga an
        // opaque full-body decal. Resolve these values explicitly for every vehicle.
        // (the overlays and their slots are found once, in the plan: every material of every
        // mesh was compared by name each frame of every AI vehicle)
        for &(i, slot, wiped) in &self.props_plan.dirt {
            let value = if wiped {
                PropSource::value(self.props_plan.dirt_wiped.map(PropSource::Var).unwrap_or(PropSource::Missing), &self.state.vars, 0.0)
            } else {
                self.dirt
            };
            if let Some(alpha) = self.mesh_props[i].slot_alpha.get_mut(slot) {
                *alpha = value.clamp(0.0, 1.0);
            }
        }
        // the lamps come on and go out with their `timeconst`
        let mut lf = std::mem::take(&mut self.light_fade);
        let mut part_fades: Vec<Vec<f32>> = self.trailers.iter_mut().map(|t| std::mem::take(&mut t.light_fade)).collect();
        // (the vehicle's own lights and the coupled parts' - which read this vehicle's
        // variables too - by their plans' sources, resolved against this vehicle's variables
        // as the names were: each part's lamp was a look-up by name every frame)
        // (a train car with a `CarRun`: by its own variables)
        for t in &mut self.trailers {
            let index = t.run.as_deref().map_or(&self.var_index, |r| &r.names);
            t.props_plan.refresh(&t.ty, index);
        }
        let value = |part: Option<&TrailerPart>, n: &str| -> f32 {
            let n = n.trim();
            let run = part.and_then(|t| t.run.as_deref());
            n.parse::<f32>().ok().or_else(|| run.map_or_else(|| self.var(n), |r| r.get(n))).unwrap_or(0.0)
        };
        let fade = |part: Option<&TrailerPart>, model: &omsi_model::Model, fades: &mut Vec<f32>, lights: &[PropSource]| {
            let vars = part.and_then(|t| t.run.as_deref()).map_or(&self.state.vars, |r| &r.state.vars);
            let mut k = 0;
            for md in &model.meshes {
                for l in &md.light_enh_2 {
                    let v = match lights.get(k) {
                        Some(src) => src.value(vars, 0.0),
                        None => value(part, &l.variable),
                    };
                    let target = (v * if l.factor > 0.0 { l.factor } else { 1.0 }).clamp(0.0, 2.0);
                    // (a NaN stays NaN through clamp: the lamp's glow was drawn black on Windows)
                    let target = if target.is_finite() { target } else { 0.0 };
                    if fades.len() <= k {
                        fades.push(target);
                    }
                    let b = &mut fades[k];
                    if !b.is_finite() {
                        *b = target;
                    }
                    *b = if l.time_const <= 0.001 {
                        target
                    } else {
                        let rate = if target > *b { 1.0 } else { 1.31 } / l.time_const;
                        *b + (target - *b) * (1.0 - (-dt * rate).exp())
                    };
                    k += 1;
                }
            }
        };
        fade(None, &self.ty.model, &mut lf, &self.props_plan.lights);
        for (t, f) in self.trailers.iter().zip(part_fades.iter_mut()) {
            fade(Some(t), &t.ty.model, f, &t.props_plan.lights);
        }
        self.light_fade = lf;
        for (t, f) in self.trailers.iter_mut().zip(part_fades) {
            t.light_fade = f;
        }
        // the particle systems ([smoke]: exhaust, boiling coolant, wheel spray) of the
        // vehicle and its coupled parts, which read the same scripts' variables
        if !self.particles.is_empty() || self.trailers.iter().any(|t| !t.particles.is_empty()) {
            let mut ps = std::mem::take(&mut self.particles);
            let mut parts: Vec<ParticleSet> = self.trailers.iter_mut().map(|t| std::mem::take(&mut t.particles)).collect();
            {
                // (each puff keeps the height of the road under it - the plane the wheels
                // stand on - for the renderer to fade it out into, see `particles`)
                let value = |n: &str| self.var(n).unwrap_or(0.0);
                ps.update_over(dt, self.position, self.body_rotation(), &|| self.particle_ground(), &value);
                for (t, set) in self.trailers.iter().zip(parts.iter_mut()) {
                    let value = |n: &str| t.run_var(n).or_else(|| self.var(n)).unwrap_or(0.0);
                    set.update_over(dt, t.position, t.body_rotation(), &|| [-t.ground_lift(), 0.0, 0.0], &value);
                }
            }
            self.particles = ps;
            for (t, set) in self.trailers.iter_mut().zip(parts) {
                t.particles = set;
            }
        }
    }

    /// How far each tyre of the model (and of the coupled parts) stands above (+) or sinks
    /// into (-) what is under it, in metres, from the meshes as they are animated right now:
    /// the check that the wheels touch the road. (mesh file, gap). An AI type keeps no
    /// vertices on the CPU, so its wheel meshes are read again ([`VehicleType::mesh_data`]).
    pub fn wheel_ground_gaps(&self) -> Vec<(String, f32)> {
        let under = |x: f64, y: f64, top: f64| -> Option<f64> {
            match (&self.contact, &self.ground) {
                (Some(c), _) => c.probe(x, y, top).below,
                (None, Some(g)) => g(x, y),
                _ => None,
            }
        };
        let is_wheel = |ty: &VehicleType, def_index: usize| {
            ty.model.meshes[def_index].animations.iter().any(|a| {
                a.variable
                    .to_ascii_lowercase()
                    .starts_with("wheel_rotation_")
            })
        };
        let mut out = Vec::new();
        let mut measure = |ty: &VehicleType, origin: DVec3, xf: &dyn Fn(usize) -> Mat4| {
            for (i, vm) in ty.meshes.iter().enumerate() {
                if !is_wheel(ty, vm.def_index) {
                    continue;
                }
                let m = xf(i);
                let Some(data) = ty.mesh_data(i) else {
                    continue;
                };
                let Some(low) = data
                    .positions
                    .iter()
                    .map(|p| m.transform_point3(*p))
                    .min_by(|a, b| a.z.total_cmp(&b.z))
                else {
                    continue;
                };
                let w = origin + low.as_dvec3();
                if let Some(g) = under(w.x, w.y, w.z + 0.3) {
                    out.push((ty.model.meshes[vm.def_index].file.clone(), (w.z - g) as f32));
                }
            }
        };
        measure(&self.ty, self.position, &|i| self.mesh_local_transform(i));
        for t in &self.trailers {
            measure(&t.ty, t.position, &|i| t.mesh_local_transform(i));
        }
        out
    }

    /// The static pose of an AI copy on the road: how far its origin stands above the plane
    /// its wheels touch (m) and the `Axle_Suspension` that goes with it. OMSI sets an AI copy
    /// `[ai_deltaheight]` lower than its model origin and lifts the wheels by as much through
    /// `Axle_Suspension` (every stock AI road model animates it), so it stands exactly like a
    /// driven one that has settled on its springs - the delta is that sag; left at zero the
    /// AI buses rode 10 cm high. On top of it, the model's wheel geometry (see `ai_lift`).
    /// `AiBody` adds its own travel on its springs, which is zero at rest.
    pub fn ai_rest_offset(&self) -> (f32, f32) {
        let delta = if self.ty.suspension_axles.is_empty() {
            0.0
        } else {
            self.ty.def.ai_delta_height
        };
        (delta + self.ai_lift, delta)
    }

    /// Where the wheel meshes turn (`OMSI_DEBUG_WHEELS`): for every mesh animated by a
    /// `Wheel_Rotation_*` or `Axle_Steering_*` variable, how far the centre of its vertices
    /// moves when that variable alone is set - a wheel spinning or steering about its own
    /// centre does not move at all, one with an offset pivot swings around. An AI type keeps
    /// no vertices on the CPU (`VehicleType::load_ai`), so the mesh is read again here.
    pub fn wheel_pivot_report(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (i, m) in self.ty.meshes.iter().enumerate() {
            let a = &self.animators[i];
            let names: Vec<&str> = a
                .anims
                .iter()
                .map(|(d, _, _)| d.variable.as_str())
                .filter(|v| v.starts_with("Wheel_Rotation") || v.starts_with("Axle_Steering"))
                .collect();
            if names.is_empty() {
                continue;
            }
            let Some(data) = self.ty.mesh_data(i) else {
                continue;
            };
            if data.positions.is_empty() {
                continue;
            }
            let (lo, hi) = data.positions.iter().fold(
                (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
                |(lo, hi), p| (lo.min(*p), hi.max(*p)),
            );
            let c = (lo + hi) * 0.5;
            let pv = m.pivot.col(3);
            let px = m.pivot.col(0);
            let mut line = format!("{} {}: centre ({:.3}, {:.3}, {:.3}) pivot ({:.3}, {:.3}, {:.3}) x ({:.2}, {:.2}, {:.2}) det {:.2}", self.ty.def.path.file_name().unwrap_or_default().to_string_lossy(), self.ty.model.meshes[m.def_index].file, c.x, c.y, c.z, pv.x, pv.y, pv.z, px.x, px.y, px.z, m.pivot.determinant());
            for name in names {
                let Some(id) = self.ty.program.var(name) else {
                    continue;
                };
                let mut vars = vec![0.0; self.state.vars.len()];
                vars[id as usize] = 0.35;
                let mut probe = a.clone();
                for (_, _, st) in probe.anims.iter_mut() {
                    st.initialized = false;
                }
                let t = probe.update(0.0, &vars);
                let moved = t.transform_point3(c) - c;
                let (axis, angle) = Quat::from_mat4(&t).to_axis_angle();
                line.push_str(&format!("; {name} 0.35 rad moves it ({:.3}, {:.3}, {:.3}), axis ({:.2}, {:.2}, {:.2}) {:.1} deg", moved.x, moved.y, moved.z, axis.x, axis.y, axis.z, angle.to_degrees()));
            }
            out.push(line);
        }
        out
    }
}
