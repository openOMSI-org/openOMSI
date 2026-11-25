//! `[light_emitter_interior]` (openOMSI): a mesh whose light map (`[matl_lightmap]`) or
//! night map (`[matl_nightmap]`) shows lit parts - a ceiling's light strips, a destination
//! sign's back, an illuminated advert, the glow of a ticket machine - is turned into lamps
//! that light the saloon, as `[interiorlight]`s do, from what its texture holds.
//!
//! Every triangle of the mapped material slots is sampled at its middle: the map's colour
//! there, by the triangle's area, is the light it gives. The lit triangles are gathered into
//! at most `max_lamps` groups (k-means over their places, weighted by their light), and each
//! group becomes one `[interiorlight]` at its light's centre - a hand's width off the
//! surface along its normal - in its light's colour, with a reach that grows with how much
//! of it glows and how brightly. The lamps go to every mesh of the model that is lit by the
//! saloon's lamps (those with an `[illumination_interior]`), and switch with the light map's
//! variable (or the tag's own).

use glam::{Vec2, Vec3};
use omsi_model::{InteriorLight, Model};
use std::path::Path;

use super::VehicleMesh;

/// The most lamps a mesh may name (renderer `MAX_LAMPS_PER_MESH`).
const MAX_LAMPS_PER_MESH: usize = 63;

struct Lit {
    pos: Vec3,
    normal: Vec3,
    color: Vec3,
    weight: f32,
}

/// Adds the lamps of every `[light_emitter_interior]` mesh to `model` (see the module).
pub(crate) fn add(model: &mut Model, meshes: &[VehicleMesh], dirs: &[&Path]) {
    let mut made: Vec<(InteriorLight, String)> = Vec::new();
    for vm in meshes {
        let Some(md) = model.meshes.get(vm.def_index) else {
            continue;
        };
        let Some(tag) = md.light_emitter_interior.clone() else {
            continue;
        };
        if tag.strength <= 0.0 {
            continue;
        }
        let data = &vm.data;
        if data.indices.is_empty() || data.uvs.len() != data.positions.len() {
            continue;
        }
        let mut lit: Vec<Lit> = Vec::new();
        let mut variable = tag.variable.clone();
        for &(first, count, slot) in &data.ranges {
            let Some(ov) = vm.overrides.get(slot as usize) else {
                continue;
            };
            let (tex, var) = match (&ov.lightmap, &ov.nightmap) {
                (Some((t, v)), _) => (t.clone(), Some(v.clone())),
                (None, Some(t)) => (t.clone(), None),
                _ => continue,
            };
            if variable.is_none() {
                variable = var;
            }
            let Some(path) = find_map(&tex, dirs) else {
                log::warn!(
                    "[light_emitter_interior] {}: its map {tex} is not found",
                    vm.file.display()
                );
                continue;
            };
            let Ok(img) = crate::image_decode::decode(&path) else {
                continue;
            };
            if img.width == 0 || img.height == 0 {
                continue;
            }
            let sample = |uv: Vec2| -> Vec3 {
                let u = uv.x.rem_euclid(1.0);
                let v = uv.y.rem_euclid(1.0);
                let x = ((u * img.width as f32) as u32).min(img.width - 1);
                let y = ((v * img.height as f32) as u32).min(img.height - 1);
                let i = ((y * img.width + x) * 4) as usize;
                let px = &img.rgba[i..i + 4];
                // (sRGB to linear light)
                let lin = |c: u8| (c as f32 / 255.0).powf(2.2);
                Vec3::new(lin(px[0]), lin(px[1]), lin(px[2]))
            };
            let tri = &data.indices[first as usize..(first + count) as usize];
            for t in tri.chunks_exact(3) {
                let [a, b, c] = [t[0] as usize, t[1] as usize, t[2] as usize];
                if a.max(b).max(c) >= data.positions.len() {
                    continue;
                }
                let (pa, pb, pc) = (data.positions[a], data.positions[b], data.positions[c]);
                let cr = (pb - pa).cross(pc - pa);
                let area = cr.length() * 0.5;
                if !(area > 1e-6) {
                    continue;
                }
                let uvm = (data.uvs[a] + data.uvs[b] + data.uvs[c]) / 3.0;
                // the middle and the corners pulled half way in: a strip's lit band is seen
                // even on a large triangle
                let col = (sample(uvm)
                    + sample(uvm.lerp(data.uvs[a], 0.5))
                    + sample(uvm.lerp(data.uvs[b], 0.5))
                    + sample(uvm.lerp(data.uvs[c], 0.5)))
                    * 0.25;
                let lum = col.dot(Vec3::new(0.2126, 0.7152, 0.0722));
                if lum < 0.02 {
                    continue;
                }
                let n = if data.normals.len() == data.positions.len() {
                    (data.normals[a] + data.normals[b] + data.normals[c]).normalize_or_zero()
                } else {
                    cr.normalize_or_zero()
                };
                lit.push(Lit {
                    pos: (pa + pb + pc) / 3.0,
                    normal: n,
                    color: col,
                    weight: lum * area,
                });
            }
        }
        if lit.is_empty() {
            continue;
        }
        let Some(variable) = variable else {
            log::warn!(
                "[light_emitter_interior] {}: a night map alone has no switch; give the tag a variable",
                vm.file.display()
            );
            continue;
        };
        for g in cluster(&lit, tag.max_lamps as usize) {
            let total: f32 = g.iter().map(|l| l.weight).sum();
            if total <= 0.0 {
                continue;
            }
            let pos = g.iter().map(|l| l.pos * l.weight).sum::<Vec3>() / total;
            let normal = g
                .iter()
                .map(|l| l.normal * l.weight)
                .sum::<Vec3>()
                .normalize_or_zero();
            let color = g.iter().map(|l| l.color * l.weight).sum::<Vec3>() / total;
            let peak = color.max_element().max(1e-4);
            // (the reach: an `[interiorlight]`'s range is how far its light stays at full
            // strength - a lit square metre of a white strip reaches about a metre)
            let range = (total.sqrt() * 2.2).clamp(0.3, 3.0);
            made.push((
                InteriorLight {
                    variable: variable.clone(),
                    range,
                    // (the strength scales the light's level: an `[interiorlight]`'s colour is
                    // its brightness, 255 the full one)
                    color: (color / peak * 255.0 * peak.sqrt().min(1.0) * tag.strength).to_array(),
                    pos: (pos + normal * 0.08).to_array(),
                },
                vm.file.display().to_string(),
            ));
        }
    }
    if made.is_empty() {
        return;
    }
    let first = model.interior_lights.len();
    let n = made.len();
    log::info!(
        "[light_emitter_interior]: {n} lamps from {} ({})",
        made.iter()
            .map(|(_, f)| f.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        made.first().map(|(_, f)| f.as_str()).unwrap_or("")
    );
    model
        .interior_lights
        .extend(made.into_iter().map(|(l, _)| l));
    for md in &mut model.meshes {
        if md.illumination_interior.iter().any(|&i| i >= 0) {
            for k in first..first + n {
                if md.illumination_interior.len() >= MAX_LAMPS_PER_MESH {
                    break;
                }
                md.illumination_interior.push(k as i32);
            }
        }
    }
}

/// A map's file in the model's texture folders (as spelled, or as a `.dds` beside it).
fn find_map(name: &str, dirs: &[&Path]) -> Option<std::path::PathBuf> {
    let dds = Path::new(name).with_extension("dds");
    for d in dirs {
        for n in [Path::new(name), dds.as_path()] {
            let p = omsi_cfg::resolve_path(d, &n.to_string_lossy());
            if omsi_cfg::vfs::exists(&p) {
                return Some(p);
            }
        }
    }
    None
}

/// At most `k` groups of the lit triangles (weighted k-means over their places, started
/// from the farthest points).
fn cluster(lit: &[Lit], k: usize) -> Vec<Vec<&Lit>> {
    let k = k.clamp(1, 16).min(lit.len());
    let total: f32 = lit.iter().map(|l| l.weight).sum();
    let mean = lit.iter().map(|l| l.pos * l.weight).sum::<Vec3>() / total.max(1e-9);
    let mut centres: Vec<Vec3> = Vec::with_capacity(k);
    let start = lit
        .iter()
        .max_by(|a, b| {
            a.pos
                .distance_squared(mean)
                .total_cmp(&b.pos.distance_squared(mean))
        })
        .map(|l| l.pos)
        .unwrap_or(mean);
    centres.push(start);
    while centres.len() < k {
        let far = lit
            .iter()
            .map(|l| {
                let d = centres
                    .iter()
                    .map(|c| c.distance_squared(l.pos))
                    .fold(f32::MAX, f32::min);
                (d * l.weight.sqrt(), l.pos)
            })
            .max_by(|a, b| a.0.total_cmp(&b.0));
        match far {
            Some((d, p)) if d > 0.04 => centres.push(p),
            _ => break,
        }
    }
    let mut owner = vec![0usize; lit.len()];
    for _ in 0..12 {
        for (i, l) in lit.iter().enumerate() {
            owner[i] = centres
                .iter()
                .enumerate()
                .min_by(|a, b| {
                    a.1.distance_squared(l.pos)
                        .total_cmp(&b.1.distance_squared(l.pos))
                })
                .map(|(j, _)| j)
                .unwrap_or(0);
        }
        for (j, c) in centres.iter_mut().enumerate() {
            let (mut s, mut w) = (Vec3::ZERO, 0.0);
            for (i, l) in lit.iter().enumerate() {
                if owner[i] == j {
                    s += l.pos * l.weight;
                    w += l.weight;
                }
            }
            if w > 0.0 {
                *c = s / w;
            }
        }
    }
    let mut groups: Vec<Vec<&Lit>> = vec![Vec::new(); centres.len()];
    for (i, l) in lit.iter().enumerate() {
        groups[owner[i]].push(l);
    }
    groups.retain(|g| !g.is_empty());
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f32, w: f32) -> Lit {
        Lit {
            pos: Vec3::new(x, 0.0, 2.0),
            normal: -Vec3::Z,
            color: Vec3::ONE,
            weight: w,
        }
    }

    #[test]
    fn two_strips_make_two_lamps() {
        let lit: Vec<Lit> = (0..10)
            .map(|i| at(i as f32 * 0.1, 1.0))
            .chain((0..10).map(|i| at(5.0 + i as f32 * 0.1, 1.0)))
            .collect();
        let g = cluster(&lit, 2);
        assert_eq!(g.len(), 2);
        assert!(g.iter().all(|g| g.len() == 10));
    }

    #[test]
    fn one_spot_stays_one_lamp() {
        let lit: Vec<Lit> = (0..5).map(|_| at(1.0, 1.0)).collect();
        assert_eq!(cluster(&lit, 4).len(), 1);
    }
}
