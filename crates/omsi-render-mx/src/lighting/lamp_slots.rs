//! Which lights have a shadow map, and in which tile of the atlas under the far map.
//!
//! The Enhanced picture gives every light that matters a map of its own: a street lamp or a
//! floodlight looks straight down from its head, a vehicle's headlamps look along the road,
//! the saloon's lamps look down from the roof. The lamps' tiles are kept while nothing in
//! them changes (`LampTiles`), so a street full of lamps costs a few passes when the camera
//! reaches it and none while it drives on.
//!
//! The tiles are taken in three runs. The world's lamps stand still: they keep a tile for as
//! long as they are among the strongest, against a lamp only a little stronger, and a lamp new
//! to the set takes a free one. The vehicles' headlamps move with the cars: they are given the
//! tiles of their run in order of strength each frame (a pair of lamps shares one map). The
//! saloon's lamps look down from the player's bus: see `cabin`.

use glam::{Mat4, Vec3};
use std::ops::Range;

/// How many shadow maps there are in all.
pub(crate) const LAMP_SHADOWS: usize = 56;
/// The tiles of the world's lamps (street lamps, floodlights, platform and building lights).
pub(crate) const WORLD_SLOTS: Range<usize> = 0..20;
/// The tiles of vehicles' headlamps: the player's first, then the nearest cars'.
pub(crate) const VEHICLE_SLOTS: Range<usize> = 20..28;
/// The tiles of the lamps of the saloon of the player's vehicle.
pub(crate) const CABIN_SLOTS: Range<usize> = 28..44;
/// The cube maps of lamps that shine every way (a point light that is no street lamp's head and
/// has no axis): `CUBE_BLOCKS` of them, a block of six consecutive tiles each, one for every
/// direction - +x, -x, +y, -y, +z, -z. The shaders read a lamp's block by the first tile its
/// record names (any tile from here on) and take the face the lit point lies in.
pub(crate) const CUBE_SLOTS: Range<usize> = 44..56;
pub(crate) const CUBE_BLOCKS: usize = 2;
/// A face's field of view (degrees): a little over the right angle, so that the filter's taps
/// stay inside the face they were found in.
pub(crate) const CUBE_FACE_FOV: f32 = 96.0;
/// The directions of a cube's faces, in the order of its tiles.
pub(crate) const CUBE_FACES: [Vec3; 6] = [Vec3::X, Vec3::NEG_X, Vec3::Y, Vec3::NEG_Y, Vec3::Z, Vec3::NEG_Z];
/// The shadow casters' lists of a frame: the near, far and close cascades, then one per
/// tile (each lamp's own: an embedded source leaves its own fixture out), and last the
/// precipitation's roof map (`passes::precip_map`).
pub(crate) const SHADOW_LISTS: usize = 4 + LAMP_SHADOWS;
pub(crate) const PRECIP_LIST: usize = 3 + LAMP_SHADOWS;
/// How a lamp tile's draw list names an entry: the entry's number in the low `LAMP_ENTRY_BITS`
/// bits, the tile above them (the one vertex shader of the lamps' maps, `vs_shadow_lamp`,
/// reads which matrix to draw by from there).
pub(crate) const LAMP_ENTRY_BITS: u32 = 26;

/// How many lamps new to the set of those with a shadow map are given one in a frame: each
/// is a tile to draw. (A scene's first frame has no limit.)
pub(crate) const NEW_LAMPS_PER_FRAME: usize = 6;
/// How much stronger than a lamp that has a tile another must be to take it: the set
/// changing with every metre the camera moved switched shadows on and off between lamps that
/// light the view about alike (#1613).
pub(crate) const KEEP_FACTOR: f32 = 1.6;

/// What a light's tile adds to the marks in `pos.w` of its record in the light buffer (below
/// zero, see `Renderer::prepare_lights`): 32 for the first tile, the cookie layer and the
/// player's headlamps' mark (16) being the low bits.
pub(crate) const LAMP_MARK: f32 = 32.0;

/// A lamp's identity: its place in centimetres.
pub(crate) type LampKey = [i64; 3];

pub(crate) fn lamp_key(p: glam::DVec3) -> LampKey {
    p.to_array().map(|c| (c * 100.0).round() as i64)
}

/// The lamps that have a tile among those of a run of `N`, and the tile of each.
#[derive(Clone, Debug)]
pub(crate) struct SlotSet<const N: usize> {
    pub keys: [Option<LampKey>; N],
}

impl<const N: usize> Default for SlotSet<N> {
    fn default() -> Self {
        SlotSet { keys: [None; N] }
    }
}

/// The tiles of the world's lamps as the last window frame gave them: which lamp each holds.
pub(crate) type WorldSlots = SlotSet<{ WORLD_SLOTS.end }>;
/// The tiles of the saloon's lamps, the same.
pub(crate) type CabinSlots = SlotSet<{ CABIN_SLOTS.end - CABIN_SLOTS.start }>;

impl<const N: usize> SlotSet<N> {
    pub(crate) fn slot_of(&self, key: &LampKey) -> Option<usize> {
        self.keys.iter().position(|k| k.as_ref() == Some(key))
    }

    pub(crate) fn is_cold(&self) -> bool {
        self.keys.iter().all(|k| k.is_none())
    }

    /// Give the wanted lamps (`wants`, strongest first) the tiles: a lamp that had one keeps
    /// it while it is wanted, the others lose theirs, and a lamp new to the set takes a free
    /// tile (at most `fresh` of them this frame; the rest are left for the next).
    /// Returns, for each of `wants` that has a tile, which.
    pub(crate) fn assign(&mut self, wants: &[LampKey], fresh: usize) -> Vec<(usize, usize)> {
        let wants = &wants[..wants.len().min(N)];
        for k in self.keys.iter_mut() {
            if k.as_ref().is_some_and(|key| !wants.contains(key)) {
                *k = None;
            }
        }
        let mut out = Vec::with_capacity(wants.len());
        let mut fresh = fresh;
        for (w, key) in wants.iter().enumerate() {
            if let Some(slot) = self.slot_of(key) {
                out.push((w, slot));
            } else if fresh > 0 {
                if let Some(slot) = self.keys.iter().position(|k| k.is_none()) {
                    self.keys[slot] = Some(*key);
                    out.push((w, slot));
                    fresh -= 1;
                }
            }
        }
        out
    }
}

/// The lamps that hold a cube map, one per block.
#[derive(Clone, Debug, Default)]
pub(crate) struct CubeSlots {
    pub keys: [Option<LampKey>; CUBE_BLOCKS],
}

impl CubeSlots {
    /// Give the strongest of the wanted lamps (`wants`, strongest first) the blocks: one that
    /// had a block keeps it while it is among them. Returns (index in `wants`, block).
    pub(crate) fn assign(&mut self, wants: &[LampKey]) -> Vec<(usize, usize)> {
        let top = &wants[..wants.len().min(CUBE_BLOCKS)];
        for k in self.keys.iter_mut() {
            if k.as_ref().is_some_and(|key| !top.contains(key)) {
                *k = None;
            }
        }
        let mut out = Vec::new();
        for (w, key) in top.iter().enumerate() {
            let block = match self.keys.iter().position(|k| k.as_ref() == Some(key)) {
                Some(b) => b,
                None => match self.keys.iter().position(|k| k.is_none()) {
                    Some(b) => {
                        self.keys[b] = Some(*key);
                        b
                    }
                    None => continue,
                },
            };
            out.push((w, block));
        }
        out
    }
}

/// A headlamp (a spot aimed along the road) as `group_spots` takes it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Spot {
    /// Its place in the light buffer.
    pub index: u32,
    pub position: Vec3,
    pub direction: Vec3,
    pub range: f32,
    pub score: f32,
}

/// The headlamps of one vehicle, taken as one source between them.
#[derive(Clone, Debug)]
pub(crate) struct SpotGroup {
    pub position: Vec3,
    /// The mean direction on the level (unit).
    pub look: Vec3,
    pub range: f32,
    pub score: f32,
    pub members: Vec<u32>,
}

/// How far apart (m) two lamps may stand and still be a pair of one vehicle's headlamps.
const PAIR_SPAN: f32 = 4.0;

/// Gather headlamps into the pairs they come in: lamps within `PAIR_SPAN` of each other aimed
/// the same way are one vehicle's. The groups come strongest first.
pub(crate) fn group_spots(spots: &mut [Spot]) -> Vec<SpotGroup> {
    spots.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut groups: Vec<(SpotGroup, Vec3, Vec3)> = Vec::new();
    for s in spots.iter() {
        let level = Vec3::new(s.direction.x, s.direction.y, 0.0).normalize_or_zero();
        if level == Vec3::ZERO {
            continue;
        }
        let home = groups.iter_mut().find(|(g, _, _)| (g.position - s.position).length() < PAIR_SPAN && g.look.dot(level) > 0.9);
        match home {
            Some((g, sum, dirs)) => {
                g.members.push(s.index);
                *sum += s.position;
                *dirs += level;
                let n = g.members.len() as f32;
                g.position = *sum / n;
                g.look = dirs.normalize_or(g.look);
                g.range = g.range.max(s.range);
                g.score += s.score;
            }
            None => groups.push((SpotGroup { position: s.position, look: level, range: s.range, score: s.score, members: vec![s.index] }, s.position, level)),
        }
    }
    let mut out: Vec<SpotGroup> = groups.into_iter().map(|g| g.0).collect();
    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    out
}

/// Whether a sphere may lie in the view of `view_proj` (render-origin relative): it does not
/// touch the side planes of the frustum, nor the near and far ones, whichever way depth runs.
/// A plane the projection makes none of (an infinite far one) is not tested.
pub(crate) fn sphere_in_view(view_proj: &Mat4, center: Vec3, radius: f32) -> bool {
    let r = [view_proj.row(0), view_proj.row(1), view_proj.row(2), view_proj.row(3)];
    let planes = [r[3] + r[0], r[3] - r[0], r[3] + r[1], r[3] - r[1], r[2], r[3] - r[2]];
    let c = center.extend(1.0);
    planes.iter().all(|p| {
        let n = p.truncate().length();
        n < 1e-4 || p.dot(c) / n >= -radius
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(n: i64) -> LampKey {
        [n, 0, 600]
    }

    /// A lamp keeps its tile while it stays wanted, a lamp that drops out frees its tile for the
    /// next, and no more than `fresh` lamps are given tiles in a frame.
    #[test]
    fn lamps_keep_their_tiles() {
        let mut slots = WorldSlots::default();
        assert!(slots.is_cold());
        let first = slots.assign(&[key(1), key(2), key(3)], usize::MAX);
        assert_eq!(first, vec![(0, 0), (1, 1), (2, 2)]);
        // the order of strength changes, a fourth lamp arrives: the three keep their tiles
        let second = slots.assign(&[key(3), key(4), key(1), key(2)], usize::MAX);
        assert_eq!(second, vec![(0, 2), (1, 3), (2, 0), (3, 1)]);
        // lamp 2 is no longer wanted: its tile goes to the new lamp 5
        let third = slots.assign(&[key(1), key(3), key(4), key(5)], 1);
        assert_eq!(third, vec![(0, 0), (1, 2), (2, 3), (3, 1)]);
        // two new lamps at once with room for one this frame: the stronger takes it
        let fourth = slots.assign(&[key(1), key(6), key(7)], 1);
        assert_eq!(fourth, vec![(0, 0), (1, 1)]);
        assert!(!slots.is_cold());
    }

    /// More lamps than tiles: the strongest ones have them.
    #[test]
    fn only_as_many_lamps_as_tiles() {
        let mut slots = WorldSlots::default();
        let wants: Vec<LampKey> = (0..40).map(key).collect();
        let given = slots.assign(&wants, usize::MAX);
        assert_eq!(given.len(), WORLD_SLOTS.end);
        assert!(given.iter().all(|(w, _)| *w < WORLD_SLOTS.end));
    }

    /// A sphere is in the view when it touches the frustum, however the depth runs.
    #[test]
    fn the_view_test_culls_what_lies_outside_it() {
        let look = Mat4::look_to_rh(Vec3::ZERO, Vec3::Y, Vec3::Z);
        for vp in [Mat4::perspective_rh(1.2, 1.5, 0.1, 100.0) * look, Mat4::perspective_rh(1.2, 1.5, 100.0, 0.1) * look, Mat4::perspective_infinite_reverse_rh(1.2, 1.5, 0.1) * look] {
            assert!(sphere_in_view(&vp, Vec3::new(0.0, 20.0, 0.0), 1.0));
            assert!(sphere_in_view(&vp, Vec3::new(0.0, -2.0, 0.0), 3.0), "touching the near plane");
            assert!(!sphere_in_view(&vp, Vec3::new(0.0, -30.0, 0.0), 5.0), "behind");
            assert!(!sphere_in_view(&vp, Vec3::new(60.0, 10.0, 0.0), 2.0), "far to the side");
            assert!(sphere_in_view(&vp, Vec3::new(14.0, 10.0, 0.0), 8.0), "touching the side");
        }
    }

    /// Two cube blocks, and a lamp keeps its block while it stays among the strongest.
    #[test]
    fn cube_blocks_stay_with_their_lamps() {
        let mut slots = CubeSlots::default();
        let k = |n: i64| [n, 0, 0];
        assert_eq!(slots.assign(&[k(1), k(2), k(3)]), vec![(0, 0), (1, 1)]);
        // the strongest is now another lamp: the two that had blocks keep them while wanted
        assert_eq!(slots.assign(&[k(2), k(1)]), vec![(0, 1), (1, 0)]);
        // a lamp that fell out of the top two gives its block to the newcomer
        assert_eq!(slots.assign(&[k(3), k(2)]), vec![(0, 0), (1, 1)]);
        assert!(slots.assign(&[]).is_empty() && slots.keys.iter().all(|k| k.is_none()));
        assert_eq!(CUBE_SLOTS.end - CUBE_SLOTS.start, CUBE_BLOCKS * 6);
        assert_eq!(CUBE_SLOTS.end, LAMP_SHADOWS);
    }

    /// The shaders carry the numbers of the tiles as literals: they must be these.
    #[test]
    fn the_shaders_know_how_many_tiles_there_are() {
        let n = LAMP_SHADOWS;
        for (name, text) in [
            ("camera", include_str!("../shaders/scene/01_camera.wgsl")),
            ("sky", include_str!("../sky.wgsl")),
            ("fog lamps", include_str!("../fog_lamps.wgsl")),
            ("precipitation", include_str!("../precip_common.wgsl")),
        ] {
            assert!(text.contains(&format!("lamp_view_proj: array<mat4x4<f32>, {n}>,")), "{name}");
        }
        // (a beam's shadow in the air is read from the world's, the vehicles' and the saloon's tiles;
        // a cube's faces are not looked up there)
        let m = CUBE_SLOTS.start;
        assert!(include_str!("../beam_shadow.wgsl").contains(&format!("if (k >= {m}u) {{")));
        let air = include_str!("../lamp_air.wgsl");
        assert!(air.contains(&format!("tile_k < {m}u")) && air.contains(&format!("tile_p < {m}u")));
        let lamps = include_str!("../shaders/enhanced/02_lamps.wgsl");
        assert!(lamps.contains(&format!("let saloon = k >= {}u && k < {}u;", CABIN_SLOTS.start, CABIN_SLOTS.end)));
        assert!(lamps.contains(&format!("if (k >= {}u) {{", CUBE_SLOTS.start)), "the cube blocks start where the shader says");
        assert!(include_str!("../shaders/scene/04_shadow_passes.wgsl").contains(&format!("const LAMP_ENTRY_BITS: u32 = {LAMP_ENTRY_BITS}u;")));
        assert!(lamps.contains(&format!("floor(m / {:.1})", LAMP_MARK)), "marks are decoded in steps of LAMP_MARK");
        assert!(n < (1usize << (32 - LAMP_ENTRY_BITS)), "a tile's number fits above the entry's bits");
        assert!(LAMP_MARK * (n as f32 + 1.0) < 1e6, "marks stay exact in a float");
    }

    /// Two headlamps of a car are one source, a car across the road another.
    #[test]
    fn headlamps_come_in_pairs() {
        let lamp = |index: u32, x: f32, y: f32, dir: Vec3, score: f32| Spot { index, position: Vec3::new(x, y, 0.8), direction: dir, range: 60.0, score };
        let mut spots = [
            lamp(10, -0.7, 5.0, Vec3::new(0.0, 1.0, -0.03), 4.0),
            lamp(11, 0.7, 5.0, Vec3::new(0.0, 1.0, -0.03), 4.0),
            lamp(12, 3.0, 30.0, Vec3::new(0.0, -1.0, 0.0), 2.0),
            lamp(13, 4.4, 30.0, Vec3::new(0.0, -1.0, 0.0), 2.0),
            lamp(14, 0.0, 6.5, Vec3::new(0.0, 0.0, -1.0), 9.0),
        ];
        let groups = group_spots(&mut spots[..4]);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].members.len(), 2);
        assert!((groups[0].position - Vec3::new(0.0, 5.0, 0.8)).length() < 1e-4);
        assert!(groups[0].look.dot(Vec3::Y) > 0.999);
        assert!(groups[1].look.dot(-Vec3::Y) > 0.999);
        // a lamp aimed straight down has no way along the road
        assert!(group_spots(&mut spots[4..]).is_empty());
    }
}
