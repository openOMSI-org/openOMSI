//! Helper methods for resolving placed scenery types and poses during staging.
use super::*;

impl World {
    /// The type an object record puts on the map: a parking space gets a random car of the
    /// map's parklist (a quarter of them stay empty, as in the original). None when nothing
    /// is to be placed; a missing type is counted and logged once.
    pub(super) fn placed_type(
        &self,
        file: &str,
        captions: &[String],
        key: i64,
        tx: i32,
        ty: i32,
        stats: &Mutex<LoadStats>,
    ) -> Option<(Arc<ObjectType>, bool)> {
        if file.trim().is_empty() {
            // a damaged record names nothing
            return None;
        }
        let Some(ot) = self.object_type(file) else {
            self.note_missing(file, "scenery object", tx, ty, key);
            stats.lock().failed_objects += 1;
            if omsi_cfg::flags::OMSI_DEBUG_MISSING.is_set() {
                log::info!("object not placed: {file} (tile {tx},{ty}, id {key})");
            }
            return None;
        };
        if !ot.sco.is_car_park {
            // (a plant of a season's phase in its own look)
            return Some((self.plant_look(file, ot, key, tx, ty), false));
        }
        let list = self.parked_car_types(parklist_index(captions));
        if list.is_empty() {
            return Some((ot, false));
        }
        let h = (key as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 33;
        // leave some spaces empty like the original (the options' count of parked cars is
        // kept when the tile is placed: see `place_step`)
        if h % 4 == 0 || self.parked_max < 0 {
            stats.lock().empty_spaces += 1;
            return None;
        }
        let car = &list[(h as usize / 4) % list.len()];
        let Some(base) = self.object_type(car) else {
            self.note_missing(car, "parked car", tx, ty, key);
            return None;
        };
        if base.paint_scheme_count > 0 {
            return self
                .object_type_scheme(car, Some((h as usize / 64) % base.paint_scheme_count))
                .map(|t| (t, true));
        }
        Some((base, true))
    }

    /// The ground as the tile files have it at world x, y (None outside `src`).
    pub(super) fn base_ground(
        src: &HashMap<(i32, i32), Arc<StagedTile>>,
        x: f64,
        y: f64,
    ) -> Option<f64> {
        let k = (
            (x / tile_size()).floor() as i32,
            (y / tile_size()).floor() as i32,
        );
        let st = src.get(&k)?;
        Some(
            st.base_terrain
                .sample((x - st.origin.x) as f32, (y - st.origin.y) as f32) as f64,
        )
    }

    /// Where a staged object stands before the ground is edited: crossings are warped and
    /// the ground deformed from there.
    pub(super) fn provisional_pose(
        st: &StagedTile,
        o: &StagedObject,
        src: &HashMap<(i32, i32), Arc<StagedTile>>,
    ) -> Option<Pose> {
        match &o.place {
            Placement::Pose(p) => Some(*p),
            Placement::Ground { x, y, z, rot } => {
                let (lx, ly) = (
                    (x - st.origin.x).clamp(0.0, tile_size()) as f32,
                    (y - st.origin.y).clamp(0.0, tile_size()) as f32,
                );
                let base_height = Self::base_ground(src, *x, *y)
                    .unwrap_or_else(|| st.base_terrain.sample(lx, ly) as f64);
                // Omsi.exe sets every object without an absolute height (those are `Pose`s,
                // `SceneryObject::absolute_height`) on the terrain, `[surface]` ones as well
                // (TMap.RefreshObjectsKacheln 0x79e3c8 reads sco+0x194).
                Some(Pose {
                    pos: DVec3::new(*x, *y, z + base_height),
                    rot: object_rotation(*rot),
                })
            }
            Placement::Attached { .. } => None,
        }
    }

    /// The `[maplight]`s of the objects of tile `key` and its eight neighbours, as a light map
    /// bakes them (Omsi.exe 0x7903e0 goes through the tile's near objects, those of the nine
    /// tiles, 0x77fe5c).
    pub(super) fn light_map_lamps(
        &self,
        key: (i32, i32),
        layout: &TileLayout,
        staged: &HashMap<(i32, i32), Arc<StagedTile>>,
    ) -> Vec<BakeLamp> {
        let mut lamps = Vec::new();
        for q in layout.ring(key) {
            let Some(qs) = staged.get(&q) else { continue };
            if qs.objects.iter().all(|o| o.ot.sco.map_lights.is_empty()) {
                continue;
            }
            let Some(res) = self.resolve(q, &Self::sources(layout, staged, q)) else {
                continue;
            };
            for (o, pose) in qs.objects.iter().zip(res.poses.iter()) {
                let Some(pose) = pose else { continue };
                for ml in &o.ot.sco.map_lights {
                    let p = pose
                        .rot
                        .transform_point3(glam::Vec3::from(ml.pos))
                        .as_dvec3()
                        + pose.pos;
                    lamps.push(BakeLamp {
                        x: p.x,
                        y: p.y,
                        height: ml.pos[2],
                        color: ml.color,
                        radius: ml.radius,
                    });
                }
            }
        }
        lamps
    }

    /// A tile's final ground and final object poses (computed once per staging; `src` are
    /// the staged tiles it depends on).
    pub(super) fn resolve(
        &self,
        key: (i32, i32),
        src: &HashMap<(i32, i32), Arc<StagedTile>>,
    ) -> Option<Arc<Resolved>> {
        let st = src.get(&key)?;
        Some(
            st.resolved
                .get_or_init(|| Arc::new(self.compute_resolved(st, src)))
                .clone(),
        )
    }

    pub(super) fn compute_resolved(
        &self,
        st: &StagedTile,
        src: &HashMap<(i32, i32), Arc<StagedTile>>,
    ) -> Resolved {
        let key = (st.tx, st.ty);
        let (terrain, aligned_points, biggest, deformed) = self.final_ground(key, src);
        let warped = self.warp_crossings(st, src);
        let ground_at = |x: f64, y: f64| -> f64 {
            let actual_key = (
                (x / tile_size()).floor() as i32,
                (y / tile_size()).floor() as i32,
            );
            if actual_key == key {
                let lx = (x - st.origin.x).clamp(0.0, tile_size()) as f32;
                let ly = (y - st.origin.y).clamp(0.0, tile_size()) as f32;
                terrain.sample(lx, ly) as f64
            } else {
                // Old maps and converted maps can keep an object in the neighbouring tile's
                // file with local coordinates past the edge. Sample the terrain actually
                // under the object instead of pinning it to this tile's border height.
                Self::base_ground(src, x, y).unwrap_or_else(|| {
                    let lx = (x - st.origin.x).clamp(0.0, tile_size()) as f32;
                    let ly = (y - st.origin.y).clamp(0.0, tile_size()) as f32;
                    terrain.sample(lx, ly) as f64
                })
            }
        };
        // poses of everything that can carry an attachment: objects by id, spline rows by
        // their first object
        let mut poses: HashMap<(i64, usize), (Pose, Arc<ObjectType>)> = HashMap::new();
        let mut final_poses: Vec<Option<Pose>> = st
            .objects
            .iter()
            .map(|o| match &o.place {
                // Omsi.exe places every object, a parking space's car as well, with the pitch
                // and bank of the map file on the terrain height at its position (0x79e3c8
                // .. 0x79e5fb: RotationX(pitch), RotationZ(bank), RotationY(heading), the
                // translation) - it is never leaned to the slope. Leaned by the terrain under
                // it, a car at the kerb of a hill street stood crooked on a road that runs
                // on a different grade from the ground beneath.
                Placement::Ground { x, y, z, rot } => Some(Pose {
                    pos: DVec3::new(*x, *y, z + ground_at(*x, *y)),
                    rot: object_rotation(*rot),
                }),
                Placement::Pose(p) => Some(*p),
                Placement::Attached { .. } => None,
            })
            .collect();
        for (o, p) in st.objects.iter().zip(&final_poses) {
            if let Some(p) = p {
                if !matches!(o.place, Placement::Pose(_)) || o.map_object {
                    poses
                        .entry((o.id, o.instance))
                        .or_insert((*p, o.ot.clone()));
                }
            }
        }
        for (id, pose, ot) in &st.anchors {
            poses.entry((*id, 0)).or_insert((*pose, ot.clone()));
        }
        // attachments hang on attachments (a whip beam on a lamp post, a traffic light on
        // the beam): resolve them round by round
        loop {
            let mut progress = false;
            for (o, fp) in st.objects.iter().zip(final_poses.iter_mut()) {
                let Placement::Attached { parent, index, rot } = &o.place else {
                    continue;
                };
                if fp.is_some() {
                    continue;
                }
                // (a spline attachment row by its first object: Omsi.exe refuses objects
                // on its later ones)
                let Some((pp, pt)) = poses.get(&(*parent, 0)) else {
                    continue;
                };
                // a point the parent does not have (its object was changed after the map
                // was made: the Ahlheim signal heads on `Arm_3f` point 10 of 1) is the
                // parent's own origin, as in the original - the object is not dropped
                let m = pt
                    .sco
                    .attachments
                    .get(*index)
                    .map(omsi_world::map_index::attachment_matrix)
                    .unwrap_or(glam::Mat4::IDENTITY);
                if *index >= pt.sco.attachments.len() {
                    // `OMSI_NO_ATTACH_FALLBACK=1` drops them instead, for an A/B
                    if omsi_cfg::flags::OMSI_NO_ATTACH_FALLBACK.is_set() {
                        continue;
                    }
                    if omsi_cfg::flags::OMSI_DEBUG_MISSING.is_set() {
                        log::info!(
                            "attached object {} (id {}) on point index {index} of {parent} ({} has {}): at the parent's origin ({:.1}, {:.1}, {:.1})",
                            o.ot.sco.path.display(),
                            o.id,
                            pt.sco.path.display(),
                            pt.sco.attachments.len(),
                            pp.pos.x,
                            pp.pos.y,
                            pp.pos.z
                        );
                    }
                }
                let pose = pp.attached(&m, *rot);
                *fp = Some(pose);
                poses.entry((o.id, 0)).or_insert((pose, o.ot.clone()));
                progress = true;
            }
            if !progress {
                break;
            }
        }
        let mut unattached = 0usize;
        for (o, fp) in st.objects.iter().zip(&final_poses) {
            if let (Placement::Attached { parent, .. }, None) = (&o.place, fp) {
                unattached += 1;
                if omsi_cfg::flags::OMSI_DEBUG_MISSING.is_set() {
                    log::info!(
                        "attached object {} (id {}) on {parent} not placed: the parent is not in the tile",
                        o.ot.sco.path.display(),
                        o.id
                    );
                }
            }
        }
        Resolved {
            terrain: Arc::new(terrain),
            warped,
            poses: final_poses,
            unattached,
            aligned_points,
            biggest,
            deformed,
        }
    }
}
