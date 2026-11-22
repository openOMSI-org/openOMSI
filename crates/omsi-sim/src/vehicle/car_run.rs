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
const PLACE_IN_TRAIN: [&str; 3] = [
    "train_frontcoupling",
    "train_backcoupling",
    "train_me_reverse",
];

/// Variables (lower case) every car takes from the leading vehicle even where its scripts
/// store them: the controls of the cab driven from and how the train moves. (`ai_*` too.)
const DRIVEN_BY_ENGINE: [&str; 10] = [
    "ai",
    "throttle",
    "brake",
    "clutch",
    "velocity",
    "velocity_ground",
    "a_trans_x",
    "a_trans_y",
    "a_trans_z",
    "colorscheme",
];

fn is_place_var(name: &str) -> bool {
    let name = name.trim();
    PLACE_IN_TRAIN.iter().any(|p| p.eq_ignore_ascii_case(name))
}

/// Whether a coupled part needs a [`CarRun`]: it has scripts of its own (a frame, and no
/// `[scriptshare]` with the car in front), its program reads its place in the train, or an
/// animation, visibility or lamp of its model does. Omsi.exe runs every car's own scripts,
/// with or without control cables: its motor, brakes and the rest.
pub(super) fn uses_place_in_train(ty: &VehicleType) -> bool {
    let wired = !ty.def.script_share && !ty.program.frame.is_empty();
    let read_by_program = PLACE_IN_TRAIN
        .iter()
        .filter_map(|n| ty.program.var(n))
        .any(|id| ty.program.reads(id));
    wired
        || read_by_program
        || ty.model.meshes.iter().any(|mesh| {
            let mut names = mesh.animations.iter().map(|a| a.variable.as_str());
            names.any(is_place_var)
                || mesh
                    .visible
                    .as_ref()
                    .is_some_and(|(name, _)| is_place_var(name))
                || mesh
                    .light_enh
                    .iter()
                    .map(|lamp| &lamp.variable)
                    .chain(mesh.light_enh_2.iter().map(|lamp| &lamp.variable))
                    .any(|v| is_place_var(v))
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

/// One `[control_cable_*]` line of a car's plug: its variables' slots, None where the car has
/// no such variable.
#[derive(Debug, Clone, Copy, Default)]
struct Wire {
    read: Option<VarId>,
    write: Option<VarId>,
    link: Option<VarId>,
}

/// A car's control cables as Omsi.exe keeps them (`TVehicle`, 0x544): per end (0 the car's
/// front, 1 its back) and kind (0 `L`, 1 `C`, 2 `R`) the wires by their number.
type Plugs = [[Vec<Wire>; 3]; 2];

/// The highest wire number a plug may have: a vehicle file's number sizes a list, and a number
/// far beyond any real plug's would take all the memory there is.
const MAX_WIRE: usize = 255;

fn plugs_of(def: &omsi_vehicle::Vehicle, var: impl Fn(&str) -> Option<VarId>) -> Plugs {
    let mut plugs = Plugs::default();
    for (end, cables) in [&def.control_cable_front, &def.control_cable_back]
        .into_iter()
        .enumerate()
    {
        for c in cables {
            let kind = match c
                .lines
                .first()
                .and_then(|k| k.trim().chars().next())
                .map(|k| k.to_ascii_uppercase())
            {
                Some('L') => 0,
                Some('C') => 1,
                Some('R') => 2,
                _ => continue,
            };
            let Some(number) = c.lines.get(1).and_then(|n| n.trim().parse::<usize>().ok()) else {
                continue;
            };
            if number > MAX_WIRE {
                log::warn!(
                    "{}: control cable number {number} is over {MAX_WIRE}; left out",
                    def.path.display()
                );
                continue;
            }
            let wires = &mut plugs[end][kind];
            if wires.len() <= number {
                wires.resize(number + 1, Wire::default());
            }
            let slot = |i: usize| c.lines.get(i).and_then(|n| var(n));
            wires[number] = Wire {
                read: slot(2),
                write: slot(3),
                link: slot(4),
            };
        }
    }
    plugs
}

/// The train's control cables, as Omsi.exe runs them before the scripts of a frame
/// (0x7d3ef8, 0x7d4020). A cable is a wire, and whatever is on it is the largest value put on
/// it: a car's `read` variable starts from 0 (so a negative value reads 0), takes the car's
/// own `write` variable if that is larger, and then what the coupled cars' `read` variables
/// hold, walking from the first car to the last and back. So every car reads the largest
/// `write` of all the cars on the wire, its own among them, however many cabs the train has.
/// No car owns a wire. Two cars say different things to each other with two wires: the `L`
/// of a plug is coupled to the other plug's `R`, and `C` to `C`. Two coupled wires connect
/// only when both have a read and a write variable and neither has a link variable at 0.5 or
/// below (a cable without one, as on `main`, is always linked); a car that does not name a
/// wire ends it.
#[derive(Debug, Default)]
pub(super) struct Cables {
    /// The leading vehicle's plugs, for the program they were made for.
    lead_plugs: Option<(usize, Arc<Plugs>)>,
    /// What the leading vehicle's `read` variables take at its next frame.
    pub(super) values: Vec<(VarId, f32)>,
    /// What was last logged (`OMSI_DEBUG_CABLES`).
    logged: HashMap<(usize, VarId), f32>,
    /// What was last logged (`OMSI_DEBUG_CAR_VARS`), and how many times.
    watched: HashMap<(usize, bool, usize), (String, u32)>,
    /// Working storage of [`run_cables`], kept from frame to frame: the coupled parts on the
    /// wires and each car's `read` variables so far (the leading vehicle first).
    wired: Vec<usize>,
    reads: Vec<HashMap<VarId, f32>>,
}

/// A car on the wires for one run of [`run_cables`]; what its `read` variables hold so far is
/// kept beside it, in [`Cables::reads`].
#[derive(Clone, Copy)]
struct WiredCar<'a> {
    plugs: &'a Plugs,
    vars: &'a [f32],
    /// Which variables are the car's own (None: all of them).
    own: Option<&'a [bool]>,
    reversed: bool,
}

impl WiredCar<'_> {
    /// Every `read` variable of the car starts from 0.
    fn start(&self, read: &mut HashMap<VarId, f32>) {
        read.clear();
        for slot in self.plugs.iter().flatten().flatten().filter_map(|w| w.read) {
            read.insert(slot, 0.0);
        }
    }

    fn value(&self, read: &HashMap<VarId, f32>, slot: VarId) -> f32 {
        read.get(&slot)
            .copied()
            .unwrap_or_else(|| self.vars.get(slot as usize).copied().unwrap_or(0.0))
    }

    /// Each `read` variable takes the car's own `write` variable. (A variable of the leading
    /// vehicle a car's scripts have not stored yet is not on the car's wire.)
    fn own_wires(&self, read: &mut HashMap<VarId, f32>) {
        for w in self.plugs.iter().flatten().flatten() {
            let (Some(r), Some(write)) = (w.read, w.write) else {
                continue;
            };
            if self
                .own
                .is_some_and(|own| own.get(write as usize) != Some(&true))
            {
                continue;
            }
            let v = self.value(read, write).max(self.value(read, r));
            read.insert(r, v);
        }
    }

    fn live(&self, read: &HashMap<VarId, f32>, w: &Wire) -> bool {
        w.read.is_some()
            && w.write.is_some()
            && w.link.is_none_or(|link| self.value(read, link) > 0.5)
    }
}

/// What `from`'s `read` variables hold goes into the ones of the car it is coupled to at its
/// `end` (0: towards the first car, 1: towards the last).
fn cross_coupling(
    (from, from_read): (&WiredCar, &HashMap<VarId, f32>),
    (to, to_read): (&WiredCar, &mut HashMap<VarId, f32>),
    end: usize,
) {
    let side = |car: &WiredCar, toward_last: bool| usize::from(toward_last != car.reversed);
    let (near, far) = (side(from, end == 1), side(to, end == 0));
    for kind in 0..3 {
        for (a, b) in from.plugs[near][kind].iter().zip(&to.plugs[far][2 - kind]) {
            if from.live(from_read, a) && to.live(to_read, b) {
                let (Some(here), Some(there)) = (a.read, b.read) else {
                    continue;
                };
                let v = from.value(from_read, here).max(to.value(to_read, there));
                to_read.insert(there, v);
            }
        }
    }
}

/// After the cars ran: what every car's `read` variables hold at its next frame (the leading
/// vehicle: [`apply_lead_cables`]; a part: [`CarRun::frame`]). A part without wires or
/// scripts of its own is not in the way. `parts`: the train's coupled parts. It runs every
/// frame, so it keeps its working storage in `lead.cables` and builds nothing where no car
/// has a cable.
pub(super) fn run_cables(lead: &mut VehicleInstance, parts: &mut [TrailerPart]) {
    if let Some(want) = omsi_cfg::flags::OMSI_DEBUG_CAR_VARS.var() {
        let want: Vec<String> = want
            .to_string()
            .split(',')
            .map(|w| w.trim().to_ascii_lowercase())
            .filter(|w| !w.is_empty())
            .collect();
        let mut watched = std::mem::take(&mut lead.cables.watched);
        log_car_vars(
            &mut watched,
            &want,
            0,
            &lead.ty,
            &lead.ty.program,
            &lead.state,
        );
        for (k, t) in parts.iter().enumerate() {
            if let Some(run) = t.run.as_deref() {
                log_car_vars(&mut watched, &want, k + 1, &t.ty, &run.prog, &run.state);
            }
        }
        lead.cables.watched = watched;
    }
    let id = Arc::as_ptr(&lead.ty.program) as usize;
    if !matches!(&lead.cables.lead_plugs, Some((was, _)) if *was == id) {
        let plugs = plugs_of(&lead.ty.def, |n| lead.ty.program.var(n));
        lead.cables.lead_plugs = Some((id, Arc::new(plugs)));
    }
    let Cables {
        lead_plugs,
        values,
        logged,
        wired,
        reads,
        ..
    } = &mut lead.cables;
    let lead_plugs = &lead_plugs.as_ref().expect("made above").1;
    wired.clear();
    wired.extend(
        (0..parts.len()).filter(|&k| parts[k].run.as_deref().is_some_and(|r| r.has_cables)),
    );
    // (no cable anywhere: nothing to put on a wire, nothing for any car to read)
    let lead_wired = lead_plugs
        .iter()
        .flatten()
        .flatten()
        .any(|w| w.read.is_some());
    if wired.is_empty() && !lead_wired {
        values.clear();
        return;
    }
    let (lead_vars, wired) = (&lead.state.vars, &*wired);
    let car = |k: usize| -> WiredCar<'_> {
        match k.checked_sub(1) {
            None => WiredCar {
                plugs: lead_plugs,
                vars: lead_vars,
                own: None,
                reversed: false,
            },
            Some(i) => {
                let part = &parts[wired[i]];
                let run = part
                    .run
                    .as_deref()
                    .expect("a wired part runs its own scripts");
                WiredCar {
                    plugs: &run.plugs,
                    vars: &run.state.vars,
                    own: Some(&run.local),
                    reversed: part.reversed,
                }
            }
        }
    };
    let n = wired.len() + 1;
    if reads.len() < n {
        reads.resize_with(n, HashMap::new);
    }
    for (k, read) in reads.iter_mut().take(n).enumerate() {
        let c = car(k);
        c.start(read);
        c.own_wires(read);
    }
    for i in 1..n {
        let (done, rest) = reads.split_at_mut(i);
        cross_coupling((&car(i - 1), &done[i - 1]), (&car(i), &mut rest[0]), 1);
    }
    for i in (1..n).rev() {
        let (rest, done) = reads.split_at_mut(i);
        cross_coupling((&car(i), &done[0]), (&car(i - 1), &mut rest[i - 1]), 0);
    }
    if omsi_cfg::flags::OMSI_DEBUG_CABLES.is_set() {
        log_wires(logged, &lead.ty, parts, wired, &reads[..n]);
    }
    values.clear();
    values.extend(reads[0].iter().map(|(&slot, &v)| (slot, v)));
    for (&k, read) in wired.iter().zip(&reads[1..n]) {
        if let Some(run) = parts[k].run.as_deref_mut() {
            run.lines.clear();
            run.lines.extend(read.iter().map(|(&slot, &v)| (slot, v)));
        }
    }
}

/// The changes a variable may log (`OMSI_DEBUG_CAR_VARS`): a timer changes every frame.
const WATCHED_CHANGES: u32 = 60;

/// Log what changed of the variables and strings of car `car` (0 the leading vehicle, `k + 1`
/// coupled part `k`) named like `want` (`OMSI_DEBUG_CAR_VARS`).
fn log_car_vars(
    watched: &mut HashMap<(usize, bool, usize), (String, u32)>,
    want: &[String],
    car: usize,
    ty: &VehicleType,
    prog: &Program,
    state: &State,
) {
    let file = ty
        .def
        .path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let wanted = |name: &str| {
        let name = name.to_ascii_lowercase();
        want.iter().any(|w| name.contains(w.as_str()))
    };
    let numbers = prog
        .var_names
        .iter()
        .enumerate()
        .filter(|(_, n)| wanted(n))
        .map(|(i, n)| ((false, i), n, state.vars.get(i).map(f32::to_string)));
    let strings = prog
        .str_var_names
        .iter()
        .enumerate()
        .filter(|(_, n)| wanted(n))
        .map(|(i, n)| {
            (
                (true, i),
                n,
                state.str_vars.get(i).map(|s| format!("{s:?}")),
            )
        });
    for ((text, i), name, value) in numbers.chain(strings) {
        let Some(value) = value else { continue };
        let seen = watched
            .entry((car, text, i))
            .or_insert_with(|| (String::new(), 0));
        if seen.0 != value && seen.1 <= WATCHED_CHANGES {
            seen.0.clone_from(&value);
            seen.1 += 1;
            if seen.1 > WATCHED_CHANGES {
                log::info!("var: {file} (car {car}) {name}: changes too often, no longer logged");
            } else {
                log::info!("var: {file} (car {car}) {name} = {value}");
            }
        }
    }
}

/// Log each `read` variable of the train that changed (`OMSI_DEBUG_CABLES`).
fn log_wires(
    logged: &mut HashMap<(usize, VarId), f32>,
    lead: &VehicleType,
    parts: &[TrailerPart],
    wired: &[usize],
    read: &[HashMap<VarId, f32>],
) {
    for (car, lines) in read.iter().enumerate() {
        let ty = if car == 0 {
            lead
        } else {
            &parts[wired[car - 1]].ty
        };
        for (&slot, &value) in lines {
            if logged.insert((car, slot), value) != Some(value) {
                let file = ty
                    .def
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                let name = ty
                    .program
                    .var_names
                    .get(slot as usize)
                    .map_or("?", String::as_str);
                log::info!("cable: {file} (car {car}) {name} = {value}");
            }
        }
    }
}

/// Put what the cars behind sent into the leading vehicle's `read` variables, over its own
/// `write` variables already in them (`run_control_cables`).
pub(super) fn apply_lead_cables(lead: &mut VehicleInstance) {
    for i in 0..lead.cables.values.len() {
        let (slot, value) = lead.cables.values[i];
        if let Some(v) = lead.state.vars.get_mut(slot as usize) {
            *v = v.max(value);
        }
    }
}

/// Per variable of `p`: whether {init}, {frame} or {frame_ai} (or a macro they call)
/// stores into it.
fn stored_vars(p: &Program) -> Vec<bool> {
    stored_by(
        p,
        [&p.init, &p.frame, &p.frame_ai]
            .into_iter()
            .flatten()
            .map(|&b| b as usize)
            .collect(),
    )
}

/// Per variable of `p`: whether the blocks `pending` (or a macro they call) store into it.
fn stored_by(p: &Program, mut pending: Vec<usize>) -> Vec<bool> {
    let mut stored = vec![false; p.var_names.len()];
    let mut visited = vec![false; p.blocks.len()];
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
/// happen twice: no messages, no change coins handed out (and no script texture drawn, see
/// [`quietly`]). The other callbacks only read. The sounds its scripts start go to the car's
/// own list (`CarRun::fired_sounds`), for its own sound set: the NF6D's rear cab clicked
/// and rang for nobody.
struct SilentHost<'a>(&'a mut crate::host::VehicleHost, &'a mut FiredSounds);

/// What a car's scripts started since the game last asked: the triggers, and the `(T.F.x)`
/// ones with the file they name (the NF6D's rear cab's BKK FUTAR says its stops with these).
#[derive(Default)]
struct FiredSounds {
    triggers: Vec<String>,
    files: Vec<(String, String)>,
}

impl omsi_script::Host for SilentHost<'_> {
    fn sys_var(&mut self, v: omsi_script::SysVar) -> f32 {
        omsi_script::Host::sys_var(self.0, v)
    }
    fn callback(
        &mut self,
        name: &str,
        id: omsi_script::NameId,
        stacks: &mut omsi_script::Stacks,
        state: &mut State,
    ) {
        if name.trim().eq_ignore_ascii_case("givechangecoin") {
            // (its coin index taken off the stack, as the call does)
            stacks.pop();
            return;
        }
        omsi_script::Host::callback(self.0, name, id, stacks, state);
    }
    fn sound_trigger(&mut self, name: &str, _id: omsi_script::NameId) {
        self.1.triggers.push(name.to_string());
    }
    fn sound_trigger_vars(&mut self, name: &str, _id: omsi_script::NameId, _vars: &[f32]) {
        self.1.triggers.push(name.to_string());
    }
    fn sound_trigger_file(&mut self, name: &str, file: &str) {
        // (as the leading vehicle's host: `" " (T.F.x)` plays nothing)
        if !file.trim().is_empty() {
            self.1.files.push((name.to_string(), file.to_string()));
        }
    }
    fn message(&mut self, _text: &str) {}
}

/// Run `f` with the leading vehicle's host made silent (the sounds into `sounds`); its
/// script textures are not drawn by a car's run either (the drawing callbacks see them as
/// another machine's).
fn quietly(lead: &mut VehicleInstance, sounds: &mut FiredSounds, f: impl FnOnce(&mut SilentHost)) {
    let remote_before = std::mem::replace(&mut lead.host.remote_script_textures, true);
    f(&mut SilentHost(&mut lead.host, sounds));
    lead.host.remote_script_textures = remote_before;
}

/// [`quietly`], but a car with `[scripttexture]`s of its own (`own`, see
/// `TrailerPart::script_textures`) draws into them: they stand in for the leading
/// vehicle's while `f` runs.
fn quietly_drawing(
    lead: &mut VehicleInstance,
    own: &mut Vec<crate::scripttex::ScriptTexture>,
    sounds: &mut FiredSounds,
    f: impl FnOnce(&mut SilentHost),
) {
    if own.is_empty() {
        return quietly(lead, sounds, f);
    }
    std::mem::swap(&mut lead.host.script_textures, own);
    let remote_before = std::mem::replace(&mut lead.host.remote_script_textures, false);
    f(&mut SilentHost(&mut lead.host, sounds));
    lead.host.remote_script_textures = remote_before;
    std::mem::swap(&mut lead.host.script_textures, own);
}

/// A train car's scripts with variables of its own (see the module).
pub struct CarRun {
    pub(super) prog: Arc<Program>,
    pub(super) state: State,
    machine: Vm,
    /// Name (lower case) to slot: the car's program's variables first, then the leading
    /// vehicle's that the program lacks.
    pub(super) names: Arc<HashMap<String, VarId>>,
    /// Per slot of the car's program: its scripts store there, so it is not refreshed.
    pub(super) local: Vec<bool>,
    /// (slot here, slot in the leading vehicle) refreshed every frame.
    pub(super) shared: Vec<(VarId, VarId)>,
    /// The leading vehicle's variable count `shared` was worked out for.
    pub(super) lead_len: Option<usize>,
    /// Slots of [`PLACE_IN_TRAIN`] here.
    place_slots: [Option<VarId>; 3],
    /// The car's control cables (see [`Plugs`]).
    plugs: Plugs,
    /// It has any: a car without is not on the train's wires.
    has_cables: bool,
    /// `[scriptshare]`: the leading vehicle's variables are its variables, those its
    /// scripts hold too. It runs only to read its place in the train.
    pub(super) script_share: bool,
    /// What its `read` variables take at its next frame (see [`Cables`]).
    lines: Vec<(VarId, f32)>,
    /// The sound triggers its scripts fired since the game last took them, for the car's own
    /// sound set (see [`CarRun::take_fired_sounds`]).
    fired_sounds: FiredSounds,
    paint_base: Vec<(String, VarId, f32)>,
}

impl CarRun {
    /// The sound triggers the car's scripts fired since this was last asked (the game plays
    /// them in the car's own sound set, where it has one).
    pub fn take_fired_sounds(&mut self) -> Vec<String> {
        std::mem::take(&mut self.fired_sounds.triggers)
    }

    /// A string variable of the car's own scripts, where they have it.
    fn set_str_var(&mut self, name: &str, value: &str) {
        if let Some(i) = self.prog.str_var(name) {
            self.state.str_vars[i as usize] = value.to_string();
        }
    }

    /// The `(T.F.x)` sounds the car's scripts fired since this was last asked: trigger and
    /// file, for the car's own sound set.
    pub fn take_fired_files(&mut self) -> Vec<(String, String)> {
        std::mem::take(&mut self.fired_sounds.files)
    }

    /// Start a car's scripts: every value as the leading vehicle has it, the place in the
    /// train (`place`, as [`TrailerPart::place_in_train`] gives it) set, {init} run (drawing
    /// into `pictures`, the car's own script textures, where it has them).
    pub(super) fn start(
        ty: &VehicleType,
        lead: &mut VehicleInstance,
        place: [f32; 3],
        pictures: &mut Vec<crate::scripttex::ScriptTexture>,
    ) -> CarRun {
        let prog = ty.program.clone();
        let names: HashMap<String, VarId> = prog
            .var_names
            .iter()
            .zip(0..)
            .map(|(name, slot)| (name.to_ascii_lowercase(), slot))
            .collect();
        let mut state = State::new(&prog);
        // strings come from the leading vehicle too (its number, its destination)
        for (slot, name) in prog.str_var_names.iter().enumerate() {
            let lead_str = lead
                .ty
                .program
                .str_var(name)
                .and_then(|k| lead.state.str_vars.get(k as usize));
            if let Some(text) = lead_str {
                state.str_vars[slot] = text.clone();
            }
        }
        let plugs = plugs_of(&ty.def, |n| prog.var(n));
        let has_cables =
            !(ty.def.control_cable_front.is_empty() && ty.def.control_cable_back.is_empty());
        let mut run = CarRun {
            plugs,
            has_cables,
            script_share: ty.def.script_share,
            local: stored_vars(&prog),
            prog,
            state,
            machine: Vm::new(),
            names: Arc::new(names),
            shared: Vec::new(),
            lead_len: None,
            place_slots: [None; 3],
            lines: Vec::new(),
            fired_sounds: FiredSounds::default(),
            paint_base: Vec::new(),
        };
        run.sync_names(lead);
        for (slot, from) in run.shared.clone() {
            run.state.vars[slot as usize] = lead.state.vars[from as usize];
        }
        run.write_place(place);
        let (prog, state, machine) = (run.prog.clone(), &mut run.state, &mut run.machine);
        quietly_drawing(lead, pictures, &mut run.fired_sounds, |host| {
            machine.run_init(&prog, state, host)
        });
        run.paint_base = super::instance::scheme_var_names(ty)
            .into_iter()
            .filter_map(|k| {
                let slot = *run.names.get(&k)?;
                let now = run.state.vars[slot as usize];
                let lead_base = lead
                    .paint_base
                    .iter()
                    .find(|(n, _)| *n == k)
                    .map(|(_, v)| *v);
                Some((
                    k,
                    slot,
                    if run.prog.init_stores(slot) {
                        now
                    } else {
                        lead_base.unwrap_or(now)
                    },
                ))
            })
            .collect();
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
            if let Some(slot) =
                omsi_script::compile::with_lower(name, |k| self.names.get(k).copied())
            {
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

    pub(super) fn take_own_brakeforce(&mut self) -> f32 {
        if !self.stores("Brakeforce") {
            return 0.0;
        }
        let Some(slot) =
            omsi_script::compile::with_lower("Brakeforce", |k| self.names.get(k).copied())
        else {
            return 0.0;
        };
        std::mem::take(&mut self.state.vars[slot as usize]).max(0.0)
    }

    pub(super) fn release_paint(
        &mut self,
        ty: &VehicleType,
        old: Option<usize>,
        new: Option<usize>,
    ) {
        for (name, slot, v) in &self.paint_base {
            if super::instance::scheme_sets(ty, old, name)
                && !super::instance::scheme_sets(ty, new, name)
            {
                self.state.vars[*slot as usize] = *v;
            }
        }
    }

    /// Whether this car's scripts store into `name` (what they leave alone is the leading
    /// vehicle's, or what a cable brings).
    fn stores(&self, name: &str) -> bool {
        omsi_script::compile::with_lower(name, |k| self.names.get(k).copied())
            .is_some_and(|slot| self.local.get(slot as usize) == Some(&true))
    }

    /// Fire trigger `name` of this car's own scripts (a key or a switch of the cab the
    /// player drives from, a button on its body). What the trigger stores is the car's own
    /// from then on, not the leading vehicle's refreshed over it each frame. False when the
    /// car's program has no such trigger.
    pub(super) fn trigger(
        &mut self,
        lead: &mut VehicleInstance,
        name: &str,
        pictures: &mut Vec<crate::scripttex::ScriptTexture>,
    ) -> bool {
        let Some(block) = self.prog.trigger(name) else {
            return false;
        };
        let mut gained = false;
        for (slot, stored) in stored_by(&self.prog, vec![block as usize])
            .into_iter()
            .enumerate()
        {
            if stored && self.local.get(slot) == Some(&false) {
                self.local[slot] = true;
                gained = true;
            }
        }
        if gained {
            self.lead_len = None;
            self.sync_names(lead);
        }
        let (prog, state, machine) = (self.prog.clone(), &mut self.state, &mut self.machine);
        let mut fired = false;
        quietly_drawing(lead, pictures, &mut self.fired_sounds, |host| {
            fired = machine.run_trigger(&prog, name, state, host)
        });
        fired
    }

    /// Whether this car's own scripts have trigger `name`.
    pub(super) fn has_trigger(&self, name: &str) -> bool {
        self.prog.trigger(name).is_some()
    }

    /// When the leading vehicle has gained variables: give this set the names it lacks and
    /// work out again which slots are refreshed from it: what the engine gives every car,
    /// and what the car's program does not know (a part's model can still show that). What
    /// the leading vehicle's scripts hold is theirs: in OMSI every car has variables of its
    /// own, and a rear cab that read the head cab's switches here worked them along with it.
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
            let scripts_own = !self.script_share
                && (slot as usize) < self.prog.var_names.len()
                && !DRIVEN_BY_ENGINE.contains(&name.as_str())
                && !name.starts_with("ai_")
                && lead
                    .ty
                    .program
                    .var(name)
                    .is_some_and(|id| lead.ty.program.stores(id));
            if let (false, false, Some(&from)) =
                (stored_here, scripts_own, lead.var_index.get(name))
            {
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
    /// `probe`: `GetHeightAbovePoint` at the car's own place; `pictures`: the car's own
    /// script textures (see [`quietly_drawing`]).
    pub(super) fn frame(
        &mut self,
        lead: &mut VehicleInstance,
        place: [f32; 3],
        probe: Option<GroundHeightProbe>,
        pictures: &mut Vec<crate::scripttex::ScriptTexture>,
    ) {
        self.sync_names(lead);
        let lead_vars = &lead.state.vars;
        for &(slot, from) in &self.shared {
            self.state.vars[slot as usize] = lead_vars[from as usize];
        }
        // (`train_me_reverse` is as seen from the end the train is driven from, as after
        // Omsi.exe turned the train round, 0x613a98: the cab driven from reads 0)
        self.write_place([
            place[0],
            place[1],
            if (place[2] > 0.5) != lead.turned {
                1.0
            } else {
                0.0
            },
        ]);
        // a car turned round reads the leading vehicle's speed turned round: the NF6D's rear
        // cab steps its throttle by its own `Velocity`
        if place[2] > 0.5 {
            for name in ["velocity", "velocity_ground"] {
                if let Some(&slot) = self.names.get(name) {
                    if self.local.get(slot as usize) != Some(&true) {
                        self.state.vars[slot as usize] = -self.state.vars[slot as usize];
                    }
                }
            }
        }
        for &(slot, value) in &self.lines {
            self.state.vars[slot as usize] = value;
        }
        let as_ai = !self.prog.frame_ai.is_empty() && lead.ev(Ev::Ai).unwrap_or(0.0) > 0.5;
        let (prog, state, machine) = (self.prog.clone(), &mut self.state, &mut self.machine);
        let own_probe = probe.map(|p| std::mem::replace(&mut lead.host.ground_probe, Some(p)));
        quietly_drawing(lead, pictures, &mut self.fired_sounds, |host| {
            if as_ai {
                machine.run_frame_ai(&prog, state, host);
            } else {
                machine.run_frame(&prog, state, host);
            }
        });
        if self.fired_sounds.triggers.len() > 256 {
            let extra = self.fired_sounds.triggers.len() - 256;
            self.fired_sounds.triggers.drain(..extra);
        }
        if self.fired_sounds.files.len() > 256 {
            let extra = self.fired_sounds.files.len() - 256;
            self.fired_sounds.files.drain(..extra);
        }
        if let Some(before) = own_probe {
            lead.host.ground_probe = before;
        }
    }

    /// The car's value of variable `name`.
    pub fn get(&self, name: &str) -> Option<f32> {
        let slot = omsi_script::compile::with_lower(name, |k| self.names.get(k).copied())?;
        Some(self.state.vars[slot as usize])
    }

    /// The car's value of `name` where its own scripts work it out (store it), None where it
    /// is only the leading vehicle's refreshed: a motor car's own `M_Wheel`, `Brakeforce`.
    pub fn own_stored(&self, name: &str) -> Option<f32> {
        self.stores(name).then(|| self.get(name)).flatten()
    }

    /// The car's value of `name` where its own program declares it (None for one it only
    /// has from the leading vehicle).
    pub fn own(&self, name: &str) -> Option<f32> {
        let slot = self.prog.var(name)?;
        self.state.vars.get(slot as usize).copied()
    }

    /// Set `name` where the car's own program declares it, and keep it the car's own from
    /// then on (not refreshed over from the leading vehicle): what the engine tells this car
    /// alone, its passengers' requests at its own doors. False where it has no such variable.
    pub(super) fn set_own_str(&mut self, name: &str, text: &str) {
        if let Some(s) = self
            .prog
            .str_var(name)
            .and_then(|i| self.state.str_vars.get_mut(i as usize))
        {
            *s = text.to_string();
        }
    }

    pub fn set_own(&mut self, name: &str, v: f32) -> bool {
        let Some(slot) = self.prog.var(name) else {
            return false;
        };
        self.state.vars[slot as usize] = v;
        if self.local.get(slot as usize) == Some(&false) {
            self.local[slot as usize] = true;
            self.lead_len = None;
        }
        true
    }

    /// The car's own string variable `name` (one its program declares).
    pub fn get_str(&self, name: &str) -> Option<&str> {
        let slot = self.prog.str_var(name)?;
        self.state.str_vars.get(slot as usize).map(String::as_str)
    }
}

#[cfg(test)]
#[path = "car_run_tests.rs"]
mod tests;

impl VehicleInstance {
    /// `$.yard` when the depot file is changed in the game: it is the depot's `[name]`, set
    /// once at loading (see `VehicleInstance::new`), and BKK FUTAR finds its announcements
    /// by it (`sound\stops\<yard>\...`). The leading vehicle's and every car's own scripts.
    pub fn set_yard(&mut self, name: &str) {
        if let Some(i) = self.ty.program.str_var("yard") {
            self.state.str_vars[i as usize] = name.to_string();
        }
        for t in &mut self.trailers {
            if let Some(r) = t.run.as_deref_mut() {
                r.set_str_var("yard", name);
            }
        }
    }
}
