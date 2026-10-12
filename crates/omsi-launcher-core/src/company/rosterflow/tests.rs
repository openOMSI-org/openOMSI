use super::super::network::tours_of_day;
use super::super::studio::{self as st, Swap, Target};
use super::super::{found, Founding};
use super::*;
use crate::company::model::CompanyLine;
use crate::{StopInfo, TourInfo, TripInfo};

/// A trip of the timetable: from `a` to `b`, `dep` to `arr` (minutes), with passengers.
fn tt(name: &str, a: &str, b: &str, dep: i32, arr: i32) -> TripInfo {
    TripInfo {
        name: name.into(),
        index: 0,
        line: "5".into(),
        from: a.into(),
        terminus: format!("{b} Bf"),
        departure: dep as f64 * 60.0,
        arrival: arr as f64 * 60.0,
        stops: (0..6).map(|s| StopInfo { name: match s { 0 => a.to_string(), 5 => b.to_string(), _ => format!("S{s}") }, id: s, arr: 0.0, dep: 0.0 }).collect(),
        km: (arr - dep) as f64 * 0.4,
    }
}

/// Line 5 between A and B with these tours (number, first departure, trips): trips of 50
/// minutes back and forth from A, ten minutes at each end.
fn line_of(tours: &[(&str, i32, i32)]) -> LineInfo {
    let tour = |&(no, from, n): &(&str, i32, i32)| TourInfo {
        number: no.into(),
        ai_group: String::new(),
        first: 0.0,
        last: 0.0,
        days: "daily".into(),
        runs: true,
        next_run: None,
        trips: (0..n)
            .map(|k| {
                let (a, b) = if k % 2 == 0 { ("A", "B") } else { ("B", "A") };
                tt(&format!("5 {no}-{k}"), a, b, from + k * 60, from + k * 60 + 50)
            })
            .collect(),
    };
    LineInfo { name: "Linie5".into(), user_allowed: true, termini: vec!["A".into(), "B".into()], tours: tours.iter().map(tour).collect() }
}

fn line5() -> LineInfo {
    line_of(&[("1", 360, 8), ("2", 390, 8)])
}

fn company() -> Company {
    let mut c = found(&Founding { name: "Flow".into(), date: "2024-03-04".into(), ..Default::default() }, "Luc");
    c.mode = super::super::Mode::Advanced;
    c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), numbers: vec!["5".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
    c
}

fn day(c: &Company, l: &LineInfo) -> Vec<TourOfDay> {
    tours_of_day(c, std::slice::from_ref(l), &c.date)
}

fn block<'a>(tours: &'a [TourOfDay], no: &str) -> (usize, &'a TourOfDay) {
    tours.iter().enumerate().find(|(_, t)| t.tour == no).unwrap()
}

fn wd(c: &Company) -> u8 {
    super::super::plan::weekday_of(&c.date)
}

#[test]
fn trips_move_together_with_their_links_and_a_locked_bus_stays_as_it_is() {
    let mut c = company();
    let l = line5();
    let w = wd(&c);
    let tours = day(&c, &l);
    let (two, _) = block(&tours, "2");
    // tour 2's last two trips linked: moving the last takes the one before along
    link(&mut c, w, &tours, two, &[6, 7]).unwrap();
    assert_eq!(linked(&c, w, "Linie5", &TripRef::of(&tours[two].trips[7])).len(), 2);
    st::move_trip(&mut c, w, &tours, two, 7, Target::New).unwrap();
    let after = day(&c, &l);
    assert_eq!(block(&after, "3").1.trips.len(), 2);
    assert_eq!(block(&after, "2").1.trips.len(), 6);
    // a link needs two trips of one bus
    assert_eq!(link(&mut c, w, &after, 0, &[0]), Err(LINK_TWO));
    // two trips picked at once go together
    let mut c2 = company();
    let tours = day(&c2, &l);
    let (one, _) = block(&tours, "1");
    st::move_trips(&mut c2, w, &tours, one, &[6, 7], Target::New).unwrap();
    assert_eq!(block(&day(&c2, &l), "3").1.trips.len(), 2);
    // a locked bus: nothing moves off it or onto it, it is not cut
    let mut c3 = company();
    let tours = day(&c3, &l);
    let (one, _) = block(&tours, "1");
    let (two, _) = block(&tours, "2");
    set_block_lock(&mut c3, w, "Linie5", "1", true);
    assert!(block_locked(&c3, w, "Linie5", "1"));
    assert_eq!(st::move_trip(&mut c3, w, &tours, one, 7, Target::New), Err(st::LOCKED));
    assert_eq!(st::cut(&mut c3, w, &tours, one, 2), Err(st::LOCKED));
    assert_eq!(st::plan_swap(&c3, w, &tours, two, &[3], one, Swap::Span), Err(st::LOCKED));
    set_block_lock(&mut c3, w, "Linie5", "1", false);
    assert!(st::cut(&mut c3, w, &tours, one, 2).is_ok());
}

#[test]
fn trips_are_swapped_between_buses_where_they_fit() {
    let mut c = company();
    let l = line_of(&[("1", 360, 6), ("2", 360, 6)]);
    let w = wd(&c);
    let tours = day(&c, &l);
    let (one, t1) = block(&tours, "1");
    let (two, t2) = block(&tours, "2");
    let a3 = TripRef::of(&t1.trips[3]);
    let b3 = TripRef::of(&t2.trips[3]);
    // the two buses run the same times: trip 3 of each swapped
    st::swap(&mut c, w, &tours, one, &[3], two, Swap::Span).unwrap();
    let after = day(&c, &l);
    assert!(block(&after, "1").1.trips.iter().any(|x| b3.is(x)));
    assert!(block(&after, "2").1.trips.iter().any(|x| a3.is(x)));
    // from trip 4 on: both tails change bus
    let mut c2 = company();
    let tours = day(&c2, &l);
    let a5 = TripRef::of(&tours[one].trips[5]);
    st::swap(&mut c2, w, &tours, one, &[4], two, Swap::FromHere).unwrap();
    let after = day(&c2, &l);
    assert!(block(&after, "2").1.trips.iter().any(|x| a5.is(x)));
    assert_eq!(block(&after, "1").1.trips.len(), 6);
    // a trip swapped with a bus that is elsewhere then: refused
    let l2 = line_of(&[("1", 360, 6), ("2", 390, 6)]);
    let c3 = company();
    let tours = day(&c3, &l2);
    assert_eq!(st::plan_swap(&c3, w, &tours, 0, &[2], 1, Swap::Span), Err(st::SWAP_FAILS));
}

#[test]
fn an_unassigned_trip_waits_on_the_stack_and_is_assigned_again() {
    let mut c = company();
    let l = line5();
    let w = wd(&c);
    let tours = day(&c, &l);
    let (two, t2) = block(&tours, "2");
    let gone = t2.trips[7].clone();
    st::unassign(&mut c, w, &tours, two, &[7]).unwrap();
    let after = day(&c, &l);
    assert_eq!(block(&after, "2").1.trips.len(), 7);
    let s = stack(&c, std::slice::from_ref(&l), w);
    assert_eq!(s.len(), 1);
    assert!(TripRef::of(&s[0].1) == TripRef::of(&gone));
    // the day says so, and counts it unassigned
    let b = board(&c, std::slice::from_ref(&l), &c.date.clone());
    assert!(b.issues.iter().any(|i| i.text == I_UNASSIGNED && i.scope == Scope::Global));
    assert_eq!((b.figures.assigned, b.figures.trips), (15, 16));
    // back onto tour 2
    let (two, _) = block(&after, "2");
    st::assign(&mut c, w, &after, "Linie5", &s[0].1, Target::Block(two)).unwrap();
    assert!(stack(&c, std::slice::from_ref(&l), w).is_empty());
    assert_eq!(block(&day(&c, &l), "2").1.trips.len(), 8);
}

#[test]
fn a_duty_signs_on_and_off_and_its_paid_time_leaves_the_split_out() {
    let mut c = company();
    let l = line5();
    let w = wd(&c);
    let tours = day(&c, &l);
    // tour 1 cut after its second trip, tour 2 after its sixth; tour 1's first piece and tour 2's
    // last one driver's split duty
    st::cut(&mut c, w, &tours, block(&tours, "1").0, 2).unwrap();
    let tours = day(&c, &l);
    st::cut(&mut c, w, &tours, block(&tours, "2").0, 6).unwrap();
    let tours = day(&c, &l);
    st::join(&mut c, w, &tours, (block(&tours, "1").0, 0), (block(&tours, "2").0, 1)).unwrap();
    let b = board(&c, std::slice::from_ref(&l), &c.date.clone());
    let s = b.shaped.iter().find(|s| s.spans.len() == 2).unwrap();
    let p = Prefs::default();
    // 06:00 less the pre-trip check (5) and the sign-on (10): 05:45
    assert_eq!(s.spans[0].from, 355);
    assert_eq!(s.start, 345);
    // the first piece ends on arrival at 07:50; the second begins at departure, 12:30, and runs
    // to 14:20 with the post-trip check and the sign-off
    assert_eq!(s.spans[0].to, 470);
    assert_eq!(s.spans[1].from, 750);
    assert_eq!(s.end, 860 + p.post_trip + p.sign_off);
    // the split: from the sign-off after the first piece to the sign-on before the second
    assert_eq!(s.splits, vec![(470 + p.sign_off_split, 750 - p.sign_on_split)]);
    assert_eq!(s.work, s.spread() - (750 - 5 - 475));
    assert_eq!(s.paid, s.work.max(MIN_PAID));
    // its type: a split duty
    assert_eq!(s.kind_name(&p), "Split shift");
    // the driving: four trips of 50 minutes
    assert_eq!(s.driving, 200);
    // paid time set by hand
    let key = s.key.clone().unwrap();
    note_mut(&mut c, w, &key).paid = Some(600);
    let b = board(&c, std::slice::from_ref(&l), &c.date.clone());
    assert_eq!(b.shaped.iter().find(|s| s.spans.len() == 2).unwrap().paid, 600);
}

#[test]
fn relief_timing_sets_when_the_driver_changes_in_the_layover() {
    let l = line5();
    let c = company();
    let tours = day(&c, &l);
    let (_, t1) = block(&tours, "1");
    // a piece of tour 1 from its third trip (08:00) to its fifth (until 10:50); the bus arrives
    // at 07:50 before it and leaves again at 11:00 after it
    let r = 2..5;
    let mut p = Prefs::default();
    assert_eq!(span_of(&p, t1, &r), (480, 650));
    p.relief = ReliefTiming::OnArrival;
    assert_eq!(span_of(&p, t1, &r), (470, 650));
    p.relief = ReliefTiming::OnDeparture;
    assert_eq!(span_of(&p, t1, &r), (480, 660));
    p.relief = ReliefTiming::MidLayover;
    assert_eq!(span_of(&p, t1, &r), (475, 655));
    p.relief = ReliefTiming::BeforeDeparture;
    p.relief_before = 3;
    assert_eq!(span_of(&p, t1, &r), (477, 657));
    // the block's first piece: the pre-trip check before it
    assert_eq!(span_of(&Prefs::default(), t1, &(0..2)).0, 355);
}

#[test]
fn travel_to_a_relief_point_elsewhere_takes_its_time() {
    let mut c = company();
    // line 5 between A and B, line 7 between C and D
    let l5 = line_of(&[("1", 360, 2)]);
    let mut l7 = line_of(&[("1", 510, 2)]);
    l7.name = "Linie7".into();
    for t in l7.tours.iter_mut().flat_map(|t| t.trips.iter_mut()) {
        t.from = if t.from == "A" { "C".into() } else { "D".into() };
        t.stops[0].name = t.from.clone();
        t.stops[5].name = if t.from == "C" { "D".into() } else { "C".into() };
    }
    c.lines.push(CompanyLine { name: "Linie7".into(), number: "7".into(), numbers: vec!["7".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
    let lines = vec![l5, l7];
    let w = wd(&c);
    let tours = tours_of_day(&c, &lines, &c.date);
    let a = tours.iter().position(|t| t.line == "Linie5").unwrap();
    let b = tours.iter().position(|t| t.line == "Linie7").unwrap();
    st::join(&mut c, w, &tours, (a, 0), (b, 0)).unwrap();
    let bd = board(&c, &lines, &c.date.clone());
    let s = bd.shaped.iter().find(|s| s.spans.len() == 2).unwrap();
    // the walk from A to C: five minutes after the first piece
    let walk = s.events.iter().find(|e| matches!(e.kind, EvKind::Travel(Travel::Walk))).unwrap();
    assert_eq!((walk.from, walk.to - walk.from), (s.spans[0].to, 5));
    assert!(!s.issues.iter().any(|&i| bd.issues[i].text == I_TRAVEL));
    // by public transport it takes 15 minutes; set to a staff car on this change, 10
    let key = s.key.clone().unwrap();
    note_mut(&mut c, w, &key).travel = vec![(0, Travel::Car)];
    let bd = board(&c, &lines, &c.date.clone());
    let s = bd.shaped.iter().find(|s| s.spans.len() == 2).unwrap();
    let car = s.events.iter().find(|e| matches!(e.kind, EvKind::Travel(_))).unwrap();
    assert_eq!((car.kind, car.to - car.from), (EvKind::Travel(Travel::Car), 10));
    // a travel longer than the gap: an issue
    c.planning.flow.prefs.car = 200;
    let bd = board(&c, &lines, &c.date.clone());
    assert!(bd.issues.iter().any(|i| i.text == I_TRAVEL && i.hard));
}

#[test]
fn breaks_meals_and_continuous_driving_are_checked() {
    let p = Prefs::default();
    // 5 h 30 of driving with a 10-minute layover in it: the layover is no break of 15
    assert_eq!(longest_stretch(0, 330, &[(150, 160)], 15), 330);
    // a 20-minute layover is
    assert_eq!(longest_stretch(0, 330, &[(150, 170)], 15), 160);
    // a whole day on tour 1 of 07:50 (06:00-13:50, ten-minute layovers): over 4 h 30 without a
    // break of 15, and no meal break of 30 in a day over 6 h
    let c = company();
    let l = line5();
    let b = board(&c, std::slice::from_ref(&l), &c.date.clone());
    let s = &b.shaped[0];
    let texts: Vec<&str> = s.issues.iter().map(|&i| b.issues[i].text).collect();
    assert!(texts.contains(&I_CONTINUOUS), "{texts:?}");
    assert!(texts.contains(&I_MEAL));
    // the rules off: no issue
    let mut c2 = company();
    c2.planning.flow.prefs.meal.on = false;
    c2.planning.flow.prefs.continuous.on = false;
    let b = board(&c2, std::slice::from_ref(&l), &c2.date.clone());
    assert!(!b.issues.iter().any(|i| i.text == I_CONTINUOUS || i.text == I_MEAL));
    // with layovers of 20 minutes counting as breaks: fine
    let mut c3 = company();
    c3.planning.flow.prefs.break_min = 10;
    c3.planning.flow.prefs.meal.value = 10;
    let b = board(&c3, std::slice::from_ref(&l), &c3.date.clone());
    assert!(!b.issues.iter().any(|i| i.text == I_CONTINUOUS || i.text == I_MEAL));
    let _ = p;
}

#[test]
fn duty_types_classify_the_duties_and_an_unmatched_one_is_other() {
    let p = Prefs::default();
    let t = &p.duty_types;
    assert!(t[0].fits(345, 900, false));
    assert!(!t[0].fits(345, 900, true));
    assert!(t[3].fits(345, 1100, true));
    // a duty from 05:45 to 14:30 is an early one
    let c = company();
    let l = line5();
    let b = board(&c, std::slice::from_ref(&l), &c.date.clone());
    assert_eq!(b.shaped[0].kind_name(&c.planning.flow.prefs), "Early shift");
    // no type allows it: "Other", an issue
    let mut c2 = company();
    for t in c2.planning.flow.prefs.duty_types.iter_mut() {
        t.allowed = false;
    }
    let b = board(&c2, std::slice::from_ref(&l), &c2.date.clone());
    assert!(b.issues.iter().any(|i| i.text == I_TYPE && i.hard));
    c2.planning.flow.prefs.duty_types.clear();
    let b = board(&c2, std::slice::from_ref(&l), &c2.date.clone());
    assert_eq!(b.shaped[0].kind, None);
    assert_eq!(b.figures.types.last().unwrap(), &(OTHER.to_string(), 2));
    // set by hand
    let mut c3 = company();
    let tours = day(&c3, &l);
    let key = duty_key(&tours, &st::duties(&c3.planning.joined, wd(&c3), &tours)[0]).unwrap();
    let w3 = wd(&c3);
    note_mut(&mut c3, w3, &key).kind = Some("Late shift".into());
    let b = board(&c3, std::slice::from_ref(&l), &c3.date.clone());
    assert!(b.shaped.iter().any(|s| s.kind_set && s.kind_name(&c3.planning.flow.prefs) == "Late shift"));
}

#[test]
fn a_locked_duty_is_not_cut_or_joined_and_optimise_leaves_it() {
    let mut c = company();
    let l = line5();
    let w = wd(&c);
    let tours = day(&c, &l);
    let key = duty_key(&tours, &st::duties(&c.planning.joined, w, &tours)[0]).unwrap();
    set_duty_lock(&mut c, w, &key, true);
    let (i, k) = st::piece_at(&tours, &key).unwrap();
    assert!(piece_locked(&c, w, &tours, i, k));
    assert_eq!(st::cut(&mut c, w, &tours, i, 2), Err(st::DUTY_LOCKED));
    let other = if i == 0 { 1 } else { 0 };
    assert_eq!(st::join(&mut c, w, &tours, (other, 0), (i, k)), Err(st::DUTY_LOCKED));
    // Optimise keeps its bus and its duty as they are
    let (lays, _) = optimise_with(&c, std::slice::from_ref(&l), &c.date.clone(), true, true);
    let kept = lays.iter().flat_map(|x| x.blocks.iter()).find(|b| b.tour == tours[i].tour).unwrap();
    assert_eq!(kept.trips.len(), tours[i].trips.iter().filter(|x| !x.origin.is_empty()).count());
    set_duty_lock(&mut c, w, &key, false);
    assert!(c.planning.flow.sheet.notes.is_empty(), "a blank note is dropped");
}

#[test]
fn spare_duties_and_custom_work_count_as_work() {
    let mut c = company();
    let l = line5();
    let w = wd(&c);
    let id = add_spare(&mut c, w, "Spare", 810, 1380);
    let b = board(&c, std::slice::from_ref(&l), &c.date.clone());
    let s = b.shaped.iter().find(|s| s.spare == Some(id)).unwrap();
    assert_eq!((s.start, s.end, s.work, s.paid), (810, 1380, 570, 570));
    assert_eq!(s.label(0), "9001");
    assert_eq!(b.figures.duties, 3);
    assert!(remove_spare(&mut c, w, id));
    // custom work in a duty lengthens it
    let tours = day(&c, &l);
    let key = duty_key(&tours, &st::duties(&c.planning.joined, w, &tours)[0]).unwrap();
    note_mut(&mut c, w, &key).custom.push(Custom { kind: "Office".into(), from: 900, to: 960, place: String::new() });
    let b = board(&c, std::slice::from_ref(&l), &c.date.clone());
    let s = b.shaped.iter().find(|s| s.key.as_ref().is_some_and(|k| k.is(&key))).unwrap();
    assert_eq!(s.end, 960);
    assert!(s.events.iter().any(|e| e.kind == EvKind::Custom(0)));
}

#[test]
fn the_validation_lists_vehicle_and_duty_issues_and_dismissed_ones_stay_dismissed() {
    let mut c = company();
    // a layover of five minutes against the least of ten
    let l = line5();
    c.planning.flow.prefs.layover.value = 15;
    let b = board(&c, std::slice::from_ref(&l), &c.date.clone());
    let lay: Vec<&Issue> = b.issues.iter().filter(|i| i.text == I_LAYOVER).collect();
    assert_eq!(lay.len(), 14);
    assert!(lay.iter().all(|i| i.scope == Scope::Vehicle && !i.hard && i.penalty == 20_00));
    // more duties than drivers, more buses at the peak than the company has
    assert!(b.issues.iter().any(|i| i.text == I_DRIVERS));
    // dismissed: in the dismissed list, out of the open one
    let w = wd(&c);
    let open = b.open(&c, Scope::Vehicle, false).len();
    let key = lay[0].key.clone();
    set_dismissed(&mut c, w, &key, true);
    let b = board(&c, std::slice::from_ref(&l), &c.date.clone());
    assert_eq!(b.open(&c, Scope::Vehicle, false).len(), open - 1);
    assert_eq!(b.open(&c, Scope::Vehicle, true).len(), 1);
    // the words filled in
    let i = b.issues.iter().find(|i| i.text == I_LAYOVER).unwrap();
    assert_eq!(issue_text(i, &|s| s.to_string()), "Layover of 0:10 at B, shorter than 0:15");
}

#[test]
fn the_figures_count_efficiency_cost_and_histograms() {
    let c = company();
    let l = line5();
    let b = board(&c, std::slice::from_ref(&l), &c.date.clone());
    let f = &b.figures;
    assert_eq!((f.duties, f.blocks, f.pvr), (2, 2, 2));
    // no empty kilometres: all service
    assert!((f.vehicle_eff() - 1.0).abs() < 1e-9);
    // driving 16 trips of 50 minutes against what is paid
    assert_eq!(f.driving, 800);
    assert!(f.crew_eff() > 0.7 && f.crew_eff() < 1.0, "{}", f.crew_eff());
    assert!(f.crew_cost > 0 && f.vehicle_cost > 0);
    assert_eq!(f.total(), f.crew_cost + f.vehicle_cost);
    assert_eq!(f.paid_bands.iter().sum::<usize>(), 2);
    assert_eq!(f.paid_bands[2], 2);
    assert_eq!(f.types.iter().map(|t| t.1).sum::<usize>(), 2);
    // the soft rules broken add their penalties
    let mut c2 = company();
    c2.planning.flow.prefs.layover.value = 15;
    let b2 = board(&c2, std::slice::from_ref(&l), &c2.date.clone());
    assert!(b2.figures.penalties >= 14 * 20_00);
    assert!(b2.figures.algorithmic() > f.algorithmic());
}

#[test]
fn the_draft_is_the_studios_until_it_is_saved_or_discarded() {
    let mut c = company();
    let l = line5();
    let w = wd(&c);
    let mut v = view(&c);
    let tours = day(&v, &l);
    st::move_trip(&mut v, w, &tours, block(&tours, "2").0, 7, Target::New).unwrap();
    keep_draft(&mut c, &v);
    assert!(has_draft(&c));
    // the game still runs the saved day: two buses; the studio shows three
    assert_eq!(day(&c, &l).len(), 2);
    assert_eq!(day(&view(&c), &l).len(), 3);
    // undo to a state, then discard
    let before = draft_of(&c);
    discard_draft(&mut c);
    assert!(!has_draft(&c));
    set_draft(&mut c, before);
    assert!(has_draft(&c));
    assert!(save_draft(&mut c));
    assert_eq!(day(&c, &l).len(), 3);
    assert!(!has_draft(&c));
    // an old company without RosterFlow's fields loads, and an empty flow is not written
    let json = serde_json::to_value(&company().planning).unwrap();
    assert!(json.get("flow").is_none());
    let p: super::super::plan::Planning = serde_json::from_value(json).unwrap();
    assert_eq!(p.flow, Flow::default());
    // a layout without a stack loads with none
    let lay: LineLayout = serde_json::from_str(r#"{"weekday":0,"line":"L","blocks":[]}"#).unwrap();
    assert!(lay.stack.is_empty());
}

#[test]
fn optimise_covers_every_trip_with_few_buses_and_cuts_the_long_blocks() {
    let mut c = company();
    // tours that hand over badly: four short tours a bus can chain
    let l = line_of(&[("1", 360, 2), ("2", 480, 2), ("3", 600, 2), ("4", 720, 2)]);
    let (lays, joined) = optimise(&c, std::slice::from_ref(&l), &c.date.clone());
    let w = wd(&c);
    apply(&mut c, w, lays, joined);
    let tours = day(&c, &l);
    assert_eq!(tours.iter().map(|t| t.trips.len()).sum::<usize>(), 8);
    assert_eq!(st::peak(&tours), 1);
    assert_eq!(tours.len(), 1, "one bus runs them all");
    // a bus from 06:00 to 13:50: cut where a driver may work no longer without a break
    let b = board(&c, std::slice::from_ref(&l), &c.date.clone());
    assert!(b.shaped.len() >= 2);
    assert!(b.shaped.iter().all(|s| s.platform <= c.planning.flow.prefs.continuous.value + 20), "{:?}", b.shaped.iter().map(|s| s.platform).collect::<Vec<_>>());
    // nothing to optimise for the vehicles: the blocks kept, only the duties made
    let mut c2 = company();
    c2.planning.flow.prefs.optimise_vehicles = false;
    let l5 = line5();
    let (lays, _) = optimise_with(&c2, std::slice::from_ref(&l5), &c2.date.clone(), false, true);
    assert_eq!(lays[0].blocks.len(), 2);
    // linked trips stay on one bus
    let mut c3 = company();
    let tours = day(&c3, &l);
    let w = wd(&c3);
    link(&mut c3, w, &tours, 0, &[0, 1]).unwrap();
    let (lays, _) = optimise_with(&c3, std::slice::from_ref(&l), &c3.date.clone(), true, true);
    let refs = linked(&c3, w, "Linie5", &TripRef::of(&tours[0].trips[1]));
    assert!(lays[0].blocks.iter().any(|b| refs.iter().all(|r| b.trips.contains(r))));
    // Optimise is never dearer than the day as it is
    for l in [line5(), line_of(&[("1", 360, 8), ("2", 400, 6), ("3", 700, 5)])] {
        let c4 = company();
        let date = c4.date.clone();
        let before = weighed(&board(&c4, std::slice::from_ref(&l), &date));
        let (lays, joined) = optimise(&c4, std::slice::from_ref(&l), &date);
        let mut c5 = c4.clone();
        let w5 = wd(&c5);
        apply(&mut c5, w5, lays, joined);
        assert!(weighed(&board(&c5, std::slice::from_ref(&l), &date)) <= before);
    }
}

#[test]
fn a_line_has_its_own_colour_or_the_palettes() {
    let mut c = company();
    assert_eq!(line_colour(&c, "Linie5"), PALETTE[0]);
    assert!(set_line_colour(&mut c, "linie5", Some([1, 2, 255])));
    assert_eq!(c.lines[0].colour, "#0102ff");
    assert_eq!(line_colour(&c, "Linie5"), [1, 2, 255]);
    set_line_colour(&mut c, "Linie5", None);
    assert_eq!(line_colour(&c, "Linie5"), PALETTE[0]);
    assert_eq!(parse_hex("#zz0000"), None);
    assert_eq!((hm(65), clock(1445)), ("1:05".to_string(), "00:05".to_string()));
}

#[test]
fn a_lessons_snapshot_puts_the_draft_the_saved_planning_and_the_colours_back() {
    let mut c = company();
    let l = line5();
    let w = wd(&c);
    let snap = Snapshot::take(&c);
    assert!(!snap.restore(&mut c.clone()), "nothing changed: nothing to put back");
    // a lesson cuts a duty in the draft, saves it, cuts again, colours the line and locks a bus
    let tours = day(&c, &l);
    let (i, t) = block(&tours, "1");
    let at = (1..t.trips.len()).find(|&k| st::can_cut(&c, &tours, i, k).is_ok()).expect("a place to cut");
    let mut v = view(&c);
    st::cut(&mut v, w, &tours, i, at).unwrap();
    keep_draft(&mut c, &v);
    assert!(has_draft(&c));
    assert!(save_draft(&mut c));
    assert!(!snap.same_saved(&c), "the saved planning changed");
    let tours = day(&c, &l);
    let mut v = view(&c);
    set_block_lock(&mut v, w, &tours[0].line, &tours[0].tour, true);
    keep_draft(&mut c, &v);
    set_line_colour(&mut c, "Linie5", Some([9, 9, 9]));
    assert!(has_draft(&c));
    // "Undo the lesson": everything as it was, the draft gone
    assert!(snap.restore(&mut c));
    assert_eq!(Snapshot::take(&c), snap);
    assert!(snap.same_saved(&c));
    assert!(!has_draft(&c));
    assert_eq!(line_colour(&c, "Linie5"), PALETTE[0]);
    assert_eq!(board(&c, std::slice::from_ref(&l), &c.date.clone()).figures, board(&company(), std::slice::from_ref(&l), &company().date).figures);
    // a line added during the lesson keeps its colour
    c.lines.push(CompanyLine { name: "Neu".into(), colour: "#123456".into(), ..Default::default() });
    snap.restore(&mut c);
    assert_eq!(c.lines[1].colour, "#123456");
}
