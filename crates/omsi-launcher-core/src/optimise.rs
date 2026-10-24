use anyhow::{anyhow, Result};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

use crate::{data_dir, effective_mode, get_settings, save_settings, GRAPHICS_PROFILE_KEYS};

const AUTO_MOST: u32 = 100;
const SETTLE_S: f32 = 2.0;
const REBUILD_SETTLE_S: f32 = 4.0;
const FIRST_SETTLE_S: f32 = 8.0;
const MEASURE_S: f32 = 5.0;
const MIN_FRAMES: usize = 90;
const MAX_STEPS: u32 = 8;
const GPU_SHARE: f32 = 0.2;
const WORTH: f32 = 1.05;
const RAISE_FROM: f32 = 1.5;
const RAISE_KEEPS: f32 = 1.2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Limit {
    Gpu,
    Cpu,
}

impl Limit {
    fn name(self) -> &'static str {
        match self {
            Limit::Gpu => "gpu",
            Limit::Cpu => "cpu",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Apply {
    Now,
    Rebuild,
    NextStart,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub key: &'static str,
    pub to: Value,
    pub apply: Apply,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Machine {
    pub cores: usize,
    pub msaa_counts: u32,
    pub ssao: bool,
}

impl Default for Machine {
    fn default() -> Self {
        Machine { cores: 4, msaa_counts: 1 | 2 | 4 | 8, ssao: true }
    }
}

impl Machine {
    fn msaa(&self, n: i64) -> bool {
        n == 1 || (n > 0 && self.msaa_counts & n as u32 != 0)
    }
}

fn step(key: &'static str, to: Value, apply: Apply) -> Step {
    Step { key, to, apply }
}

fn num(s: &Value, key: &str, default: f64) -> f64 {
    match s.get(key) {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(default),
        Some(Value::String(t)) => t.trim().parse().unwrap_or(default),
        Some(Value::Bool(b)) => f64::from(u8::from(*b)),
        _ => default,
    }
}

fn on(s: &Value, key: &str, default: bool) -> bool {
    match s.get(key) {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0) != 0.0,
        Some(Value::String(t)) => matches!(t.trim(), "1" | "true" | "on" | "yes"),
        _ => default,
    }
}

fn text<'a>(s: &'a Value, key: &str, default: &'a str) -> &'a str {
    s.get(key).and_then(Value::as_str).unwrap_or(default)
}

fn distance(s: &Value, key: &str, auto: f64) -> f64 {
    match s.get(key) {
        Some(Value::String(t)) if t.trim().eq_ignore_ascii_case("auto") => auto,
        _ => num(s, key, auto),
    }
}

fn scale(s: &Value) -> f64 {
    match num(s, "render_scale", 0.0) {
        x if x > 0.0 => x,
        _ => 1.0,
    }
}

pub fn lighter_steps(s: &Value, limit: Limit, m: &Machine) -> Vec<Step> {
    let mode = effective_mode(s);
    let classic = mode == "vanilla";
    let traced = mode == "enhanced_plus";
    let enhanced = traced || mode == "enhanced";
    let msaa = num(s, "msaa", 4.0) as i64;
    let shadow = num(s, "shadow_size", 2048.0) as i64;
    let mirror = num(s, "mirror_size", 256.0) as i64;
    let refresh = text(s, "mirror_refresh", "full");
    let render = scale(s);
    let lit = !classic && !traced;
    let mut out = Vec::new();
    let mut add = |ok: bool, st: Step| {
        if ok {
            out.push(st);
        }
    };
    let mirrors_redrawn = mirror > 0 && matches!(refresh, "full" | "every");
    match limit {
        Limit::Gpu => {
            add(mirrors_redrawn, step("mirror_refresh", json!("eco"), Apply::Now));
            add(msaa > 2 && m.msaa(2), step("msaa", json!(2), Apply::Rebuild));
            add(mirror > 512, step("mirror_size", json!(512), Apply::NextStart));
            // (what Enhanced's light adds, dearest first: the ray tracing, every light's shadows, the
            // clouds' shadow, the glow - the street lamps' shadows go last, with the sun's)
            add(traced, step("ray_tracing", json!(false), Apply::NextStart));
            add(enhanced && text(s, "light_shadows", "street") == "all", step("light_shadows", json!("street"), Apply::Now));
            add(enhanced && on(s, "cloud_shadows", false), step("cloud_shadows", json!(false), Apply::Now));
            add(enhanced && on(s, "light_glow", false), step("light_glow", json!(false), Apply::Now));
            // (the street light first: it is the cheaper cut, and turning the AO off takes it along)
            add(mode == "enhanced" && on(s, "ssao", true) && text(s, "street_light", "off") != "off", step("street_light", json!("off"), Apply::Now));
            add(lit && on(s, "ssao", true), step("ssao", json!(false), Apply::Now));
            add(enhanced && on(s, "clouds", true) && text(s, "cloud_quality", "high") == "high", step("cloud_quality", json!("low"), Apply::Now));
            add(!classic && shadow > 2048, step("shadow_size", json!(2048), Apply::Rebuild));
            add(text(s, "rain_quality", "high") == "high", step("rain_quality", json!("medium"), Apply::Now));
            add(msaa > 1, step("msaa", json!(1), Apply::Rebuild));
            add(render > 0.86, step("render_scale", json!("0.85"), Apply::Now));
            add(!classic && shadow > 1024, step("shadow_size", json!(1024), Apply::Rebuild));
            add(mirror > 256, step("mirror_size", json!(256), Apply::NextStart));
            add(render > 0.76, step("render_scale", json!("0.75"), Apply::Now));
            add(enhanced && text(s, "light_shadows", "street") == "street", step("light_shadows", json!("off"), Apply::Now));
            add(lit && on(s, "shadows", true), step("shadows", json!(false), Apply::Now));
            add(render > 0.68, step("render_scale", json!("0.67"), Apply::Now));
        }
        Limit::Cpu => {
            add(mirrors_redrawn, step("mirror_refresh", json!("eco"), Apply::Now));
            add(!on(s, "sim_thread", false) && m.cores >= 4, step("sim_thread", json!(true), Apply::NextStart));
            add(distance(s, "max_obj_dist", 900.0) > 650.0, step("max_obj_dist", json!("650"), Apply::Now));
            add(num(s, "min_obj_size", 0.013) < 0.02, step("min_obj_size", json!(0.02), Apply::Now));
            add(distance(s, "view_distance", 1200.0) > 900.0, step("view_distance", json!("900"), Apply::NextStart));
            add(distance(s, "max_obj_dist", 900.0) > 500.0, step("max_obj_dist", json!("500"), Apply::Now));
            add(num(s, "min_obj_size", 0.013) < 0.03, step("min_obj_size", json!(0.03), Apply::Now));
        }
    }
    out
}

pub fn heavier_steps(s: &Value, start: &Value, m: &Machine) -> Vec<Step> {
    let mode = effective_mode(s);
    let classic = mode == "vanilla";
    let traced = mode == "enhanced_plus";
    let lit = !classic && !traced;
    let mut out = Vec::new();
    let mut add = |ok: bool, st: Step| {
        if ok {
            out.push(st);
        }
    };
    let render = scale(s);
    add(render < 0.99, step("render_scale", json!("auto"), Apply::Now));
    add(lit && !on(s, "shadows", true), step("shadows", json!(true), Apply::Now));
    let msaa = num(s, "msaa", 4.0) as i64;
    let most = if traced { 2 } else { 4 };
    if let Some(n) = [2, 4].into_iter().find(|&n| n > msaa && n <= most && m.msaa(n)) {
        add(true, step("msaa", json!(n), Apply::Rebuild));
    }
    add(lit && m.ssao && !on(s, "ssao", true), step("ssao", json!(true), Apply::Now));
    let shadow = num(s, "shadow_size", 2048.0) as i64;
    add(!classic && shadow < 2048, step("shadow_size", json!(2048), Apply::Rebuild));
    add(distance(s, "max_obj_dist", 900.0) < distance(start, "max_obj_dist", 900.0), step("max_obj_dist", start.get("max_obj_dist").cloned().unwrap_or(json!("auto")), Apply::Now));
    add(num(s, "min_obj_size", 0.013) > num(start, "min_obj_size", 0.013), step("min_obj_size", start.get("min_obj_size").cloned().unwrap_or(json!(0.013)), Apply::Now));
    add(text(s, "rain_quality", "high") != "high", step("rain_quality", json!("high"), Apply::Now));
    out
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reading {
    pub fps: f32,
    pub low_fps: f32,
    pub gpu_share: f32,
}

impl Reading {
    pub fn of(frames: &[f32], gpu_wait: f64) -> Option<Reading> {
        if frames.len() < 10 {
            return None;
        }
        let mut sorted = frames.to_vec();
        sorted.sort_by(f32::total_cmp);
        let at = |p: f32| sorted[((p * sorted.len() as f32) as usize).min(sorted.len() - 1)];
        let total: f64 = frames.iter().map(|&f| f as f64).sum();
        Some(Reading {
            fps: 1.0 / at(0.5).max(1e-4),
            low_fps: 1.0 / at(0.9).max(1e-4),
            gpu_share: (gpu_wait / total.max(1e-6)) as f32,
        })
    }

    pub fn limit(&self) -> Limit {
        if self.gpu_share >= GPU_SHARE {
            Limit::Gpu
        } else {
            Limit::Cpu
        }
    }

    fn meets(&self, target: f32) -> bool {
        self.fps >= target && self.low_fps >= 0.8 * target
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub key: String,
    pub from: Value,
    pub to: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Wait,
    Apply(Vec<Step>),
    Finish(Vec<Step>, Outcome),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub target: u32,
    pub before_fps: f32,
    pub after_fps: f32,
    pub limit: Option<Limit>,
    pub changes: Vec<Change>,
    pub reached: bool,
    pub restart: bool,
    pub settings: Value,
}

impl Outcome {
    pub fn to_json(&self) -> Value {
        json!({
            "target": self.target,
            "before_fps": self.before_fps.round(),
            "after_fps": self.after_fps.round(),
            "limit": self.limit.map(Limit::name),
            "changes": self.changes.iter().map(|c| json!({ "key": c.key, "from": c.from, "to": c.to })).collect::<Vec<_>>(),
            "reached": self.reached,
            "restart": self.restart,
            "when": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    Baseline,
    Lower,
    Raise,
}

struct Trial {
    step: Step,
    was: Value,
    before: Reading,
}

pub struct Tuner {
    target: u32,
    machine: Machine,
    later: Vec<Step>,
    start: Value,
    settings: Value,
    mode: Mode,
    settle: f32,
    measured: f32,
    frames: Vec<f32>,
    gpu_wait: f64,
    first: Option<Reading>,
    last: Option<Reading>,
    trial: Option<Trial>,
    tried: Vec<(String, String)>,
    changes: Vec<Change>,
    steps: u32,
    restart: bool,
}

impl Tuner {
    pub fn new(settings: Value, target: u32, machine: Machine) -> Tuner {
        Tuner {
            target: target.max(30),
            machine,
            later: Vec::new(),
            start: settings.clone(),
            settings,
            mode: Mode::Baseline,
            settle: FIRST_SETTLE_S,
            measured: 0.0,
            frames: Vec::new(),
            gpu_wait: 0.0,
            first: None,
            last: None,
            trial: None,
            tried: Vec::new(),
            changes: Vec::new(),
            steps: 0,
            restart: false,
        }
    }

    pub fn target(&self) -> u32 {
        self.target
    }

    pub fn settings(&self) -> &Value {
        &self.settings
    }

    pub fn steps(&self) -> u32 {
        self.steps
    }

    pub fn last_fps(&self) -> Option<f32> {
        self.last.map(|r| r.fps)
    }

    pub fn last_reading(&self) -> Option<Reading> {
        self.last
    }

    pub fn measuring(&self) -> bool {
        self.settle <= 0.0
    }

    pub fn frame(&mut self, dt: f32, gpu_wait: f32) -> Action {
        if !(dt > 0.0) || dt > 0.5 {
            return Action::Wait;
        }
        if self.settle > 0.0 {
            self.settle -= dt;
            return Action::Wait;
        }
        self.frames.push(dt);
        self.gpu_wait += gpu_wait.max(0.0) as f64;
        self.measured += dt;
        if self.measured < MEASURE_S || self.frames.len() < MIN_FRAMES {
            return Action::Wait;
        }
        let reading = Reading::of(&self.frames, self.gpu_wait);
        self.frames.clear();
        self.gpu_wait = 0.0;
        self.measured = 0.0;
        match reading {
            Some(r) => self.decide(r),
            None => Action::Wait,
        }
    }

    fn decide(&mut self, r: Reading) -> Action {
        let target = self.target as f32;
        if self.first.is_none() {
            self.first = Some(r);
        }
        if let Some(trial) = self.trial.take() {
            let kept = match self.mode {
                Mode::Raise => r.fps >= RAISE_KEEPS * target && r.meets(target),
                _ => r.fps >= trial.before.fps * WORTH,
            };
            if !kept {
                self.settings[trial.step.key] = trial.was.clone();
                self.last = Some(trial.before);
                let back = step(trial.step.key, trial.was, trial.step.apply);
                if self.mode == Mode::Raise {
                    return self.finish(vec![back]);
                }
                self.settle = Self::settle_for(back.apply);
                return Action::Apply(vec![back]);
            }
            self.changes.push(Change { key: trial.step.key.to_string(), from: trial.was, to: trial.step.to });
        }
        self.last = Some(r);
        if self.mode == Mode::Baseline {
            self.mode = if r.meets(target) && r.fps >= RAISE_FROM * target { Mode::Raise } else { Mode::Lower };
        }
        if self.mode == Mode::Lower && r.meets(target) || self.steps >= MAX_STEPS {
            return self.finish(Vec::new());
        }
        self.next(r)
    }

    fn next(&mut self, r: Reading) -> Action {
        let limit = r.limit();
        let other = if limit == Limit::Gpu { Limit::Cpu } else { Limit::Gpu };
        let candidates = match self.mode {
            Mode::Raise => heavier_steps(&self.settings, &self.start, &self.machine),
            _ => {
                let mut c = lighter_steps(&self.settings, limit, &self.machine);
                c.extend(lighter_steps(&self.settings, other, &self.machine).into_iter().filter(|st| st.apply != Apply::NextStart));
                c
            }
        };
        for st in candidates {
            let id = (st.key.to_string(), st.to.to_string());
            if self.tried.contains(&id) {
                continue;
            }
            self.tried.push(id);
            if st.apply == Apply::NextStart {
                if !self.later.iter().any(|l| l.key == st.key) {
                    self.later.push(st);
                }
                continue;
            }
            let was = self.settings.get(st.key).cloned().unwrap_or(Value::Null);
            self.settings[st.key] = st.to.clone();
            self.steps += 1;
            self.settle = Self::settle_for(st.apply);
            self.trial = Some(Trial { step: st.clone(), was, before: r });
            return Action::Apply(vec![st]);
        }
        self.finish(Vec::new())
    }

    fn finish(&mut self, mut apply: Vec<Step>) -> Action {
        let reached = self.last.or(self.first).is_some_and(|r| r.meets(self.target as f32));
        if !reached && self.mode != Mode::Raise {
            for st in std::mem::take(&mut self.later) {
                let was = self.settings.get(st.key).cloned().unwrap_or(Value::Null);
                if was == st.to {
                    continue;
                }
                self.settings[st.key] = st.to.clone();
                self.changes.push(Change { key: st.key.to_string(), from: was, to: st.to.clone() });
                self.restart = true;
                apply.push(st);
            }
        }
        if num(&self.settings, "max_fps", 0.0) >= 1000.0 {
            let was = self.settings["max_fps"].clone();
            self.settings["max_fps"] = json!(0);
            self.changes.push(Change { key: "max_fps".into(), from: was, to: json!(0) });
            apply.push(step("max_fps", json!(0), Apply::Now));
        }
        Action::Finish(apply, self.outcome())
    }

    fn settle_for(apply: Apply) -> f32 {
        match apply {
            Apply::Rebuild => REBUILD_SETTLE_S,
            _ => SETTLE_S,
        }
    }

    pub fn outcome(&self) -> Outcome {
        let target = self.target as f32;
        let last = self.last.or(self.first);
        Outcome {
            target: self.target,
            before_fps: self.first.map_or(0.0, |r| r.fps),
            after_fps: last.map_or(0.0, |r| r.fps),
            limit: self.first.map(|r| r.limit()),
            changes: self.changes.clone(),
            reached: last.is_some_and(|r| r.meets(target)),
            restart: self.restart,
            settings: self.settings.clone(),
        }
    }
}

fn file(dir: &Path, name: &str) -> PathBuf {
    dir.join(name)
}

const BACKUP: &str = "settings-before-optimise.json";
const RESULT: &str = "optimise.json";
const ARMED: &str = "optimise-next-drive.json";
const RUNNING: &str = "optimise-running.json";

fn read(dir: &Path, name: &str) -> Option<Value> {
    std::fs::read_to_string(file(dir, name)).ok().and_then(|t| serde_json::from_str(&t).ok())
}

fn kept_keys() -> impl Iterator<Item = &'static str> {
    GRAPHICS_PROFILE_KEYS.into_iter().chain(["sim_thread"])
}

fn graphics_of(settings: &Value) -> Map<String, Value> {
    kept_keys().filter_map(|k| settings.get(k).map(|v| (k.to_string(), v.clone()))).collect()
}

fn back_up_in(dir: &Path, settings: &Value) -> Result<()> {
    let ours = read(dir, RESULT).and_then(|r| r.get("settings").cloned());
    if backup_in(dir).is_some() && ours.is_some_and(|o| o == Value::Object(graphics_of(settings))) {
        return Ok(());
    }
    std::fs::write(file(dir, BACKUP), serde_json::to_string_pretty(&Value::Object(graphics_of(settings)))?)?;
    Ok(())
}

fn begin_in(dir: &Path, settings: &Value) -> Result<()> {
    back_up_in(dir, settings)?;
    std::fs::write(file(dir, RUNNING), serde_json::to_string_pretty(&Value::Object(graphics_of(settings)))?)?;
    Ok(())
}

fn recover_from(dir: &Path, settings: &mut Value) -> bool {
    let Some(before) = read(dir, RUNNING).filter(Value::is_object) else { return false };
    if let Some(m) = before.as_object() {
        for (k, v) in m {
            settings[k] = v.clone();
        }
    }
    true
}

fn backup_in(dir: &Path) -> Option<Value> {
    read(dir, BACKUP).filter(Value::is_object)
}

fn restore_into_from(dir: &Path, settings: &mut Value) -> bool {
    let Some(saved) = backup_in(dir) else { return false };
    if let Some(m) = saved.as_object() {
        for (k, v) in m {
            settings[k] = v.clone();
        }
    }
    true
}

fn forget_in(dir: &Path) {
    for name in [BACKUP, RESULT, ARMED, RUNNING] {
        let _ = std::fs::remove_file(file(dir, name));
    }
}

fn save_result_in(dir: &Path, outcome: &Outcome) -> Result<()> {
    let mut v = outcome.to_json();
    v["settings"] = Value::Object(graphics_of(&outcome.settings));
    std::fs::write(file(dir, RESULT), serde_json::to_string_pretty(&v)?)?;
    Ok(())
}

fn arm_in(dir: &Path, target: u32) -> Result<()> {
    std::fs::write(file(dir, ARMED), serde_json::to_string(&json!({ "target": target }))?)?;
    Ok(())
}

fn armed_in(dir: &Path) -> Option<u32> {
    read(dir, ARMED)?.get("target").and_then(Value::as_u64).map(|t| t as u32)
}

fn chosen_target_in(dir: &Path) -> u32 {
    armed_in(dir).or_else(|| read(dir, RESULT)?.get("target").and_then(Value::as_u64).map(|t| t as u32)).unwrap_or(0)
}

pub fn begin(settings: &Value) -> Result<()> {
    begin_in(&data_dir(), settings)
}

pub fn end() {
    let _ = std::fs::remove_file(file(&data_dir(), RUNNING));
}

pub fn running() -> bool {
    file(&data_dir(), RUNNING).exists()
}

pub fn recover() -> Result<bool> {
    let dir = data_dir();
    let mut settings = get_settings()?;
    if !recover_from(&dir, &mut settings) {
        return Ok(false);
    }
    save_settings(&settings)?;
    let _ = std::fs::remove_file(file(&dir, RUNNING));
    Ok(true)
}

pub fn backup() -> Option<Value> {
    backup_in(&data_dir())
}

pub fn restore_into(settings: &mut Value) -> bool {
    restore_into_from(&data_dir(), settings)
}

pub fn forget() {
    forget_in(&data_dir());
}

pub fn restore() -> Result<Value> {
    if running() {
        return Err(anyhow!(crate::i18n::tr("Close the game first: it is optimising the settings.")));
    }
    let mut settings = get_settings()?;
    if !restore_into(&mut settings) {
        return Err(anyhow!(crate::i18n::tr("No settings were saved before an optimisation.")));
    }
    save_settings(&settings)?;
    forget();
    Ok(settings)
}

pub fn save_result(outcome: &Outcome) -> Result<()> {
    save_result_in(&data_dir(), outcome)
}

pub fn last_result() -> Option<Value> {
    read(&data_dir(), RESULT)
}

pub fn arm(target: u32) -> Result<()> {
    arm_in(&data_dir(), target)
}

pub fn disarm() {
    let _ = std::fs::remove_file(file(&data_dir(), ARMED));
}

pub fn armed() -> Option<u32> {
    armed_in(&data_dir())
}

pub fn chosen_target() -> u32 {
    chosen_target_in(&data_dir())
}

pub fn status() -> Value {
    json!({ "armed": armed(), "backup": backup().is_some(), "last": last_result() })
}

pub fn target_for(asked: u32, refresh_hz: Option<u32>) -> u32 {
    match asked {
        0 => refresh_hz.filter(|&r| r >= 30).map_or(60, |r| r.clamp(60, AUTO_MOST)),
        t => t.clamp(30, 240),
    }
}

#[cfg(test)]
mod tests;
