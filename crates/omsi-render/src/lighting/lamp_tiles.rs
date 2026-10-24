//! Which lights get a shadow tile this frame: the part of `Renderer::prepare_lights` that
//! chooses them. The world's lamps lighting the camera's surroundings most keep their tiles
//! while they stay among the strongest, the headlamps of the player's vehicle and of the cars in
//! front of the camera are given the vehicle run, and the lamps of the player's saloon the
//! saloon's (see `lamp_slots`).

use crate::*;

/// The picture's eye, as the lights' tiles are chosen for it.
pub(crate) struct LightEye<'a> {
    /// The camera (render-origin relative).
    pub(crate) cam_rel: Vec3,
    /// The picture's view-projection (render-origin relative): headlamps that light nothing it
    /// shows take no tile. `None`: a picture with no such test.
    pub(crate) view_proj: Option<Mat4>,
    /// The player's vehicle: its box, and the rotation of its body (the heading alone without it).
    pub(crate) vehicle: Option<(&'a (DVec3, f64, [f32; 6]), Option<Mat4>)>,
}

#[cfg(test)]
impl LightEye<'_> {
    /// An eye at `cam_rel` that knows of no view and no vehicle.
    pub(crate) fn at(cam_rel: Vec3) -> LightEye<'static> {
        LightEye { cam_rel, view_proj: None, vehicle: None }
    }
}

/// How coarsely the position and the heading of a car's headlamps are taken for their map (m,
/// radians): a car driving by is a new map every half metre or so, not every frame.
const SPOT_POSITION_STEP: f32 = 0.5;
const SPOT_HEADING_STEP: f32 = 0.035;

/// What the loop over the frame's lights gathers for the tiles.
#[derive(Default)]
pub(crate) struct TileWants {
    /// The world's lamps that may have a tile: strength, identity, record in the light buffer,
    /// and the tile's description (the slot is not yet known).
    world: Vec<(f32, LampKey, u32, LampShadow)>,
    /// The headlamps of the vehicles in front of the camera.
    spots: Vec<Spot>,
    /// The player's headlamps (place, direction, reach) and their records.
    own_lamps: Vec<(Vec3, Vec3, f32)>,
    own_marked: Vec<u32>,
}

impl TileWants {
    /// One of the player's headlamps, light `idx` of the buffer.
    pub(crate) fn own_headlamp(&mut self, idx: u32, p: Vec3, l: &PointLight) {
        self.own_lamps.push((p, l.direction, l.radius));
        self.own_marked.push(idx);
    }

    /// Takes note of the light `idx` (at `p`) as one that may cast shadows, for an Enhanced
    /// frame: a lamp aimed along the road is one of a vehicle's headlamps, any other light with
    /// no direction a lamp that looks straight down (a spot that lights a door or a work place
    /// has none yet). Returns the marks its record gets now: where a frame draws no tiles - a
    /// mirror's picture - the tile the window's last frame gave the lamp, which it looks up.
    pub(crate) fn consider(&mut self, slots: &WorldSlots, eye: &LightEye, l: &PointLight, p: Vec3, idx: u32, lamp_shadows: bool) -> f32 {
        let key = lamp_key(l.position);
        let road = l.beam != 0.0 || (l.cookie.is_some() && l.direction.normalize_or_zero().z.abs() < 0.5);
        let aimed = l.direction.length_squared() > 1e-6 || l.beam != 0.0;
        // (how far it is from the camera along the ground: a camera high above the street
        // - the free camera - sees the ground below it, which the lamps round it light)
        let d = (p - eye.cam_rel).length();
        let d_ground = (p - eye.cam_rel).truncate().length();
        if !lamp_shadows {
            // (a frame that draws no tiles marks the lamps the window's last frame gave one)
            return slots.slot_of(&key).filter(|_| !aimed).map_or(0.0, |slot| LAMP_MARK * (slot + 1) as f32);
        }
        let strength = l.intensity * (l.color[0] + l.color[1] + l.color[2]);
        if road {
            let reach = l.radius.min(passes::HEADLAMP_SHADOW_REACH);
            if l.mode == LightMode::Enhanced && d < reach + LAMP_SHADOW_REACH && lights_the_view(eye, p, l.direction, reach) {
                self.spots.push(Spot { index: idx, position: p, direction: l.direction, range: l.radius, score: strength / (d * d + 25.0) });
            }
        } else if !aimed && !l.flash && d_ground < l.radius + LAMP_SHADOW_REACH {
            let core = l.core.max(l.radius * 0.125);
            let mut score = strength * core * core / (d_ground * d_ground + 25.0);
            // (a lamp that had a map keeps it until another is clearly stronger: the set
            // changing with every metre the camera moved switched shadows on and off between
            // lamps that light the view about alike, #1613)
            if slots.slot_of(&key).is_some() {
                score *= KEEP_FACTOR;
            }
            self.world.push((score, key, idx, LampShadow { position: p, range: l.radius, owner: l.shadow_owner, ..LampShadow::new(0) }));
        }
        0.0
    }

    /// The tiles of the frame: the world's lamps, the vehicles' headlamps, the saloon's lamps.
    /// Marks the lights that have one in `gpu_lights`, by their records (`LAMP_MARK`).
    /// `own`: the box of the player's vehicle, for its headlamps' map.
    pub(crate) fn tiles(mut self, scene: &mut Scene, eye: &LightEye, own: Option<passes::VehicleBox>, gpu_lights: &mut [GpuPointLight]) -> Vec<LampShadow> {
        let mut tiles: Vec<LampShadow> = Vec::new();
        let mut tiled: Vec<(u32, usize)> = Vec::new();
        self.world_tiles(scene, &mut tiles, &mut tiled);
        self.vehicle_tiles(eye.cam_rel, own, &mut tiles, &mut tiled);
        for &(idx, slot) in &tiled {
            if let Some(g) = gpu_lights.get_mut(idx as usize) {
                g.pos[3] = -(-g.pos[3]).max(0.0) - LAMP_MARK * (slot + 1) as f32;
            }
        }
        if let Some((b, rotation)) = eye.vehicle.filter(|_| !omsi_cfg::flags::OMSI_NO_CABIN_SHADOWS.is_set()) {
            let ro = scene.render_origin;
            // (the world to the frame of the vehicle's body, whose rotation the app gives)
            let to_world = rotation.unwrap_or_else(|| Mat4::from_rotation_z(-(b.1 as f32).to_radians()));
            let to_local = (Mat4::from_translation((b.0 - ro).as_vec3()) * to_world).inverse();
            cabin_tiles(scene, eye.cam_rel, passes::VehicleBox::new(b, ro), to_local, gpu_lights, &mut tiles);
        }
        tiles
    }

    /// The world's lamps keep their tiles while they stay among the strongest.
    fn world_tiles(&mut self, scene: &mut Scene, tiles: &mut Vec<LampShadow>, tiled: &mut Vec<(u32, usize)>) {
        self.world.sort_by(|a, b| b.0.total_cmp(&a.0));
        let wants: Vec<LampKey> = self.world.iter().map(|w| w.1).collect();
        let fresh = if scene.lamp_slots.is_cold() { usize::MAX } else { NEW_LAMPS_PER_FRAME };
        for (w, slot) in scene.lamp_slots.assign(&wants, fresh) {
            let (_, _, idx, mut lamp) = self.world[w];
            lamp.slot = slot;
            tiled.push((idx, slot));
            tiles.push(lamp);
        }
    }

    /// The vehicles' headlamps are given the vehicle run in order of strength, the player's pair
    /// first (a pair of lamps shares one map, which looks along the road from between them).
    fn vehicle_tiles(&mut self, cam_rel: Vec3, own: Option<passes::VehicleBox>, tiles: &mut Vec<LampShadow>, tiled: &mut Vec<(u32, usize)>) {
        let mut next = VEHICLE_SLOTS.start;
        if let Some(mut map) = own.and_then(|b| passes::headlamp_shadow(&self.own_lamps, b, cam_rel)) {
            map.slot = next;
            next += 1;
            tiled.extend(self.own_marked.iter().map(|&i| (i, map.slot)));
            tiles.push(map);
        }
        for g in group_spots(&mut self.spots) {
            if next >= VEHICLE_SLOTS.end {
                break;
            }
            let range = g.range.min(passes::HEADLAMP_SHADOW_REACH);
            // (a car's map is taken for a coarse place and heading: while the car drives by the
            // map is drawn every few frames, not every one)
            let position = (g.position / SPOT_POSITION_STEP).round() * SPOT_POSITION_STEP;
            let heading = (g.look.x.atan2(g.look.y) / SPOT_HEADING_STEP).round() * SPOT_HEADING_STEP;
            let look = Vec3::new(heading.sin(), heading.cos(), 0.0);
            tiles.push(LampShadow { slot: next, position, range, look: Some(look), hosts: true, ..LampShadow::new(0) });
            tiled.extend(g.members.iter().map(|&i| (i, next)));
            next += 1;
        }
    }
}

/// Whether a headlamp at `p` aimed along `direction` lights anything the picture shows: a car
/// behind the camera that drives away from it does not. (The beam lies in a sphere ahead of the
/// lamp, which has to touch the view.)
fn lights_the_view(eye: &LightEye, p: Vec3, direction: Vec3, reach: f32) -> bool {
    let Some(vp) = eye.view_proj else { return true };
    let level = Vec3::new(direction.x, direction.y, 0.0).normalize_or(Vec3::Y);
    sphere_in_view(&vp, p + level * reach * 0.5, reach * 0.65)
}

/// The lamps of the player's saloon, the strongest in the camera's reach (a lamp appears once
/// for each set of meshes it lights, in the slots of the interior lights: one tile for all of
/// them, named by where it hangs in the vehicle). `vbox`: the vehicle's box, `to_local` the world
/// to its frame.
fn cabin_tiles(scene: &mut Scene, cam_rel: Vec3, vbox: passes::VehicleBox, to_local: Mat4, gpu_lights: &mut [GpuPointLight], tiles: &mut Vec<LampShadow>) {
    let ro = scene.render_origin;
    let mut groups: Vec<(LampKey, Vec3, Vec3, Vec<usize>, f32)> = Vec::new();
    scene.cabin_places.resize(scene.interior_lights.len(), Vec3::NAN);
    for (i, l) in scene.interior_lights.iter().enumerate() {
        if l.intensity <= 0.0 {
            continue;
        }
        let p = (l.position - ro).as_vec3();
        if !vbox.contains(p, 0.3) {
            continue;
        }
        // (the lamp's place in the frame of a driving bus differs by the rounding of the world's
        // numbers from frame to frame, and the tile is kept while its matrix is the same: the place
        // is taken to a centimetre, and kept while it stays within one)
        let now = to_local.transform_point3(p);
        let lp = if (now - scene.cabin_places[i]).abs().max_element() < 0.01 { scene.cabin_places[i] } else { (now * 100.0).round() / 100.0 };
        scene.cabin_places[i] = lp;
        let key: LampKey = [(lp.x * 40.0).round() as i64, (lp.y * 40.0).round() as i64, (lp.z * 40.0).round() as i64];
        if let Some(g) = groups.iter_mut().find(|g| g.0 == key) {
            g.3.push(i);
            continue;
        }
        // (the lamp's strength over its distance from the camera, as the world's)
        let core = l.core.max(0.1);
        let d = (p - cam_rel).length();
        let mut score = l.intensity * (l.color[0] + l.color[1] + l.color[2]) * core * core / (d * d + 1.0);
        if scene.cabin_slots.slot_of(&key).is_some() {
            score *= KEEP_FACTOR;
        }
        groups.push((key, p, lp, vec![i], score));
    }
    groups.sort_by(|a, b| b.4.total_cmp(&a.4));
    let wants: Vec<LampKey> = groups.iter().map(|g| g.0).collect();
    let fresh = if scene.cabin_slots.is_cold() { usize::MAX } else { NEW_LAMPS_PER_FRAME };
    for (w, s) in scene.cabin_slots.assign(&wants, fresh) {
        let (_, p, lp, members, _) = &groups[w];
        let slot = CABIN_SLOTS.start + s;
        tiles.push(LampShadow {
            slot,
            position: *p,
            range: CABIN_REACH,
            near: CABIN_NEAR,
            fov: CABIN_FOV,
            cabin: Some(Cabin { vehicle: vbox, to_local, lamp: *lp }),
            index: members[0] as u32,
            ..LampShadow::new(0)
        });
        // (one record names the tile; the lamp's copies in the other sets are lit as before,
        // and shaded by `cabin_visibility`)
        if let Some(g) = gpu_lights.get_mut(members[0]) {
            g.pos[3] = -LAMP_MARK * (slot + 1) as f32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headlamp(at: Vec3, direction: Vec3) -> PointLight {
        PointLight { position: at.as_dvec3(), radius: 60.0, intensity: 400.0, color: [1.0; 3], direction, beam: 1.0, core: 1.0, mode: LightMode::Enhanced, ..Default::default() }
    }

    /// A view from the origin along the road (+y), 70 degrees wide.
    fn eye_along_the_road() -> LightEye<'static> {
        let vp = Mat4::perspective_rh(70f32.to_radians(), 1.5, 0.1, 400.0) * Mat4::look_to_rh(Vec3::new(0.0, 0.0, 1.5), Vec3::Y, Vec3::Z);
        LightEye { cam_rel: Vec3::new(0.0, 0.0, 1.5), view_proj: Some(vp), vehicle: None }
    }

    /// A car ahead that drives towards the camera lights the view, so does one behind the camera
    /// that drives the way the camera looks; a car behind the camera that drives away from it, or
    /// one far beside the road driving off it, lights nothing the picture shows.
    #[test]
    fn cars_that_light_nothing_in_view_take_no_tile() {
        let eye = eye_along_the_road();
        let slots = WorldSlots::default();
        let mut wants = TileWants::default();
        let toward = Vec3::new(0.0, -1.0, 0.0);
        let cars = [
            (Vec3::new(0.0, 40.0, 0.8), toward),
            (Vec3::new(0.0, -20.0, 0.8), -toward),
            (Vec3::new(0.0, -20.0, 0.8), toward),
            (Vec3::new(50.0, 20.0, 0.8), Vec3::X),
        ];
        for (k, (at, dir)) in cars.iter().enumerate() {
            wants.consider(&slots, &eye, &headlamp(*at, *dir), *at, k as u32, true);
        }
        let taken: Vec<u32> = wants.spots.iter().map(|s| s.index).collect();
        assert_eq!(taken, vec![0, 1], "{taken:?}");
        // an eye with no view takes every car in reach
        let mut wants = TileWants::default();
        for (k, (at, dir)) in cars.iter().enumerate() {
            wants.consider(&slots, &LightEye::at(Vec3::ZERO), &headlamp(*at, *dir), *at, k as u32, true);
        }
        assert_eq!(wants.spots.len(), 4);
    }

    #[test]
    fn a_trolley_spark_takes_no_world_tile() {
        let at = Vec3::new(0.0, 10.0, 5.0);
        let lamp = PointLight { position: at.as_dvec3(), radius: 20.0, intensity: 4.0, ..Default::default() };
        let mut wants = TileWants::default();
        wants.consider(&WorldSlots::default(), &LightEye::at(Vec3::ZERO), &lamp, at, 0, true);
        wants.consider(&WorldSlots::default(), &LightEye::at(Vec3::ZERO), &PointLight { flash: true, ..lamp }, at, 1, true);
        assert_eq!(wants.world.iter().map(|w| w.2).collect::<Vec<_>>(), vec![0]);
    }

    /// A car's map is taken for a place and a heading to a half metre and two degrees: a car
    /// that drives by a few centimetres, or turns a little, keeps its map.
    #[test]
    fn a_cars_map_keeps_its_place_while_the_car_drives_a_little() {
        let map = |x: f32, y: f32, dir: Vec3| {
            let mut wants = TileWants::default();
            let lamp = |side: f32| Spot { index: 0, position: Vec3::new(x + side, y, 0.8), direction: dir, range: 60.0, score: 4.0 };
            wants.spots = vec![lamp(-0.7), Spot { index: 1, ..lamp(0.7) }];
            let (mut tiles, mut tiled) = (Vec::new(), Vec::new());
            wants.vehicle_tiles(Vec3::ZERO, None, &mut tiles, &mut tiled);
            assert_eq!((tiles.len(), tiled.len()), (1, 2));
            (tiles[0].position, tiles[0].look.unwrap())
        };
        let ahead = Vec3::new(0.0, 1.0, -0.03);
        let a = map(0.0, 20.0, ahead);
        assert_eq!(a, map(0.1, 20.13, ahead), "a few centimetres");
        assert_eq!(a, map(0.0, 20.0, Vec3::new(0.01, 1.0, -0.03)), "half a degree");
        assert_ne!(a.0, map(0.0, 21.0, ahead).0, "a metre");
        assert_ne!(a.1, map(0.0, 20.0, Vec3::new(0.1, 1.0, -0.03)).1, "six degrees");
    }
}
