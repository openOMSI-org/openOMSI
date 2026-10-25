//! The scripted scenery objects of the loaded tiles as plain simulation state: their
//! scripts and variables, which of them run (the active set), and one step of them that
//! ends in a list of commands for the view (what to draw and play), in object order.

use super::{InUse, SceneryInstance, SceneryVars};
use crate::host::Arrival;
use crate::vehicle_api::Departure;
use crate::SimClock;
use glam::{DVec3, Mat4};
use hashbrown::HashMap;
use omsi_map::calendar::DayKind;
use std::path::Path;
use std::sync::Arc;

/// Objects farther than this from the listener do not run (OMSI's own cut).
pub const SCRIPT_RANGE: f64 = 800.0;

/// The side of the cells the camera moves through: the active set is made anew when it
/// crosses into another one (or tiles came or went), from the objects within reach of
/// anywhere in its cell.
const CELL: f64 = 50.0;

const SCRIPTS_SECS_PER_HELPER: f64 = 0.25e-3;

/// A placed scenery object with a running script: what the simulation keeps of it (its
/// render side, materials and sounds stay with the view, under the same key).
pub struct SceneryObject {
    /// Given when it was placed, counting up (see [`SceneryInbox`]): the list is in key
    /// order, and the view finds its half of the object by it.
    pub key: u64,
    pub map_id: i64,
    pub pos: DVec3,
    /// The tile it belongs to (it goes when the tile is unloaded).
    pub tile: (i32, i32),
    pub inst: SceneryInstance,
    /// Traffic light program of this object (crossings) or of its parent (lamps).
    pub controller: Option<usize>,
    pub light_index: usize,
    /// A child of a crossing that names one of its lights without being one of our lamps
    /// (its own textures to choose): (the crossing, the light). Its script reads that
    /// light's `TrafficLightPhase` and `TrafficLightApproach`, as Omsi.exe's
    /// RefreshAmpelParenting (0x77d460) hands them to any such child. The crossing's
    /// program is looked up when the script runs: it may stand on a tile loaded later.
    pub light_parent: Option<(i64, usize)>,
    /// `[varparent]`: the object whose data this one shows (a departure display's stop).
    pub var_parent: Option<i64>,
    /// The script asks for the buses due at its stop (`GetArrBus*`).
    pub arrivals: bool,
    /// The string variables its `[texttexture]`s show, and what each showed last.
    pub text_vars: Vec<String>,
    pub text_last: Vec<Option<String>>,
    /// It has `[htmltexture]` pages.
    pub has_pages: bool,
    /// The view reads its variables by name (materials, faded slots, texture choices,
    /// sounds): they go with each frame's [`SceneryShow`].
    pub needs_vars: bool,
    /// Per `[path]` its `[switchdir]` (points: the `Switch` value that sets it).
    pub switch_dirs: Vec<Option<i32>>,
    /// The type's `.sco` (for the switches' log).
    pub sco_path: Option<Arc<Path>>,
    /// Its `[NightMapMode]` hours (the same building keeps its hours: seeded by its id).
    pub use_: InUse,
    /// The Lua / `.oop` scripts that drive it (`[lua_script]` of its type), with where it
    /// stands and faces for them.
    pub lua: Option<Arc<ObjectLua>>,
    pub heading: f32,
}

/// The scripts of an object type and how they run (see `docs/OBJECT_SCRIPTS.md`).
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectLua {
    pub files: Vec<std::path::PathBuf>,
    /// The camera must be this close for the scripts to run (m).
    pub range: f64,
    /// Frames of the scripts a second (0: every frame).
    pub rate: f32,
    /// The type's `.sco`, by which the scripts tell what they drive.
    pub type_name: String,
}

impl SceneryObject {
    /// `night_map_mode`: the type's `[NightMapMode]`.
    pub fn new(key: u64, map_id: i64, pos: DVec3, tile: (i32, i32), inst: SceneryInstance, night_map_mode: i32) -> SceneryObject {
        SceneryObject {
            key,
            map_id,
            pos,
            tile,
            inst,
            controller: None,
            light_index: 0,
            light_parent: None,
            var_parent: None,
            arrivals: false,
            text_vars: Vec::new(),
            text_last: Vec::new(),
            has_pages: false,
            needs_vars: false,
            switch_dirs: Vec::new(),
            sco_path: None,
            use_: InUse::new(night_map_mode, map_id as u64),
            lua: None,
            heading: 0.0,
        }
    }
}

/// What the scripts read of the world this frame besides their own variables, gathered
/// before they run: the time and day, the daylight, the traffic lights and the timetable's
/// boards.
pub struct SceneryInputs<'a> {
    /// The scripts' time of day, the timetable's clock (None: they keep their own).
    pub clock: Option<&'a SimClock>,
    /// The time of day the buildings' hours and lights go by.
    pub time: f64,
    pub day: DayKind,
    pub brightness: f32,
    /// The listener: the scripts within [`SCRIPT_RANGE`] of it run.
    pub center: DVec3,
    /// `(controller, light)` -> (`TrafficLightPhase`, `TrafficLightApproach`).
    pub phase_of: &'a dyn Fn(usize, usize) -> (f32, f32),
    /// The crossing (map id) -> its light program, for `light_parent`.
    pub controllers: &'a HashMap<i64, usize>,
    /// Per bus stop (map object id): the buses due, soonest first, as (line, terminus,
    /// expected arrival in seconds of the day).
    pub by_stop: &'a HashMap<i64, Vec<(String, String, f64)>>,
    /// The departures made for the stop names the pages asked for, and their generation.
    pub departures: &'a std::collections::HashMap<String, Vec<Departure>>,
    pub departures_gen: u64,
    /// The HTML pages of objects nearer than this are driven.
    pub pages_near: f64,
}

/// [`SceneryInputs`] as owned values, to step the scripts on another thread: made where
/// the world's boards and light owners are (`World::scenery_prepare`); the light phases
/// come from the traffic of the same step.
#[derive(Clone, Default)]
pub struct OwnedSceneryInputs {
    pub clock: Option<SimClock>,
    pub time: f64,
    pub day: DayKind,
    pub brightness: f32,
    pub center: DVec3,
    pub controllers: Arc<HashMap<i64, usize>>,
    pub by_stop: Arc<HashMap<i64, Vec<(String, String, f64)>>>,
    pub departures: Arc<std::collections::HashMap<String, Vec<Departure>>>,
    pub departures_gen: u64,
    pub pages_near: f64,
}

impl OwnedSceneryInputs {
    pub fn as_inputs<'a>(&'a self, phase_of: &'a dyn Fn(usize, usize) -> (f32, f32)) -> SceneryInputs<'a> {
        SceneryInputs {
            clock: self.clock.as_ref(),
            time: self.time,
            day: self.day,
            brightness: self.brightness,
            center: self.center,
            phase_of,
            controllers: &self.controllers,
            by_stop: &self.by_stop,
            departures: &self.departures,
            departures_gen: self.departures_gen,
            pages_near: self.pages_near,
        }
    }
}

/// What the view does for one object after a step, in object order (by key).
#[derive(Debug)]
pub enum SceneryCmd {
    /// The object (its key) is out of range: its sounds stop.
    Silence(u64),
    /// The object ran: show what its script made of it.
    Show(SceneryShow),
}

/// What an object that ran looks like now, as values (the view does not read the
/// simulation's list).
#[derive(Debug)]
pub struct SceneryShow {
    pub key: u64,
    /// From the listener (m).
    pub dist: f64,
    /// Its `NightlightA` this frame.
    pub nightlight: f32,
    /// The sound triggers its script fired.
    pub fired: Vec<String>,
    /// The new pictures of its pages: (script texture index, width, height, RGBA).
    pub pages: Vec<(usize, u32, u32, Vec<u8>)>,
    /// The texts whose strings changed: (its `[texttexture]` in the object's order, the
    /// string), as they were before the pages ran.
    pub texts: Vec<(usize, String)>,
    /// Its variables after the frame (a span of [`SceneryOutputs::vars`]; empty unless
    /// [`SceneryObject::needs_vars`]): read by name with [`var_in`].
    pub vars: (u32, u32),
    /// Its meshes' transforms and visibility (a span of [`SceneryOutputs::transforms`]
    /// and [`SceneryOutputs::visible`]).
    pub meshes: (u32, u32),
}

/// Variable `name` of `program` out of a copy of its values (as
/// [`SceneryInstance::var`] reads it from the object).
pub fn var_in(program: &omsi_script::Program, vars: &[f32], name: &str) -> Option<f32> {
    // (no values: an object whose variables the view was not to read)
    program.var(name).and_then(|i| vars.get(i as usize).copied())
}

/// The commands of one step and what the timetable is asked for.
#[derive(Default, Debug)]
pub struct SceneryOutputs {
    pub cmds: Vec<SceneryCmd>,
    /// The values the commands' spans point into.
    pub vars: Vec<f32>,
    pub transforms: Vec<Mat4>,
    pub visible: Vec<bool>,
    /// The variables of the watched objects (particle emitters read them) as they were
    /// before the step's scripts ran: (map id, its script, span of `vars`), by map id, the
    /// first object of each id.
    pub watched: Vec<(i64, Arc<omsi_script::Program>, (u32, u32))>,
    /// The stops whose displays asked (sorted, each once).
    pub wanted: Vec<i64>,
    /// The stop names the HTML pages asked departures for, in the order first asked.
    pub wanted_names: Vec<String>,
    /// How many objects ran.
    pub ran: usize,
    /// What driving the pages and gathering the values took (s), for the view's profile.
    pub post: f64,
}

impl SceneryOutputs {
    pub fn vars_of(&self, span: (u32, u32)) -> &[f32] {
        &self.vars[span.0 as usize..span.1 as usize]
    }

    /// (transforms, visibility) of a span of meshes.
    pub fn meshes_of(&self, span: (u32, u32)) -> (&[Mat4], &[bool]) {
        let r = span.0 as usize..span.1 as usize;
        (&self.transforms[r.clone()], &self.visible[r])
    }

    /// Variable `name` of watched object `map_id` before the step.
    pub fn watched_var(&self, map_id: i64, name: &str) -> Option<f32> {
        let i = self.watched.binary_search_by_key(&map_id, |w| w.0).ok()?;
        let (_, program, span) = &self.watched[i];
        var_in(program, self.vars_of(*span), name)
    }
}

fn copy_span<T: Copy>(to: &mut Vec<T>, from: &[T]) -> (u32, u32) {
    let a = to.len() as u32;
    to.extend_from_slice(from);
    (a, to.len() as u32)
}

/// (object, its engine variables, its distance from the listener) for the parallel run;
/// kept between frames as a buffer of the same layout (see [`reuse`]).
type Job<'a> = (&'a mut SceneryObject, SceneryVars, f64, u32);
type JobBuf = (usize, SceneryVars, f64, u32);

/// Times of one step (seconds): gathering what the scripts are given, the scripts, what
/// came of them (pages, triggers).
#[derive(Default, Clone, Copy, Debug)]
pub struct StepTimes {
    pub inputs: f64,
    pub scripts: f64,
    pub post: f64,
}

/// The scripted scenery objects of the loaded tiles.
#[derive(Default)]
pub struct SceneryState {
    objects: Vec<SceneryObject>,
    /// The first object of each map id (a search of the list would find that one).
    by_id: HashMap<i64, u32>,
    /// The objects within reach of the camera's cell, by index (ascending).
    candidates: Vec<u32>,
    /// The cell the candidates were gathered for (None: to be gathered).
    cell: Option<(i64, i64)>,
    jobs: Vec<JobBuf>,
    out: SceneryOutputs,
    /// The scripts' time last frame on all the threads that ran them.
    scripts_work: f64,
    /// Counts up whenever objects come or go.
    generation: u64,
    /// The map ids whose variables the particle emitters read (sorted).
    watched: Vec<i64>,
    /// The signals' indices (`set_signals`) and the generation they were found at.
    pub(super) signals: (u64, Vec<u32>),
    /// The inbox the list follows (see [`SceneryState::take_inbox`]).
    pub(super) inbox_id: u64,
    /// The traffic light lamps' scripts (run by the traffic's view sync).
    pub lamps: super::LampScripts,
}

/// A vector's allocation for another element type of the same size (the parallel run's
/// list borrows the objects for one frame; its buffer stays).
fn reuse<T, U>(mut v: Vec<T>) -> Vec<U> {
    v.clear();
    v.into_iter().map(|_| unreachable!()).collect()
}

impl SceneryState {
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    pub fn objects(&self) -> &[SceneryObject] {
        &self.objects
    }

    /// For changes that keep the list as it is (variables, triggers).
    pub fn objects_mut(&mut self) -> &mut [SceneryObject] {
        &mut self.objects
    }

    pub fn get(&self, index: u32) -> &SceneryObject {
        &self.objects[index as usize]
    }

    /// Counts up whenever objects come or go (indices kept elsewhere are good while it
    /// stays).
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Variables of objects `map_id` go with each step's outputs (a particle emitter
    /// reads them).
    pub fn watch(&mut self, map_id: i64) {
        if let Err(i) = self.watched.binary_search(&map_id) {
            self.watched.insert(i, map_id);
        }
    }

    /// The first object of id `map_id`.
    pub fn find(&self, map_id: i64) -> Option<u32> {
        self.by_id.get(&map_id).copied()
    }

    pub fn find_mut(&mut self, map_id: i64) -> Option<&mut SceneryObject> {
        let i = self.find(map_id)?;
        Some(&mut self.objects[i as usize])
    }

    /// A newly placed object goes to the end of the list (its index is returned); it runs
    /// from the next step when it is within reach of the camera's cell.
    pub fn push(&mut self, o: SceneryObject) -> u32 {
        debug_assert!(self.objects.last().is_none_or(|l| l.key < o.key), "scenery objects come in key order");
        self.generation += 1;
        let i = self.objects.len() as u32;
        self.by_id.entry(o.map_id).or_insert(i);
        if let Some(cell) = self.cell {
            if near_cell(o.pos, cell) {
                self.candidates.push(i);
            }
        }
        self.objects.push(o);
        i
    }

    /// The objects of tile `key` go; `keep` is told for each object whether it stays, in
    /// order (to take the view's halves away with them).
    pub fn drop_tile(&mut self, key: (i32, i32), mut keep: impl FnMut(usize, bool)) -> bool {
        if !self.objects.iter().any(|o| o.tile == key) {
            return false;
        }
        self.generation += 1;
        let mut i = 0;
        self.objects.retain(|o| {
            let stays = o.tile != key;
            keep(i, stays);
            i += 1;
            stays
        });
        self.by_id.clear();
        for (i, o) in self.objects.iter().enumerate() {
            self.by_id.entry(o.map_id).or_insert(i as u32);
        }
        self.cell = None;
        true
    }

    /// The scripts of the objects within [`SCRIPT_RANGE`] of `inputs.center` run one frame
    /// (side by side on the worker threads, each on its own variables); the commands for
    /// the view come back in object order, as they did when the view walked the list.
    pub fn step(&mut self, dt: f32, inputs: &SceneryInputs, times: &mut StepTimes) {
        let t = std::time::Instant::now();
        let out = &mut self.out;
        out.cmds.clear();
        out.vars.clear();
        out.transforms.clear();
        out.visible.clear();
        out.watched.clear();
        out.wanted.clear();
        out.wanted_names.clear();
        out.ran = 0;
        for &id in &self.watched {
            if let Some(&i) = self.by_id.get(&id) {
                let o = &self.objects[i as usize];
                let span = copy_span(&mut out.vars, &o.inst.state.vars);
                out.watched.push((id, o.inst.program.clone(), span));
            }
        }
        let cell = cell_of(inputs.center);
        if self.cell != Some(cell) {
            // (the objects that drop out of reach were all farther than the range: they are
            // silenced below, in order with the rest)
            let was = std::mem::take(&mut self.candidates);
            self.candidates.extend(
                self.objects.iter().enumerate().filter(|(_, o)| near_cell(o.pos, cell)).map(|(i, _)| i as u32),
            );
            let fresh = self.cell.is_none();
            self.cell = Some(cell);
            if !fresh {
                // the old ones that are no candidates any more (the list did not change)
                let mut j = 0;
                for &i in &was {
                    while j < self.candidates.len() && self.candidates[j] < i {
                        j += 1;
                    }
                    if self.candidates.get(j) != Some(&i) {
                        out.cmds.push(SceneryCmd::Silence(self.objects[i as usize].key));
                    }
                }
            } else {
                // (tiles came or went: any object may have been heard)
                let mut j = 0;
                for i in 0..self.objects.len() as u32 {
                    if self.candidates.get(j) == Some(&i) {
                        j += 1;
                    } else {
                        out.cmds.push(SceneryCmd::Silence(self.objects[i as usize].key));
                    }
                }
            }
        }
        let mut jobs: Vec<Job> = reuse(std::mem::take(&mut self.jobs));
        {
            let mut rest = self.objects.iter_mut();
            let mut next = 0u32;
            for &i in &self.candidates {
                let o = rest.nth((i - next) as usize).expect("candidate in the list");
                next = i + 1;
                let dist = (o.pos - inputs.center).length();
                if dist > SCRIPT_RANGE {
                    out.cmds.push(SceneryCmd::Silence(o.key));
                    continue;
                }
                let vars = gather(o, dist, inputs, out);
                jobs.push((o, vars, dist, i));
            }
        }
        // (the silences of objects that left the candidates, and of the candidates out of
        // range, in object order: as the old walk over the whole list stopped them)
        out.cmds.sort_unstable_by_key(|c| match c {
            SceneryCmd::Silence(k) => *k,
            SceneryCmd::Show(s) => s.key,
        });
        out.wanted.sort_unstable();
        out.wanted.dedup();
        times.inputs += t.elapsed().as_secs_f64();
        let t = std::time::Instant::now();
        // (on this thread, workers joining in as they are free: handed whole to the pool,
        // the frame waited on a latch for busy or unscheduled workers - 20-70 ms frames
        // standing still on a city map - for 4 ms of scripts)
        let clock = inputs.clock;
        let helpers = omsi_cfg::par::helpers_for(self.scripts_work, SCRIPTS_SECS_PER_HELPER).min(jobs.len().saturating_sub(1));
        omsi_cfg::par::frame_each(&mut jobs, helpers, |(o, vars, _, _)| {
            o.inst.update_on(dt, vars, clock);
        });
        let elapsed = t.elapsed().as_secs_f64();
        self.scripts_work = elapsed * (helpers + 1) as f64;
        times.scripts += elapsed;
        let t = std::time::Instant::now();
        for (o, vars, dist, _) in jobs.iter_mut() {
            // `Refresh_Strings` is cleared and the texts read (the view drew them before the
            // pages ran, which may change the strings), then the pages run
            let mut texts = Vec::new();
            if !o.text_vars.is_empty() {
                let _ = o.inst.take_refresh_strings();
                for (k, (name, last)) in o.text_vars.iter().zip(o.text_last.iter_mut()).enumerate() {
                    let text = o.inst.str_var(name);
                    if last.as_deref() != Some(text) {
                        *last = Some(text.to_string());
                        texts.push((k, text.to_string()));
                    }
                }
            }
            let pages = if o.has_pages && *dist < inputs.pages_near { o.inst.update_html_textures() } else { Vec::new() };
            let fired = std::mem::take(&mut o.inst.host.fired_triggers);
            let var_span = if o.needs_vars { copy_span(&mut out.vars, &o.inst.state.vars) } else { (0, 0) };
            let mesh_span = copy_span(&mut out.transforms, &o.inst.mesh_transforms);
            out.visible.extend_from_slice(&o.inst.mesh_visible);
            out.cmds.push(SceneryCmd::Show(SceneryShow {
                key: o.key,
                dist: *dist,
                nightlight: vars.nightlight,
                fired,
                pages,
                texts,
                vars: var_span,
                meshes: mesh_span,
            }));
        }
        out.ran = jobs.len();
        self.jobs = reuse(jobs);
        self.out.post = t.elapsed().as_secs_f64();
        times.post += self.out.post;
    }

    /// The last step's commands, for the view to apply.
    pub fn outputs(&mut self) -> &mut SceneryOutputs {
        &mut self.out
    }
}

/// What object `o` is given this frame (in order: the light programs and the boards are
/// read here), and what it asks the timetable for.
fn gather(o: &mut SceneryObject, dist: f64, inputs: &SceneryInputs, out: &mut SceneryOutputs) -> SceneryVars {
    // the object's own hours ([NightMapMode]): in use, and lit while in use and the
    // daylight under its own threshold (0.6, or 0.3-0.75 with a [NightMapMode])
    let in_use = o.use_.in_use(inputs.time, inputs.day);
    let light = match (o.controller, o.light_parent) {
        (Some(c), _) => Some((c, o.light_index)),
        (None, Some((parent, li))) => inputs.controllers.get(&parent).map(|&c| (c, li)),
        _ => None,
    };
    let vars = SceneryVars {
        nightlight: o.use_.lit(inputs.time, inputs.day, inputs.brightness) as i32 as f32,
        in_use: in_use as i32 as f32,
        traffic_light_phase: light
            .map(|(c, li)| (inputs.phase_of)(c, li).0)
            .unwrap_or(crate::traffic::UNLINKED_PHASE as f32),
        traffic_light_approach: light.map(|(c, li)| (inputs.phase_of)(c, li).1).unwrap_or(0.0),
        switch: None,
    };
    // a departure display: the buses due at its stop
    if let (true, Some(stop)) = (o.arrivals, o.var_parent) {
        out.wanted.push(stop);
        let now = inputs.clock.map(|c| c.time).unwrap_or(0.0);
        // The list changes about once a second, the times to go every frame: the
        // same buses keep their names and only count down (the whole list was made
        // anew, its names copied, every frame for every board).
        let arrivals = &mut o.inst.host.arrivals;
        match inputs.by_stop.get(&stop) {
            Some(l)
                if arrivals.len() == l.len()
                    && arrivals.iter().zip(l).all(|(a, (line, terminus, _))| a.line == *line && a.terminus == *terminus) =>
            {
                for (a, (_, _, t)) in arrivals.iter_mut().zip(l) {
                    a.due = (t - now) as f32;
                }
            }
            Some(l) => {
                *arrivals = l
                    .iter()
                    .map(|(line, terminus, t)| Arrival { line: line.clone(), terminus: terminus.clone(), due: (t - now) as f32 })
                    .collect();
            }
            None => arrivals.clear(),
        }
    }
    // an HTML page that asks for departures by stop name
    if o.has_pages && dist < inputs.pages_near && !o.inst.host.html_departure_wants.is_empty() {
        for k in &o.inst.host.html_departure_wants {
            if !out.wanted_names.contains(k) {
                out.wanted_names.push(k.clone());
            }
        }
        if o.inst.host.html_departures_gen != inputs.departures_gen {
            o.inst.host.html_departures = o
                .inst
                .host
                .html_departure_wants
                .iter()
                .filter_map(|k| inputs.departures.get(k).map(|l| (k.clone(), l.clone())))
                .collect();
            o.inst.host.html_departures_gen = inputs.departures_gen;
        }
    }
    vars
}

fn cell_of(p: DVec3) -> (i64, i64) {
    ((p.x / CELL).floor() as i64, (p.y / CELL).floor() as i64)
}

/// Whether an object at `p` can come within range of a camera anywhere in `cell` (a metre
/// to spare for the rounding).
fn near_cell(p: DVec3, cell: (i64, i64)) -> bool {
    let c = DVec3::new((cell.0 as f64 + 0.5) * CELL, (cell.1 as f64 + 0.5) * CELL, 0.0);
    let reach = SCRIPT_RANGE + CELL * std::f64::consts::FRAC_1_SQRT_2 + 1.0;
    let d = p - c;
    // (height is part of the distance the range is measured in; leaving it out keeps more)
    d.x * d.x + d.y * d.y <= reach * reach
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_script::Program;
    use std::sync::Arc;

    fn object(map_id: i64, pos: DVec3, tile: (i32, i32)) -> SceneryObject {
        let inst = SceneryInstance::new(Arc::new(Program::default()), &[], SimClock::default(), &[]);
        SceneryObject::new(KEY.fetch_add(1, std::sync::atomic::Ordering::Relaxed), map_id, pos, tile, inst, 0)
    }

    static KEY: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

    fn step(s: &mut SceneryState, center: DVec3) -> (Vec<u32>, Vec<u32>) {
        let phase = |_: usize, _: usize| (0.0, 0.0);
        let (controllers, by_stop, departures) = (HashMap::new(), HashMap::new(), Default::default());
        let inputs = SceneryInputs {
            clock: None,
            time: 0.0,
            day: DayKind::default(),
            brightness: 1.0,
            center,
            phase_of: &phase,
            controllers: &controllers,
            by_stop: &by_stop,
            departures: &departures,
            departures_gen: 0,
            pages_near: 60.0,
        };
        s.step(1.0 / 30.0, &inputs, &mut StepTimes::default());
        let index: HashMap<u64, u32> = s.objects().iter().enumerate().map(|(i, o)| (o.key, i as u32)).collect();
        let out = s.outputs();
        let (mut shown, mut silenced) = (Vec::new(), Vec::new());
        for c in &out.cmds {
            match c {
                SceneryCmd::Silence(k) => {
                    assert!(shown.is_empty(), "the silences come first");
                    silenced.push(index[k])
                }
                SceneryCmd::Show(s) => shown.push(index[&s.key]),
            }
        }
        assert!(silenced.windows(2).all(|w| w[0] < w[1]) && shown.windows(2).all(|w| w[0] < w[1]), "in object order");
        (shown, silenced)
    }

    /// The objects that run are exactly those within range, in list order, wherever the
    /// camera goes; one that was heard and is out of range now is silenced.
    #[test]
    fn the_active_set_is_the_range_cut() {
        let mut s = SceneryState::default();
        let mut seed = 7u64;
        let mut rnd = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((seed >> 11) as f64 / (1u64 << 53) as f64) * 4000.0 - 2000.0
        };
        for i in 0..3000 {
            let p = DVec3::new(rnd(), rnd(), rnd() / 100.0);
            s.push(object(i, p, ((p.x / 300.0).floor() as i32, 0)));
        }
        let mut last: Vec<u32> = Vec::new();
        for k in 0..400 {
            let t = k as f64;
            // (along a road, then a jump across the map, then back)
            let center = if k == 200 { DVec3::new(1500.0, -1500.0, 3.0) } else { DVec3::new(-1500.0 + t * 7.3, (t * 0.05).sin() * 600.0, 2.0) };
            if k == 300 {
                s.drop_tile((2, 0), |_, _| {});
                last.clear();
            }
            if k == 310 {
                s.push(object(99_999, center + DVec3::new(10.0, 0.0, 0.0), (9, 9)));
            }
            let (shown, silenced) = step(&mut s, center);
            let expected: Vec<u32> =
                s.objects().iter().enumerate().filter(|(_, o)| !((o.pos - center).length() > SCRIPT_RANGE)).map(|(i, _)| i as u32).collect();
            assert_eq!(shown, expected, "step {k}");
            for i in &last {
                assert!(shown.contains(i) || silenced.contains(i), "step {k}: object {i} went out of range unheard");
            }
            last = shown;
        }
        assert!(s.find(99_999).is_some());
    }

    #[test]
    fn find_gives_the_first_of_an_id_after_a_tile_went() {
        let mut s = SceneryState::default();
        s.push(object(5, DVec3::ZERO, (0, 0)));
        s.push(object(5, DVec3::ZERO, (1, 0)));
        s.push(object(6, DVec3::ZERO, (1, 0)));
        assert_eq!(s.find(5), Some(0));
        let mut kept = Vec::new();
        assert!(s.drop_tile((0, 0), |i, stays| kept.push((i, stays))));
        assert_eq!(kept, vec![(0, false), (1, true), (2, true)]);
        assert_eq!((s.find(5), s.find(6), s.len()), (Some(0), Some(1), 2));
        assert!(!s.drop_tile((7, 7), |_, _| unreachable!()));
    }
}
