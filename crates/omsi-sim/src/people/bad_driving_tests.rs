use super::*;

fn rider() -> Person {
    let dir = std::env::temp_dir().join(format!("omsi-bad-driving-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("person.hum"), "[model]\nmodel.cfg\n").unwrap();
    std::fs::write(dir.join("model.cfg"), "").unwrap();
    let ty = Arc::new(HumanType::load(&dir.join("person.hum")).unwrap());
    std::fs::remove_dir_all(&dir).ok();
    let mut pax = Pax::new(1.1, 0.5);
    pax.bus = Some(BusId::Player);
    pax.inside = Some(BusId::Player);
    pax.task = Task::SittingInBus;
    pax.bad_at = [0.05, 0.3, 0.9];
    Person {
        id: 1,
        ty,
        variant: 0,
        position: DVec3::ZERO,
        heading: 0.0,
        lheading: 0.0,
        place: Place::Ground,
        vel: DVec2::ZERO,
        pace: 1.1,
        activity: Activity::Stand,
        anim: OmsiAnim::default(),
        state: State::Pax(Box::new(pax)),
        t_state: 0.0,
        interior: 0.0,
        tilt: Mat4::IDENTITY,
        age: 40.0,
        stuck: 0.0,
        ghost: 0.0,
        stuck_total: 0.0,
        car_wait: 0.0,
        detour: 0.0,
        detour_side: 0.0,
        why: "",
        puppet: None,
        remote: false,
        remote_seat: None,
    }
}

fn emergency_stop(on: bool, alight: bool, severe: bool) -> (u8, f32, Task) {
    let world = crate::world_data::WorldData::default();
    let mut h = PeopleSim::new(Path::new("/nonexistent"), 200);
    h.bad_driving = on;
    h.bad_driving_alight = alight;
    h.time = 10.0;
    h.people = vec![rider()];
    if severe {
        h.pax_mut(0).unwrap().bad_at = [0.001, 0.002, 0.003];
    }
    let bus = PlayerBusView {
        position: DVec3::ZERO,
        heading: 0.0,
        rot: Mat4::IDENTITY,
        speed: 14.0,
        speed_kmh: 50.4,
        a_trans: Vec3::new(0.0, -7.0, 0.0),
        bounding_box: None,
        interior: 1.0,
        air: CabinAir::default(),
        given_ticket: None,
        terminus: None,
        tt_stop: None,
        act_busstop: String::new(),
        route: None,
        cabin: None,
    };
    for _ in 0..100 {
        h.time += 0.02;
        h.ride_comfort(0.02, Some(&bus), &[], &HashMap::new(), &world);
    }
    let p = h.pax(0).unwrap();
    (p.complaint, p.discomfort, p.task)
}

#[test]
fn riders_complain_about_an_emergency_stop_unless_the_setting_is_off() {
    let (said, toll, _) = emergency_stop(true, true, false);
    assert_eq!(said, 1);
    assert!(toll > 0.05, "{toll}");
    assert_eq!(
        emergency_stop(false, true, false),
        (0, 0.0, Task::SittingInBus)
    );
    assert_eq!(
        emergency_stop(true, false, false).0,
        said,
        "disabling early exit must not silence complaints"
    );
}

#[test]
fn complain_only_preserves_the_seat_even_after_the_third_complaint() {
    let (complaints, discomfort, task) = emergency_stop(true, false, true);
    assert_eq!(complaints, 3);
    assert!(discomfort > 0.003);
    assert_eq!(task, Task::SittingInBus);
}
