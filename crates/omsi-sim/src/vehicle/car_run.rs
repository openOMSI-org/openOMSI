//! Train cars that run their scripts themselves. OMSI runs the scripts of every car of a
//! train, and a car's lamps come from where it stands: whether something is coupled at its
//! front (`train_frontcoupling`) or its back (`train_backcoupling`), and whether it runs
//! turned round (`train_me_reverse`). A coupled car whose program or model reads any of
//! these gets a [`CarRun`]: its scripts on a set of variables of its own, refreshed from the
//! leading vehicle every frame except where its own scripts store, and its meshes, lamps and
//! particles drawn from that set. (All read from the leading vehicle, the ET 425's second
//! unit, turned round at the back of the train, lit white head lamps at the tail.) Other
//! coupled parts keep animating from the leading vehicle.

use super::*;
use omsi_script::{Op, VarId};

/// What a car's scripts read of its place in the train, in the order [`CarRun`] sets them.
const PLACE_IN_TRAIN: [&str; 3] = ["train_frontcoupling", "train_backcoupling", "train_me_reverse"];

fn is_place_var(name: &str) -> bool {
    let name = name.trim();
    PLACE_IN_TRAIN.iter().any(|p| p.eq_ignore_ascii_case(name))
}

/// Whether a coupled part needs a [`CarRun`]: its program reads its place in the train, or
/// an animation, visibility or lamp of its model does.
pub(super) fn uses_place_in_train(ty: &VehicleType) -> bool {
    // (a car with a frame of its own and plugs of control cables reads what the cars ahead
    // send down them)
    let wired = !ty.def.script_share
        && !ty.program.frame.is_empty()
        && !(ty.def.control_cable_front.is_empty() && ty.def.control_cable_back.is_empty());
    let read_by_program = PLACE_IN_TRAIN.iter().filter_map(|n| ty.program.var(n)).any(|id| ty.program.reads(id));
    wired
        || read_by_program
        || ty.model.meshes.iter().any(|mesh| {
            let mut names = mesh.animations.iter().map(|a| a.variable.as_str());
            names.any(is_place_var)
                || mesh.visible.as_ref().is_some_and(|(name, _)| is_place_var(name))
                || mesh.light_enh.iter().map(|lamp| &lamp.variable).chain(mesh.light_enh_2.iter().map(|lamp| &lamp.variable)).any(|v| is_place_var(v))
        })
}

/// Tell each part of a train (in the train's order) what is coupled to it: the part before
/// it always, a part after it unless it is the last.
pub(super) fn set_couplings(parts: &mut [TrailerPart]) {
    let last = parts.len().saturating_sub(1);
    for (i, part) in parts.iter_mut().enumerate() {
        part.coupled_at = [true, i < last];
    }
}

/// How high a point of a car stands over the ground (`GetHeightAbovePoint`), at the car's place.
pub(super) type GroundHeightProbe = Arc<dyn Fn(f32, f32, f32) -> f32 + Send + Sync>;

/// What the control cables carry past one car, a signal per plug the car in front offers:
/// `(plug, number, value)`, the plug being the one a car receiving it must have (`C` for
/// `C`, `L` for the other's `R`).
pub(super) type CableBus = Vec<(char, i32, f32)>;

/// The plug `kind` meets: `L` and `R` pair up, `C` meets `C`.
fn mate(kind: &str) -> char {
    match kind.trim().chars().next().map(|c| c.to_ascii_uppercase()) {
        Some('L') => 'R',
        Some('R') => 'L',
        _ => 'C',
    }
}

fn plug(c: &omsi_vehicle::vehicle::ControlCable) -> Option<(char, i32)> {
    let number = c.lines.get(1)?.trim().parse::<i32>().ok()?;
    Some((c.lines.first()?.trim().chars().next()?.to_ascii_uppercase(), number))
}

/// The signals the leading vehicle sends down its back plug (what its script writes to the
/// `write` variable of each cable, the cable's coupling variable above 0.5): the bus the
/// first car behind it reads, set every frame before the cars run.
pub(super) fn start_bus(lead: &mut VehicleInstance) {
    let bus = lead
        .ty
        .def
        .control_cable_back
        .iter()
        .filter_map(|c| {
            let (kind, number) = plug(c)?;
            let coupled = c.lines.get(4).and_then(|n| lead.var(n)).is_none_or(|v| v > 0.5);
            let value = lead.var(c.lines.get(3)?)?;
            coupled.then_some((mate(&kind.to_string()), number, value))
        })
        .collect();
    lead.cable_bus = bus;
}

/// Per variable of `p`: whether {init}, {frame} or {frame_ai} (or a macro they call)
/// stores into it.
fn stored_vars(p: &Program) -> Vec<bool> {
    let mut stored = vec![false; p.var_names.len()];
    let mut visited = vec![false; p.blocks.len()];
    let mut pending: Vec<usize> = [&p.init, &p.frame, &p.frame_ai].into_iter().flatten().map(|&b| b as usize).collect();
    while let Some(block) = pending.pop() {
        match visited.get_mut(block) {
            Some(v) if !*v => *v = true,
            _ => continue,
        }
        for op in &p.blocks[block].ops {
            if let Op::Store(var) = op {
                if let Some(flag) = stored.get_mut(*var as usize) {
                    *flag = true;
                }
            } else if let Op::Macro(m) = op {
                pending.push(*m as usize);
            }
        }
    }
    stored
}

/// The leading vehicle's host as a car's own scripts see it: they may ask it anything (the
/// time, the weather, the depot), but what the leading vehicle's scripts do already does not
/// happen twice: no sounds, no messages, no change coins handed out (and no script texture
/// drawn, see [`quietly`]). The other callbacks only read.
struct SilentHost<'a>(&'a mut crate::host::VehicleHost);

impl omsi_script::Host for SilentHost<'_> {
    fn sys_var(&mut self, v: omsi_script::SysVar) -> f32 {
        omsi_script::Host::sys_var(self.0, v)
    }
    fn callback(&mut self, name: &str, id: omsi_script::NameId, stacks: &mut omsi_script::Stacks, state: &mut State) {
        if name.trim().eq_ignore_ascii_case("givechangecoin") {
            // (its coin index taken off the stack, as the call does)
            stacks.pop();
            return;
        }
        omsi_script::Host::callback(self.0, name, id, stacks, state);
    }
    fn sound_trigger(&mut self, _name: &str, _id: omsi_script::NameId) {}
    fn sound_trigger_vars(&mut self, _name: &str, _id: omsi_script::NameId, _vars: &[f32]) {}
    fn message(&mut self, _text: &str) {}
}

/// Run `f` with the leading vehicle's host made silent; its script textures are not drawn
/// by a car's run either (the drawing callbacks see them as another machine's).
fn quietly(lead: &mut VehicleInstance, f: impl FnOnce(&mut SilentHost)) {
    let remote_before = std::mem::replace(&mut lead.host.remote_script_textures, true);
    f(&mut SilentHost(&mut lead.host));
    lead.host.remote_script_textures = remote_before;
}

/// A train car's scripts with variables of its own (see the module).
pub struct CarRun {
    prog: Arc<Program>,
    pub(super) state: State,
    machine: Vm,
    /// Name (lower case) to slot: the car's program's variables first, then the leading
    /// vehicle's that the program lacks.
    pub(super) names: Arc<HashMap<String, VarId>>,
    /// Per slot of the car's program: its scripts store there, so it is not refreshed.
    local: Vec<bool>,
    /// (slot here, slot in the leading vehicle) refreshed every frame.
    shared: Vec<(VarId, VarId)>,
    /// The leading vehicle's variable count `shared` was worked out for.
    lead_len: Option<usize>,
    /// Slots of [`PLACE_IN_TRAIN`] here.
    place_slots: [Option<VarId>; 3],
}

impl CarRun {
    /// Start a car's scripts: every value as the leading vehicle has it, the place in the
    /// train (`place`, as [`TrailerPart::place_in_train`] gives it) set, {init} run.
    pub(super) fn start(ty: &VehicleType, lead: &mut VehicleInstance, place: [f32; 3]) -> CarRun {
        let prog = ty.program.clone();
        let names: HashMap<String, VarId> =
            prog.var_names.iter().zip(0..).map(|(name, slot)| (name.to_ascii_lowercase(), slot)).collect();
        let mut state = State::new(&prog);
        // strings come from the leading vehicle too (its number, its destination)
        for (slot, name) in prog.str_var_names.iter().enumerate() {
            let lead_str = lead.ty.program.str_var(name).and_then(|k| lead.state.str_vars.get(k as usize));
            if let Some(text) = lead_str {
                state.str_vars[slot] = text.clone();
            }
        }
        let mut run = CarRun {
            local: stored_vars(&prog),
            prog,
            state,
            machine: Vm::new(),
            names: Arc::new(names),
            shared: Vec::new(),
            lead_len: None,
            place_slots: [None; 3],
        };
        run.sync_names(lead);
        let starting: Vec<(VarId, VarId)> =
            run.names.iter().filter_map(|(name, &slot)| lead.var_index.get(name).map(|&from| (slot, from))).collect();
        for (slot, from) in starting {
            run.state.vars[slot as usize] = lead.state.vars[from as usize];
        }
        run.write_place(place);
        let (prog, state, machine) = (run.prog.clone(), &mut run.state, &mut run.machine);
        quietly(lead, |host| machine.run_init(&prog, state, host));
        // ({init} puts the defaults the paint's variables are meant to replace)
        if let Some(scheme) = lead.host.paint_scheme {
            run.apply_paint(ty, scheme);
        }
        run
    }

    /// The paint scheme `scheme` of the leading vehicle, as this car's own variables take it:
    /// `Colorscheme` and the scheme's `[setvar]`s of the car's own paints (the same number).
    pub(super) fn apply_paint(&mut self, ty: &VehicleType, scheme: Option<usize>) {
        let scheme = scheme.filter(|i| *i < ty.paint_schemes.len());
        let mut put = |name: &str, v: f32| {
            if let Some(slot) = omsi_script::compile::with_lower(name, |k| self.names.get(k).copied()) {
                self.state.vars[slot as usize] = v;
            }
        };
        put("Colorscheme", scheme.map(|i| i as f32).unwrap_or(-1.0));
        if let Some(i) = scheme {
            for (name, v) in &ty.paint_schemes[i].set_vars {
                put(name, *v);
            }
        }
    }

    /// Whether this car's scripts store into `name` (what they leave alone is the leading
    /// vehicle's, or what a cable brings).
    fn stores(&self, name: &str) -> bool {
        omsi_script::compile::with_lower(name, |k| self.names.get(k).copied())
            .is_some_and(|slot| self.local.get(slot as usize) == Some(&true))
    }

    /// Put what the cars ahead send into this car's `read` variables: the plugs facing the
    /// train's front (`facing`) take the bus's signals of their kind and number.
    fn read_cables(&mut self, facing: &[omsi_vehicle::vehicle::ControlCable], bus: &CableBus) {
        for c in facing {
            let Some((kind, number)) = plug(c) else { continue };
            let Some(&(_, _, value)) = bus.iter().find(|(k, n, _)| *k == kind && *n == number) else { continue };
            if let Some(slot) = c.lines.get(2).and_then(|n| omsi_script::compile::with_lower(n, |k| self.names.get(k).copied())) {
                self.state.vars[slot as usize] = value;
            }
        }
    }

    /// What this car sends on down its far plug: what its script wrote to a cable's `write`
    /// variable, or - where the script leaves it alone, as a coupler does - what the cable
    /// brought in.
    fn write_cables(&self, far: &[omsi_vehicle::vehicle::ControlCable]) -> CableBus {
        far.iter()
            .filter_map(|c| {
                let (kind, number) = plug(c)?;
                let (read, write) = (c.lines.get(2)?, c.lines.get(3)?);
                let coupled = c.lines.get(4).and_then(|n| self.get(n)).is_none_or(|v| v > 0.5);
                let value = if self.stores(write) { self.get(write)? } else { self.get(read)? };
                coupled.then_some((mate(&kind.to_string()), number, value))
            })
            .collect()
    }

    /// When the leading vehicle has gained variables: give this set the names it lacks and
    /// work out again which slots are refreshed from it.
    fn sync_names(&mut self, lead: &VehicleInstance) {
        if self.lead_len == Some(lead.var_index.len()) {
            return;
        }
        self.lead_len = Some(lead.var_index.len());
        let names = Arc::make_mut(&mut self.names);
        for name in lead.var_index.keys() {
            if names.get(name).is_none() {
                let slot = self.state.vars.len() as VarId;
                self.state.vars.push(0.0);
                names.insert(name.clone(), slot);
            }
        }
        let mut shared = Vec::with_capacity(names.len());
        for (name, &slot) in names.iter() {
            let stored_here = self.local.get(slot as usize) == Some(&true);
            if let (false, Some(&from)) = (stored_here, lead.var_index.get(name)) {
                shared.push((slot, from));
            }
        }
        shared.sort_unstable();
        self.shared = shared;
        self.place_slots = PLACE_IN_TRAIN.map(|name| names.get(name).copied());
    }

    fn write_place(&mut self, place: [f32; 3]) {
        for (slot, value) in self.place_slots.into_iter().zip(place) {
            if let Some(slot) = slot {
                self.state.vars[slot as usize] = value;
            }
        }
    }

    /// One frame of the car: refreshed from the leading vehicle, its place set, its
    /// {frame} run ({frame_ai} for an AI train that has one, as its leading car runs).
    ///
    /// `plugs`: the car's control cable plugs, facing the train's front and away from it;
    /// `probe`: `GetHeightAbovePoint` at the car's own place.
    pub(super) fn frame(&mut self, lead: &mut VehicleInstance, place: [f32; 3], plugs: [&[omsi_vehicle::vehicle::ControlCable]; 2], probe: Option<GroundHeightProbe>) {
        self.sync_names(lead);
        let lead_vars = &lead.state.vars;
        for &(slot, from) in &self.shared {
            self.state.vars[slot as usize] = lead_vars[from as usize];
        }
        self.write_place(place);
        let bus = std::mem::take(&mut lead.cable_bus);
        self.read_cables(plugs[0], &bus);
        let as_ai = !self.prog.frame_ai.is_empty() && lead.ev(Ev::Ai).unwrap_or(0.0) > 0.5;
        let (prog, state, machine) = (self.prog.clone(), &mut self.state, &mut self.machine);
        let own_probe = probe.map(|p| std::mem::replace(&mut lead.host.ground_probe, Some(p)));
        quietly(lead, |host| {
            if as_ai {
                machine.run_frame_ai(&prog, state, host);
            } else {
                machine.run_frame(&prog, state, host);
            }
        });
        if let Some(before) = own_probe {
            lead.host.ground_probe = before;
        }
        lead.cable_bus = self.write_cables(plugs[1]);
        if !self.has_cables(plugs) {
            // (a car with no plugs lets the signals by)
            lead.cable_bus = bus;
        }
    }

    fn has_cables(&self, plugs: [&[omsi_vehicle::vehicle::ControlCable]; 2]) -> bool {
        !(plugs[0].is_empty() && plugs[1].is_empty())
    }

    /// The car's value of variable `name`.
    pub fn get(&self, name: &str) -> Option<f32> {
        let slot = omsi_script::compile::with_lower(name, |k| self.names.get(k).copied())?;
        Some(self.state.vars[slot as usize])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lamp variables of the test car, in the order of its lamps: front white, front red,
    /// rear white, rear red.
    const LAMPS: [&str; 4] = ["lamp_front_white", "lamp_front_red", "lamp_rear_white", "lamp_rear_red"];

    /// A multiple-unit car: each free end shows white where the car leads and red where it
    /// trails, from its own place in the train.
    fn unit_car() -> Arc<VehicleType> {
        static MADE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = MADE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("omsi-car-run-test-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("cab_ends.osc");
        let list = dir.join("cab_ends_varlist.txt");
        std::fs::write(
            &script,
            "{frame}\n\
             (L.L.train_me_reverse) (S.L.turned)\n\
             0 (S.L.lamp_front_white) (S.L.lamp_front_red) (S.L.lamp_rear_white) (S.L.lamp_rear_red)\n\
             (L.L.train_frontcoupling) 0 = {if}\n\
               (L.L.turned) {if} 1 (S.L.lamp_front_red) {else} 1 (S.L.lamp_front_white) {endif}\n\
             {endif}\n\
             (L.L.train_backcoupling) 0 = {if}\n\
               (L.L.turned) {if} 1 (S.L.lamp_rear_white) {else} 1 (S.L.lamp_rear_red) {endif}\n\
             {endif}\n\
             {end}\n",
        )
        .unwrap();
        let mut names = PLACE_IN_TRAIN.join("\n");
        for extra in ["turned"].iter().chain(&LAMPS) {
            names.push('\n');
            names.push_str(extra);
        }
        std::fs::write(&list, names).unwrap();
        let program = omsi_script::compile(&CompileInput { varlists: vec![list], scripts: vec![script], ..Default::default() });
        std::fs::remove_dir_all(&dir).ok();
        assert!(program.errors.is_empty(), "{:?}", program.errors);
        let mut ty = super::super::tests::coupling_test_type(Some(-4.0));
        let t = Arc::get_mut(&mut ty).unwrap();
        t.program = Arc::new(program);
        let lamp_at = |i: usize| omsi_model::LightEnh2 {
            pos: [0.4, if i < 2 { 7.5 } else { -7.5 }, 1.2],
            omni: true,
            color: if i % 2 == 0 { [250.0, 250.0, 240.0] } else { [250.0, 20.0, 10.0] },
            size: 0.15,
            factor: 1.0,
            variable: LAMPS[i].to_string(),
            ..Default::default()
        };
        t.model.meshes = vec![omsi_model::MeshDef { light_enh_2: (0..4).map(lamp_at).collect(), ..Default::default() }];
        ty
    }

    fn run_frames(v: &mut VehicleInstance, n: usize) {
        for _ in 0..n {
            v.update_dirt(0.0);
            v.update_scripts_only(0.0);
            v.update_visuals(0.04);
        }
    }

    /// Which of a car's four lamps are lit, as `LAMPS` orders them.
    fn lit(fades: &[f32]) -> [bool; 4] {
        std::array::from_fn(|k| fades.get(k).is_some_and(|&f| f > 0.5))
    }

    /// Four cars, the second and the fourth turned round: white only at the head of the
    /// train, red only at its tail, every car between dark. Two cars uncoupled, the
    /// second car (turned round) is the tail and shows red at its own front.
    #[test]
    fn turned_cars_light_the_end_they_stand_at() {
        let car = unit_car();
        let mut v = VehicleInstance::new(car.clone(), crate::host::VehicleHost::new(Default::default()));
        for turned in [true, false, true] {
            v.attach_trailer_ex(car.clone(), turned);
        }
        run_frames(&mut v, 4);
        assert_eq!(lit(&v.light_fade), [true, false, false, false], "head car");
        for (k, part) in v.trailers.iter().enumerate() {
            assert!(part.run.is_some(), "car {k} runs its own scripts");
        }
        assert_eq!(lit(&v.trailers[0].light_fade), [false; 4]);
        assert_eq!(lit(&v.trailers[1].light_fade), [false; 4]);
        assert_eq!(lit(&v.trailers[2].light_fade), [false, true, false, false], "tail car, turned round");
        assert_eq!(v.trailers[2].run_var("train_backcoupling"), Some(1.0));
        assert_eq!(v.var("lamp_front_red"), Some(0.0), "the head car's values stay its own");
        v.detach_last_trailer();
        v.detach_last_trailer();
        run_frames(&mut v, 4);
        assert_eq!(lit(&v.trailers[0].light_fade), [false, true, false, false], "second car is the tail now");
        assert_eq!(lit(&v.light_fade), [true, false, false, false]);
    }

    #[test]
    fn cars_without_place_variables_need_no_run() {
        let plain = super::super::tests::coupling_test_type(None);
        assert!(!uses_place_in_train(&plain));
        assert!(uses_place_in_train(&unit_car()));
    }
}
