//! The plugins' say in the passengers' tickets (`ticket_hook`).

use super::*;

/// A passenger of the player's bus on the way to its door, about to buy ticket 1.
pub(super) fn boarding(ty: &Arc<HumanType>, id: u32, task: Task) -> Person {
    let mut pax = Pax::new(1.1, 0.5);
    pax.bus = Some(BusId::Player);
    pax.stop = Some(77);
    pax.dest = Some("Nový Bor".into());
    pax.task = task;
    pax.ticket = TICKET_BUY;
    pax.ticket_id = 1;
    pax.price = 2.7;
    Person {
        id,
        ty: ty.clone(),
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
        car_wait: 0.0,
        detour: 0.0,
        detour_side: 0.0,
        why: "",
        puppet: None,
        remote: false,
    }
}

fn pack() -> omsi_content::tickets::TicketPack {
    let t = |name: &str, value: f32| omsi_content::tickets::Ticket { name: name.into(), age_min: 14, age_max: 200, value, probability: 1.0, ..Default::default() };
    omsi_content::tickets::TicketPack { tickets: vec![t("Fahrschein", 2.7), t("Kurzstrecke", 1.4)], ..Default::default() }
}

#[test]
fn plugins_hear_of_a_ticket_and_change_it_until_it_is_asked_for() {
    let dir = std::env::temp_dir().join(format!("omsi-ticket-hook-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("person.hum"), "[model]\nmodel.cfg\n").unwrap();
    std::fs::write(dir.join("model.cfg"), "").unwrap();
    let ty = Arc::new(HumanType::load(&dir.join("person.hum")).unwrap());
    std::fs::remove_dir_all(&dir).unwrap();
    let mut h = PeopleSim::new(Path::new("/nonexistent"), 200);
    h.tickets = Some(Arc::new(pack()));
    h.people = vec![boarding(&ty, 1, Task::WalkingToBus), boarding(&ty, 2, Task::WaitingForBus), boarding(&ty, 3, Task::WalkingToBus)];
    h.stop_names = Some([(77, "Střížkov".to_string())].into_iter().collect());
    h.duty = Some((Arc::new(DutyTrip { name: "1".into(), departure: 0.0, terminus: "Nový Bor".into(), public: true, stops: vec![(70, "Ládví".into(), true), (77, "Střížkov".into(), true), (80, "Mladá Boleslav".into(), true), (90, "Nový Bor".into(), true)] }), 1, false));

    // nobody listens: nothing is reported, nobody waits
    h.ask_ticket(0);
    assert!(h.take_ticket_asks().is_empty());
    assert_eq!(h.pax(0).unwrap().ticket_hold, None);

    // a plugin listens: the passenger is reported with their ride
    h.ticket_hook = true;
    h.ask_ticket(0);
    let asks = h.take_ticket_asks();
    assert_eq!(asks.len(), 1);
    let a = &asks[0];
    assert_eq!((a.person, a.kind, a.ticket, a.name.as_str(), a.price), (1, TICKET_BUY, 1, "Fahrschein", 2.7));
    assert_eq!((a.stop, a.stop_name.as_deref(), a.destination.as_deref()), (Some(77), Some("Střížkov"), Some("Nový Bor")));
    assert_eq!((a.stop_index, a.destination_index), (Some(1), Some(3)));
    assert!(h.pax(0).unwrap().ticket_hold.is_some(), "the request at the desk waits for the plugins");
    h.ticket_asks_heard(&[1]);
    assert_eq!(h.pax(0).unwrap().ticket_hold, None);

    // the plugin prices the ride by its zones
    let zones = TicketChange { name: Some("IDOL 4001".into()), price: Some(64.0), voice: Some("Zone_4001".into()), tag: Some("telmax".into()), ..Default::default() };
    assert_eq!(h.set_ticket(1, &zones, "telmax"), Ok(()));
    let p = h.pax(0).unwrap();
    assert_eq!((p.plugin().name.as_deref(), p.price, p.plugin().voice.as_deref()), (Some("IDOL 4001"), 64.0, Some("Zone_4001")));
    assert_eq!((p.plugin().by.as_deref(), p.plugin().tag.as_deref(), p.ticket_id), (Some("telmax"), Some("telmax"), 1));
    // another ticket of the pack brings its own name and price
    assert_eq!(h.set_ticket(1, &TicketChange { ticket: Some(2), ..Default::default() }, "telmax"), Ok(()));
    let p = h.pax(0).unwrap();
    assert_eq!((p.ticket_id, p.plugin().name.as_deref(), p.price), (2, None, 1.4));
    assert_eq!(h.set_ticket(1, &TicketChange { ticket: Some(9), ..Default::default() }, "x"), Err("no such ticket"));
    assert_eq!(h.set_ticket(42, &zones, "x"), Err("no such person"));

    // somebody still waiting has decided nothing yet; a tag is kept all the same
    assert_eq!(h.set_ticket(2, &zones, "x"), Err("not boarding the bus yet"));
    assert_eq!(h.set_ticket(2, &TicketChange { tag: Some("seen".into()), ..Default::default() }, "x"), Ok(()));
    assert_eq!(h.pax(1).unwrap().plugin().tag.as_deref(), Some("seen"));

    // a pass instead of a ticket, before they are in; buying needs a cash desk
    let pass = TicketChange { kind: Some(TICKET_NONE), ..Default::default() };
    assert_eq!(h.set_ticket(3, &pass, "x"), Ok(()));
    assert_eq!(h.pax(2).unwrap().ticket, TICKET_NONE);
    let buy = TicketChange { kind: Some(TICKET_BUY), ..Default::default() };
    assert_eq!(h.set_ticket(3, &buy, "x"), Err("the bus sells no tickets"));

    // in the bus on the way to the desk: the ticket still, not the kind; once asked, nothing
    {
        let p = h.pax_mut(0).unwrap();
        p.inside = Some(BusId::Player);
        p.task = Task::InBusToPlace;
        p.sub = 3;
    }
    assert_eq!(h.set_ticket(1, &pass, "x"), Err("already in the bus"));
    assert_eq!(h.set_ticket(1, &zones, "x"), Ok(()));
    h.pax_mut(0).unwrap().sub = 4;
    assert_eq!(h.set_ticket(1, &zones, "x"), Err("already asked for the ticket"));
}

pub(super) fn human_type(tag: &str) -> Arc<HumanType> {
    let dir = std::env::temp_dir().join(format!("omsi-ticket-hook-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("person.hum"), "[model]\nmodel.cfg\n").unwrap();
    std::fs::write(dir.join("model.cfg"), "").unwrap();
    let ty = Arc::new(HumanType::load(&dir.join("person.hum")).unwrap());
    std::fs::remove_dir_all(&dir).unwrap();
    ty
}

/// The player's bus standing at stop 77 with its door open: one entry, one exit, a cash
/// desk, one seat.
/// (`tag`: a folder of its own - the tests run at the same time, and one removing the files
/// another is still reading left it without a cabin)
pub(super) fn player_bus(tag: &str) -> BusNow {
    let dir = std::env::temp_dir().join(format!("omsi-ticket-hook-bus-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("test.bus"), "[boundingbox]\n2.5\n12\n3\n0\n0\n1.5\n\n[passengercabin]\ncabin.cfg\n\n[paths]\npaths.cfg\n").unwrap();
    std::fs::write(dir.join("paths.cfg"), "[pathpnt]\n1.1\n4\n0.4\n\n[pathpnt]\n0\n4\n0.5\n\n[pathpnt]\n0\n-3\n0.5\n\n[pathpnt]\n1.1\n-3\n0.4\n\n[pathlink]\n0\n1\n\n[pathlink]\n1\n2\n\n[pathlink]\n2\n3\n").unwrap();
    std::fs::write(dir.join("cabin.cfg"), "[entry]\n0\n\n[exit]\n3\n\n[ticket_sale]\n1\n0\n4\n1\n\n[passpos]\n-0.5\n0\n0.9\n0.45\n0\n").unwrap();
    let def = omsi_vehicle::Vehicle::load(&dir.join("test.bus")).unwrap();
    let cabin = Arc::new(Cabin::load_train(&[(&def, Vec3::ZERO, f32::INFINITY)]).expect("cabin"));
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

/// Review of #2708: the plugins hear of a passenger in their next frame, and the people may
/// take several steps before it. Somebody who decides right at an open door waits in the
/// doorway until then - a plugin can still make them a pass holder - and no longer than
/// `TICKET_HOLD`.
#[test]
fn boarding_waits_in_the_doorway_until_the_plugins_had_their_frame() {
    let ty = human_type("door");
    let world = crate::world_data::WorldData::default();
    let mut h = PeopleSim::new(Path::new("/nonexistent"), 200);
    h.tickets = Some(Arc::new(pack()));
    h.ticket_hook = true;
    let mut stop = h.build_pax_stop(&world, None, 77, DVec3::ZERO, 0.0, "Střížkov");
    stop.buses.push((BusId::Player, true));
    h.stops.insert(77, stop);
    let buses = vec![player_bus("door")];
    let bus_ix: HashMap<BusId, usize> = [(BusId::Player, 0)].into_iter().collect();
    // two passengers in the open doorway, their places reserved, deciding to buy a ticket
    h.people = vec![boarding(&ty, 1, Task::WalkingToBus), boarding(&ty, 2, Task::WalkingToBus)];
    for i in 0..2 {
        {
            let p = h.pax_mut(i).unwrap();
            p.seat = Some(0);
            p.door = Some(0);
            p.st = 3;
        }
        h.new_ticket(i, TICKET_BUY, 1, 2.7, true);
    }
    assert_eq!(h.take_ticket_asks().len(), 2);
    let step = |h: &mut PeopleSim, dt: f64| {
        h.time += dt;
        for i in 0..2 {
            if h.pax(i).unwrap().task == Task::WalkingToBus {
                h.task_to_bus(i, &buses, &bus_ix, &world);
            }
        }
    };
    let task = |h: &PeopleSim, i: usize| h.pax(i).unwrap().task;
    // ten steps of the people before the plugins' frame: nobody gets in
    for _ in 0..10 {
        step(&mut h, 1.0 / 60.0);
        assert_eq!((task(&h, 0), task(&h, 1)), (Task::WalkingToBus, Task::WalkingToBus), "in before the plugins heard of them");
    }
    // the plugins' frame came for the first: made a pass holder in the doorway, in they go
    let pass = TicketChange { kind: Some(TICKET_NONE), ..Default::default() };
    assert_eq!(h.set_ticket(1, &pass, "x"), Ok(()));
    h.ticket_asks_heard(&[1]);
    step(&mut h, 1.0 / 60.0);
    assert_eq!(task(&h, 0), Task::InBusToPlace);
    assert_eq!(h.pax(0).unwrap().ticket, TICKET_NONE, "no cash desk for a pass holder");
    // the second never heard of (plugins stopped): in after `TICKET_HOLD` all the same
    assert_eq!(task(&h, 1), Task::WalkingToBus);
    step(&mut h, super::ticket_hook::TICKET_HOLD);
    assert_eq!(task(&h, 1), Task::InBusToPlace);
}

/// Review of #2708: a boarding given up (a bus pulling away, a shut door) and tried again
/// is a new ticket: what a plugin made of the first one is gone, its tag is kept.
#[test]
fn a_new_boarding_forgets_the_plugins_ticket_but_not_its_tag() {
    let ty = human_type("retry");
    let mut h = PeopleSim::new(Path::new("/nonexistent"), 200);
    h.tickets = Some(Arc::new(pack()));
    h.ticket_hook = true;
    h.people = vec![boarding(&ty, 1, Task::WalkingToBus)];
    h.new_ticket(0, TICKET_BUY, 1, 2.7, true);
    let zones = TicketChange { name: Some("IDOL 4001".into()), price: Some(64.0), voice: Some("Zone_4001".into()), tag: Some("telmax".into()), ..Default::default() };
    assert_eq!(h.set_ticket(1, &zones, "telmax"), Ok(()));
    // the boarding is given up (the bus pulls away), and at the next bus they decide again -
    // this time the plugin leaves the ticket as it is
    h.new_ticket(0, TICKET_BUY, 2, 1.4, true);
    let p = h.pax(0).unwrap();
    assert_eq!((p.ticket_id, p.price), (2, 1.4));
    assert_eq!((p.plugin().name.as_deref(), p.plugin().voice.as_deref(), p.plugin().by.as_deref()), (None, None, None));
    assert_eq!(p.plugin().tag.as_deref(), Some("telmax"));
    // reported again as the pack's ticket, and held for the plugins again
    let asks = h.take_ticket_asks();
    assert_eq!(asks.len(), 2);
    assert_eq!((asks[1].ticket, asks[1].name.as_str(), asks[1].price), (2, "Kurzstrecke", 1.4));
    assert!(h.ticket_pending(0));
}

/// A custom payment can be set while boarding, but not after
/// the passenger has asked for their ticket. A new ticket clears it.
#[test]
fn custom_payment_can_only_be_set_before_the_cash_desk_and_is_cleared() {
    let ty = human_type("payment");
    let mut h = PeopleSim::new(Path::new("/nonexistent"), 200);
    h.tickets = Some(Arc::new(pack()));
    h.player_cabin = Some(player_bus("payment").cabin);
    h.people = vec![boarding(&ty, 1, Task::WalkingToBus)];

    // A plugin can set the payment while the passenger is boarding.
    let payment = TicketChange {
        payment: Some(20.0),
        ..Default::default()
    };
    assert_eq!(h.set_ticket(1, &payment, "x"), Ok(()));
    assert_eq!(h.pax(0).unwrap().plugin().payment, Some(20.0));

    // Negative payments are refused without changing the existing value.
    let invalid = TicketChange {
        payment: Some(-1.0),
        ..Default::default()
    };
    assert_eq!(h.set_ticket(1, &invalid, "x"), Err("invalid payment"));
    assert_eq!(h.pax(0).unwrap().plugin().payment, Some(20.0));

    // It is still allowed on the way to the cash desk (sub < 4).
    {
        let p = h.pax_mut(0).unwrap();
        p.inside = Some(BusId::Player);
        p.task = Task::InBusToPlace;
        p.sub = 3;
    }

    let updated_payment = TicketChange {
        payment: Some(10.0),
        ..Default::default()
    };
    assert_eq!(h.set_ticket(1, &updated_payment, "x"), Ok(()));
    assert_eq!(h.pax(0).unwrap().plugin().payment, Some(10.0));

    // Once the passenger has reached the cash desk (sub >= 4),
    // the payment can no longer be changed.
    h.pax_mut(0).unwrap().sub = 4;
    assert_eq!(
        h.set_ticket(1, &payment, "x"),
        Err("already asked for the ticket")
    );
    assert_eq!(h.pax(0).unwrap().plugin().payment, Some(10.0));

    // Starting a new ticket clears the previous payment.
    h.new_ticket(0, TICKET_BUY, 1, 2.7, false);
    assert_eq!(h.pax(0).unwrap().plugin().payment, None);
}

#[test]
fn payment_is_rejected_for_a_pass_holder() {
    let ty = human_type("pass-payment");
    let mut h = PeopleSim::new(Path::new("/nonexistent"), 200);

    h.tickets = Some(Arc::new(pack()));
    h.player_cabin = Some(player_bus("pass-payment").cabin);
    h.people = vec![boarding(&ty, 1, Task::WalkingToBus)];

    // This passenger does not buy a ticket.
    h.new_ticket(0, TICKET_NONE, 0, 0.0, false);

    let payment = TicketChange {
        payment: Some(20.0),
        ..Default::default()
    };

    assert_eq!(
        h.set_ticket(1, &payment, "x"),
        Err("payment requires a ticket purchase")
    );

    assert_eq!(h.pax(0).unwrap().plugin().payment, None);
}

/// Review of #2708: a buyer always has a ticket of the pack the driver can give - else they
/// wait at the desk for a ticket that cannot be given.
#[test]
fn a_buyer_always_has_a_ticket_of_the_pack() {
    let ty = human_type("buyer");
    let mut h = PeopleSim::new(Path::new("/nonexistent"), 200);
    h.tickets = Some(Arc::new(pack()));
    h.player_cabin = Some(player_bus("buyer").cabin);
    h.people = vec![boarding(&ty, 1, Task::WalkingToBus), boarding(&ty, 2, Task::WalkingToBus)];
    // a pass holder (no ticket, as `decide_pax_ticket` leaves them) made a buyer: the pack's
    // ticket for their age, with its price
    h.new_ticket(0, TICKET_NONE, 0, 0.0, false);
    let buy = TicketChange { kind: Some(TICKET_BUY), ..Default::default() };
    assert_eq!(h.set_ticket(1, &buy, "x"), Ok(()));
    let p = h.pax(0).unwrap();
    assert_eq!(p.ticket, TICKET_BUY);
    assert!((1..=2).contains(&p.ticket_id), "ticket {}", p.ticket_id);
    assert_eq!(p.price, [2.7, 1.4][p.ticket_id as usize - 1]);
    // ticket 0 for a buyer: refused, also together with the kind
    let none = TicketChange { ticket: Some(0), ..Default::default() };
    assert_eq!(h.set_ticket(1, &none, "x"), Err("a buyer needs a ticket of the pack (from 1)"));
    assert_eq!(h.set_ticket(1, &TicketChange { kind: Some(TICKET_BUY), ..none.clone() }, "x"), Err("a buyer needs a ticket of the pack (from 1)"));
    // a child no ticket of the pack is for: buying only with a ticket given
    h.people[1].age = 8.0;
    h.new_ticket(1, TICKET_NONE, 0, 0.0, false);
    assert_eq!(h.set_ticket(2, &buy, "x"), Err("no ticket of the pack is for their age: give one with `ticket`"));
    assert_eq!(h.pax(1).unwrap().ticket, TICKET_NONE, "nothing changed");
    assert_eq!(h.set_ticket(2, &TicketChange { ticket: Some(2), ..buy.clone() }, "x"), Ok(()));
    let p = h.pax(1).unwrap();
    assert_eq!((p.ticket, p.ticket_id, p.price), (TICKET_BUY, 2, 1.4));
}

/// Review of #2708: with no plugin the desk asks for what it always asked for - for a buyer
/// no ticket fits (0), the pack's first at its value; a plugin's price only once it set one.
/// And nobody waits for plugins that are gone.
#[test]
fn without_a_plugin_the_desk_asks_as_before() {
    let ty = human_type("asbefore");
    let mut h = PeopleSim::new(Path::new("/nonexistent"), 200);
    h.tickets = Some(Arc::new(pack()));
    h.people = vec![boarding(&ty, 1, Task::WalkingToBus)];
    h.new_ticket(0, TICKET_BUY, 0, 0.0, false);
    assert_eq!(h.ticket_request(h.pax(0).unwrap()), ("Fahrschein".to_string(), 2.7));
    // somebody who validates (or has a pass) asks for nothing - reported so too
    h.new_ticket(0, TICKET_STAMP, 0, 0.0, false);
    assert_eq!(h.ticket_request(h.pax(0).unwrap()), (String::new(), 0.0));
    h.new_ticket(0, TICKET_BUY, 2, 1.4, false);
    assert_eq!(h.ticket_request(h.pax(0).unwrap()), ("Kurzstrecke".to_string(), 1.4));
    let zones = TicketChange { name: Some("IDOL 4001".into()), price: Some(64.0), ..Default::default() };
    assert_eq!(h.set_ticket(1, &zones, "telmax"), Ok(()));
    assert_eq!(h.ticket_request(h.pax(0).unwrap()), ("IDOL 4001".to_string(), 64.0));
    // reported and waiting - until no plugin listens any more
    h.ticket_hook = true;
    h.new_ticket(0, TICKET_BUY, 1, 2.7, true);
    assert!(h.ticket_pending(0));
    h.ticket_hook = false;
    assert!(!h.ticket_pending(0));
    // a passenger nobody changed carries no plugin record (`Pax` stays small)
    assert!(h.pax(0).unwrap().plugin.is_none());
}
