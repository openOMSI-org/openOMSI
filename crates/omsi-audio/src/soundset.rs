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

mod only_one;
use only_one::OnlyOne;

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
    /// An `[onlyone]` entry: the file it shares with the other objects' entries.
    only_one: Option<OnlyOne>,
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
        RuntimeSound {
            clip,
            voice: None,
            held: false,
            active_since: None,
            last_gain: 1.0,
            last_pitch: 1.0,
            curve_inputs,
            ix: VarIx::default(),
            fired_curves: None,
            only_one: def
                .only_one
                .then(|| OnlyOne::new(def.file.trim().to_ascii_lowercase())),
            def,
        }
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
    /// This set as a sound pack, for `[sound_maxcount]` (see `crate::packs`).
    pack: crate::packs::PackInfo,
}

/// The file a `(T.F.)` trigger names, as Omsi.exe opens it (0x74f2e8 through 0x7eed78): the
/// characters a Windows file name cannot have are `%` in it, the folders' `\` and a drive's
/// `:` stay. The stock IBIS announces a stop by its name, and Spandau's "Falkenseer
/// Ch/Stadtrandstr" is `Falkenseer Ch%Stadtrandstr.wav`: read as a folder, it was not found
/// and the stop was not announced (#1096).
fn file_trigger_name(file: &str) -> String {
    file.chars()
        .map(|c| {
            if matches!(c, '/' | '?' | '*' | '"' | '<' | '>' | '|') {
                '%'
            } else {
                c
            }
        })
        .collect()
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
    let i = (1..points.len())
        .find(|&i| x < points[i].0)
        .unwrap_or(points.len() - 1);
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
                let mut s = RuntimeSound::new(def.clone(), clip);
                // (Omsi.exe tells `[onlyone]` sounds apart by their whole path)
                if s.only_one.is_some() {
                    s.only_one = Some(OnlyOne::new(path.to_string_lossy().to_ascii_lowercase()));
                }
                s
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
            pack: crate::packs::PackInfo::of(&cfg.sounds),
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
            let curves = s
                .def
                .vol_curves
                .iter()
                .zip(&s.curve_inputs)
                .map(|(vc, k)| {
                    if *k == CurveInput::Var {
                        at(&vc.variable)
                    } else {
                        0
                    }
                })
                .collect();
            let pitch = if s.def.is_loop {
                at(&s.def.pitch_variable)
            } else {
                0
            };
            s.ix = VarIx {
                conds,
                curves,
                pitch,
            };
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
    /// trigger had fired (0x74f2e8). An empty `file` loads nothing: 0x74f2e8 skips the
    /// load for an empty string and fires the entry all the same, so it plays again the
    /// file it already has (its own, or the one the last `(T.F.)` gave it) from the start.
    ///
    /// Any other name first frees the entry's buffer (0x74fb3c calls 0x75030c: the playing
    /// sound stops) before the new file is loaded. A file that cannot be loaded - `" "`, a
    /// name of blanks only, names none - leaves the entry without a buffer and marked
    /// "not loaded" (0x74fe28 sets +0x1c, which 0x750340 skips the entry for): silent, its
    /// `""` re-fires too, until a `(T.F.)` names a file that loads.
    pub fn play_file_trigger(
        &mut self,
        engine: &AudioEngine,
        trigger: &str,
        file: &str,
        var: &dyn Fn(&str) -> Option<f32>,
        object_to_world: &Mat4,
    ) {
        // too far for `[sound_maxcount]`: 0x74f2e8 plays nothing
        if engine.pack_too_far(self.pack.id) {
            return;
        }
        if !engine.enabled
            || !self.sounds.iter().any(|s| {
                s.def
                    .triggers
                    .iter()
                    .any(|d| d.trim().eq_ignore_ascii_case(trigger))
            })
        {
            return;
        }
        let loaded = if file.is_empty() {
            None
        } else {
            // (blanks only: no file, and nothing to look for on disk)
            let clip = (!file.trim().is_empty())
                .then(|| omsi_cfg::resolve_path(&self.dir, &file_trigger_name(file)))
                .and_then(|path| {
                    let clip = engine.load_clip(&path);
                    if clip.is_none() {
                        warn_missing_once(trigger, &path);
                    }
                    clip
                });
            let Some(clip) = clip else {
                self.free_file_trigger_entries(engine, trigger);
                return;
            };
            Some(clip)
        };
        let ctx = self.ctx(engine);
        for s in self.sounds.iter_mut() {
            if !s
                .def
                .triggers
                .iter()
                .any(|d| d.trim().eq_ignore_ascii_case(trigger))
            {
                continue;
            }
            if let Some(id) = s.voice.take() {
                engine.stop(id);
            }
            if let Some(clip) = &loaded {
                s.clip = Some(clip.clone());
            }
            // (none yet: its own file was not found and no `(T.F.)` gave it one)
            let Some(clip) = s.clip.clone() else { continue };
            s.last_gain = 1.0;
            s.last_pitch = 1.0;
            let e = ctx.eval(s, var, object_to_world);
            if e.audible && !e.wrong_view {
                s.voice = Some(engine.play(clip, ctx.params(s, &e, false, object_to_world)));
            }
        }
    }

    /// The entries listening to `trigger` after a `(T.F.)` whose file did not load: their
    /// buffer freed (0x75030c stops the playing sound) and none in its place, so they stay
    /// silent until a `(T.F.)` gives them a file that loads (see `play_file_trigger`).
    fn free_file_trigger_entries(&mut self, engine: &AudioEngine, trigger: &str) {
        for s in self.sounds.iter_mut() {
            if s.def
                .triggers
                .iter()
                .any(|d| d.trim().eq_ignore_ascii_case(trigger))
            {
                if let Some(id) = s.voice.take() {
                    engine.stop(id);
                }
                s.clip = None;
            }
        }
    }

    fn ctx(&self, engine: &AudioEngine) -> Ctx {
        Ctx {
            listener: engine.listener_position(),
            now: engine.clock(),
            ..self.ctx_without_engine()
        }
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
        s.def.conditions.iter().enumerate().all(|(k, c)| {
            c.holds(
                var.read(&c.variable, s.ix.conds.get(k).copied().unwrap_or(0))
                    .unwrap_or(0.0),
            )
        })
    }

    /// A `[loopsound]`'s frequency request in Hz (`rate * |variable| / reference`), `None`
    /// for a `[sound]`, which plays at its file's own rate.
    fn frequency(def: &SoundEntry, var: &dyn Fn(&str) -> Option<f32>) -> Option<f32> {
        def.is_loop.then(|| {
            def.sample_rate.trunc() * var(&def.pitch_variable).unwrap_or(0.0).abs() / def.pitch_ref
        })
    }

    /// An entry plays on sound trigger `name`.
    pub fn has_trigger(&self, name: &str) -> bool {
        self.sounds.iter().any(|s| {
            s.def
                .triggers
                .iter()
                .any(|t| t.trim().eq_ignore_ascii_case(name))
        })
    }

    /// An entry's volume curve or condition reads variable `name`.
    pub fn reads_var(&self, name: &str) -> bool {
        let is = |v: &str| v.trim().eq_ignore_ascii_case(name);
        self.sounds.iter().any(|s| {
            s.def.vol_curves.iter().any(|c| is(&c.variable))
                || s.def.conditions.iter().any(|c| is(&c.variable))
        })
    }

    /// The triggers (lower case) whose entries read script variables in a volume curve:
    /// what [`SoundSet::update_fired`] wants the values of at the moment they fire.
    pub fn curve_triggers(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for s in &self.sounds {
            if !s
                .def
                .vol_curves
                .iter()
                .any(|vc| vc.variable.trim().parse::<i32>().is_err())
            {
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
        // `[sound_maxcount]`: a pack too far is silent and not updated (0x74f224)
        if !self.pack_heard(engine, object_to_world) {
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
        // (how near this object is among those sharing an `[onlyone]` sound)
        let only_one_key = if self.sounds.iter().any(|s| s.only_one.is_some()) {
            let at = object_to_world.transform_point3(Vec3::ZERO);
            (at - ctx.listener).length() / only_one::pack_range(self.sounds.iter().map(|s| &s.def))
        } else {
            0.0
        };
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
            let Some(clip) = s.clip.clone() else {
                if let Some(o) = &mut s.only_one {
                    o.release();
                }
                continue;
            };
            let fired_by = if triggered {
                triggers.iter().find(|t| {
                    s.def
                        .triggers
                        .iter()
                        .any(|d| d.trim().eq_ignore_ascii_case(t))
                })
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
                Some(t) => ctx.eval_holding(
                    s,
                    &|n| at_fire(t, n).or_else(|| var(n)),
                    object_to_world,
                    holds,
                ),
                None => ctx.eval_with(s, &table, object_to_world, holds),
            };
            if e.wrong_view {
                // "Stopped Sound ... globalstop/wrongview"
                if let Some(id) = s.voice.take() {
                    engine.stop(id);
                }
                if let Some(o) = &mut s.only_one {
                    o.release();
                }
                continue;
            }
            if let Some(o) = &mut s.only_one {
                // whether it plays after this update (Omsi.exe's +0x7c): a loop while it is
                // heard; a one-shot started now, or still playing - one fired again unheard
                // plays on (below), as does one whose volume has fallen
                let playing = s.voice.is_some_and(|id| engine.is_playing(id));
                let plays = if looping {
                    e.audible
                } else {
                    playing || (e.audible && (fired || rising))
                };
                if !o.permits(only_one_key, plays) {
                    // a nearer object plays this sound: this one is stopped
                    if let Some(id) = s.voice.take() {
                        engine.stop(id);
                    }
                    continue;
                }
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
            } else if let Some(c) =
                s.def.conditions.iter().find(|c| {
                    s.def.triggers.is_empty() && !c.holds(var(&c.variable).unwrap_or(0.0))
                })
            {
                why = format!("condition {} = {:?}", c.variable, var(&c.variable));
            } else if let Some(vc) = s.def.vol_curves.iter().find(|vc| {
                let active = s
                    .active_since
                    .map_or(0.0, |t| ctx.now.saturating_duration_since(t).as_secs_f32());
                Self::curve_input(vc, var, active, 1.0)
                    .is_some_and(|x| curve(&vc.points, x) <= 1.0e-5)
            }) {
                why = format!("volcurve {} = {:?}", vc.variable, var(&vc.variable));
            } else if let Some(f) = Self::frequency(&s.def, var).filter(|f| !(*f >= FREQ_MIN)) {
                why = format!(
                    "frequency {f:.0} Hz ({} = {:?})",
                    s.def.pitch_variable,
                    var(&s.def.pitch_variable)
                );
            }
            let playing = s.voice.and_then(|id| engine.voice_state(id));
            match (playing, why.is_empty()) {
                (Some((p, heard)), _) => {
                    out.push(format!("{file}: {heard:.3} heard, pitch {:.2}", p.pitch))
                }
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
        let ctx = Ctx {
            listener,
            ..self.ctx_without_engine()
        };
        self.sounds
            .iter_mut()
            .map(|s| {
                if !s.def.triggers.is_empty() || Self::conditions_hold(&s.def, var) {
                    s.active_since
                        .get_or_insert_with(|| ctx.now - std::time::Duration::from_secs(5));
                }
                let e = ctx.eval(s, var, object_to_world);
                let dist = s
                    .def
                    .pos
                    .map(|p| {
                        crate::mixer::distance_gain(
                            s.def.range,
                            (object_to_world.transform_point3(Vec3::from_array(p)) - listener)
                                .length(),
                        )
                    })
                    .unwrap_or(1.0);
                let heard = if e.wrong_view || !e.audible {
                    0.0
                } else {
                    e.gain * dist
                };
                (
                    s.def.file.trim().to_string(),
                    heard,
                    e.pitch,
                    e.audible && !e.wrong_view,
                )
            })
            .collect()
    }

    fn ctx_without_engine(&self) -> Ctx {
        Ctx {
            view: self.view_mask(),
            amount: self.amount,
            cab: self.muffled,
            master: self.master,
            doppler: !self.listener_vehicle,
            listener: Vec3::ZERO,
            now: std::time::Instant::now(),
        }
    }

    pub fn stop_all(&mut self, engine: &AudioEngine) {
        for s in self.sounds.iter_mut() {
            if let Some(id) = s.voice.take() {
                engine.stop(id);
            }
            if let Some(o) = &mut s.only_one {
                o.release();
            }
        }
        for (_, p) in &mut self.parts {
            p.stop_all(engine);
        }
    }

    /// `[sound_maxcount]`: say where this set is as a sound pack and whether it may be
    /// heard (see `crate::packs`); the sounds of one too far are stopped ("Stopped Sound
    /// ... too far", 0x750340). Its coupled parts are packs of their own.
    fn pack_heard(&mut self, engine: &AudioEngine, object_to_world: &Mat4) -> bool {
        // The user's vehicles go in at 0 (0x751398 with +0x26c): the sets made with `new`
        // are the bus the player drives (and one ridden in, heard from inside - the
        // nearest pack anyway). The others at their distance from the camera over their
        // range (0x74f4fc).
        let key = if !self.ai && !self.scenery {
            0.0
        } else {
            let at = object_to_world.transform_point3(Vec3::ZERO);
            (at - engine.listener_position()).length() / self.pack.range
        };
        let heard = engine.pack_heard(crate::packs::Pack {
            id: self.pack.id,
            key,
            entries: self.pack.entries,
        });
        if !heard {
            for s in self.sounds.iter_mut() {
                if let Some(id) = s.voice.take() {
                    engine.stop(id);
                }
            }
        }
        heard
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
        let cab = if !passes(vp, 2) {
            0.0
        } else if outside_only {
            outside_vol()
        } else {
            1.0
        };
        let street = if passes(vp, 1) { 1.0 } else { 0.0 };
        Some(self.amount * cab + (1.0 - self.amount) * street)
    }

    /// One entry this frame (`TSound` update 0x750340): its volume and pitch as DirectSound
    /// takes them, and whether it can be started.
    fn eval(
        &self,
        s: &mut RuntimeSound,
        var: &dyn Fn(&str) -> Option<f32>,
        object_to_world: &Mat4,
    ) -> Eval {
        let holds = !s.def.triggers.is_empty() || SoundSet::conditions_hold(&s.def, var);
        self.eval_holding(s, var, object_to_world, holds)
    }

    /// [`Ctx::eval`] with whether the entry is triggered or its conditions hold already known.
    fn eval_holding(
        &self,
        s: &mut RuntimeSound,
        var: &dyn Fn(&str) -> Option<f32>,
        object_to_world: &Mat4,
        holds: bool,
    ) -> Eval {
        self.eval_with(s, &ByName(var), object_to_world, holds)
    }

    /// [`Ctx::eval_holding`] with the variables read from `var`.
    fn eval_with<V: Vars + ?Sized>(
        &self,
        s: &mut RuntimeSound,
        var: &V,
        object_to_world: &Mat4,
        holds: bool,
    ) -> Eval {
        let def = &s.def;
        if !self.view_lets_through(def.viewpoint) {
            return Eval {
                gain: 0.0,
                audible: false,
                pitch: s.last_pitch,
                wrong_view: true,
            };
        }
        let triggered = !def.triggers.is_empty();
        let pos = def
            .pos
            .map(|p| object_to_world.transform_point3(Vec3::from_array(p)));
        let active = if triggered {
            TRIGGERED_ACTIVE
        } else {
            s.active_since
                .map_or(0.0, |t| self.now.saturating_duration_since(t).as_secs_f32())
        };
        let facing = match (pos, def.dir) {
            (Some(at), Some(d)) => {
                let dir = object_to_world
                    .transform_vector3(Vec3::from_array(d))
                    .normalize_or_zero();
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
            if let Some(x) = SoundSet::curve_input_as(
                kind,
                vc,
                s.ix.curves.get(k).copied().unwrap_or(0),
                var,
                active,
                facing,
            ) {
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
        let dist_gain =
            pos.map(|p| crate::mixer::distance_gain(def.range, (p - self.listener).length()));
        if let Some(g) = dist_gain {
            vol *= g;
        }
        // an AI vehicle (view 4) heard from a cab: through the player's bus's bodywork
        // (stepping in through a door, by as much as the listener is in)
        if self.view & 4 != 0 && self.cab {
            let w = cab_amount();
            vol *= if w >= 1.0 {
                0.2 + outside_vol()
            } else {
                1.0 + (0.2 + outside_vol() - 1.0) * w
            };
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
        let freq = def.is_loop.then(|| {
            def.sample_rate.trunc()
                * var
                    .read(&def.pitch_variable, s.ix.pitch)
                    .unwrap_or(0.0)
                    .abs()
                / def.pitch_ref
        });
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
        Eval {
            gain,
            audible,
            pitch,
            wrong_view: false,
        }
    }

    fn params(
        &self,
        s: &RuntimeSound,
        e: &Eval,
        looping: bool,
        object_to_world: &Mat4,
    ) -> VoiceParams {
        let position = s
            .def
            .pos
            .map(|p| object_to_world.transform_point3(Vec3::from_array(p)));
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
    MASTER_VOLUME.store(
        v.clamp(0.0, 1.0).to_bits(),
        std::sync::atomic::Ordering::Relaxed,
    );
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
mod tests;

#[cfg(test)]
mod file_trigger_tests;
