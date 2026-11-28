use super::*;

#[test]
fn streaming_starts_around_the_bus() {
    let cam = Camera {
        position: DVec3::new(5000.0, 6000.0, 50.0),
        yaw: 0.0,
        pitch: 0.0,
        roll: 0.0,
        fov_deg: 60.0,
        near: 0.5,
        far: 100.0,
    };
    let args = |extra: &[&str]| Args::parse_from(["omsi"].iter().chain(extra.iter()).copied());
    // the camera alone
    assert_eq!(start_centers(&args(&[]), &cam, None), vec![cam.position]);
    // a bus viewed from its seat: only where it stands, even with a camera given (the
    // view follows the bus)
    let seat = vec![DVec3::new(10.0, 20.0, 0.0)];
    assert_eq!(
        start_centers(
            &args(&["--bus", "Vehicles/x.bus", "--spawn", "10,20,90"]),
            &cam,
            None
        ),
        seat
    );
    assert_eq!(
        start_centers(
            &args(&[
                "--bus",
                "Vehicles/x.bus",
                "--spawn",
                "10,20,90",
                "--cam",
                "5000,6000,50,0,0"
            ]),
            &cam,
            None
        ),
        seat
    );
    // ... and with the free camera: both places
    let both = start_centers(
        &args(&[
            "--bus",
            "Vehicles/x.bus",
            "--spawn",
            "10,20,90",
            "--cam",
            "5000,6000,50,0,0",
            "--view",
            "free",
        ]),
        &cam,
        None,
    );
    assert_eq!(both, vec![DVec3::new(10.0, 20.0, 0.0), cam.position]);
}

#[test]
fn the_export_root_uses_the_same_discovery_as_a_game_start() {
    let missing = format!("missing-{}", std::process::id());
    let found = PathBuf::from("found").join("OMSI 2");
    let mut args = Args::parse_from(["omsi", "--root", missing.as_str()]);
    assert!(discover_root(&mut args, || Some(found.clone())));
    assert_eq!(args.root, found);

    let mut args = Args::parse_from(["omsi", "--root", missing.as_str(), "--list-maps"]);
    assert!(!discover_root(&mut args, || Some(found.clone())));
    assert_ne!(args.root, found);
}
