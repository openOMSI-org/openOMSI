//! Making a vehicle instance from its type, restoring its script state, and its text textures.

use super::*;

impl VehicleInstance {
    /// The fleet number the scripts know as `number` (empty for a vehicle without one).
    pub fn number(&self) -> String {
        self.ty
            .program
            .str_var("number")
            .and_then(|i| self.state.str_vars.get(i as usize))
            .cloned()
            .unwrap_or_default()
    }

    /// The license plate the scripts know as `ident` (empty for a vehicle without one).
    pub fn ident(&self) -> String {
        self.ty
            .program
            .str_var("ident")
            .and_then(|i| self.state.str_vars.get(i as usize))
            .cloned()
            .unwrap_or_default()
    }

    pub fn new(ty: Arc<VehicleType>, host: VehicleHost) -> VehicleInstance {
        let program = ty.program.clone();
        let mut state = State::new(&program);
        let mut vm = Vm::new();
        let mut host = host;
        host.script_textures = ty
            .model
            .script_textures
            .iter()
            .map(|(w, h)| crate::scripttex::ScriptTexture::new(*w, *h))
            .collect();
        host.content_dir = ty.def.dir().to_path_buf();
        let html_textures: Vec<crate::htmltex::HtmlTexture> = ty
            .model
            .html_textures
            .iter()
            .map(|d| {
                let dirs = [ty.model_dir.as_path(), ty.def.dir()];
                let html = crate::htmltex::load_page(&dirs, &d.path);
                crate::htmltex::HtmlTexture::new(d.script_index, d.width, d.height, &html)
                    .with_asset_dirs(crate::htmltex::asset_dirs(&dirs, &d.path))
            })
            .collect();
        host.number_var = program.str_var("number");
        // The vehicle dialog has already chosen these. They must exist before {init}: many
        // mods branch on the fleet number to choose equipment, textures or script state.
        if let (Some(i), Some(number)) = (program.str_var("number"), host.initial_number.as_ref()) {
            state.str_vars[i as usize] = number.clone();
        }
        if let (Some(i), Some(ident)) = (program.str_var("ident"), host.initial_ident.as_ref()) {
            state.str_vars[i as usize] = ident.clone();
        }
        // defaults every bus expects before {init}
        let mut var_index = HashMap::new();
        for (i, n) in program.var_names.iter().enumerate() {
            var_index.insert(n.to_ascii_lowercase(), i as omsi_script::VarId);
        }
        // engine-provided defaults
        if let Some(i) = var_index.get("envir_brightness") {
            state.vars[*i as usize] = 1.0;
        }
        // no ticket printed yet
        if let Some(i) = var_index.get("giventicket") {
            state.vars[*i as usize] = -1.0;
        }
        // `wearlifespan` (varlist_roadvehicle.txt, written by the engine from the
        // difficulty settings): every random part lifetime the scripts draw at {init} is
        // multiplied by it - the rear-door automatic, bulbs, the matrix display. Left at 0
        // the SD200's rear door was worn out on its first closing and reopened by itself,
        // with the opening sound, from then on; the matrix "spinnt" for the same reason.
        if let Some(i) = var_index.get("wearlifespan") {
            state.vars[*i as usize] = host.wear_lifespan.max(0.01);
        }
        // These are engine-owned visual inputs, not script variables.  They still need
        // to exist before {init}: material/prop setup can read them while the scene is
        // created, which otherwise makes stock buses (notably GN92) start with the dirt
        // layer at full opacity until the first simulation tick repairs it.
        for (name, value) in [
            ("Dirt_Norm", 0.0),
            ("DirtRate", 0.0),
            ("PrecipRate", 0.0),
            ("StreetCond", 0.0),
        ] {
            let key = name.to_ascii_lowercase();
            let id = match var_index.get(&key).copied() {
                Some(id) => id,
                None => {
                    let id = state.vars.len() as omsi_script::VarId;
                    state.vars.push(value);
                    var_index.insert(key, id);
                    id
                }
            };
            state.vars[id as usize] = value;
        }
        let mut model_only_vars: Vec<String> = Vec::new();
        for c in ty.model.meshes.iter().flat_map(|m| m.visible_groups()).flatten() {
            let key = c.variable.to_ascii_lowercase();
            if !var_index.contains_key(&key) && !model_only_vars.contains(&key) {
                model_only_vars.push(key);
            }
        }

        // `Axle_Springfactor_*` (varlist_roadvehicle.txt): the engine's spring scale, which
        // air-suspension scripts overwrite with their bellows pressure; a steel-sprung bus
        // never writes it and must not ride on springs of zero
        for n in program
            .var_names
            .iter()
            .filter(|n| n.to_ascii_lowercase().starts_with("axle_springfactor_"))
        {
            if let Some(i) = var_index.get(&n.to_ascii_lowercase()) {
                state.vars[*i as usize] = 1.0;
            }
        }
        // `$.yard` is the depot the bus runs from, the [name] of its .hof, known before
        // {init}: the matrix displays build their line list and palette paths from it
        // (`Linienlisten\<yard>_ANX.jpg`, `palettes\<yard>.bmp`), the roller blind its
        // line list, and some IBIS scripts branch on it.
        if let (Some(i), Some(h)) = (program.str_var("yard"), host.hof.as_ref()) {
            state.str_vars[i as usize] = h.name.clone();
        }
        if let Some(scheme) = host.paint_scheme {
            let scheme = scheme.filter(|i| *i < ty.paint_schemes.len());
            let mut put = |name: &str, v: f32| {
                let key = name.to_ascii_lowercase();
                match var_index.get(&key) {
                    Some(id) => state.vars[*id as usize] = v,
                    None if model_only_vars.contains(&key) => {
                        var_index.insert(key, state.vars.len() as omsi_script::VarId);
                        state.vars.push(v);
                    }
                    None => {}
                }
            };
            put("Colorscheme", scheme.map(|i| i as f32).unwrap_or(-1.0));
            if let Some(i) = scheme {
                for (var, v) in &ty.paint_schemes[i].set_vars {
                    put(var, *v);
                }
            }
        }
        if let Some(i) = var_index.get("schedule_active") {
            state.vars[*i as usize] = host.schedule_active;
        }
        vm.run_init(&program, &mut state, &mut host);
        let mut animators: Vec<MeshAnimator> = ty
            .meshes
            .iter()
            .map(|m| MeshAnimator::new(&ty.model.meshes[m.def_index], m.pivot, |n| program.var(n)))
            .collect();
        crate::anim::link_parents(
            &mut animators,
            &ty.meshes
                .iter()
                .map(|m| &ty.model.meshes[m.def_index])
                .collect::<Vec<_>>(),
        );
        let n = ty.meshes.len();
        let physics = VehiclePhysics::from_definition(&ty.def);
        let v = |name: &str| program.var(name);
        let v_wheels = (0..physics.wheels.len())
            .map(|a| {
                let side = |s: &str| {
                    [
                        v(&format!("Wheel_Rotation_{a}_{s}")),
                        v(&format!("Wheel_RotationSpeed_{a}_{s}")),
                        v(&format!("Axle_Steering_{a}_{s}")),
                        v(&format!("Axle_Suspension_{a}_{s}")),
                        v(&format!("Axle_Brakeforce_{a}_{s}")),
                    ]
                };
                [side("L"), side("R")]
            })
            .collect();
        let v_springfactor = (0..physics.wheels.len())
            .map(|a| {
                [
                    v(&format!("Axle_Springfactor_{a}_L")),
                    v(&format!("Axle_Springfactor_{a}_R")),
                ]
            })
            .collect();
        let v_surface = (0..physics.wheels.len())
            .map(|a| [v(&format!("Axle_SurfaceID_{a}_L")), v(&format!("Axle_SurfaceID_{a}_R"))])
            .collect();
        let rest_sag: Vec<(f32, f32)> = {
            let rb = crate::rigid::RigidBody::from_definition(&ty.def, &ty.hub_heights(0));
            (0..physics.wheels.len())
                .map(|a| {
                    let w = &rb.wheels[a * 2];
                    (
                        w.rest_compression().min(crate::rigid::BUMP),
                        w.radius - w.attach.z,
                    )
                })
                .collect()
        };
        // An AI copy has no rigid body: it stands on the contact plane of its wheels
        // (`ai_motion::AiBody`), and what the modelled tyre reaches below its hub less than
        // the model thinks lifts it. An axle the model does not move with `Axle_Suspension`
        // got the spring sag taken off its hub height (`VehicleType::hub_heights`), which an
        // AI copy without springs must not stand on.
        let ai_lift = {
            let offs: Vec<f32> = rest_sag
                .iter()
                .enumerate()
                .map(|(a, (comp, off))| {
                    if ty.suspension_axles.contains(&a) {
                        *off
                    } else {
                        off - comp
                    }
                })
                .collect();
            offs.iter().sum::<f32>() / offs.len().max(1) as f32
        };
        let trolley = TrolleyGear::for_type(&ty.def.trolley, std::ptr::addr_of!(host) as u64);
        let mut instance = VehicleInstance {
            pending_destination: None,
            a_trans: OmsiFrames::default(),
            particles: ParticleSet::new(ty.model.particle_systems(), std::ptr::addr_of!(host) as u64 ^ 0x9e37_79b9),
            light_fade: Vec::new(),
            v_springfactor,
            v_surface,
            rest_sag,
            ai_lift,
            contact: None,
            knocked: Vec::new(),
            knocked_now: Vec::new(),
            touching: Vec::new(),
            v_velocity: v("Velocity"),
            v_velocity_ground: v("Velocity_Ground"),
            v_n_wheel: v("n_Wheel"),
            v_m_wheel: v("M_Wheel"),
            v_throttle: v("Throttle").or_else(|| v("throttle_pedal")),
            v_brake: v("Brake").or_else(|| v("brake_pedal")),
            v_brakeforce: v("Brakeforce"),
            v_clutch: v("Clutch").or_else(|| v("clutch_pedal")),
            v_pax_req: (0..PAX_DOORS)
                .flat_map(|i| [format!("PAX_Entry{i}_Req"), format!("PAX_Exit{i}_Req"), format!("PAX_Entry{i}_Busy"), format!("PAX_Exit{i}_Busy")])
                .filter_map(|n| v(&n))
                .collect(),
            v_cables: control_cables(&ty),
            v_accel: [v("A_Trans_X"), v("A_Trans_Y"), v("A_Trans_Z")],
            v_wheels,
            ty,
            state,
            vm,
            host,
            position: DVec3::ZERO,
            heading: 0.0,
            pitch: 0.0,
            bank: 0.0,
            animators,
            mesh_transforms: vec![Mat4::IDENTITY; n],
            mesh_props: vec![MeshProps::default(); n],
            physics,
            text_textures: Vec::new(),
            html_textures,
            ground: None,
            trailers: Vec::new(),
            skin_rest: Vec::new(),
            skin_plans: SkinPlans::default(),
            collision: None,
            dynamic_boxes: Vec::new(),
            dynamic_impacts: Vec::new(),
            damage_dents: Vec::new(),
            broken_glass: Vec::new(),
            glass_broken: vec![false; n],
            collided: false,
            last_crash: 0.0,
            wheel_walls: true,
            radio_text: None,
            radio_frequency: None,
            crashes: 0,
            last_impact: 0.0,
            dirt: 0.0,
            rigid: None,
            wheel_seats: None,
            ai_odometer: 0.0,
            seat: (0.0, 0.0, Vec3::ZERO),
            driven_km: 0.0,
            km_started: false,
            cabin_air: None,
            ai_visuals: true,
            ai_script_period: 0.0,
            ai_visuals_missed: 0.0,
            trailers_stepped: false,
            var_index: std::sync::Arc::new(var_index),
            model_only_vars,
            props_plan: PropsPlan::default(),
            pax_vars: std::sync::OnceLock::new(),
            engine_ids: std::sync::OnceLock::new(),
            program_ids: std::sync::OnceLock::new(),
            trolley,
            rail_env: None,
            rail_scripted: false,
            rail_out: None,
            rail_log: 0.0,
        };
        instance.declare_trolley_vars();
        instance
    }

    /// Restore a situation without running scripts against an incomplete host. Nothing is
    /// ticked here: the caller attaches the timetable callbacks (`PlayerDuty::resume`) and
    /// then runs the frames, the first of which rebuilds the displays from the restored
    /// state. Returns how many numeric and string variables the program knew.
    pub fn restore_script_state(
        &mut self,
        vars: &[(String, f32)],
        strings: &[(String, String)],
    ) -> (usize, usize) {
        self.pending_destination = None;
        let mut numeric = 0;
        let mut textual = 0;
        for (name, value) in vars {
            numeric += usize::from(self.set_var(name, *value));
            if name.eq_ignore_ascii_case("Dirt_Norm") {
                self.dirt = value.clamp(0.0, 1.0);
            }
        }
        for (name, value) in strings {
            if let Some(i) = self.ty.program.str_var(name) {
                self.state.str_vars[i as usize] = value.clone();
                textual += 1;
            }
        }
        // Text images are per instance. An unchanged string still needs an upload when
        // a snapshot is applied to a vehicle whose previous images were already synced.
        for t in &mut self.text_textures {
            t.last_text = None;
        }
        for part in &mut self.trailers {
            for t in &mut part.text_textures {
                t.last_text = None;
            }
        }
        // The stock bitmap matrices - Script/Matrix.osc of the MAN_SD200/SD202 and
        // Matrix_D.osc / VMatrix*.osc of the MAN_NL_NG (EN92, GN92) - redraw their
        // script texture only when the IBIS line or terminus differs from
        // Matrix_Nr_Last / Matrix_TerminusIndex_Last. The texture is not in an .osn, so
        // the saved "last" values would leave this fresh instance's matrix blank: they
        // are reset to force one redraw. Text/roller displays keep all their saved
        // state; no destination or power trigger is fired here.
        if !self.host.script_textures.is_empty()
            && self.ty.program.macro_block("Matrix_frame").is_some()
        {
            self.set_var("Matrix_Nr_Last", -1.0);
            self.set_var("Matrix_TerminusIndex_Last", -1.0);
        }
        self.update_visuals(0.0);
        (numeric, textual)
    }

    /// Prepare the text textures with fonts from `lib`.
    pub fn init_text_textures(
        &mut self,
        lib: &mut crate::texttex::FontLibrary,
        decode: &dyn Fn(&Path) -> Option<(u32, u32, Vec<u8>)>,
    ) {
        self.text_textures = self
            .ty
            .model
            .text_textures
            .iter()
            .map(|t| crate::texttex::TextTextureState::new(t.clone(), lib.get(&t.font, decode)))
            .collect();
    }

    /// Current text of a string variable (empty when it does not exist).
    pub fn str_var(&self, name: &str) -> String {
        self.ty
            .program
            .str_var(name)
            .map(|i| self.state.str_vars[i as usize].clone())
            .unwrap_or_default()
    }

    /// The string a `[texttexture]` names: its first field is either a script variable's name
    /// or - as the Chinese AI cars write it, `[texttexture] 0 CN_REG ...` - the *number* of a
    /// built-in string (`program/stringvarlist_roadvehicle.txt`: 0 ident, 1 number, ...).
    /// Omsi.exe takes a number as that index (the scenery objects' `[texttexture]` do the same,
    /// see `resolve_scenery_freetex_name`); taken for a variable's name it matches nothing and
    /// the plate stays empty. See `Program::text_texture_var`.
    pub fn text_texture_string(&self, field: &str) -> String {
        self.text_texture_str(field).to_string()
    }

    /// [`Self::text_texture_string`] without a copy.
    pub fn text_texture_str(&self, field: &str) -> &str {
        self.ty.program.text_texture_var(field).map(|i| self.state.str_vars[i as usize].as_str()).unwrap_or("")
    }

    /// Re-render changed text textures; returns the indices with a pending image.
    pub fn update_text_textures(&mut self) -> Vec<usize> {
        let mut changed = Vec::new();
        // `Refresh_Strings` is reset; an unchanged string would draw the same picture again
        if let Some(id) = self.program_ids().refresh_strings {
            self.state.vars[id as usize] = 0.0;
        }
        // (the field and the text are read where they are: both were copied for every text
        // texture every frame)
        for (i, t) in self.text_textures.iter_mut().enumerate() {
            let text = self.ty.program.text_texture_var(&t.def.variable).map(|v| self.state.str_vars[v as usize].as_str()).unwrap_or("");
            if t.update(text) {
                changed.push(i);
            }
        }
        changed
    }

    /// The variables a paint scheme sets: its `[setvar]` lines, and `Colorscheme` - the
    /// scheme's index in the `.cti` list, −1 for the model's own textures (OMSI
    /// the original at the spawn; the original reads it back as the index).
    pub fn apply_paint_vars(&mut self, scheme: Option<usize>) {
        let scheme = scheme.filter(|i| *i < self.ty.paint_schemes.len());
        // (an engine variable: a model may switch meshes by it without declaring it)
        self.set_engine_var("Colorscheme", scheme.map(|i| i as f32).unwrap_or(-1.0));
        if let Some(i) = scheme {
            for (var, v) in self.ty.paint_schemes[i].set_vars.clone() {
                self.set_var(&var, v);
            }
        }
    }

    /// The scheme's `[setvar]`s set again, before every frame of the scripts: Omsi.exe
    /// writes them each frame, not once, and scripts count on it. The Ikarbus cockpit
    /// turns `v_font` to the Cyrillic font (7) while its switch is on and leaves it there
    /// when the switch goes off; the scheme's `v_font 0` every frame is what brings the
    /// Latin font back, which the matrix then draws at the next terminus. Set once, the
    /// display stayed Cyrillic for good.
    pub(crate) fn apply_scheme_vars_each_frame(&mut self) {
        let Some(Some(i)) = self.host.paint_scheme else { return };
        if let Some(s) = self.ty.paint_schemes.get(i) {
            if s.set_vars.is_empty() {
                return;
            }
            let vars = s.set_vars.clone();
            for (var, v) in vars {
                self.set_var(&var, v);
            }
        }
    }
}

pub(super) fn control_cables(ty: &VehicleType) -> Vec<(omsi_script::VarId, omsi_script::VarId)> {
    let v = |name: &str| ty.program.var(name);
    let mut cables: Vec<_> = ty.def.control_cable_front.iter().chain(&ty.def.control_cable_back)
        .filter_map(|c| Some((v(c.lines.get(3)?)?, v(c.lines.get(2)?)?)))
        .collect();
    cables.sort_unstable();
    cables.dedup();
    cables
}
