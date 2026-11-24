//! `paint_check <map folder> [x y radius]`: asks the game's own spline meshes whether the paint
//! the navigator draws (`road_paint::lane_paint`) lies on paint of the texture: for each piece
//! of a lane line a ray is cast down onto the spline's mesh and the texture at the hit is read.
//! Prints how many pieces sit on paint, and where the paint really is when they do not.

use glam::{DVec2, DVec3, Mat4, Vec2, Vec3};
use omsi_scene::road_paint::PaintCache;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: paint_check <map folder> [x y radius]");
        std::process::exit(2);
    }
    let map_dir = std::path::PathBuf::from(&args[1]);
    let root = map_dir
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or(&map_dir)
        .to_path_buf();
    let near: Option<(f64, f64, f64)> = match args.get(2..5) {
        Some([x, y, r]) => Some((x.parse().unwrap(), y.parse().unwrap(), r.parse().unwrap())),
        _ => None,
    };
    let global =
        omsi_map::global::GlobalCfg::load(&map_dir.join("global.cfg")).expect("global.cfg");
    omsi_map::configure_grid(&global);
    let ts = omsi_map::tile_size();
    let cache = PaintCache::default();
    let (mut total, mut on, mut splines) = (0u32, 0u32, 0u32);
    let mut worst: Vec<(String, f32, f32)> = Vec::new();
    for t in &global.tiles {
        let path = omsi_cfg::resolve_path(&map_dir, &t.file);
        if !path.is_file() {
            continue;
        }
        let o2 = DVec2::new(t.x as f64 * ts, t.y as f64 * ts);
        if let Some((x, y, r)) = near {
            if (o2 + DVec2::splat(ts * 0.5) - DVec2::new(x, y)).length() > r + ts {
                continue;
            }
        }
        let Some(tile) = omsi_world::map_index::read_tile(&path, &[]) else {
            continue;
        };
        for sp in tile
            .splines
            .iter()
            .filter(|s| !s.deleted && !s.file.trim().is_empty() && s.length >= 3.0)
        {
            if let Some((x, y, r)) = near {
                let p = o2 + DVec2::new(sp.pos[0], sp.pos[1]);
                if (p - DVec2::new(x, y)).length() > r {
                    continue;
                }
            }
            let Ok(def) = omsi_scenery::sli::Spline::load(&omsi_cfg::resolve_path(&root, &sp.file))
            else {
                continue;
            };
            let raw = omsi_geometry::SplineCurve::from_map(sp, o2);
            let mesh = omsi_geometry::build_spline_mesh(&def, &raw, sp.mirror, DVec3::ZERO);
            let curve = raw.clone().with_sli(&def);
            let side = if sp.mirror { -1.0 } else { 1.0 };
            let dirs = omsi_scene::texture_dirs(&root, def.path.parent().unwrap_or(&root));
            let dirs: Vec<&std::path::Path> = dirs.iter().map(|d| d.as_path()).collect();
            let mut used = false;
            for profile in &def.profiles {
                let Some(tex) = def.textures.get(profile.texture) else {
                    continue;
                };
                if tex.patchwork.is_some()
                    || tex.scale_by_length
                    || !tex.file.to_lowercase().contains("asph")
                {
                    continue;
                }
                let Some(paint) = cache.get(&tex.file, &dirs) else {
                    continue;
                };
                for pair in profile.points.windows(2) {
                    let at_s = |s: f64, x: f64| -> Vec2 {
                        curve.offset_point(s, side * x, 0.0).truncate().as_vec2()
                    };
                    for (_, tris) in omsi_scene::road_paint::lane_paint(
                        &paint,
                        [&pair[0], &pair[1]],
                        curve.length,
                        curve.tex_offset,
                        sp.mirror,
                        &at_s,
                    ) {
                        for q in tris.chunks_exact(6) {
                            let c = (q[0] + q[1] + q[2] + q[3] + q[4] + q[5]) / 6.0;
                            let origin = Vec3::new(c.x, c.y, 5000.0);
                            // (the highest face of this profile's slot under the point)
                            let hit = omsi_geometry::ray_mesh_hit(
                                origin,
                                Vec3::NEG_Z,
                                &mesh,
                                &Mat4::IDENTITY,
                            );
                            let Some(h) = hit else { continue };
                            if mesh.slot_of(h.index) != profile.texture as u32 {
                                continue;
                            }
                            used = true;
                            total += 1;
                            if paint.painted_at(h.uv.x, h.uv.y) {
                                on += 1;
                            } else if worst.len() < 12 {
                                worst.push((sp.file.clone(), h.uv.x, h.uv.y));
                            }
                        }
                    }
                }
            }
            splines += used as u32;
        }
    }
    println!(
        "{splines} splines, {total} pieces of paint tested, {on} on paint ({:.1} %)",
        100.0 * on as f32 / total.max(1) as f32
    );
    for (f, u, v) in worst {
        println!("  off paint: {f} at uv ({u:.3}, {v:.3})");
    }
}
