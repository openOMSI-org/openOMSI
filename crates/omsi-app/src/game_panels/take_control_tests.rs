use super::{forget_gone, ray_box};
use glam::DVec3;

#[test]
fn the_question_goes_when_its_bus_leaves_the_traffic_before_the_answer() {
    let args = <crate::cli::Args as clap::Parser>::parse_from(["openomsi"]);
    let mut app = crate::assemble_app(args, Default::default());
    app.menus.game_panels.take = Some(42);
    assert!(app.menus.game_panels.any());
    forget_gone(&mut app);
    assert_eq!(app.menus.game_panels.take, None);
    assert!(
        !app.menus.game_panels.any(),
        "nothing left open to hold the mouse"
    );
}

#[test]
fn the_map_cameras_ray_meets_a_turned_bus_box() {
    let b = omsi_sim::collision::Obb::from_box(
        [2.5, 12.0, 3.0, 0.0, 0.0, 1.5],
        DVec3::new(100.0, 50.0, 0.0),
        90.0,
    );
    let down = DVec3::new(0.0, 0.0, -1.0);
    let t = ray_box(&b, DVec3::new(105.0, 50.0, 30.0), down).expect("hit");
    assert!((t - 27.0).abs() < 1e-6, "{t}");
    assert!(ray_box(&b, DVec3::new(100.0, 55.0, 30.0), down).is_none());
    assert!(ray_box(&b, DVec3::new(105.0, 50.0, 30.0), -down).is_none());
}
