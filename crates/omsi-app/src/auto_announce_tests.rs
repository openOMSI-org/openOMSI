use super::*;

#[test]
fn the_counter_is_parked_before_the_first_next_stop_recording() {
    // (the terminus' own recording at 0, the line's start recording at 1, the first
    // "next stop is ..." one at 2: the counter is parked at 1)
    let beijing = TwoRecordings { first: 2, pairs: 6 };
    assert_eq!(park_target(beijing, Some(0.0)), Some(1));
    // (a map that repeats the terminus' arrival before its pairs: parked at 2)
    let zhengzhou = TwoRecordings { first: 3, pairs: 5 };
    assert_eq!(park_target(zhengzhou, Some(0.0)), Some(2));
    // (already there, or past it: nothing to do)
    assert_eq!(park_target(beijing, Some(1.0)), None);
    assert_eq!(park_target(beijing, Some(2.0)), None);
    // (no counter to read: nothing to do)
    assert_eq!(park_target(beijing, None), None);
}

/// An IBIS whose `IBIS_vor` plays (counted) and steps the counter on, or not, with the
/// electrics on; with them off, or `deaf` (another IBIS mode, the script busy), a press
/// does nothing. A `late` one plays at the next frame.
struct FakeIbis {
    busstop: f32,
    steps: bool,
    plays: usize,
    power: bool,
    deaf: bool,
    late: bool,
    silent_key: bool,
    pending: usize,
    route: f32,
}

impl FakeIbis {
    fn new(steps: bool) -> Self {
        FakeIbis {
            busstop: 0.0,
            steps,
            plays: 0,
            power: true,
            deaf: false,
            late: false,
            silent_key: true,
            pending: 0,
            route: 57.0,
        }
    }

    /// A frame of the scripts: what a `late` IBIS was pressed for plays now.
    fn frame(&mut self) {
        self.plays += std::mem::take(&mut self.pending);
    }
}

impl Ibis for FakeIbis {
    fn var(&self, name: &str) -> Option<f32> {
        match name {
            "IBIS_busstop" => Some(self.busstop),
            "IBIS_RouteIndex" => Some(self.route),
            "elec_busbar_main" => Some(if self.power { 1.0 } else { 0.0 }),
            _ => None,
        }
    }

    fn trigger(&mut self, name: &str) -> bool {
        match name {
            "IBIS_vor" => {
                if self.power && !self.deaf {
                    if self.late {
                        self.pending += 1;
                    } else {
                        self.plays += 1;
                    }
                    if self.steps {
                        self.busstop += 1.0;
                    }
                }
                true
            }
            "IBIS_vor_stumm" if self.silent_key => {
                if self.power {
                    self.busstop += 1.0;
                }
                true
            }
            _ => name.ends_with("_off"),
        }
    }

    fn recordings(&self) -> u64 {
        self.plays as u64
    }
}

const LAYOUT: Option<TwoRecordings> = Some(TwoRecordings {
    first: 2,
    pairs: 18,
});

/// `secs` at a stop at 60 frames a second, as `step` runs it there, from `t0`; `bus`
/// may be changed at each frame.
fn stand(
    a: &mut AutoAnnounce,
    bus: &mut FakeIbis,
    next_stop: usize,
    t0: f64,
    secs: f64,
    at: &mut dyn FnMut(f64, &mut FakeIbis),
) {
    a.at_stop = Some(true);
    for f in 0..(secs * 60.0) as usize {
        let now = t0 + f as f64 / 60.0;
        at(now, bus);
        bus.frame();
        a.park(bus, next_stop, LAYOUT, now);
        a.press_step(bus, now);
    }
}

#[test]
fn the_start_recording_plays_once_even_where_the_counter_stays() {
    // (the O560 of #3282: its script plays `welcome…` and leaves the counter at 0 - it
    // was pressed again every second, 63 times in a minute)
    let mut keeps = FakeIbis::new(false);
    stand(
        &mut AutoAnnounce::new(),
        &mut keeps,
        0,
        0.0,
        60.0,
        &mut |_, _| {},
    );
    assert_eq!(keeps.plays, 1);
    // (a stock script steps on to the start recording's entry: once, and parked there)
    let mut steps = FakeIbis::new(true);
    stand(
        &mut AutoAnnounce::new(),
        &mut steps,
        0,
        0.0,
        60.0,
        &mut |_, _| {},
    );
    assert_eq!((steps.plays, steps.busstop), (1, 1.0));
}

#[test]
fn the_start_recording_waits_for_the_electrics() {
    // (the driver switches on 30 s after the duty starts: the start recording is played
    // then, once - not lost because the first presses went unheard)
    let mut bus = FakeIbis::new(false);
    bus.power = false;
    stand(
        &mut AutoAnnounce::new(),
        &mut bus,
        0,
        0.0,
        60.0,
        &mut |now, b| b.power = now >= 30.0,
    );
    assert_eq!(bus.plays, 1);
}

#[test]
fn another_route_or_the_first_stop_again_plays_the_start_recording_again() {
    let mut a = AutoAnnounce::new();
    let mut bus = FakeIbis::new(false);
    stand(&mut a, &mut bus, 0, 0.0, 10.0, &mut |_, _| {});
    // (another route typed into the IBIS at the first stop)
    stand(&mut a, &mut bus, 0, 10.0, 10.0, &mut |_, b| b.route = 58.0);
    assert_eq!(bus.plays, 2);
    // (the duty sent on and back to its first stop on the same trip)
    stand(&mut a, &mut bus, 1, 20.0, 1.0, &mut |_, _| {});
    stand(&mut a, &mut bus, 0, 21.0, 10.0, &mut |_, _| {});
    assert_eq!(bus.plays, 3);
}

/// An arrival press at entry 5 with the IBIS at 4, for `secs`: the electrics come on at
/// `power_at`, the IBIS takes presses from `heard_at`.
fn arrival(bus: &mut FakeIbis, secs: f64, power_at: f64, heard_at: f64) {
    let mut a = AutoAnnounce::new();
    a.at_stop = Some(true);
    a.want = Some(Want {
        entry: 5,
        until: ARRIVAL_WAIT,
        at_stop: true,
        start: false,
        recordings: None,
    });
    bus.busstop = 4.0;
    for f in 0..(secs * 60.0) as usize {
        let now = f as f64 / 60.0;
        bus.power = now >= power_at;
        bus.deaf = now < heard_at;
        bus.frame();
        a.press_step(bus, now);
    }
}

#[test]
fn an_arrival_is_played_once_and_waits_for_the_electrics() {
    // (a script that plays without stepping on: once, not every second for 20 s)
    let mut keeps = FakeIbis::new(false);
    arrival(&mut keeps, 10.0, 0.0, 0.0);
    assert_eq!(keeps.plays, 1);
    // (electrics off for the first 3 s: tried again until they are on, then played once)
    let mut late = FakeIbis::new(true);
    arrival(&mut late, 10.0, 3.0, 0.0);
    assert_eq!((late.plays, late.busstop), (1, 5.0));
}

#[test]
fn an_arrival_the_ibis_did_not_take_is_pressed_again() {
    // (electrics on, but the IBIS in another mode for the first 3 s: the press it did not
    // take is not counted as played, and the stop is announced once it takes one)
    let mut steps = FakeIbis::new(true);
    arrival(&mut steps, 10.0, 0.0, 3.0);
    assert_eq!((steps.plays, steps.busstop), (1, 5.0));
    let mut keeps = FakeIbis::new(false);
    arrival(&mut keeps, 10.0, 0.0, 3.0);
    assert_eq!(keeps.plays, 1);
}

#[test]
fn a_start_recording_out_of_reach_is_given_up_once() {
    // (no `IBIS_vor_stumm` and the start recording three entries on, the KPP2 IBIS of the
    // Urbino III: left alone, and not asked for again every frame)
    let mut a = AutoAnnounce::new();
    let mut bus = FakeIbis::new(false);
    bus.silent_key = false;
    a.at_stop = Some(true);
    let far = Some(TwoRecordings { first: 4, pairs: 3 });
    for f in 0..600 {
        let now = f as f64 / 60.0;
        a.park(&bus, 0, far, now);
        a.press_step(&mut bus, now);
    }
    assert_eq!(
        (bus.plays, bus.busstop, a.parked, a.want.is_none()),
        (0, 0.0, Some(57), true)
    );
}

#[test]
fn a_recording_played_a_frame_late_is_not_pressed_for_again() {
    // (a script that plays at the next frame and keeps its counter: once, not again a
    // second later)
    let mut bus = FakeIbis::new(false);
    bus.late = true;
    arrival(&mut bus, 10.0, 0.0, 0.0);
    assert_eq!(bus.plays, 1);
    let mut bus = FakeIbis::new(false);
    bus.late = true;
    stand(
        &mut AutoAnnounce::new(),
        &mut bus,
        0,
        0.0,
        60.0,
        &mut |_, _| {},
    );
    assert_eq!(bus.plays, 1);
}

#[test]
fn a_previously_unavailable_hof_is_retried_on_a_new_trip() {
    let mut a = AutoAnnounce::new();
    let mut hof = omsi_vehicle::Hof::default();
    hof.info_busstop_lists = vec![vec![
        "Start".to_string(),
        "welcome19".to_string(),
        "FRoad".to_string(),
        "FRoad1".to_string(),
        "GRoad".to_string(),
        "GRoad1".to_string(),
        "HRoad".to_string(),
        "HRoad1".to_string(),
    ]];
    assert_eq!(a.layout_lookup(0, None), None);
    assert_eq!(a.layout, Some((0, None)));
    assert_eq!(a.layout_lookup(0, Some(&hof)), None);

    a.new_trip(1, true);

    assert_eq!(
        a.layout_lookup(0, Some(&hof)),
        Some(LayoutStatus::Two(TwoRecordings { first: 2, pairs: 3 }))
    );
}

#[test]
fn a_bus_without_the_counter_is_reported_without_pressing_a_key() {
    struct NoCounterBus {
        pressed: bool,
    }
    impl Ibis for NoCounterBus {
        fn var(&self, _name: &str) -> Option<f32> {
            None
        }
        fn trigger(&mut self, _name: &str) -> bool {
            self.pressed = true;
            true
        }
        fn recordings(&self) -> u64 {
            0
        }
    }

    let mut bus = NoCounterBus { pressed: false };
    let mut a = AutoAnnounce::new();
    a.at_stop = Some(true);
    a.pairs = 1;
    a.want = Some(Want {
        entry: 1,
        until: 10.0,
        at_stop: true,
        start: false,
        recordings: None,
    });
    a.press_step(&mut bus, 1.0);
    assert!(!bus.pressed);
    assert!(a.off);
    assert_eq!(a.take_note(), Some(announce::QuietReason::NoBusstopCounter));
    assert!(a.want.is_none());
}

#[test]
fn a_bus_without_the_counter_is_reported_at_first_stop() {
    struct NoCounterBus;
    impl Ibis for NoCounterBus {
        fn var(&self, _name: &str) -> Option<f32> {
            None
        }
        fn trigger(&mut self, _name: &str) -> bool {
            true
        }
        fn recordings(&self) -> u64 {
            0
        }
    }

    let mut a = AutoAnnounce::new();
    a.at_stop = Some(true);
    a.park(&NoCounterBus, 0, Some(TwoRecordings { first: 2, pairs: 3 }), 1.0);
    assert!(a.off);
    assert_eq!(a.take_note(), Some(announce::QuietReason::NoBusstopCounter));
}
