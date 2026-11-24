//! Check the rig of every human: which o3d bones map to which engine id, how the vertices
//! are weighted, where each bone's vertices lie against the `[links]` joints, and what the
//! procedural poses do to the feet, knees and hands.
//! usage: human_rig <root> [more roots...]   (every Humans/*/*.hum below each root)
use glam::Vec3;
use glam::{DVec2, DVec3};
use omsi_sim::human::{Activity, HumanType, Pose, PoseInput};
use std::path::Path;

fn main() {
    let roots: Vec<String> = std::env::args().skip(1).collect();
    for root in &roots {
        let dir = Path::new(root).join("Humans");
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut files: Vec<_> = rd
            .flatten()
            .filter_map(|e| std::fs::read_dir(e.path()).ok())
            .flat_map(|s| s.flatten().map(|f| f.path()))
            .filter(|p| {
                p.extension()
                    .map(|x| x.eq_ignore_ascii_case("hum"))
                    .unwrap_or(false)
            })
            .collect();
        files.sort();
        for f in files {
            match HumanType::load(&f) {
                Ok(t) => report(&t),
                Err(e) => println!("{}: {e:#}", f.display()),
            }
        }
    }
}

fn report(t: &HumanType) {
    let j = &t.joints;
    println!(
        "== {} (model {}, height {}, walk {:?})",
        t.def.path.display(),
        t.def.model,
        t.def.height,
        t.def.walk_param
    );
    println!(
        "   links hip {:?} knee {:?} waist {:?} shoulder {:?} elbow {:?} neck {:?} hand {:?} finger {:?}",
        j.hip, j.knee, j.waist, j.shoulder, j.elbow, j.neck, j.hand, j.finger
    );
    let r = &t.rig;
    println!(
        "   rig: ankle {:?} sole {:.3} ankle_h {:.3} heel {:.3} ball {:.3} toe {:.3} thigh {:.3} shin {:.3} arm {:.3}+{:.3} head_top {:.2} seat_lift {:.2} cadence(1.35) {:.2}/s walk {} {} {}",
        r.ankle[1],
        r.sole,
        r.ankle_h,
        r.heel,
        r.ball,
        r.toe,
        r.thigh,
        r.shin,
        r.upper_arm,
        r.forearm,
        r.head_top,
        r.seat_lift,
        r.cadence(1.35),
        r.walk_step,
        r.arm_swing,
        r.hip_sway
    );
    // how far the thigh-weighted vertices lie from the thigh (skirts are far out)
    for m in &t.meshes {
        let mut d: Vec<f32> = Vec::new();
        for (i, inf) in m.skin.iter().enumerate() {
            for side in 0..2 {
                let slot = side as u8;
                if (0..inf.n as usize).any(|k| inf.slot[k] == slot && inf.weight[k] > 0.3) {
                    let (a, b) = (r.hip[side], r.knee[side]);
                    let p = m.data.positions[i];
                    let t = ((p - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
                    if t > 0.45 {
                        let radius = (0.13 + (0.06 - 0.13) * t) * r.scale;
                        d.push((a + (b - a) * t - p).length() - radius);
                    }
                }
            }
        }
        d.sort_by(|a, b| a.total_cmp(b));
        let q = |f: f32| {
            d.get(((d.len() as f32 - 1.0) * f) as usize)
                .copied()
                .unwrap_or(0.0)
        };
        println!(
            "   lower thigh vertices {}: beyond the leg median {:.3} p90 {:.3} max {:.3}",
            d.len(),
            q(0.5),
            q(0.9),
            q(1.0)
        );
    }
    poses(t);
    for (li, _lod) in t.model.lods.iter().enumerate() {
        for md in t.model.lod_meshes(li) {
            let p = omsi_cfg::resolve_path(&t.model_dir, &md.file);
            let Ok(m) = omsi_o3d::load_mesh(&p) else {
                println!("   lod {li} {}: cannot load", md.file);
                continue;
            };
            let n = m.vertices.len();
            let mut total = vec![0f32; n];
            let mut unmapped = Vec::new();
            let mut per: Vec<String> = Vec::new();
            for b in &m.bones {
                let id = md
                    .bones
                    .iter()
                    .find(|(nm, _)| nm.eq_ignore_ascii_case(&b.name))
                    .map(|(_, id)| *id);
                if id.is_none() {
                    unmapped.push(b.name.clone());
                }
                let (mut lo, mut hi, mut sum, mut wsum) = (
                    Vec3::splat(f32::MAX),
                    Vec3::splat(f32::MIN),
                    Vec3::ZERO,
                    0.0,
                );
                for w in &b.weights {
                    if let Some(v) = m.vertices.get(w.vertex as usize) {
                        let q = Vec3::new(v.position.x, v.position.z, v.position.y);
                        total[w.vertex as usize] += w.weight;
                        if w.weight > 0.3 {
                            lo = lo.min(q);
                            hi = hi.max(q);
                        }
                        sum += q * w.weight;
                        wsum += w.weight;
                    }
                }
                per.push(format!(
                    "{}={:?} n{} c({:.2},{:.2},{:.2}) x{:.2}..{:.2} z{:.2}..{:.2}",
                    b.name,
                    id,
                    b.weights.len(),
                    sum.x / wsum.max(1e-6),
                    sum.y / wsum.max(1e-6),
                    sum.z / wsum.max(1e-6),
                    lo.x,
                    hi.x,
                    lo.z,
                    hi.z
                ));
            }
            let none = total.iter().filter(|w| **w < 1e-4).count();
            let partial = total
                .iter()
                .filter(|w| **w >= 1e-4 && (**w - 1.0).abs() > 1e-3)
                .count();
            let nan = m
                .vertices
                .iter()
                .filter(|v| !v.position.is_finite())
                .count();
            let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            for v in &m.vertices {
                let q = Vec3::new(v.position.x, v.position.z, v.position.y);
                lo = lo.min(q);
                hi = hi.max(q);
            }
            println!(
                "   lod {li} (min {}) {}: {n} verts, bbox {:?}..{:?}, unweighted {none}, weight!=1 {partial}, nan {nan}, unmapped {:?}, smoothskin {}",
                t.model.lods[li].min_size, md.file, lo, hi, unmapped, md.smooth_skin
            );
            for s in per {
                println!("      {s}");
            }
            if std::env::var_os("RIG_WEIGHTS").is_some() && li == 0 {
                // per-vertex influences: count, duplicates, and weights far from the bone
                let mut infl: Vec<Vec<(String, f32)>> = vec![Vec::new(); n];
                let mut dups = 0;
                for b in &m.bones {
                    let mut seen = std::collections::HashSet::new();
                    for w in &b.weights {
                        if !seen.insert(w.vertex) {
                            dups += 1;
                        }
                        if let Some(x) = infl.get_mut(w.vertex as usize) {
                            x.push((b.name.clone(), w.weight));
                        }
                    }
                }
                let mut hist = std::collections::BTreeMap::new();
                for x in &infl {
                    let t: f32 = x.iter().map(|y| y.1).sum();
                    *hist
                        .entry(((t * 10.0).round() as i32, x.len()))
                        .or_insert(0) += 1;
                }
                println!(
                    "      duplicates {dups}; (total*10, influences) -> count: {:?}",
                    hist
                );
                // normalised weights counting duplicates vs. once per bone
                let (mut worst, mut differ, mut inconsistent) = (0f32, 0, 0);
                for x in &infl {
                    let t: f32 = x.iter().map(|y| y.1).sum();
                    let mut once: Vec<(String, f32, usize)> = Vec::new();
                    for (b, w) in x {
                        match once.iter_mut().find(|o| &o.0 == b) {
                            Some(o) => {
                                if (o.1 - w).abs() > 1e-4 {
                                    inconsistent += 1;
                                }
                                o.2 += 1;
                            }
                            None => once.push((b.clone(), *w, 1)),
                        }
                    }
                    let t1: f32 = once.iter().map(|o| o.1).sum();
                    if t < 1e-4 || t1 < 1e-4 {
                        continue;
                    }
                    let mut d = 0f32;
                    for o in &once {
                        d = d.max((o.1 * o.2 as f32 / t - o.1 / t1).abs());
                    }
                    if d > 0.01 {
                        differ += 1;
                    }
                    worst = worst.max(d);
                }
                println!(
                    "      dedupe: {differ} vertices differ by >1%, worst {worst:.3}, inconsistent duplicate weights {inconsistent}"
                );
                for (i, x) in infl.iter().enumerate().step_by(97).take(12) {
                    let v = m.vertices[i].position;
                    println!("      v{i} ({:.2},{:.2},{:.2}) {:?}", v.x, v.z, v.y, x);
                }
            }
        }
    }
}

/// Ranges of the joints over a few seconds of each activity.
fn poses(t: &HumanType) {
    let r = &t.rig;
    for (name, speed) in [
        ("walk 0.4", 0.4),
        ("walk 1.0", 1.0),
        ("walk 1.35", 1.35),
        ("walk 1.8", 1.8),
    ] {
        let mut p = Pose::new(1);
        let mut pos = DVec3::ZERO;
        let dt = 1.0 / 60.0;
        let (mut knee, mut ankle, mut sole, mut miss, mut wrist_z, mut wrist_x, mut steps) = (
            [f32::MAX, f32::MIN],
            [f32::MAX, f32::MIN],
            [f32::MAX, f32::MIN],
            0f32,
            f32::MAX,
            f32::MAX,
            0,
        );
        let mut was = [true; 2];
        let mut start_catch_ups = 0;
        for k in 0..600 {
            pos.y += speed * dt as f64;
            let input = PoseInput {
                activity: Activity::Walk,
                origin: pos,
                velocity: DVec2::new(0.0, speed),
                ..PoseInput::default()
            };
            p.advance(r, &input, dt);
            let posed = p.bones(r);
            if k < 180 {
                start_catch_ups = p.catch_ups();
                continue;
            }
            for side in 0..2 {
                knee[0] = knee[0].min(posed.knee_flex[side]);
                knee[1] = knee[1].max(posed.knee_flex[side]);
                ankle[0] = ankle[0].min(posed.ankle_flex[side]);
                ankle[1] = ankle[1].max(posed.ankle_flex[side]);
                sole[0] = sole[0].min(posed.sole[side]);
                sole[1] = sole[1].max(posed.sole[side]);
                let planted = p.planted(side);
                if planted {
                    miss = miss.max(posed.leg_miss[side]);
                }
                if was[side] && !planted {
                    steps += 1;
                }
                was[side] = planted;
                wrist_z = wrist_z.min(posed.wrist[side].z);
                wrist_x = wrist_x.min(posed.wrist[side].x.abs());
            }
        }
        println!(
            "   {name}: {:.2} steps/s, knee {:.0}..{:.0}, ankle {:.0}..{:.0}, sole {:.3}..{:.3}, planted miss {:.3}, wrist z >= {:.2}, |x| >= {:.2}, catch-up steps {}",
            steps as f32 / 7.0,
            knee[0],
            knee[1],
            ankle[0],
            ankle[1],
            sole[0],
            sole[1],
            miss,
            wrist_z,
            wrist_x,
            p.catch_ups() - start_catch_ups
        );
    }
    let mut p = Pose::new(2);
    let seat = glam::Vec3::new(0.0, -r.seat_front(), 0.45);
    let input = PoseInput {
        activity: Activity::Sit,
        seat: Some(seat),
        ..PoseInput::default()
    };
    for _ in 0..150 {
        p.advance(r, &input, 1.0 / 60.0);
    }
    let posed = p.bones(r);
    println!(
        "   sit (seat 0.45): hips {:?}, knees {:.0}/{:.0}, soles {:.3}/{:.3}, wrists {:?} {:?}",
        (posed.hip[0] + posed.hip[1]) * 0.5,
        posed.knee_flex[0],
        posed.knee_flex[1],
        posed.sole[0],
        posed.sole[1],
        posed.wrist[0],
        posed.wrist[1]
    );
    let mut p = Pose::new(3);
    let desk = glam::Vec3::new(-0.25, 0.55, 1.1);
    let input = PoseInput {
        activity: Activity::Pay,
        reach: Some(desk),
        look: Some(glam::Vec3::new(-0.8, 0.9, 1.3)),
        ..PoseInput::default()
    };
    for _ in 0..90 {
        p.advance(r, &input, 1.0 / 60.0);
    }
    let posed = p.bones(r);
    println!(
        "   pay: wrist {:?} (desk {:?}), elbow {:?}",
        posed.wrist[1], desk, posed.elbow[1]
    );
}
