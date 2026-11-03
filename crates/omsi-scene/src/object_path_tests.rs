//! Artificial object paths.
use super::*;
use omsi_sim::ai_motion::{AiBody, MotionKind};
use omsi_sim::traffic::{AiState, Network};
use omsi_vehicle::{Axle, Vehicle};

fn path(
    keyword: &str,
    start: DVec3,
    heading: f64,
    radius: f64,
    length: f64,
    gradients: [f64; 2],
    direction: i32,
) -> String {
    let extra = if keyword == "path_2" { "0\n0\n" } else { "" };
    format!("[{keyword}]\n{}\n{}\n{}\n{heading}\n{radius}\n{length}\n{}\n{}\n0\n3\n{direction}\n0\n{extra}", start.x, start.y, start.z, gradients[0], gradients[1])
}

fn lanes(text: &str, pos: DVec3, heading: f64) -> Vec<Lane> {
    let sco = SceneryObject::parse(&omsi_cfg::CfgFile::from_str("artificial.sco", text));
    object_lanes(&sco, pos, [heading, 0.0, 0.0], None, (0, 0), 1, &[])
}

static PUBLISHER_WORLD_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct PublisherMapFixture {
    root: PathBuf,
    signer: omsi_pack::SigningKey,
    trusted: omsi_pack::TrustedPublishers,
    encrypted_packs: Vec<PathBuf>,
}

impl PublisherMapFixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let root = std::env::temp_dir().join(format!(
            "openomsi-scene-publisher-map-{}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(root.join("Packs")).unwrap();
        let signer = omsi_pack::SigningKey::generate(&ring::rand::SystemRandom::new()).unwrap();
        let mut trust = omsi_pack::TrustedPublishers::default();
        trust.insert("fixture", signer.key_id(), signer.public_key());
        trust.save_file(&root.join("Publishers.json")).unwrap();
        let mut fixture = Self { root, signer, trusted: trust, encrypted_packs: Vec::new() };
        fixture.add_map("Fixture", "scene-types", "plain map alias", 10.0, 0.0, 4, None);
        omsi_cfg::add_content_root(fixture.root.clone());
        fixture
    }

    fn add_map(
        &mut self,
        map_name: &str,
        pack_id: &str,
        friendly_name: &str,
        spline_length: f32,
        path_offset: f32,
        texture_surface: u8,
        protected_key: Option<[u8; 32]>,
    ) {
        let map = self.root.join("maps").join(map_name);
        std::fs::create_dir_all(&map).unwrap();
        std::fs::write(
            map.join("global.cfg"),
            "[name]\nPublisher alias fixture\n[map]\n0\n0\ntiles\\sector\\tile_0_0.map\n",
        )
        .unwrap();
        let tile = map.join("tiles/sector/tile_0_0.map");
        std::fs::create_dir_all(tile.parent().unwrap()).unwrap();
        std::fs::write(
            tile,
            "[version]\n14\n\n[spline]\n0\nSplines\\Fixture\\renamed.sli\n1\n0\n0\n0\n0\n0\n0\n10\n0\n0\n0\n0\n0\n0\n0\n0\n\n",
        )
        .unwrap();
        let pack = self.root.join("Packs").join(format!("{pack_id}.oopp"));
        let key_id = protected_key.as_ref().map(|key| omsi_pack::sha256_hex(key));
        let spline = format!(
            "[length]\n{spline_length}\n[path]\n0\n{path_offset}\n0\n3\n1\n[texture]\nroad.bmp\n[texture]\nglobal.png\n"
        );
        let texture_cfg = format!("[surface]\n{texture_surface}\n");
        let global_texture_cfg = format!("[surface]\n{}\n", texture_surface + 1);
        let global_texel = [180, 32, 76, 255];
        let mut global_png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(2, 2, image::Rgba(global_texel)))
            .write_to(&mut global_png, image::ImageFormat::Png)
            .unwrap();
        let assets = [
            omsi_pack::AssetInput {
                id: "object".into(),
                path: "Sceneryobjects/Fixture/original.sco".into(),
                source_sha256: "11".repeat(32),
                bytes: format!("[friendlyname]\n{friendly_name}\n").into_bytes(),
            },
            omsi_pack::AssetInput {
                id: "spline".into(),
                path: "Splines/Fixture/original.sli".into(),
                source_sha256: "22".repeat(32),
                bytes: spline.into_bytes(),
            },
            omsi_pack::AssetInput {
                id: "spline-texture".into(),
                path: "Splines/Fixture/texture/road.bmp".into(),
                source_sha256: "33".repeat(32),
                bytes: b"synthetic texture placeholder".to_vec(),
            },
            omsi_pack::AssetInput {
                id: "spline-texture-cfg".into(),
                path: "Splines/Fixture/texture/road.bmp.cfg".into(),
                source_sha256: "44".repeat(32),
                bytes: texture_cfg.into_bytes(),
            },
            omsi_pack::AssetInput {
                id: "global-texture".into(),
                path: "Texture/global.png".into(),
                source_sha256: "55".repeat(32),
                bytes: global_png.into_inner(),
            },
            omsi_pack::AssetInput {
                id: "global-texture-cfg".into(),
                path: "Texture/global.png.cfg".into(),
                source_sha256: "66".repeat(32),
                bytes: global_texture_cfg.into_bytes(),
            },
        ];
        let manifest = omsi_pack::create_pack(
            &pack,
            omsi_pack::PackMetadata {
                publisher_id: "fixture".into(),
                author: "Fixture".into(),
                pack_id: pack_id.into(),
                version: "1.0.0".into(),
                license: "CC0-1.0".into(),
                redistribution: false,
                key_id,
                dependencies: Vec::new(),
            },
            &assets,
            &self.signer,
            protected_key.as_ref(),
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
        if let Some(key) = protected_key {
            let expires = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 3600;
            let entitlement = omsi_pack::issue_entitlement(&manifest, key, expires, &self.signer).unwrap();
            omsi_pack::unlock(&pack, &entitlement, &self.trusted, &omsi_pack::SystemKeyStore).unwrap();
            // The native entitlement is unique to this disposable fixture pack.
            self.encrypted_packs.push(pack);
        }
        std::fs::write(
            map.join("dependencies.json"),
            serde_json::json!({
                "format_version": 1,
                "dependencies": [{"publisher_id":"fixture","pack_id":pack_id,"version":"1.0.0"}],
                "assets": [
                    {"path":"Sceneryobjects/Fixture/renamed.sco","dependency":{"publisher_id":"fixture","pack_id":pack_id,"version":"1.0.0"},"asset_id":"object"},
                    {"path":"Splines/Fixture/renamed.sli","dependency":{"publisher_id":"fixture","pack_id":pack_id,"version":"1.0.0"},"asset_id":"spline"}
                ]
            })
            .to_string(),
        )
        .unwrap();
    }

    fn map_dir(&self) -> PathBuf {
        self.root.join("maps/Fixture")
    }

    fn map_dir_named(&self, name: &str) -> PathBuf {
        self.root.join("maps").join(name)
    }
}

impl Drop for PublisherMapFixture {
    fn drop(&mut self) {
        omsi_cfg::remove_content_root(&self.root);
        for pack in &self.encrypted_packs {
            let _ = omsi_pack::forget_unlock(pack, &self.trusted, &omsi_pack::SystemKeyStore);
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn world_loaders_resolve_publisher_aliases_from_the_map_scope() {
    let _guard = PUBLISHER_WORLD_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let fixture = PublisherMapFixture::new();
    let map_dir = fixture.map_dir();
    let world = World::open(&fixture.root, &map_dir.join("global.cfg"), 20261001).unwrap();

    assert_eq!(
        world.object_type("Sceneryobjects/Fixture/renamed.sco").unwrap().sco.friendly_name,
        "plain map alias"
    );
    let spline = world.spline_type("Splines/Fixture/renamed.sli").unwrap();
    assert_eq!(spline.def.length, 10.0);
    assert_eq!(spline.surface, vec![4, 5]);
    let object = world.object_type("Sceneryobjects/Fixture/renamed.sco").unwrap();
    let dirs = world.object_texture_dirs(&object);
    let dirs: Vec<&Path> = dirs.iter().map(PathBuf::as_path).collect();
    let texture = omsi_texture::find_texture("global.png", &dirs).unwrap();
    assert_eq!(texture, omsi_cfg::resolve_path(&map_dir, "Texture/global.png"));
    let decoded = omsi_texture::decode_file(&texture).unwrap();
    assert_eq!((decoded.width, decoded.height), (2, 2));
    assert_eq!(&decoded.rgba[..4], &[180, 32, 76, 255]);
    assert!(!texture.exists(), "global publisher texture must remain virtual");
    assert!(!world.navigation_map().lanes.is_empty(), "navigation must load map-scoped signed splines");

    let fallback = fixture.root.join("Sceneryobjects/Ordinary/fallback.sco");
    std::fs::create_dir_all(fallback.parent().unwrap()).unwrap();
    std::fs::write(&fallback, "[friendlyname]\nordinary physical fallback\n").unwrap();
    assert_eq!(
        world.object_type("Sceneryobjects/Ordinary/fallback.sco").unwrap().sco.friendly_name,
        "ordinary physical fallback"
    );
}

#[test]
fn publisher_worlds_keep_their_lease_when_the_same_map_is_reopened() {
    let _guard = PUBLISHER_WORLD_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut fixture = PublisherMapFixture::new();
    let map = fixture.map_dir().join("global.cfg");
    let first = World::open(&fixture.root, &map, 20261001).unwrap();
    fixture.add_map("Fixture", "second-session", "second map session", 30.0, 6.0, 8, None);
    let second = World::open(&fixture.root, &map, 20261001).unwrap();

    // Load after both leases exist so a cache cannot hide resolution through the wrong lease.
    let alias = "Sceneryobjects/Fixture/renamed.sco";
    assert_eq!(first.object_type(alias).unwrap().sco.friendly_name, "plain map alias");
    assert_eq!(second.object_type(alias).unwrap().sco.friendly_name, "second map session");
    assert_eq!(first.spline_type("Splines/Fixture/renamed.sli").unwrap().def.length, 10.0);
    assert_eq!(second.spline_type("Splines/Fixture/renamed.sli").unwrap().def.length, 30.0);
}

#[cfg(any(target_os = "macos", windows))]
#[test]
fn encrypted_and_plain_publisher_aliases_are_isolated_by_world_map_scope() {
    let _guard = PUBLISHER_WORLD_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut fixture = PublisherMapFixture::new();
    let key = omsi_pack::random_content_key().unwrap();
    fixture.add_map("Encrypted", "encrypted-scene", "encrypted map alias", 22.0, 4.0, 5, Some(key));

    let plain_dir = fixture.map_dir();
    let encrypted_dir = fixture.map_dir_named("Encrypted");
    let plain = World::open(&fixture.root, &plain_dir.join("global.cfg"), 20261001).unwrap();
    let encrypted = World::open(&fixture.root, &encrypted_dir.join("global.cfg"), 20261001).unwrap();

    assert_eq!(
        plain.object_type("Sceneryobjects/Fixture/renamed.sco").unwrap().sco.friendly_name,
        "plain map alias"
    );
    assert_eq!(
        encrypted.object_type("Sceneryobjects/Fixture/renamed.sco").unwrap().sco.friendly_name,
        "encrypted map alias"
    );
    let plain_spline = plain.spline_type("Splines/Fixture/renamed.sli").unwrap();
    let encrypted_spline = encrypted.spline_type("Splines/Fixture/renamed.sli").unwrap();
    assert_eq!(plain_spline.def.length, 10.0);
    assert_eq!(encrypted_spline.def.length, 22.0);
    assert_eq!(plain_spline.surface, vec![4, 5]);
    assert_eq!(encrypted_spline.surface, vec![5, 6]);
    let encrypted_object = encrypted.object_type("Sceneryobjects/Fixture/renamed.sco").unwrap();
    let dirs = encrypted.object_texture_dirs(&encrypted_object);
    let dirs: Vec<&Path> = dirs.iter().map(PathBuf::as_path).collect();
    assert_eq!(
        omsi_texture::find_texture("global.png", &dirs),
        Some(omsi_cfg::resolve_path(&encrypted_dir, "Texture/global.png"))
    );
    let texture = omsi_cfg::resolve_path(&encrypted_dir, "Texture/global.png");
    let decoded = omsi_texture::decode_file(&texture).unwrap();
    assert_eq!(&decoded.rgba[..4], &[180, 32, 76, 255]);
    assert!(!texture.exists(), "encrypted global texture must remain virtual");
    let plain_navigation = plain.navigation_map();
    let encrypted_navigation = encrypted.navigation_map();
    assert!(!plain_navigation.lanes.is_empty());
    assert!(!encrypted_navigation.lanes.is_empty());
    assert!((plain_navigation.lanes[0].points[0].x - 0.0).abs() < 1e-6);
    assert!((encrypted_navigation.lanes[0].points[0].x - 4.0).abs() < 1e-6);
}

#[test]
fn object_path_gradients_preserve_heights_and_reverse_samples() {
    // The expected heights are independent hand-calculated checkpoints, including
    // a crest whose end alone cannot distinguish a straight line from its profile.
    for keyword in ["path", "path_2"] {
        for (gradients, expected) in [
            ([0.0, 0.0], [5.0, 5.0, 5.0]),
            ([0.04, 0.04], [5.0, 5.4, 5.8]),
            ([-0.03, -0.03], [5.0, 4.7, 4.4]),
            ([0.06, -0.04], [5.0, 5.35, 5.2]),
        ] {
            for radius in [0.0, 40.0, -40.0] {
                let text = path(
                    keyword,
                    DVec3::new(2.0, 3.0, 5.0),
                    25.0,
                    radius,
                    20.0,
                    gradients,
                    2,
                );
                let ls = lanes(&text, DVec3::new(100.0, -80.0, 7.0), 63.0);
                assert_eq!(ls.len(), 2);
                for (i, z) in [(0, expected[0]), (5, expected[1]), (10, expected[2])] {
                    assert!(
                        (ls[0].points[i].z - (z + 7.0)).abs() < 1e-6,
                        "{keyword} {gradients:?} radius {radius}: {:?}",
                        ls[0].points[i]
                    );
                }
                let h = 63f64.to_radians();
                let start = DVec3::new(
                    100.0 + 2.0 * h.cos() + 3.0 * h.sin(),
                    -80.0 - 2.0 * h.sin() + 3.0 * h.cos(),
                    12.0,
                );
                let horizontal =
                    LaneBuilder::arc(start, 88.0, 20.0, radius, 0.0, LaneKind::Street, 3.0);
                for (p, q) in ls[0].points.iter().zip(&horizontal.points) {
                    assert!((p.truncate() - q.truncate()).length() < 1e-6);
                }
                assert!(ls[1].reversed);
                assert_eq!(
                    ls[1].points,
                    ls[0].points.iter().rev().copied().collect::<Vec<_>>()
                );
            }
        }
    }
}

#[test]
fn descending_object_path_joins_do_not_lift_the_rear_axle() {
    for keyword in ["path", "path_2"] {
        for reverse in [false, true] {
            // Long paths alternate with short ones. The real road descends smoothly,
            // but treating a gradient as a height change leaves the long lane far above it.
            let mut text = String::new();
            let mut y = 0.0;
            for length in [48.0, 4.0, 36.0, 4.0, 36.0] {
                text.push_str(&path(
                    keyword,
                    DVec3::new(0.0, y, 10.0 - 0.03 * y),
                    0.0,
                    0.0,
                    length,
                    [-0.03, -0.03],
                    reverse as i32,
                ));
                y += length;
            }
            let mut net = Network {
                lanes: lanes(&text, DVec3::ZERO, 0.0),
                ..Default::default()
            };
            if reverse {
                net.lanes.reverse();
            }
            for i in 0..net.lanes.len() - 1 {
                net.lanes[i].next = vec![i + 1];
            }
            let mut state = AiState::new(0, 5.0, 1);
            state.plan_next(&net);
            let mut def = Vehicle {
                mass: 1.2,
                moment_of_inertia: [1.6, 0.5, 1.8],
                cog_height: 0.45,
                rot_pnt_long: -1.5,
                ..Default::default()
            };
            def.axles = [1.5, -1.5]
                .into_iter()
                .map(|long| Axle {
                    long,
                    max_width: 1.6,
                    spring: 45.0,
                    damper: 3.0,
                    wheel_diameter: 0.6,
                    ..Default::default()
                })
                .collect();
            let ground = |_x: f64, y: f64, top: f64| {
                let z = 10.0 - 0.03 * y;
                omsi_sim::rigid::GroundProbe {
                    below: (z <= top).then_some(z),
                    above: None,
                }
            };
            let mut body = AiBody::new(&def, MotionKind::Road);
            body.place(&|d| state.way_point(&net, d), None, Some(&ground), 6.0);
            let mut worst_gap = 0.0f64;
            let mut worst_pitch = 0.0f32;
            for _ in 0..550 {
                state.s += 0.2;
                if state.s >= net.lanes[state.lane].length() {
                    state.s -= net.lanes[state.lane].length();
                    state.prev_lane = Some(state.lane);
                    state.lane += 1;
                    state.planned_next = None;
                    state.ahead.clear();
                    state.plan_next(&net);
                }
                body.step(
                    1.0 / 30.0,
                    6.0,
                    &|d| state.way_point(&net, d),
                    None,
                    Some(&ground),
                );
                let slope = if reverse { 0.03f32 } else { -0.03f32 };
                worst_pitch = worst_pitch.max((body.pitch_deg - slope.atan().to_degrees()).abs());
                for (i, a) in def.axles.iter().enumerate() {
                    let wheel_y = body.position.y + body.heading.to_radians().cos() * a.long as f64;
                    let contact = body.position.z
                        + (a.long * body.pitch_deg.to_radians().tan() - body.suspension[i][0])
                            as f64;
                    worst_gap = worst_gap.max((contact - (10.0 - 0.03 * wheel_y)).abs());
                }
            }
            assert!(
                worst_gap < 0.02,
                "{keyword}: axle contact left the road by {worst_gap:.3} m"
            );
            assert!(
                worst_pitch < 0.5,
                "{keyword}: pitch departed from the slope by {worst_pitch:.2} degrees"
            );
        }
    }
}
