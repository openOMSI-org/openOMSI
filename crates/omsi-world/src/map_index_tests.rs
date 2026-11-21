mod tests {
    use super::*;
    include!("../../../tools/test-support/original_root.rs");
    fn synthetic_spline_stop(
        version: i32,
        stop: i64,
        offset: f64,
        distance: f64,
        spline_index: usize,
    ) -> String {
        let links = if version >= 11 { "0\n0\n" } else { "-1\n" };
        let chain = if version >= 11 {
            format!("0\n0\n{offset}\n")
        } else {
            String::new()
        };
        let turn = if version >= 12 {
            "0\n0\n0\n10\n0\n0\n"
        } else {
            "0\n10\n0\n"
        };
        format!(
            "[version]\n{version}\n\n[spline]\n0\nsynthetic.sli\n10\n{links}0\n0\n0\n0\n50\n0\n0\n0\n0\n0\n{chain}\n[splineAttachement]\n0\nsynthetic-stop.sco\n{stop}\n{spline_index}\n0\n0\n{distance}\n{turn}0\n"
        )
    }
    /// Great Grundorf 2 (tile version 10): a linked spline is written at x, height, y and
    /// heading 0 and goes on from the end of the spline before it - a 40 m right-hand curve
    /// of radius 300 here, climbing at 2 %, whose IDCode is 0; one written with -1 keeps its
    /// own place.
    #[test]
    fn old_linked_splines_go_on_from_the_one_before() {
        let text = "[version]\n10\n\n\
[spline]\n0\nSplines\\road.sli\n0\n-1\n180\n5\n170\n10\n40\n300\n2\n2\n0\n0\n\n\
[spline]\n0\nSplines\\road.sli\n2\n0\n0\n0\n0\n0\n50\n50\n0\n8\n0\n0\n\n\
[spline]\n0\nSplines\\road.sli\n3\n-1\n0\n0\n0\n0\n20\n0\n0\n0\n0\n0\n";
        let mut tile = Tile::parse(&omsi_cfg::CfgFile::from_str("tile_0_0.map", text));
        chain_linked_splines(&mut tile);
        let first = SplineCurve::from_map(&tile.splines[0], DVec2::ZERO);
        let (end, heading) = (first.end_point(), first.heading_at(40.0));
        let b = &tile.splines[1];
        assert!(
            (DVec3::from_array(b.pos) - end).length() < 1e-9,
            "{:?} vs {end:?}",
            b.pos
        );
        assert!(
            (b.heading - heading).abs() < 1e-9
                && (heading - (10.0 + (40.0f64 / 300.0).to_degrees())).abs() < 1e-9
        );
        assert!(
            (b.pos[2] - 5.8).abs() < 1e-9,
            "the height at the end of a 2 % climb of 40 m"
        );
        // (unlinked: its own place, even the corner)
        assert_eq!(
            (tile.splines[2].pos, tile.splines[2].heading),
            ([0.0; 3], 0.0)
        );
        // a version 11 tile names the neighbours and keeps every place as written
        let newer = text.replace("[version]\n10", "[version]\n11");
        let mut tile = Tile::parse(&omsi_cfg::CfgFile::from_str("tile_0_0.map", &newer));
        let before = tile.splines.iter().map(|s| s.pos).collect::<Vec<_>>();
        chain_linked_splines(&mut tile);
        assert_eq!(
            tile.splines.iter().map(|s| s.pos).collect::<Vec<_>>(),
            before
        );
    }
    #[test]
    fn cold_child_stop_keeps_its_spline_attached_parent_and_metadata() {
        let dir =
            std::env::temp_dir().join(format!("omsi-index-child-spline-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tile_3_-2.map");
        // The parent's 625 m distance includes 600 m before its own 50 m segment.
        // Its child can be indexed only after that parent's row pose is resolved.
        let contents = synthetic_spline_stop(14, 100, 600.0, 625.0, 0)
            + "\n[attachObj]\n0\nsynthetic-child.sco\n200\n100\n0\n0\n30\n0\n0\n6\nChild stop\n7\n2\n11\n22.5\n1\n";
        std::fs::write(&path, contents).unwrap();
        let index = MapIndex::build(&[(0, 3, -2, path)], &[], &dir);
        let expected = DVec3::new(3.0 * tile_size(), -2.0 * tile_size() + 25.0, 0.0);
        for id in [100, 200] {
            let (tile, pos, _) = index
                .objects
                .get(&id)
                .expect("both parent and child must be indexed cold");
            assert_eq!(*tile, (3, -2));
            assert!((*pos - expected).length() < 1e-8);
        }
        assert_eq!(index.objects[&200].2, [30.0, 0.0, 0.0]);
        assert_eq!(index.stop_enter[&200], (7.0, 2.0));
        assert_eq!(index.stop_weights[&200], 11.0);
        assert_eq!(index.stop_length[&200], 22.5);
        assert_eq!(index.stop_side[&200], 1.0);
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn authored_offsets_keep_stops_on_tiles_with_duplicate_spline_ids() {
        let dir = std::env::temp_dir().join(format!(
            "omsi-index-duplicate-spline-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let (first, second) = (dir.join("tile_0_0.map"), dir.join("tile_2_-1.map"));
        for version in [10, 14] {
            // A legacy root has no authored offset; a modern root explicitly owns zero.
            // Neither must inherit the unrelated town's offset from the same spline id.
            std::fs::write(&first, synthetic_spline_stop(version, 100, 0.0, 25.0, 0)).unwrap();
            std::fs::write(&second, synthetic_spline_stop(14, 200, 600.0, 625.0, 0)).unwrap();
            let index = MapIndex::build(
                &[(0, 0, 0, first.clone()), (1, 2, -1, second.clone())],
                &[],
                &dir,
            );
            assert_eq!(index.splines[&10].map_chain_offset, Some(600.0));
            for (tile_index, stop, x, y, path) in [(0, 100, 0, 0, &first), (1, 200, 2, -1, &second)]
            {
                let origin = DVec2::new(x as f64 * tile_size(), y as f64 * tile_size());
                let pos = index
                    .objects
                    .get(&stop)
                    .expect("both towns must keep their stop")
                    .1;
                assert!((pos - origin.extend(0.0) - DVec3::new(0.0, 25.0, 0.0)).length() < 1e-8);
                assert_eq!(index.masters[&(tile_index, stop)].1, 25.0);
                let tile = read_tile(path, &[]).unwrap();
                let placed = tile_row_objects(
                    &tile.spline_attachments[0],
                    &tile.splines,
                    origin,
                    Some(&index),
                );
                assert_eq!(placed.len(), 1);
                assert!(
                    (placed[0].1.pose.pos - pos).length() < 1e-8,
                    "streamed placement also uses the local record's offset"
                );
            }
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn legacy_stops_survive_modern_chrono_with_reused_spline_id() {
        let dir =
            std::env::temp_dir().join(format!("omsi-index-chrono-spline-{}", std::process::id()));
        let chrono = dir.join("Chrono/addition");
        std::fs::create_dir_all(&chrono).unwrap();
        let path = dir.join("tile_0_0.map");
        std::fs::write(&path, synthetic_spline_stop(10, 100, 0.0, 25.0, 0)).unwrap();
        // Chrono attachment indices count in the combined spline list.
        std::fs::write(
            chrono.join("tile_0_0.map"),
            synthetic_spline_stop(14, 200, 600.0, 625.0, 1),
        )
        .unwrap();
        let index = MapIndex::build(&[(0, 0, 0, path.clone())], &[chrono.clone()], &dir);
        let tile = read_tile(&path, &[chrono]).unwrap();
        assert_eq!(tile.splines[0].map_chain_offset, None);
        assert_eq!(tile.splines[1].map_chain_offset, Some(600.0));
        for attachment in &tile.spline_attachments {
            let pos = index
                .objects
                .get(&attachment.id)
                .expect("chrono must preserve both stops")
                .1;
            assert!((pos - DVec3::new(0.0, 25.0, 0.0)).length() < 1e-8);
            assert_eq!(index.masters[&(0, attachment.id)].1, 25.0);
            let placed = tile_row_objects(attachment, &tile.splines, DVec2::ZERO, Some(&index));
            assert_eq!(placed.len(), 1);
            assert!((placed[0].1.pose.pos - pos).length() < 1e-8);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn cold_index_resolves_chained_spline_stops() {
        let dir =
            std::env::temp_dir().join(format!("omsi-index-stop-chain-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for version in [10, 14] {
            // Entirely synthetic: the stop lies 25 m into the second segment, but its
            // authored distance (625 m) includes the 600 m before that segment.
            let spline = |id: i64, y: f64, length: f64, offset: f64| {
                let links = if version >= 11 {
                    // Stale predecessor links must not override the authored offset.
                    format!("0\n{}\n", if id == 1 { 2 } else { 0 })
                } else {
                    format!("{}\n", if id == 1 { -1 } else { 0 })
                };
                let chain = if version >= 11 {
                    format!("0\n0\n{offset}\n")
                } else {
                    String::new()
                };
                format!(
                    "[spline]\n0\nsynthetic.sli\n{id}\n{links}0\n0\n{y}\n0\n{length}\n0\n0\n0\n0\n0\n{chain}\n"
                )
            };
            let turn = if version >= 12 {
                "180\n0\n0\n10\n0\n1\n"
            } else {
                "180\n10\n0\n"
            };
            let contents = format!(
                "[version]\n{version}\n\n{}{}[splineAttachement]\n0\nsynthetic-stop.sco\n100\n1\n-3\n2\n625\n{turn}0\n",
                spline(1, 0.0, 600.0, 0.0),
                spline(2, 600.0, 50.0, 600.0)
            );
            let path = dir.join(format!("tile_3_-2_v{version}.map"));
            std::fs::write(&path, contents).unwrap();
            let index = MapIndex::build(&[(0, 3, -2, path.clone())], &[], &dir);
            let (_, pos, rot) = index
                .objects
                .get(&100)
                .expect("the cold index must know the stop");
            assert!(
                (*pos - DVec3::new(3.0 * tile_size() - 3.0, -2.0 * tile_size() + 625.0, 2.0))
                    .length()
                    < 1e-8
            );
            let tile = read_tile(&path, &[]).unwrap();
            let placed = tile_row_objects(
                &tile.spline_attachments[0],
                &tile.splines,
                DVec2::new(3.0 * tile_size(), -2.0 * tile_size()),
                Some(&index),
            );
            assert_eq!(placed.len(), 1);
            assert!(
                (*pos - placed[0].1.pose.pos).length() < 1e-8,
                "cold and streamed placement must agree"
            );
            assert!((rot[0] - placed[0].1.pose.heading()).abs() < 1e-6);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
    /// An active chrono patch with `[terrain]`/`[water]` and their files beside it gives the
    /// tile its ground and water (#923, #925); one without them leaves the map's own.
    #[test]
    fn a_chrono_patch_with_its_own_terrain_and_water_replaces_the_tiles() {
        let dir = std::env::temp_dir().join(format!("omsi-chrono-terrain-{}", std::process::id()));
        let (base, c1, c2) = (
            dir.join("map"),
            dir.join("map/Chrono/a"),
            dir.join("map/Chrono/b"),
        );
        for d in [&base, &c1, &c2] {
            std::fs::create_dir_all(d).unwrap();
        }
        let name = "tile_0_0.map";
        std::fs::write(
            base.join(name),
            "[version]\n14\n\n[terrain]\n0\n\n[water]\n0\n",
        )
        .unwrap();
        std::fs::write(base.join(format!("{name}.terrain")), b"x").unwrap();
        std::fs::write(base.join(format!("{name}.water")), b"x").unwrap();
        // a: reshapes the ground and the water; b (later): only objects
        std::fs::write(
            c1.join(name),
            "[version]\n14\n\n[terrain]\n0\n\n[water]\n0\n",
        )
        .unwrap();
        std::fs::write(c1.join(format!("{name}.terrain")), b"x").unwrap();
        std::fs::write(c1.join(format!("{name}.water")), b"x").unwrap();
        std::fs::write(c2.join(name), "[version]\n14\n").unwrap();
        let path = base.join(name);
        let t = read_tile(&path, &[c1.clone(), c2.clone()]).unwrap();
        assert_eq!(terrain_file(&t, &path), c1.join(format!("{name}.terrain")));
        assert_eq!(water_file(&t, &path), c1.join(format!("{name}.water")));
        let t = read_tile(&path, &[c2.clone()]).unwrap();
        assert_eq!(
            terrain_file(&t, &path),
            base.join(format!("{name}.terrain"))
        );
        assert_eq!(water_file(&t, &path), base.join(format!("{name}.water")));
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn traffic_light_parents_follow_placed_signals_not_other_children() {
        let object = |file: &str, parent| omsi_map::MapObject {
            file: file.into(),
            var_parent: Some(parent),
            ..Default::default()
        };
        let tile = Tile {
            objects: vec![object("signal.sco", 10), object("sign.sco", 20)],
            attach_objects: vec![
                omsi_map::MapObject {
                    file: "signal.sco".into(),
                    parent_id: Some(30),
                    ..Default::default()
                },
                omsi_map::MapObject {
                    parent_id: Some(40),
                    ..object("signal.sco", 50)
                },
            ],
            spline_attachments: vec![SplineAttachment {
                file: "signal.sco".into(),
                var_parent: Some(60),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert_eq!(
            traffic_light_parents(&tile, |file| file == "signal.sco"),
            [10, 30, 50, 60].into_iter().collect()
        );
    }
    #[test]
    fn a_child_naming_a_light_counts_whatever_its_type() {
        // (OMSI's RefreshAmpelParenting reads the light index, not the object's kind)
        let object = |file: &str, parent, strings: &[&str]| omsi_map::MapObject {
            file: file.into(),
            var_parent: Some(parent),
            extra: strings.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        };
        let tile = Tile {
            objects: vec![
                object("mod_lamp.sco", 10, &["2"]),
                object("mod_lamp.sco", 20, &[" 0 "]),
                object("display.sco", 30, &["", ""]),
                object("sign.sco", 40, &["-1"]),
                object("sign.sco", 50, &["Hbf"]),
                object("sign.sco", 60, &[]),
            ],
            ..Default::default()
        };
        assert_eq!(light_naming_parents(&tile), [10, 20].into_iter().collect());
        assert!(traffic_light_parents(&tile, |_| false).is_empty());
        assert!(names_traffic_light(&["3".into()]) && !names_traffic_light(&["x".into()]));
    }
    #[test]
    fn a_junction_template_without_placed_signals_is_unsignalized() {
        let tile = Tile {
            objects: vec![omsi_map::MapObject {
                file: "junction.sco".into(),
                id: 10,
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(traffic_light_parents(&tile, |_| false).is_empty());
    }
    #[test]
    #[ignore = "needs the original OMSI 2 install (OMSI_ROOT)"]
    fn grundorf_traffic_light_parents_exclude_gaussdorf() {
        let root = original_root();
        let map_dir = root.join("maps/Grundorf");
        let global = omsi_map::GlobalCfg::load(&map_dir.join("global.cfg")).expect("Grundorf");
        let tiles = global
            .tiles
            .iter()
            .map(|t| (t.index, t.x, t.y, map_dir.join(&t.file)))
            .collect::<Vec<_>>();
        let index = MapIndex::build(&tiles, &[], &root);
        assert_eq!(index.tiles_failed, 0);
        assert!(
            index.traffic_light_parents.contains(&4174),
            "the real signalized junction"
        );
        for id in [759, 761] {
            assert!(
                !index.traffic_light_parents.contains(&id),
                "Gaussdorf junction {id} has no signals"
            );
        }
    }
    /// The objects row record `att` puts on its own spline `spline`.
    fn row_objects(
        att: &SplineAttachment,
        spline: &MapSpline,
        origin: DVec2,
        index: Option<&MapIndex>,
    ) -> Vec<RowObject> {
        row_start(att, spline, index)
            .map(|start| place_on(att, spline, origin, index, start))
            .unwrap_or_default()
    }
    fn spline(id: i64, prev: i64, next: i64, length: f64) -> MapSpline {
        MapSpline {
            file: String::new(),
            id,
            prev_id: prev,
            next_id: next,
            pos: [0.0, 0.0, 10.0],
            heading: 0.0,
            length,
            ..Default::default()
        }
    }
    fn row(
        interval: f64,
        range: f64,
        d: f64,
        repeater: Option<(usize, usize)>,
    ) -> SplineAttachment {
        SplineAttachment {
            file: "lamp.sco".into(),
            id: 5,
            spline_index: 0,
            offset: [3.0, 0.25, d],
            interval,
            range,
            repeater,
            ..Default::default()
        }
    }
    #[test]
    fn master_row() {
        let s = spline(1, 0, 2, 100.0);
        let objs = row_objects(&row(30.0, 1000.0, 10.0, None), &s, DVec2::ZERO, None);
        let at: Vec<f64> = objs.iter().map(|o| o.pose.pos.y).collect();
        // 10, 40, 70 and 100 m: the one on the joint belongs to this spline
        assert_eq!(at.len(), 4);
        assert!(
            (at[0] - 10.0).abs() < 1e-6 && (at[3] - 100.0).abs() < 1e-6,
            "{at:?}"
        );
        // right of a spline heading north is east, the height is above the spline
        assert!(
            (objs[0].pose.pos.x - 3.0).abs() < 1e-6 && (objs[0].pose.pos.z - 10.25).abs() < 1e-6
        );
        // range 45 m: objects 0 and 1 only
        assert_eq!(
            row_objects(&row(30.0, 45.0, 10.0, None), &s, DVec2::ZERO, None).len(),
            2
        );
        // a single object beyond the end belongs to the next spline of the chain
        assert!(row_objects(&row(0.0, 0.0, 130.0, None), &s, DVec2::ZERO, None).is_empty());
        // ... unless the chain ends here: a little past the end is at the end
        let dead_end = spline(1, 0, 0, 100.0);
        let one = row_objects(&row(0.0, 0.0, 100.6, None), &dead_end, DVec2::ZERO, None);
        assert_eq!(one.len(), 1);
        assert!((one[0].pose.pos.y - 100.0).abs() < 1e-6);
        assert!(row_objects(&row(0.0, 0.0, 102.0, None), &dead_end, DVec2::ZERO, None).is_empty());
    }
    #[test]
    fn the_object_on_a_tile_joint_is_drawn_once_whichever_side_the_rounding_puts_it() {
        let first = spline(1, 0, 2, 100.0);
        let master = row(30.0, 1000.0, 10.0002, None);
        assert_eq!(
            row_objects(&master, &first, DVec2::ZERO, None).len(),
            4,
            "10, 40, 70 and the joint, 0.2 mm past the end"
        );
        let next = MapSpline {
            map_chain_offset: Some(100.0004),
            ..spline(2, 1, 0, 100.0)
        };
        let repeater = SplineAttachment {
            repeater: Some((3, 3)),
            ..master.clone()
        };
        let at: Vec<usize> = row_objects(&repeater, &next, DVec2::ZERO, None)
            .iter()
            .map(|o| o.index)
            .collect();
        assert_eq!(
            at,
            vec![4, 5, 6],
            "the joint's object, 0.2 mm before this spline, is the first spline's"
        );
    }
    #[test]
    fn an_object_on_a_joint_inside_a_tile_is_drawn_once() {
        let ix = buffer_stop_chain();
        let a = spline(10, 0, 11, 400.0);
        let b = MapSpline {
            pos: [0.0, 600.0, 10.0],
            heading: 180.0,
            ..spline(11, 12, 10, 200.0)
        };
        let master = SplineAttachment {
            id: 9,
            ..row(130.0, 600.0, 10.0002, None)
        };
        let objs = tile_row_objects(&master, &[a, b], DVec2::ZERO, Some(&ix));
        let idx: Vec<usize> = objs.iter().map(|(_, o)| o.index).collect();
        assert_eq!(
            idx,
            vec![0, 1, 2, 3, 4],
            "the object 0.2 mm past the joint at 400 m once: {idx:?}"
        );
    }
    #[test]
    fn a_row_s_last_object_just_past_its_range_is_drawn_on_every_spline_alike() {
        let long = spline(1, 0, 0, 1000.0);
        assert_eq!(
            row_objects(&row(130.0, 389.9995, 10.0, None), &long, DVec2::ZERO, None).len(),
            4,
            "390 m against a range written as 389.9995"
        );
        let ix = buffer_stop_chain();
        let a = spline(10, 0, 11, 300.0);
        let b = MapSpline {
            pos: [0.0, 600.0, 10.0],
            heading: 180.0,
            ..spline(11, 12, 10, 200.0)
        };
        let objs = tile_row_objects(
            &SplineAttachment {
                id: 9,
                ..row(130.0, 389.9995, 10.0, None)
            },
            &[a, b],
            DVec2::ZERO,
            Some(&ix),
        );
        assert_eq!(
            objs.iter().map(|(_, o)| o.index).collect::<Vec<_>>(),
            vec![0, 1, 2, 3],
            "the fourth, on the next spline, too"
        );
    }
    #[test]
    fn tangential_row_rotates_within_the_spline_plane() {
        let s = MapSpline {
            heading: 37.0,
            radius: 100.0,
            grad_start: 15.0,
            grad_end: -5.0,
            cant_start: 7.0,
            cant_end: 3.0,
            ..spline(1, 0, 0, 80.0)
        };
        let curve = SplineCurve::from_map(&s, DVec2::ZERO);
        let u = 25.0;
        let pitch = curve.slope_at(u).atan();
        let bank = (curve.cant_at(u) / 100.0).atan();
        let heading = curve.heading_at(u).to_radians();
        // An object's normal must not change when it is turned on that surface.
        let normal = DVec3::new(
            bank.sin() * pitch.cos(),
            -pitch.sin(),
            bank.cos() * pitch.cos(),
        );
        let expected = DVec3::new(
            normal.x * heading.cos() + normal.y * heading.sin(),
            -normal.x * heading.sin() + normal.y * heading.cos(),
            normal.z,
        )
        .as_vec3();
        for own_heading in [0.0, 90.0, 180.0, -90.0, 27.0] {
            let att = SplineAttachment {
                tilt: true,
                rot: [own_heading, 0.0, 0.0],
                ..row(0.0, 0.0, u, None)
            };
            let objects = row_objects(&att, &s, DVec2::ZERO, None);
            assert_eq!(objects.len(), 1);
            let rot = objects[0].pose.rot;
            assert!(
                (rot.transform_vector3(Vec3::Z) - expected).length() < 1e-6,
                "heading {own_heading}"
            );
            for axis in [Vec3::X, Vec3::Y] {
                assert!(
                    rot.transform_vector3(axis).dot(expected).abs() < 1e-6,
                    "heading {own_heading}"
                );
            }
        }
    }
    #[test]
    fn compound_row_rotation_keeps_retaining_posts_out_of_the_street() {
        // A mod uses a 5 m fence post laid on its side as a retaining block.
        // Its two quarter turns must send the post's long axis to the right of
        // the spline, rather than along the road as the reversed order did.
        let s = MapSpline {
            heading: 26.0,
            ..spline(1, 0, 0, 10.0)
        };
        let att = SplineAttachment {
            rot: [0.0, 90.0, -90.0],
            ..row(0.0, 0.0, 2.0, None)
        };
        let pose = row_objects(&att, &s, DVec2::ZERO, None)[0].pose;
        let right = SplineCurve::dir(s.heading + 90.0).as_vec2().extend(0.0);
        let tip = pose.rot.transform_vector3(Vec3::Z * 5.0);
        assert!(
            (tip - right * 5.0).length() < 1e-5,
            "a horizontal retaining post must extend into the bank, not the carriageway: {tip:?}"
        );
        assert!(tip.z.abs() < 1e-5);
    }
    #[test]
    fn tangential_row_keeps_its_frame_when_the_spline_runs_backwards() {
        let s = MapSpline {
            heading: 31.0,
            grad_start: 12.0,
            grad_end: 12.0,
            cant_start: 5.0,
            cant_end: 5.0,
            ..spline(1, 0, 0, 80.0)
        };
        let curve = SplineCurve::from_map(&s, DVec2::ZERO);
        let reverse = MapSpline {
            pos: curve.end_point().to_array(),
            heading: s.heading + 180.0,
            grad_start: -12.0,
            grad_end: -12.0,
            cant_start: -5.0,
            cant_end: -5.0,
            ..s.clone()
        };
        // Include the object's own pitch/bank: the spline frame must compose with
        // these too, and children inherit the resulting complete rotation.
        let att = SplineAttachment {
            tilt: true,
            rot: [90.0, 3.0, -2.0],
            ..row(0.0, 0.0, 20.0, None)
        };
        let forward = row_objects(&att, &s, DVec2::ZERO, None)[0].pose;
        let start = RowStart {
            s: 20.0,
            j: 0,
            backwards: true,
            d0: 20.0,
            acc: 0.0,
        };
        let backward = place_on(&att, &reverse, DVec2::ZERO, None, start)[0].pose;
        assert!((forward.pos - backward.pos).length() < 1e-8);
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            assert!(
                (forward.rot.transform_vector3(axis) - backward.rot.transform_vector3(axis))
                    .length()
                    < 1e-6
            );
        }
    }
    #[test]
    fn upright_row_and_objects_outside_cant_width_keep_their_placement_rules() {
        let s = MapSpline {
            heading: 65.0,
            grad_start: 10.0,
            grad_end: 10.0,
            cant_start: 20.0,
            cant_end: 20.0,
            ..spline(1, 0, 0, 80.0)
        };
        let att = SplineAttachment {
            rot: [90.0, 3.0, -2.0],
            ..row(0.0, 0.0, 20.0, None)
        };
        let upright = row_objects(&att, &s, DVec2::ZERO, None)[0].pose;
        let expected = omsi_geometry::object_rotation_ypr([155.0, -3.0, 2.0]);
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            assert!(
                (upright.rot.transform_vector3(axis) - expected.transform_vector3(axis)).length()
                    < 1e-6
            );
        }
        let outside = SplineAttachment {
            tilt: true,
            offset: [11.0, 0.25, 20.0],
            rot: [90.0, 0.0, 0.0],
            ..att
        };
        let pose = row_objects(&outside, &s, DVec2::ZERO, None)[0].pose;
        let right = SplineCurve::dir(s.heading + 90.0).as_vec2().extend(0.0);
        assert!(
            pose.rot.transform_vector3(Vec3::Z).dot(right).abs() < 1e-6,
            "beyond the half cant width, the row follows only the longitudinal slope"
        );
        assert!(
            (pose.pos.z - 10.25).abs() < 1e-8,
            "the position still includes the cant's clamped height"
        );
    }
    #[test]
    fn tangential_railing_follows_the_praha_spline() {
        // Placement of railing 26386 on spline 24096, Praha 200 tile 2623/10579.
        // Its mesh runs along local X, so the map turns it 90 degrees to the road.
        let s = MapSpline {
            id: 24096,
            pos: [163.9941, 44.16156, 270.8004],
            heading: -90.15699,
            length: 46.00197,
            grad_start: -0.5,
            grad_end: -0.5,
            tex_offset: 771.2027,
            map_chain_offset: Some(771.2027),
            ..Default::default()
        };
        let att = SplineAttachment {
            id: 26386,
            offset: [-0.0584662628010295, 0.279999999041864, 785.154450166645],
            rot: [90.0000020235813, 0.0, 0.0],
            tilt: true,
            ..Default::default()
        };
        let mut index = MapIndex::default();
        index.splines.insert(
            s.id,
            IndexedSpline {
                length: s.length,
                map_chain_offset: Some(s.tex_offset),
                prev: 0,
                next: 0,
            },
        );
        let objects = row_objects(&att, &s, DVec2::ZERO, Some(&index));
        assert_eq!(objects.len(), 1);
        let direction = SplineCurve::dir(s.heading);
        let tangent = DVec3::new(direction.x, direction.y, -0.005)
            .normalize()
            .as_vec3();
        assert!(
            (-objects[0].pose.rot.transform_vector3(Vec3::X) - tangent).length() < 1e-6,
            "the railing must descend along the kerb, not lean across it"
        );
    }
    /// The chain of the Spandau buffer stop 3212811: a dead-end track of 250 m behind 600 m
    /// of predecessors, one of them joined end to end.
    fn buffer_stop_chain() -> MapIndex {
        let mut ix = MapIndex::default();
        ix.splines.insert(
            10,
            IndexedSpline {
                length: 400.0,
                map_chain_offset: Some(0.0),
                prev: 0,
                next: 11,
            },
        );
        // spline 11 runs against the chain: its end meets 10, its start meets 12
        ix.splines.insert(
            11,
            IndexedSpline {
                length: 200.0,
                map_chain_offset: Some(250.0),
                prev: 12,
                next: 10,
            },
        );
        ix.splines.insert(
            12,
            IndexedSpline {
                length: 250.0,
                map_chain_offset: Some(600.0),
                prev: 11,
                next: 0,
            },
        );
        ix
    }
    #[test]
    fn row_start_counts_from_the_chain_start() {
        let mut ix = buffer_stop_chain();
        // Berlin's map links can be stale: keep the authored chain distance even when the
        // segment no longer points back to the spline that the distance includes.
        ix.splines.get_mut(&12).unwrap().prev = 0;
        assert_eq!(chain_offset(&ix, 10), 0.0);
        assert_eq!(chain_offset(&ix, 12), 600.0);
        // a row on spline 11 runs its way, from the joint with 12: the chain before it is 12
        assert_eq!(chain_offset(&ix, 11), 250.0);
        // d = 845.3 on the last spline: 4.7 m before the end of the track, not 95.3 m along
        let track = spline(12, 11, 0, 250.0);
        let objs = row_objects(&row(10.0, 0.0, 845.3, None), &track, DVec2::ZERO, Some(&ix));
        assert_eq!(objs.len(), 1);
        assert!(
            (objs[0].pose.pos.y - 245.3).abs() < 1e-6,
            "{:?}",
            objs[0].pose.pos
        );
        // a loop has no start: the walk stops where it comes round
        let mut ring = MapIndex::default();
        ring.splines.insert(
            1,
            IndexedSpline {
                length: 10.0,
                map_chain_offset: None,
                prev: 3,
                next: 2,
            },
        );
        ring.splines.insert(
            2,
            IndexedSpline {
                length: 20.0,
                map_chain_offset: None,
                prev: 1,
                next: 3,
            },
        );
        ring.splines.insert(
            3,
            IndexedSpline {
                length: 30.0,
                map_chain_offset: None,
                prev: 2,
                next: 1,
            },
        );
        assert_eq!(chain_offset(&ring, 1), 50.0);
    }

    /// A row's post due 0.75 m past a dead end is not put at the end (Thüringer Wald's
    /// fence post in the carriageway, #1693); a single object a little past it still is.
    #[test]
    fn a_row_stops_at_a_dead_end_a_single_object_stands_there() {
        let mut ix = MapIndex::default();
        ix.splines.insert(
            5,
            IndexedSpline {
                length: 12.0,
                map_chain_offset: Some(91.01),
                prev: 0,
                next: 0,
            },
        );
        let end = MapSpline {
            map_chain_offset: Some(91.01),
            ..spline(5, 0, 0, 12.0)
        };
        // posts every 3 m from 100.76 m of the chain over 7 m: 100.76 (9.75 m along) fits,
        // 103.76 is 0.75 m past the chain's end at 103.01
        let posts = row_objects(&row(3.0, 7.0, 100.7586, None), &end, DVec2::ZERO, Some(&ix));
        assert_eq!(
            posts.len(),
            1,
            "{:?}",
            posts.iter().map(|o| o.pose.pos.y).collect::<Vec<_>>()
        );
        assert_eq!(
            row_objects(&row(3.0, 3.0, 100.7586, None), &end, DVec2::ZERO, Some(&ix)).len(),
            1
        );
        let to_the_spline = row_objects(&row(3.0, 12.0, 91.21, None), &end, DVec2::ZERO, Some(&ix));
        assert_eq!(
            to_the_spline.len(),
            5,
            "{:?}",
            to_the_spline
                .iter()
                .map(|o| o.pose.pos.y)
                .collect::<Vec<_>>()
        );
        assert!((to_the_spline[4].pose.pos.y - 12.0).abs() < 1e-6);
        let stop = row_objects(&row(10.0, 0.0, 103.5, None), &end, DVec2::ZERO, Some(&ix));
        assert_eq!(stop.len(), 1);
        assert!(
            (stop[0].pose.pos.y - 12.0).abs() < 1e-6,
            "{:?}",
            stop[0].pose.pos
        );
    }

    #[test]
    fn row_runs_on_through_its_tile() {
        let ix = buffer_stop_chain();
        // spline 11 runs against the chain: its end meets 10 at y = 400, its start meets 12
        let s10 = spline(10, 0, 11, 400.0);
        let s11 = MapSpline {
            pos: [0.0, 600.0, 10.0],
            heading: 180.0,
            ..spline(11, 12, 10, 200.0)
        };
        let s12 = MapSpline {
            pos: [0.0, 600.0, 10.0],
            ..spline(12, 11, 0, 250.0)
        };
        // a row on spline 10 with objects at 20, 220, 420 and 620 m along the chain
        let master = SplineAttachment {
            id: 7,
            ..row(200.0, 600.0, 20.0, None)
        };
        let tile = [s10.clone(), s11.clone(), s12.clone()];
        let objs = tile_row_objects(&master, &tile, DVec2::ZERO, Some(&ix));
        let at: Vec<(usize, usize, f64)> = objs
            .iter()
            .map(|(si, o)| (*si, o.index, o.pose.pos.y))
            .collect();
        assert_eq!(at.len(), 4, "{at:?}");
        for (k, (si, j, y)) in [(0, 0, 20.0), (0, 1, 220.0), (1, 2, 420.0), (2, 3, 620.0)]
            .into_iter()
            .enumerate()
        {
            assert!(
                at[k].0 == si && at[k].1 == j && (at[k].2 - y).abs() < 1e-6,
                "{at:?}"
            );
        }
        // the row's right is the spline's left on the reversed spline: still east of the chain
        assert!(objs[2].1.pose.pos.x > 2.9, "{:?}", objs[2].1.pose.pos);
        // where the chain leaves the tile, the row stops (a repeater there carries it on)
        let objs = tile_row_objects(&master, &tile[..2], DVec2::ZERO, Some(&ix));
        assert_eq!(
            objs.iter().map(|(_, o)| o.index).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        // ... like this one on spline 12, which then goes on through its own tile: its own
        // copy of the row starts 20 m along the chain and reaches 600 m, so its objects lie at
        // 20, 220, 420 and 620 m of the chain and only the last stands on this spline
        let repeater = SplineAttachment {
            id: 7,
            spline_index: 0,
            repeater: Some((3, 3)),
            ..row(200.0, 600.0, 20.0, None)
        };
        let objs = tile_row_objects(&repeater, &[s12], DVec2::ZERO, Some(&ix));
        assert_eq!(objs.len(), 1);
        assert!(
            objs[0].1.index == 3 && (objs[0].1.pose.pos.y - 620.0).abs() < 1e-6,
            "{:?}",
            objs[0].1.pose.pos
        );
        let mut gap = buffer_stop_chain();
        gap.repeater_first.insert((7, 12), 4);
        assert_eq!(
            tile_row_objects(&master, &tile[..2], DVec2::ZERO, Some(&gap)).len(),
            3
        );
        let drifted = SplineAttachment {
            id: 9,
            ..row(200.0, 600.0, 0.5, None)
        };
        gap.repeater_first.insert((9, 12), 4);
        let objs = tile_row_objects(&drifted, &tile[..2], DVec2::ZERO, Some(&gap));
        assert_eq!(
            objs.iter()
                .map(|(si, o)| (*si, o.index))
                .collect::<Vec<_>>(),
            vec![(0, 0), (0, 1), (1, 2), (1, 3)]
        );
        assert!(
            (objs[3].1.pose.pos.y - 600.0).abs() < 1e-6,
            "{:?}",
            objs[3].1.pose.pos
        );
        gap.repeater_first.insert((9, 12), 3);
        assert_eq!(
            tile_row_objects(&drifted, &tile[..2], DVec2::ZERO, Some(&gap)).len(),
            3
        );
        gap.repeater_first.clear();
        assert_eq!(
            tile_row_objects(&drifted, &tile[..2], DVec2::ZERO, Some(&gap)).len(),
            3
        );
        let to_the_border = SplineAttachment {
            id: 9,
            ..row(200.0, 600.0, 0.2, None)
        };
        let objs = tile_row_objects(&to_the_border, &tile[..2], DVec2::ZERO, Some(&gap));
        assert_eq!(
            objs.iter().map(|(_, o)| o.index).collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
        assert!(
            (objs[3].1.pose.pos.y - 600.0).abs() < 1e-6,
            "{:?}",
            objs[3].1.pose.pos
        );
        // a single object past the end of its spline goes onto the next one in the tile
        let single = SplineAttachment {
            id: 8,
            ..row(0.0, 0.0, 450.0, None)
        };
        let objs = tile_row_objects(&single, &tile, DVec2::ZERO, Some(&ix));
        assert_eq!(objs.len(), 1);
        assert!(
            objs[0].0 == 1 && (objs[0].1.pose.pos.y - 450.0).abs() < 1e-6,
            "{:?}",
            objs[0].1.pose.pos
        );
    }

    #[test]
    fn a_repeater_lays_its_own_copy_of_the_row() {
        // Every record of a row lays the row out itself, from its own copy of the parameters,
        // so a tile is drawn without the tile that holds the record the row was attached on.
        // The tile index a repeater stores names the master's tile as it was when the record
        // was written; it is not consulted, so a tile added to or removed from global.cfg's
        // `[map]` list since then (32 of Berlin-Spandau's 213 repeaters) cannot move the row
        // into another tile's chain. Grundorf Island 4's bridge rows carry a copy that has
        // drifted from the record on the bridge's first spline (start 25 m along the chain and
        // interval 60 m where that one says 0 m and 55 m): the row goes where its own record
        // says, as Omsi.exe places it.
        let ix = buffer_stop_chain();
        // this spline starts 600 m along the chain, so the objects the record's copy puts at
        // 20, 220, 420, 620 and 820 m of the chain leave 620 and 820 on it
        let tile = [MapSpline {
            pos: [0.0, 600.0, 10.0],
            ..spline(12, 11, 0, 250.0)
        }];
        // the record names tile 9, which holds no master of id 7
        let repeater = SplineAttachment {
            id: 7,
            spline_index: 0,
            repeater: Some((9, 3)),
            ..row(200.0, 2000.0, 20.0, None)
        };
        let objs = tile_row_objects(&repeater, &tile, DVec2::ZERO, Some(&ix));
        assert_eq!(
            objs.len(),
            2,
            "{:?}",
            objs.iter()
                .map(|(_, o)| (o.index, o.pose.pos.y))
                .collect::<Vec<_>>()
        );
        assert_eq!(objs[0].1.index, 3);
        assert!(
            (objs[0].1.pose.pos.y - 620.0).abs() < 1e-6,
            "{:?}",
            objs[0].1.pose.pos
        );
        assert_eq!(objs[1].1.index, 4);
        assert!(
            (objs[1].1.pose.pos.y - 820.0).abs() < 1e-6,
            "{:?}",
            objs[1].1.pose.pos
        );
        // the record's own spacing lays the row out: with 130 m between two objects it is at
        // 20, 150, 280, 410, 540, 670 and 800 m of the chain, so two stand on this spline
        let own = SplineAttachment {
            id: 7,
            spline_index: 0,
            repeater: Some((9, 5)),
            ..row(130.0, 2000.0, 20.0, None)
        };
        let objs = tile_row_objects(&own, &tile, DVec2::ZERO, Some(&ix));
        assert_eq!(
            objs.iter().map(|(_, o)| o.index).collect::<Vec<_>>(),
            vec![5, 6]
        );
        assert!(
            (objs[0].1.pose.pos.y - 670.0).abs() < 1e-6,
            "{:?}",
            objs[0].1.pose.pos
        );
        assert!(
            (objs[1].1.pose.pos.y - 800.0).abs() < 1e-6,
            "{:?}",
            objs[1].1.pose.pos
        );
    }

    #[test]
    fn repeater_continues_the_row() {
        let mut ix = MapIndex::default();
        ix.splines.insert(
            1,
            IndexedSpline {
                length: 100.0,
                map_chain_offset: None,
                prev: 0,
                next: 2,
            },
        );
        ix.splines.insert(
            2,
            IndexedSpline {
                length: 50.0,
                map_chain_offset: None,
                prev: 1,
                next: 3,
            },
        );
        // spline 3 is joined end to end: it runs against the chain
        ix.splines.insert(
            3,
            IndexedSpline {
                length: 80.0,
                map_chain_offset: None,
                prev: 0,
                next: 2,
            },
        );
        // the master row: 20, 50, 80 m on spline 1
        let s1 = spline(1, 0, 2, 100.0);
        assert_eq!(
            row_objects(&row(30.0, 1000.0, 20.0, None), &s1, DVec2::ZERO, None).len(),
            3
        );
        // spline 2 (100..150 m of the chain): objects 3 (110 m) and 4 (140 m)
        let s2 = MapSpline {
            pos: [0.0, 100.0, 10.0],
            ..spline(2, 1, 3, 50.0)
        };
        let objs = row_objects(
            &row(30.0, 1000.0, 20.0, Some((0, 3))),
            &s2,
            DVec2::ZERO,
            Some(&ix),
        );
        assert_eq!(objs.iter().map(|o| o.index).collect::<Vec<_>>(), vec![3, 4]);
        assert!(
            (objs[0].pose.pos.y - 110.0).abs() < 1e-6 && (objs[1].pose.pos.y - 140.0).abs() < 1e-6
        );
        // on a spline that runs against the chain the record's own copy still starts where it
        // says: 20 m along this spline, then every 30 m
        let s3 = MapSpline {
            pos: [0.0, 230.0, 10.0],
            heading: 180.0,
            ..spline(3, 0, 2, 80.0)
        };
        let objs = row_objects(
            &row(30.0, 1000.0, 20.0, Some((0, 5))),
            &s3,
            DVec2::ZERO,
            Some(&ix),
        );
        assert_eq!(
            objs.iter().map(|o| o.index).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert!(
            (objs[0].pose.pos.y - 210.0).abs() < 1e-6,
            "{:?}",
            objs[0].pose.pos
        );
        assert!(
            (objs[2].pose.pos.y - 150.0).abs() < 1e-6,
            "{:?}",
            objs[2].pose.pos
        );
    }

    #[test]
    fn stop_exit_weights_as_omsi_reads_them() {
        let v = |a: &[&str]| stop_exit_weight(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        // the stock stops: name, enter max, enter min, exit
        assert_eq!(v(&["Krankenhaus", "0", "0", "10", "", "", ""]), 10.0);
        // no exit number: the mean of the two entering numbers
        assert_eq!(v(&["A", "6", "2"]), 4.0);
        assert_eq!(v(&["A", "", ""]), 0.5);
        // rounded, never below 0, rubbish ignored
        assert_eq!(v(&["A", "1", "0", "2.6"]), 3.0);
        assert_eq!(v(&["A", "1", "0", "-4"]), 0.0);
        assert_eq!(v(&["A", "1", "0", "x"]), 0.5);
    }

    #[test]
    fn stop_side_as_omsi_reads_it() {
        let v = |a: &[&str]| stop_side(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        // Urumqi61's shape: name, enter max, enter min, exit, ?, side, "", ""
        assert_eq!(v(&["NianZiGou", "10", "0", "", "80", "1", "", ""]), 1.0);
        assert_eq!(
            v(&["RenMinGuangChang", "50", "20", "100", "80", "0", "", ""]),
            0.0
        );
        // both sides
        assert_eq!(v(&["A", "10", "0", "", "30", "2", "", ""]), 2.0);
        // nothing said / rubbish / a short block: the right-hand side, as OMSI's default
        assert_eq!(v(&["A", "10", "0", "", "30"]), 0.0);
        assert_eq!(v(&["A", "10", "0", "", "30", "", "", ""]), 0.0);
        assert_eq!(
            v(&["bss1\\11.jpg", "bss1\\6.jpg", "", "", "", "", "", ""]),
            0.0
        );
        assert_eq!(v(&["A", "10", "0", "", "30", "x", "", ""]), 0.0);
        // a value out of range is clamped, not trusted into a side that does not exist
        assert_eq!(v(&["A", "10", "0", "", "30", "80", "", ""]), 2.0);
    }

    #[test]
    fn attachment_order() {
        let a = omsi_scenery::sco::Attachment {
            ops: vec![
                ("attach_rot_z".into(), vec![180.0]),
                ("attach_trans".into(), vec![-4.0, 0.4, 5.0]),
            ],
        };
        let m = attachment_matrix(&a);
        let p = m.transform_point3(Vec3::ZERO);
        assert!((p - Vec3::new(-4.0, 0.4, 5.0)).length() < 1e-5, "{p:?}");
        // the attached object is turned round, its forward axis points backwards
        assert!((m.transform_vector3(Vec3::Y) - Vec3::new(0.0, -1.0, 0.0)).length() < 1e-5);
        let parent = Pose {
            pos: DVec3::new(100.0, 200.0, 30.0),
            rot: omsi_geometry::object_rotation([90.0, 0.0, 0.0]),
        };
        let child = parent.attached(&m, [0.0; 3]);
        // parent faces east: its left (-x) is north
        assert!(
            (child.pos - DVec3::new(100.4, 204.0, 35.0)).length() < 1e-4,
            "{:?}",
            child.pos
        );
        assert!(
            (child.heading() - 270.0).abs() < 1e-3,
            "{}",
            child.heading()
        );
    }
}
