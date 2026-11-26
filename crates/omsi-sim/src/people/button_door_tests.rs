//! A shut entry with a button (`{withbutton}`) as Omsi.exe's sub_72506c counts it.

use super::*;

/// A shut door with a button (`{withbutton}`) counts as Omsi.exe counts it (sub_72506c,
/// 0x7251f5): with every door shut it is the one to wait at, and with another door open it
/// still draws the people nearest it, who ask for it; a shut door without one draws nobody.
#[test]
fn a_shut_button_door_draws_the_people_nearest_it_as_in_omsi() {
    let dir = std::env::temp_dir().join(format!("omsi-button-door-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let write = |name: &str, text: &str| std::fs::write(dir.join(name), text).unwrap();
    // a door at the front right (point 0) and one at the rear right with a button (point 2)
    write(
        "paths.cfg",
        "[pathpnt]
1.1
4
0.4
[pathpnt]
0
0
0.5
[pathpnt]
1.1
-4
0.4
[pathlink]
0
1
[pathlink]
1
2
",
    );
    write(
        "cabin.cfg",
        "[entry]
0
[entry]
2
{withbutton}
[exit]
2
[passpos]
-0.5
0
1.0
0.45
0
",
    );
    let bus = omsi_vehicle::Vehicle {
        path: dir.join("bus.bus"),
        passenger_cabin: Some("cabin.cfg".into()),
        paths: Some("paths.cfg".into()),
        bounding_box: Some([2.5, 9.0, 3.0, 0.0, 0.0, 1.5]),
        ..Default::default()
    };
    let cabin = Cabin::load_train(&[(&bus, Vec3::ZERO, f32::INFINITY, false)]).expect("cabin");
    std::fs::remove_dir_all(&dir).ok();
    let entries = cabin.entry_points();
    assert_eq!(entries, [Some(0), Some(2)]);
    let flags = cabin.entry_flags();
    assert_eq!(flags[1].1, true, "the rear door has its button");
    let (near_front, near_rear) = (Vec3::new(1.6, 4.0, 0.0), Vec3::new(1.6, -4.0, 0.0));
    // every door shut: the button door, from wherever
    assert_eq!(
        cabin.omsi_nearest(
            near_front,
            &entries,
            false,
            false,
            Some(&flags),
            Some(&[false, false])
        ),
        Some(2)
    );
    // the front open, the rear shut with its button: the rear from the rear, the front
    // from the front
    assert_eq!(
        cabin.omsi_nearest(
            near_rear,
            &entries,
            false,
            false,
            Some(&flags),
            Some(&[true, false])
        ),
        Some(2)
    );
    assert_eq!(
        cabin.omsi_nearest(
            near_front,
            &entries,
            false,
            false,
            Some(&flags),
            Some(&[true, false])
        ),
        Some(0)
    );
    // the front shut (no button), the rear open: the rear, even from the front
    assert_eq!(
        cabin.omsi_nearest(
            near_front,
            &entries,
            false,
            false,
            Some(&flags),
            Some(&[false, true])
        ),
        Some(2)
    );
    // both open: the nearest
    assert_eq!(
        cabin.omsi_nearest(
            near_rear,
            &entries,
            false,
            false,
            Some(&flags),
            Some(&[true, true])
        ),
        Some(2)
    );
    // the exits: with one open, the shut one is passed by; with none, the nearest
    let exits = cabin.exit_points();
    assert_eq!(
        cabin.omsi_nearest(near_front, &exits, false, false, None, Some(&[true])),
        Some(2)
    );
}
