use super::*;
use omsi_vehicle::VolCurve;

/// The tests that set the player's bus's `Snd_OutsideVol` or the walker's way in (one
/// value for the whole program) take turns.
static GLOBALS: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub(super) fn globals() -> std::sync::MutexGuard<'static, ()> {
    GLOBALS.lock().unwrap_or_else(|e| e.into_inner())
}

fn ctx(view: i32, cab: bool) -> Ctx {
    Ctx {
        view,
        amount: if view == 2 { 1.0 } else { 0.0 },
        cab,
        master: 1.0,
        doppler: false,
        listener: Vec3::ZERO,
        now: std::time::Instant::now(),
    }
}

fn eval(c: &Ctx, def: SoundEntry, var: &dyn Fn(&str) -> Option<f32>) -> Eval {
    let mut s = RuntimeSound::new(def, None);
    c.eval(&mut s, var, &Mat4::IDENTITY)
}

#[test]
fn scenery_is_heard_through_the_bodywork_from_a_cab() {
    let _turn = globals();
    let none = |_: &str| None;
    set_outside_open(Some(0.5));
    let bells = SoundEntry {
        volume: 0.8,
        ..Default::default()
    };
    assert!((eval(&ctx(5, true), bells.clone(), &none).gain - 0.56).abs() < 1e-3);
    assert!(
        (eval(&ctx(5, false), bells, &none).gain - 0.8).abs() < 1e-3,
        "from the street: as it is"
    );
    assert!(
        !eval(
            &ctx(5, false),
            SoundEntry {
                volume: 1.0,
                viewpoint: 1,
                ..Default::default()
            },
            &none
        )
        .wrong_view
    );
    assert!(
        eval(
            &ctx(5, true),
            SoundEntry {
                volume: 1.0,
                viewpoint: 2,
                ..Default::default()
            },
            &none
        )
        .wrong_view
    );
    set_outside_open(None);
}

#[test]
fn a_scenery_set_is_heard_as_scenery_through_a_cab() {
    let engine = AudioEngine::silent();
    let mut set = SoundSet::new_scenery(
        &engine,
        &omsi_vehicle::sound::SoundCfg::default(),
        Path::new("/nonexistent"),
    );
    set.set_muffled(true);
    let c = set.ctx(&engine);
    assert_eq!((c.view, c.cab), (5, true));
    let mut ai = SoundSet::new_exterior(
        &engine,
        &omsi_vehicle::sound::SoundCfg::default(),
        Path::new("/nonexistent"),
    );
    ai.set_muffled(true);
    assert_eq!(ai.ctx(&engine).view, 4, "an AI vehicle stays view 4");
}

#[test]
fn curves_are_linear_and_held_at_the_ends() {
    let p = [(0.0, 0.0), (1.0, 1.0), (1.0, 0.5), (3.0, 0.0)];
    assert_eq!(curve(&p, -1.0), 0.0);
    assert_eq!(curve(&p, 0.5), 0.5);
    // at x = 1 the segment that starts there: (1, 0.5) .. (3, 0)
    assert_eq!(curve(&p, 1.0), 0.5);
    assert_eq!(curve(&p, 2.0), 0.25);
    assert_eq!(curve(&p, 9.0), 0.0);
    assert_eq!(curve(&[], 9.0), 1.0);
}

#[test]
fn direct_sound_refuses_over_0_db() {
    assert_eq!(direct_sound_volume(1.0, 0.3), (1.0, true));
    // 44100 "as a volume": the buffer keeps what it had
    assert_eq!(direct_sound_volume(44100.0, 0.3), (0.3, true));
    assert_eq!(direct_sound_volume(1.3, 1.0), (1.0, true));
    let (g, _) = buffer_volume(2.0, 1.0, 0.3);
    assert!((g - 2.0).abs() < 0.01, "{g}");
    let (g, _) = buffer_volume(2.5, 2.0, 0.3);
    assert!((g - 2.5).abs() < 0.01, "{g}");
    assert_eq!(buffer_volume(2.0, 1.0, 1.0), (1.0, true));
    let (g, a) = direct_sound_volume(0.5, 1.0);
    assert!((g - 0.5).abs() < 1e-3 && a);
    assert_eq!(direct_sound_volume(0.0, 0.7), (0.0, false));
    // under -100 dB: refused and not started
    assert_eq!(direct_sound_volume(1.0e-6, 0.7), (0.7, false));
}

#[test]
fn the_cab_hears_outside_through_what_is_open() {
    let _turn = globals();
    let none = |_: &str| None;
    // the own bus's outside-only entry (`[viewpoint] 5`) in the cab: at Snd_OutsideVol,
    // not at all with everything shut
    let engine_out = SoundEntry {
        volume: 0.8,
        viewpoint: 5,
        ..Default::default()
    };
    set_outside_open(Some(0.0));
    assert!(eval(&ctx(2, true), engine_out.clone(), &none).wrong_view);
    set_outside_open(Some(0.5));
    let e = eval(&ctx(2, true), engine_out.clone(), &none);
    assert!((e.gain - 0.4).abs() < 1e-3, "{}", e.gain);
    assert!(
        (eval(&ctx(1, false), engine_out.clone(), &none).gain - 0.8).abs() < 1e-3,
        "outside: as it is"
    );
    // an AI bus (view 4) heard from a cab: 0.2 + Snd_OutsideVol, no filter
    let ai = SoundEntry {
        volume: 0.5,
        viewpoint: 4,
        ..Default::default()
    };
    assert!((eval(&ctx(4, true), ai.clone(), &none).gain - 0.35).abs() < 1e-3);
    assert!(
        (eval(&ctx(4, false), ai, &none).gain - 0.5).abs() < 1e-3,
        "from the street: as it is"
    );
    // a scenery object heard from a cab: as an AI vehicle, its street sounds too
    let bells = SoundEntry {
        volume: 0.5,
        viewpoint: 1,
        ..Default::default()
    };
    assert!((eval(&ctx(5, true), bells.clone(), &none).gain - 0.35).abs() < 1e-3);
    assert!((eval(&ctx(5, false), bells, &none).gain - 0.5).abs() < 1e-3);
    // the player's bus heard from the street on an AI bus: never
    assert!(
        eval(
            &ctx(4, false),
            SoundEntry {
                volume: 1.0,
                viewpoint: 1,
                ..Default::default()
            },
            &none
        )
        .wrong_view
    );
    let cab = SoundEntry {
        volume: 0.8,
        viewpoint: 2,
        ..Default::default()
    };
    assert!(
        eval(&ctx(1, false), cab, &none).wrong_view,
        "a cab sound stays in"
    );
    set_outside_open(None);
}

#[test]
fn outside_sounds_in_the_cab_are_low_passed_by_how_open_it_is() {
    let _turn = globals();
    assert!((cab_lowpass_hz(0.0) - 500.0).abs() < 1e-3);
    assert_eq!(cab_lowpass_hz(0.95), 0.0, "open from 0.95 on");
    assert_eq!(cab_lowpass_hz(1.0), 0.0);
    let mut last = 0.0;
    for k in 0..95 {
        let f = cab_lowpass_hz(k as f32 / 100.0);
        assert!(f > last, "rising");
        last = f;
    }
    let engine_out = SoundEntry {
        volume: 0.8,
        viewpoint: 5,
        ..Default::default()
    };
    let s = RuntimeSound::new(engine_out.clone(), None);
    let e = Eval {
        gain: 1.0,
        audible: true,
        pitch: 1.0,
        wrong_view: false,
    };
    set_outside_open(Some(0.3));
    let in_cab = ctx(2, true).params(&s, &e, true, &Mat4::IDENTITY);
    assert!((in_cab.lowpass_hz - cab_lowpass_hz(0.3)).abs() < 1e-3);
    // the street hears it as it is, and the cab sounds and the traffic are never filtered
    assert_eq!(
        ctx(1, false)
            .params(&s, &e, true, &Mat4::IDENTITY)
            .lowpass_hz,
        0.0
    );
    let cab_only = RuntimeSound::new(
        SoundEntry {
            viewpoint: 2,
            ..engine_out.clone()
        },
        None,
    );
    assert_eq!(
        ctx(2, true)
            .params(&cab_only, &e, true, &Mat4::IDENTITY)
            .lowpass_hz,
        0.0
    );
    assert_eq!(
        ctx(4, true)
            .params(&s, &e, true, &Mat4::IDENTITY)
            .lowpass_hz,
        0.0
    );
    set_outside_open(Some(0.96));
    assert_eq!(
        ctx(2, true)
            .params(&s, &e, true, &Mat4::IDENTITY)
            .lowpass_hz,
        0.0,
        "doors open: as before"
    );
    set_outside_open(None);
}

#[test]
fn stepping_in_crosses_from_the_street_to_the_cab() {
    let _turn = globals();
    let none = |_: &str| None;
    set_outside_open(Some(0.2));
    let half = |w: f32| Ctx {
        amount: w,
        ..ctx(if w >= 0.5 { 2 } else { 1 }, w >= 0.5)
    };
    let engine_out = SoundEntry {
        volume: 1.0,
        viewpoint: 1,
        ..Default::default()
    };
    let rain_in = SoundEntry {
        volume: 1.0,
        viewpoint: 2,
        ..Default::default()
    };
    let both = SoundEntry {
        volume: 1.0,
        viewpoint: 0,
        ..Default::default()
    };
    // all out and all in: as the two views always were
    assert_eq!(eval(&half(0.0), engine_out.clone(), &none).gain, 1.0);
    assert!(eval(&half(0.0), rain_in.clone(), &none).wrong_view);
    assert!((eval(&half(1.0), engine_out.clone(), &none).gain - 0.2).abs() < 1e-3);
    // a quarter in: three quarters of the street, a quarter of the cab
    let w = 0.25;
    assert!((eval(&half(w), engine_out.clone(), &none).gain - (0.75 + 0.25 * 0.2)).abs() < 2e-3);
    assert!((eval(&half(w), rain_in.clone(), &none).gain - 0.25).abs() < 2e-3);
    assert_eq!(eval(&half(w), both, &none).gain, 1.0);
    // the outside sound grows duller the further in
    let s = RuntimeSound::new(engine_out, None);
    let e = Eval {
        gain: 1.0,
        audible: true,
        pitch: 1.0,
        wrong_view: false,
    };
    let f = |w: f32| half(w).params(&s, &e, true, &Mat4::IDENTITY).lowpass_hz;
    assert_eq!(f(0.0), 0.0);
    assert!(
        f(0.25) > f(0.75) && f(0.75) > f(1.0) && f(1.0) > 0.0,
        "{} {} {}",
        f(0.25),
        f(0.75),
        f(1.0)
    );
    // the traffic slides from its street level to its cab level
    let ai = SoundEntry {
        volume: 1.0,
        viewpoint: 4,
        ..Default::default()
    };
    set_cab_amount(Some(0.5));
    assert!((eval(&ctx(4, true), ai.clone(), &none).gain - (1.0 + (0.4 - 1.0) * 0.5)).abs() < 2e-3);
    set_cab_amount(None);
    assert!((eval(&ctx(4, true), ai, &none).gain - 0.4).abs() < 2e-3);
    set_outside_open(None);
}

#[test]
fn volume_curves_multiply_and_a_3d_sound_fades_with_distance() {
    let def = SoundEntry {
        volume: 1.0,
        pos: Some([0.0, 8.0, 0.0]),
        range: 2.0,
        vol_curves: vec![
            VolCurve {
                variable: "a".into(),
                points: vec![(0.0, 0.0), (1.0, 1.0)],
            },
            VolCurve {
                variable: "b".into(),
                points: vec![(0.0, 1.0), (1.0, 0.0)],
            },
        ],
        ..Default::default()
    };
    let var = |n: &str| Some(if n == "a" { 0.5 } else { 0.5 });
    let e = eval(&ctx(1, false), def, &var);
    // 0.5 * 0.5 * (2 / 8), the distance left to the mixer: 0.25 given to the voice
    assert!((e.gain - 0.25).abs() < 1e-3, "{}", e.gain);
    assert!(e.audible);
}

#[test]
fn a_loop_sound_is_not_started_below_100_hz_and_keeps_its_rate_past_200_khz() {
    let def = SoundEntry {
        volume: 1.0,
        is_loop: true,
        sample_rate: 44100.0,
        pitch_variable: "n".into(),
        pitch_ref: 1000.0,
        ..Default::default()
    };
    let mut s = RuntimeSound::new(
        def,
        Some(Arc::new(Clip {
            sample_rate: 44100,
            channels: 1,
            samples: vec![0; 4],
        })),
    );
    let c = ctx(1, false);
    let e = c.eval(&mut s, &|_| Some(1.0), &Mat4::IDENTITY);
    assert!(!e.audible, "44 Hz");
    let e = c.eval(&mut s, &|_| Some(2000.0), &Mat4::IDENTITY);
    assert!(e.audible && (e.pitch - 2.0).abs() < 1e-4);
    // 6 x 44100 = 264 600 Hz: refused, the rate stays at 2x
    let e = c.eval(&mut s, &|_| Some(6000.0), &Mat4::IDENTITY);
    assert!(e.audible && (e.pitch - 2.0).abs() < 1e-4, "{}", e.pitch);
}

/// The SD200's door hit: `[volcurve] doorSpeed_0` from 1 at -1 down to 0 at -0.5,
/// fired while the door still closes at -1 m/s; by the frame's end the script has turned
/// the speed round. The volume is the one of the moment it fired (#676).
#[test]
fn a_frame_reads_each_variable_once() {
    let e = AudioEngine::capture(48_000);
    let clip = Arc::new(Clip {
        sample_rate: 48_000,
        channels: 1,
        samples: vec![1000; 4800],
    });
    let entry = SoundEntry {
        volume: 1.0,
        conditions: vec![omsi_vehicle::sound::Condition {
            variable: "engine_on".into(),
            value: 1.0,
            relation: 1,
            kind: Default::default(),
        }],
        vol_curves: vec![
            VolCurve {
                variable: "n".into(),
                points: vec![(0.0, 0.0), (1.0, 1.0)],
            },
            VolCurve {
                variable: "-1".into(),
                points: vec![(0.0, 1.0), (1.0, 1.0)],
            },
        ],
        ..Default::default()
    };
    let sounds = (0..3)
        .map(|_| RuntimeSound::new(entry.clone(), Some(clip.clone())))
        .collect();
    let mut set = SoundSet {
        sounds,
        master: 1.0,
        dir: Default::default(),
        inside: false,
        amount: 0.0,
        ai: false,
        scenery: false,
        listener_vehicle: false,
        muffled: false,
        parts: Vec::new(),
        vars: None,
    };
    let reads = std::cell::Cell::new(0);
    let var = |n: &str| {
        reads.set(reads.get() + 1);
        Some(if n == "n" { 0.5 } else { 1.0 })
    };
    set.update(&e, &var, &Mat4::IDENTITY, &[]);
    assert_eq!(reads.get(), 2, "engine_on and n, once each");
    assert_eq!(
        set.sounds
            .iter()
            .filter(|s| s.voice.is_some_and(|id| e.is_playing(id)))
            .count(),
        3
    );
    let gains: Vec<f32> = set
        .sounds
        .iter()
        .filter_map(|s| e.voice_state(s.voice?))
        .map(|v| v.1)
        .collect();
    assert!(
        gains.len() == 3 && gains.iter().all(|g| (g - 0.5).abs() < 0.01),
        "the curve read through the table: {gains:?}"
    );
}

#[test]
fn a_triggered_sound_reads_its_curve_when_it_fires() {
    let hit = SoundEntry {
        volume: 1.0,
        triggers: vec!["ev_doorhitclose_0".into()],
        vol_curves: vec![VolCurve {
            variable: "doorSpeed_0".into(),
            points: vec![(-1.0, 1.0), (-0.5, 0.0)],
        }],
        ..Default::default()
    };
    let now = |n: &str| (n == "doorSpeed_0").then_some(0.8);
    let at_fire =
        |t: &str, n: &str| (t == "ev_doorhitclose_0" && n == "doorSpeed_0").then_some(-1.0);
    assert_eq!(
        eval(&ctx(2, true), hit.clone(), &now).gain,
        0.0,
        "read at the frame's end: silent"
    );
    let fired = |n: &str| at_fire("ev_doorhitclose_0", n).or_else(|| now(n));
    assert_eq!(eval(&ctx(2, true), hit.clone(), &fired).gain, 1.0);
    let set = SoundSet {
        sounds: vec![RuntimeSound::new(hit.clone(), None)],
        master: 1.0,
        dir: Default::default(),
        inside: true,
        amount: 1.0,
        ai: false,
        scenery: false,
        listener_vehicle: true,
        muffled: false,
        parts: Vec::new(),
        vars: None,
    };
    assert_eq!(set.curve_triggers(), vec!["ev_doorhitclose_0".to_string()]);
}

/// The door hit keeps the volume it fired with to its end, though `doorSpeed_0` is 0
/// from the next frame on (#611, #1268); fired again, it reads the curves anew.
#[test]
fn articulated_parts_use_trigger_time_curves_and_keep_them() {
    let _g = globals();
    let engine = AudioEngine::capture(48_000);
    let clip = Arc::new(Clip {
        sample_rate: 48_000,
        channels: 1,
        samples: vec![1000; 4800],
    });
    let entry = SoundEntry {
        volume: 1.0,
        triggers: vec!["door_hit".into()],
        vol_curves: vec![VolCurve {
            variable: "door_speed".into(),
            points: vec![(-1.0, 1.0), (0.0, 0.0)],
        }],
        ..Default::default()
    };
    let make = || SoundSet {
        sounds: vec![RuntimeSound::new(entry.clone(), Some(clip.clone()))],
        master: 1.0,
        dir: Default::default(),
        inside: false,
        amount: 0.0,
        ai: false,
        scenery: false,
        listener_vehicle: false,
        muffled: false,
        parts: Vec::new(),
        vars: None,
    };
    let mut set = make();
    set.add_part(0, make());
    set.update_parts(
        &engine,
        &|_| Some(0.0),
        &|_| Some(Mat4::IDENTITY),
        &["door_hit".into()],
        &|t, n| (t == "door_hit" && n == "door_speed").then_some(-1.0),
    );
    assert_eq!(set.parts[0].1.sounds[0].fired_curves, Some(1.0));
    set.update_parts(
        &engine,
        &|_| Some(0.0),
        &|_| Some(Mat4::IDENTITY),
        &[],
        &|_, _| None,
    );
    assert_eq!(set.parts[0].1.sounds[0].fired_curves, Some(1.0));
}

#[test]
fn a_triggered_sound_keeps_the_volume_it_started_with() {
    let _g = globals();
    let hit = SoundEntry {
        volume: 1.0,
        triggers: vec!["ev_doorhitclose_0".into()],
        vol_curves: vec![VolCurve {
            variable: "doorSpeed_0".into(),
            points: vec![(-1.0, 1.0), (-0.5, 0.0)],
        }],
        ..Default::default()
    };
    let c = ctx(2, true);
    let mut s = RuntimeSound::new(hit, None);
    let fast = |n: &str| (n == "doorSpeed_0").then_some(-1.0);
    let stopped = |n: &str| (n == "doorSpeed_0").then_some(0.0);
    let slow = |n: &str| (n == "doorSpeed_0").then_some(-0.75);
    assert_eq!(
        c.eval_holding(&mut s, &fast, &Mat4::IDENTITY, true).gain,
        1.0,
        "as it fires"
    );
    assert_eq!(
        c.eval_holding(&mut s, &stopped, &Mat4::IDENTITY, true).gain,
        1.0,
        "the leaf has stopped: still playing out"
    );
    s.fired_curves = None;
    assert!(
        (c.eval_holding(&mut s, &slow, &Mat4::IDENTITY, true).gain - 0.5).abs() < 1e-4,
        "fired again, slower"
    );
}

/// The stock IBIS announces Spandau's "Falkenseer Ch/Stadtrandstr" with the file
/// `Falkenseer Ch%Stadtrandstr.wav`, as Omsi.exe's file triggers write `/` (#1096).
#[test]
fn a_file_trigger_writes_what_a_file_name_cannot_have_as_percent() {
    let file = r"..\..\Announcements\Spandau\Falkenseer Ch/Stadtrandstr.wav";
    assert_eq!(
        file_trigger_name(file),
        r"..\..\Announcements\Spandau\Falkenseer Ch%Stadtrandstr.wav"
    );
    assert_eq!(
        file_trigger_name(r"C:\a?b*c\d<e>f|g.wav"),
        r"C:\a%b%c\d%e%f%g.wav"
    );
    // found where the announcement lies
    let root = std::env::temp_dir().join(format!("openomsi-file-trigger-{}", std::process::id()));
    let sound = root.join("Vehicles").join("MAN_NL_NG").join("Sound");
    let spandau = root.join("Vehicles").join("Announcements").join("Spandau");
    std::fs::create_dir_all(&sound).unwrap();
    std::fs::create_dir_all(&spandau).unwrap();
    std::fs::write(spandau.join("Falkenseer Ch%Stadtrandstr.wav"), b"RIFF").unwrap();
    let path = omsi_cfg::resolve_path(&sound, &file_trigger_name(file));
    assert!(path.is_file(), "{}", path.display());
    // (the name as the script wrote it: a folder "Falkenseer Ch" that is not there)
    assert!(!omsi_cfg::resolve_path(&sound, file).is_file());
    let _ = std::fs::remove_dir_all(&root);
}
