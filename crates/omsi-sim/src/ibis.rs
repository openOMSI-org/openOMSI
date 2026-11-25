//! Typing a duty into a bus's IBIS the way a driver does it, whatever unit the bus has.
//!
//! The stock IBIS takes the line after its line key, the route after its route key, and
//! each after the entry key. Mods build very different units on the same variables: the
//! FloFix "Atron" wants the driver's PIN first and then walks from the line to the route
//! and the destination by itself; the BVG Citaro's ALMEX ticket machine boots for 17 s,
//! wants the driver's card, number and PIN, takes line, suffix, course and route as one
//! number after its L/S/K key (modes 102/103), makes the driver wait (104) and confirm the
//! destination (105), the first stop (106) and then the whole new route twice before the
//! IBIS shows it - typed the stock way, nothing of that happened and the destination had to
//! be written into the variables directly.
//!
//! So nothing here knows a unit. The compiled scripts say which triggers are the number
//! keys (`<prefix>_0` … `<prefix>_9`), which the entry key (it reads the typed number),
//! which switch the unit's mode (`IBIS_mode`), which log the driver in (the constfile
//! numbers the entry key compares with) and which advance the stop; the model and the
//! keyboard say which of them a driver can reach. A [`Plan`] is then found by trying the
//! keys out on copies of the bus's script state with the scripts' own frames running in
//! between: a key is pressed when it has an effect (a unit that is still booting or shows
//! "please wait" is waited for), the typed codes are tried in the layouts the digit
//! keys take, and entries are confirmed for as long as the unit asks - until the IBIS
//! variables show the duty. The plan is then played on the bus at a driver's pace,
//! checking on the way that the unit is where the trial run was.

use crate::{VehicleHost, VehicleInstance};
use hashbrown::HashSet;
use omsi_script::{BlockId, Op, Program, State, VarId, Vm};
use std::sync::Arc;

/// What the IBIS should show when the typing is done.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Target {
    /// The IBIS line number (the depot file's route code without its last two digits).
    pub line: u32,
    pub suffix: u32,
    /// The route within the line (the route code's last two digits).
    pub route: Option<u32>,
    /// The destination code, typed when the depot file has no route for the trip.
    pub terminus_code: Option<u32>,
    /// `IBIS_RouteIndex` the route leads to (None: not checked).
    pub route_index: Option<i32>,
    /// `IBIS_TerminusIndex` the entry leads to.
    pub terminus_index: i32,
    /// The stop of the route the IBIS should stand at (the duty's next stop).
    pub stop: usize,
}

/// Seconds between two frames of a trial run.
const STEP: f32 = 0.05;
/// A key is held this long, and the next one pressed this long after it is let go.
const HOLD: f32 = 0.15;
const GAP: f32 = 0.2;
/// How long a trial waits for a unit to take a key (a boot, a "please wait" screen).
const BOOT_WAIT: f32 = 40.0;
const KEY_WAIT: f32 = 12.0;
/// How long a next-stop key is given before the next one is tried: the unit has just taken
/// the whole entry, so a key that does nothing now does nothing (the O530 Facelift's
/// mute key never moves the stop, and waiting 12 s for it at every stop took 54 s).
const STOP_KEY_WAIT: f32 = 1.0;
/// Confirmations a unit may ask for after an entry.
const MAX_CONFIRM: usize = 8;
/// How long the real run waits for the unit to reach the state the trial had before a key.
const SLIP: f32 = 6.0;

/// One key press of a plan.
#[derive(Debug, Clone, PartialEq)]
pub struct Press {
    /// Seconds after the start of the plan.
    pub at: f32,
    pub key: String,
    /// `IBIS_mode` just before the press in the trial run.
    pub mode: Option<f32>,
    /// The press announced a stop in the trial run.
    pub announces: bool,
    /// A stop passed over on the way to the duty's stop: its announcement is not played.
    pub quiet: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Plan {
    pub presses: Vec<Press>,
    /// Seconds from the start to the end of the trial run.
    pub length: f32,
    /// What was done, for the log.
    pub summary: String,
}

/// What the scripts tell about the unit.
#[derive(Debug, Clone, Default)]
struct Unit {
    mode: Option<VarId>,
    line: Option<VarId>,
    route: Option<VarId>,
    terminus: Option<VarId>,
    busstop: Option<VarId>,
    /// Digit keys: the prefix and the ten triggers.
    keypads: Vec<(String, Vec<String>)>,
    enters: Vec<String>,
    mode_keys: Vec<String>,
    /// Pure toggles the unit's own script tests against 1 before it shows its keypad
    /// (the ALMEX's driver card).
    toggles: Vec<(String, VarId)>,
    /// Numbers the entry key compares the typed number with (driver number, PIN).
    logins: Vec<u32>,
    /// Keys that advance the stop, silent ones first.
    next_stop: Vec<String>,
    /// The variable the number keys look at before they type (the ALMEX's screen), the
    /// keys that set it, and the value the driver drives with.
    session: Option<(VarId, Vec<String>, Option<f32>)>,
    /// For a unit without such a gate: the screen variable the mode keys switch to their
    /// entry screen (the FloFix Atron's `atron_display_mode` 3), and the keys that switch it
    /// elsewhere without typing (its "Löschen" goes back to the main menu with the ticket
    /// keys).
    home: Option<(VarId, Vec<String>)>,
    /// Float and string variables the number keys type into (the entry key reads them).
    input: Vec<VarId>,
    input_str: Vec<u32>,
    /// State variables an entry key press may move.
    watch: Vec<VarId>,
    watch_str: Vec<u32>,
    /// Learnt by trial, not from the scripts (see `differs`): mode keys may take two presses.
    dynamic: bool,
}

/// Every op of `block` and the macros it calls, once each.
fn each_op(p: &Program, block: BlockId, f: &mut dyn FnMut(&[Op], usize)) {
    let mut seen = HashSet::new();
    let mut todo = vec![block];
    while let Some(b) = todo.pop() {
        if !seen.insert(b) {
            continue;
        }
        let Some(blk) = p.blocks.get(b as usize) else {
            continue;
        };
        for (i, op) in blk.ops.iter().enumerate() {
            if let Op::Macro(m) | Op::FallThrough(m) = op {
                todo.push(*m);
            }
            f(&blk.ops, i);
        }
    }
}

/// The value a `Store` at `i` writes when it is a literal or constfile number.
fn stored_constant(ops: &[Op], i: usize) -> Option<f32> {
    match ops[..i].iter().rev().find(|o| !matches!(o, Op::Store(_))) {
        Some(Op::Push(c)) | Some(Op::Const(c)) => Some(*c),
        _ => None,
    }
}

fn stores(p: &Program, block: BlockId) -> (HashSet<VarId>, HashSet<u32>) {
    let (mut f, mut s) = (HashSet::new(), HashSet::new());
    each_op(p, block, &mut |ops, i| match &ops[i] {
        Op::Store(v) => {
            f.insert(*v);
        }
        Op::StoreStr(v) => {
            s.insert(*v);
        }
        _ => {}
    });
    (f, s)
}

fn loads(p: &Program, block: BlockId) -> (HashSet<VarId>, HashSet<u32>) {
    let (mut f, mut s) = (HashSet::new(), HashSet::new());
    each_op(p, block, &mut |ops, i| match &ops[i] {
        Op::Load(v) => {
            f.insert(*v);
        }
        Op::LoadStr(v) => {
            s.insert(*v);
        }
        _ => {}
    });
    (f, s)
}

/// Constants `block` (with its macros) stores into `var`.
fn constants_stored(p: &Program, block: BlockId, var: VarId) -> Vec<f32> {
    let mut out = Vec::new();
    each_op(p, block, &mut |ops, i| {
        if ops[i] == Op::Store(var) {
            if let Some(c) = stored_constant(ops, i) {
                out.push(c);
            }
        }
    });
    out
}

/// Values the block itself (not its macros) compares `var` with (`(L.L.var) c =`).
fn gate_values(p: &Program, block: BlockId, var: VarId) -> Vec<f32> {
    let Some(b) = p.blocks.get(block as usize) else {
        return Vec::new();
    };
    b.ops
        .windows(3)
        .filter_map(|w| {
            if let (Op::Load(v), Op::Push(c), Op::Eq) = (&w[0], &w[1], &w[2]) {
                (*v == var).then_some(*c)
            } else {
                None
            }
        })
        .collect()
}

impl Unit {
    fn learn(p: &Program, operable: &dyn Fn(&str) -> bool) -> Unit {
        let mut u = Unit {
            mode: p.var("IBIS_mode"),
            line: p.var("IBIS_LinieKurs").or_else(|| p.var("IBIS_Linie")),
            route: p
                .var("IBIS_RouteIndex")
                .or_else(|| p.var("ibox_routenindex")),
            terminus: p.var("IBIS_TerminusIndex"),
            // Aachen's ibox keeps the stop on `ibox_busstop` (no stock `IBIS_busstop`).
            busstop: p.var("IBIS_busstop").or_else(|| p.var("ibox_busstop")),
            ..Default::default()
        };
        let names: Vec<String> = {
            let mut n: Vec<String> = p.triggers.keys().cloned().collect();
            n.sort();
            n
        };
        let press_keys: Vec<&String> = names
            .iter()
            .filter(|n| !n.ends_with("_off") && !n.ends_with("_drag") && !n.starts_with("ai_"))
            .collect();
        // number keys: ten triggers <prefix>0 … <prefix>9; a driver's first
        for driver_only in [true, false] {
            for n in &press_keys {
                let Some(prefix) = n.strip_suffix('0') else {
                    continue;
                };
                if prefix.is_empty() || prefix.ends_with(|c: char| c.is_ascii_digit()) {
                    continue;
                }
                let keys: Vec<String> = (0..10).map(|d| format!("{prefix}{d}")).collect();
                if keys
                    .iter()
                    .all(|k| p.trigger(k).is_some() && (!driver_only || operable(k)))
                {
                    u.keypads.push((prefix.to_string(), keys));
                }
            }
            if !u.keypads.is_empty() {
                break;
            }
        }
        let digit_keys: HashSet<String> =
            u.keypads.iter().flat_map(|k| k.1.iter().cloned()).collect();
        // what the number keys type into: written by them (not by letting go) and read by
        // an entry key - or by the number keys themselves, like the count of digits typed
        // (a leading 0 changes nothing else)
        let mut typed = (HashSet::new(), HashSet::new());
        let mut digit_reads = (HashSet::new(), HashSet::new());
        for k in &digit_keys {
            let b = p.trigger(k).unwrap();
            let (f, s) = stores(p, b);
            let (of, os) = p
                .trigger(&format!("{k}_off"))
                .map(|o| stores(p, o))
                .unwrap_or_default();
            typed
                .0
                .extend(f.difference(&of).copied().filter(|v| Some(*v) != u.mode));
            typed.1.extend(s.difference(&os).copied());
            let (lf, ls) = loads(p, b);
            digit_reads.0.extend(lf);
            digit_reads.1.extend(ls);
        }
        let refused = |n: &str| {
            [
                "cancel", "loesch", "lösch", "clear", "storno", "korr", "back", "rueck", "zurueck",
            ]
            .iter()
            .any(|w| n.contains(w))
        };
        let driver = |n: &str| operable(n) || !press_keys.iter().any(|k| operable(k));
        // entry keys: read what was typed and move the mode
        for n in &press_keys {
            if digit_keys.contains(n.as_str()) || refused(n) || !driver(n) {
                continue;
            }
            let b = p.trigger(n).unwrap();
            let (lf, ls) = loads(p, b);
            let reads_input =
                typed.0.iter().any(|v| lf.contains(v)) || typed.1.iter().any(|v| ls.contains(v));
            let moves_mode = u.mode.map(|m| stores(p, b).0.contains(&m)).unwrap_or(false);
            if reads_input && moves_mode {
                u.enters.push(n.to_string());
            }
        }
        let rank = |n: &str, words: &[&str]| {
            words
                .iter()
                .position(|w| n.contains(w))
                .unwrap_or(words.len())
        };
        // a unit whose keys only store a key code for its frame macro: the entry key by its
        // name, among the keys of the keypad's own script
        if u.enters.is_empty() && !u.keypads.is_empty() {
            let pad_file = p
                .trigger(&u.keypads[0].1[1])
                .map(|b| p.blocks[b as usize].file.clone());
            for n in &press_keys {
                let low = n.to_ascii_lowercase();
                let same_file = p.trigger(n).map(|b| p.blocks[b as usize].file.clone()) == pad_file;
                if digit_keys.contains(n.as_str()) || refused(&low) || !driver(n) || !same_file {
                    continue;
                }
                if ["eingabe", "enter", "ok", "quit", "bestaet", "confirm", "_e"]
                    .iter()
                    .any(|w| low.contains(w))
                {
                    u.enters.push(n.to_string());
                }
            }
        }
        u.enters
            .sort_by_key(|n| rank(n, &["eingabe", "enter", "ok", "quit"]));
        for e in &u.enters {
            let b = p.trigger(e).unwrap();
            let (lf, ls) = loads(p, b);
            u.input.extend(
                typed
                    .0
                    .iter()
                    .filter(|v| lf.contains(*v) || digit_reads.0.contains(*v)),
            );
            u.input_str.extend(
                typed
                    .1
                    .iter()
                    .filter(|v| ls.contains(*v) || digit_reads.1.contains(*v)),
            );
            // the numbers it compares with: `… (C.L.driver_pin) =`
            each_op(p, b, &mut |ops, i| {
                if let (Op::Const(c), Some(Op::Eq)) = (&ops[i], ops.get(i + 1)) {
                    if *c >= 1.0 && *c < 1e8 && c.fract() == 0.0 && !u.logins.contains(&(*c as u32))
                    {
                        u.logins.push(*c as u32);
                    }
                }
            });
        }
        u.input.sort_unstable();
        u.input.dedup();
        u.input_str.sort_unstable();
        u.input_str.dedup();
        // mode keys: store a mode other than 0
        if let Some(m) = u.mode {
            for n in &press_keys {
                if digit_keys.contains(n.as_str())
                    || u.enters.contains(n)
                    || refused(n)
                    || !driver(n)
                {
                    continue;
                }
                if constants_stored(p, p.trigger(n).unwrap(), m)
                    .iter()
                    .any(|c| *c != 0.0)
                {
                    u.mode_keys.push(n.to_string());
                }
            }
            u.mode_keys.sort_by_key(|n| {
                rank(
                    n,
                    &[
                        "linie", "line", "lsk", "kurs", "route", "ziel", "dest", "terminus",
                    ],
                )
            });
            // no key stores a mode: the frame macro switches it on a key code, so any key of
            // the keypad may open an entry screen (the Procity's 7 = line, 0 then 3 = code)
            if u.mode_keys.is_empty() && u.input.is_empty() && u.input_str.is_empty() {
                let pad_file = u
                    .keypads
                    .first()
                    .and_then(|k| p.trigger(&k.1[1]))
                    .map(|b| p.blocks[b as usize].file.clone());
                for n in &press_keys {
                    let same_file =
                        p.trigger(n).map(|b| p.blocks[b as usize].file.clone()) == pad_file;
                    if same_file
                        && driver(n)
                        && !u.enters.contains(n)
                        && !refused(&n.to_ascii_lowercase())
                    {
                        u.mode_keys.push(n.to_string());
                    }
                }
                u.dynamic = true;
            }
        }
        // variables that hold a state (compared with numbers somewhere)
        let mut states: HashSet<VarId> = HashSet::new();
        for b in &p.blocks {
            for w in b.ops.windows(3) {
                if let (Op::Load(v), Op::Push(_), Op::Eq) = (&w[0], &w[1], &w[2]) {
                    states.insert(*v);
                }
            }
        }
        for e in &u.enters {
            let b = p.trigger(e).unwrap();
            let (f, s) = stores(p, b);
            let (of, os) = p
                .trigger(&format!("{e}_off"))
                .map(|o| stores(p, o))
                .unwrap_or_default();
            let goal = [u.mode, u.line, u.route, u.terminus, u.busstop];
            u.watch.extend(f.iter().filter(|v| {
                !of.contains(*v)
                    && (states.contains(*v) || u.input.contains(*v) || goal.contains(&Some(**v)))
            }));
            u.watch_str.extend(
                s.iter()
                    .filter(|v| !os.contains(*v) && u.input_str.contains(*v)),
            );
        }
        // (a key-code unit's entry key stores only the key code, which its frame resets:
        // what the entry moves is found by comparing everything)
        if u.dynamic {
            u.watch.clear();
            u.watch_str.clear();
        }
        u.watch.sort_unstable();
        u.watch.dedup();
        u.watch_str.sort_unstable();
        u.watch_str.dedup();
        // a card or switch of the unit: a pure toggle its script compares with 1
        let unit_files: HashSet<&std::path::Path> = u
            .keypads
            .iter()
            .flat_map(|k| k.1.iter())
            .filter_map(|k| p.trigger(k))
            .map(|b| p.blocks[b as usize].file.as_path())
            .collect();
        for n in &press_keys {
            if !driver(n) {
                continue;
            }
            let blk = &p.blocks[p.trigger(n).unwrap() as usize];
            let ops: Vec<&Op> = blk
                .ops
                .iter()
                .filter(|o| !matches!(o, Op::SoundTrigger(_)))
                .collect();
            let [Op::Load(a), Op::Not, Op::Store(b)] = ops.as_slice() else {
                continue;
            };
            if a != b {
                continue;
            }
            let tested = p.blocks.iter().filter(|x| unit_files.contains(x.file.as_path())).any(|x| x.ops.windows(3).any(|w| matches!((&w[0], &w[1], &w[2]), (Op::Load(v), Op::Push(c), Op::Eq) if v == a && *c == 1.0)));
            if tested {
                u.toggles.push((n.to_string(), *a));
            }
        }
        // keys that move the IBIS on by a stop: `(L.L.IBIS_busstop) 1 + (S.L.IBIS_busstop)`
        if let Some(bs) = u.busstop {
            for n in &press_keys {
                if !driver(n) || digit_keys.contains(n.as_str()) {
                    continue;
                }
                let mut adds = false;
                each_op(p, p.trigger(n).unwrap(), &mut |ops, i| {
                    if i >= 3
                        && ops[i] == Op::Store(bs)
                        && ops[i - 1] == Op::Add
                        && ops[i - 2] == Op::Push(1.0)
                        && ops[i - 3] == Op::Load(bs)
                    {
                        adds = true;
                    }
                });
                if adds {
                    u.next_stop.push(n.to_string());
                }
            }
            u.next_stop
                .sort_by_key(|n| rank(n, &["stumm", "mute", "silent", "quiet"]));
        }
        // the variable the number keys test before they type (the ALMEX's screen), the keys
        // that set it, and the screen a driver drives with: where the entry and stop keys
        // work but the number keys do not
        if let Some((_, keys)) = u.keypads.first() {
            let blk = &p.blocks[p.trigger(&keys[1]).unwrap() as usize];
            let gate = blk
                .ops
                .windows(3)
                .find_map(|w| {
                    if let (Op::Load(v), Op::Push(_), Op::Eq) = (&w[0], &w[1], &w[2]) {
                        Some(*v)
                    } else {
                        None
                    }
                })
                .filter(|v| Some(*v) != u.mode);
            if let Some(g) = gate {
                let setters: Vec<String> = press_keys
                    .iter()
                    .filter(|n| {
                        driver(n)
                            && !digit_keys.contains(n.as_str())
                            && constants_stored(p, p.trigger(n).unwrap(), g)
                                .iter()
                                .any(|c| *c != 0.0)
                    })
                    .map(|n| n.to_string())
                    .collect();
                let values = |keys: &[String]| -> HashSet<i64> {
                    keys.iter()
                        .filter_map(|k| p.trigger(k))
                        .flat_map(|b| gate_values(p, b, g))
                        .map(|c| c as i64)
                        .collect()
                };
                let digits = values(keys);
                let enter = values(&u.enters);
                let stop = values(&u.next_stop);
                let mut driving: Vec<i64> = enter
                    .iter()
                    .filter(|v| !digits.contains(*v) && (stop.is_empty() || stop.contains(*v)))
                    .copied()
                    .collect();
                driving.sort_unstable();
                u.session = Some((g, setters, driving.first().map(|v| *v as f32)));
            }
        }
        if let (None, Some(m)) = (&u.session, u.mode) {
            let goal = [u.line, u.route, u.terminus, u.busstop];
            let mut entry_screens: Vec<(VarId, f32)> = Vec::new();
            for k in &u.mode_keys {
                each_op(p, p.trigger(k).unwrap(), &mut |ops, i| {
                    if let Op::Store(v) = ops[i] {
                        if let Some(c) = stored_constant(ops, i) {
                            if c != 0.0
                                && v != m
                                && !u.input.contains(&v)
                                && !goal.contains(&Some(v))
                                && states.contains(&v)
                                && !entry_screens.contains(&(v, c))
                            {
                                entry_screens.push((v, c));
                            }
                        }
                    }
                });
            }
            for (v, c) in entry_screens {
                let keys: Vec<String> = press_keys
                    .iter()
                    .filter(|n| {
                        refused(n)
                            && driver(n)
                            && !digit_keys.contains(n.as_str())
                            && !u.enters.contains(n)
                            && !u.mode_keys.contains(n)
                    })
                    .filter(|n| {
                        constants_stored(p, p.trigger(n).unwrap(), v)
                            .iter()
                            .any(|x| *x != c && *x != 0.0)
                    })
                    .map(|n| n.to_string())
                    .collect();
                if !keys.is_empty() {
                    u.home = Some((v, keys));
                    break;
                }
            }
        }
        u
    }

    fn describe(&self, p: &Program) -> String {
        let vn = |v: &VarId| p.var_names.get(*v as usize).cloned().unwrap_or_default();
        format!(
            "keypads {:?}, entry {:?}, modes {:?}, log-in {:?}, toggles {:?}, next stop {:?}, screen {:?}, home {:?}, typed into {:?}",
            self.keypads.iter().map(|k| &k.0).collect::<Vec<_>>(),
            self.enters,
            self.mode_keys,
            self.logins,
            self.toggles.iter().map(|t| &t.0).collect::<Vec<_>>(),
            self.next_stop,
            self.session
                .as_ref()
                .map(|(g, k, h)| (vn(g), k.clone(), *h)),
            self.home.as_ref().map(|(g, k)| (vn(g), k.clone())),
            self.input.iter().map(vn).collect::<Vec<_>>()
        )
    }
}

/// A trial run on a copy of the bus's script state.
struct Sim {
    program: Arc<Program>,
    state: State,
    host: VehicleHost,
    vm: Vm,
    t: f32,
    presses: Vec<Press>,
    mode: Option<VarId>,
}

impl Clone for Sim {
    fn clone(&self) -> Sim {
        Sim {
            program: self.program.clone(),
            state: self.state.clone(),
            host: self.host.scratch(),
            vm: Vm::new(),
            t: self.t,
            presses: self.presses.clone(),
            mode: self.mode,
        }
    }
}

thread_local! {
    /// When the trials of this thread give up (see `TRIAL_BUDGET`): past it the copies of
    /// the bus stand still, so every trial fails at once and the search ends.
    static DEADLINE: std::cell::Cell<Option<std::time::Instant>> = const { std::cell::Cell::new(None) };
}

/// How long the search for a way to type may take. The stock units need a tenth of a
/// second, a unit that boots and wants a log-in a second or two; the Atron RBL of the
/// Citaro C2 (Ahlheim) - a menu the trials cannot find their way through - kept a worker
/// busy for two and a half minutes, and the displays stayed blank until the duty was set
/// directly after that. `OMSI_IBIS_BUDGET` (seconds) changes it.
fn trial_budget() -> std::time::Duration {
    let s = omsi_cfg::flags::OMSI_IBIS_BUDGET
        .parse::<f64>()
        .unwrap_or(10.0);
    std::time::Duration::from_secs_f64(s.max(0.1))
}

fn out_of_time() -> bool {
    DEADLINE.with(|d| d.get().is_some_and(|t| std::time::Instant::now() >= t))
}

impl Sim {
    fn frames(&mut self, secs: f32) {
        if out_of_time() {
            self.t += secs;
            return;
        }
        let n = (secs / STEP).round().max(1.0) as usize;
        for _ in 0..n {
            self.host.clock.advance(STEP);
            let p = self.program.clone();
            self.vm.run_frame(&p, &mut self.state, &mut self.host);
            self.t += STEP;
        }
        self.host.fired_triggers.clear();
        self.host.fired_trigger_vars.clear();
        self.host.fired_file_triggers.clear();
        self.host.messages.clear();
    }

    fn var(&self, v: Option<VarId>) -> Option<f32> {
        v.and_then(|v| self.state.vars.get(v as usize).copied())
    }

    fn press(&mut self, key: &str) {
        let mode = self.var(self.mode);
        let p = self.program.clone();
        self.host.fired_file_triggers.clear();
        let at = self.t;
        self.vm
            .run_trigger(&p, key, &mut self.state, &mut self.host);
        let announces = !self.host.fired_file_triggers.is_empty();
        self.presses.push(Press {
            at,
            key: key.to_string(),
            mode,
            announces,
            quiet: false,
        });
        self.frames(HOLD);
        self.vm
            .run_trigger(&p, &format!("{key}_off"), &mut self.state, &mut self.host);
        self.frames(GAP);
    }

    /// Press `key` once it does something (`effect` compares the pressed copy with one that
    /// was left alone for as long), waiting up to `wait` seconds for the unit to take it.
    fn press_when(&mut self, key: &str, wait: f32, effect: &dyn Fn(&Sim, &Sim) -> bool) -> bool {
        let mut waited = 0.0;
        loop {
            let mut pressed = self.clone();
            pressed.press(key);
            let mut idle = self.clone();
            idle.frames(HOLD + GAP);
            if effect(&pressed, &idle) {
                *self = pressed;
                return true;
            }
            if waited >= wait {
                return false;
            }
            self.frames(0.25);
            waited += 0.25;
        }
    }
}

/// Whether two trial states differ in the given variables - in any variable when none are
/// given: a unit whose keys only store a key code (`IBIS_Taste`) that its frame macro reads
/// (Krüger's IBIS, the Procity's) says nothing statically about what typing moves.
fn differs(a: &Sim, b: &Sim, vars: &[VarId], strs: &[u32]) -> bool {
    if vars.is_empty() && strs.is_empty() {
        return a.state.vars != b.state.vars || a.state.str_vars != b.state.str_vars;
    }
    vars.iter()
        .any(|v| a.state.vars.get(*v as usize) != b.state.vars.get(*v as usize))
        || strs
            .iter()
            .any(|v| a.state.str_vars.get(*v as usize) != b.state.str_vars.get(*v as usize))
}

/// Digit layouts to try for an entry that takes up to `room` digits.
fn layouts(t: &Target, room: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let ls = t.line * 100 + t.suffix;
    let mut add = |s: String| {
        if s.len() <= room && !out.contains(&s) {
            out.push(s);
        }
    };
    if let Some(r) = t.route {
        // line, suffix, course and route in one number (ALMEX L/S/K/R)
        if room >= 9 {
            add(format!("{:03}{:02}00{:02}", t.line, t.suffix, r));
            add(format!("{:03}{:02}00{:03}", t.line, t.suffix, r));
        }
    }
    add(format!("{ls:03}"));
    if let Some(r) = t.route {
        add(format!("{r:02}"));
    }
    if let Some(z) = t.terminus_code {
        add(format!("{z:03}"));
    }
    add(format!("{ls:05}"));
    // units that take the numbers as they are (the Procity's line field: "76")
    add(format!("{}", t.line));
    if let Some(r) = t.route {
        add(format!("{r}"));
    }
    if let Some(z) = t.terminus_code {
        add(format!("{z}"));
    }
    out
}

/// A running typing job: the plan found for the bus, played at a driver's pace.
pub struct Typist {
    target: Target,
    unit: Unit,
    plan: Option<Plan>,
    /// Seconds into the plan, how far the plan was held back to wait for the unit, the
    /// next press and the keys to let go.
    t: f32,
    slip: f32,
    waited: f32,
    next: usize,
    releases: Vec<(f32, String)>,
    outcome: Option<Result<String, String>>,
    /// The plan still being looked for on a worker thread.
    planning: Option<std::sync::mpsc::Receiver<Result<Plan, String>>>,
}

impl Typist {
    /// Learn the unit and look for a plan for `target` on a copy of the bus's state, then
    /// type it. `operable` says whether a trigger is a key the driver can reach. The trial
    /// runs take from a tenth of a second (the stock IBIS) to a second and a half (a unit
    /// that boots, wants a log-in and asks twice): in `background` they run on a worker
    /// thread and the typing starts when they are done - the keys come a little later than
    /// the trial had them, which only finds the unit further along.
    pub fn new(
        v: &VehicleInstance,
        target: Target,
        operable: &dyn Fn(&str) -> bool,
        background: bool,
    ) -> Typist {
        let p = v.ty.program.clone();
        let unit = Unit::learn(&p, operable);
        if debug() {
            log::info!("IBIS unit: {}; target {target:?}", unit.describe(&p));
        }
        let base = Sim {
            program: p.clone(),
            state: v.state.clone(),
            host: v.host.scratch(),
            vm: Vm::new(),
            t: 0.0,
            presses: Vec::new(),
            mode: unit.mode,
        };
        let mut typist = Typist {
            target: target.clone(),
            unit: unit.clone(),
            plan: None,
            t: 0.0,
            slip: 0.0,
            waited: 0.0,
            next: 0,
            releases: Vec::new(),
            outcome: None,
            planning: None,
        };
        let search = move || {
            let t0 = std::time::Instant::now();
            DEADLINE.with(|d| d.set(Some(t0 + trial_budget())));
            let found = find_plan(&unit, &target, base);
            DEADLINE.with(|d| d.set(None));
            let found = found.filter(|_| t0.elapsed() < trial_budget());
            match found {
                Some(pl) => {
                    log::info!(
                        "IBIS: {} ({} keys over {:.1} s, found in {:.0} ms)",
                        pl.summary,
                        pl.presses.len(),
                        pl.length,
                        t0.elapsed().as_secs_f64() * 1000.0
                    );
                    Ok(pl)
                }
                None => Err(format!(
                    "no way to type the duty found ({}; {:.0} ms of trials)",
                    unit.describe(&p),
                    t0.elapsed().as_secs_f64() * 1000.0
                )),
            }
        };
        if background {
            let (tx, rx) = std::sync::mpsc::channel();
            let spawned = std::thread::Builder::new()
                .name("ibis-typist".into())
                .spawn(move || {
                    let _ = tx.send(search());
                });
            match spawned {
                Ok(_) => typist.planning = Some(rx),
                Err(e) => typist.outcome = Some(Err(format!("no thread for the trials: {e}"))),
            }
        } else {
            typist.take(search());
        }
        typist
    }

    fn take(&mut self, found: Result<Plan, String>) {
        match found {
            Ok(plan) => self.plan = Some(plan),
            Err(e) => self.outcome = Some(Err(e)),
        }
    }

    /// Whether the IBIS variables show the target.
    pub fn shows(&self, v: &VehicleInstance) -> bool {
        goal_met(&self.unit, &self.target, &|id| {
            v.state.vars.get(id as usize).copied()
        })
    }

    /// The result once the typing is over: what was typed, or why it could not be.
    pub fn outcome(&self) -> Option<&Result<String, String>> {
        self.outcome.as_ref()
    }

    /// Play the plan on the bus; false once it is over.
    pub fn tick(&mut self, v: &mut VehicleInstance, dt: f32) -> bool {
        if let Some(rx) = self.planning.as_ref() {
            match rx.try_recv() {
                Ok(found) => {
                    self.planning = None;
                    self.take(found);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return true,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.planning = None;
                    self.outcome = Some(Err("the trials ended without a result".into()));
                }
            }
        }
        if self.outcome.is_some() {
            return false;
        }
        let Some(plan) = self.plan.as_ref() else {
            return false;
        };
        self.t += dt;
        let now = self.t - self.slip;
        let mut i = 0;
        while i < self.releases.len() {
            if self.releases[i].0 <= self.t {
                let (_, key) = self.releases.swap_remove(i);
                v.trigger(&format!("{key}_off"));
            } else {
                i += 1;
            }
        }
        if let Some(press) = plan.presses.get(self.next) {
            if now >= press.at {
                let mode = self
                    .unit
                    .mode
                    .and_then(|m| v.state.vars.get(m as usize).copied());
                if press.mode.is_none() || mode == press.mode {
                    let announced = v.host.fired_file_triggers.len();
                    v.trigger(&press.key);
                    if press.quiet {
                        v.host.fired_file_triggers.truncate(announced);
                    }
                    self.releases.push((self.t + HOLD, press.key.clone()));
                    self.next += 1;
                    self.waited = 0.0;
                } else {
                    // the unit is not there yet (a real frame is not a trial frame): wait
                    self.slip += dt;
                    self.waited += dt;
                    if self.waited > SLIP {
                        let msg = format!(
                            "gave up at key {} ({}): the unit is in mode {:?}, the trial had {:?}",
                            self.next + 1,
                            press.key,
                            mode,
                            press.mode
                        );
                        self.finish(v, Err(msg));
                        return false;
                    }
                }
            }
            return true;
        }
        if now < plan.length || !self.releases.is_empty() {
            return true;
        }
        let summary = plan.summary.clone();
        if self.shows(v) {
            self.finish(v, Ok(summary));
        } else {
            let got = [self.unit.line, self.unit.route, self.unit.terminus]
                .map(|x| x.and_then(|m| v.state.vars.get(m as usize).copied()));
            self.finish(
                v,
                Err(format!(
                    "typed, but the IBIS shows line/route/terminus {got:?}"
                )),
            );
        }
        false
    }

    /// Stop typing (a new entry replaces this one): the keys still held are let go.
    pub fn abandon(&mut self, v: &mut VehicleInstance) {
        if self.outcome.is_none() {
            self.finish(v, Err("replaced by a new entry".into()));
        }
    }

    fn finish(&mut self, v: &mut VehicleInstance, outcome: Result<String, String>) {
        for (_, key) in self.releases.drain(..) {
            v.trigger(&format!("{key}_off"));
        }
        self.outcome = Some(outcome);
    }
}

fn debug() -> bool {
    omsi_cfg::flags::OMSI_DEBUG_IBIS.is_set()
}

fn goal_met(u: &Unit, t: &Target, var: &dyn Fn(VarId) -> Option<f32>) -> bool {
    let is =
        |v: Option<VarId>, x: f32| v.and_then(var).map(|y| (y - x).abs() < 0.5).unwrap_or(true);
    let has_any = u.line.is_some() || u.route.is_some() || u.terminus.is_some();
    has_any
        && is(u.line, t.line as f32)
        && t.route_index.map(|r| is(u.route, r as f32)).unwrap_or(true)
        && is(u.terminus, t.terminus_index as f32)
}

/// How many of the target's variables a state shows.
fn progress(u: &Unit, t: &Target, s: &Sim) -> usize {
    let is = |v: Option<VarId>, x: f32| {
        v.is_some() && s.var(v).map(|y| (y - x).abs() < 0.5).unwrap_or(false)
    };
    is(u.line, t.line as f32) as usize
        + t.route_index
            .map(|r| is(u.route, r as f32) as usize)
            .unwrap_or(0)
        + is(u.terminus, t.terminus_index as f32) as usize
}

fn met(u: &Unit, t: &Target, s: &Sim) -> bool {
    goal_met(u, t, &|v| s.var(Some(v)))
}

fn find_plan(u: &Unit, t: &Target, base: Sim) -> Option<Plan> {
    if u.keypads.is_empty() || u.enters.is_empty() {
        return None;
    }
    let mut steps = Vec::new();
    let mut sim = if met(u, t, &base) {
        steps.push("the IBIS already shows the duty".to_string());
        base
    } else {
        entry_with_prerequisites(u, t, &base, &mut steps)?
    };
    // the stop the bus stands at
    if let Some(bs) = u.busstop {
        let start = sim.var(Some(bs)).unwrap_or(0.0).round() as i64;
        let mut at = start;
        // the key found to move the stop is used for the rest; the others get a moment
        let mut working: Option<&String> = None;
        let mut quiet = 0;
        while at < t.stop as i64 {
            // the silent key for the stops passed over, the announcing one for the last
            let last = at + 1 == t.stop as i64;
            let order: Vec<&String> = if last {
                u.next_stop.iter().rev().collect()
            } else {
                u.next_stop.iter().collect()
            };
            let mut tries: Vec<(&String, f32)> = order
                .iter()
                .map(|k| {
                    (
                        *k,
                        if Some(*k) == working {
                            KEY_WAIT
                        } else {
                            STOP_KEY_WAIT
                        },
                    )
                })
                .collect();
            if working.is_none() {
                if let Some(t) = tries.last_mut() {
                    t.1 = KEY_WAIT;
                }
            }
            let mut moved = None;
            for (k, wait) in tries {
                if sim.press_when(k, wait, &|a, b| a.var(Some(bs)) > b.var(Some(bs))) {
                    moved = Some(k);
                    break;
                }
            }
            let Some(k) = moved else {
                steps.push(format!("could not move on to stop {}", t.stop));
                break;
            };
            if !last {
                working = Some(k);
                // a stop passed over with a key that announces (the unit's silent key does
                // not work): the announcement is left out, as the silent key would have
                if let Some(pr) = sim.presses.iter_mut().rev().find(|p| p.key == *k) {
                    if pr.announces {
                        pr.quiet = true;
                        quiet += 1;
                    }
                }
            }
            at = sim.var(Some(bs)).unwrap_or(0.0).round() as i64;
        }
        if at > start {
            steps.push(format!(
                "stop {at}{}",
                if quiet > 0 {
                    format!(" ({quiet} passed over without their announcement)")
                } else {
                    String::new()
                }
            ));
        }
    }
    // back to the screen the driver drives with
    if let Some((g, keys, Some(home))) = &u.session {
        if sim.var(Some(*g)) != Some(*home) {
            for k in keys {
                let mut tried = sim.clone();
                tried.press(k);
                tried.frames(0.5);
                if tried.var(Some(*g)) == Some(*home) && met(u, t, &tried) {
                    steps.push(format!("screen {home} ({k})"));
                    sim = tried;
                    break;
                }
            }
        }
    }
    // or, on a unit without that gate, back from the entry screen the mode keys opened
    if let (None, Some((g, keys))) = (&u.session, &u.home) {
        for k in keys {
            let mut tried = sim.clone();
            tried.press(k);
            tried.frames(0.5);
            let moved_on = tried.var(Some(*g)) != sim.var(Some(*g));
            let kept = met(u, t, &tried)
                && tried.var(u.mode) == sim.var(u.mode)
                && u.busstop
                    .map(|b| tried.var(Some(b)) == sim.var(Some(b)))
                    .unwrap_or(true);
            if moved_on && kept {
                steps.push(format!(
                    "screen {} ({k})",
                    tried.var(Some(*g)).unwrap_or(0.0)
                ));
                sim = tried;
                break;
            }
        }
    }
    sim.frames(0.5);
    Some(Plan {
        length: sim.t,
        summary: steps.join(", "),
        presses: sim.presses,
    })
}

/// The entry, after the driver's card, log-in and screen where the unit wants them. The
/// log-in is tried at once, then not at all (a driver already logged in, who may have to
/// open the unit's menu first), then after waiting for the unit to boot.
fn entry_with_prerequisites(
    u: &Unit,
    t: &Target,
    base: &Sim,
    steps: &mut Vec<String>,
) -> Option<Sim> {
    let logins: &[Option<f32>] = if u.logins.is_empty() {
        &[None]
    } else {
        &[Some(2.0), None, Some(BOOT_WAIT)]
    };
    let p = &base.program;
    let file_of = |key: &str| p.trigger(key).map(|b| p.blocks[b as usize].file.clone());
    for (prefix, digits) in &u.keypads {
        // the entry key of the keypad's own script first (the ALMEX's is the ticket
        // printer's, the IBIS's its own)
        let home = file_of(&digits[1]);
        let mut enters: Vec<&String> = u.enters.iter().collect();
        enters.sort_by_key(|e| file_of(e) != home);
        for enter in enters {
            for login in logins {
                let mut s = base.clone();
                let mut done: Vec<String> = Vec::new();
                for (key, var) in &u.toggles {
                    if s.var(Some(*var)) == Some(0.0) {
                        s.press(key);
                        done.push(format!("{key} on"));
                    }
                }
                if let Some(wait) = login {
                    let mut step = String::new();
                    let logged_in = u.logins.iter().all(|n| {
                        let typed = type_number(u, &mut s, digits, &n.to_string(), *wait);
                        let entered = typed
                            && s.press_when(enter, KEY_WAIT, &|a, b| {
                                differs(a, b, &u.watch, &u.watch_str)
                            });
                        step = format!("{n}: typed {typed} entered {entered}");
                        entered
                    });
                    if debug() {
                        log::info!(
                            "IBIS log-in trial with {enter} after {wait} s: {} ({step})",
                            if logged_in { "done" } else { "failed" }
                        );
                    }
                    if !logged_in {
                        continue;
                    }
                    done.push(format!("log-in {:?}", u.logins));
                }
                // the unit's screen as it is, else each one a key opens
                let mut screens: Vec<Option<&String>> = vec![None];
                // (after a log-in as well: the ALMEX logs in to its menu, and its entry key
                // opens the line entry from there)
                if let Some((g, keys, _)) = &u.session {
                    let g = *g;
                    for k in keys.iter().filter(|k| !u.enters.contains(k)) {
                        if s.clone()
                            .press_when(k, 0.0, &|a, b| a.var(Some(g)) != b.var(Some(g)))
                        {
                            screens.push(Some(k));
                        }
                    }
                }
                for screen in screens {
                    let mut s1 = s.clone();
                    if let Some(k) = screen {
                        s1.press(k);
                        s1.frames(0.5);
                    }
                    if let Some((sim, what)) = entry(u, t, &s1, digits, enter, 0) {
                        done.extend(screen.map(|k| format!("screen {k}")));
                        done.push(format!("{what} with {prefix}0-9 and {enter}"));
                        steps.extend(done);
                        return Some(sim);
                    }
                }
            }
        }
    }
    None
}

/// Type `number` on the keypad, each key once the unit takes it.
fn type_number(u: &Unit, s: &mut Sim, digits: &[String], number: &str, wait: f32) -> bool {
    number.chars().all(|c| {
        let d = c.to_digit(10).unwrap_or(0) as usize;
        s.press_when(&digits[d], wait, &|a, b| {
            differs(a, b, &u.input, &u.input_str)
        })
    })
}

/// How many digits the unit takes now.
fn room(u: &Unit, s: &Sim, digits: &[String]) -> usize {
    let mut probe = s.clone();
    let mut n = 0;
    while n < 12 {
        let mut next = probe.clone();
        next.press(&digits[1]);
        if !differs(&next, &probe, &u.input, &u.input_str) {
            break;
        }
        probe = next;
        n += 1;
    }
    n
}

/// A mode key (or none), a number and the entry key, confirmed as often as the unit asks;
/// a second entry when the first one set part of the duty. Returns the state and what was
/// typed.
fn entry(
    u: &Unit,
    t: &Target,
    s: &Sim,
    digits: &[String],
    enter: &str,
    depth: usize,
) -> Option<(Sim, String)> {
    let before = progress(u, t, s);
    let mut keys: Vec<Vec<&String>> = u.mode_keys.iter().map(|k| vec![k]).collect();
    keys.push(Vec::new());
    if u.dynamic {
        // a list screen first, then the key that opens its code entry
        for a in &u.mode_keys {
            for b in &u.mode_keys {
                keys.push(vec![a, b]);
            }
        }
    }
    for prefix in keys {
        let mut s1 = s.clone();
        let mode = u.mode;
        let mut ok = true;
        for k in &prefix {
            if !s1.press_when(k, if u.dynamic { 0.5 } else { 2.0 }, &|a, b| {
                a.var(mode) != b.var(mode) || (!u.dynamic && differs(a, b, &u.input, &u.input_str))
            }) {
                ok = false;
                break;
            }
        }
        if !ok {
            continue;
        }
        let key: Option<String> = (!prefix.is_empty()).then(|| {
            prefix
                .iter()
                .map(|k| k.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        });
        let key = key.as_ref();
        // a unit learnt by trial: what the number keys type into on this screen - the
        // string that a press of 1 makes end in a 1
        let probed;
        let u: &Unit = if u.dynamic {
            let mut pressed = s1.clone();
            pressed.press(&digits[1]);
            let mut idle = s1.clone();
            idle.frames(HOLD + GAP);
            let typed: Vec<u32> = (0..pressed.state.str_vars.len() as u32)
                .filter(|&i| {
                    let (a, b) = (
                        &pressed.state.str_vars[i as usize],
                        &idle.state.str_vars[i as usize],
                    );
                    a != b && a.trim_end().ends_with('1') && a.trim().len() > b.trim().len()
                })
                .collect();
            if debug() {
                let changed: Vec<String> = (0..pressed.state.str_vars.len())
                    .filter(|&i| pressed.state.str_vars[i] != idle.state.str_vars[i])
                    .map(|i| {
                        format!(
                            "{}={:?} (idle {:?})",
                            pressed
                                .program
                                .str_var_names
                                .get(i)
                                .cloned()
                                .unwrap_or_default(),
                            pressed.state.str_vars[i],
                            idle.state.str_vars[i]
                        )
                    })
                    .collect();
                log::info!(
                    "IBIS probe after {:?} (mode {:?}): strings changed by 1: {changed:?}",
                    prefix,
                    s1.var(u.mode)
                );
            }
            if typed.is_empty() {
                continue;
            }
            let mut v = u.clone();
            v.input_str = typed;
            probed = v;
            &probed
        } else {
            u
        };
        // wait for the unit to take numbers (a "please wait" after the mode key)
        let mut waited = 0.0;
        let mut n = room(u, &s1, digits);
        while n == 0 && waited < 4.0 {
            s1.frames(0.25);
            waited += 0.25;
            n = room(u, &s1, digits);
        }
        if debug() {
            log::info!(
                "IBIS trial prefix {:?}: mode {:?}, room {n}",
                key,
                s1.var(u.mode)
            );
        }
        if n == 0 {
            continue;
        }
        for layout in layouts(t, n) {
            let mut s2 = s1.clone();
            if !type_number(u, &mut s2, digits, &layout, 1.0) {
                if debug() {
                    log::info!("IBIS trial: {layout} did not type");
                }
                continue;
            }
            if !s2.press_when(enter, 1.0, &|a, b| differs(a, b, &u.watch, &u.watch_str)) {
                if debug() {
                    log::info!("IBIS trial: {enter} after {layout} changed nothing");
                }
                continue;
            }
            confirm(u, t, &mut s2, enter);
            // a unit that shows the entry "coming" and takes it with a key of its own (the
            // Scania's ALMEX: "Quitieren" on its RBL panel)
            let mut quit_with = String::new();
            if !met(u, t, &s2) {
                for other in u.enters.iter().filter(|e| e.as_str() != enter) {
                    let mut s3 = s2.clone();
                    if s3.press_when(other, 2.0, &|a, b| differs(a, b, &u.watch, &u.watch_str)) {
                        confirm(u, t, &mut s3, other);
                        if met(u, t, &s3) || progress(u, t, &s3) > progress(u, t, &s2) {
                            s2 = s3;
                            quit_with = format!(" + {other}");
                            break;
                        }
                    }
                }
            }
            let what = format!(
                "{}{layout}{quit_with}",
                key.map(|k| format!("{k} ")).unwrap_or_default()
            );
            if debug() {
                log::info!(
                    "IBIS trial (depth {depth}): {what} + {enter} -> mode {:?} line {:?} route {:?} terminus {:?} at {:.1} s (want {} {:?} {})",
                    s2.var(u.mode),
                    s2.var(u.line),
                    s2.var(u.route),
                    s2.var(u.terminus),
                    s2.t,
                    t.line,
                    t.route_index,
                    t.terminus_index
                );
            }
            if met(u, t, &s2) {
                return Some((s2, what));
            }
            if depth == 0 && progress(u, t, &s2) > before {
                if let Some((s3, more)) = entry(u, t, &s2, digits, enter, 1) {
                    return Some((s3, format!("{what}, {more}")));
                }
            }
        }
    }
    None
}

/// Press the entry key while the unit asks for it: until the duty shows (and then as long
/// as a press still moves something without undoing it), waiting where the unit moves on
/// by itself.
fn confirm(u: &Unit, t: &Target, s: &mut Sim, enter: &str) {
    for _ in 0..MAX_CONFIRM {
        let shown = met(u, t, s);
        let mut pressed = s.clone();
        if !pressed.press_when(enter, if shown { 0.0 } else { KEY_WAIT }, &|a, b| {
            differs(a, b, &u.watch, &u.watch_str)
        }) {
            return;
        }
        if shown && !met(u, t, &pressed) {
            return;
        }
        *s = pressed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_fit_the_room() {
        let t = Target {
            line: 5,
            suffix: 0,
            route: Some(1),
            terminus_code: Some(34),
            route_index: Some(3),
            terminus_index: 7,
            stop: 0,
        };
        assert_eq!(
            layouts(&t, 10),
            vec![
                "005000001",
                "0050000001",
                "500",
                "01",
                "034",
                "00500",
                "5",
                "1",
                "34"
            ]
        );
        assert_eq!(
            layouts(&t, 5),
            vec!["500", "01", "034", "00500", "5", "1", "34"]
        );
        assert_eq!(layouts(&t, 2), vec!["01", "5", "1", "34"]);
        let t = Target {
            line: 137,
            suffix: 0,
            route: None,
            terminus_code: Some(5),
            ..Default::default()
        };
        assert_eq!(layouts(&t, 5), vec!["13700", "005", "137", "5"]);
    }
}
