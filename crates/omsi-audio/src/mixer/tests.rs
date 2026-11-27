use super::render::apply_params;
use super::*;

/// A mixer at 48 kHz, mono, and the game's ends of its rings.
fn shared() -> (
    Mixer,
    crate::spsc::Producer<Cmd>,
    crate::spsc::Consumer<Back>,
) {
    let s = Arc::new(Shared::new(48_000, 1, false, false));
    let (cmd_tx, cmd_rx) = crate::spsc::channel(64);
    let (back_tx, back_rx) = crate::spsc::channel(64);
    (Mixer::new(s, cmd_rx, back_tx, FIRST_ROOM), cmd_tx, back_rx)
}

fn shared_with(
    ch: usize,
) -> (
    Mixer,
    crate::spsc::Producer<Cmd>,
    crate::spsc::Consumer<Back>,
) {
    let s = Arc::new(Shared::new(48_000, ch, false, false));
    let (cmd_tx, cmd_rx) = crate::spsc::channel(64);
    let (back_tx, back_rx) = crate::spsc::channel(64);
    (Mixer::new(s, cmd_rx, back_tx, FIRST_ROOM), cmd_tx, back_rx)
}

/// A steady sound `deg` to the right of the view, 5 m away, mixed on `ch` channels with the
/// surround option on or off: how much of the sound each of the six speakers of a 5.1
/// system gets (the share of the whole energy, 0..1).
fn surround_energy(deg: f32, ch: usize, surround: bool) -> [f32; 6] {
    let (mut s, _cmd, _back) = shared_with(ch);
    s.shared.listener.store(&Listener {
        position: Vec3::ZERO,
        forward: Vec3::Y,
        right: Vec3::X,
        surround,
        ..Default::default()
    });
    let r = deg.to_radians();
    let clip = Arc::new(Clip {
        sample_rate: 48_000,
        channels: 1,
        samples: vec![16_384; 4_800],
    });
    let mut v = voice(clip, 1.0);
    v.params.position = Some(Vec3::new(r.sin() * 5.0, r.cos() * 5.0, 0.0));
    s.voices.push(v);
    let frames = 4_000;
    let mut out = vec![0.0f32; frames * ch];
    s.render(&mut out);
    let mut e = [0.0f32; 6];
    for i in 0..frames {
        for (c, x) in e.iter_mut().enumerate().take(ch.min(6)) {
            let y = out[i * ch + c];
            *x += y * y;
        }
    }
    let total: f32 = e.iter().sum::<f32>().max(1e-9);
    e.map(|x| x / total)
}

/// #2037: on a 5.1 device a sound is heard from the speakers it faces - the front right
/// door from the front right speaker, the rear one from the back right - and on two
/// channels, or with the option off, exactly as before.
#[test]
fn a_51_device_places_a_sound_where_it_is() {
    // (a source to the right on two speakers: mostly on the right one - `[sound_stereo]`
    // narrows the picture, as in OMSI 2, so the left is not quite silent)
    let two = surround_energy(30.0, 2, false);
    assert!(two[1] > two[0] && two[1] > 0.5 && two[0] > 0.0, "{two:?}");
    assert!(two[2..].iter().all(|x| *x == 0.0));
    // (the option off on a six-channel device: the same two)
    let off = surround_energy(30.0, 6, false);
    assert_eq!(off[0], two[0], "{off:?}");
    assert!(off[2..].iter().all(|x| *x == 0.0));
    // (on: the front right speaker takes it, the centre goes along a little)
    let front_right = surround_energy(30.0, 6, true);
    assert!(
        front_right[1] > 0.7 && front_right[2] > 0.05 && front_right[2] < front_right[1],
        "{front_right:?}"
    );
    assert!(
        front_right[0] < 0.01 && front_right[4] < 0.01 && front_right[5] < 0.01,
        "{front_right:?}"
    );
    assert_eq!(front_right[3], 0.0, "the sub-woofer gets nothing");
    // (a rear door behind on the right: the back right speaker)
    let back_right = surround_energy(110.0, 6, true);
    assert!(back_right[5] > 0.99, "{back_right:?}");
    assert!(
        back_right[1] < 0.01 && back_right[2] < 0.01,
        "{back_right:?}"
    );
    // (the engine ahead: the centre, the fronts along with it)
    let ahead = surround_energy(0.0, 6, true);
    assert!(
        ahead[2] > 0.5 && ahead[2] > ahead[0] && ahead[2] > ahead[1],
        "{ahead:?}"
    );
    assert!(ahead[4] < 0.01 && ahead[5] < 0.01, "{ahead:?}");
    // (the option on but a two-channel device: still two, as before)
    let two_on = surround_energy(30.0, 2, true);
    assert_eq!(two_on[0], two[0], "{two_on:?}");
}

fn voice(clip: Arc<Clip>, gain: f32) -> Voice {
    let mut v = Voice::new(
        1,
        clip,
        None,
        VoiceParams {
            gain,
            pitch: 1.0,
            looping: true,
            position: None,
            doppler: true,
            range: 10.0,
            lowpass_hz: 0.0,
            important: false,
        },
        0.0,
    );
    v.cur_gain = gain;
    v
}

#[test]
fn a_loop_has_no_gap_where_it_turns() {
    // a 5-frame loop of a constant level at the device rate: every output frame carries it
    let clip = Arc::new(Clip {
        sample_rate: 48_000,
        channels: 1,
        samples: vec![16_384; 5],
    });
    let (mut s, _, _) = shared();
    s.voices.push(voice(clip, 1.0));
    let mut out = vec![0.0f32; 64];
    s.render(&mut out);
    assert!(out.iter().all(|x| (*x - 0.5).abs() < 1e-3), "{out:?}");
}

#[test]
fn parameters_arrive_with_the_next_block() {
    let clip = Arc::new(Clip {
        sample_rate: 48_000,
        channels: 1,
        samples: vec![16_384; 5],
    });
    let (mut s, mut cmds, _) = shared();
    s.voices.push(voice(clip, 1.0));
    assert!(
        cmds.push(Cmd::Params(
            1,
            VoiceParams {
                gain: 0.0,
                looping: true,
                ..Default::default()
            },
            std::time::Instant::now(),
            7
        ))
        .is_ok()
    );
    let mut out = vec![0.0f32; 4];
    s.render(&mut out);
    assert_eq!(s.voices[0].params.gain, 0.0);
    assert!(cmds.is_empty());
    assert_eq!(s.shared.taken.load(Ordering::Relaxed), 7);
}

#[test]
fn listener_vehicle_keeps_spatial_sound_at_its_original_pitch() {
    let clip = Arc::new(Clip {
        sample_rate: 48_000,
        channels: 1,
        samples: vec![0; 5],
    });
    let mut own = voice(clip.clone(), 1.0);
    let mut passing = voice(clip, 1.0);
    let now = std::time::Instant::now();
    for (distance, elapsed) in [(2.0, 0), (2.2, 20)] {
        let at = now + std::time::Duration::from_millis(elapsed);
        let position = Some(Vec3::new(distance, 0.0, 0.0));
        apply_params(
            &mut own,
            VoiceParams {
                position,
                doppler: false,
                ..Default::default()
            },
            at,
            Vec3::ZERO,
            true,
        );
        apply_params(
            &mut passing,
            VoiceParams {
                position,
                ..Default::default()
            },
            at,
            Vec3::ZERO,
            true,
        );
    }
    assert_eq!(own.doppler.2, 1.0);
    assert!(passing.doppler.2 < 1.0);
}

/// A recording's clock is the sound it mixed, not the wall clock: a fade-in by seconds
/// played comes out the same however slowly the run goes.
#[test]
fn a_recording_goes_by_the_sound_it_mixed() {
    let e = AudioEngine::capture(48_000);
    let t0 = e.clock();
    std::thread::sleep(std::time::Duration::from_millis(20));
    assert_eq!(e.clock(), t0);
    e.render_capture(24_000);
    assert_eq!(e.clock() - t0, std::time::Duration::from_millis(500));
}

/// The stream opened again (as when the system's output device changed) keeps playing;
/// on a machine without an output device there is nothing to follow.
#[test]
fn follows_the_output_device() {
    let e = AudioEngine::new();
    if !e.enabled {
        return;
    }
    let before = e.device.lock().name.clone();
    e.reopen.store(true, Ordering::Relaxed);
    e.device.lock().opened = std::time::Instant::now() - std::time::Duration::from_secs(2);
    e.follow_device();
    assert!(e.device.lock().playing);
    assert_eq!(e.device.lock().name, before);
    assert!(!e.reopen.load(Ordering::Relaxed));
}

/// A voice at `d` metres in front of the listener, heard at full gain whatever `d` is.
fn far_voice(clip: Arc<Clip>, d: f32) -> Voice {
    let mut v = voice(clip, 1.0);
    v.params.position = Some(Vec3::new(0.0, 0.0, -d));
    v.params.range = 1.0e6;
    v.params.doppler = false;
    v
}

/// A capture engine mixes on demand, with its own clock.
#[test]
fn a_capture_mixes_voices_on_demand() {
    let e = AudioEngine::capture(48_000);
    let clip = Arc::new(Clip {
        sample_rate: 48_000,
        channels: 1,
        samples: vec![8_192; 4_800],
    });
    e.play(
        clip,
        VoiceParams {
            gain: 1.0,
            looping: true,
            ..Default::default()
        },
    );
    let t0 = e.shared.now();
    let a = e.render_capture(4_800);
    assert_eq!(a.len(), 9_600);
    assert!(
        (e.shared.now() - t0).as_millis() == 100,
        "the clock moves with the sound"
    );
    assert!(a[2000..].iter().all(|x| (*x - 0.25).abs() < 0.01));
    assert!(
        AudioEngine::new_for_test_device_less()
            .render_capture(10)
            .is_empty()
    );
}

/// What the game asks and tells a voice never waits for a block being mixed (the mixer
/// holds the voices a whole block); a voice that ended by itself is gone once mixed, one
/// stopped at once.
#[test]
fn the_game_never_waits_for_the_mixer() {
    let e = AudioEngine::capture(48_000);
    let clip = Arc::new(Clip {
        sample_rate: 48_000,
        channels: 1,
        samples: vec![8_192; 480],
    });
    let short = e.play(clip.clone(), VoiceParams::default());
    let looped = e.play(
        clip.clone(),
        VoiceParams {
            looping: true,
            ..Default::default()
        },
    );
    e.render_capture(4_800);
    assert!(!e.is_playing(short) && e.is_playing(looped) && e.voice_count() == 1);
    // (a block being mixed: the mixer held for half a second)
    let (mixer, (tx, rx)) = (e.mixer.clone(), std::sync::mpsc::channel());
    let mixing = std::thread::spawn(move || {
        mixer.with(|_| {
            tx.send(()).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(500));
        })
    });
    rx.recv().unwrap();
    let t = std::time::Instant::now();
    e.set_params(
        looped,
        VoiceParams {
            gain: 0.5,
            looping: true,
            ..Default::default()
        },
    );
    let other = e.play(clip, VoiceParams::default());
    e.set_radio_fx(other, Default::default());
    let asked = (
        e.is_playing(looped),
        e.voice_state(looped).map(|s| s.1),
        e.is_playing(other),
    );
    e.stop(other);
    assert!(
        t.elapsed() < std::time::Duration::from_millis(250),
        "the game waited for the mixer"
    );
    assert_eq!(
        (asked, e.is_playing(other)),
        ((true, Some(1.0), true), false)
    );
    mixing.join().unwrap();
    e.render_capture(480);
    assert_eq!(e.voice_state(looped).map(|s| s.1), Some(0.5));
    assert_eq!(e.mixer.with(|m| m.voices.len()), 1);
}

impl AudioEngine {
    fn new_for_test_device_less() -> AudioEngine {
        AudioEngine::silent()
    }
}

/// A stream with its effects all at 0 plays exactly as one never given any; with the
/// speaker on it does not.
#[test]
fn a_stream_with_no_effects_is_bit_identical() {
    let render = |fx: Option<crate::radio_fx::RadioFxParams>| {
        let e = AudioEngine::capture(48_000);
        let buf = Arc::new(crate::stream::StreamBuf::default());
        buf.push(
            48_000,
            (0..48_000 * 3).map(|i| {
                let t = i as f32 / 48_000.0;
                [
                    (t * 440.0 * std::f32::consts::TAU).sin() * 0.5,
                    (t * 660.0 * std::f32::consts::TAU).sin() * 0.4,
                ]
            }),
        );
        let id = e.play_stream(
            buf,
            VoiceParams {
                gain: 0.8,
                ..Default::default()
            },
        );
        if let Some(p) = fx {
            e.set_radio_fx(id, p);
        }
        e.render_capture(48_000 * 2)
    };
    let plain = render(None);
    assert!(plain.iter().any(|x| x.abs() > 0.1));
    let off = render(Some(Default::default()));
    assert!(
        plain
            .iter()
            .zip(&off)
            .all(|(a, b)| a.to_bits() == b.to_bits())
    );
    let speaker = render(Some(crate::radio_fx::RadioFxParams {
        speaker: 1.0,
        ..Default::default()
    }));
    assert!(
        plain
            .iter()
            .zip(&speaker)
            .any(|(a, b)| (a - b).abs() > 0.01)
    );
}

/// The one-pole low-pass of a voice heard through the bodywork: a 5 kHz tone through
/// 500 Hz comes out some 20 dB down, a 100 Hz one hardly touched.
#[test]
fn the_voice_low_pass_follows_its_corner() {
    let level = |hz: f32| {
        let n = 48_000usize;
        let samples: Vec<i16> = (0..n)
            .map(|i| ((i as f32 / 48_000.0 * hz * std::f32::consts::TAU).sin() * 16_000.0) as i16)
            .collect();
        let clip = Arc::new(Clip {
            sample_rate: 48_000,
            channels: 1,
            samples,
        });
        let (mut s, _, _) = shared();
        let mut v = voice(clip, 1.0);
        v.params.lowpass_hz = 500.0;
        v.params.looping = false;
        s.voices.push(v);
        let mut out = vec![0.0f32; n];
        s.render(&mut out);
        let tail = &out[n / 2..];
        (tail.iter().map(|x| x * x).sum::<f32>() / tail.len() as f32).sqrt()
            / (16_000.0 / 32_768.0 / 2f32.sqrt())
    };
    let high = 20.0 * level(5_000.0).log10();
    assert!((high + 20.0).abs() < 1.5, "{high} dB");
    assert!(level(100.0) > 0.97);
}

#[test]
fn a_low_pass_switched_on_starts_from_the_sound() {
    let mut v = voice(
        Arc::new(Clip {
            sample_rate: 48_000,
            channels: 1,
            samples: vec![0; 4],
        }),
        1.0,
    );
    v.lp = [0.7, -0.7];
    apply_params(
        &mut v,
        VoiceParams {
            lowpass_hz: 800.0,
            ..Default::default()
        },
        std::time::Instant::now(),
        Vec3::ZERO,
        true,
    );
    assert!(v.lp_fresh);
    apply_params(
        &mut v,
        VoiceParams {
            lowpass_hz: 0.0,
            ..Default::default()
        },
        std::time::Instant::now(),
        Vec3::ZERO,
        true,
    );
    let mut w = voice(
        Arc::new(Clip {
            sample_rate: 48_000,
            channels: 1,
            samples: vec![0; 4],
        }),
        1.0,
    );
    apply_params(
        &mut w,
        VoiceParams {
            lowpass_hz: 0.0,
            ..Default::default()
        },
        std::time::Instant::now(),
        Vec3::ZERO,
        true,
    );
    assert!(!w.lp_fresh);
}

/// The mixer cuts no voice, near or far: `[sound_maxcount]` cuts whole sound packs
/// before they play (`crate::packs`).
#[test]
fn every_voice_is_mixed_however_many_there_are() {
    let clip = Arc::new(Clip {
        sample_rate: 48_000,
        channels: 1,
        samples: vec![64; 100],
    });
    let (mut s, _, _) = shared();
    for k in 0..250 {
        s.voices
            .push(far_voice(clip.clone(), if k < 50 { 900.0 } else { 5.0 }));
    }
    let mut out = vec![0.0f32; 16];
    s.render(&mut out);
    let expect = 250.0 * 64.0 / 32_768.0;
    assert!((out[0] - expect).abs() < 1e-3, "{} vs {expect}", out[0]);
}

/// More voices than the mixer had room for at first: room is made on the game's side
/// (the mixer never grows a list), and the voices that end come back to the game, which
/// lets go of their clips.
#[test]
fn room_is_made_by_the_game_and_ended_voices_come_back() {
    let e = AudioEngine::capture(48_000);
    let clip = Arc::new(Clip {
        sample_rate: 48_000,
        channels: 1,
        samples: vec![64; 48],
    });
    let ids: Vec<VoiceId> = (0..FIRST_ROOM * 3)
        .map(|_| e.play(clip.clone(), VoiceParams::default()))
        .collect();
    e.render_capture(16);
    let cap = e.mixer.with(|m| (m.voices.len(), m.voices.capacity()));
    assert_eq!(cap.0, FIRST_ROOM * 3);
    assert!(
        cap.1 >= FIRST_ROOM * 3 && e.game.lock().room == cap.1,
        "{cap:?}"
    );
    e.render_capture(480);
    assert!(ids.iter().all(|id| !e.is_playing(*id)) && e.voice_count() == 0);
    assert_eq!(
        Arc::strong_count(&clip),
        1,
        "the ended voices' clips are let go of"
    );
    assert_eq!(e.game.lock().in_mixer, 0);
}

/// Commands beyond the ring's room wait on the game's side and arrive in order.
#[test]
fn commands_beyond_the_ring_arrive_in_order() {
    let e = AudioEngine::capture(48_000);
    let clip = Arc::new(Clip {
        sample_rate: 48_000,
        channels: 1,
        samples: vec![64; 48],
    });
    let id = e.play(
        clip,
        VoiceParams {
            looping: true,
            ..Default::default()
        },
    );
    for k in 0..RING + 100 {
        e.set_params(
            id,
            VoiceParams {
                gain: k as f32,
                looping: true,
                ..Default::default()
            },
        );
    }
    assert!(!e.game.lock().overflow.is_empty());
    assert_eq!(e.voice_state(id).map(|s| s.0.gain), Some(1.0));
    e.render_capture(16);
    assert_eq!(
        e.voice_state(id).map(|s| s.0.gain),
        Some((RING + 99) as f32)
    );
    assert_eq!(
        e.mixer.with(|m| m.voices[0].params.gain),
        (RING + 99) as f32
    );
}
