//! The game's camera helpers: the follow and offscreen cameras, and drawing the mirrors;
//! the geometry they use is `omsi_scene::camera_util`.

use super::*;
pub(crate) use omsi_scene::camera_util::*;




/// `--look yaw,pitch`: how far the head is turned in an offscreen run.
pub(crate) fn look_of(args: &Args) -> (f32, f32) {
    args.look
        .as_deref()
        .map(|s| {
            let v: Vec<f32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            (
                v.first().copied().unwrap_or(0.0),
                v.get(1).copied().unwrap_or(0.0),
            )
        })
        .unwrap_or((0.0, 0.0))
}

/// How far the offscreen outside camera sits from the vehicle: 18 m, or `OMSI_ORBIT_DIST`
/// metres for close-ups (a headlight, a door) together with `--look`.
pub(crate) fn offscreen_orbit() -> f32 {
    omsi_cfg::flags::OMSI_ORBIT_DIST
        .var()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(18.0)
}


/// The AI car to follow: an id, "auto" = the first overtaker, "moving" = the oldest car
/// driving when first asked, "turn" = the first car entering a turning lane, "steer" = the
/// car steering hardest, "bus" = the first timetable bus, "type:<name>" = the first vehicle
/// whose file name contains it.
pub(crate) fn follow_id(args: &Args, traffic: Option<&traffic::Traffic>) -> Option<u64> {
    match args.follow.as_deref()? {
        "auto" => traffic?.last_overtaker.map(|o| o.0),
        // the oldest car that is driving when first asked, kept while it exists
        "moving" => {
            thread_local!(static CHOSEN: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) });
            let t = traffic?;
            if let Some(id) = CHOSEN
                .with(|c| c.get())
                .filter(|id| t.car_pose(*id).is_some())
            {
                return Some(id);
            }
            let id = t
                .cars
                .iter()
                .filter(|c| c.state.speed > 3.0)
                .map(|c| c.id)
                .min()?;
            CHOSEN.with(|c| c.set(Some(id)));
            Some(id)
        }
        // the oldest timetable bus that is under way when first asked, kept while it exists
        "bus" => {
            thread_local!(static BUS: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) });
            let t = traffic?;
            if let Some(id) = BUS.with(|c| c.get()).filter(|id| t.car_pose(*id).is_some()) {
                return Some(id);
            }
            let id = t.cars.iter().filter(|c| c.is_bus() && c.state.speed > 2.0).map(|c| c.id).min()?;
            BUS.with(|c| c.set(Some(id)));
            Some(id)
        }
        "turn" => traffic?.first_turner.map(|o| o.0),
        // the first car stopped by a red light, giving way at a junction without lights,
        // pulling out onto the other half of the road round an obstacle
        "red" => traffic?.first_red.map(|o| o.0),
        "yield" => traffic?.first_yield.map(|o| o.0),
        "pass" => traffic?.first_passer.map(|o| o.0),
        // whichever moving car steers hardest at this moment (wheel close-ups)
        "steer" => traffic?
            .cars
            .iter()
            .filter(|c| c.state.speed > 1.0 && !c.is_bus())
            .max_by(|a, b| a.body.steer.abs().total_cmp(&b.body.steer.abs()))
            .map(|c| c.id),
        v if v.starts_with("type:") => {
            let want = v[5..].to_ascii_lowercase();
            traffic?
                .cars
                .iter()
                .find(|c| {
                    c.vehicle
                        .ty
                        .def
                        .path
                        .to_string_lossy()
                        .to_ascii_lowercase()
                        .contains(&want)
                })
                .map(|c| c.id)
        }
        v => v.parse().ok(),
    }
}

/// Chase camera behind AI car `id`: from high above by default, or where
/// `OMSI_FOLLOW_CAM=right,forward,up,yaw,pitch` puts it in the car's frame (metres, and
/// degrees relative to the car's heading); a sixth value 1 reads the offset as east, north,
/// up and the yaw as a compass heading, so the view does not turn with the car.
pub(crate) fn follow_camera(traffic: Option<&traffic::Traffic>, id: u64) -> Option<Camera> {
    let (pos, heading) = traffic?.car_pose(id)?;
    let h = (heading as f32).to_radians();
    let back = DVec3::new(-(h.sin() as f64), -(h.cos() as f64), 0.0);
    // OMSI_FOLLOW_CAM=right,ahead,up,yaw,pitch: a camera in the car's own frame (metres,
    // degrees relative to its heading), e.g. beside the road with the car at the edge of
    // the picture; `yaw` may be a list a/b/c, one per snapshot (--snapshots); a sixth value
    // 1 reads the offset as east, north, up and the yaw as a compass heading, so the view
    // does not turn with the car
    if let Some(v) = omsi_cfg::flags::OMSI_FOLLOW_CAM.var() {
        thread_local!(static CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) });
        let mut fields: Vec<String> = v.split(',').map(|x| x.trim().to_string()).collect();
        if let Some(yaws) = fields.get_mut(3).filter(|y| y.contains('/')) {
            let list: Vec<String> = yaws.split('/').map(|s| s.to_string()).collect();
            let k = CALLS.with(|c| c.replace(c.get() + 1));
            *yaws = list[k.min(list.len() - 1)].clone();
        }
        let f: Vec<f64> = fields.iter().filter_map(|x| x.parse().ok()).collect();
        if f.len() == 6 && f[5] != 0.0 {
            let position = pos + DVec3::new(f[0], f[1], f[2]);
            return Some(Camera {
                position,
                yaw: f[3] as f32,
                pitch: f[4] as f32,
                roll: 0.0,
                fov_deg: 60.0,
                near: 0.1,
                far: 6000.0,
            });
        }
        if f.len() >= 5 {
            let right = DVec3::new(h.cos() as f64, -(h.sin() as f64), 0.0);
            return Some(Camera {
                position: pos + right * f[0] - back * f[1] + DVec3::new(0.0, 0.0, f[2]),
                yaw: heading as f32 + f[3] as f32,
                pitch: f[4] as f32,
                roll: 0.0,
                fov_deg: 60.0,
                near: 0.1,
                far: 6000.0,
            });
        }
    }
    Some(Camera {
        position: pos + back * 6.0 + DVec3::new(0.0, 0.0, 45.0),
        yaw: heading as f32,
        pitch: -80.0,
        roll: 0.0,
        fov_deg: 60.0,
        near: 0.1,
        far: 6000.0,
    })
}

/// How much of the plain light a mirror loses at full night in enhanced graphics (see
/// `render_mirrors`): measured in the window at 23:30, the mirrors' street, the bus's own
/// flank and the sky then match the enhanced picture beside them (0.45 left them 2-3 times
/// as bright).
const MIRROR_NIGHT_DIM: f32 = 0.8;

/// The least radius a mirror's camera counts with in the visibility test (m). A mirror's
/// camera sits in the middle of its glass; with the file's radius 0 (`[add_camera_reflexion]`)
/// a mirror whose lower half shows at the edge of the picture - the SD200's right mirror
/// from the driver's seat - was never redrawn and stood frozen.
const MIRROR_MIN_RADIUS: f32 = 0.3;


/// A mirror's picture is drawn this wide over its height (Omsi.exe sub_6f6468 gives the
/// reflection cameras' projection 1.6 on their square textures), and the mirror meshes show
/// its middle: drawn square, a mirror showed a slice two thirds as wide as OMSI's - the
/// strip of the bus at its edge and hardly any of the lane beside it.
const MIRROR_ASPECT: f32 = 1.6;



/// Where the driver's eye is (for a mirror drawn with no view to aim it by).
pub(crate) fn driver_eye(p: &Player) -> DVec3 {
    if let Some((t, c)) = p.trailer_driver_camera() {
        return t.camera_world_full(c).0;
    }
    let def = &p.vehicle.ty.def;
    match def.cameras_driver.get(p.driver_camera_slot()) {
        Some(c) => p.vehicle.camera_world(c).0 + p.vehicle.body_rotation().transform_vector3(p.head_offset() + p.seat).as_dvec3(),
        None => p.vehicle.position + DVec3::Z * 2.0,
    }
}

/// Which of the vehicle's mirrors [`render_mirrors`] draws.
#[derive(Clone, Copy, Debug)]
pub(crate) enum MirrorPick<'a> {
    /// Every mirror `view` sees (all of them with no view) but those in the list.
    All(&'a [usize]),
    /// Just one, the `k % n`-th of those `view` sees and the list leaves out (round robin, as
    /// OMSI takes its turns among the mirrors in the picture).
    Turn(usize, &'a [usize]),
    /// These mirrors, seen or not (the mirror panels': they are on the screen wherever the
    /// glass is).
    Only(&'a [usize]),
}

/// Draw the views of the vehicle's `[add_camera_reflexion]` cameras into its mirror textures,
/// those `pick` takes; the number of mirrors it chose among (those in the picture, or the
/// list `Only` gives).
pub(crate) fn render_mirrors(
    renderer: &mut Renderer,
    scene: &mut Scene,
    world: &scene::World,
    p: &Player,
    lighting: &omsi_render::Lighting,
    pick: MirrorPick,
    view: Option<(Camera, f32)>,
    on_screen: &[usize],
) -> usize {
    // (aimed from the eye of the view being drawn, as Omsi.exe aims them - from the
    // driver's without one)
    let eye = view.as_ref().map(|v| v.0.position).unwrap_or_else(|| driver_eye(p));
    // the leading vehicle's cameras, then its rear sections' (openOMSI: numbered on from the
    // last of the one in front, so a screen in the cab can show a camera on a rear door),
    // each aimed and placed in its own body
    let v = &p.vehicle;
    let mut cams: Vec<(omsi_vehicle::Camera, (DVec3, f32, f32, f32))> = Vec::new();
    let aim = |i: usize, c: &omsi_vehicle::Camera, position: DVec3, rot: glam::Mat4| {
        let c = adjusted(c, p.mirror_shifts.get(i).copied().unwrap_or([0.0; 3]), p.mirror_fovs.get(i).copied().unwrap_or(0.0));
        mirror_view_in(position, rot, &c, eye, p.mirror_offsets.get(i).copied().unwrap_or([0.0; 2]))
    };
    for c in &v.ty.def.cameras_reflexion {
        let c = aim(cams.len(), c, v.position, v.body_rotation());
        let at = v.camera_world_full(&c);
        cams.push((c, at));
    }
    for t in &v.trailers {
        for c in &t.ty.def.cameras_reflexion {
            let c = aim(cams.len(), c, t.position, t.body_rotation());
            let at = t.camera_world_full(&c);
            cams.push((c, at));
        }
    }
    if cams.is_empty() {
        return 0;
    }
    let textures = world.mirror_textures.lock().clone();
    // small images: skip objects that would be tiny anyway (the original's
    // performance_minObjSizeRefl)
    let mut lighting = lighting.clone();
    // a mirror's small picture: nothing smaller than a few of its pixels, and nothing much
    // beyond what a mirror shows (the far distance below)
    let px = world.settings.mirror_size().max(64) as f32;
    lighting.min_obj_size = lighting.min_obj_size.max((12.0 / px).clamp(0.03, 0.09));
    // (no shadow pass of their own: the window's shadow maps are looked up, see
    // `Renderer::plan_shadow_cache`; the mirrors drawn as the Enhanced picture is show them)
    lighting.shadows = (renderer.options.mirror_shadows || (lighting.enhanced && renderer.options.mirror_enhanced)) && lighting.shadows;
    // Enhanced graphics draw the mirrors with the plain shading unless they are asked to show the
    // picture's (see Renderer::render_inner), whose light is OMSI's: a night that stays a blue dusk. The window's enhanced night is
    // far darker, and the mirrors showed the street by daylight beside it. The plain light
    // is taken down with the night (the lamps keep theirs) to the enhanced picture's level.
    // (By the sun's darkness, Envir_Brightness's ramp from +6 to -6 degrees: `night` is
    // whole at sunset already, from +10 degrees on, and rain raises it by day, and the
    // mirrors were a fifth of the window's light through the whole dusk, #432.)
    if lighting.enhanced && !omsi_cfg::flags::OMSI_MIRROR_ENHANCED.is_set() && !renderer.options.mirror_enhanced {
        let alt = lighting.sun_dir.z.clamp(-1.0, 1.0).asin().to_degrees();
        let dark = 1.0 - ((alt + 6.0) / 12.0).clamp(0.0, 1.0);
        let k = 1.0 - MIRROR_NIGHT_DIM * dark;
        lighting.ambient *= k;
        lighting.secondary *= k;
        lighting.sun_color *= k;
        lighting.sky_color *= k;
        lighting.fog_color *= k;
        // (the plain sky dome is drawn from its day/twilight/night pictures by these
        // weights, not by the light: the mirrors showed a sky three times the window's)
        for w in lighting.sky_weights.iter_mut() {
            *w *= k;
        }
    }
    // the mirrors in the picture (all of them without a view); none in it, none redrawn (a
    // static camera's screen is somewhere else than the camera: it always takes its turn)
    let seen: Vec<usize> = match pick {
        MirrorPick::Only(set) => (0..cams.len()).filter(|i| set.contains(i)).collect(),
        MirrorPick::All(skip) | MirrorPick::Turn(_, skip) => (0..cams.len())
            .filter(|i| !skip.contains(i))
            .filter(|&i| cams[i].0.fixed || on_screen.contains(&i) || view.as_ref().map(|v| mirror_in_view(cams[i].1 .0, cams[i].0.extra.unwrap_or(0.0).max(MIRROR_MIN_RADIUS), v)).unwrap_or(true))
            .collect(),
    };
    if seen.is_empty() {
        return 0;
    }
    let turn = match pick {
        MirrorPick::Turn(k, _) => Some(seen[k % seen.len()]),
        _ => None,
    };
    for (i, (c, at)) in cams.iter().enumerate() {
        if !seen.contains(&i) || turn.is_some_and(|k| k != i) {
            continue;
        }
        let Some(Some(tex)) = textures.get(i) else {
            continue;
        };
        let (eye, yaw, pitch, roll) = *at;
        let pitch = pitch.clamp(-89.0, 89.0);
        if omsi_cfg::flags::OMSI_DEBUG_MIRRORS.is_set() {
            log::info!("mirror {i}: eye {:.2},{:.2},{:.2} yaw {yaw:.1} pitch {pitch:.1} roll {roll:.2} fov {:.0} ({} of {} in view)", eye.x, eye.y, eye.z, c.fov, seen.len(), cams.len());
        }
        let cam = Camera {
            position: eye,
            yaw,
            pitch,
            roll,
            fov_deg: if c.fov > 1.0 { c.fov } else { 50.0 },
            // (Omsi.exe's reflection pass, 0x6f68a8: near 0.1 m, far 100 km. Ours cut at
            // 0.3 m and 450 m: what is close to an inside mirror's camera - a handrail, the
            // driver's head - vanished, and the street behind ended at the next junction.
            // The far end is the objects' own reach here, as in the main view.)
            near: 0.1,
            far: if renderer.options.max_obj_dist > 0.0 { renderer.options.max_obj_dist.clamp(450.0, 6000.0) } else { 3000.0 },
        };
        renderer.render_to_texture(scene, *tex, &cam, &lighting, MIRROR_ASPECT);
    }
    seen.len()
}

