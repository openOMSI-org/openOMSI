//! Rays into a vehicle: the `[mouseevent]` meshes, the HTML pages and the trailers' switches.

use super::*;

/// The `[mouseevent]` mesh a ray picks, and whether the ray through the cursor itself found
/// it (not the rings around it). `reach`: how far along the ray through the cursor and along
/// the rings a switch may be and still be reached ([`Reach`]).
pub(crate) fn pick_in(vehicle: &omsi_sim::VehicleInstance, origin: DVec3, dir: Vec3, spread: f32, reach: Reach) -> Option<(usize, bool)> {
    let o = (origin - vehicle.position).as_vec3();
    let right = Vec3::new(-dir.y, dir.x, 0.0).normalize_or_zero();
    let up = dir.cross(right).normalize_or_zero();
    let mut rings: Vec<Vec<Vec3>> = vec![vec![dir]];
    if spread > 0.0 {
        for ring in 1..=2 {
            let r = spread * ring as f32;
            let n = 8 * ring;
            rings.push(
                (0..n)
                    .map(|k| {
                        let a = k as f32 / n as f32 * std::f32::consts::TAU;
                        (dir + right * (a.cos() * r) + up * (a.sin() * r)).normalize()
                    })
                    .collect(),
            );
        }
    }
    // The same broad-phase applies to every ring. A large cockpit can contain
    // hundreds of meshes; calculating their posed transforms three times made
    // hovering over its controls needlessly expensive.
    let candidates: Vec<(usize, glam::Mat4, Vec<u32>)> = vehicle.ty.meshes.iter().enumerate().filter_map(|(i, vm)| {
        if vehicle.ty.model.meshes[vm.def_index].mouse_event.is_none() || !vehicle.mesh_props[i].visible {
            return None;
        }
        let xf = vehicle.mesh_local_transform(i);
        if !ray_may_hit(&vehicle.ty, i, &xf, o, dir, spread * 2.2) {
            return None;
        }
        let tris = omsi_geometry::cone_triangles(o, dir, spread * 2.0 + 1e-4, &vm.data, &xf);
        (!tris.is_empty()).then_some((i, xf, tris))
    }).collect();
    for (ring, dirs) in rings.iter().enumerate() {
        let limit = reach.limit(ring);
        // Collect every hit in this ring, then among surfaces within a depth band of the
        // nearest prefer the mesh whose centre the ray aims at. Aachen ibox D10/D11/D12 are
        // tilted, overlap by ~6 mm, and sit ~16 mm apart along Z - a 2 mm depth-only pick
        // let backspace (D10) steal digit 0 (D11) whenever both boxes were hit.
        let mut hits: Vec<(f32, f32, usize)> = Vec::new();
        for (i, xf, tris) in &candidates {
            let (i, xf) = (*i, *xf);
            let vm = &vehicle.ty.meshes[i];
            let lateral = vehicle
                .ty
                .mesh_bounds
                .get(i)
                .map(|&(c, _)| {
                    let p = xf.transform_point3(c) - o;
                    let along = p.dot(dir);
                    (p - dir * along).length_squared()
                })
                .unwrap_or(0.0);
            for d in dirs {
                if let Some(t) = omsi_geometry::ray_triangles(o, *d, &vm.data, &xf, tris).filter(|&t| t <= limit) {
                    hits.push((t, lateral, i));
                }
            }
        }
        if let Some(i) = best_mouseevent_hit(&hits) {
            return Some((i, ring == 0));
        }
    }
    None
}

/// How far a switch may stand behind the nearest part that hides it, along the ray through
/// the cursor, and still take the click: a button flush with its panel, or a hair behind its
/// face where the two were modelled as one surface, stays reachable.
pub(crate) const FLUSH_SLACK: f32 = 0.01;

/// The same for the rings of rays around the cursor, measured against the part the ray
/// through the cursor meets: a ring ray lands on a switch beside it at another depth than the
/// middle ray on a tilted panel, so it is given more room; a switch modelled well behind a
/// panel (an engine hatch behind the dashboard) is still out of reach.
pub(crate) const RING_SLACK: f32 = 0.05;

/// How far along a ray into a vehicle a switch can be reached: up to the nearest part that
/// hides what is behind it (infinite: nothing does).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Reach(pub f32);

impl Reach {
    pub(crate) const ANY: Reach = Reach(f32::INFINITY);

    /// The farthest hit ring `ring` (0: the ray through the cursor) may take.
    fn limit(self, ring: usize) -> f32 {
        self.0 + if ring == 0 { FLUSH_SLACK } else { RING_SLACK }
    }
}

/// How far a switch of body `ty` can be reached along a ray from `o` (the body's frame, `dir`
/// of unit length). From the cab nothing hides a switch, as in Omsi.exe: buses made for it
/// put seats, ticket machines and body panels in front of switches that are clicked through
/// them (the O560's stop request behind the driver's seat, the Lion's City G's roof hatches
/// behind its body). From outside (`outside`) any part of the body hides what is behind it.
pub(crate) fn reach_into(ty: &omsi_sim::VehicleType, props: &[omsi_sim::vehicle::MeshProps], xf_of: impl Fn(usize) -> glam::Mat4, o: Vec3, dir: Vec3, outside: bool) -> Reach {
    if !outside {
        return Reach::ANY;
    }
    Reach(hiding_part(ty, props, xf_of, o, dir).map_or(f32::INFINITY, |(_, t)| t))
}

/// The nearest part of a body, seen from outside, that hides what is behind it along a ray
/// (see [`reach_into`]): its mesh and how far along the ray it is. The switches themselves
/// (`[mouseevent]` meshes) are left out - among them the nearest wins anyway - and so are
/// shadow meshes, meshes the outside view does not draw (`[viewpoint]`) and slots a script
/// has faded out; walls and windows alike hide.
pub(crate) fn hiding_part(ty: &omsi_sim::VehicleType, props: &[omsi_sim::vehicle::MeshProps], xf_of: impl Fn(usize) -> glam::Mat4, o: Vec3, dir: Vec3) -> Option<(usize, f32)> {
    let mut nearest: Option<(usize, f32)> = None;
    for (i, vm) in ty.meshes.iter().enumerate() {
        let def = &ty.model.meshes[vm.def_index];
        let Some(p) = props.get(i).filter(|p| p.visible) else { continue };
        let vp = def.viewpoint;
        let drawn = vp == 0 || vp & 1 != 0;
        if def.mouse_event.is_some() || def.is_shadow || !drawn {
            continue;
        }
        let xf = xf_of(i);
        if !ray_may_hit(ty, i, &xf, o, dir, 0.0) {
            continue;
        }
        let hides = |slot: usize| p.slot_alpha.get(slot).is_none_or(|&a| a > 0.01);
        let tris: Vec<u32> = vm.data.ranges.iter().filter(|r| hides(r.2 as usize)).flat_map(|r| (r.0..r.0 + r.1).step_by(3)).collect();
        // (a face the eye stands in hides nothing)
        if let Some(t) = omsi_geometry::ray_triangles(o, dir, &vm.data, &xf, &tris).filter(|&t| t > 0.02) {
            if nearest.is_none_or(|(_, n)| t < n) {
                nearest = Some((i, t));
            }
        }
    }
    nearest
}

/// Depth band (m) inside which overlapping cockpit `[mouseevent]` meshes are disambiguated
/// by aim (lateral distance to the mesh centre) rather than by which face is a hair closer.
/// Sized for the Aachen ibox digit row (D10/D11/D12 centres ~16 mm apart on a tilted pad).
pub(crate) const MOUSEEVENT_PICK_DEPTH_BAND: f32 = 0.03;

/// Among ray hits `(t, lateral², id)`, pick the aimed mesh: nearest surface first, then the
/// smallest lateral distance among hits within [`MOUSEEVENT_PICK_DEPTH_BAND`] of that nearest.
pub(crate) fn best_mouseevent_hit(hits: &[(f32, f32, usize)]) -> Option<usize> {
    let min_t = hits.iter().map(|h| h.0).fold(f32::INFINITY, f32::min);
    if !min_t.is_finite() {
        return None;
    }
    hits.iter()
        .filter(|h| h.0 <= min_t + MOUSEEVENT_PICK_DEPTH_BAND)
        .min_by(|a, b| {
            a.1.partial_cmp(&b.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
        })
        .map(|h| h.2)
}

#[cfg(test)]
mod mouseevent_pick_tests {
    use super::{best_mouseevent_hit, MOUSEEVENT_PICK_DEPTH_BAND};

    #[test]
    fn aachen_ibox_d11_wins_when_d10_is_closer_but_off_aim() {
        // Realistic AC O530 digit row: D10 (backspace) ~12 mm closer along the ray than
        // D11 (digit 0), but the cursor aims at D11's centre (lateral ≈ 0).
        let d10 = 0usize;
        let d11 = 1usize;
        let d12 = 2usize;
        let hits = [
            (0.488, 0.012_f32.powi(2), d10),
            (0.500, 0.001_f32.powi(2), d11),
            (0.512, 0.015_f32.powi(2), d12),
        ];
        assert!(
            (hits[1].0 - hits[0].0) > 0.002,
            "fixture must exceed the old 2 mm tie window"
        );
        assert!((hits[1].0 - hits[0].0) < MOUSEEVENT_PICK_DEPTH_BAND);
        assert_eq!(best_mouseevent_hit(&hits), Some(d11));
    }

    #[test]
    fn clearly_nearer_surface_still_wins_outside_the_band() {
        let hits = [(0.40, 0.02_f32.powi(2), 0usize), (0.50, 0.0, 1usize)];
        assert_eq!(best_mouseevent_hit(&hits), Some(0));
    }

    #[test]
    fn empty_hits_yield_none() {
        assert_eq!(best_mouseevent_hit(&[]), None);
    }
}

/// The page of an `[htmltexture]` a ray lands on: its script texture index and the place on it
/// (`u`/`v` from 0 to 1, `v` down from the top), for the nearest such surface. The nearest
/// triangle of a mesh decides: a bezel of the same mesh in front of the screen takes the click
/// away from it. `also`: script textures clicked like pages (those `.opl` plugins draw).
pub(crate) fn pick_html_in(vehicle: &omsi_sim::VehicleInstance, origin: DVec3, dir: Vec3, also: &[usize]) -> Option<(usize, f32, f32)> {
    let pages: Vec<usize> = vehicle.html_textures.iter().map(|t| t.script_index).chain(also.iter().copied()).collect();
    if pages.is_empty() {
        return None;
    }
    let o = (origin - vehicle.position).as_vec3();
    let mut best: Option<(f32, usize, f32, f32)> = None;
    for (i, vm) in vehicle.ty.meshes.iter().enumerate() {
        if !vehicle.mesh_props[i].visible {
            continue;
        }
        let def = &vehicle.ty.model.meshes[vm.def_index];
        let shows_page = |n: Option<i32>| n.is_some_and(|n| pages.contains(&(n.max(0) as usize)));
        if !def.materials.iter().any(|m| shows_page(m.use_script_texture)) {
            continue;
        }
        let xf = vehicle.mesh_local_transform(i);
        if !ray_may_hit(&vehicle.ty, i, &xf, o, dir, 0.0) {
            continue;
        }
        let Some(hit) = omsi_geometry::ray_mesh_hit(o, dir, &vm.data, &xf) else {
            continue;
        };
        // the page the hit material slot shows (a slot that shows none is in the way)
        let slot = vm.data.slot_of(hit.index) as usize;
        let page = def
            .materials
            .iter()
            .filter(|m| omsi_sim::vehicle::override_slot(&vm.materials, m) == Some(slot))
            .find_map(|m| m.use_script_texture)
            .map(|n| n.max(0) as usize)
            .filter(|n| pages.contains(n));
        let Some(page) = page else { continue };
        if best.is_none_or(|b| hit.t < b.0) {
            best = Some((hit.t, page, hit.uv.x.clamp(0.0, 1.0), hit.uv.y.clamp(0.0, 1.0)));
        }
    }
    best.map(|(_, page, u, v)| (page, u, v))
}

/// The same forgiving pick as `pick`, but for the coupled sections of an articulated
/// bus.  The old picker only searched the lead vehicle, so GN92's rear door opener and
/// every other button in the second section could never receive a click.
pub(crate) fn pick_trailer_in(vehicle: &omsi_sim::VehicleInstance, origin: DVec3, dir: Vec3, spread: f32, reach: Reach) -> Option<((usize, usize), bool)> {
    let right = Vec3::new(-dir.y, dir.x, 0.0).normalize_or_zero();
    let up = dir.cross(right).normalize_or_zero();
    let mut rings: Vec<Vec<Vec3>> = vec![vec![dir]];
    if spread > 0.0 {
        for ring in 1..=2 {
            let r = spread * ring as f32;
            let n = 8 * ring;
            rings.push(
                (0..n)
                    .map(|k| {
                        let a = k as f32 / n as f32 * std::f32::consts::TAU;
                        (dir + right * (a.cos() * r) + up * (a.sin() * r)).normalize()
                    })
                    .collect(),
            );
        }
    }
    let candidates: Vec<(usize, usize, glam::Mat4, Vec<u32>)> = vehicle.trailers.iter().enumerate().flat_map(|(ti, trailer)| {
        let o = (origin - trailer.position).as_vec3();
        trailer.ty.meshes.iter().enumerate().filter_map(move |(i, vm)| {
            if trailer.ty.model.meshes[vm.def_index].mouse_event.is_none() || !trailer.mesh_props[i].visible {
                return None;
            }
            let xf = trailer.mesh_local_transform(i);
            if !ray_may_hit(&trailer.ty, i, &xf, o, dir, spread * 2.2) {
                return None;
            }
            let tris = omsi_geometry::cone_triangles(o, dir, spread * 2.0 + 1e-4, &vm.data, &xf);
            (!tris.is_empty()).then_some((ti, i, xf, tris))
        })
    }).collect();
    for (ring, dirs) in rings.iter().enumerate() {
        let limit = reach.limit(ring);
        let mut hits: Vec<(f32, f32, usize)> = Vec::new();
        let mut id_of: Vec<(usize, usize)> = Vec::new();
        for (ti, i, xf, tris) in &candidates {
            let (ti, i, xf) = (*ti, *i, *xf);
            let trailer = &vehicle.trailers[ti];
            let o = (origin - trailer.position).as_vec3();
            let vm = &trailer.ty.meshes[i];
            let lateral = trailer
                .ty
                .mesh_bounds
                .get(i)
                .map(|&(c, _)| {
                    let p = xf.transform_point3(c) - o;
                    let along = p.dot(dir);
                    (p - dir * along).length_squared()
                })
                .unwrap_or(0.0);
            for d in dirs {
                if let Some(t) = omsi_geometry::ray_triangles(o, *d, &vm.data, &xf, tris).filter(|&t| t <= limit) {
                    let id = id_of.len();
                    id_of.push((ti, i));
                    hits.push((t, lateral, id));
                }
            }
        }
        if let Some(id) = best_mouseevent_hit(&hits) {
            return Some((id_of[id], ring == 0));
        }
    }
    None
}

#[cfg(test)]
mod reach_tests {
    use super::{pick_in, reach_into, Reach};
    use glam::{DVec3, Mat4, Vec3};

    /// A square of 1 m across `y` = `at`, facing the origin, its one slot showing `texture`.
    fn wall(at: f32, texture: &str, mouse: bool, def_index: usize) -> (omsi_sim::vehicle::VehicleMesh, omsi_model::MeshDef) {
        let data = omsi_geometry::MeshData {
            positions: vec![Vec3::new(-0.5, at, -0.5), Vec3::new(0.5, at, -0.5), Vec3::new(0.5, at, 0.5), Vec3::new(-0.5, at, 0.5)],
            indices: vec![0, 1, 2, 0, 2, 3],
            ranges: vec![(0, 6, 0)],
            ..Default::default()
        };
        let material = omsi_o3d::Material { diffuse: [1.0; 4], specular: [0.0; 3], emissive: [0.0; 3], specular_power: 0.0, texture: texture.into() };
        let glass = omsi_model::MaterialDef { texture: texture.into(), alpha: 2, alpha_set: true, ..Default::default() };
        let mesh = omsi_sim::vehicle::VehicleMesh {
            def_index,
            data,
            file: Default::default(),
            materials: vec![material],
            overrides: if texture == "glass.dds" { vec![glass.clone()] } else { Vec::new() },
            pivot: Mat4::IDENTITY,
            viewpoint: 0,
            skin: Vec::new(),
            keep_winding: false,
        };
        let def = omsi_model::MeshDef {
            mouse_event: mouse.then(|| "switch".to_string()),
            materials: if texture == "glass.dds" { vec![glass] } else { Vec::new() },
            ..Default::default()
        };
        (mesh, def)
    }

    /// A bus of a panel (`panel`: its texture) at 1 m in front of the eye and a switch at
    /// `switch` m.
    fn bus(panel: &str, switch: f32) -> omsi_sim::VehicleInstance {
        let (pm, pd) = wall(1.0, panel, false, 0);
        let (sm, sd) = wall(switch, "switch.dds", true, 1);
        let mut model = omsi_model::Model::default();
        model.meshes = vec![pd, sd];
        let ty = omsi_sim::VehicleType {
            def: Default::default(),
            model,
            model_dir: Default::default(),
            program: Default::default(),
            meshes: vec![pm, sm],
            lod_meshes: Vec::new(),
            paint_schemes: Vec::new(),
            texchanges: Vec::new(),
            wheel_meshes: Vec::new(),
            suspension_axles: Vec::new(),
            missing_packs: Vec::new(),
            mesh_bounds: Vec::new(),
            mesh_boxes: Vec::new(),
        };
        omsi_sim::VehicleInstance::new(std::sync::Arc::new(ty), omsi_sim::VehicleHost::new(Default::default()))
    }

    fn picks(v: &omsi_sim::VehicleInstance, outside: bool) -> Option<usize> {
        let reach = reach_into(&v.ty, &v.mesh_props, |i| v.mesh_local_transform(i), Vec3::ZERO, Vec3::Y, outside);
        pick_in(v, v.position, Vec3::Y, 0.0, reach).map(|(i, _)| i)
    }

    /// From outside a switch behind any part of the bus cannot be clicked through it, glass
    /// too; a switch flush with its panel, or in front of it, stays reachable. From the cab
    /// nothing hides a switch, as in Omsi.exe.
    #[test]
    fn a_switch_behind_another_part_is_out_of_reach_from_outside() {
        assert_eq!(picks(&bus("dash.dds", 1.2), true), None);
        assert_eq!(picks(&bus("glass.dds", 1.2), true), None);
        assert_eq!(picks(&bus("dash.dds", 1.2), false), Some(1));
        assert_eq!(picks(&bus("glass.dds", 1.2), false), Some(1));
        assert_eq!(picks(&bus("dash.dds", 1.004), false), Some(1));
        assert_eq!(picks(&bus("dash.dds", 0.995), true), Some(1));
        assert_eq!(pick_in(&bus("dash.dds", 1.2), DVec3::ZERO, Vec3::Y, 0.0, Reach::ANY).map(|h| h.0), Some(1));
    }

    /// Real buses: every switch that a ray from one of the bus's cameras picks with nothing
    /// hiding it (aimed at the faces of the switch) is still picked from one of them through
    /// `reach_into` as the cab uses it. Buttons on the body's outer skin or named `_external`
    /// (the doors' outside openers, the pram request) and the SD202's ticket while it rests
    /// inside its printer are for other views and not counted. Stock buses by default; more
    /// with `OMSI_PICK_BUSES` (comma-separated `.bus` paths under the root, or absolute).
    #[test]
    #[ignore = "needs the installed content; set OMSI_ROOT"]
    fn installed_switches_stay_reachable_from_inside() {
        let Some(root) = omsi_cfg::flags::OMSI_ROOT.live_os().map(std::path::PathBuf::from) else {
            eprintln!("skipped: set OMSI_ROOT");
            return;
        };
        let mut files: Vec<String> = ["Vehicles/MAN_SD202/MAN_D92.bus", "Vehicles/MAN_NL_NG/MAN_EN92_main.bus", "Vehicles/MAN_NL_NG/MAN_GN92_main.bus"].map(String::from).to_vec();
        files.extend(std::env::var("OMSI_PICK_BUSES").unwrap_or_default().split(',').filter(|f| !f.trim().is_empty()).map(|f| f.trim().to_string()));
        let mut lost = Vec::new();
        for file in &files {
            let ty = omsi_sim::VehicleType::load(&root, &root.join(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
            let mut v = omsi_sim::VehicleInstance::new(std::sync::Arc::new(ty), omsi_sim::VehicleHost::new(Default::default()));
            // (a few frames standing: the scripts set which parts and switches show)
            for _ in 0..3 {
                v.update(1.0 / 30.0);
            }
            // (the driver's cameras and the passengers': a switch in the saloon is for the
            // views there, not for the driver behind the cab wall)
            let eyes: Vec<Vec3> = v.ty.def.cameras_driver.iter().chain(&v.ty.def.cameras_pax).map(|c| Vec3::from(c.pos)).collect();
            assert!(!v.ty.def.cameras_driver.is_empty(), "{file}: no driver camera");
            let half_width = v.ty.def.bounding_box.map(|b| b[0] * 0.5);
            let (mut switches, mut reachable) = (0, 0);
            for (i, vm) in v.ty.meshes.iter().enumerate() {
                let def = &v.ty.model.meshes[vm.def_index];
                if def.mouse_event.is_none() || !v.mesh_props.get(i).is_some_and(|p| p.visible) || vm.data.indices.len() < 3 {
                    continue;
                }
                let xf = v.mesh_local_transform(i);
                let faces = vm.data.indices.len() / 3;
                // (the middle of each face, and a point near each corner of it: what the eye
                // can see of the switch)
                let samples = faces.min(400);
                let aims: Vec<Vec3> = (0..samples)
                    .flat_map(|k| {
                        let f = k * faces / samples;
                        let corner = |c: usize| xf.transform_point3(vm.data.positions[vm.data.indices[f * 3 + c] as usize]);
                        let mid = (corner(0) + corner(1) + corner(2)) / 3.0;
                        [mid, mid.lerp(corner(0), 0.8), mid.lerp(corner(1), 0.8), mid.lerp(corner(2), 0.8)]
                    })
                    .collect();
                let picked = |with_parts: bool| {
                    eyes.iter().any(|&eye| {
                        aims.iter().any(|&aim| {
                            let dir = (aim - eye).normalize_or_zero();
                            let reach = if with_parts { reach_into(&v.ty, &v.mesh_props, |m| v.mesh_local_transform(m), eye, dir, false) } else { Reach::ANY };
                            pick_in(&v, v.position + eye.as_dvec3(), dir, 0.0, reach).is_some_and(|h| h.0 == i)
                        })
                    })
                };
                let name = def.mouse_event.clone().unwrap_or_default();
                // (on the body's outer skin, or wider: a button for outside)
                let side = aims.iter().map(|a| a.x.abs()).sum::<f32>() / aims.len() as f32;
                let outside = half_width.is_some_and(|h| side >= h - 0.03) || name.ends_with("_external");
                if !picked(false) || outside || name == "ticketprinter_getticket" {
                    continue;
                }
                switches += 1;
                if picked(true) {
                    reachable += 1;
                } else {
                    // (what hides it, along the first aim from the first camera)
                    let (eye, aim) = (eyes[0], aims[0]);
                    let dir = (aim - eye).normalize_or_zero();
                    let hider = super::hiding_part(&v.ty, &v.mesh_props, |m| v.mesh_local_transform(m), eye, dir);
                    let what = hider.map(|(m, t)| {
                        let file = v.ty.meshes[m].file.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
                        format!("{file} at {t:.3} m, switch at {:.3} m", (aim - eye).length())
                    });
                    lost.push(format!("{file}: {} behind {}", def.mouse_event.clone().unwrap_or_default(), what.unwrap_or_default()));
                }
            }
            eprintln!("{file}: {reachable} of {switches} switches reachable from inside");
        }
        assert!(lost.is_empty(), "switches lost to the parts in front of them: {lost:#?}");
    }
}
