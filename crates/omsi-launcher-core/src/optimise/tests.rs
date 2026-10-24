use super::*;
use std::path::PathBuf;

struct Bench {
    gpu_ms: f64,
    cpu_ms: f64,
    restarts: u32,
}

impl Bench {
    fn frame(&self, s: &Value, live: &Value) -> (f32, f32) {
        let mut gpu = self.gpu_ms;
        gpu *= match num(live, "msaa", 4.0) as i64 {
            8 => 1.6,
            4 => 1.3,
            2 => 1.1,
            _ => 1.0,
        };
        if text(live, "mirror_refresh", "full") != "eco" {
            gpu *= 1.08;
        }
        gpu *= 1.0 + num(live, "mirror_size", 256.0) / 4096.0;
        if on(live, "ssao", true) {
            gpu *= 1.12;
        }
        gpu *= num(live, "shadow_size", 2048.0) / 2048.0 * 0.1 + 0.9;
        let render = scale(live);
        gpu *= render * render;
        let mut cpu = self.cpu_ms;
        if on(live, "sim_thread", false) {
            cpu *= 0.7;
        }
        cpu *= 0.8 + 0.2 * distance(live, "max_obj_dist", 900.0) / 900.0;
        let _ = s;
        let frame = gpu.max(cpu);
        ((frame / 1000.0) as f32, ((gpu - cpu).max(0.0) / 1000.0) as f32)
    }
}

fn run(machine: &mut Bench, settings: Value, target: u32) -> (Outcome, Value) {
    run_on(machine, settings, target, Machine { cores: 8, ..Default::default() })
}

fn run_on(machine: &mut Bench, settings: Value, target: u32, hw: Machine) -> (Outcome, Value) {
    let mut tuner = Tuner::new(settings.clone(), target, hw);
    let mut live = settings;
    let mut next_start = live.clone();
    for _ in 0..200_000 {
        let (dt, wait) = machine.frame(tuner.settings(), &live);
        let apply = |live: &mut Value, next: &mut Value, steps: &[Step]| {
            for st in steps {
                next[st.key] = st.to.clone();
                if st.apply != Apply::NextStart {
                    live[st.key] = st.to.clone();
                }
            }
        };
        match tuner.frame(dt, wait) {
            Action::Wait => {}
            Action::Apply(steps) => apply(&mut live, &mut next_start, &steps),
            Action::Finish(steps, outcome) => {
                apply(&mut live, &mut next_start, &steps);
                if outcome.restart {
                    machine.restarts += 1;
                }
                return (outcome, next_start);
            }
        }
    }
    panic!("the tuner never finished");
}

fn pc_1440p() -> Value {
    json!({ "graphics": "enhanced", "msaa": 4, "mirror_size": 1024, "mirror_refresh": "full", "ssao": true, "shadows": true, "shadow_size": 2048, "render_scale": "auto", "max_obj_dist": "auto", "min_obj_size": 0.013, "max_fps": 1000, "sim_thread": true, "clouds": true, "cloud_quality": "high", "rain_quality": "high" })
}

fn changed(o: &Outcome, key: &str) -> Option<Value> {
    o.changes.iter().rev().find(|c| c.key == key).map(|c| c.to.clone())
}

#[test]
fn a_card_that_cannot_keep_up_gets_a_lighter_picture_until_the_target() {
    let mut m = Bench { gpu_ms: 11.0, cpu_ms: 6.0, restarts: 0 };
    let (o, after) = run(&mut m, pc_1440p(), 90);
    assert_eq!(o.limit, Some(Limit::Gpu));
    assert!(o.before_fps < 60.0, "{o:?}");
    assert!(o.reached, "{o:?}");
    assert!(o.after_fps >= 90.0, "{o:?}");
    assert_eq!(changed(&o, "mirror_refresh"), Some(json!("eco")));
    assert!(changed(&o, "msaa").and_then(|v| v.as_i64()).is_some_and(|m| m <= 2), "{o:?}");
    assert_eq!(changed(&o, "mirror_size"), None, "the aim was reached without what only the next start would bring");
    assert!(!o.restart);
    assert_eq!(changed(&o, "cloud_quality"), None, "the clouds cost nothing here: {o:?}");
    assert_eq!(changed(&o, "max_obj_dist"), None, "the processor was not the limit");
    assert_eq!(after["max_fps"], json!(0), "no more frames than the screen shows");
}

#[test]
fn a_card_far_too_slow_gets_the_settings_of_the_next_start_too() {
    let mut m = Bench { gpu_ms: 40.0, cpu_ms: 6.0, restarts: 0 };
    let (o, after) = run(&mut m, pc_1440p(), 90);
    assert!(!o.reached, "{o:?}");
    assert!(o.restart);
    assert_eq!(after["mirror_size"], json!(512));
    assert_eq!(after["sim_thread"], json!(true));
    assert_eq!(after["view_distance"], Value::Null, "the processor was not the limit: {after}");
}

#[test]
fn a_device_without_2x_is_not_tried_at_2x() {
    let apple = Machine { cores: 8, msaa_counts: 1 | 4, ssao: true };
    assert!(!lighter_steps(&pc_1440p(), Limit::Gpu, &apple).iter().any(|s| s.key == "msaa" && s.to == json!(2)));
    let low = json!({ "graphics": "enhanced", "msaa": 1, "ssao": false });
    let up: Vec<Value> = heavier_steps(&low, &low, &apple).into_iter().filter(|s| s.key == "msaa").map(|s| s.to).collect();
    assert_eq!(up, vec![json!(4)]);
    let no_ssao = Machine { ssao: false, ..apple };
    assert!(!heavier_steps(&low, &low, &no_ssao).iter().any(|s| s.key == "ssao"));
}

#[test]
fn a_busy_processor_gets_the_simulation_thread_and_a_restart() {
    let mut m = Bench { gpu_ms: 4.0, cpu_ms: 20.0, restarts: 0 };
    let mut s = pc_1440p();
    s["sim_thread"] = json!(false);
    s["max_fps"] = json!(0);
    let (o, after) = run(&mut m, s, 60);
    assert_eq!(o.limit, Some(Limit::Cpu));
    assert!(o.restart);
    assert_eq!(after["sim_thread"], json!(true));
    assert_eq!(changed(&o, "msaa"), None, "the card was not the limit");
    assert_eq!(m.restarts, 1);
}

#[test]
fn a_step_that_brings_nothing_is_taken_back() {
    let mut m = Bench { gpu_ms: 4.0, cpu_ms: 28.0, restarts: 0 };
    let mut s = pc_1440p();
    s["max_fps"] = json!(0);
    let (o, after) = run(&mut m, s, 60);
    assert_eq!(after["mirror_refresh"], json!("full"), "the mirrors cost the card only, which waited anyway: {o:?}");
    assert_eq!(changed(&o, "mirror_refresh"), None);
    assert_ne!(after["max_obj_dist"], json!("auto"), "the processor was the limit: {o:?}");
}

#[test]
fn a_fast_machine_gets_a_finer_picture_and_keeps_its_margin() {
    let mut m = Bench { gpu_ms: 2.0, cpu_ms: 2.0, restarts: 0 };
    let low = json!({ "graphics": "enhanced", "msaa": 1, "mirror_size": 256, "mirror_refresh": "eco", "ssao": false, "shadows": true, "shadow_size": 1024, "render_scale": "0.75", "max_obj_dist": "500", "min_obj_size": 0.03, "max_fps": 0 });
    let (o, after) = run(&mut m, low.clone(), 60);
    assert!(o.after_fps >= 72.0, "{o:?}");
    assert_eq!(after["render_scale"], json!("auto"));
    assert!(num(&after, "msaa", 1.0) >= 2.0, "{after}");
    assert_eq!(after["max_obj_dist"], json!("500"), "nothing beyond what the player chose: {after}");
}

#[test]
fn a_machine_on_target_is_left_alone() {
    let mut m = Bench { gpu_ms: 8.0, cpu_ms: 6.0, restarts: 0 };
    let mut s = pc_1440p();
    s["max_fps"] = json!(0);
    s["mirror_size"] = json!(256);
    let (o, after) = run(&mut m, s.clone(), 60);
    assert!(o.changes.is_empty(), "{o:?}");
    assert_eq!(after, s);
    assert!(o.reached);
}

#[test]
fn the_steps_fit_the_graphics_mode() {
    let vanilla = json!({ "graphics": "vanilla", "msaa": 4, "ssao": true, "shadows": true, "shadow_size": 4096, "mirror_size": 0 });
    let hw = Machine { cores: 8, ..Default::default() };
    let keys: Vec<&str> = lighter_steps(&vanilla, Limit::Gpu, &hw).iter().map(|s| s.key).collect();
    assert!(!keys.contains(&"ssao") && !keys.contains(&"shadows") && !keys.contains(&"shadow_size"), "{keys:?}");
    assert!(!keys.contains(&"mirror_refresh"), "no mirrors, nothing to save there");
    let traced = json!({ "graphics": "enhanced_plus", "msaa": 2, "ssao": true });
    assert!(!lighter_steps(&traced, Limit::Gpu, &hw).iter().any(|s| s.key == "ssao"));
    let two_cores = json!({ "sim_thread": false });
    assert!(!lighter_steps(&two_cores, Limit::Cpu, &Machine { cores: 2, ..hw }).iter().any(|s| s.key == "sim_thread"));
}

/// "Optimise for my PC" knows Enhanced's light: ray tracing goes first, then every light's shadows,
/// the clouds' shadow and the glow, the street lamps' shadows last; and none of it for the plain modes.
#[test]
fn the_steps_know_enhanceds_light() {
    let hw = Machine { cores: 8, ..Default::default() };
    let full = json!({ "graphics": "enhanced", "ray_tracing": true, "light_shadows": "all", "cloud_shadows": true, "light_glow": true, "msaa": 2, "ssao": true, "shadows": true, "shadow_size": 2048 });
    let keys: Vec<&str> = lighter_steps(&full, Limit::Gpu, &hw).iter().map(|s| s.key).collect();
    let at = |k: &str| keys.iter().position(|x| *x == k).unwrap_or_else(|| panic!("{k} in {keys:?}"));
    assert!(at("ray_tracing") < at("light_shadows") && at("light_shadows") < at("cloud_shadows") && at("cloud_shadows") < at("light_glow"));
    // the street lamps' shadows are a step of their own, after Every light's
    let street = json!({ "graphics": "enhanced", "light_shadows": "street", "msaa": 1, "shadows": true });
    let steps = lighter_steps(&street, Limit::Gpu, &hw);
    assert_eq!(steps.iter().find(|s| s.key == "light_shadows").map(|s| s.to.clone()), Some(json!("off")));
    // ray tracing needs a restart, the rest takes effect at once
    let rt = lighter_steps(&full, Limit::Gpu, &hw).into_iter().find(|s| s.key == "ray_tracing").unwrap();
    assert_eq!((rt.to, rt.apply), (json!(false), Apply::NextStart));
    // Vanilla+ has none of it
    let plain = json!({ "graphics": "vanilla_plus", "light_shadows": "all", "cloud_shadows": true, "light_glow": true, "msaa": 1 });
    let keys: Vec<&str> = lighter_steps(&plain, Limit::Gpu, &hw).iter().map(|s| s.key).collect();
    assert!(!keys.contains(&"light_shadows") && !keys.contains(&"cloud_shadows") && !keys.contains(&"light_glow"), "{keys:?}");
}

#[test]
fn a_reading_of_frames() {
    let frames: Vec<f32> = (0..100).map(|i| if i < 90 { 0.010 } else { 0.020 }).collect();
    let r = Reading::of(&frames, 0.5).unwrap();
    assert!((r.fps - 100.0).abs() < 0.5);
    assert!((r.low_fps - 50.0).abs() < 0.5);
    assert_eq!(r.limit(), Limit::Gpu);
    assert_eq!(Reading::of(&frames, 0.01).unwrap().limit(), Limit::Cpu);
    assert!(Reading::of(&frames[..5], 0.0).is_none());
}

#[test]
fn the_target_of_the_screen() {
    assert_eq!(target_for(0, Some(170)), 100);
    assert_eq!(target_for(0, Some(75)), 75);
    assert_eq!(target_for(0, None), 60);
    assert_eq!(target_for(0, Some(20)), 60);
    assert_eq!(target_for(90, Some(60)), 90);
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("openomsi-optimise-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn outcome_of(settings: Value) -> Outcome {
    Outcome { target: 90, before_fps: 48.0, after_fps: 112.0, limit: Some(Limit::Gpu), changes: vec![Change { key: "msaa".into(), from: json!(4), to: json!(2) }], reached: true, restart: false, settings }
}

#[test]
fn the_settings_from_before_come_back() {
    let dir = scratch("back");
    let mine = pc_1440p();
    back_up_in(&dir, &mine).unwrap();
    let mut tuned = mine.clone();
    tuned["msaa"] = json!(2);
    tuned["volume"] = json!(0.3);
    save_result_in(&dir, &outcome_of(tuned.clone())).unwrap();
    assert!(restore_into_from(&dir, &mut tuned));
    assert_eq!(tuned["msaa"], json!(4));
    assert_eq!(tuned["volume"], json!(0.3), "only the graphics go back");
    forget_in(&dir);
    assert!(backup_in(&dir).is_none());
    assert!(!restore_into_from(&dir, &mut tuned));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn optimising_again_keeps_the_settings_of_before_the_first_time() {
    let dir = scratch("again");
    let mine = pc_1440p();
    back_up_in(&dir, &mine).unwrap();
    let mut tuned = mine.clone();
    tuned["msaa"] = json!(2);
    save_result_in(&dir, &outcome_of(tuned.clone())).unwrap();
    back_up_in(&dir, &tuned).unwrap();
    assert_eq!(backup_in(&dir).unwrap()["msaa"], json!(4), "the tuned settings did not replace the player's");
    let mut changed_by_hand = tuned.clone();
    changed_by_hand["msaa"] = json!(8);
    back_up_in(&dir, &changed_by_hand).unwrap();
    assert_eq!(backup_in(&dir).unwrap()["msaa"], json!(8), "settings the player changed since are theirs");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_next_drive_is_armed_with_its_target() {
    let dir = scratch("armed");
    assert_eq!(chosen_target_in(&dir), 0);
    arm_in(&dir, 120).unwrap();
    assert_eq!(armed_in(&dir), Some(120));
    assert_eq!(chosen_target_in(&dir), 120);
    forget_in(&dir);
    assert_eq!(armed_in(&dir), None);
    save_result_in(&dir, &outcome_of(pc_1440p())).unwrap();
    assert_eq!(chosen_target_in(&dir), 90, "the last aim, once nothing waits");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_run_cut_short_leaves_the_settings_and_the_backup_as_they_were() {
    let dir = scratch("cut");
    let mine = pc_1440p();
    begin_in(&dir, &mine).unwrap();
    let mut half = mine.clone();
    half["msaa"] = json!(2);
    half["render_scale"] = json!("0.85");
    half["volume"] = json!(0.2);
    assert!(recover_from(&dir, &mut half));
    assert_eq!(half["msaa"], json!(4));
    assert_eq!(half["render_scale"], json!("auto"));
    assert_eq!(half["volume"], json!(0.2));
    let _ = std::fs::remove_file(file(&dir, RUNNING));
    begin_in(&dir, &half).unwrap();
    assert_eq!(backup_in(&dir).unwrap()["msaa"], json!(4));
    let _ = std::fs::remove_dir_all(&dir);
}
