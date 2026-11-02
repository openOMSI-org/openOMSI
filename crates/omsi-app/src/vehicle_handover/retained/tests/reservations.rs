use super::*;
use omsi_sim::timetable_run::ScheduleSim;

fn schedule() -> ScheduleSim {
    let path = std::env::temp_dir().join("openomsi-mixed-entry-no-assets");
    let world = omsi_sim::world_data::WorldData {
        map: std::sync::Arc::new(omsi_sim::world_data::MapFacts {
            root: path.clone(),
            map_dir: path.clone(),
            date: 20261030,
            ..Default::default()
        }),
        ..Default::default()
    };
    ScheduleSim::new(&path, &world, &omsi_sim::SimClock::default())
}

fn swap(app: &mut App, schedule: &mut ScheduleSim) {
    let mut parts = app.parts();
    assert!(exchange_placed_duty(
        0,
        false,
        &mut parts.player,
        &mut parts.duty,
        Some(schedule)
    ));
    crate::on_foot::take_placed(
        0,
        &mut parts.player,
        &mut parts.people,
        &mut parts.view,
        &mut parts.ui,
        parts.sound,
        parts.common.settings,
    );
    app.sync_driven_duty_args();
}

#[test]
fn the_shared_exchange_moves_the_reservation_with_the_duty_in_both_directions() {
    let (mut app, a, b) = two_buses();
    let mut schedule = schedule();
    schedule.reserve_tour("A", "tour");
    let reservation_a = schedule.duty_reservation();
    schedule.reserve_tour("B", "tour");
    let reservation_b = schedule.duty_reservation();
    schedule.restore_duty_reservation(reservation_a.clone());
    app.session.placed[0]
        .parked_duty
        .as_mut()
        .unwrap()
        .reservation = reservation_b.clone();
    for (uid, line, stop, active, parked) in [
        (b, "B", 7, &reservation_b, &reservation_a),
        (a, "A", 3, &reservation_a, &reservation_b),
        (b, "B", 7, &reservation_b, &reservation_a),
    ] {
        swap(&mut app, &mut schedule);
        assert_driving(&app, uid, line, stop);
        assert!(schedule.duty_reservation() == *active);
        assert!(
            app.session.placed[0]
                .parked_duty
                .as_ref()
                .unwrap()
                .reservation
                == *parked
        );
    }
}

#[test]
fn an_unscheduled_target_releases_only_the_active_reservation_and_keeps_the_parked_one() {
    let (mut app, a, _) = two_buses();
    let mut schedule = schedule();
    schedule.reserve_tour("A", "tour");
    let reservation_a = schedule.duty_reservation();
    app.session.placed[0].parked_duty = None;
    swap(&mut app, &mut schedule);
    assert!(schedule.duty_reservation().is_none());
    assert!(app.session.duty.is_none());
    assert!(
        app.session.placed[0]
            .parked_duty
            .as_ref()
            .unwrap()
            .reservation
            == reservation_a
    );
    swap(&mut app, &mut schedule);
    assert_driving(&app, a, "A", 3);
    assert!(schedule.duty_reservation() == reservation_a);
}

#[test]
fn an_invalid_target_cannot_release_an_existing_reservation() {
    let (mut app, _, _) = two_buses();
    let mut schedule = schedule();
    schedule.reserve_tour("A", "tour");
    let before = schedule.duty_reservation();
    let mut parts = app.parts();
    assert!(!exchange_placed_duty(
        usize::MAX,
        false,
        &mut parts.player,
        &mut parts.duty,
        Some(&mut schedule)
    ));
    assert!(schedule.duty_reservation() == before);
    assert_eq!(parts.duty.duty.as_ref().unwrap().line, "A");
    assert!(parts.player.placed[0].parked_duty.is_some());
}

#[test]
fn lan_exchange_keeps_the_active_reservation_and_does_not_park_it() {
    let (mut app, _, _) = two_buses();
    app.session.placed[0].parked_duty = None;
    let mut schedule = schedule();
    schedule.reserve_tour("A", "tour");
    let before = schedule.duty_reservation();
    let mut parts = app.parts();
    assert!(exchange_placed_duty(0, true, &mut parts.player, &mut parts.duty, Some(&mut schedule)));
    assert!(schedule.duty_reservation() == before);
    assert_eq!(parts.duty.duty.as_ref().unwrap().line, "A");
    assert!(parts.player.player.as_ref().unwrap().parked_duty.is_none());
    assert!(parts.player.placed[0].parked_duty.is_none());
}
