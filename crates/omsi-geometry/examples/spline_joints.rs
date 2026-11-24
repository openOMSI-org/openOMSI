//! Do the splines of a map meet where they say they meet? For every spline that names the
//! next or the previous one, the end of the one against the start (or end) of the other,
//! in the geometry the game builds (`SplineCurve`): the gap and the height step, and the
//! kink in the heading. A wrong curve formula shows as gaps and steps all over a map.
//!
//! usage: spline_joints <map folder> [--list]
use glam::DVec2;
use omsi_geometry::SplineCurve;
use std::collections::HashMap;
use std::path::PathBuf;

fn main() {
    let dir = PathBuf::from(std::env::args().nth(1).expect("map folder"));
    let list = std::env::args().any(|a| a == "--list");
    let g = omsi_map::global::GlobalCfg::load(&dir.join("global.cfg")).expect("global.cfg");
    if g.world_coordinates && std::env::var_os("OLD_GRID").is_none() {
        omsi_map::set_tile_size(omsi_map::world_tile_size(g.tiles.iter().map(|t| t.y)));
        omsi_map::set_world_coordinates(true);
    } else if g.world_coordinates {
        omsi_map::set_tile_size(371.9);
    }
    let ts = omsi_map::tile_size();
    // id -> (file, start, start heading, end, end heading, is_h)
    let mut ends: HashMap<
        i64,
        (
            String,
            glam::DVec3,
            f64,
            glam::DVec3,
            f64,
            bool,
            i64,
            i64,
            (i32, i32),
        ),
    > = HashMap::new();
    for t in &g.tiles {
        let p = dir.join(&t.file);
        let Ok(mut tile) = omsi_map::tile::Tile::load(&p) else {
            continue;
        };
        tile.fit_to_world_grid(t.y);
        let origin = DVec2::new(t.x as f64 * ts, t.y as f64 * ts);
        for s in &tile.splines {
            let c = SplineCurve::from_map(s, origin);
            let e = c.point_at(c.length);
            ends.insert(
                s.id,
                (
                    s.file.clone(),
                    c.start,
                    c.heading_at(0.0),
                    e,
                    c.heading_at(c.length),
                    s.is_h,
                    s.prev_id,
                    s.next_id,
                    (t.x, t.y),
                ),
            );
        }
    }
    let (mut n, mut gap5, mut gap50, mut step2, mut step10, mut kink) = (0, 0, 0, 0, 0, 0);
    let mut worst: Vec<(f64, f64, String)> = Vec::new();
    let mut cross_stats: Vec<(i32, i32, f64, f64)> = Vec::new();
    let mut same_gaps = 0;
    for (id, a) in &ends {
        if a.7 <= 0 {
            continue;
        }
        let Some(b) = ends.get(&a.7) else { continue };
        // b follows a: a's end meets b's start, or b's end when b runs the other way
        let (d_start, d_end) = ((a.3 - b.1).length(), (a.3 - b.3).length());
        let (q, hb) = if d_start <= d_end {
            (b.1, b.2)
        } else {
            (b.3, b.4 + 180.0)
        };
        let d = (a.3 - q).truncate().length();
        if d > 2.0 {
            continue; // not a real joint (ids reused, chrono)
        }
        n += 1;
        let dz = (a.3.z - q.z).abs();
        let dh = ((a.4 - hb + 540.0).rem_euclid(360.0) - 180.0).abs();
        if d > 0.05 {
            gap5 += 1;
        }
        let cross = a.8 != b.8;
        let (dxt, dyt) = (b.8.0 - a.8.0, b.8.1 - a.8.1);
        if cross {
            let v = q - a.3;
            cross_stats.push((dxt, dyt, v.x, v.y));
            if std::env::var_os("JOINT_DUMP").is_some() {
                println!(
                    "J {} {} {} {} {:.4} {:.4} {:.1} {:.1}",
                    a.8.0,
                    a.8.1,
                    dxt,
                    dyt,
                    v.x,
                    v.y,
                    a.3.x - a.8.0 as f64 * ts,
                    a.3.y - a.8.1 as f64 * ts
                );
            }
        } else if d > 0.05 {
            same_gaps += 1;
        }
        if d > 0.5 {
            gap50 += 1;
        }
        if dz > 0.02 {
            step2 += 1;
        }
        if dz > 0.10 {
            step10 += 1;
        }
        if dh > 2.0 {
            kink += 1;
        }
        worst.push((dz.max(d), dh, format!("{id} {} ({}) -> {} ({}) tile {:?}: gap {d:.3} m, step {dz:.3} m, kink {dh:.1} deg{}", a.0, if a.5 { "h" } else { "-" }, a.7, b.0, a.8, if b.5 { " (next is spline_h)" } else { "" })));
    }
    // across tile borders: the offset per tile step (a wrong tile size shows as a gap that
    // grows with the step and has the step's direction)
    let mean = |f: &dyn Fn(&(i32, i32, f64, f64)) -> Option<f64>| {
        let v: Vec<f64> = cross_stats.iter().filter_map(f).collect();
        (v.iter().sum::<f64>() / v.len().max(1) as f64, v.len())
    };
    println!(
        "same-tile joints with gaps > 5 cm: {same_gaps}; cross-tile joints {}",
        cross_stats.len()
    );
    println!(
        "  x steps: mean dx per tile step {:?}; y steps: mean dy per tile step {:?}",
        mean(&|c| (c.0 != 0 && c.1 == 0).then(|| c.2 / c.0 as f64)),
        mean(&|c| (c.1 != 0 && c.0 == 0).then(|| c.3 / c.1 as f64))
    );
    worst.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    println!(
        "{} splines, {n} joints: gaps > 5 cm {gap5}, > 50 cm {gap50}; steps > 2 cm {step2}, > 10 cm {step10}; kinks > 2 deg {kink}",
        ends.len()
    );
    for w in worst.iter().take(if list { 60 } else { 15 }) {
        println!("  {}", w.2);
    }
}
