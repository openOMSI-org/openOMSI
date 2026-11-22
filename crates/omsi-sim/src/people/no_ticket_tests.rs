//! #2742: a fare paid in cash without a ticket - the passenger pays, gets the change and
//! goes on, instead of waiting at the desk for a ticket nobody in that country gets.

use super::ticket_hook_tests::{boarding, human_type};
use super::*;
use omsi_content::Currency;

/// A cash desk's money: half-euro, ten and one cent coins, no notes.
fn money() -> Money {
    let mut m = Money::new(Path::new("/nonexistent"), "money.cfg");
    m.currency = Some(Currency {
        path: Default::default(),
        name: "Euro".into(),
        decimals: 2,
        coins: vec![
            ("c50".into(), 0.5),
            ("c10".into(), 0.1),
            ("c01".into(), 0.01),
        ],
        bills: Vec::new(),
    });
    m
}

/// The bus's ticket pack: tickets with a price, so the sale asks for money.
fn pack() -> omsi_content::tickets::TicketPack {
    let t = |name: &str, value: f32| omsi_content::tickets::Ticket {
        name: name.into(),
        age_min: 14,
        age_max: 200,
        value,
        probability: 1.0,
        ..Default::default()
    };
    omsi_content::tickets::TicketPack {
        tickets: vec![t("Fahrschein", 2.7), t("Kurzstrecke", 1.4)],
        ..Default::default()
    }
}

/// The player's bus with a cash desk and a money point on it.
fn player_bus(tag: &str) -> BusNow {
    let dir = std::env::temp_dir().join(format!("omsi-no-ticket-bus-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("test.bus"), "[boundingbox]\n2.5\n12\n3\n0\n0\n1.5\n\n[passengercabin]\ncabin.cfg\n\n[paths]\npaths.cfg\n").unwrap();
    std::fs::write(dir.join("paths.cfg"), "[pathpnt]\n1.1\n4\n0.4\n\n[pathpnt]\n0\n4\n0.5\n\n[pathpnt]\n0\n-3\n0.5\n\n[pathpnt]\n1.1\n-3\n0.4\n\n[pathlink]\n0\n1\n\n[pathlink]\n1\n2\n\n[pathlink]\n2\n3\n").unwrap();
    std::fs::write(
        dir.join("cabin.cfg"),
        "[entry]\n0\n\n[exit]\n3\n\n[ticket_sale]\n1\n0\n4\n1\n\n[ticket_sale_money_point]\n0\n0\n0.9\n0\n0.5\n\n[passpos]\n-0.5\n0\n0.9\n0.45\n0\n",
    )
    .unwrap();
    let def = omsi_vehicle::Vehicle::load(&dir.join("test.bus")).unwrap();
    let cabin =
        Arc::new(Cabin::load_train(&[(&def, Vec3::ZERO, f32::INFINITY, false)]).expect("cabin"));
    std::fs::remove_dir_all(&dir).ok();
    assert!(cabin.sale.is_some(), "the cabin has its cash desk");
    BusNow {
        id: BusId::Player,
        cabin,
        pos: DVec3::ZERO,
        rot: Mat4::IDENTITY,
        heading: 0.0,
        speed: 0.0,
        entry_open: vec![true],
        exit_open: vec![true],
        walk_open: None,
        interior: 1.0,
        air: CabinAir::default(),
        half: DVec2::new(1.25, 6.0),
        centre: DVec2::ZERO,
        accel: DVec2::ZERO,
        trailers: Vec::new(),
        terminus: None,
        takes: Takes::Terminus,
        next_stop: None,
        places_off: Vec::new(),
        served: Some(77),
        route: None,
    }
}

/// A passenger of the player's bus at the cash desk with ticket 1 asked for.
fn at_the_desk(tag: &str, no_ticket: bool) -> (PeopleSim, BusNow) {
    let ty = human_type(tag);
    let bus = player_bus(tag);
    let mut h = PeopleSim::new(Path::new("/nonexistent"), 200);
    h.boarding = "manual".into();
    h.give_ticket = true;
    h.money = Some(money());
    h.tickets = Some(Arc::new(pack()));
    h.no_ticket = no_ticket;
    h.people = vec![boarding(&ty, 1, Task::InBusToPlace)];
    let p = h.pax_mut(0).unwrap();
    p.inside = Some(BusId::Player);
    p.sub = 3;
    p.timer = 0.0;
    (h, bus)
}

fn step(h: &mut PeopleSim, bus: &BusNow) -> bool {
    let mut taken = false;
    h.desk_sale(0, 0.0, bus, None, &mut taken);
    taken
}

#[test]
fn a_fare_without_a_ticket_goes_straight_to_the_change() {
    let (mut h, bus) = at_the_desk("no-ticket", true);
    step(&mut h, &bus);
    assert_eq!(h.pax(0).unwrap().sub, 4, "the ticket asked for");
    h.pax_mut(0).unwrap().timer = 0.0;
    step(&mut h, &bus);
    // paid - and, with no ticket to be had, at once where the change is counted
    assert_eq!(h.pax(0).unwrap().sub, 6, "no waiting for a ticket");
    let (paid, value) = h.paid.unwrap();
    assert!(value > 0.0, "ticket 1 has its price: {value}");
    assert!(
        (paid - value).abs() < 1e-6,
        "the exact fare: {paid} of {value}"
    );
    let taken = step(&mut h, &bus);
    assert!(!taken, "taken_ticket remains false for ticketless fare");
    assert_eq!(
        h.pax(0).unwrap().sub,
        8,
        "nothing to give back, off to the place"
    );
    assert_eq!(h.change_due, Some(0.0));
    h.pax_mut(0).unwrap().timer = 0.0;
    step(&mut h, &bus);
    assert_eq!(h.pax(0).unwrap().sub, 0);
    assert_eq!(h.boarded, 1);
    // the fare is in the box and counted as one sale - without a ticket
    assert!((h.ticket_cash - value).abs() < 1e-6, "{}", h.ticket_cash);
    assert_eq!(h.tickets_sold, 0, "no tickets sold for ticketless fares");
    assert_eq!(h.fares_no_ticket, 1, "one fare, no ticket");
    assert!(
        h.take_sales().is_empty(),
        "take_sales has no tickets for ticketless fares"
    );
    assert!(
        !h.take_sale_events()
            .iter()
            .any(|(_, e)| matches!(e, SaleEvent::PassengerTookTicket(_))),
        "no ticket was taken"
    );
}

#[test]
fn with_a_ticket_the_sale_still_waits_for_it() {
    let (mut h, bus) = at_the_desk("ticket", false);
    step(&mut h, &bus);
    h.pax_mut(0).unwrap().timer = 0.0;
    step(&mut h, &bus);
    assert_eq!(h.pax(0).unwrap().sub, 5, "the ticket comes out first");
    assert_eq!(h.fares_no_ticket, 0);
    h.pax_mut(0).unwrap().sub = 6;
    h.pax_mut(0).unwrap().timer = 0.0;
    let taken = step(&mut h, &bus);
    assert!(taken, "taken_ticket is true when ticket is sold");
    assert_eq!(h.tickets_sold, 1);
    assert_eq!(h.take_sales().len(), 1, "take_sales records sold ticket");
}
