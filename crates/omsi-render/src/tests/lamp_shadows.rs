use super::*;

fn renderer() -> Renderer {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions {
            msaa: 1,
            ssao: false,
            shadow_size: 1024,
            fxaa: false,
            render_scale: 1.0,
            light_shadows: 2,
            ..Default::default()
        },
    ))
    .expect("test renderer")
}

fn shadowed(renderer: &Renderer, scene: &mut Scene, y: f32) -> Vec<i32> {
    let (_, lamps) = renderer.prepare_lights(scene, &LightEye::at(Vec3::new(0.0, y, 1.5)), true, true);
    let mut ys: Vec<i32> = lamps.iter().map(|l| l.position.y.round() as i32).collect();
    ys.sort();
    ys
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_mirror_picture_leaves_the_street_lamps_their_shadow_maps() {
    let renderer = renderer();
    let mut scene = renderer.new_scene();
    scene.lights = (0..5)
        .map(|k| PointLight {
            position: DVec3::new(0.0, k as f64 * 10.0, 6.0),
            radius: 20.0,
            color: [1.0, 0.8, 0.5],
            core: 3.0,
            housed: true,
            mode: LightMode::Both,
            ..Default::default()
        })
        .collect();
    assert_eq!(shadowed(&renderer, &mut scene, 15.0), vec![0, 10, 20, 30, 40]);
    assert_eq!(shadowed(&renderer, &mut scene, 21.0), vec![0, 10, 20, 30, 40]);
    let _ = renderer.prepare_lights(&mut scene, &LightEye::at(Vec3::new(0.0, 21.0, 1.5)), false, false);
    assert_eq!(shadowed(&renderer, &mut scene, 21.0), vec![0, 10, 20, 30, 40]);
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn the_players_vehicle_keeps_a_lamp_behind_it_off_the_street_ahead() {
    let mut renderer = renderer();
    let mut scene = renderer.new_scene();
    let grey = renderer.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [0.8, 0.8, 0.8, 1.0],
        false,
    );
    let mesh = renderer.add_mesh(
        &mut scene,
        &MeshData {
            positions: vec![
                Vec3::new(-4.0, 4.0, -4.0),
                Vec3::new(4.0, 4.0, -4.0),
                Vec3::new(4.0, 4.0, 4.0),
                Vec3::new(-4.0, 4.0, 4.0),
            ],
            normals: vec![-Vec3::Y; 4],
            uvs: vec![glam::Vec2::ZERO; 4],
            indices: vec![0, 1, 2, 0, 2, 3],
            ranges: vec![(0, 6, 0)],
            one_sided: false,
        },
    );
    renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![grey]);
    scene.lights = vec![PointLight {
        position: DVec3::new(0.0, -6.0, 0.0),
        radius: 30.0,
        intensity: 1.0,
        core: 4.0,
        ..Default::default()
    }];
    let camera = Camera {
        position: DVec3::ZERO,
        yaw: 0.0,
        pitch: 0.0,
        roll: 0.0,
        fov_deg: 90.0,
        near: 0.1,
        far: 100.0,
    };
    let bus = Some((
        DVec3::new(-2.0, 1.0, 0.0),
        0.0,
        [4.5, 3.5, 3.0, 0.0, 0.0, 0.0],
    ));
    for (classic, enhanced) in [(true, false), (false, false), (false, true)] {
        let lighting = Lighting {
            classic,
            enhanced,
            sun_dir: -Vec3::Z,
            sun_intensity: 0.0,
            ambient: Vec3::ZERO,
            secondary: Vec3::ZERO,
            shadows: false,
            fog_density: 0.0,
            night: 1.0,
            inside: bus,
            ..Default::default()
        };
        let rgba = renderer
            .render_to_image(&mut scene, 64, 64, &camera, &lighting)
            .unwrap();
        let at = |x: usize| {
            rgba[(32 * 64 + x) * 4..][..3]
                .iter()
                .map(|&c| c as u32)
                .sum::<u32>()
        };
        let (behind, open) = (at(16), at(47));
        assert!(
            open > 60 && behind * 4 < open,
            "classic {classic}, enhanced {enhanced}: behind the vehicle {behind}, beside it {open}"
        );
    }
}

/// A square of `half` metres a side standing in the plane `normal` is the axis of, centred
/// at `at`, as a mesh.
fn quad(at: Vec3, axis: usize, half: Vec3) -> MeshData {
    let (a, b) = match axis {
        0 => (Vec3::new(0.0, half.y, 0.0), Vec3::new(0.0, 0.0, half.z)),
        1 => (Vec3::new(half.x, 0.0, 0.0), Vec3::new(0.0, 0.0, half.z)),
        _ => (Vec3::new(half.x, 0.0, 0.0), Vec3::new(0.0, half.y, 0.0)),
    };
    let mut normal = Vec3::ZERO;
    normal[axis] = 1.0;
    MeshData {
        positions: vec![at - a - b, at + a - b, at + a + b, at - a + b],
        normals: vec![normal; 4],
        uvs: vec![glam::Vec2::ZERO; 4],
        indices: vec![0, 1, 2, 0, 2, 3],
        ranges: vec![(0, 6, 0)],
        one_sided: false,
    }
}

fn night() -> Lighting {
    Lighting {
        enhanced: true,
        lamp_shadows: true,
        shadows: false,
        sun_dir: -Vec3::Z,
        sun_intensity: 0.0,
        ambient: Vec3::ZERO,
        secondary: Vec3::ZERO,
        fog_density: 0.0,
        night: 1.0,
        ..Default::default()
    }
}

fn from_above(height: f64) -> Camera {
    Camera { position: DVec3::new(0.0, 0.0, height), yaw: 0.0, pitch: -89.9, roll: 0.0, fov_deg: 60.0, near: 0.1, far: 200.0 }
}

/// The grey of the picture at the ground point (x, y) of a view from straight above, 64 pixels
/// across `2 * reach` metres.
fn ground_at(rgba: &[u8], reach: f32, x: f32, y: f32) -> f32 {
    let (px, py) = ((x / reach * 0.5 + 0.5) * 64.0, (0.5 - y / reach * 0.5) * 64.0);
    let i = (py as usize * 64 + px as usize) * 4;
    rgba[i..i + 3].iter().map(|&c| c as f32).sum::<f32>() / 3.0
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_wall_between_a_street_lamp_and_the_ground_throws_its_shadow() {
    let mut renderer = renderer();
    let mut scene = renderer.new_scene();
    let grey = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.8, 0.8, 0.8, 1.0], false);
    // the ground, and a wall across it 6 m high; the lamp at the wall's height 3 m to one side
    let ground = renderer.add_mesh(&mut scene, &quad(Vec3::ZERO, 2, Vec3::new(40.0, 40.0, 0.0)));
    let wall = renderer.add_mesh(&mut scene, &quad(Vec3::new(0.0, 0.0, 3.0), 0, Vec3::new(0.0, 10.0, 3.0)));
    renderer.add_instance(&mut scene, ground, DVec3::ZERO, Mat4::IDENTITY, vec![grey]);
    renderer.add_instance(&mut scene, wall, DVec3::ZERO, Mat4::IDENTITY, vec![grey]);
    scene.lights = vec![PointLight {
        position: DVec3::new(-3.0, 0.0, 6.0),
        radius: 40.0,
        color: [1.0, 1.0, 1.0],
        core: 6.0,
        housed: true,
        mode: LightMode::Both,
        ..Default::default()
    }];
    let rgba = renderer.render_to_image(&mut scene, 64, 64, &from_above(25.0), &night()).unwrap();
    // (the view is 29 m across at the ground: the points are in it)
    let reach = 25.0 * 2.0 * 30f32.to_radians().tan();
    let lit = ground_at(&rgba, reach, -5.0, 0.0);
    let shaded = ground_at(&rgba, reach, 5.0, 0.0);
    assert!(lit > 12.0, "the lamp lights the ground on its own side: {lit}");
    assert!(shaded * 3.0 < lit, "behind the wall the lamp's light does not arrive: {shaded} against {lit}");
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_wall_in_the_beam_of_a_floodlight_throws_its_shadow_along_the_beam() {
    let mut renderer = renderer();
    let mut scene = renderer.new_scene();
    let grey = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.8, 0.8, 0.8, 1.0], false);
    let ground = renderer.add_mesh(&mut scene, &quad(Vec3::ZERO, 2, Vec3::new(40.0, 40.0, 0.0)));
    let wall = renderer.add_mesh(&mut scene, &quad(Vec3::new(0.0, 0.0, 3.0), 0, Vec3::new(0.0, 10.0, 3.0)));
    renderer.add_instance(&mut scene, ground, DVec3::ZERO, Mat4::IDENTITY, vec![grey]);
    renderer.add_instance(&mut scene, wall, DVec3::ZERO, Mat4::IDENTITY, vec![grey]);
    // a spot on a mast 8 m off the wall, aimed across the ground at it and a little down, no
    // street lamp: its light is a cone along its axis, and so must be its shadow
    scene.lights = vec![PointLight {
        position: DVec3::new(-12.0, 0.0, 5.0),
        radius: 40.0,
        color: [1.0, 1.0, 1.0],
        core: 6.0,
        direction: Vec3::new(1.0, 0.0, -0.3).normalize(),
        cone: [50f32.to_radians().cos(), 70f32.to_radians().cos()],
        mode: LightMode::Both,
        ..Default::default()
    }];
    let rgba = renderer.render_to_image(&mut scene, 64, 64, &from_above(25.0), &night()).unwrap();
    let reach = 25.0 * 2.0 * 30f32.to_radians().tan();
    let lit = ground_at(&rgba, reach, -5.0, 0.0);
    let shaded = ground_at(&rgba, reach, 5.0, 0.0);
    assert!(lit > 8.0, "the spot lights the ground in front of the wall: {lit}");
    assert!(shaded * 3.0 < lit, "behind the wall the spot's light does not arrive: {shaded} against {lit}");
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_low_lamp_that_shines_every_way_is_shaded_by_a_wall_far_to_its_side() {
    let mut renderer = renderer();
    let mut scene = renderer.new_scene();
    let grey = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.8, 0.8, 0.8, 1.0], false);
    let ground = renderer.add_mesh(&mut scene, &quad(Vec3::ZERO, 2, Vec3::new(40.0, 40.0, 0.0)));
    let wall = renderer.add_mesh(&mut scene, &quad(Vec3::new(0.0, 0.0, 3.0), 0, Vec3::new(0.0, 10.0, 3.0)));
    renderer.add_instance(&mut scene, ground, DVec3::ZERO, Mat4::IDENTITY, vec![grey]);
    renderer.add_instance(&mut scene, wall, DVec3::ZERO, Mat4::IDENTITY, vec![grey]);
    // a bare point light 1.5 m up: the ground 8 m past the wall lies 10 degrees under its horizon,
    // outside the 150 degrees a lamp looking straight down takes in
    scene.lights = vec![PointLight {
        position: DVec3::new(-3.0, 0.0, 1.5),
        radius: 40.0,
        color: [1.0, 1.0, 1.0],
        core: 6.0,
        mode: LightMode::Both,
        ..Default::default()
    }];
    let rgba = renderer.render_to_image(&mut scene, 64, 64, &from_above(25.0), &night()).unwrap();
    let reach = 25.0 * 2.0 * 30f32.to_radians().tan();
    let lit = ground_at(&rgba, reach, -5.0, 0.0);
    let shaded = ground_at(&rgba, reach, 5.0, 0.0);
    assert!(lit > 8.0, "the lamp lights the ground on its own side: {lit}");
    assert!(shaded * 3.0 < lit, "past the wall the lamp's light does not arrive: {shaded} against {lit}");
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_handrail_in_the_players_saloon_shades_the_floor_from_its_lamp() {
    let mut renderer = renderer();
    let mut scene = renderer.new_scene();
    let grey = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.8, 0.8, 0.8, 1.0], false);
    // a floor and a pole 2 m tall, in a vehicle box 10 m across and 4 m high
    let floor = renderer.add_mesh(&mut scene, &quad(Vec3::ZERO, 2, Vec3::new(4.5, 4.5, 0.0)));
    let pole = renderer.add_mesh(&mut scene, &quad(Vec3::new(0.0, 0.0, 1.5), 0, Vec3::new(0.0, 0.3, 1.5)));
    let floor_instance = renderer.add_instance(&mut scene, floor, DVec3::ZERO, Mat4::IDENTITY, vec![grey]);
    renderer.add_instance(&mut scene, pole, DVec3::ZERO, Mat4::IDENTITY, vec![grey]);
    // the floor takes the saloon's plain light, a lamp hangs 3.5 m up beside the pole
    renderer.set_interior(&mut scene, floor_instance, 0.6);
    let slot = renderer.alloc_interior_lights(&mut scene, 1);
    renderer.set_interior_light(&mut scene, slot, PointLight { position: DVec3::new(-1.5, 0.0, 3.5), radius: 100.0, core: 1.0, color: [1.0; 3], intensity: 1.0, ..Default::default() });
    let mut lighting = night();
    lighting.inside = Some((DVec3::ZERO, 0.0, [10.0, 10.0, 4.0, 0.0, 0.0, 2.0]));
    let (_, tiles) = renderer.prepare_lights(&mut scene, &LightEye { cam_rel: Vec3::new(0.0, 0.0, 8.0), view_proj: None, vehicle: lighting.inside.as_ref().map(|b| (b, None)), level: 2 }, true, true);
    assert!(tiles.iter().any(|t| t.cabin.is_some() && CABIN_SLOTS.contains(&t.slot)), "the saloon's lamp has a tile: {} tiles", tiles.len());
    let reach = 8.0 * 2.0 * 30f32.to_radians().tan();
    let rgba = renderer.render_to_image(&mut scene, 64, 64, &from_above(8.0), &lighting).unwrap();
    // (the saloon's plain light is the same everywhere: what differs is how much of the lamp's
    // light the point is in - behind the pole, or to its side)
    let behind = ground_at(&rgba, reach, 1.5, 0.0);
    let beside = ground_at(&rgba, reach, 1.5, 3.0);
    assert!(behind * 1.5 < beside, "the pole's shadow darkens the floor behind it: {behind} against {beside}");
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn another_vehicles_headlamps_throw_the_shadow_of_what_stands_in_their_beams() {
    let mut renderer = renderer();
    let mut scene = renderer.new_scene();
    let grey = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.8, 0.8, 0.8, 1.0], false);
    // the road, a post 12 m ahead of a car's lamps (a thin square across the beams, 2 m high)
    let ground = renderer.add_mesh(&mut scene, &quad(Vec3::new(0.0, 14.0, 0.0), 2, Vec3::new(40.0, 40.0, 0.0)));
    let post = renderer.add_mesh(&mut scene, &quad(Vec3::new(0.0, 12.0, 1.0), 1, Vec3::new(0.4, 0.0, 1.0)));
    renderer.add_instance(&mut scene, ground, DVec3::ZERO, Mat4::IDENTITY, vec![grey]);
    renderer.add_instance(&mut scene, post, DVec3::ZERO, Mat4::IDENTITY, vec![grey]);
    // (a car's body round the lamps would shade the road in front of it: its parts are left out of the
    // lamps' map, and here there are none)
    scene.lights = [-0.7, 0.7]
        .map(|x| PointLight {
            position: DVec3::new(x, 0.0, 0.8),
            radius: 50.0,
            color: [1.0, 1.0, 1.0],
            intensity: 400.0,
            direction: Vec3::new(0.0, 1.0, -0.05).normalize(),
            cone: [0.5, 0.0],
            core: 1.0,
            beam: 1.0,
            mode: LightMode::Enhanced,
            ..Default::default()
        })
        .to_vec();
    let (_, tiles) = renderer.prepare_lights(&mut scene, &LightEye::at(Vec3::new(0.0, 14.0, 20.0)), true, true);
    assert!(tiles.iter().any(|t| t.look.is_some() && VEHICLE_SLOTS.contains(&t.slot) && t.hosts), "the pair has a map: {} tiles", tiles.len());
    let camera = Camera { position: DVec3::new(0.0, 14.0, 20.0), ..from_above(20.0) };
    let rgba = renderer.render_to_image(&mut scene, 64, 64, &camera, &night()).unwrap();
    let reach = 20.0 * 2.0 * 30f32.to_radians().tan();
    // (the view's middle is 14 m ahead of the lamps)
    let behind = ground_at(&rgba, reach, 0.0, 18.0 - 14.0);
    let beside = ground_at(&rgba, reach, 3.0, 18.0 - 14.0);
    assert!(beside > 12.0, "the beams light the road: {beside}");
    assert!(behind * 2.0 < beside, "the post keeps the light off the road behind it: {behind} against {beside}");
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_part_that_moves_in_the_saloon_is_left_out_of_its_lamps_tiles_until_it_stops() {
    let renderer = renderer();
    let mut scene = renderer.new_scene();
    let grey = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.8, 0.8, 0.8, 1.0], false);
    let mesh = renderer.add_mesh(&mut scene, &quad(Vec3::new(0.0, 0.0, 1.0), 0, Vec3::new(0.0, 0.3, 1.0)));
    let seat = renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![grey]);
    let wheel = renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::from_translation(Vec3::new(2.0, 0.0, 0.0)), vec![grey]);
    let outside = renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::from_translation(Vec3::new(40.0, 0.0, 0.0)), vec![grey]);
    let bus = (DVec3::ZERO, 0.0, [10.0, 10.0, 4.0, 0.0, 0.0, 2.0]);
    let cabin = Cabin { vehicle: passes::VehicleBox::new(&bus, DVec3::ZERO), to_local: Mat4::IDENTITY, lamp: Vec3::new(0.0, 0.0, 3.5) };
    let mut motion = passes::CabinMotion::default();
    let still = |m: &passes::CabinMotion| [m.still[seat], m.still[wheel], m.still[outside]];
    renderer.prepare(&mut scene);
    // the first frame: what is in the vehicle counts as still (a picture of one frame has its saloon)
    motion.update(&scene, &cabin);
    assert_eq!(still(&motion), [true, true, false]);
    // the wheel turns a little every frame, the seat stands: the wheel is out from the second frame
    for k in 1..=12 {
        let turned = Mat4::from_translation(Vec3::new(2.0, 0.0, 0.0)) * Mat4::from_rotation_z(k as f32 * 0.2);
        renderer.set_transform(&mut scene, wheel, DVec3::ZERO, turned);
        renderer.prepare(&mut scene);
        motion.update(&scene, &cabin);
        assert_eq!(still(&motion), [true, false, false], "frame {k}");
    }
    // it stops: it is back after the frames it takes to settle, and not before
    let mut back = 0;
    for k in 1..=60 {
        motion.update(&scene, &cabin);
        if motion.still[wheel] {
            back = k;
            break;
        }
    }
    assert!((29..=31).contains(&back), "the wheel is back after {back} frames");
    // the whole vehicle drives off with the saloon in it (the frame of the vehicle moves): nothing in it moves
    let ahead = DVec3::new(0.0, 5.0, 0.0);
    let moved = Cabin { vehicle: passes::VehicleBox::new(&(ahead, 0.0, bus.2), DVec3::ZERO), to_local: Mat4::from_translation(-ahead.as_vec3()), ..cabin };
    for id in [seat, wheel] {
        let at = scene.instances[id].transform;
        renderer.set_transform(&mut scene, id, ahead, at);
    }
    renderer.prepare(&mut scene);
    motion.update(&scene, &moved);
    assert_eq!(still(&motion), [true, true, false]);
}
