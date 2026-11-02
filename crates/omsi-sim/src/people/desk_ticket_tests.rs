use super::ticket_hook_tests::{boarding, human_type, player_bus};
use super::*;

fn view(given_ticket: Option<f32>) -> PlayerBusView {
    PlayerBusView {
        position: DVec3::ZERO,
        heading: 0.0,
        rot: Mat4::IDENTITY,
        speed: 0.0,
        speed_kmh: 0.0,
        a_trans: Vec3::ZERO,
        bounding_box: None,
        interior: 1.0,
        air: CabinAir::default(),
        given_ticket,
        terminus: None,
        tt_stop: None,
        act_busstop: String::new(),
        route: None,
        cabin: None,
    }
}

fn reaching_for(tag: &str, accepted: f32) -> (PeopleSim, BusNow) {
    let ty = human_type(tag);
    let bus = player_bus(tag);
    let mut h = PeopleSim::new(Path::new("/nonexistent"), 200);
    h.boarding = "manual".into();
    h.people = vec![boarding(&ty, 1, Task::InBusToPlace)];
    let p = h.pax_mut(0).unwrap();
    p.inside = Some(BusId::Player);
    p.sub = 5;
    p.timer = 10.0;
    let mut taken = false;
    h.desk_sale(0, 0.0, &bus, Some(&view(Some(accepted))), &mut taken);
    assert_eq!(h.pax(0).unwrap().sub, 6);
    assert!(!taken);
    h.pax_mut(0).unwrap().timer = 0.0;
    (h, bus)
}

fn pick_up(h: &mut PeopleSim, bus: &BusNow, given_now: Option<f32>) {
    let mut taken = false;
    h.desk_sale(0, 0.0, bus, Some(&view(given_now)), &mut taken);
    assert!(taken);
    assert_eq!(h.tickets_sold, 1);
    assert_eq!(h.pax(0).unwrap().taking_ticket, None);
}

#[test]
fn a_ticket_issued_during_the_reach_does_not_replace_the_accepted_one() {
    let (mut h, bus) = reaching_for("desk-swap", 0.0);
    pick_up(&mut h, &bus, Some(1.0));
    assert_eq!(h.take_sale_events(), vec![(None, SaleEvent::PassengerTookTicket(0))]);
}

#[test]
fn given_ticket_reset_during_the_reach_still_picks_up_the_accepted_one() {
    let (mut h, bus) = reaching_for("desk-reset", 0.0);
    pick_up(&mut h, &bus, Some(-1.0));
    assert_eq!(h.take_sale_events(), vec![(None, SaleEvent::PassengerTookTicket(0))]);

    let (mut h, bus) = reaching_for("desk-gone", 0.0);
    pick_up(&mut h, &bus, None);
    assert_eq!(h.take_sale_events(), vec![(None, SaleEvent::PassengerTookTicket(0))]);
}
