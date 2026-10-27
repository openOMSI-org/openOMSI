//! A vehicle's files read again while it stands or drives (the developer tools' hot
//! reload): its `.bus`, its model.cfg and its constfiles, each taken over by the running
//! vehicle where that can be done without placing it anew - the scripts' variables, the
//! body's pose and motion, the wheels' spin and travel are kept. What cannot be taken over
//! (other axles, other meshes, other variables) is refused with the reason, and the vehicle
//! goes on as it was.

use super::*;

impl VehicleInstance {
    /// The type with one part changed, for this vehicle alone (the others of its type keep
    /// theirs until they are made again).
    fn retyped(&mut self, change: impl FnOnce(&mut VehicleType)) {
        let mut ty = (*self.ty).clone();
        change(&mut ty);
        self.ty = Arc::new(ty);
        // (the mesh props, dirt overlays and lights were resolved against the old model:
        // a model.cfg read again with other slots or lights kept the old ones)
        self.props_plan = PropsPlan::default();
        self.program_ids = std::sync::OnceLock::new();
    }

    /// The `.bus` file read again (`def`, as `Vehicle::load` gives it): its cameras, mirrors,
    /// bounding box, centre of gravity, mass and the axles' springs, dampers and sizes count
    /// from now on. Its scripts are not compiled again. Refused when the number of axles
    /// changed (the scripts' wheel variables are laid out per axle).
    pub fn reload_definition(&mut self, mut def: Vehicle) -> Result<(), String> {
        if def.axles.len() != self.ty.def.axles.len() {
            return Err(format!("the axles changed ({} to {}): place the bus anew", self.ty.def.axles.len(), def.axles.len()));
        }
        // the box cut down to the model, as `VehicleType::load` does it
        if let (Some(bb), Some((lo, hi))) = (def.bounding_box.as_mut(), self.ty.model_box()) {
            let (size, mid) = (hi - lo, (hi + lo) * 0.5);
            if bb[1] > size.y + 1.5 {
                bb[1] = size.y + 0.1;
                bb[4] = mid.y;
            }
            if bb[0] > size.x + 0.8 {
                bb[0] = size.x + 0.05;
                bb[3] = mid.x;
            }
        }
        // (the scripts stay those compiled: the program is not made again)
        def.scripts = self.ty.def.scripts.clone();
        self.retyped(|ty| ty.def = def);
        self.v_cables = super::instance::control_cables(&self.ty);
        // the simple physics: the new sizes and masses, the motion as it was
        let mut physics = VehiclePhysics::from_definition(&self.ty.def);
        let old = &self.physics;
        physics.speed = old.speed;
        physics.accel = old.accel;
        physics.a_trans = old.a_trans;
        physics.steer_deg = old.steer_deg;
        physics.controls = old.controls;
        for (new, old) in physics.wheels.iter_mut().zip(&old.wheels) {
            for (n, o) in new.iter_mut().zip(old) {
                n.rotation_deg = o.rotation_deg;
                n.rpm = o.rpm;
                n.suspension = o.suspension;
            }
        }
        self.physics = physics;
        // the rigid body: made from the new file, put where the old one was and moving as
        // it moved
        if let Some(old) = self.rigid.take() {
            let mut rb = crate::rigid::RigidBody::from_definition(&self.ty.def, &self.ty.hub_heights(0));
            rb.orientation = old.orientation;
            rb.position = old.position + (old.orientation * (rb.cog - old.cog)).as_dvec3();
            rb.velocity = old.velocity;
            rb.omega = old.omega;
            rb.steer_deg = old.steer_deg;
            rb.kappa = old.kappa;
            rb.holding = old.holding;
            rb.accel_body = old.accel_body;
            rb.coupled = old.coupled;
            rb.spawned_inside = old.spawned_inside;
            rb.wheel_walls = old.wheel_walls;
            for (i, (n, o)) in rb.wheels.iter_mut().zip(&old.wheels).enumerate() {
                n.spring_factor = self.spring_factor(i / 2, i % 2);
                n.spin = o.spin;
                n.slipping = o.slipping;
                n.locked = o.locked;
                n.steer = o.steer;
                n.compression = o.compression;
                n.compression_rate = o.compression_rate;
                n.touch = o.touch;
                n.on_ground = o.on_ground;
                n.rotation_deg = o.rotation_deg;
                n.rpm = o.rpm;
                n.load = o.load;
                n.ground_z = o.ground_z;
            }
            self.rigid = Some(rb);
        }
        Ok(())
    }

    /// model.cfg read again, softly: taken over only when it names the same meshes in the
    /// same order (the meshes on the graphics card stay), with its animations, visibility
    /// variables and lights. Anything else waits for the bus to be placed anew.
    pub fn reload_model(&mut self, model: Model) -> Result<(), String> {
        let files = |m: &Model| m.meshes.iter().map(|d| d.file.to_ascii_lowercase()).collect::<Vec<_>>();
        if files(&model) != files(&self.ty.model) {
            return Err("its meshes changed: place the bus anew to see them".into());
        }
        self.retyped(|ty| ty.model = model);
        let ty = self.ty.clone();
        let program = ty.program.clone();
        let mut animators: Vec<MeshAnimator> = ty
            .meshes
            .iter()
            .map(|m| MeshAnimator::new(&ty.model.meshes[m.def_index], m.pivot, |n| program.var(n)))
            .collect();
        crate::anim::link_parents(&mut animators, &ty.meshes.iter().map(|m| &ty.model.meshes[m.def_index]).collect::<Vec<_>>());
        self.animators = animators;
        // (the cab wheel scaled again: the new animators have the model's factors)
        self.apply_cab_wheel_rotation();
        Ok(())
    }

    /// The vehicle's scripts compiled again from their files as they are now (`root`: the
    /// game folder, for the built-in variable lists), for [`VehicleInstance::reload_program`].
    pub fn recompile(&self, root: &Path) -> Program {
        let def = &self.ty.def;
        let input = CompileInput {
            builtin_vars: builtin_vars(root),
            builtin_str_vars: builtin_str_vars(root),
            varlists: def.scripts.varlists.clone(),
            stringvarlists: def.scripts.stringvarlists.clone(),
            constfiles: def.scripts.constfiles.clone(),
            scripts: def.scripts.scripts.clone(),
            ..Default::default()
        };
        compile(&input)
    }

    /// A program compiled again after the constfiles changed: its constants and curves are
    /// used from the next frame on, the variables keep their values. Refused when its
    /// variables are not the ones the vehicle has (the scripts or their lists changed).
    pub fn reload_program(&mut self, program: Program) -> Result<(), String> {
        let old = &self.ty.program;
        if program.var_names != old.var_names || program.str_var_names != old.str_var_names {
            return Err("the variables changed: place the bus anew".into());
        }
        if !program.errors.is_empty() {
            return Err(format!("{} errors compiling: {}", program.errors.len(), program.errors[0]));
        }
        self.retyped(|ty| ty.program = Arc::new(program));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bus() -> VehicleInstance {
        let mut def = Vehicle::default();
        def.axles = vec![omsi_vehicle::Axle { long: 3.0, ..Default::default() }, omsi_vehicle::Axle { long: -3.0, driven: true, ..Default::default() }];
        def.mass = 12.0;
        def.bounding_box = Some([2.5, 12.0, 3.0, 0.0, 0.0, 1.5]);
        let ty = Arc::new(VehicleType {
            def,
            model: Default::default(),
            model_dir: Default::default(),
            program: Arc::new(compile(&Default::default())),
            meshes: Vec::new(),
            lod_meshes: Vec::new(),
            paint_schemes: Vec::new(),
            texchanges: Vec::new(),
            wheel_meshes: Vec::new(),
            suspension_axles: Vec::new(),
            missing_packs: Vec::new(),
            mesh_bounds: Vec::new(),
            mesh_boxes: Vec::new(),
        });
        VehicleInstance::new(ty, VehicleHost::new(Default::default()))
    }

    #[test]
    fn a_bus_file_read_again_keeps_the_motion_and_takes_the_new_sizes() {
        let mut v = bus();
        v.enable_rigid_body();
        v.set_speed(5.0);
        let rb = v.rigid.as_mut().unwrap();
        rb.velocity = Vec3::new(0.0, 5.0, 0.0);
        rb.wheels[0].spin = 12.0;
        let pose = (rb.position, rb.orientation);
        let mut def = v.ty.def.clone();
        def.mass = 15.0;
        def.bounding_box = Some([2.5, 10.0, 3.0, 0.0, 0.5, 1.5]);
        def.cameras_reflexion.push(Default::default());
        let other = v.ty.clone();
        v.reload_definition(def.clone()).unwrap();
        assert_eq!(v.ty.def.bounding_box, Some([2.5, 10.0, 3.0, 0.0, 0.5, 1.5]));
        assert_eq!(v.ty.def.cameras_reflexion.len(), 1);
        assert_eq!(other.def.cameras_reflexion.len(), 0, "the type of the others stays");
        assert_eq!(v.physics.mass_kg, 15_000.0);
        assert_eq!(v.physics.speed, 5.0);
        let rb = v.rigid.as_ref().unwrap();
        assert_eq!((rb.mass, rb.velocity, rb.wheels[0].spin), (15_000.0, Vec3::new(0.0, 5.0, 0.0), 12.0));
        assert_eq!(rb.orientation, pose.1);
        assert!((rb.position - pose.0).length() < 1e-6, "same centre of gravity");
        // one axle less: refused, nothing changed
        def.axles.pop();
        assert!(v.reload_definition(def).is_err());
        assert_eq!(v.ty.def.axles.len(), 2);
    }

    #[test]
    fn a_program_with_other_variables_is_refused() {
        let mut v = bus();
        let same = compile(&Default::default());
        assert!(v.reload_program(same).is_ok());
        let dir = std::env::temp_dir().join(format!("omsi_reload_vars_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("vars.txt"), "extra_var\n").unwrap();
        let other = compile(&CompileInput { varlists: vec![dir.join("vars.txt")], ..Default::default() });
        let _ = std::fs::remove_dir_all(&dir);
        assert!(v.reload_program(other).is_err());
    }

    #[test]
    fn a_model_with_other_meshes_is_refused() {
        let mut v = bus();
        assert!(v.reload_model(Model::default()).is_ok());
        let mut m = Model::default();
        m.meshes.push(Default::default());
        assert!(v.reload_model(m).is_err());
    }
}
