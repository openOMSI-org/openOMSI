//! Which of an object's surfaces lie on an earlier one in its own plane - the instruments
//! printed on a dashboard, the lines on a plate - and so take the draw order's depth step
//! (`draw_order_level`, `scene_clip`). Omsi.exe draws a model mesh by mesh and subset by
//! subset with LESSEQUAL, so of two such surfaces the later one is seen; everything else
//! keeps its own depth.

use glam::{Mat4, Vec3};
use std::collections::HashMap;

use crate::MeshData;

/// The most flat surfaces kept of one material slot of a mesh (the largest).
const FLATS_PER_SLOT: usize = 32;
/// How far a surface's tilt may lift its edge off another's plane and it still lie on it.
const PLANE_GAP: f32 = 2e-4;
// Allow tiny display-layer offsets without merging genuinely separate 0.1 mm surfaces.
const CENTRE_GAP: f32 = 5e-5;
/// How far off an earlier surface's plane a later one may lie, without lying in it, and still
/// take no step beyond that surface's (`near_on`). The step lifts a surface towards the eye by
/// 3e-5 a level of its distance, so a surface lying nearer than that in front of a stepped one
/// is overtaken by it far off, though it is in front - a windscreen's labels, a monitor's
/// screen: half a millimetre covers the subsequent layers of a model's glass and body while
/// leaving an instrument's glass, a few millimetres over its dial's print, to its own depth.
const NEAR_GAP: f32 = 5e-4;
/// How far apart two surfaces' boxes may be and still overlap.
const BOX_GAP: f32 = 1e-3;
/// The highest step: a surface on a surface on a surface ... is rare beyond that.
pub(crate) const MAX_LEVEL: u8 = 3;

/// A flat surface of a mesh's material slot: its plane (unit normal, its sign made the same
/// for both faces, and the plane's offset along it) and the box its triangles lie in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Flat {
    pub(crate) slot: u32,
    pub(crate) normal: Vec3,
    pub(crate) offset: f32,
    pub(crate) lo: Vec3,
    pub(crate) hi: Vec3,
}

/// The normal with the sign that makes its largest component positive, and the offset to
/// go with it (a surface and one facing the other way in the same plane are one plane).
fn canonical(n: Vec3, offset: f32) -> (Vec3, f32) {
    let a = n.abs();
    let big = if a.x >= a.y && a.x >= a.z { n.x } else if a.y >= a.z { n.y } else { n.z };
    if big < 0.0 { (-n, -offset) } else { (n, offset) }
}

/// The flat surfaces of `data`, slot by slot: its triangles grouped by plane, the largest
/// `FLATS_PER_SLOT` of each slot kept.
pub(crate) fn mesh_flats(data: &MeshData) -> Vec<Flat> {
    let mut out = Vec::new();
    for &(first, count, slot) in &data.ranges {
        // (plane key -> flat, area)
        // (a hashbrown map sized ahead, no list per triangle: a GPM street lamp of 150 000 triangles took 23 ms)
        let end = (first + count).min(data.indices.len() as u32);
        let mut planes: hashbrown::HashMap<(i32, i32, i32, i32), (Flat, f32)> = hashbrown::HashMap::with_capacity((end.saturating_sub(first) / 3) as usize);
        let mut k = first as usize;
        let at = |i: u32| data.positions.get(i as usize).copied();
        while k + 2 < end as usize {
            let idx = [data.indices[k], data.indices[k + 1], data.indices[k + 2]];
            k += 3;
            let (Some(a), Some(b), Some(c)) = (at(idx[0]), at(idx[1]), at(idx[2])) else { continue };
            let p = [a, b, c];
            let cross = (p[1] - p[0]).cross(p[2] - p[0]);
            let area = cross.length() * 0.5;
            if area < 1e-8 {
                continue;
            }
            let (n, offset) = canonical(cross / (2.0 * area), (cross / (2.0 * area)).dot(p[0]));
            let key = ((n.x * 1000.0).round() as i32, (n.y * 1000.0).round() as i32, (n.z * 1000.0).round() as i32, (offset / PLANE_GAP).round() as i32);
            let (lo, hi) = (p[0].min(p[1]).min(p[2]), p[0].max(p[1]).max(p[2]));
            let e = planes.entry(key).or_insert((Flat { slot, normal: n, offset, lo, hi }, 0.0));
            e.0.lo = e.0.lo.min(lo);
            e.0.hi = e.0.hi.max(hi);
            e.1 += area;
        }
        let mut flats: Vec<(Flat, f32)> = planes.into_values().collect();
        // (the largest picked out before they are sorted: a curved mesh has a plane per triangle)
        if flats.len() > FLATS_PER_SLOT {
            flats.select_nth_unstable_by(FLATS_PER_SLOT, |a, b| b.1.total_cmp(&a.1));
            flats.truncate(FLATS_PER_SLOT);
        }
        flats.sort_by(|a, b| b.1.total_cmp(&a.1));
        out.extend(flats.into_iter().map(|(f, _)| f));
    }
    out
}

/// The object whose meshes are being added: where it stands, the creation order its next
/// mesh would have, and its surfaces so far (in its frame) with their steps, found by normal.
#[derive(Default)]
pub(crate) struct ObjectRun {
    /// An object was begun (`begin`) and none of another has been added since.
    open: bool,
    /// Its first mesh is still to come.
    fresh: bool,
    origin: [u64; 3],
    next: u64,
    flats: Vec<(Flat, u8)>,
    by_normal: HashMap<(i32, i32, i32), Vec<usize>>,
    model: Option<u64>,
    added: usize,
    models: HashMap<u64, Vec<(usize, Vec<u8>)>>,
}

fn normal_key(n: Vec3) -> (i32, i32, i32) {
    ((n.x * 100.0).round() as i32, (n.y * 100.0).round() as i32, (n.z * 100.0).round() as i32)
}

/// `f` against `g` when the two face the same way and their boxes overlap within `margin`: how
/// far `f`'s middle lies off `g`'s plane, and how far `f`'s tilt lifts its corners off it.
fn facing(f: &Flat, g: &Flat, margin: f32) -> Option<(f32, f32)> {
    let overlap = (f.lo - Vec3::splat(margin)).cmple(g.hi).all() && (g.lo - Vec3::splat(margin)).cmple(f.hi).all();
    if !overlap || f.normal.dot(g.normal).abs() <= 0.9995 {
        return None;
    }
    let c = (f.lo + f.hi) * 0.5;
    let c = c - f.normal * (f.normal.dot(c) - f.offset);
    let tilt = f.normal.cross(g.normal).length() * (f.hi - f.lo).length() * 0.5;
    Some(((g.normal.dot(c) - g.offset).abs(), tilt))
}

/// Whether `f` lies on `g`: its middle within `CENTRE_GAP` of `g`'s plane, its tilt against
/// it lifting no edge further than `PLANE_GAP`, and their boxes overlapping.
fn lies_on(f: &Flat, g: &Flat) -> bool {
    facing(f, g, BOX_GAP).is_some_and(|(gap, tilt)| gap < CENTRE_GAP && tilt < PLANE_GAP)
}

/// Whether `f` lies just off `g`'s plane, further than `CENTRE_GAP` but nearer than
/// `NEAR_GAP`: a surface the step of `f` would pass far off, though they are really apart.
/// (Their tilt is not asked here: a flat seen nearly edge-on lifts its own edge over a small
/// angle by more than the gap, and it is the middle distance that the step's lift reaches.)
fn near_on(f: &Flat, g: &Flat) -> bool {
    facing(f, g, NEAR_GAP).is_some_and(|(gap, _)| (CENTRE_GAP..NEAR_GAP).contains(&gap))
}

impl ObjectRun {
    /// A new object (a model) begins: the meshes added from now on, one after another from
    /// one place, are its own. Without it a mesh takes no step: only the scenery objects and
    /// the vehicles mark where their models begin - a vehicle's meshes all stand at the
    /// origin, so two in a row would otherwise share one history. `model` names a model
    /// whose meshes always come in the same order and place (a vehicle set): its steps are
    /// worked out once and taken again for every further copy.
    pub(crate) fn begin(&mut self, model: Option<u64>) {
        self.open = true;
        self.fresh = true;
        self.flats.clear();
        self.by_normal.clear();
        self.model = model;
        self.added = 0;
    }

    /// The steps of the slots (`slots` of them) of mesh `mesh` with surfaces `flats`, placed
    /// with `transform` at `origin` (its key) as instance number `draw_order`: a slot lying
    /// on an earlier surface of the same object (`lies_on`) takes one step more than the
    /// highest of those, at most `MAX_LEVEL`; a slot with another surface of the object lying
    /// just off, but not in, its plane (`near_on`) takes no step beyond that surface's, so
    /// that the step's lift never carries it past a surface that is really apart from it.
    /// Any other slot 0. Outside an object begun with `begin` every slot is 0, and a mesh
    /// that is not the next one of the object (another place, a gap in the order) ends it.
    pub(crate) fn levels(&mut self, origin: [u64; 3], draw_order: u64, mesh: usize, transform: Mat4, flats: &[Flat], slots: usize) -> Vec<u8> {
        if !self.fresh && (origin != self.origin || draw_order != self.next) {
            self.open = false;
            self.model = None;
            self.flats.clear();
            self.by_normal.clear();
        }
        self.fresh = false;
        self.origin = origin;
        self.next = draw_order + 1;
        if !self.open {
            return vec![0; slots];
        }
        let known = self.model.and_then(|m| self.models.get(&m)).and_then(|l| l.get(self.added)).filter(|(id, l)| *id == mesh && l.len() == slots).map(|(_, l)| l.clone());
        self.added += 1;
        if let Some(levels) = known {
            return levels;
        }
        let normal_m = transform.inverse().transpose();
        let placed: Vec<Flat> = flats
            .iter()
            .filter_map(|f| {
                let n = normal_m.transform_vector3(f.normal).try_normalize()?;
                let p = transform.transform_point3(f.normal * f.offset);
                let (normal, offset) = canonical(n, n.dot(p));
                let corners = (0..8).map(|c| {
                    let pick = |bit: usize, lo: f32, hi: f32| if c & bit != 0 { hi } else { lo };
                    transform.transform_point3(Vec3::new(pick(1, f.lo.x, f.hi.x), pick(2, f.lo.y, f.hi.y), pick(4, f.lo.z, f.hi.z)))
                });
                let (lo, hi) = corners.fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), c| (lo.min(c), hi.max(c)));
                Some(Flat { slot: f.slot, normal, offset, lo, hi })
            })
            .collect();
        let mut levels = vec![0u8; slots];
        for slot in 0..slots {
            let mine: Vec<&Flat> = placed.iter().filter(|f| f.slot as usize == slot).collect();
            let mut level = 0u8;
            let mut under: Vec<usize> = Vec::new();
            // (the surfaces lying just off this slot's plane, with their steps)
            let mut beside: Vec<(usize, u8)> = Vec::new();
            for f in &mine {
                for (kx, ky, kz) in [normal_key(f.normal), normal_key(-f.normal)] {
                    for dx in -1..=1 { for dy in -1..=1 { for dz in -1..=1 {
                        let Some(list) = self.by_normal.get(&(kx + dx, ky + dy, kz + dz)) else { continue };
                        for &i in list {
                            let (g, gl) = &self.flats[i];
                            if lies_on(f, g) {
                                level = level.max(gl.saturating_add(1).min(MAX_LEVEL));
                                under.push(i);
                            } else if near_on(f, g) {
                                beside.push((i, *gl));
                            }
                        }
                    }}}
                }
            }
            // (a surface of the base this slot is printed on is that base: a neighbouring patch
            // of it lying a little off the plane is no reason to drop the step over it)
            beside.retain(|(i, _)| !under.contains(i));
            let beside_lo = beside.iter().map(|b| b.1).min();
            let beside_hi = beside.iter().map(|b| b.1).max();
            // (a slot more than two steps above its base is a stack - a windscreen's own patches
            // lying on each other, a lens on a display on a panel - and its lift outgrows what
            // the step is for: it takes no more than the lowest step of the surfaces lying just
            // off its plane, so that it cannot carry itself past one of them, but never less than
            // two. A slot one or two steps above its base keeps its step, which is what orders
            // a print over its base and a second print over the first, even with a pane just in
            // front; a slot on nothing takes the highest step beside it, so the step does not
            // decide that pair either.)
            let level = if level == 0 {
                beside_hi.unwrap_or(0)
            } else if level > 2 {
                level.min(beside_lo.unwrap_or(MAX_LEVEL).max(2))
            } else {
                level
            };
            levels[slot] = level;
            for f in mine {
                self.by_normal.entry(normal_key(f.normal)).or_default().push(self.flats.len());
                self.flats.push((*f, level));
            }
        }
        if let Some(m) = self.model {
            let list = self.models.entry(m).or_default();
            if list.len() + 1 == self.added {
                list.push((mesh, levels.clone()));
            }
        }
        levels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(z: f32, half: f32, slot: u32, cut_other_way: bool) -> MeshData {
        let positions = vec![Vec3::new(-half, -half, z), Vec3::new(half, -half, z), Vec3::new(half, half, z), Vec3::new(-half, half, z)];
        let indices = if cut_other_way { vec![0, 1, 3, 1, 2, 3] } else { vec![0, 1, 2, 0, 2, 3] };
        MeshData { positions, normals: vec![Vec3::Z; 4], uvs: vec![glam::Vec2::ZERO; 4], ranges: vec![(0, 6, slot)], indices, one_sided: false }
    }

    fn box_mesh(at: Vec3) -> MeshData {
        // (an unrelated mesh: a box somewhere else)
        let mut m = quad(0.0, 0.5, 0, false);
        for p in &mut m.positions {
            *p += at;
        }
        m
    }

    #[test]
    fn a_slots_triangles_in_one_plane_are_one_flat_and_broken_ones_are_left_out() {
        let mut m = quad(1.0, 0.5, 2, false);
        // (a degenerate triangle and one with an index past the vertices)
        m.indices.extend_from_slice(&[0, 0, 1, 0, 1, 9]);
        m.ranges = vec![(0, 12, 2)];
        let flats = mesh_flats(&m);
        assert_eq!(flats.len(), 1);
        let f = flats[0];
        assert_eq!((f.slot, f.normal, f.offset), (2, Vec3::Z, 1.0));
        assert_eq!((f.lo, f.hi), (Vec3::new(-0.5, -0.5, 1.0), Vec3::new(0.5, 0.5, 1.0)));
    }

    /// The bus case (#1488): its meshes all stand at one place, the dashboard and its
    /// instruments far down the list. Unrelated meshes stay at 0, the instruments lying on
    /// the dashboard take 1, a sticker on the instruments 2; capped at `MAX_LEVEL`.
    #[test]
    fn a_surface_on_an_earlier_one_of_its_object_takes_a_step() {
        let mut run = ObjectRun::default();
        run.begin(None);
        let origin = [1, 2, 3];
        let mut order = 0;
        let mut add = |run: &mut ObjectRun, m: &MeshData, t: Mat4| {
            let l = run.levels(origin, order, 0, t, &mesh_flats(m), 1);
            order += 1;
            l[0]
        };
        for k in 0..6 {
            assert_eq!(add(&mut run, &box_mesh(Vec3::new(10.0 * k as f32, 5.0, 2.0)), Mat4::IDENTITY), 0);
        }
        assert_eq!(add(&mut run, &quad(1.0, 0.4, 0, false), Mat4::IDENTITY), 0, "the dashboard");
        assert_eq!(add(&mut run, &quad(1.0, 0.1, 0, true), Mat4::IDENTITY), 1, "instruments on it");
        assert_eq!(add(&mut run, &quad(1.0, 0.05, 0, false), Mat4::IDENTITY), 2, "a sticker on those");
        assert_eq!(add(&mut run, &quad(1.0, 0.02, 0, false), Mat4::IDENTITY), 3);
        assert_eq!(add(&mut run, &quad(1.0, 0.01, 0, false), Mat4::IDENTITY), MAX_LEVEL);
        // (a millimetre over it is not in its plane: its height decides, as without the step)
        assert_eq!(add(&mut run, &quad(1.001, 0.1, 0, false), Mat4::IDENTITY), 0);
        // (in the plane but beside it: no overlap)
        assert_eq!(add(&mut run, &moved(quad(1.0, 0.1, 0, false), Vec3::new(3.0, 0.0, 0.0)), Mat4::IDENTITY), 0);
        // (placed by its transform onto the dashboard's plane, in a corner of it)
        assert_eq!(add(&mut run, &quad(0.0, 0.05, 0, false), Mat4::from_translation(Vec3::new(0.3, 0.3, 1.0))), 1);
    }

    /// Slots of one mesh: the later one on the earlier one takes the step.
    #[test]
    fn a_later_slot_on_an_earlier_one_takes_a_step() {
        let mut m = quad(0.0, 1.0, 0, false);
        let over = quad(0.0, 0.5, 1, true);
        m.positions.extend(&over.positions);
        m.normals.extend(&over.normals);
        m.uvs.extend(&over.uvs);
        m.indices.extend(over.indices.iter().map(|i| i + 4));
        m.ranges = vec![(0, 6, 0), (6, 6, 1)];
        let mut run = ObjectRun::default();
        run.begin(None);
        assert_eq!(run.levels([0; 3], 0, 0, Mat4::IDENTITY, &mesh_flats(&m), 2), vec![0, 1]);
    }

    /// A step-by-step stack is where the lift outgrows its purpose: a slot three steps up, on a
    /// windscreen's own patches or a lens on a display on a panel, takes no more than the
    /// lowest step of the surfaces lying just off its plane (two at least), so that its lift
    /// cannot carry it past one of them (#2790). A slot one or two steps above its base keeps its step - it is what
    /// orders it over the base (#1488, #1536) - and a slot on nothing takes the highest step
    /// beside it, so that the step does not decide that pair either. A print keeps the step that
    /// orders it over its base even with a pane 0.2 mm in front of it, since without the step it
    /// ties with the base and flickers against it at every distance; the stack is what is cut
    /// back, so the glass cannot carry itself past the label in front of it.
    #[test]
    fn a_print_keeps_its_step_over_its_base_beside_a_surface_just_off_its_plane() {
        // (the order-sensitive case: a pane 0.2 mm in front, added first, then a base and the
        // print in its plane - the print stays one step above its base)
        let pane = quad(0.0002, 0.5, 0, false);
        let base = quad(0.0, 1.0, 0, false);
        let print = quad(0.0, 0.5, 0, true);
        assert_eq!(levels_of(&[pane, base, print]), vec![0, 0, 1], "the print keeps its step over its base");
        // (the same pair without the pane, and with the pane 2 mm ahead - past the near window)
        assert_eq!(levels_of(&[quad(0.0, 1.0, 0, false), quad(0.0, 0.5, 0, true)]), vec![0, 1]);
        assert_eq!(levels_of(&[quad(0.002, 0.5, 0, false), quad(0.0, 1.0, 0, false), quad(0.0, 0.5, 0, true)]), vec![0, 0, 1]);
        // (a print over a base of two patches a fraction of a millimetre apart, as a curved
        // panel's: the patch is the base, not a surface to drop the step for)
        let patch = quad(0.0002, 0.5, 0, false);
        let base = quad(0.0, 1.0, 0, false);
        let print = quad(0.0, 0.8, 0, true);
        assert_eq!(levels_of(&[patch, base, print]), vec![0, 0, 1], "a patch of the base does not strip the step");
    }

    /// A stack of steps is cut back to (at least two) the surface lying just off it: a glass three steps up
    /// because its own patches lie on each other must not lift itself past the fleet number
    /// 0.2 mm in front of it (#2790).
    #[test]
    fn a_stack_of_steps_is_cut_back_to_the_surface_in_front_of_it() {
        let label = quad(0.0002, 0.2, 0, false);
        let base = quad(0.0, 1.0, 0, false);
        let glass = quad(0.0, 0.5, 0, true);
        assert_eq!(levels_of(&[label.clone(), base.clone(), glass.clone()]), vec![0, 0, 1], "one step above its base is kept, and the label stays in front by depth");
        // (a glass three steps up - a windscreen's own patches, a lens on a display on a panel)
        let deep = |z: f32, half: f32, cut: bool, slot: u32| quad(z, half, slot, cut);
        let mut run = ObjectRun::default();
        run.begin(None);
        let label = quad(0.0002, 0.2, 0, false);
        assert_eq!(run.levels([0; 3], 0, 0, Mat4::IDENTITY, &mesh_flats(&label), 1), vec![0]);
        for (k, m) in [deep(0.0, 1.0, false, 0), deep(0.0, 0.5, true, 0), deep(0.0, 0.3, false, 0), deep(0.0, 0.2, true, 0)].iter().enumerate() {
            run.levels([0; 3], k as u64 + 1, 0, Mat4::IDENTITY, &mesh_flats(m), 1);
        }
        assert!(run.flats.iter().all(|f| f.1 <= 2), "the stack is cut back to two steps");
        // (a millimetre is past what the step's lift reaches: nothing is cut back)
        let past = quad(-0.001, 0.2, 0, false);
        assert_eq!(levels_of(&[past, base, glass]), vec![0, 0, 1]);
    }

    /// A print on a dial keeps its step when the instrument's glass lies a few millimetres
    /// over it - far enough not to be a surface lying just off its plane (#1488, #1536).
    #[test]
    fn a_print_on_a_dial_keeps_its_step_under_a_glass() {
        let dial = quad(0.0, 1.0, 0, false);
        let print = quad(0.0, 0.5, 0, true);
        let glass = quad(0.003, 0.2, 0, false);
        assert_eq!(levels_of(&[dial, print, glass]), vec![0, 1, 0]);
    }

    /// A surface drawn after a stepped one, a fraction of a millimetre off its plane, takes
    /// that step too: the step must not decide between the two (the same across meshes).
    #[test]
    fn a_surface_after_a_stepped_one_a_fraction_of_a_millimetre_off_takes_its_step() {
        let base = quad(0.0, 1.0, 0, false);
        let glass = quad(0.0, 0.5, 0, true);
        let label = quad(0.0002, 0.2, 0, false);
        assert_eq!(levels_of(&[base, glass, label]), vec![0, 1, 1]);
    }
    /// Two prints over one base keep their order with a pane just in front: a second print
    /// lies on the first, so it stays above it instead of being cut back to the pane's step
    /// (behind the first print), whatever order the pane was added in.
    #[test]
    fn a_second_print_stays_over_the_first_beside_a_pane() {
        let pane = quad(0.0002, 0.5, 0, false);
        let base = quad(0.0, 1.0, 0, false);
        let a = quad(0.0, 0.6, 0, true);
        let b = quad(0.0, 0.3, 0, false);
        assert_eq!(levels_of(&[pane.clone(), base.clone(), a.clone(), b.clone()]), vec![0, 0, 1, 2]);
        // (on a base of two patches a fraction of a millimetre apart, as a curved panel's)
        let patch = quad(0.0002, 0.8, 0, false);
        assert_eq!(levels_of(&[patch, base, a, b]), vec![0, 0, 1, 2]);
    }

    /// Another object starts afresh: a new place, or a gap in the creation order, ends it;
    /// meshes added outside a begun object take no step.
    #[test]
    fn another_object_starts_at_zero() {
        let mut run = ObjectRun::default();
        let (a, b) = (quad(0.0, 1.0, 0, false), quad(0.0, 0.5, 0, true));
        assert_eq!(run.levels([1; 3], 0, 0, Mat4::IDENTITY, &mesh_flats(&a), 1), vec![0]);
        assert_eq!(run.levels([1; 3], 1, 0, Mat4::IDENTITY, &mesh_flats(&b), 1), vec![0], "no object begun");
        run.begin(None);
        assert_eq!(run.levels([1; 3], 2, 0, Mat4::IDENTITY, &mesh_flats(&a), 1), vec![0]);
        assert_eq!(run.levels([2; 3], 3, 0, Mat4::IDENTITY, &mesh_flats(&b), 1), vec![0], "another place");
        assert_eq!(run.levels([2; 3], 4, 0, Mat4::IDENTITY, &mesh_flats(&b), 1), vec![0], "ended there");
        run.begin(None);
        assert_eq!(run.levels([2; 3], 5, 0, Mat4::IDENTITY, &mesh_flats(&a), 1), vec![0]);
        assert_eq!(run.levels([2; 3], 9, 0, Mat4::IDENTITY, &mesh_flats(&b), 1), vec![0], "a gap in the order");
    }

    /// Two vehicles of one model in a row: all their meshes at the origin, created one after
    /// the other with no gap. Each begins its own object and keeps its own steps (#2603).
    #[test]
    fn models_in_a_row_at_one_place_keep_their_own_steps() {
        let layers = [quad(0.0, 1.0, 0, false), quad(0.0, 0.5, 0, true), quad(0.0, 0.2, 0, false)];
        let mut run = ObjectRun::default();
        let mut order = 0;
        for vehicle in 0..2 {
            run.begin(None);
            let levels: Vec<u8> = layers
                .iter()
                .map(|m| {
                    order += 1;
                    run.levels([0; 3], order, 0, Mat4::IDENTITY, &mesh_flats(m), 1)[0]
                })
                .collect();
            assert_eq!(levels, vec![0, 1, 2], "vehicle {vehicle}");
        }
    }

    fn tilted(centre: Vec3, half: f32, degrees: f32) -> MeshData {
        let rot = Mat4::from_translation(centre) * Mat4::from_rotation_x(degrees.to_radians());
        let mut m = quad(0.0, half, 0, false);
        for p in &mut m.positions {
            *p = rot.transform_point3(*p);
        }
        m
    }

    fn levels_of(meshes: &[MeshData]) -> Vec<u8> {
        let mut run = ObjectRun::default();
        run.begin(None);
        meshes.iter().enumerate().map(|(k, m)| run.levels([0; 3], k as u64, k, Mat4::IDENTITY, &mesh_flats(m), 1)[0]).collect()
    }

    #[test]
    fn a_button_slightly_tilted_on_a_panel_far_from_the_origin_takes_a_step() {
        let at = Vec3::new(0.3, 5.6, 1.4);
        assert_eq!(levels_of(&[tilted(at, 0.2, 30.0), tilted(at, 0.015, 30.3)]), vec![0, 1]);
    }

    #[test]
    fn a_panel_at_45_degrees_keeps_its_overlay() {
        let at = Vec3::new(0.0, 5.5, 1.2);
        assert_eq!(levels_of(&[tilted(at, 0.3, 45.0), tilted(at, 0.05, 45.0)]), vec![0, 1]);
        assert_eq!(levels_of(&[tilted(at, 0.3, 44.999), tilted(at, 0.05, 45.001)]), vec![0, 1]);
    }

    #[test]
    fn slightly_separated_display_layers_keep_their_draw_order() {
        for gap in [-4e-5, -2.5e-5, 2.5e-5, 4e-5] {
            let mut mesh = quad(1.0, 0.2, 0, false);
            let over = quad(1.0 + gap, 0.1, 1, true);
            mesh.positions.extend(&over.positions);
            mesh.normals.extend(&over.normals);
            mesh.uvs.extend(&over.uvs);
            mesh.indices.extend(over.indices.iter().map(|i| i + 4));
            mesh.ranges = vec![(0, 6, 0), (6, 6, 1)];
            let mut run = ObjectRun::default();
            run.begin(None);
            let levels = run.levels([0; 3], 0, 0, Mat4::IDENTITY, &mesh_flats(&mesh), 2);
            assert_eq!(levels, vec![0, 1], "display layer gap {gap}");
        }
    }

    #[test]
    fn a_board_drawn_after_the_text_a_tenth_of_a_millimetre_in_front_of_it_takes_no_step() {
        assert_eq!(levels_of(&[quad(1.0001, 0.2, 0, false), quad(1.0, 0.4, 0, true)]), vec![0, 0]);
        assert_eq!(levels_of(&[quad(1.0, 0.2, 0, false), quad(1.0001, 0.4, 0, true)]), vec![0, 0]);
    }

    #[test]
    fn a_surface_crossing_another_at_an_angle_takes_no_step() {
        let at = Vec3::new(0.0, 5.5, 1.2);
        assert_eq!(levels_of(&[tilted(at, 0.3, 30.0), tilted(at, 0.05, 32.0)]), vec![0, 0]);
    }

    #[test]
    fn copies_of_a_shared_model_take_the_first_copys_steps() {
        let layers = [quad(0.0, 1.0, 0, false), quad(0.0, 0.5, 0, true)];
        let apart = [quad(0.0, 1.0, 0, false), moved(quad(0.0, 0.5, 0, true), Vec3::new(9.0, 0.0, 0.0))];
        let mut run = ObjectRun::default();
        let mut order = 0;
        let mut copy = |run: &mut ObjectRun, meshes: &[MeshData], model: u64| -> Vec<u8> {
            run.begin(Some(model));
            meshes
                .iter()
                .enumerate()
                .map(|(k, m)| {
                    order += 1;
                    run.levels([0; 3], order, k, Mat4::IDENTITY, &mesh_flats(m), 1)[0]
                })
                .collect()
        };
        assert_eq!(copy(&mut run, &layers, 7), vec![0, 1]);
        assert_eq!(copy(&mut run, &apart, 7), vec![0, 1], "taken from the first copy");
        assert_eq!(copy(&mut run, &apart, 8), vec![0, 0], "another model");
    }

    fn moved(mut m: MeshData, by: Vec3) -> MeshData {
        for p in &mut m.positions {
            *p += by;
        }
        m
    }
}
