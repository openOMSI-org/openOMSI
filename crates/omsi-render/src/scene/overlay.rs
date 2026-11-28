//! Which of an object's surfaces lie on an earlier one in its own plane - the instruments
//! printed on a dashboard, the lines on a plate - and so take the draw order's depth step
//! (`draw_order_level`, `scene_clip`). Omsi.exe draws a model mesh by mesh and subset by
//! subset with LESSEQUAL, so of two such surfaces the later one is seen; everything else
//! keeps its own depth.

use glam::{Mat4, Vec3};
use std::collections::HashMap;
use std::sync::Arc;

use crate::MeshData;

mod flats;
mod step;
pub(crate) use flats::{Flat, mesh_flats};

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

#[derive(Clone, Copy)]
struct DepthCap {
    value: f32,
    positive_side: bool,
    occluder: Flat,
}

/// A depth limit on each side of the slot's geometric normal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DepthCaps {
    pub(crate) positive: f32,
    pub(crate) negative: f32,
    pub(crate) shared: bool,
}

impl Default for DepthCaps {
    fn default() -> Self {
        Self {
            positive: f32::INFINITY,
            negative: f32::INFINITY,
            shared: false,
        }
    }
}

fn push_depth_cap(caps: &mut Vec<DepthCap>, candidate: DepthCap) {
    if let Some(existing) = caps.iter_mut().find(|cap| {
        cap.occluder == candidate.occluder && cap.positive_side == candidate.positive_side
    }) {
        existing.value = existing.value.min(candidate.value);
    } else {
        caps.push(candidate);
    }
}

/// The normal with the sign that makes its largest component positive, and the offset to
/// go with it (a surface and one facing the other way in the same plane are one plane).
fn canonical(n: Vec3, offset: f32) -> (Vec3, f32) {
    let a = n.abs();
    let big = if a.x >= a.y && a.x >= a.z {
        n.x
    } else if a.y >= a.z {
        n.y
    } else {
        n.z
    };
    if big < 0.0 {
        (-n, -offset)
    } else {
        (n, offset)
    }
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
    flat_caps: Vec<Arc<[DepthCap]>>,
    flat_meshes: Vec<usize>,
    by_normal: HashMap<(i32, i32, i32), Vec<usize>>,
    model: Option<u64>,
    added: usize,
    models: HashMap<u64, Vec<CachedDepth>>,
}

#[derive(Clone)]
struct CachedDepth {
    mesh: usize,
    levels: Vec<u8>,
    floors: Vec<u8>,
    caps: Vec<Arc<[DepthCap]>>,
    limits: Vec<DepthCaps>,
    alpha: Vec<crate::AlphaMode>,
}

/// Cells of 1/30, so neighbouring cells cover the normals accepted by `facing`.
fn normal_key(n: Vec3) -> (i32, i32, i32) {
    (
        (n.x * 30.0).round() as i32,
        (n.y * 30.0).round() as i32,
        (n.z * 30.0).round() as i32,
    )
}

fn boxes_overlap(a: &Flat, b: &Flat, margin: f32) -> bool {
    (a.lo - Vec3::splat(margin)).cmple(b.hi).all() && (b.lo - Vec3::splat(margin)).cmple(a.hi).all()
}

fn projected_overlap(a: &Flat, b: &Flat, minimum: f32) -> bool {
    let across = a.normal.cross(a.axis);
    [a.axis, across].into_iter().all(|axis| {
        let interval = |flat: &Flat| {
            let mut center = (flat.lo + flat.hi) * 0.5;
            center -= flat.normal * (flat.normal.dot(center) - flat.offset);
            let flat_across = flat.normal.cross(flat.axis);
            let radius = flat.half[0] * axis.dot(flat.axis).abs()
                + flat.half[1] * axis.dot(flat_across).abs();
            let middle = center.dot(axis);
            (middle - radius, middle + radius)
        };
        let (a_lo, a_hi) = interval(a);
        let (b_lo, b_hi) = interval(b);
        a_hi.min(b_hi) - a_lo.max(b_lo) > minimum
    })
}
fn projected_contains(outer: &Flat, inner: &Flat, tolerance: f32) -> bool {
    let across = outer.normal.cross(outer.axis);
    [outer.axis, across].into_iter().all(|axis| {
        let interval = |flat: &Flat| {
            let mut center = (flat.lo + flat.hi) * 0.5;
            center -= flat.normal * (flat.normal.dot(center) - flat.offset);
            let flat_across = flat.normal.cross(flat.axis);
            let radius = flat.half[0] * axis.dot(flat.axis).abs()
                + flat.half[1] * axis.dot(flat_across).abs();
            let middle = center.dot(axis);
            (middle - radius, middle + radius)
        };
        let (outer_lo, outer_hi) = interval(outer);
        let (inner_lo, inner_hi) = interval(inner);
        outer_lo <= inner_lo + tolerance && outer_hi >= inner_hi - tolerance
    })
}

/// `f` against `g` when the two face the same way and their boxes overlap within `margin`: how
/// far `f`'s middle lies off `g`'s plane, and how far `f`'s tilt lifts its corners off it.
fn facing(f: &Flat, g: &Flat, margin: f32) -> Option<(f32, f32)> {
    if !boxes_overlap(f, g, margin) || f.normal.dot(g.normal).abs() <= 0.9995 {
        return None;
    }
    let c = (f.lo + f.hi) * 0.5;
    let c = c - f.normal * (f.normal.dot(c) - f.offset);
    let axis = f.normal.cross(g.normal);
    let sin = axis.length();
    let tilt = match axis.try_normalize() {
        Some(axis) => {
            let up = f.normal.cross(axis);
            let across = f.normal.cross(f.axis);
            sin * (up.dot(f.axis).abs() * f.half[0] + up.dot(across).abs() * f.half[1])
        }
        None => 0.0,
    };
    Some(((g.normal.dot(c) - g.offset).abs(), tilt))
}

/// Whether `f` lies on `g`: its middle within `CENTRE_GAP` of `g`'s plane, its tilt against
/// it lifting no edge further than `PLANE_GAP`, and their boxes overlapping.
fn lies_on(f: &Flat, g: &Flat) -> bool {
    facing(f, g, BOX_GAP).is_some_and(|(gap, tilt)| gap < CENTRE_GAP + tilt && tilt < PLANE_GAP)
}
fn slot_is_single_flat(data: &MeshData, flat: &Flat) -> bool {
    let mut found = false;
    for &(first, count, slot) in &data.ranges {
        if slot != flat.slot {
            continue;
        }
        let end = first.saturating_add(count).min(data.indices.len() as u32) as usize;
        let mut i = first as usize;
        while i + 2 < end {
            let Some(a) = data.positions.get(data.indices[i] as usize).copied() else {
                return false;
            };
            let Some(b) = data.positions.get(data.indices[i + 1] as usize).copied() else {
                return false;
            };
            let Some(c) = data.positions.get(data.indices[i + 2] as usize).copied() else {
                return false;
            };
            i += 3;
            let points = [a, b, c];
            let Some(normal) = (b - a).cross(c - a).try_normalize() else {
                continue;
            };
            if normal.dot(flat.normal).abs() <= 0.9995
                || points
                    .iter()
                    .any(|p| (flat.normal.dot(*p) - flat.offset).abs() >= PLANE_GAP)
            {
                return false;
            }
            found = true;
        }
    }
    found
}

/// The first material slot of each fully duplicated flat support surface.
/// Mixed-geometry slots are left alone because the GPU applies depth per slot.
pub(crate) fn duplicate_support_slots(data: &MeshData, flats: &[Flat]) -> Vec<usize> {
    let overlaps = |a: &Flat, b: &Flat| {
        boxes_overlap(a, b, BOX_GAP) && lies_on(a, b) && projected_overlap(a, b, CENTRE_GAP)
    };
    let mut slots = std::collections::BTreeSet::new();
    for flat in flats {
        if !slot_is_single_flat(data, flat) {
            continue;
        }
        let has_earlier = flats
            .iter()
            .any(|other| other.slot < flat.slot && overlaps(other, flat));
        let has_later = flats.iter().any(|other| {
            other.slot > flat.slot
                && overlaps(flat, other)
                && projected_contains(other, flat, CENTRE_GAP)
        });
        if !has_earlier && has_later {
            slots.insert(flat.slot as usize);
        }
    }
    slots.into_iter().collect()
}

/// Whether `f` lies just off `g`'s plane, further than `CENTRE_GAP` but nearer than
/// `NEAR_GAP`: a surface the step of `f` would pass far off, though they are really apart.
/// (Their tilt is not asked here: a flat seen nearly edge-on lifts its own edge over a small
/// angle by more than the gap, and it is the middle distance that the step's lift reaches.)
fn near_gap_side(f: &Flat, g: &Flat) -> Option<(f32, bool)> {
    let (centre_gap, radius, positive_side) = plane_separation(f, g)?;
    let nearest_gap = centre_gap.abs() - radius;
    (CENTRE_GAP..NEAR_GAP)
        .contains(&nearest_gap)
        .then_some((nearest_gap, positive_side))
}

fn plane_separation(f: &Flat, g: &Flat) -> Option<(f32, f32, bool)> {
    facing(f, g, NEAR_GAP)?;
    let centre = (f.lo + f.hi) * 0.5;
    let centre = centre - f.normal * (f.normal.dot(centre) - f.offset);
    let across = f.normal.cross(f.axis);
    let centre_gap = g.normal.dot(centre) - g.offset;
    let radius = f.half[0] * g.normal.dot(f.axis).abs() + f.half[1] * g.normal.dot(across).abs();
    let orientation = if f.normal.dot(g.normal) < 0.0 {
        -1.0
    } else {
        1.0
    };
    let candidate_side = -(orientation * centre_gap);
    Some((centre_gap, radius, candidate_side * f.orientation >= 0.0))
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
        self.flat_caps.clear();
        self.flat_meshes.clear();
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
    #[cfg(test)]
    pub(crate) fn levels(
        &mut self,
        origin: [u64; 3],
        draw_order: u64,
        mesh: usize,
        transform: Mat4,
        flats: &[Flat],
        slots: usize,
    ) -> Vec<u8> {
        self.levels_with_caps(origin, draw_order, mesh, transform, flats, slots)
            .0
    }

    pub(crate) fn levels_with_caps(
        &mut self,
        origin: [u64; 3],
        draw_order: u64,
        mesh: usize,
        transform: Mat4,
        flats: &[Flat],
        slots: usize,
    ) -> (Vec<u8>, Vec<DepthCaps>, Vec<u8>) {
        self.levels_for_materials_inner(
            origin,
            draw_order,
            mesh,
            transform,
            flats,
            slots,
            &[],
            None,
        )
    }

    pub(crate) fn levels_for_materials_with_depth_planes(
        &mut self,
        origin: [u64; 3],
        draw_order: u64,
        mesh: usize,
        transform: Mat4,
        flats: &[Flat],
        slots: usize,
        alpha: &[crate::AlphaMode],
        depth_normals: &[Vec3],
    ) -> (Vec<u8>, Vec<DepthCaps>, Vec<u8>) {
        self.levels_for_materials_inner(
            origin,
            draw_order,
            mesh,
            transform,
            flats,
            slots,
            alpha,
            Some(depth_normals),
        )
    }

    fn levels_for_materials_inner(
        &mut self,
        origin: [u64; 3],
        draw_order: u64,
        mesh: usize,
        transform: Mat4,
        flats: &[Flat],
        slots: usize,
        alpha: &[crate::AlphaMode],
        depth_normals: Option<&[Vec3]>,
    ) -> (Vec<u8>, Vec<DepthCaps>, Vec<u8>) {
        if !self.fresh && (origin != self.origin || draw_order != self.next) {
            self.open = false;
            self.model = None;
            self.flats.clear();
            self.flat_caps.clear();
            self.flat_meshes.clear();
            self.by_normal.clear();
        }
        self.fresh = false;
        self.origin = origin;
        self.next = draw_order + 1;
        if !self.open {
            return (
                vec![0; slots],
                vec![DepthCaps::default(); slots],
                vec![0; slots],
            );
        }
        let known = self
            .model
            .and_then(|model| self.models.get(&model))
            .and_then(|list| list.get(self.added))
            .filter(|entry| {
                entry.mesh == mesh && entry.levels.len() == slots && entry.alpha == alpha
            })
            .cloned();
        if let Some(list) = self.model.and_then(|model| self.models.get(&model))
            && known.is_none()
            && self.added < list.len()
        {
            self.model = None;
        }
        self.added += 1;
        if let Some(CachedDepth {
            levels,
            floors,
            caps,
            limits,
            ..
        }) = known
        {
            for flat in flats.iter().filter(|flat| (flat.slot as usize) < slots) {
                let slot = flat.slot as usize;
                self.by_normal
                    .entry(normal_key(flat.normal))
                    .or_default()
                    .push(self.flats.len());
                self.flats.push((*flat, levels[slot]));
                self.flat_caps.push(Arc::clone(&caps[slot]));
                self.flat_meshes.push(mesh);
            }
            return (levels, limits, floors);
        }
        let normal_m = transform.inverse().transpose();
        let placed: Vec<Flat> = flats
            .iter()
            .filter_map(|f| {
                let n = normal_m.transform_vector3(f.normal).try_normalize()?;
                let source_normal = normal_m
                    .transform_vector3(f.normal * f.orientation)
                    .try_normalize()?;
                let p = transform.transform_point3(f.normal * f.offset);
                let (normal, offset) = canonical(n, n.dot(p));
                let corners = (0..8).map(|c| {
                    let pick = |bit: usize, lo: f32, hi: f32| if c & bit != 0 { hi } else { lo };
                    transform.transform_point3(Vec3::new(
                        pick(1, f.lo.x, f.hi.x),
                        pick(2, f.lo.y, f.hi.y),
                        pick(4, f.lo.z, f.hi.z),
                    ))
                });
                let (lo, hi) = corners.fold(
                    (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
                    |(lo, hi), c| (lo.min(c), hi.max(c)),
                );
                let (axis, across) = (
                    transform.transform_vector3(f.axis),
                    transform.transform_vector3(f.normal.cross(f.axis)),
                );
                let half = [f.half[0] * axis.length(), f.half[1] * across.length()];
                Some(Flat {
                    slot: f.slot,
                    normal,
                    orientation: if normal.dot(source_normal) < 0.0 {
                        -1.0
                    } else {
                        1.0
                    },
                    offset,
                    lo,
                    hi,
                    axis: axis.try_normalize().unwrap_or(f.axis),
                    half,
                })
            })
            .collect();
        let mut levels = vec![0u8; slots];
        let mut floors = vec![0u8; slots];
        let mut depth_caps = vec![DepthCaps::default(); slots];
        let mut slot_caps: Vec<Arc<[DepthCap]>> = vec![Arc::from([]); slots];
        let first_flat = self.flats.len();
        let mut background_caps = HashMap::<usize, Vec<DepthCap>>::new();
        for slot in 0..slots {
            let mine: Vec<&Flat> = placed.iter().filter(|f| f.slot as usize == slot).collect();
            if depth_normals
                .is_some_and(|normals| normals.get(slot).is_none_or(|normal| *normal == Vec3::ZERO))
                && !self.repeats_stepped_planes(&mine, first_flat, alpha.get(slot))
            {
                self.remember_unstepped_flats(&mine, mesh);
                continue;
            }
            let mut level = 0u8;
            let mut under: Vec<usize> = Vec::new();
            // (the surfaces lying just off this slot's plane, with their steps)
            let mut beside: Vec<(usize, u8, f32, bool, Flat)> = Vec::new();
            for f in &mine {
                for (kx, ky, kz) in [normal_key(f.normal), normal_key(-f.normal)] {
                    for dx in -1..=1 {
                        for dy in -1..=1 {
                            for dz in -1..=1 {
                                let Some(list) = self.by_normal.get(&(kx + dx, ky + dy, kz + dz))
                                else {
                                    continue;
                                };
                                for &i in list {
                                    let (g, gl) = &self.flats[i];
                                    if lies_on(f, g) {
                                        level = level.max(gl.saturating_add(1).min(MAX_LEVEL));
                                        under.push(i);
                                    } else if let Some((gap, positive_side)) = near_gap_side(f, g) {
                                        beside.push((i, *gl, gap, positive_side, **f));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            under.sort_unstable();
            under.dedup();
            // (a surface of the base this slot is printed on is that base: a neighbouring patch
            // of it lying a little off the plane is no reason to drop the step over it)
            beside.retain(|(i, _, _, _, _)| !under.contains(i));
            let beside_lo = beside.iter().map(|b| b.1).min();
            let beside_hi = beside.iter().map(|b| b.1).max();
            let base_level = under.iter().map(|&i| self.flats[i].1).min();
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
            let floor = under.iter().map(|&i| self.flats[i].1).max().unwrap_or(0);
            floors[slot] = floor;
            let level = level.max(floor);
            let mut cap_constraints: Vec<DepthCap> = Vec::new();
            for (near, _, gap, positive_side, _) in
                beside.iter().filter(|(near, near_level, _, _, target)| {
                    under.iter().any(|&base| {
                        let overlaps_base =
                            boxes_overlap(&self.flats[*near].0, &self.flats[base].0, BOX_GAP);
                        let same_mesh_occluder = *near_level <= 1
                            && Some(self.flats[base].1) == base_level
                            && self.flats[*near].0.slot != self.flats[base].0.slot
                            && projected_overlap(&self.flats[*near].0, target, CENTRE_GAP)
                            && projected_overlap(
                                &self.flats[*near].0,
                                &self.flats[base].0,
                                CENTRE_GAP,
                            );
                        overlaps_base && (self.flat_meshes[*near] != mesh || same_mesh_occluder)
                    })
                })
            {
                push_depth_cap(
                    &mut cap_constraints,
                    DepthCap {
                        value: gap * 0.9,
                        positive_side: *positive_side,
                        occluder: self.flats[*near].0,
                    },
                );
            }
            for &base in &under {
                for inherited in self.flat_caps[base].iter() {
                    let stays_on_side = mine.iter().any(|target| {
                        plane_separation(target, &inherited.occluder).is_some_and(
                            |(centre_gap, radius, positive_side)| {
                                positive_side == inherited.positive_side
                                    && centre_gap.abs() > radius
                                    && projected_overlap(&inherited.occluder, target, CENTRE_GAP)
                            },
                        )
                    });
                    if stays_on_side {
                        push_depth_cap(&mut cap_constraints, *inherited);
                    }
                }
            }
            let mut limits = depth_cap_limits(&cap_constraints);
            if level >= 2 && !cap_constraints.is_empty() {
                for &i in under.iter().filter(|&&i| i >= first_flat) {
                    let (base, base_level) = self.flats[i];
                    let base = &mut depth_caps[base.slot as usize];
                    base.shared |= base_level == 1 && *base == limits;
                }
            }
            // 只降低同階的共面挖空背景，避免跨行文字把上限傳給其他文字材質。
            if floor == MAX_LEVEL
                && alpha.get(slot) == Some(&crate::AlphaMode::Blend)
                && !cap_constraints.is_empty()
            {
                for &i in under.iter().filter(|&&i| i >= first_flat) {
                    let (base, base_level) = self.flats[i];
                    if base_level == level
                        && alpha.get(base.slot as usize) == Some(&crate::AlphaMode::Test)
                        && mine.iter().any(|&&flat| {
                            Flat {
                                slot: base.slot,
                                ..flat
                            } == base
                        })
                    {
                        limits.shared = true;
                        for &cap in &cap_constraints {
                            push_depth_cap(
                                background_caps.entry(base.slot as usize).or_default(),
                                cap,
                            );
                        }
                    }
                }
            }
            depth_caps[slot] = limits;
            let cap_constraints: Arc<[DepthCap]> = cap_constraints.into();
            slot_caps[slot] = Arc::clone(&cap_constraints);
            levels[slot] = level;
            for f in mine {
                self.by_normal
                    .entry(normal_key(f.normal))
                    .or_default()
                    .push(self.flats.len());
                self.flats.push((*f, level));
                self.flat_caps.push(Arc::clone(&cap_constraints));
                self.flat_meshes.push(mesh);
            }
        }
        if !background_caps.is_empty() {
            for (slot, mut caps) in background_caps {
                for &cap in slot_caps[slot].iter() {
                    push_depth_cap(&mut caps, cap);
                }
                depth_caps[slot] = depth_cap_limits(&caps);
                depth_caps[slot].shared = true;
                slot_caps[slot] = caps.into();
            }
            for (flat, caps) in self.flats[first_flat..]
                .iter()
                .zip(&mut self.flat_caps[first_flat..])
            {
                *caps = Arc::clone(&slot_caps[flat.0.slot as usize]);
            }
        }
        if let Some(m) = self.model {
            let list = self.models.entry(m).or_default();
            if list.len() + 1 == self.added {
                list.push(CachedDepth {
                    mesh,
                    levels: levels.clone(),
                    floors: floors.clone(),
                    caps: slot_caps,
                    limits: depth_caps.clone(),
                    alpha: alpha.to_vec(),
                });
            }
        }
        (levels, depth_caps, floors)
    }
}

fn depth_cap_limits(caps: &[DepthCap]) -> DepthCaps {
    caps.iter().fold(DepthCaps::default(), |mut limits, cap| {
        let limit = if cap.positive_side {
            &mut limits.positive
        } else {
            &mut limits.negative
        };
        *limit = limit.min(cap.value);
        limits
    })
}

pub(crate) fn duplicate_support_level(current: u8, floor: u8) -> u8 {
    if floor > 1 { current } else { current.min(1) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(z: f32, half: f32, slot: u32, cut_other_way: bool) -> MeshData {
        let positions = vec![
            Vec3::new(-half, -half, z),
            Vec3::new(half, -half, z),
            Vec3::new(half, half, z),
            Vec3::new(-half, half, z),
        ];
        let indices = if cut_other_way {
            vec![0, 1, 3, 1, 2, 3]
        } else {
            vec![0, 1, 2, 0, 2, 3]
        };
        MeshData {
            wire_wind: Vec::new(),
            positions,
            normals: vec![Vec3::Z; 4],
            uvs: vec![glam::Vec2::ZERO; 4],
            ranges: vec![(0, 6, slot)],
            indices,
            one_sided: false,
        }
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
        assert_eq!(
            (f.lo, f.hi),
            (Vec3::new(-0.5, -0.5, 1.0), Vec3::new(0.5, 0.5, 1.0))
        );
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
            assert_eq!(
                add(
                    &mut run,
                    &box_mesh(Vec3::new(10.0 * k as f32, 5.0, 2.0)),
                    Mat4::IDENTITY
                ),
                0
            );
        }
        assert_eq!(
            add(&mut run, &quad(1.0, 0.4, 0, false), Mat4::IDENTITY),
            0,
            "the dashboard"
        );
        assert_eq!(
            add(&mut run, &quad(1.0, 0.1, 0, true), Mat4::IDENTITY),
            1,
            "instruments on it"
        );
        assert_eq!(
            add(&mut run, &quad(1.0, 0.05, 0, false), Mat4::IDENTITY),
            2,
            "a sticker on those"
        );
        assert_eq!(add(&mut run, &quad(1.0, 0.02, 0, false), Mat4::IDENTITY), 3);
        assert_eq!(
            add(&mut run, &quad(1.0, 0.01, 0, false), Mat4::IDENTITY),
            MAX_LEVEL
        );
        // (a millimetre over it is not in its plane: its height decides, as without the step)
        assert_eq!(
            add(&mut run, &quad(1.001, 0.1, 0, false), Mat4::IDENTITY),
            0
        );
        // (in the plane but beside it: no overlap)
        assert_eq!(
            add(
                &mut run,
                &moved(quad(1.0, 0.1, 0, false), Vec3::new(3.0, 0.0, 0.0)),
                Mat4::IDENTITY
            ),
            0
        );
        // (placed by its transform onto the dashboard's plane, in a corner of it)
        assert_eq!(
            add(
                &mut run,
                &quad(0.0, 0.05, 0, false),
                Mat4::from_translation(Vec3::new(0.3, 0.3, 1.0))
            ),
            1
        );
    }

    #[test]
    fn a_nonplanar_slot_does_not_step_over_an_earlier_coplanar_label() {
        let label = quad(0.0, 0.2, 0, false);
        let mixed = joined_mesh([quad(0.0, 0.4, 0, false), tilted(Vec3::ZERO, 0.2, 15.0)]);
        let depth_normals = crate::scene::depth_plane::normals(&mixed);
        assert_eq!(depth_normals, [Vec3::ZERO]);

        let mut run = ObjectRun::default();
        run.begin(None);
        run.levels([0; 3], 0, 0, Mat4::IDENTITY, &mesh_flats(&label), 1);
        let (levels, _, _) = run.levels_for_materials_with_depth_planes(
            [0; 3],
            1,
            1,
            Mat4::IDENTITY,
            &mesh_flats(&mixed),
            1,
            &[crate::AlphaMode::Opaque],
            &depth_normals,
        );

        assert_eq!(levels, [0]);
    }

    #[test]
    fn a_planar_slot_keeps_its_draw_order_depth_step() {
        let support = quad(0.0, 0.4, 0, false);
        let label = quad(0.0, 0.2, 0, true);
        let support_normals = crate::scene::depth_plane::normals(&support);
        let label_normals = crate::scene::depth_plane::normals(&label);
        assert_eq!(support_normals, [Vec3::Z]);
        assert_eq!(label_normals, [Vec3::Z]);

        let mut run = ObjectRun::default();
        run.begin(None);
        run.levels_for_materials_with_depth_planes(
            [0; 3],
            0,
            0,
            Mat4::IDENTITY,
            &mesh_flats(&support),
            1,
            &[crate::AlphaMode::Opaque],
            &support_normals,
        );
        let (levels, _, _) = run.levels_for_materials_with_depth_planes(
            [0; 3],
            1,
            1,
            Mat4::IDENTITY,
            &mesh_flats(&label),
            1,
            &[crate::AlphaMode::Opaque],
            &label_normals,
        );

        assert_eq!(levels, [1]);
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
        assert_eq!(
            run.levels([0; 3], 0, 0, Mat4::IDENTITY, &mesh_flats(&m), 2),
            vec![0, 1]
        );
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
        assert_eq!(
            levels_of(&[pane, base, print]),
            vec![0, 0, 1],
            "the print keeps its step over its base"
        );
        // (the same pair without the pane, and with the pane 2 mm ahead - past the near window)
        assert_eq!(
            levels_of(&[quad(0.0, 1.0, 0, false), quad(0.0, 0.5, 0, true)]),
            vec![0, 1]
        );
        assert_eq!(
            levels_of(&[
                quad(0.002, 0.5, 0, false),
                quad(0.0, 1.0, 0, false),
                quad(0.0, 0.5, 0, true)
            ]),
            vec![0, 0, 1]
        );
        // (a print over a base of two patches a fraction of a millimetre apart, as a curved
        // panel's: the patch is the base, not a surface to drop the step for)
        let patch = quad(0.0002, 0.5, 0, false);
        let base = quad(0.0, 1.0, 0, false);
        let print = quad(0.0, 0.8, 0, true);
        assert_eq!(
            levels_of(&[patch, base, print]),
            vec![0, 0, 1],
            "a patch of the base does not strip the step"
        );
    }

    /// A stack of steps is cut back to (at least two) the surface lying just off it: a glass three steps up
    /// because its own patches lie on each other must not lift itself past the fleet number
    /// 0.2 mm in front of it (#2790).
    #[test]
    fn a_stack_of_steps_is_cut_back_to_the_surface_in_front_of_it() {
        let label = quad(0.0002, 0.2, 0, false);
        let base = quad(0.0, 1.0, 0, false);
        let glass = quad(0.0, 0.5, 0, true);
        assert_eq!(
            levels_of(&[label.clone(), base.clone(), glass.clone()]),
            vec![0, 0, 1],
            "one step above its base is kept, and the label stays in front by depth"
        );
        // (a glass three steps up - a windscreen's own patches, a lens on a display on a panel)
        let deep = |z: f32, half: f32, cut: bool, slot: u32| quad(z, half, slot, cut);
        let mut run = ObjectRun::default();
        run.begin(None);
        let label = quad(0.0002, 0.2, 0, false);
        assert_eq!(
            run.levels([0; 3], 0, 0, Mat4::IDENTITY, &mesh_flats(&label), 1),
            vec![0]
        );
        for (k, m) in [
            deep(0.0, 1.0, false, 0),
            deep(0.0, 0.5, true, 0),
            deep(0.0, 0.3, false, 0),
            deep(0.0, 0.2, true, 0),
        ]
        .iter()
        .enumerate()
        {
            run.levels([0; 3], k as u64 + 1, 0, Mat4::IDENTITY, &mesh_flats(m), 1);
        }
        assert!(
            run.flats.iter().all(|f| f.1 <= 2),
            "the stack is cut back to two steps"
        );
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
        assert_eq!(
            levels_of(&[pane.clone(), base.clone(), a.clone(), b.clone()]),
            vec![0, 0, 1, 2]
        );
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
        assert_eq!(
            run.levels([1; 3], 0, 0, Mat4::IDENTITY, &mesh_flats(&a), 1),
            vec![0]
        );
        assert_eq!(
            run.levels([1; 3], 1, 0, Mat4::IDENTITY, &mesh_flats(&b), 1),
            vec![0],
            "no object begun"
        );
        run.begin(None);
        assert_eq!(
            run.levels([1; 3], 2, 0, Mat4::IDENTITY, &mesh_flats(&a), 1),
            vec![0]
        );
        assert_eq!(
            run.levels([2; 3], 3, 0, Mat4::IDENTITY, &mesh_flats(&b), 1),
            vec![0],
            "another place"
        );
        assert_eq!(
            run.levels([2; 3], 4, 0, Mat4::IDENTITY, &mesh_flats(&b), 1),
            vec![0],
            "ended there"
        );
        run.begin(None);
        assert_eq!(
            run.levels([2; 3], 5, 0, Mat4::IDENTITY, &mesh_flats(&a), 1),
            vec![0]
        );
        assert_eq!(
            run.levels([2; 3], 9, 0, Mat4::IDENTITY, &mesh_flats(&b), 1),
            vec![0],
            "a gap in the order"
        );
    }

    /// Two vehicles of one model in a row: all their meshes at the origin, created one after
    /// the other with no gap. Each begins its own object and keeps its own steps (#2603).
    #[test]
    fn models_in_a_row_at_one_place_keep_their_own_steps() {
        let layers = [
            quad(0.0, 1.0, 0, false),
            quad(0.0, 0.5, 0, true),
            quad(0.0, 0.2, 0, false),
        ];
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
        meshes
            .iter()
            .enumerate()
            .map(|(k, m)| run.levels([0; 3], k as u64, k, Mat4::IDENTITY, &mesh_flats(m), 1)[0])
            .collect()
    }

    #[test]
    fn a_button_slightly_tilted_on_a_panel_far_from_the_origin_takes_a_step() {
        let at = Vec3::new(0.3, 5.6, 1.4);
        assert_eq!(
            levels_of(&[tilted(at, 0.2, 30.0), tilted(at, 0.015, 30.3)]),
            vec![0, 1]
        );
    }

    #[test]
    fn a_panel_at_45_degrees_keeps_its_overlay() {
        let at = Vec3::new(0.0, 5.5, 1.2);
        assert_eq!(
            levels_of(&[tilted(at, 0.3, 45.0), tilted(at, 0.05, 45.0)]),
            vec![0, 1]
        );
        assert_eq!(
            levels_of(&[tilted(at, 0.3, 44.999), tilted(at, 0.05, 45.001)]),
            vec![0, 1]
        );
    }

    #[test]
    fn a_later_print_never_drops_below_the_stack_it_covers() {
        let patch = moved(quad(0.0002, 0.1, 0, false), Vec3::new(0.8, 0.0, 0.0));
        let levels = levels_of(&[
            patch,
            quad(0.0, 0.3, 0, false),
            quad(0.0, 0.25, 0, true),
            quad(0.0, 0.2, 0, false),
            quad(0.0, 0.15, 0, true),
            quad(0.0, 1.0, 0, false),
        ]);
        assert_eq!(&levels[..5], &[0, 0, 1, 2, 3]);
        assert_eq!(
            levels[5], 3,
            "a nearby patch cannot lower the print below its base"
        );
    }

    #[test]
    fn a_text_strip_tilted_across_its_tile_takes_a_step() {
        let slant = Mat4::from_translation(Vec3::new(0.06, 1.49, 5.3))
            * Mat4::from_rotation_z(-0.9)
            * Mat4::from_rotation_x(0.3);
        let make_mesh = |raised_top: f32| {
            let mut mesh = quad(0.0, 0.1, 0, false);
            for point in &mut mesh.positions {
                point.y *= 0.07;
            }
            let strip = [
                Vec3::new(-0.1, -0.0037, 0.0),
                Vec3::new(0.1, -0.0037, 0.0),
                Vec3::new(0.1, 0.0037, raised_top),
                Vec3::new(-0.1, 0.0037, raised_top),
            ];
            let base = mesh.positions.len() as u32;
            mesh.positions.extend(strip);
            mesh.normals.extend([Vec3::Z; 4]);
            mesh.uvs.extend([glam::Vec2::ZERO; 4]);
            mesh.indices
                .extend([0, 1, 2, 0, 2, 3].map(|index| index + base));
            mesh.ranges = vec![(0, 6, 0), (6, 6, 1)];
            for point in &mut mesh.positions {
                *point = slant.transform_point3(*point);
            }
            mesh
        };
        let get_levels = |mesh: &MeshData| {
            let mut run = ObjectRun::default();
            run.begin(None);
            run.levels([0; 3], 0, 0, Mat4::IDENTITY, &mesh_flats(mesh), 2)
        };
        assert_eq!(get_levels(&make_mesh(1.4e-4)), vec![0, 1]);

        let mut detached = make_mesh(0.0);
        let normal = slant.transform_vector3(Vec3::Z);
        for point in &mut detached.positions[4..] {
            *point += normal * 1.0e-4;
        }
        assert_eq!(get_levels(&detached), vec![0, 0]);
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

    /// A slightly tilted surface standing 0.1 mm off its base at its nearest edge is not on
    /// the base: stepping it can carry it past geometry in front at a distance.
    #[test]
    fn a_slightly_tilted_surface_a_tenth_of_a_millimetre_off_at_its_nearest_edge_is_not_on_the_base()
     {
        for lift in [1.0e-4_f32, -1.0e-4] {
            let mut mesh = quad(1.0, 0.2, 0, false);
            let mut over = quad(1.0, 0.1, 1, true);
            let slope = 0.2 * 1.0_f32.to_radians();
            over.positions[0].z += lift;
            over.positions[3].z += lift;
            over.positions[1].z += lift + slope;
            over.positions[2].z += lift + slope;
            mesh.positions.extend(&over.positions);
            mesh.normals.extend(&over.normals);
            mesh.uvs.extend(&over.uvs);
            mesh.indices.extend(over.indices.iter().map(|i| i + 4));
            mesh.ranges = vec![(0, 6, 0), (6, 6, 1)];
            let mut run = ObjectRun::default();
            run.begin(None);
            let levels = run.levels([0; 3], 0, 0, Mat4::IDENTITY, &mesh_flats(&mesh), 2);
            assert_eq!(levels, [0, 0], "lift {lift}");
        }
    }

    #[test]
    fn a_board_drawn_after_the_text_a_tenth_of_a_millimetre_in_front_of_it_takes_no_step() {
        assert_eq!(
            levels_of(&[quad(1.0001, 0.2, 0, false), quad(1.0, 0.4, 0, true)]),
            vec![0, 0]
        );
        assert_eq!(
            levels_of(&[quad(1.0, 0.2, 0, false), quad(1.0001, 0.4, 0, true)]),
            vec![0, 0]
        );
    }

    #[test]
    fn a_surface_crossing_another_at_an_angle_takes_no_step() {
        let at = Vec3::new(0.0, 5.5, 1.2);
        assert_eq!(
            levels_of(&[tilted(at, 0.3, 30.0), tilted(at, 0.05, 32.0)]),
            vec![0, 0]
        );
    }

    #[test]
    fn copies_of_a_shared_model_take_the_first_copys_steps() {
        let layers = [quad(0.0, 1.0, 0, false), quad(0.0, 0.5, 0, true)];
        let apart = [
            quad(0.0, 1.0, 0, false),
            moved(quad(0.0, 0.5, 0, true), Vec3::new(9.0, 0.0, 0.0)),
        ];
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
        assert_eq!(
            copy(&mut run, &apart, 7),
            vec![0, 1],
            "taken from the first copy"
        );
        assert_eq!(copy(&mut run, &apart, 8), vec![0, 0], "another model");
    }

    fn moved(mut m: MeshData, by: Vec3) -> MeshData {
        for p in &mut m.positions {
            *p += by;
        }
        m
    }

    fn joined_mesh(parts: impl IntoIterator<Item = MeshData>) -> MeshData {
        let mut mesh = MeshData::default();
        for part in parts {
            let base = mesh.positions.len() as u32;
            let first = mesh.indices.len() as u32;
            mesh.positions.extend(part.positions);
            mesh.normals.extend(part.normals);
            mesh.uvs.extend(part.uvs);
            mesh.indices
                .extend(part.indices.into_iter().map(|i| i + base));
            mesh.ranges.extend(
                part.ranges
                    .into_iter()
                    .map(|(start, count, slot)| (first + start, count, slot)),
            );
        }
        mesh
    }

    #[test]
    fn a_stepped_front_label_caps_a_later_print_in_the_same_mesh() {
        let mesh = joined_mesh([
            quad(0.0, 0.4, 0, false),
            quad(0.0, 0.3, 1, true),
            quad(0.0002, 0.1, 2, false),
            quad(0.0, 0.25, 3, false),
            quad(0.0, 0.2, 4, true),
        ]);
        let mut run = ObjectRun::default();
        run.begin(None);
        let (levels, caps, _) =
            run.levels_with_caps([0; 3], 0, 0, Mat4::IDENTITY, &mesh_flats(&mesh), 5);
        assert_eq!(levels, [0, 1, 1, 2, 2]);
        assert!((caps[4].positive - 0.00018).abs() < 1e-7);
        assert!(caps[4].negative.is_infinite());
    }

    #[test]
    fn a_near_surface_caps_only_the_side_it_occupies() {
        for transform in [Mat4::IDENTITY, Mat4::from_rotation_y(std::f32::consts::PI)] {
            for (offset, positive_side) in [(0.00007, true), (-0.00007, false)] {
                let mesh = joined_mesh([
                    quad(0.0, 0.4, 0, false),
                    quad(offset, 0.3, 1, true),
                    quad(0.0, 0.3, 2, false),
                    quad(0.0, 0.25, 3, true),
                    quad(0.0, 0.2, 4, false),
                ]);
                let mut run = ObjectRun::default();
                run.begin(None);
                let (levels, caps, _) =
                    run.levels_with_caps([0; 3], 0, 0, transform, &mesh_flats(&mesh), 5);
                assert_eq!(levels, [0, 0, 1, 2, 2]);
                let (near, far) = if positive_side {
                    (caps[4].positive, caps[4].negative)
                } else {
                    (caps[4].negative, caps[4].positive)
                };
                assert!((near - 0.000063).abs() < 1e-7, "offset {offset}: {caps:?}");
                assert!(far.is_infinite(), "offset {offset}: {caps:?}");
            }
        }
    }

    #[test]
    fn a_near_plane_crossing_a_target_does_not_cap_the_whole_slot() {
        let mut crossing = quad(0.0, 0.3, 1, true);
        for point in &mut crossing.positions {
            point.z += 0.00018 + point.x * 0.001;
        }
        let mesh = joined_mesh([
            quad(0.0, 0.5, 0, false),
            crossing,
            quad(0.0, 0.4, 2, false),
            quad(0.0, 0.3, 3, true),
        ]);
        let mut run = ObjectRun::default();
        run.begin(None);
        let (levels, caps, _) =
            run.levels_with_caps([0; 3], 0, 0, Mat4::IDENTITY, &mesh_flats(&mesh), 4);
        assert_eq!(levels, [0, 0, 1, 2]);
        assert!(caps[3].positive.is_infinite());
        assert!(caps[3].negative.is_infinite());
    }

    #[test]
    fn a_print_inherits_the_cap_of_its_support_surface() {
        let mut run = ObjectRun::default();
        run.begin(None);
        let add = |run: &mut ObjectRun, mesh: usize, z, half| {
            run.levels_with_caps(
                [0; 3],
                mesh as u64,
                mesh,
                Mat4::IDENTITY,
                &mesh_flats(&quad(z, half, 0, false)),
                1,
            )
        };

        assert_eq!(add(&mut run, 0, 0.0, 1.2).0, [0]);
        assert_eq!(add(&mut run, 1, 0.00007, 0.2).0, [0]);
        let (support_levels, support_caps, _) = add(&mut run, 2, 0.0, 1.0);
        assert_eq!(support_levels, [1]);
        assert!((support_caps[0].positive - 0.000063).abs() < 1e-7);

        let (child_levels, child_caps, _) = add(&mut run, 3, 0.00004, 0.1);
        assert_eq!(child_levels, [2]);
        assert!((child_caps[0].positive - 0.000063).abs() < 1e-7);
    }

    #[test]
    fn an_inherited_cap_is_dropped_when_its_plane_crosses_the_child() {
        let mut near = quad(0.00018, 0.4, 1, true);
        for point in &mut near.positions {
            point.z += point.x * 0.001;
        }
        let mesh = joined_mesh([
            quad(0.0, 0.8, 0, false),
            near,
            quad(0.0, 0.1, 2, false),
            quad(0.0, 0.4, 3, true),
        ]);
        let mut run = ObjectRun::default();
        run.begin(None);
        let (levels, caps, _) =
            run.levels_with_caps([0; 3], 0, 0, Mat4::IDENTITY, &mesh_flats(&mesh), 4);
        assert_eq!(levels, [0, 0, 1, 2]);
        assert!(
            caps[2].positive.is_finite(),
            "support must keep its front cap"
        );
        assert!(
            caps[3].positive.is_infinite(),
            "crossing inherited cap: {caps:?}"
        );
        assert!(
            caps[3].negative.is_infinite(),
            "crossing inherited cap: {caps:?}"
        );
    }

    #[test]
    fn repeated_support_flats_do_not_duplicate_inherited_caps() {
        let mut run = ObjectRun::default();
        run.begin(None);
        let add = |run: &mut ObjectRun, mesh: usize, flats: &[Flat]| {
            run.levels_with_caps([0; 3], mesh as u64, mesh, Mat4::IDENTITY, flats, 1)
        };
        let base = mesh_flats(&quad(0.0, 1.2, 0, false))[0];
        let occluder = mesh_flats(&quad(0.00007, 0.2, 0, false))[0];
        add(&mut run, 0, &[base]);
        add(&mut run, 1, &[occluder]);

        let support_flats = vec![base; 16];
        let support_start = run.flat_caps.len();
        add(&mut run, 2, &support_flats);
        assert!(
            run.flat_caps[support_start..]
                .iter()
                .all(|caps| caps.len() == 1)
        );
        assert!((run.flat_caps[support_start][0].value - 0.000063).abs() < 1e-7);

        let child_flat = mesh_flats(&quad(0.00004, 0.1, 0, false))[0];
        let child_flats = vec![child_flat; 16];
        let child_start = run.flat_caps.len();
        add(&mut run, 3, &child_flats);
        assert!(
            run.flat_caps[child_start..]
                .iter()
                .all(|caps| caps.len() == 1)
        );
        assert!((run.flat_caps[child_start][0].value - 0.000063).abs() < 1e-7);
    }
    #[test]
    fn favourite_page_limits_only_earlier_duplicate_panel_geometry() {
        let mesh = joined_mesh([
            quad(0.0, 0.2, 3, false),
            quad(0.0, 0.2, 10, true),
            quad(0.0, 0.2, 7, true),
            quad(0.001, 0.1, 8, false),
        ]);
        let flats = mesh_flats(&mesh);
        assert_eq!(duplicate_support_slots(&mesh, &flats), vec![3]);
    }

    #[test]
    fn duplicate_support_keeps_the_floor_of_its_existing_mesh_stack() {
        let base = quad(0.0, 1.0, 0, false);
        let first_layer = quad(0.0, 0.8, 0, true);
        let second_layer = quad(0.0, 0.6, 0, false);
        let support = joined_mesh([quad(0.0, 0.2, 3, false), quad(0.0, 0.2, 10, true)]);
        assert_eq!(
            duplicate_support_slots(&support, &mesh_flats(&support)),
            vec![3]
        );
        let mut run = ObjectRun::default();
        run.begin(None);
        run.levels_with_caps([0; 3], 0, 0, Mat4::IDENTITY, &mesh_flats(&base), 1);
        run.levels_with_caps([0; 3], 1, 1, Mat4::IDENTITY, &mesh_flats(&first_layer), 1);
        run.levels_with_caps([0; 3], 2, 2, Mat4::IDENTITY, &mesh_flats(&second_layer), 1);
        let (levels, _, floors) =
            run.levels_with_caps([0; 3], 3, 3, Mat4::IDENTITY, &mesh_flats(&support), 11);

        assert_eq!(levels[3], 3);
        assert_eq!(floors[3], 2);
        assert_eq!(duplicate_support_level(levels[3], floors[3]), levels[3]);
        assert_eq!(duplicate_support_level(2, 1), 1);
    }

    #[test]
    fn separated_same_mesh_layers_do_not_get_a_geometry_depth_limit() {
        let mesh = joined_mesh([quad(0.0, 0.2, 2, false), quad(0.0003, 0.2, 5, true)]);
        let flats = mesh_flats(&mesh);
        assert!(duplicate_support_slots(&mesh, &flats).is_empty());
    }

    #[test]
    fn mixed_material_slot_is_not_limited_for_only_one_duplicate_plane() {
        let mesh = joined_mesh([
            quad(0.0, 0.2, 3, false),
            quad(0.0, 0.2, 7, true),
            quad(0.004, 0.1, 3, false),
            quad(0.004, 0.1, 8, true),
        ]);
        let flats = mesh_flats(&mesh);
        assert!(duplicate_support_slots(&mesh, &flats).is_empty());
    }
}
