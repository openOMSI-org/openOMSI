//! Runtime for a sound configuration attached to an object with script variables.
//!
//! Everything here follows Omsi.exe's `TSound` update (0x750340, 2.2.032), which plays each
//! entry through its own DirectSound buffer:
//!
//! * volume = the entry's volume x every `[volcurve]` (linear between `[pnt]`s, held at the
//!   ends), x `min(range / distance, 1)` for a `[3d]` sound, x `0.2 + Snd_OutsideVol` for an
//!   AI vehicle while the camera is in a cab, x `Snd_OutsideVol` for the own bus's outside
//!   sounds heard in the cab - as a linear amplitude, turned into hundredths of a dB
//!   (`2000 * log10`) for `SetVolume`: over 0 dB or under -100 dB the call is refused and the
//!   buffer keeps the volume it had, at -100 dB or below the sound is not started;
//! * pitch = a `[loopsound]`'s `rate * |variable| / reference` for `SetFrequency` (refused
//!   outside DirectSound's 100 to 200 000 Hz; not started below 100 Hz); a `[sound]` plays
//!   at its file's rate;
//! * a `[3d]` sound is panned by at most `50 * [sound_stereo]` hundredths of a dB (see
//!   `mixer::pan_gains`), nothing else is spatial: a sound without `[3d]` is heard at the
//!   same level wherever the listener is - on an AI vehicle too.

use crate::mixer::{AudioEngine, Clip, VoiceId, VoiceParams};
use glam::{Mat4, Vec3};
use omsi_vehicle::{SoundCfg, SoundEntry};
use std::path::Path;
use std::sync::Arc;

struct RuntimeSound {
    def: SoundEntry,
    clip: Option<Arc<Clip>>,
    voice: Option<VoiceId>,
    /// The conditions held last frame (a `[noloop]` entry without a trigger plays once
    /// when they start to hold).
    held: bool,
    /// Since when the conditions hold - what a `[volcurve] -1` reads, see
    /// [`SoundSet::curve_input`].
    active_since: Option<std::time::Instant>,
    /// The buffer's volume (linear) as DirectSound last took it: a `SetVolume` over 0 dB
    /// or under -100 dB is refused and the buffer keeps this one. A new buffer is at 0 dB.
    last_gain: f32,
    /// The buffer's playback rate relative to the clip, as last taken by `SetFrequency`.
    last_pitch: f32,
    /// What each of the entry's `[volcurve]`s reads (see [`SoundSet::curve_input`]), worked
    /// out once from its variable's name instead of every frame.
    curve_inputs: Vec<CurveInput>,
    /// Where the variables the entry reads stand in its set's table (see `SoundSet::vars`).
    ix: VarIx,
    /// A triggered entry: what its volume curves gave when the trigger last fired, kept
    /// for as long as that sound plays (None: read them now).
    fired_curves: Option<f32>,
}

/// The places of an entry's variables in its set's table of variables read each frame: its
/// conditions', its volume curves' and its pitch's.
#[derive(Default)]
struct VarIx {
    conds: Vec<u16>,
    curves: Vec<u16>,
    pitch: u16,
}

/// Where an entry's variables come from: by name (as the scripts are asked), or from the
/// set's table read once this frame (`ix`: the place there).
trait Vars {
    fn read(&self, name: &str, ix: u16) -> Option<f32>;
}

struct ByName<'a>(&'a dyn Fn(&str) -> Option<f32>);

impl Vars for ByName<'_> {
    fn read(&self, name: &str, _: u16) -> Option<f32> {
        (self.0)(name)
    }
}

struct Table<'a>(&'a [Option<f32>]);

impl Vars for Table<'_> {
    fn read(&self, _: &str, ix: u16) -> Option<f32> {
        self.0.get(ix as usize).copied().flatten()
    }
}

/// What a `[volcurve]` reads, by its variable's name (see [`SoundSet::curve_input`]).
#[derive(Clone, Copy, PartialEq, Debug)]
enum CurveInput {
    /// -1: seconds since the sound became active.
    Active,
    /// -2: how much the sound faces the listener.
    Facing,
    /// Any other negative number: skipped.
    Skip,
    /// A variable.
    Var,
}

impl CurveInput {
    fn of(vc: &omsi_vehicle::VolCurve) -> CurveInput {
        match vc.variable.trim().parse::<i32>() {
            Ok(-1) => CurveInput::Active,
            Ok(-2) => CurveInput::Facing,
            Ok(n) if n < 0 => CurveInput::Skip,
            _ => CurveInput::Var,
        }
    }
}

impl RuntimeSound {
    fn new(def: SoundEntry, clip: Option<Arc<Clip>>) -> RuntimeSound {
        let curve_inputs = def.vol_curves.iter().map(CurveInput::of).collect();
        RuntimeSound { def, clip, voice: None, held: false, active_since: None, last_gain: 1.0, last_pitch: 1.0, curve_inputs, ix: VarIx::default(), fired_curves: None }
    }
}

pub struct SoundSet {
    sounds: Vec<RuntimeSound>,
    /// The `[sound_ai]`/`[sound_scenery]` share of this set (the settings' sliders).
    pub master: f32,
    /// Folder of the sound config: files of `(T.F.)` triggers resolve against it.
    dir: std::path::PathBuf,
    /// The listener sits in this vehicle's interior (see [`SoundSet::set_inside`]).
    inside: bool,
    /// How far in: 1 in a cab view, 0 outside, in between while a walker steps in or out
    /// through a door (see [`SoundSet::set_inside_amount`]).
    amount: f32,
    /// This set belongs to an AI vehicle or another player's (Omsi.exe's view 4).
    ai: bool,
    scenery: bool,
    /// The player's own vehicle moves with its listener; its 3D sounds still pan and fade,
    /// but frame timing must not turn their fixed cabin positions into Doppler pitch shifts.
    listener_vehicle: bool,
    /// The camera is in a cab view (driver or passenger: Omsi.exe's camera modes 0 and 1),
    /// set every frame on every sound set (see [`SoundSet::set_muffled`]).
    muffled: bool,
    /// The sound sets of the coupled parts (with the part's index among the vehicle's
    /// trailers): the rear section of an articulated bus has a `[sound]` of its own - on a
    /// pusher like the MB C2 G that is where the engine is - and plays it on the triggers
    /// and variables of the scripts it shares with the front (see [`SoundSet::update_parts`]).
    pub parts: Vec<(usize, SoundSet)>,
    /// The variables the entries read (each name once) and their values this frame: a
    /// vehicle's sounds read the same few variables many times over (an engine's dozen
    /// loops its revolutions and throttle), and every read was a lookup by name. Made at
    /// the first update.
    vars: Option<(Vec<String>, Vec<Option<f32>>)>,
}

/// The file a `(T.F.)` trigger names, as Omsi.exe opens it (0x74f2e8 through 0x7eed78): the
/// characters a Windows file name cannot have are `%` in it, the folders' `\` and a drive's
/// `:` stay. The stock IBIS announces a stop by its name, and Spandau's "Falkenseer
/// Ch/Stadtrandstr" is `Falkenseer Ch%Stadtrandstr.wav`: read as a folder, it was not found
/// and the stop was not announced (#1096).
fn file_trigger_name(file: &str) -> String {
    file.chars().map(|c| if matches!(c, '/' | '?' | '*' | '"' | '<' | '>' | '|') { '%' } else { c }).collect()
}

/// Say once per file that a `(T.F.)` sound cannot be found: every AI bus of a type asks for
/// the same missing announcement at every stop.
fn warn_missing_once(trigger: &str, path: &Path) {
    static SEEN: std::sync::Mutex<Option<std::collections::HashSet<std::path::PathBuf>>> =
        std::sync::Mutex::new(None);
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if seen
        .get_or_insert_with(Default::default)
        .insert(path.to_path_buf())
    {
        log::warn!(
            "sound {} for (T.F.{trigger}) not found (further requests for it are not logged)",
            path.display()
        );
    }
}

/// A `[volcurve]` at `x`, as Omsi.exe's `TFuncClass` (0x7f061c) reads it: the first point's
/// value left of it, the last one's from the last point on, and in between the straight
/// line between the last point at or left of `x` and the first one right of it (0x7f07b4;
/// two points at the same place give their mean).
fn curve(points: &[(f32, f32)], x: f32) -> f32 {
    let Some(&(x0, y0)) = points.first() else {
        return 1.0;
    };
    if x < x0 {
        return y0;
    }
    let last = points[points.len() - 1];
    if x >= last.0 {
        return last.1;
    }
    let i = (1..points.len()).find(|&i| x < points[i].0).unwrap_or(points.len() - 1);
    let (xa, ya) = points[i - 1];
    let (xb, yb) = points[i];
    if xb == xa {
        (ya + yb) / 2.0
    } else {
        (x - xa) * ((yb - ya) / (xb - xa)) + ya
    }
}

/// What DirectSound makes of a linear volume (`SetVolume(Round(2000 * log10 v))`, the
/// buffer at `last`): the volume it plays at, and whether it is loud enough to be started
/// (over -100 dB). A request over 0 dB or under -100 dB is refused - the buffer keeps
/// `last`; the MB 412D's `[sound] start2.wav`, which carries a loop sound's lines and reads
/// "44100" as its volume, plays at full volume, not 44 100 times too loud.
fn direct_sound_volume(v: f32, last: f32) -> (f32, bool) {
    if !(v > 1.0e-10) {
        // (-10000 is a valid request: silence)
        return (0.0, false);
    }
    let hundredths = (2000.0 * v.log10()).round();
    if hundredths > 0.0 {
        (last, true)
    } else if hundredths < -10000.0 {
        (last, false)
    } else {
        (10f32.powf(hundredths / 2000.0), hundredths > -10000.0)
    }
}

fn buffer_volume(v: f32, last: f32, master: f32) -> (f32, bool) {
    let (gain, audible) = direct_sound_volume(v * master, last * master);
    (gain / master, audible)
}

/// DirectSound's frequency range (`DSBFREQUENCY_MIN`/`MAX`).
const FREQ_MIN: f32 = 100.0;
const FREQ_MAX: f32 = 200_000.0;

/// What a `[volcurve] -1` reads on a triggered entry: Omsi.exe never starts its clock (it
/// is reset only while the conditions of an untriggered entry fail), so it reads
/// `GetTickCount / 1000`, the seconds since Windows started - always past the last point.
const TRIGGERED_ACTIVE: f32 = 1.0e6;

/// One entry's state this frame, as the exe computes it.
struct Eval {
    /// What the voice is given (the distance attenuation is left to the mixer).
    gain: f32,
    audible: bool,
    pitch: f32,
    /// `[viewpoint]` (or the global stop) silences it: a playing voice is stopped.
    wrong_view: bool,
}

impl SoundSet {
    /// The files of a sound config's fixed clips (`dir` as for [`SoundSet::new`]), for
    /// [`AudioEngine::clips_ready`].
    pub fn clip_paths(cfg: &SoundCfg, dir: &Path) -> Vec<std::path::PathBuf> {
        cfg.sounds
            .iter()
            .filter(|def| def.file.trim().parse::<i32>().is_err())
            .map(|def| omsi_cfg::resolve_path(dir, &def.file))
            .collect()
    }

    /// Load the clips of a sound config; `dir` is the directory of the config file.
    pub fn new(engine: &AudioEngine, cfg: &SoundCfg, dir: &Path) -> SoundSet {
        let sounds = cfg
            .sounds
            .iter()
            .map(|def| {
                // `[sound] N`: no fixed file, the script names one with `(T.F.trigger)`
                let dynamic = def.file.trim().parse::<i32>().is_ok();
                let path = omsi_cfg::resolve_path(dir, &def.file);
                let clip = if engine.enabled && !dynamic {
                    engine.load_clip(&path)
                } else {
                    None
                };
                RuntimeSound::new(def.clone(), clip)
            })
            .collect();
        SoundSet {
            sounds,
            master: 1.0,
            dir: dir.to_path_buf(),
            inside: false,
            amount: 0.0,
            ai: false,
            scenery: false,
            listener_vehicle: false,
            muffled: false,
            parts: Vec::new(),
            vars: None,
        }
    }

    /// The table of variables the entries read, each name once, and each entry's places in it.
    fn index_vars(sounds: &mut [RuntimeSound]) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        let mut at = |n: &str| -> u16 {
            match names.iter().position(|x| x == n) {
                Some(k) => k as u16,
                None => {
                    names.push(n.to_string());
                    (names.len() - 1) as u16
                }
            }
        };
        for s in sounds.iter_mut() {
            let conds = s.def.conditions.iter().map(|c| at(&c.variable)).collect();
            let curves = s.def.vol_curves.iter().zip(&s.curve_inputs).map(|(vc, k)| if *k == CurveInput::Var { at(&vc.variable) } else { 0 }).collect();
            let pitch = if s.def.is_loop { at(&s.def.pitch_variable) } else { 0 };
            s.ix = VarIx { conds, curves, pitch };
        }
        names
    }

    /// Where the listener is, for the `[viewpoint]` of an entry: `true` while the camera is
    /// one of this vehicle's interior views. The player's bus is told every frame; a scenery
    /// object or another vehicle is always listened to from outside.
    pub fn set_inside(&mut self, inside: bool) {
        self.inside = inside;
        self.amount = if inside { 1.0 } else { 0.0 };
        for (_, p) in &mut self.parts {
            p.set_inside(inside);
        }
    }

    /// [`SoundSet::set_inside`] by degrees, for a listener walking in or out through a
    /// door: `amount` 1 is all in (as `set_inside(true)`), 0 all out. In between, the
    /// entries of both views are heard, the cab's at `amount` of their volume and the
    /// street's at the rest, and the bus's outside sounds come through what is open as
    /// much more as the listener is out - no switch from one sound to the other at the
    /// step.
    pub fn set_inside_amount(&mut self, amount: f32) {
        let amount = if amount > 0.0 { amount.min(1.0) } else { 0.0 };
        self.inside = amount >= 0.5;
        self.amount = amount;
        for (_, p) in &mut self.parts {
            p.set_inside_amount(amount);
        }
    }

    /// Track whether the listener travels with the player's vehicle and its coupled parts.
    pub fn set_listener_vehicle(&mut self, follows: bool) {
        self.listener_vehicle = follows;
        for (_, p) in &mut self.parts {
            p.set_listener_vehicle(follows);
        }
    }

    /// The camera is in a cab view (driver or passenger, of any vehicle): called every frame
    /// on every sound set. An AI vehicle is then heard at `0.2 + Snd_OutsideVol` of the
    /// player's bus (`TSound` update 0x750a4d) - no filter, no other change.
    pub fn set_muffled(&mut self, muffled: bool) {
        self.muffled = muffled;
        for (_, p) in &mut self.parts {
            p.set_muffled(muffled);
        }
    }

    /// Attach the sound set of coupled part `index` (built like this one: [`SoundSet::new`]
    /// for the player's bus, [`SoundSet::new_exterior`] for the others).
    pub fn add_part(&mut self, index: usize, mut part: SoundSet) {
        part.master = self.master;
        part.inside = self.inside;
        part.amount = self.amount;
        part.muffled = self.muffled;
        part.ai = self.ai;
        part.listener_vehicle = self.listener_vehicle;
        self.parts.push((index, part));
    }

    /// Per-frame update of the coupled parts' sound sets: the same variables and triggers
    /// as the leading vehicle's, each at its part's place (`part_to_world`; a part that is
    /// gone is silenced).
    pub fn update_parts(
        &mut self,
        engine: &AudioEngine,
        var: &dyn Fn(&str) -> Option<f32>,
        part_to_world: &dyn Fn(usize) -> Option<Mat4>,
        triggers: &[String],
        at_fire: &dyn Fn(&str, &str) -> Option<f32>,
    ) {
        for (i, p) in &mut self.parts {
            match part_to_world(*i) {
                Some(xf) => p.update_fired(engine, var, &xf, triggers, at_fire),
                None => p.stop_all(engine),
            }
        }
    }

    /// The view an entry's `[viewpoint]` is tested against (Omsi.exe sets it per vehicle,
    /// 0x6ff86c and 0x700158): 2 for the player's bus while the camera is in a cab view, 1
    /// for it from outside, 4 for every other vehicle - an AI bus never plays its
    /// `[viewpoint] 1` sounds, which are the player's bus heard from the street.
    fn view_mask(&self) -> i32 {
        if self.ai {
            if self.scenery { 5 } else { 4 }
        } else if self.inside {
            2
        } else {
            1
        }
    }

    /// A sound set heard from outside: an AI vehicle's or another player's (view 4). Its
    /// sounds are placed exactly as the player's bus's: a `[3d]` one at its position, the
    /// others nowhere in particular.
    pub fn new_exterior(engine: &AudioEngine, cfg: &SoundCfg, dir: &Path) -> SoundSet {
        let mut s = Self::new(engine, cfg, dir);
        s.ai = true;
        s
    }

    pub fn new_scenery(engine: &AudioEngine, cfg: &SoundCfg, dir: &Path) -> SoundSet {
        let mut s = Self::new_exterior(engine, cfg, dir);
        s.scenery = true;
        s
    }

    /// Play the sound of a `(T.F.trigger)` event: the entry listening to `trigger` loads
    /// `file` (relative to the sound folder) into a new buffer and plays it as if its
    /// trigger had fired (0x74f2e8).
    pub fn play_file_trigger(
        &mut self,
        engine: &AudioEngine,
        trigger: &str,
        file: &str,
        var: &dyn Fn(&str) -> Option<f32>,
        object_to_world: &Mat4,
    ) {
        if !engine.enabled || file.trim().is_empty() || !self.sounds.iter().any(|s| s.def.triggers.iter().any(|d| d.trim().eq_ignore_ascii_case(trigger))) {
            return;
        }
        let path = omsi_cfg::resolve_path(&self.dir, &file_trigger_name(file));
        let Some(clip) = engine.load_clip(&path) else {
            warn_missing_once(trigger, &path);
            return;
        };
        let ctx = self.ctx(engine);
        for s in self.sounds.iter_mut() {
            if !s.def.triggers.iter().any(|d| d.trim().eq_ignore_ascii_case(trigger)) {
                continue;
            }
            if let Some(id) = s.voice.take() {
                engine.stop(id);
            }
            s.clip = Some(clip.clone());
            s.last_gain = 1.0;
            s.last_pitch = 1.0;
            let e = ctx.eval(s, var, object_to_world);
            if e.audible && !e.wrong_view {
                s.voice = Some(engine.play(clip.clone(), ctx.params(s, &e, false, object_to_world)));
            }
        }
    }

    fn ctx(&self, engine: &AudioEngine) -> Ctx {
        Ctx { listener: engine.listener_position(), now: engine.clock(), ..self.ctx_without_engine() }
    }

    /// What a `[volcurve]` reads. The exe (`TSound` load, 0x74e408) looks the name up in the
    /// vehicle's variables and, when it is none of them, stores `StrToInt(name)` as the
    /// index; the update (0x750584) then reads a negative index from the sound's own
    /// values: -1 = seconds since the sound became active (its conditions started to hold,
    /// GetTickCount/1000), -2 = how much a `[3d]` sound with a direction faces the listener
    /// (1 for a `[3d]` one without, 0 for one without `[3d]`), anything below is skipped
    /// with "Volume Variable not valid!". The LiAZ 5292's engine loops fade in with
    /// `[volcurve] -1` (0 at 1 s, 1 at 1.3 s).
    fn curve_input(
        vc: &omsi_vehicle::VolCurve,
        var: &dyn Fn(&str) -> Option<f32>,
        active: f32,
        facing: f32,
    ) -> Option<f32> {
        Self::curve_input_as(CurveInput::of(vc), vc, 0, &ByName(var), active, facing)
    }

    /// [`SoundSet::curve_input`] with what the curve reads already known (`ix`: its place
    /// in the set's table).
    fn curve_input_as<V: Vars + ?Sized>(
        kind: CurveInput,
        vc: &omsi_vehicle::VolCurve,
        ix: u16,
        var: &V,
        active: f32,
        facing: f32,
    ) -> Option<f32> {
        match kind {
            CurveInput::Active => Some(active),
            CurveInput::Facing => Some(facing),
            CurveInput::Skip => None,
            CurveInput::Var => Some(var.read(&vc.variable, ix).unwrap_or(0.0)),
        }
    }

    fn conditions_hold(def: &SoundEntry, var: &dyn Fn(&str) -> Option<f32>) -> bool {
        def.conditions
            .iter()
            .all(|c| c.holds(var(&c.variable).unwrap_or(0.0)))
    }

    /// [`SoundSet::conditions_hold`] of entry `s`, its variables read from `var`.
    fn conditions_hold_in<V: Vars + ?Sized>(s: &RuntimeSound, var: &V) -> bool {
        s.def
            .conditions
            .iter()
            .enumerate()
            .all(|(k, c)| c.holds(var.read(&c.variable, s.ix.conds.get(k).copied().unwrap_or(0)).unwrap_or(0.0)))
    }

    /// A `[loopsound]`'s frequency request in Hz (`rate * |variable| / reference`), `None`
    /// for a `[sound]`, which plays at its file's own rate.
    fn frequency(def: &SoundEntry, var: &dyn Fn(&str) -> Option<f32>) -> Option<f32> {
        def.is_loop.then(|| def.sample_rate.trunc() * var(&def.pitch_variable).unwrap_or(0.0).abs() / def.pitch_ref)
    }

    /// An entry plays on sound trigger `name`.
    pub fn has_trigger(&self, name: &str) -> bool {
        self.sounds.iter().any(|s| s.def.triggers.iter().any(|t| t.trim().eq_ignore_ascii_case(name)))
    }

    /// An entry's volume curve or condition reads variable `name`.
    pub fn reads_var(&self, name: &str) -> bool {
        let is = |v: &str| v.trim().eq_ignore_ascii_case(name);
        self.sounds.iter().any(|s| s.def.vol_curves.iter().any(|c| is(&c.variable)) || s.def.conditions.iter().any(|c| is(&c.variable)))
    }

    /// The triggers (lower case) whose entries read script variables in a volume curve:
    /// what [`SoundSet::update_fired`] wants the values of at the moment they fire.
    pub fn curve_triggers(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for s in &self.sounds {
            if !s.def.vol_curves.iter().any(|vc| vc.variable.trim().parse::<i32>().is_err()) {
                continue;
            }
            for t in &s.def.triggers {
                let t = t.trim().to_ascii_lowercase();
                if !out.contains(&t) {
                    out.push(t);
                }
            }
        }
        out
    }

    /// Per-frame update. `triggers` are the sound triggers fired by the scripts this frame.
    pub fn update(
        &mut self,
        engine: &AudioEngine,
        var: &dyn Fn(&str) -> Option<f32>,
        object_to_world: &Mat4,
        triggers: &[String],
    ) {
        self.update_fired(engine, var, object_to_world, triggers, &|_, _| None);
    }

    /// [`SoundSet::update`] with `at_fire(trigger, variable)`: a variable as it stood when
    /// the trigger fired (None: as it is now). Omsi.exe starts a trigger's sounds in the
    /// middle of the script (0x74f2e8), so the volume they start with is that of the moment:
    /// the SD200's door hits read `doorSpeed_<n>`, which the door script reverses right
    /// after firing them - read at the frame's end they were silent (#676). What the curves
    /// gave then stays the sound's until it ends (#611).
    pub fn update_fired(
        &mut self,
        engine: &AudioEngine,
        var: &dyn Fn(&str) -> Option<f32>,
        object_to_world: &Mat4,
        triggers: &[String],
        at_fire: &dyn Fn(&str, &str) -> Option<f32>,
    ) {
        if !engine.enabled {
            return;
        }
        let ctx = self.ctx(engine);
        // (every variable the entries read, once)
        if self.vars.is_none() {
            let names = Self::index_vars(&mut self.sounds);
            self.vars = Some((names, Vec::new()));
        }
        let (names, values) = self.vars.as_mut().expect("the table of variables");
        values.clear();
        values.extend(names.iter().map(|n| var(n)));
        let table = Table(values);
        for s in self.sounds.iter_mut() {
            // How the exe plays an entry (`TSound` update, 2.2.032):
            // * with a `[trigger]`: once each time the trigger fires, from the start, never
            //   looped (a `[loopsound]` too) and without looking at its conditions;
            // * without one: looped (from a random place in the clip) for as long as its
            //   conditions hold and it can be heard - `[sound]` and `[loopsound]` both loop;
            // * without one but with `[noloop]`: once, when its conditions start to hold.
            let triggered = !s.def.triggers.is_empty();
            let holds = triggered || Self::conditions_hold_in(s, &table);
            let rising = !triggered && holds && !s.held;
            s.held = holds;
            if !triggered {
                if !holds {
                    s.active_since = None;
                } else if s.active_since.is_none() {
                    s.active_since = Some(ctx.now);
                }
            }
            let Some(clip) = s.clip.clone() else { continue };
            let fired_by = if triggered {
                triggers.iter().find(|t| s.def.triggers.iter().any(|d| d.trim().eq_ignore_ascii_case(t)))
            } else {
                None
            };
            let fired = fired_by.is_some();
            // A voice that starts now is a buffer played anew, at DirectSound's full volume:
            // a request over 0 dB keeps that, not whatever an entry that was not playing had
            // been given in the frames before (a door's end sound, a parking brake or a
            // blinker with `[volume] 2` and a curve low at rest played at that low level, or
            // not at all - #1851, #611, #1268, #1311, #1515, #1900, #1418).
            let looping = !triggered && !s.def.no_loop;
            let idle = s.voice.is_none_or(|id| !engine.is_playing(id));
            let gain_before = s.last_gain;
            if fired || rising || (looping && idle) {
                s.last_gain = 1.0;
            }
            // (fired: its curves are read anew, as they stand now)
            let before = if fired { s.fired_curves.take() } else { None };
            // (the conditions as read above: not read again)
            let e = match fired_by {
                Some(t) => ctx.eval_holding(s, &|n| at_fire(t, n).or_else(|| var(n)), object_to_world, holds),
                None => ctx.eval_with(s, &table, object_to_world, holds),
            };
            if e.wrong_view {
                // "Stopped Sound ... globalstop/wrongview"
                if let Some(id) = s.voice.take() {
                    engine.stop(id);
                }
                continue;
            }
            if !triggered && !s.def.no_loop {
                let params = ctx.params(s, &e, true, object_to_world);
                match (s.voice, e.audible) {
                    (Some(id), true) => {
                        if engine.is_playing(id) {
                            engine.set_params(id, params);
                        } else {
                            s.voice = Some(engine.play_from_random_place(clip, params));
                        }
                    }
                    (Some(id), false) => {
                        // "Stopped Sound ... due to volume or pitch"
                        engine.stop(id);
                        s.voice = None;
                    }
                    (None, true) => s.voice = Some(engine.play_from_random_place(clip, params)),
                    (None, false) => {}
                }
                continue;
            }
            // one-shot: started by its trigger or by its conditions starting to hold; the
            // volume follows the curves while it plays (a triggered one's: those of the
            // moment it fired)
            // Fired again at a volume that is not heard - a door leaf bouncing back off its
            // stop hits it a second time, slowly - the hit that is playing plays on as it was.
            if fired && !e.audible && s.voice.is_some_and(|id| engine.is_playing(id)) {
                s.fired_curves = before;
                s.last_gain = gain_before;
                continue;
            }
            let params = ctx.params(s, &e, false, object_to_world);
            if (fired || rising) && e.audible {
                if let Some(id) = s.voice {
                    if s.def.only_one && engine.is_playing(id) {
                        engine.set_params(id, params);
                        continue;
                    }
                    engine.stop(id);
                }
                s.voice = Some(engine.play(clip, params));
            } else if let Some(id) = s.voice {
                if engine.is_playing(id) {
                    engine.set_params(id, params);
                } else {
                    s.voice = None;
                }
            }
        }
    }

    /// How many `[sound]`/`[loopsound]` entries the configuration has.
    pub fn len(&self) -> usize {
        self.sounds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sounds.is_empty()
    }

    /// The sounds playing right now: (file, gain at the listener, pitch) - for the logs.
    pub fn playing(&self, engine: &AudioEngine) -> Vec<(String, f32, f32)> {
        self.sounds
            .iter()
            .filter_map(|s| {
                s.voice
                    .and_then(|id| engine.voice_state(id))
                    .map(|(p, heard)| (s.def.file.clone(), heard, p.pitch))
            })
            .collect()
    }

    /// One line per entry of the configuration, saying whether it is heard and why not
    /// (`OMSI_DEBUG_SOUND`): the only way to see which of a bus's hundred sounds the
    /// rewrite never reaches - a missing clip, a condition on a variable nobody feeds, a
    /// volume curve that stays at zero or a `[viewpoint]` the camera is not in. A heard one
    /// gives its gain at the listener (after the distance) and its playback rate.
    pub fn report(&self, engine: &AudioEngine, var: &dyn Fn(&str) -> Option<f32>) -> Vec<String> {
        let ctx = self.ctx(engine);
        let mut out = Vec::new();
        for s in &self.sounds {
            let file = s.def.file.trim();
            let mut why = String::new();
            if s.clip.is_none() {
                why = if file.parse::<i32>().is_ok() {
                    "waits for a (T.F.) file".into()
                } else {
                    "no clip".into()
                };
            } else if !ctx.view_lets_through(s.def.viewpoint) {
                why = format!("viewpoint {} (listener {})", s.def.viewpoint, ctx.view);
            } else if let Some(c) = s
                .def
                .conditions
                .iter()
                .find(|c| s.def.triggers.is_empty() && !c.holds(var(&c.variable).unwrap_or(0.0)))
            {
                why = format!("condition {} = {:?}", c.variable, var(&c.variable));
            } else if let Some(vc) = s.def.vol_curves.iter().find(|vc| {
                let active = s.active_since.map_or(0.0, |t| ctx.now.saturating_duration_since(t).as_secs_f32());
                Self::curve_input(vc, var, active, 1.0).is_some_and(|x| curve(&vc.points, x) <= 1.0e-5)
            }) {
                why = format!("volcurve {} = {:?}", vc.variable, var(&vc.variable));
            } else if let Some(f) = Self::frequency(&s.def, var).filter(|f| !(*f >= FREQ_MIN)) {
                why = format!("frequency {f:.0} Hz ({} = {:?})", s.def.pitch_variable, var(&s.def.pitch_variable));
            }
            let playing = s.voice.and_then(|id| engine.voice_state(id));
            match (playing, why.is_empty()) {
                (Some((p, heard)), _) => out.push(format!("{file}: {heard:.3} heard, pitch {:.2}", p.pitch)),
                (None, true) => out.push(format!("{file}: ready, silent")),
                (None, false) => out.push(format!("{file}: off - {why}")),
            }
        }
        out
    }

    /// Every entry's level for one state, without playing anything (for tools comparing
    /// sound configurations): (file, gain at a listener at `listener` - distance included,
    /// pan left out -, playback rate, whether it would be started). `view_inside`/`cab` as
    /// for [`SoundSet::set_inside`]/[`SoundSet::set_muffled`]; triggered entries as if
    /// their trigger fired now.
    pub fn levels(
        &mut self,
        var: &dyn Fn(&str) -> Option<f32>,
        object_to_world: &Mat4,
        listener: Vec3,
    ) -> Vec<(String, f32, f32, bool)> {
        let ctx = Ctx { listener, ..self.ctx_without_engine() };
        self.sounds
            .iter_mut()
            .map(|s| {
                if !s.def.triggers.is_empty() || Self::conditions_hold(&s.def, var) {
                    s.active_since.get_or_insert_with(|| ctx.now - std::time::Duration::from_secs(5));
                }
                let e = ctx.eval(s, var, object_to_world);
                let dist = s
                    .def
                    .pos
                    .map(|p| crate::mixer::distance_gain(s.def.range, (object_to_world.transform_point3(Vec3::from_array(p)) - listener).length()))
                    .unwrap_or(1.0);
                let heard = if e.wrong_view || !e.audible { 0.0 } else { e.gain * dist };
                (s.def.file.trim().to_string(), heard, e.pitch, e.audible && !e.wrong_view)
            })
            .collect()
    }

    fn ctx_without_engine(&self) -> Ctx {
        Ctx { view: self.view_mask(), amount: self.amount, cab: self.muffled, master: self.master, doppler: !self.listener_vehicle, listener: Vec3::ZERO, now: std::time::Instant::now() }
    }

    pub fn stop_all(&mut self, engine: &AudioEngine) {
        for s in self.sounds.iter_mut() {
            if let Some(id) = s.voice.take() {
                engine.stop(id);
            }
        }
        for (_, p) in &mut self.parts {
            p.stop_all(engine);
        }
    }
}

/// The per-set inputs of an entry's evaluation.
struct Ctx {
    view: i32,
    /// How far the listener is inside this (the player's) vehicle, see
    /// [`SoundSet::set_inside_amount`]; not read for the views of other vehicles (4).
    amount: f32,
    cab: bool,
    master: f32,
    doppler: bool,
    listener: Vec3,
    /// The engine's clock now (`AudioEngine::clock`): the wall clock, or a recording's
    /// own - what `[volcurve] -1` counts its seconds in.
    now: std::time::Instant,
}

impl Ctx {
    /// The `[viewpoint]` test (0x750462): the entry is heard when it names the view, names
    /// none, or is an outside sound of the player's bus heard from its cab while
    /// `Snd_OutsideVol` is over 0.01 (doors or a window open).
    fn view_lets_through(&self, vp: i32) -> bool {
        if self.view & 4 != 0 {
            return passes(vp, self.view);
        }
        (self.amount > 0.0 && passes(vp, 2)) || (self.amount < 1.0 && passes(vp, 1))
    }

    /// The share of its volume an entry of the player's bus is heard at, with the listener
    /// `amount` inside: in the cab an outside sound at `Snd_OutsideVol`, the rest in full;
    /// walking through a door the two views' shares mixed. None when nothing changes (all
    /// in or all out, the entry heard in full).
    fn view_share(&self, vp: i32) -> Option<f32> {
        let outside_only = vp & 2 == 0 && vp != 0;
        if self.amount >= 1.0 {
            return outside_only.then(outside_vol);
        }
        if self.amount <= 0.0 {
            return None;
        }
        let cab = if !passes(vp, 2) { 0.0 } else if outside_only { outside_vol() } else { 1.0 };
        let street = if passes(vp, 1) { 1.0 } else { 0.0 };
        Some(self.amount * cab + (1.0 - self.amount) * street)
    }

    /// One entry this frame (`TSound` update 0x750340): its volume and pitch as DirectSound
    /// takes them, and whether it can be started.
    fn eval(&self, s: &mut RuntimeSound, var: &dyn Fn(&str) -> Option<f32>, object_to_world: &Mat4) -> Eval {
        let holds = !s.def.triggers.is_empty() || SoundSet::conditions_hold(&s.def, var);
        self.eval_holding(s, var, object_to_world, holds)
    }

    /// [`Ctx::eval`] with whether the entry is triggered or its conditions hold already known.
    fn eval_holding(&self, s: &mut RuntimeSound, var: &dyn Fn(&str) -> Option<f32>, object_to_world: &Mat4, holds: bool) -> Eval {
        self.eval_with(s, &ByName(var), object_to_world, holds)
    }

    /// [`Ctx::eval_holding`] with the variables read from `var`.
    fn eval_with<V: Vars + ?Sized>(&self, s: &mut RuntimeSound, var: &V, object_to_world: &Mat4, holds: bool) -> Eval {
        let def = &s.def;
        if !self.view_lets_through(def.viewpoint) {
            return Eval { gain: 0.0, audible: false, pitch: s.last_pitch, wrong_view: true };
        }
        let triggered = !def.triggers.is_empty();
        let pos = def.pos.map(|p| object_to_world.transform_point3(Vec3::from_array(p)));
        let active = if triggered {
            TRIGGERED_ACTIVE
        } else {
            s.active_since.map_or(0.0, |t| self.now.saturating_duration_since(t).as_secs_f32())
        };
        let facing = match (pos, def.dir) {
            (Some(at), Some(d)) => {
                let dir = object_to_world.transform_vector3(Vec3::from_array(d)).normalize_or_zero();
                dir.dot((self.listener - at).normalize_or_zero())
            }
            (Some(_), None) => 1.0,
            (None, _) => 0.0,
        };
        // (an untriggered entry whose conditions fail is at volume 0)
        let mut vol = if holds { def.volume * self.master } else { 0.0 };
        // A triggered sound takes the volume its curves give when it starts and keeps it
        // to its end, as the stock sound.cfg says of `[trigger]` entries ("Volumen und Pitch
        // [werden] nur einmalig beim Abspielstart zugewiesen"): the door hits of the SD200/
        // SD202 and of Kajosoft's buses read `doorSpeed_<n>`, which is 0 once the leaf has
        // stopped, and were cut to a short click a frame after they started (#611, #1268).
        // Where it is and how far away still change while it plays. What it keeps is the
        // loudest its curves have given since it fired: a sound fired while its variable is
        // still at rest and rising right after (the Thüringer Wald buses' steering pump,
        // whose start reads the steering's speed) was held at the silence of the moment it
        // fired, from 0.2.46 on, and never heard (#2552).
        let mut now = 1.0;
        for (k, (vc, &kind)) in def.vol_curves.iter().zip(&s.curve_inputs).enumerate() {
            if let Some(x) = SoundSet::curve_input_as(kind, vc, s.ix.curves.get(k).copied().unwrap_or(0), var, active, facing) {
                now *= curve(&vc.points, x);
            }
        }
        let curves = if triggered {
            let c = s.fired_curves.map_or(now, |kept| kept.max(now));
            s.fired_curves = Some(c);
            c
        } else {
            now
        };
        vol *= curves;
        // `[3d]`: full up to the range, then range / distance
        let dist_gain = pos.map(|p| crate::mixer::distance_gain(def.range, (p - self.listener).length()));
        if let Some(g) = dist_gain {
            vol *= g;
        }
        // an AI vehicle (view 4) heard from a cab: through the player's bus's bodywork
        // (stepping in through a door, by as much as the listener is in)
        if self.view & 4 != 0 && self.cab {
            let w = cab_amount();
            vol *= if w >= 1.0 { 0.2 + outside_vol() } else { 1.0 + (0.2 + outside_vol() - 1.0) * w };
        }
        // the player's bus's outside sound heard in its cab: through what is open
        if self.view & 4 == 0 {
            if let Some(share) = self.view_share(def.viewpoint) {
                vol *= share;
            }
        }
        let (gain, audible) = buffer_volume(vol, s.last_gain, master_volume());
        s.last_gain = gain;
        let mut audible = audible;
        let mut pitch = s.last_pitch;
        let freq = def.is_loop.then(|| def.sample_rate.trunc() * var.read(&def.pitch_variable, s.ix.pitch).unwrap_or(0.0).abs() / def.pitch_ref);
        if let (Some(f), Some(clip)) = (freq, s.clip.as_ref()) {
            // (not started below DirectSound's minimum; a request outside its range is
            // refused and the buffer keeps its rate)
            audible &= f >= FREQ_MIN;
            if (FREQ_MIN..=FREQ_MAX).contains(&f.trunc()) {
                pitch = f.trunc() / clip.sample_rate.max(1) as f32;
                s.last_pitch = pitch;
            }
        }
        // the mixer applies the distance itself, as the listener moves between frames
        let gain = match dist_gain {
            Some(g) if g > 0.0 => gain / g,
            Some(_) => 0.0,
            None => gain,
        };
        Eval { gain, audible, pitch, wrong_view: false }
    }

    fn params(&self, s: &RuntimeSound, e: &Eval, looping: bool, object_to_world: &Mat4) -> VoiceParams {
        let position = s.def.pos.map(|p| object_to_world.transform_point3(Vec3::from_array(p)));
        VoiceParams {
            gain: e.gain,
            pitch: e.pitch.max(0.001),
            looping,
            position,
            // (Omsi.exe shifts the frequency of a `[3d]` loop sound only: a `[sound]`
            // keeps its file's rate)
            doppler: self.doppler && s.def.is_loop,
            range: s.def.range,
            lowpass_hz: self.cab_lowpass(s.def.viewpoint),
            important: s.def.important,
        }
    }
}

impl Ctx {
    /// The low-pass an entry of the player's own bus is heard through (Hz, 0 for none): an
    /// outside sound in the cab comes through the gaps of what is open, and what comes
    /// through a crack loses its treble - see [`cab_lowpass_hz`]. Other vehicles' sounds
    /// keep theirs (they only get quieter in a cab, as in Omsi.exe).
    fn cab_lowpass(&self, vp: i32) -> f32 {
        if self.view & 4 != 0 || self.amount <= 0.0 || !(vp & 2 == 0 && vp != 0) {
            return 0.0;
        }
        cab_lowpass_hz(self.amount * outside_vol() + (1.0 - self.amount))
    }
}

/// How bright an outside sound comes into the cab through what is open (`open` as
/// `Snd_OutsideVol`, 0..1): through closed doors and windows (0) only what is under some
/// 500 Hz, with more open the corner rising (evenly in octaves) to the whole range, which
/// is reached at 0.95 - from there on no filter at all.
pub fn cab_lowpass_hz(open: f32) -> f32 {
    const OPEN: f32 = 0.95;
    if !(open < OPEN) {
        return 0.0;
    }
    500.0 * (20_000.0f32 / 500.0).powf(open.max(0.0) / OPEN)
}

/// Whether an entry with `[viewpoint] vp` is heard in `view` (0x750462): when it names the
/// view, names none, or is an outside sound of the player's bus heard from its cab while
/// `Snd_OutsideVol` is over 0.01 (doors or a window open).
fn passes(vp: i32, view: i32) -> bool {
    vp == 0 || vp & view != 0 || (vp & 2 == 0 && view == 2 && outside_vol() > 0.01)
}

/// How far a walker is inside the player's bus (bits of an f32; NaN: nobody walking about
/// it, see [`set_cab_amount`]).
static CAB_AMOUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x7fc0_0000);

/// How far a walker's eyes are inside the player's bus (0..1), or None while nobody walks
/// about it (the camera is then in a cab or not, as the views say). While it is given the
/// player's bus is heard by that much from inside ([`SoundSet::set_inside_amount`]) and
/// the traffic's sets heard from a cab (`set_muffled`) are only as muffled as that.
pub fn set_cab_amount(v: Option<f32>) {
    let v = match v {
        Some(x) if x > 0.0 => x.min(1.0),
        Some(_) => 0.0,
        None => f32::NAN,
    };
    CAB_AMOUNT.store(v.to_bits(), std::sync::atomic::Ordering::Relaxed);
}

/// What [`set_cab_amount`] was given last.
pub fn walker_cab_amount() -> Option<f32> {
    let v = f32::from_bits(CAB_AMOUNT.load(std::sync::atomic::Ordering::Relaxed));
    (!v.is_nan()).then_some(v)
}

/// How muffled a cab makes the traffic: all the way unless a walker is on the way in.
fn cab_amount() -> f32 {
    walker_cab_amount().unwrap_or(1.0)
}

static MASTER_VOLUME: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x3f80_0000);

pub fn set_master_volume(v: f32) {
    MASTER_VOLUME.store(v.clamp(0.0, 1.0).to_bits(), std::sync::atomic::Ordering::Relaxed);
}

fn master_volume() -> f32 {
    f32::from_bits(MASTER_VOLUME.load(std::sync::atomic::Ordering::Relaxed)).max(1.0e-3)
}

/// `Snd_OutsideVol` of the player's bus (bits of an f32): Omsi.exe's 0x859a04, 1 while the
/// player has no vehicle.
static OUTSIDE_VOL: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x3f80_0000);

/// Set the player's bus's `Snd_OutsideVol` (0 for a bus whose scripts never write it), or
/// `None` without a bus of one's own.
pub fn set_outside_open(v: Option<f32>) {
    let v = match v {
        Some(x) if x.is_finite() => x,
        Some(_) => 0.0,
        None => 1.0,
    };
    OUTSIDE_VOL.store(v.to_bits(), std::sync::atomic::Ordering::Relaxed);
}

pub(crate) fn outside_vol() -> f32 {
    f32::from_bits(OUTSIDE_VOL.load(std::sync::atomic::Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_vehicle::VolCurve;

    /// The tests that set the player's bus's `Snd_OutsideVol` or the walker's way in (one
    /// value for the whole program) take turns.
    static GLOBALS: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn globals() -> std::sync::MutexGuard<'static, ()> {
        GLOBALS.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn ctx(view: i32, cab: bool) -> Ctx {
        Ctx { view, amount: if view == 2 { 1.0 } else { 0.0 }, cab, master: 1.0, doppler: false, listener: Vec3::ZERO, now: std::time::Instant::now() }
    }

    fn eval(c: &Ctx, def: SoundEntry, var: &dyn Fn(&str) -> Option<f32>) -> Eval {
        let mut s = RuntimeSound::new(def, None);
        c.eval(&mut s, var, &Mat4::IDENTITY)
    }

    #[test]
    fn scenery_is_heard_through_the_bodywork_from_a_cab() {
        let _turn = globals();
        let none = |_: &str| None;
        set_outside_open(Some(0.5));
        let bells = SoundEntry { volume: 0.8, ..Default::default() };
        assert!((eval(&ctx(5, true), bells.clone(), &none).gain - 0.56).abs() < 1e-3);
        assert!((eval(&ctx(5, false), bells, &none).gain - 0.8).abs() < 1e-3, "from the street: as it is");
        assert!(!eval(&ctx(5, false), SoundEntry { volume: 1.0, viewpoint: 1, ..Default::default() }, &none).wrong_view);
        assert!(eval(&ctx(5, true), SoundEntry { volume: 1.0, viewpoint: 2, ..Default::default() }, &none).wrong_view);
        set_outside_open(None);
    }

    #[test]
    fn a_scenery_set_is_heard_as_scenery_through_a_cab() {
        let engine = AudioEngine::silent();
        let mut set = SoundSet::new_scenery(&engine, &omsi_vehicle::sound::SoundCfg::default(), Path::new("/nonexistent"));
        set.set_muffled(true);
        let c = set.ctx(&engine);
        assert_eq!((c.view, c.cab), (5, true));
        let mut ai = SoundSet::new_exterior(&engine, &omsi_vehicle::sound::SoundCfg::default(), Path::new("/nonexistent"));
        ai.set_muffled(true);
        assert_eq!(ai.ctx(&engine).view, 4, "an AI vehicle stays view 4");
    }

    #[test]
    fn curves_are_linear_and_held_at_the_ends() {
        let p = [(0.0, 0.0), (1.0, 1.0), (1.0, 0.5), (3.0, 0.0)];
        assert_eq!(curve(&p, -1.0), 0.0);
        assert_eq!(curve(&p, 0.5), 0.5);
        // at x = 1 the segment that starts there: (1, 0.5) .. (3, 0)
        assert_eq!(curve(&p, 1.0), 0.5);
        assert_eq!(curve(&p, 2.0), 0.25);
        assert_eq!(curve(&p, 9.0), 0.0);
        assert_eq!(curve(&[], 9.0), 1.0);
    }

    #[test]
    fn direct_sound_refuses_over_0_db() {
        assert_eq!(direct_sound_volume(1.0, 0.3), (1.0, true));
        // 44100 "as a volume": the buffer keeps what it had
        assert_eq!(direct_sound_volume(44100.0, 0.3), (0.3, true));
        assert_eq!(direct_sound_volume(1.3, 1.0), (1.0, true));
        let (g, _) = buffer_volume(2.0, 1.0, 0.3);
        assert!((g - 2.0).abs() < 0.01, "{g}");
        let (g, _) = buffer_volume(2.5, 2.0, 0.3);
        assert!((g - 2.5).abs() < 0.01, "{g}");
        assert_eq!(buffer_volume(2.0, 1.0, 1.0), (1.0, true));
        let (g, a) = direct_sound_volume(0.5, 1.0);
        assert!((g - 0.5).abs() < 1e-3 && a);
        assert_eq!(direct_sound_volume(0.0, 0.7), (0.0, false));
        // under -100 dB: refused and not started
        assert_eq!(direct_sound_volume(1.0e-6, 0.7), (0.7, false));
    }

    #[test]
    fn the_cab_hears_outside_through_what_is_open() {
        let _turn = globals();
        let none = |_: &str| None;
        // the own bus's outside-only entry (`[viewpoint] 5`) in the cab: at Snd_OutsideVol,
        // not at all with everything shut
        let engine_out = SoundEntry { volume: 0.8, viewpoint: 5, ..Default::default() };
        set_outside_open(Some(0.0));
        assert!(eval(&ctx(2, true), engine_out.clone(), &none).wrong_view);
        set_outside_open(Some(0.5));
        let e = eval(&ctx(2, true), engine_out.clone(), &none);
        assert!((e.gain - 0.4).abs() < 1e-3, "{}", e.gain);
        assert!((eval(&ctx(1, false), engine_out.clone(), &none).gain - 0.8).abs() < 1e-3, "outside: as it is");
        // an AI bus (view 4) heard from a cab: 0.2 + Snd_OutsideVol, no filter
        let ai = SoundEntry { volume: 0.5, viewpoint: 4, ..Default::default() };
        assert!((eval(&ctx(4, true), ai.clone(), &none).gain - 0.35).abs() < 1e-3);
        assert!((eval(&ctx(4, false), ai, &none).gain - 0.5).abs() < 1e-3, "from the street: as it is");
        // a scenery object heard from a cab: as an AI vehicle, its street sounds too
        let bells = SoundEntry { volume: 0.5, viewpoint: 1, ..Default::default() };
        assert!((eval(&ctx(5, true), bells.clone(), &none).gain - 0.35).abs() < 1e-3);
        assert!((eval(&ctx(5, false), bells, &none).gain - 0.5).abs() < 1e-3);
        // the player's bus heard from the street on an AI bus: never
        assert!(eval(&ctx(4, false), SoundEntry { volume: 1.0, viewpoint: 1, ..Default::default() }, &none).wrong_view);
        let cab = SoundEntry { volume: 0.8, viewpoint: 2, ..Default::default() };
        assert!(eval(&ctx(1, false), cab, &none).wrong_view, "a cab sound stays in");
        set_outside_open(None);
    }

    #[test]
    fn outside_sounds_in_the_cab_are_low_passed_by_how_open_it_is() {
        let _turn = globals();
        assert!((cab_lowpass_hz(0.0) - 500.0).abs() < 1e-3);
        assert_eq!(cab_lowpass_hz(0.95), 0.0, "open from 0.95 on");
        assert_eq!(cab_lowpass_hz(1.0), 0.0);
        let mut last = 0.0;
        for k in 0..95 {
            let f = cab_lowpass_hz(k as f32 / 100.0);
            assert!(f > last, "rising");
            last = f;
        }
        let engine_out = SoundEntry { volume: 0.8, viewpoint: 5, ..Default::default() };
        let s = RuntimeSound::new(engine_out.clone(), None);
        let e = Eval { gain: 1.0, audible: true, pitch: 1.0, wrong_view: false };
        set_outside_open(Some(0.3));
        let in_cab = ctx(2, true).params(&s, &e, true, &Mat4::IDENTITY);
        assert!((in_cab.lowpass_hz - cab_lowpass_hz(0.3)).abs() < 1e-3);
        // the street hears it as it is, and the cab sounds and the traffic are never filtered
        assert_eq!(ctx(1, false).params(&s, &e, true, &Mat4::IDENTITY).lowpass_hz, 0.0);
        let cab_only = RuntimeSound::new(SoundEntry { viewpoint: 2, ..engine_out.clone() }, None);
        assert_eq!(ctx(2, true).params(&cab_only, &e, true, &Mat4::IDENTITY).lowpass_hz, 0.0);
        assert_eq!(ctx(4, true).params(&s, &e, true, &Mat4::IDENTITY).lowpass_hz, 0.0);
        set_outside_open(Some(0.96));
        assert_eq!(ctx(2, true).params(&s, &e, true, &Mat4::IDENTITY).lowpass_hz, 0.0, "doors open: as before");
        set_outside_open(None);
    }

    #[test]
    fn stepping_in_crosses_from_the_street_to_the_cab() {
        let _turn = globals();
        let none = |_: &str| None;
        set_outside_open(Some(0.2));
        let half = |w: f32| Ctx { amount: w, ..ctx(if w >= 0.5 { 2 } else { 1 }, w >= 0.5) };
        let engine_out = SoundEntry { volume: 1.0, viewpoint: 1, ..Default::default() };
        let rain_in = SoundEntry { volume: 1.0, viewpoint: 2, ..Default::default() };
        let both = SoundEntry { volume: 1.0, viewpoint: 0, ..Default::default() };
        // all out and all in: as the two views always were
        assert_eq!(eval(&half(0.0), engine_out.clone(), &none).gain, 1.0);
        assert!(eval(&half(0.0), rain_in.clone(), &none).wrong_view);
        assert!((eval(&half(1.0), engine_out.clone(), &none).gain - 0.2).abs() < 1e-3);
        // a quarter in: three quarters of the street, a quarter of the cab
        let w = 0.25;
        assert!((eval(&half(w), engine_out.clone(), &none).gain - (0.75 + 0.25 * 0.2)).abs() < 2e-3);
        assert!((eval(&half(w), rain_in.clone(), &none).gain - 0.25).abs() < 2e-3);
        assert_eq!(eval(&half(w), both, &none).gain, 1.0);
        // the outside sound grows duller the further in
        let s = RuntimeSound::new(engine_out, None);
        let e = Eval { gain: 1.0, audible: true, pitch: 1.0, wrong_view: false };
        let f = |w: f32| half(w).params(&s, &e, true, &Mat4::IDENTITY).lowpass_hz;
        assert_eq!(f(0.0), 0.0);
        assert!(f(0.25) > f(0.75) && f(0.75) > f(1.0) && f(1.0) > 0.0, "{} {} {}", f(0.25), f(0.75), f(1.0));
        // the traffic slides from its street level to its cab level
        let ai = SoundEntry { volume: 1.0, viewpoint: 4, ..Default::default() };
        set_cab_amount(Some(0.5));
        assert!((eval(&ctx(4, true), ai.clone(), &none).gain - (1.0 + (0.4 - 1.0) * 0.5)).abs() < 2e-3);
        set_cab_amount(None);
        assert!((eval(&ctx(4, true), ai, &none).gain - 0.4).abs() < 2e-3);
        set_outside_open(None);
    }

    #[test]
    fn volume_curves_multiply_and_a_3d_sound_fades_with_distance() {
        let def = SoundEntry {
            volume: 1.0,
            pos: Some([0.0, 8.0, 0.0]),
            range: 2.0,
            vol_curves: vec![
                VolCurve { variable: "a".into(), points: vec![(0.0, 0.0), (1.0, 1.0)] },
                VolCurve { variable: "b".into(), points: vec![(0.0, 1.0), (1.0, 0.0)] },
            ],
            ..Default::default()
        };
        let var = |n: &str| Some(if n == "a" { 0.5 } else { 0.5 });
        let e = eval(&ctx(1, false), def, &var);
        // 0.5 * 0.5 * (2 / 8), the distance left to the mixer: 0.25 given to the voice
        assert!((e.gain - 0.25).abs() < 1e-3, "{}", e.gain);
        assert!(e.audible);
    }

    #[test]
    fn a_loop_sound_is_not_started_below_100_hz_and_keeps_its_rate_past_200_khz() {
        let def = SoundEntry { volume: 1.0, is_loop: true, sample_rate: 44100.0, pitch_variable: "n".into(), pitch_ref: 1000.0, ..Default::default() };
        let mut s = RuntimeSound::new(def, Some(Arc::new(Clip { sample_rate: 44100, channels: 1, samples: vec![0; 4] })));
        let c = ctx(1, false);
        let e = c.eval(&mut s, &|_| Some(1.0), &Mat4::IDENTITY);
        assert!(!e.audible, "44 Hz");
        let e = c.eval(&mut s, &|_| Some(2000.0), &Mat4::IDENTITY);
        assert!(e.audible && (e.pitch - 2.0).abs() < 1e-4);
        // 6 x 44100 = 264 600 Hz: refused, the rate stays at 2x
        let e = c.eval(&mut s, &|_| Some(6000.0), &Mat4::IDENTITY);
        assert!(e.audible && (e.pitch - 2.0).abs() < 1e-4, "{}", e.pitch);
    }

    /// The SD200's door hit: `[volcurve] doorSpeed_0` from 1 at -1 down to 0 at -0.5,
    /// fired while the door still closes at -1 m/s; by the frame's end the script has turned
    /// the speed round. The volume is the one of the moment it fired (#676).
    #[test]
    fn a_frame_reads_each_variable_once() {
        let e = AudioEngine::capture(48_000);
        let clip = Arc::new(Clip { sample_rate: 48_000, channels: 1, samples: vec![1000; 4800] });
        let entry = SoundEntry {
            volume: 1.0,
            conditions: vec![omsi_vehicle::sound::Condition { variable: "engine_on".into(), value: 1.0, relation: 1, kind: Default::default() }],
            vol_curves: vec![VolCurve { variable: "n".into(), points: vec![(0.0, 0.0), (1.0, 1.0)] }, VolCurve { variable: "-1".into(), points: vec![(0.0, 1.0), (1.0, 1.0)] }],
            ..Default::default()
        };
        let sounds = (0..3).map(|_| RuntimeSound::new(entry.clone(), Some(clip.clone()))).collect();
        let mut set = SoundSet { sounds, master: 1.0, dir: Default::default(), inside: false, amount: 0.0, ai: false, scenery: false, listener_vehicle: false, muffled: false, parts: Vec::new(), vars: None };
        let reads = std::cell::Cell::new(0);
        let var = |n: &str| {
            reads.set(reads.get() + 1);
            Some(if n == "n" { 0.5 } else { 1.0 })
        };
        set.update(&e, &var, &Mat4::IDENTITY, &[]);
        assert_eq!(reads.get(), 2, "engine_on and n, once each");
        assert_eq!(set.sounds.iter().filter(|s| s.voice.is_some_and(|id| e.is_playing(id))).count(), 3);
        let gains: Vec<f32> = set.sounds.iter().filter_map(|s| e.voice_state(s.voice?)).map(|v| v.1).collect();
        assert!(gains.len() == 3 && gains.iter().all(|g| (g - 0.5).abs() < 0.01), "the curve read through the table: {gains:?}");
    }

    #[test]
    fn a_triggered_sound_reads_its_curve_when_it_fires() {
        let hit = SoundEntry {
            volume: 1.0,
            triggers: vec!["ev_doorhitclose_0".into()],
            vol_curves: vec![VolCurve { variable: "doorSpeed_0".into(), points: vec![(-1.0, 1.0), (-0.5, 0.0)] }],
            ..Default::default()
        };
        let now = |n: &str| (n == "doorSpeed_0").then_some(0.8);
        let at_fire = |t: &str, n: &str| (t == "ev_doorhitclose_0" && n == "doorSpeed_0").then_some(-1.0);
        assert_eq!(eval(&ctx(2, true), hit.clone(), &now).gain, 0.0, "read at the frame's end: silent");
        let fired = |n: &str| at_fire("ev_doorhitclose_0", n).or_else(|| now(n));
        assert_eq!(eval(&ctx(2, true), hit.clone(), &fired).gain, 1.0);
        let set = SoundSet { sounds: vec![RuntimeSound::new(hit.clone(), None)], master: 1.0, dir: Default::default(), inside: true, amount: 1.0, ai: false, scenery: false, listener_vehicle: true, muffled: false, parts: Vec::new(), vars: None };
        assert_eq!(set.curve_triggers(), vec!["ev_doorhitclose_0".to_string()]);
    }

    /// The door hit keeps the volume it fired with to its end, though `doorSpeed_0` is 0
    /// from the next frame on (#611, #1268); fired again, it reads the curves anew.
    #[test]
    fn articulated_parts_use_trigger_time_curves_and_keep_them() {
        let _g = globals();
        let engine = AudioEngine::capture(48_000);
        let clip = Arc::new(Clip { sample_rate: 48_000, channels: 1, samples: vec![1000; 4800] });
        let entry = SoundEntry {
            volume: 1.0, triggers: vec!["door_hit".into()],
            vol_curves: vec![VolCurve { variable: "door_speed".into(), points: vec![(-1.0, 1.0), (0.0, 0.0)] }],
            ..Default::default()
        };
        let make = || SoundSet { sounds: vec![RuntimeSound::new(entry.clone(), Some(clip.clone()))], master: 1.0,
            dir: Default::default(), inside: false, amount: 0.0, ai: false, scenery: false, listener_vehicle: false,
            muffled: false, parts: Vec::new(), vars: None };
        let mut set = make();
        set.add_part(0, make());
        set.update_parts(&engine, &|_| Some(0.0), &|_| Some(Mat4::IDENTITY),
            &["door_hit".into()], &|t, n| (t == "door_hit" && n == "door_speed").then_some(-1.0));
        assert_eq!(set.parts[0].1.sounds[0].fired_curves, Some(1.0));
        set.update_parts(&engine, &|_| Some(0.0), &|_| Some(Mat4::IDENTITY), &[], &|_, _| None);
        assert_eq!(set.parts[0].1.sounds[0].fired_curves, Some(1.0));
    }

    #[test]
    fn a_triggered_sound_keeps_the_volume_it_started_with() {
        let _g = globals();
        let hit = SoundEntry {
            volume: 1.0,
            triggers: vec!["ev_doorhitclose_0".into()],
            vol_curves: vec![VolCurve { variable: "doorSpeed_0".into(), points: vec![(-1.0, 1.0), (-0.5, 0.0)] }],
            ..Default::default()
        };
        let c = ctx(2, true);
        let mut s = RuntimeSound::new(hit, None);
        let fast = |n: &str| (n == "doorSpeed_0").then_some(-1.0);
        let stopped = |n: &str| (n == "doorSpeed_0").then_some(0.0);
        let slow = |n: &str| (n == "doorSpeed_0").then_some(-0.75);
        assert_eq!(c.eval_holding(&mut s, &fast, &Mat4::IDENTITY, true).gain, 1.0, "as it fires");
        assert_eq!(c.eval_holding(&mut s, &stopped, &Mat4::IDENTITY, true).gain, 1.0, "the leaf has stopped: still playing out");
        s.fired_curves = None;
        assert!((c.eval_holding(&mut s, &slow, &Mat4::IDENTITY, true).gain - 0.5).abs() < 1e-4, "fired again, slower");
    }

    /// The stock IBIS announces Spandau's "Falkenseer Ch/Stadtrandstr" with the file
    /// `Falkenseer Ch%Stadtrandstr.wav`, as Omsi.exe's file triggers write `/` (#1096).
    #[test]
    fn a_file_trigger_writes_what_a_file_name_cannot_have_as_percent() {
        let file = r"..\..\Announcements\Spandau\Falkenseer Ch/Stadtrandstr.wav";
        assert_eq!(file_trigger_name(file), r"..\..\Announcements\Spandau\Falkenseer Ch%Stadtrandstr.wav");
        assert_eq!(file_trigger_name(r"C:\a?b*c\d<e>f|g.wav"), r"C:\a%b%c\d%e%f%g.wav");
        // found where the announcement lies
        let root = std::env::temp_dir().join(format!("openomsi-file-trigger-{}", std::process::id()));
        let sound = root.join("Vehicles").join("MAN_NL_NG").join("Sound");
        let spandau = root.join("Vehicles").join("Announcements").join("Spandau");
        std::fs::create_dir_all(&sound).unwrap();
        std::fs::create_dir_all(&spandau).unwrap();
        std::fs::write(spandau.join("Falkenseer Ch%Stadtrandstr.wav"), b"RIFF").unwrap();
        let path = omsi_cfg::resolve_path(&sound, &file_trigger_name(file));
        assert!(path.is_file(), "{}", path.display());
        // (the name as the script wrote it: a folder "Falkenseer Ch" that is not there)
        assert!(!omsi_cfg::resolve_path(&sound, file).is_file());
        let _ = std::fs::remove_dir_all(&root);
    }
}
