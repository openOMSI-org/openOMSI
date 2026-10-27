use super::*;

fn send<T: Send>() {}

/// What the sim thread is handed must be `Send` (the compiler says so here, not at a call
/// site that may not exist yet).
#[test]
fn sim_state_is_send() {
    send::<TrafficSim>();
    send::<Box<TrafficSim>>();
    send::<crate::people::PeopleSim>();
    send::<crate::VehicleInstance>();
}

#[test]
fn thread_and_inline_give_the_same() {
    let th = SimThread::spawn().unwrap();
    let step = |v: &mut Vec<u32>| {
        for k in 0..1000 {
            v.push(k * 3 + 1);
        }
    };
    let (a, _, _) = th.start(Vec::new(), step).join();
    let (b, _, _) = SimThread::inline(Vec::new(), step).join();
    assert_eq!(a, b);
    // one after the other, in order
    let mut v = vec![0u64];
    for k in 1..50u64 {
        v = th.start(v, move |v| v.push(k)).join().0;
    }
    assert_eq!(v, (0..50).collect::<Vec<_>>());
}

#[test]
fn a_panic_comes_back_at_join() {
    let th = SimThread::spawn().unwrap();
    let f = th.start(1u32, |_| panic!("step failed"));
    let r = std::panic::catch_unwind(AssertUnwindSafe(|| f.join()));
    assert!(r.is_err());
    // and the thread goes on
    assert_eq!(th.start(2u32, |v| *v += 1).join().0, 3);
}

#[test]
fn the_traffic_goes_out_and_comes_back() {
    let mut p = Pipeline::default();
    // nothing owed: nothing starts
    p.start(None, None, None);
    assert!(!p.busy());
    let mut sim = empty_traffic();
    sim.time = 5.0;
    p.owe_traffic(0.25, None);
    p.owe_traffic(0.25, None);
    p.start(Some(&mut sim), None, None);
    assert!(p.busy());
    // (the stand-in meanwhile)
    assert_eq!(sim.time, 0.0);
    p.join(Some(&mut sim), None);
    assert!(!p.busy());
    // two steps of a quarter of a second owed, each made on its own: at most 4 fixed steps
    // of 0.02 s in each
    assert!((sim.time - 5.16).abs() < 1e-4, "{}", sim.time);
    // the stand-in is kept for the next frame
    assert!(p.stand_in.is_some());
}

/// Stepped on the sim thread frame after frame, the traffic comes out where the same frames
/// stepped inline leave it (the handover changes nothing of the computation).
#[test]
fn pipelined_frames_match_inline_frames() {
    let mut p = Pipeline::default();
    let mut piped = empty_traffic();
    let mut inline = empty_traffic();
    let mut x = 12345u32;
    for _ in 0..200 {
        x = x.wrapping_mul(1664525).wrapping_add(1013904223);
        let dt = 0.004 + (x >> 8) as f32 / (1u32 << 24) as f32 * 0.05;
        inline.advance(dt, None);
        p.owe_traffic(dt, None);
        p.start(Some(&mut piped), None, None);
        p.join(Some(&mut piped), None);
    }
    assert_eq!(piped.time.to_bits(), inline.time.to_bits());
    assert_eq!(piped.day_time.to_bits(), inline.day_time.to_bits());
    assert_eq!(piped.sim_accum.to_bits(), inline.sim_accum.to_bits());
    assert_eq!(piped.frame_alpha.to_bits(), inline.frame_alpha.to_bits());
    assert_eq!(piped.rng, inline.rng);
}

fn scenery_of(n: i64) -> SceneryState {
    use crate::scenery::{SceneryInstance, SceneryObject};
    let mut s = SceneryState::default();
    for i in 0..n {
        let inst = SceneryInstance::new(std::sync::Arc::new(omsi_script::Program::default()), &[], crate::SimClock::default(), &[]);
        let p = glam::DVec3::new((i * 37 % 1700) as f64 - 850.0, (i * 91 % 1300) as f64 - 650.0, 0.0);
        s.push(SceneryObject::new(1000 + i as u64, i, p, ((p.x / 300.0).floor() as i32, 0), inst, 0));
    }
    s
}

/// One frame's scenery step as the scripted stage makes it, its inputs owned.
fn scenery_step(dt: f32, center: glam::DVec3) -> impl FnOnce(&mut SceneryState, PhaseOf) + Send + 'static {
    move |s: &mut SceneryState, phase_of: PhaseOf| {
        let (controllers, by_stop, departures) = (hashbrown::HashMap::new(), hashbrown::HashMap::new(), Default::default());
        let inputs = crate::scenery::SceneryInputs {
            clock: None,
            time: 3600.0,
            day: Default::default(),
            brightness: 0.4,
            center,
            phase_of,
            controllers: &controllers,
            by_stop: &by_stop,
            departures: &departures,
            departures_gen: 0,
            pages_near: 60.0,
        };
        s.step(dt, &inputs, &mut crate::scenery::StepTimes::default());
    }
}

fn outputs_of(s: &mut SceneryState) -> String {
    let o = s.outputs();
    format!("{:?} {:?} {:?} {:?} {}", o.cmds, o.vars, o.transforms, o.visible, o.ran)
}

/// The scenery stepped on the sim thread (with the traffic, after it) gives the commands the
/// same frames' steps give inline, frame by frame.
#[test]
fn pipelined_scenery_matches_inline_scenery() {
    let mut p = Pipeline::default();
    let (mut piped, mut inline) = (scenery_of(400), scenery_of(400));
    let (mut piped_t, mut inline_t) = (empty_traffic(), empty_traffic());
    let unlinked = |_: usize, _: usize| (crate::traffic::UNLINKED_PHASE as f32, 0.0);
    for k in 0..120 {
        let dt = 1.0 / 60.0 + (k % 7) as f32 * 0.003;
        let center = glam::DVec3::new(-900.0 + k as f64 * 15.0, (k as f64 * 0.1).sin() * 300.0, 2.0);
        inline_t.advance(dt, None);
        scenery_step(dt, center)(&mut inline, &unlinked);
        p.owe_traffic(dt, None);
        p.owe_scenery(scenery_step(dt, center));
        p.start(Some(&mut piped_t), Some(&mut piped), None);
        assert!(piped.is_empty(), "an empty list stands in meanwhile");
        p.join(Some(&mut piped_t), Some(&mut piped));
        assert!(p.take_scenery_stepped());
        assert_eq!(outputs_of(&mut piped), outputs_of(&mut inline), "frame {k}");
    }
    assert_eq!(piped_t.time.to_bits(), inline_t.time.to_bits());
}

/// A bus with a passenger cabin (a front door, an aisle, six seats) and a person type, for
/// riders to sit in it.
fn bus_and_people() -> (crate::VehicleInstance, PeopleSim) {
    // (a folder of its own per test - its thread: two tests build this bus at once, and the one
    // that was done first took the other's files away; one test's two buses share it, the
    // people compare their bus by its files)
    let thread = format!("{:?}", std::thread::current().id()).replace(|c: char| !c.is_ascii_alphanumeric(), "");
    let dir = std::env::temp_dir().join(format!("omsi-sim-thread-people-{}-{thread}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let write = |name: &str, text: &str| std::fs::write(dir.join(name), text).unwrap();
    write("test.bus", "[model]\nmodel.cfg\n[passengercabin]\ncabin.cfg\n[paths]\npaths.cfg\n[boundingbox]\n2.5\n12\n3\n0\n0\n1.5\n[script]\n1\nmain.osc\n");
    write("model.cfg", "");
    write("main.osc", "{init}\n{end}\n");
    let pts: String = [5.0, 4.0, 0.0, -4.0].iter().map(|y| format!("[pathpnt]\n0\n{y}\n0.5\n\n")).collect();
    write("paths.cfg", &format!("[pathpnt]\n1.1\n5\n0.4\n\n{pts}[pathlink]\n0\n1\n\n[pathlink]\n1\n2\n\n[pathlink]\n2\n3\n\n[pathlink]\n3\n4\n"));
    let seats: String = (0..6).map(|k| format!("[passpos]\n-0.6\n{}\n0.9\n0.45\n0\n\n", 3.0 - k as f32 * 1.2)).collect();
    write("cabin.cfg", &format!("[entry]\n0\n\n[exit]\n0\n\n{seats}"));
    write("person.hum", "[model]\nmodel.cfg\n");
    let ty = std::sync::Arc::new(crate::VehicleType::load(&dir, &dir.join("test.bus")).unwrap());
    let human = std::sync::Arc::new(crate::human::HumanType::load(&dir.join("person.hum")).unwrap());
    let mut bus = crate::VehicleInstance::new(ty, crate::VehicleHost::new(Default::default()));
    let mut people = PeopleSim::new(std::path::Path::new("/nonexistent"), 50);
    people.types = vec![human];
    // (the cabin is read from its files here)
    people.set_cabin(&mut bus);
    std::fs::remove_dir_all(&dir).ok();
    (bus, people)
}

fn people_digest(p: &PeopleSim) -> String {
    let mut out = format!("t {} rng {} n {}", p.time.to_bits(), p.rng, p.people.len());
    for q in &p.people {
        out.push_str(&format!(" | {} {:?} {} {:?}", q.id, q.position.to_array().map(f64::to_bits), q.heading.to_bits(), q.place));
    }
    out
}

/// The people stepped on the sim thread (after the traffic, from a view of the player's bus
/// read before) come out where the same frames' steps leave them inline.
#[test]
fn pipelined_people_match_inline_people() {
    let (mut bus, mut inline) = bus_and_people();
    let (_, mut piped) = bus_and_people();
    let world = std::sync::Arc::new(crate::world_data::WorldData::default());
    inline.seed_riders(4, &bus, &*world);
    piped.seed_riders(4, &bus, &*world);
    assert!(!inline.people.is_empty(), "the riders sat down");
    let sat_at = inline.people[0].position;
    let (mut piped_t, mut inline_t) = (empty_traffic(), empty_traffic());
    let mut p = Pipeline::default();
    let mut piped = Some(piped);
    for k in 0..150 {
        let dt = 1.0 / 60.0 + (k % 5) as f32 * 0.004;
        // the bus drives off and turns
        bus.position += glam::DVec3::new((k as f64 * 0.02).sin(), 0.3, 0.0);
        bus.heading = k as f64 * 0.4;
        inline_t.advance(dt, None);
        let view = inline.player_bus_view(&bus);
        inline.tick_stages.clear();
        let took_inline = inline.tick_inner(dt, &*world, Some(&view), Some(&inline_t));
        p.owe_traffic(dt, None);
        let view = piped.as_ref().unwrap().player_bus_view(&bus);
        p.owe_people(dt, world.clone(), Some(view));
        assert!(p.start(Some(&mut piped_t), None, piped.take()).is_none(), "the people went along");
        let (_, back) = p.join(Some(&mut piped_t), None);
        piped = back;
        let stepped = p.take_people_stepped().expect("stepped");
        assert_eq!((stepped.took, stepped.dt), (took_inline, dt));
        assert_eq!(people_digest(piped.as_ref().unwrap()), people_digest(&inline), "frame {k}");
    }
    assert!((inline.people[0].position - sat_at).length() > 10.0, "the riders went along with the bus");
}

/// A frame's ticks of the fixed step (`tick::FixedStep` at 120 Hz) stepped on the sim thread -
/// each tick's traffic step, then that tick's people's step from the bus as that tick left it,
/// then the scenery once with the frame's time - come out where the same ticks leave them
/// inline, frame by frame, at frame rates that give 0, 1 or several ticks a frame.
#[test]
fn pipelined_ticks_match_inline_ticks() {
    let (mut bus, mut inline) = bus_and_people();
    let (_, piped) = bus_and_people();
    let world = std::sync::Arc::new(crate::world_data::WorldData::default());
    inline.seed_riders(4, &bus, &*world);
    let mut piped = Some(piped);
    piped.as_mut().unwrap().seed_riders(4, &bus, &*world);
    let (mut piped_t, mut inline_t) = (empty_traffic(), empty_traffic());
    let (mut piped_s, mut inline_s) = (scenery_of(200), scenery_of(200));
    let unlinked = |_: usize, _: usize| (crate::traffic::UNLINKED_PHASE as f32, 0.0);
    let mut p = Pipeline::default();
    let mut clock = crate::tick::FixedStep::new(crate::tick::RATE_HZ);
    let mut tick = 0u32;
    let mut counts = std::collections::BTreeSet::new();
    for k in 0..240 {
        // 144, 60, 30 and uneven frames a second
        let frame_dt = [1.0 / 144.0, 1.0 / 60.0, 1.0 / 30.0, 0.0113][k / 60] + (k % 3) as f32 * 0.0007;
        let ticks = clock.advance(frame_dt);
        counts.insert(ticks.n);
        let mut took_inline = false;
        for _ in 0..ticks.n {
            // the bus's own step of the tick (on the main thread either way)
            tick += 1;
            bus.position += glam::DVec3::new((tick as f64 * 0.01).sin() * 0.2, 0.08, 0.0);
            bus.heading = tick as f64 * 0.1;
            let dt = clock.dt();
            inline_t.advance(dt, None);
            let view = inline.player_bus_view(&bus);
            inline.tick_stages.clear();
            took_inline |= inline.tick_inner(dt, &*world, Some(&view), Some(&inline_t));
            p.owe_traffic(dt, None);
            let view = piped.as_ref().unwrap().player_bus_view(&bus);
            p.owe_people(dt, world.clone(), Some(view));
        }
        let center = glam::DVec3::new(-900.0 + k as f64 * 7.0, (k as f64 * 0.1).sin() * 300.0, 2.0);
        scenery_step(frame_dt, center)(&mut inline_s, &unlinked);
        if ticks.n == 0 {
            // (no tick: nothing goes out, the scripts run in the frame - `takes_scenery`)
            scenery_step(frame_dt, center)(&mut piped_s, &unlinked);
            assert!(!p.owes_people());
            piped = p.start(Some(&mut piped_t), Some(&mut piped_s), piped.take());
            assert!(piped.is_some() && !p.busy(), "nothing owed, nothing goes out");
            continue;
        }
        p.owe_scenery(scenery_step(frame_dt, center));
        assert!(p.start(Some(&mut piped_t), Some(&mut piped_s), piped.take()).is_none(), "the people went along");
        let (_, back) = p.join(Some(&mut piped_t), Some(&mut piped_s));
        piped = back;
        assert!(p.take_scenery_stepped());
        let stepped = p.take_people_stepped().expect("stepped");
        assert_eq!(stepped.took, took_inline, "frame {k}");
        assert!((stepped.dt - ticks.n as f32 * clock.dt()).abs() < 1e-6, "frame {k}: {} for {} ticks", stepped.dt, ticks.n);
        assert_eq!(people_digest(piped.as_ref().unwrap()), people_digest(&inline), "frame {k}");
        assert_eq!(piped_t.time.to_bits(), inline_t.time.to_bits(), "frame {k}");
        assert_eq!(piped_t.rng, inline_t.rng, "frame {k}");
        assert_eq!(outputs_of(&mut piped_s), outputs_of(&mut inline_s), "frame {k}");
    }
    assert!(counts.contains(&0) && counts.contains(&1) && counts.iter().any(|n| *n >= 2), "ticks a frame: {counts:?}");
    assert_eq!(piped_t.sim_accum.to_bits(), inline_t.sim_accum.to_bits());
    assert_eq!(piped_t.day_time.to_bits(), inline_t.day_time.to_bits());
    assert!((inline.people[0].position - glam::DVec3::ZERO).length() > 5.0, "the riders went along with the bus");
}

/// The setting decides, `OMSI_SIM_THREAD` over it both ways (it used to turn the thread on
/// whatever its value, `=0` too); nobody deciding is inline (the offscreen run, the tests).
#[test]
fn the_switch_and_the_setting() {
    assert!(choice(None, true));
    assert!(!choice(None, false));
    for off in ["0", "false", "OFF", " no "] {
        assert!(!choice(Some(off), true), "{off}");
    }
    for on in ["1", "true", "yes", ""] {
        assert!(choice(Some(on), false), "{on:?}");
    }
}

fn drawn_gaps(frames: &[f32]) -> Vec<(f32, f32)> {
    let mut clock = crate::tick::FixedStep::new(crate::tick::RATE_HZ);
    let tick = clock.dt();
    let mut lag = DrawnLag::default();
    let mut out = Vec::new();
    for &dt in frames {
        let ticks = clock.advance(dt);
        let owed = ticks.n as f32 * tick;
        let behind = (1.0 - ticks.alpha) * tick;
        let b = lag.behind(behind, owed);
        lag.note(owed, dt);
        out.push((owed + b - behind, b));
    }
    out
}

#[test]
fn pipelined_traffic_is_drawn_a_steady_time_behind_at_any_frame_rate() {
    for fps in [25.0f32, 40.0, 75.0, 100.0, 144.0, 160.0, 240.0, 500.0] {
        let gaps = drawn_gaps(&vec![1.0 / fps; fps as usize * 3]);
        let settled = &gaps[fps as usize..];
        for &(gap, b) in settled {
            assert!(b <= crate::ai_traffic::fixed_step::MOST_BEHIND, "{fps} fps: {b}");
            assert!((gap - settled[0].0).abs() < 1e-4, "{fps} fps: {gap} after {}", settled[0].0);
        }
    }
}

#[test]
fn after_a_hitch_the_traffic_eases_back_without_a_jump() {
    for (fps, hitch) in [(144.0f32, 0.03f32), (144.0, 0.1), (60.0, 0.1), (160.0, 0.07), (75.0, 0.5)] {
        let n = fps as usize * 4;
        let at = fps as usize * 2;
        let mut frames = vec![1.0 / fps; n];
        frames[at] = hitch;
        let gaps = drawn_gaps(&frames);
        for k in fps as usize + 1..n {
            let (gap, b) = gaps[k];
            assert!(b <= crate::ai_traffic::fixed_step::MOST_BEHIND + 1e-6, "{fps} fps, hitch {hitch}: {b} at frame {k}");
            let step = gap - gaps[k - 1].0;
            assert!(k == at || step <= 1e-5, "{fps} fps, hitch {hitch}: the cars fell back {step} at frame {k}");
            assert!(step >= -DrawnLag::EASE * frames[k] - 1e-5, "{fps} fps, hitch {hitch}: the cars jumped {step} at frame {k}");
        }
    }
}

#[test]
fn nothing_owed_draws_where_the_ticks_say() {
    let mut lag = DrawnLag::default();
    for _ in 0..100 {
        lag.note(0.0, 0.01);
    }
    assert_eq!(lag.behind(0.004, 0.0), 0.004);
}

#[test]
fn ai_handover_keeps_occupied_seats_after_cabin_rebind() {
    let (mut bus, mut people) = bus_and_people();
    let occupied = vec![true, false, true, false, false, true];
    people.seats.remove(&crate::people::BusId::Player);
    people.seats.insert(crate::people::BusId::Ai(77), occupied.clone());
    people.remap_bus(crate::people::BusId::Ai(77), crate::people::BusId::Player);
    people.served_stop = Some(42);
    people.rebind_player_cabin(&mut bus);
    assert_eq!(people.seats[&crate::people::BusId::Player], occupied);
    assert_eq!(people.served_stop, None);
    assert!(people.player_cabin.is_some());
}
