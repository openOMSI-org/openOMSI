#[test]
fn a_full_beam_reaches_further_than_the_low_beam() {
    // the stock buses: low beam 100, full beam 500 (#941)
    assert_eq!(super::spot_reach(100.0, 45.0), 45.0);
    assert_eq!(super::spot_reach(500.0, 45.0), 225.0);
    assert_eq!(super::spot_reach(500.0, 60.0), 300.0);
    // short ranges as declared, as before
    assert_eq!(super::spot_reach(30.0, 45.0), 30.0);
    assert_eq!(super::spot_reach(2.0, 45.0), 2.0);
    assert_eq!(super::spot_reach(0.1, 45.0), 0.5);
    assert_eq!(super::spot_reach(5000.0, 45.0), 225.0);
}

/// The enhanced picture lights a `[spotlight]` as a low beam below the full beam's range
/// (the stock 100, the Grand Paris-Moulon Citaro's 200) and as a full beam from it on.
#[test]
fn a_spotlight_is_a_low_or_a_full_beam_by_its_range() {
    let beam = |range: f32| {
        let vals = [0.0, 6.5, 0.76, 0.0, 1.0, -0.3, 255.0, 255.0, 233.0, range, 30.0, 80.0];
        let mut lights = Vec::new();
        super::push_spot(&mut lights, glam::DVec3::ZERO, glam::Vec3::Y, glam::Vec3::Y, &vals, 0.5);
        lights.iter().find(|l| l.mode == omsi_render::LightMode::Enhanced).map(|l| l.beam)
    };
    assert_eq!([100.0, 200.0, 450.0, 500.0].map(beam), [Some(1.0), Some(1.0), Some(-1.0), Some(-1.0)]);
    // a lamp over a door, pointing down, keeps its cone
    let vals = [0.0, 6.5, 2.5, 0.0, 0.0, -1.0, 255.0, 255.0, 233.0, 100.0, 30.0, 80.0];
    let mut lights = Vec::new();
    super::push_spot(&mut lights, glam::DVec3::ZERO, glam::Vec3::NEG_Z, glam::Vec3::ZERO, &vals, 0.5);
    assert_eq!(lights.iter().find(|l| l.mode == omsi_render::LightMode::Enhanced).map(|l| l.beam), Some(0.0));
}

/// The classic picture's `[spotlight]` is Omsi.exe's Direct3D spot (0x708acc): one light,
/// its colour 0..1, its range as declared for the shader's attenuation, its cone's half
/// angles, as strong by day as by night; a `[spotlight_2]` is lit the same way per pixel.
#[test]
fn a_spotlight_is_omsis_direct3d_spot_in_the_classic_picture() {
    let vals = [0.0, 2.0, 0.863, 0.0, 1.0, -0.3, 255.0, 255.0, 233.0, 100.0, 30.0, 70.0];
    let mut lights = Vec::new();
    let source = glam::DVec3::new(0.0, -3.9, 0.0);
    super::push_classic_spot(&mut lights, glam::DVec3::ZERO, glam::Vec3::Y, &vals, 1.0, source, true);
    assert_eq!(lights.len(), 1);
    let l = &lights[0];
    assert_eq!(l.mode, omsi_render::LightMode::Vanilla);
    assert_eq!(l.d3d_spot, Some(omsi_render::D3dSpot { range: 100.0, source, player: true }));
    assert_eq!((l.intensity, l.radius), (1.0, super::VANILLA_SPOT_REACH));
    assert_eq!(l.color, [1.0, 1.0, 233.0 / 255.0]);
    assert!((l.cone[0] - 15f32.to_radians().cos()).abs() < 1e-6 && (l.cone[1] - 35f32.to_radians().cos()).abs() < 1e-6, "{:?}", l.cone);
    // (a short range: four times it, as Direct3D cuts the light off)
    let mut short = vals;
    short[9] = 5.0;
    let mut lights = Vec::new();
    super::push_spot(&mut lights, glam::DVec3::ZERO, glam::Vec3::Y, glam::Vec3::Y, &short, 0.5);
    let van = lights.iter().find(|l| l.mode == omsi_render::LightMode::Vanilla).unwrap();
    assert_eq!((van.radius, van.intensity, van.d3d_spot), (20.0, 0.5, Some(omsi_render::D3dSpot { range: 5.0, source: glam::DVec3::ZERO, player: false })));
}

/// A headlamp's beam lies about the vehicle's floor: on a bus pitched up a ramp its
/// Enhanced light is aimed along the ramp, whatever the lamp's own aim down (#1897).
#[test]
fn a_headlamp_beam_pitches_with_its_vehicle() {
    let vals = [0.0, 6.5, 0.76, 0.0, 1.0, -0.3, 255.0, 255.0, 233.0, 100.0, 30.0, 80.0];
    let pitch = 8f32.to_radians();
    let body = glam::Mat4::from_rotation_x(pitch);
    let dl = glam::Vec3::new(0.0, 1.0, -0.3).normalize();
    let d = body.transform_vector3(dl);
    let mut lights = Vec::new();
    super::push_spot(&mut lights, glam::DVec3::ZERO, d, super::level_axis(body, dl), &vals, 0.5);
    let enh = lights.iter().find(|l| l.mode == omsi_render::LightMode::Enhanced).unwrap();
    assert_eq!(enh.beam, 1.0);
    assert!((enh.direction.z - pitch.sin()).abs() < 1e-4 && (enh.direction.y - pitch.cos()).abs() < 1e-4, "{:?}", enh.direction);
    // (the classic spot keeps the lamp's own aim)
    let van = lights.iter().find(|l| l.mode == omsi_render::LightMode::Vanilla).unwrap();
    assert!((van.direction - d).length() < 1e-5);
}

/// `[spotlight_2]`: a pair mirrored across the axis sharing the light, or one lamp, as
/// bright as its variable says, and nothing while that is off.
#[test]
fn a_spotlight_2_is_a_mirrored_pair_or_one_lamp() {
    use glam::Vec3;
    let mut sp = omsi_model::Spotlight2 {
        values: [0.9, 5.9, 0.65, 0.2, 1.0, -0.3, 255.0, 255.0, 233.0, 200.0, 30.0, 80.0],
        variable: "lights_fern".into(),
        mirrored: true,
    };
    let pair = super::spotlight_2_lamps(&sp, 1.0);
    assert_eq!(
        pair,
        [
            (Vec3::new(0.9, 5.9, 0.65), Vec3::new(0.2, 1.0, -0.3), 0.5),
            (Vec3::new(-0.9, 5.9, 0.65), Vec3::new(-0.2, 1.0, -0.3), 0.5),
        ]
    );
    assert!(super::spotlight_2_lamps(&sp, 0.0).is_empty());
    assert!(super::spotlight_2_lamps(&sp, -1.0).is_empty());
    sp.mirrored = false;
    assert_eq!(super::spotlight_2_lamps(&sp, 0.5), [(Vec3::new(0.9, 5.9, 0.65), Vec3::new(0.2, 1.0, -0.3), 0.5)]);
    assert_eq!(super::spotlight_2_lamps(&sp, 3.0)[0].2, 1.0);
}
#[test]
fn a_spotlight_shines_from_its_headlamps() {
    use glam::Vec3;
    let lamp = |x: f32, y: f32, z: f32, variable: &str| omsi_model::LightEnh2 {
        pos: [x, y, z],
        dir: [0.0, 1.0, 0.0],
        color: [255.0, 255.0, 240.0],
        variable: variable.into(),
        ..Default::default()
    };
    let mesh = |light_enh_2: Vec<omsi_model::LightEnh2>| omsi_model::Model { meshes: vec![omsi_model::MeshDef { light_enh_2, ..Default::default() }], ..Default::default() };
    let model = mesh(vec![
        lamp(-0.8, 5.8, 1.05, "abbl_l"),
        lamp(0.8, 5.8, 1.05, "abbl_r"),
        lamp(-0.8, 5.8, 1.2, "fern"),
        lamp(0.8, 5.8, 1.2, "fern"),
        lamp(-0.7, 5.8, 0.6, "nebel"),
        lamp(0.7, 5.8, 0.6, "nebel"),
        lamp(0.0, 5.8, 1.0, "logo"),
        lamp(-1.1, -5.8, 1.4, "stand"),
        lamp(1.1, -5.8, 1.4, "stand"),
        omsi_model::LightEnh2 { color: [255.0, 160.0, 0.0], ..lamp(-1.1, 5.8, 1.05, "abbl_l") },
    ]);
    let bb = Some([2.5, 12.0, 3.0, 0.0, 0.0, 1.5]);
    let low = [0.0, 2.0, 0.76, 0.0, 1.0, -0.3, 255.0, 255.0, 233.0, 100.0, 30.0, 80.0];
    let at = |model: &omsi_model::Model, on: &[&str], fades: &[f32], vals: &[f32; 12]| {
        let on: Vec<String> = on.iter().map(|s| s.to_string()).collect();
        let mut at = super::spot_origins(model, bb, vals, fades, &|name: &str| if on.iter().any(|o| o == name) { 1.0 } else { 0.0 });
        at.sort_by(|a, b| a.0.x.total_cmp(&b.0.x));
        at
    };
    let near = |got: &[(Vec3, f32)], want: &[(Vec3, f32)]| got.len() == want.len() && got.iter().zip(want).all(|(g, w)| (g.0 - w.0).length() < 1e-4 && g.1 == w.1);
    let pair = |z: f32| [(Vec3::new(-0.8, 5.9, z), 0.5), (Vec3::new(0.8, 5.9, z), 0.5)];
    let got = at(&model, &["abbl_l", "abbl_r", "stand"], &[], &low);
    assert!(near(&got, &pair(1.05)), "low beam, rear markers lit: {got:?}");
    let got = at(&model, &["stand"], &[], &low);
    assert!(near(&got, &pair(1.2)), "rear markers alone: {got:?}");
    let got = at(&model, &["abbl_l", "abbl_r", "nebel"], &[], &low);
    assert!(near(&got, &pair(1.05)), "fog lamps on: {got:?}");
    let got = at(&model, &["abbl_l", "abbl_r", "fern"], &[], &low);
    assert!(near(&got, &pair(1.2)), "full beam: {got:?}");
    let got = at(&model, &["abbl_l"], &[], &low);
    assert!(near(&got, &pair(1.05)), "one bulb out: {got:?}");
    let got = at(&model, &[], &[0.4, 0.4, 0.0, 0.0, 0.0, 0.0, 0.0], &low);
    assert!(near(&got, &pair(1.05)), "low beam fading in: {got:?}");
    let got = at(&model, &["abbl_l", "abbl_r"], &[0.0, 0.0, 1.0, 1.0], &low);
    assert!(near(&got, &pair(1.2)), "the fades rule over the variables: {got:?}");
    let dim = mesh(vec![omsi_model::LightEnh2 { factor: 0.3, ..lamp(-0.8, 5.8, 1.05, "abbl") }, omsi_model::LightEnh2 { factor: 0.3, ..lamp(0.8, 5.8, 1.05, "abbl") }, lamp(-0.7, 5.8, 0.6, "drl"), lamp(0.7, 5.8, 0.6, "drl")]);
    let got = at(&dim, &["abbl", "drl"], &[], &low);
    assert!(near(&got, &pair(1.05)), "a faint headlamp glow over a DRL: {got:?}");
    let rear = [0.0, -5.0, 0.76, 0.0, -1.0, -0.3, 255.0, 255.0, 233.0, 100.0, 30.0, 80.0];
    let got = at(&model, &["abbl_l", "abbl_r"], &[], &rear);
    assert!(got.iter().all(|(p, _)| p.z == 0.76 && (p.y + 5.85).abs() < 1e-4), "a rear spot keeps its own height: {got:?}");
    let inside = mesh(vec![lamp(-0.5, 2.0, 1.0, "x"), lamp(0.5, 2.0, 1.0, "x"), omsi_model::LightEnh2 { color: [255.0, 190.0, 0.0], ..lamp(0.9, 5.8, 0.9, "x") }]);
    let got = at(&inside, &["x"], &[], &low);
    assert!(got.iter().all(|(p, _)| p.z == 0.76 && (p.y - 5.85).abs() < 1e-4), "white lamps far behind the nose: {got:?}");
    let got = super::spot_origins(&omsi_model::Model::default(), bb, &low, &[], &|_| 0.0);
    assert!(near(&got, &[(Vec3::new(0.0, 3.55, 0.76), 1.0)]), "no lamps: {got:?}");
}
