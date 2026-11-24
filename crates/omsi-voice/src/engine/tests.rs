use super::*;
use crate::dsp::tone_frame;
use omsi_audio::{AudioEngine, VoiceParams};

fn on() -> Settings {
    Settings {
        enabled: true,
        ..Default::default()
    }
}

fn core() -> Core {
    let mut c = Core::new().unwrap();
    c.apply(&on());
    c
}

fn frames(c: &mut Core, n: usize, freq: f32, amp: f32, tx: Option<Channel>) {
    for f in 0..n {
        c.mic(&tone_frame(freq, f, amp), tx);
    }
}

fn as_frame(from: u32, o: &Outgoing) -> Frame {
    Frame {
        from,
        channel: o.channel,
        seq: o.seq,
        end: o.end,
        payload: o.payload.clone(),
    }
}

/// Energy of `x` (stereo interleaved, left) at `freq`.
fn at(x: &[f32], freq: f32) -> f32 {
    let left: Vec<f32> = x.iter().step_by(2).copied().collect();
    let (mut re, mut im) = (0.0, 0.0);
    for (i, s) in left.iter().enumerate() {
        let a = 2.0 * std::f32::consts::PI * freq * i as f32 / RATE as f32;
        re += s * a.cos();
        im += s * a.sin();
    }
    (re * re + im * im).sqrt() / left.len() as f32 * 2.0
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0f32, |m, v| m.max(v.abs()))
}

/// Play `packets` into a listener's core on a fake clock, through a real mixer without a
/// device; the sound heard (stereo) from the speaker `from` is returned.
fn hear(packets: Vec<(u64, Frame)>, settle_ms: u64, gain: f32, spatial: bool) -> Vec<f32> {
    let audio = AudioEngine::capture(RATE);
    let mut b = core();
    let t0 = Instant::now();
    let mut ids = HashMap::new();
    let mut out = Vec::new();
    let mut queue: VecDeque<(u64, Frame)> = packets.into();
    let last = queue.back().map(|p| p.0).unwrap_or(0) + settle_ms;
    let mut ms = 0u64;
    while ms <= last {
        while queue.front().is_some_and(|p| p.0 <= ms) {
            b.receive(t0 + Duration::from_millis(ms), queue.pop_front().unwrap().1);
        }
        b.play(t0 + Duration::from_millis(ms));
        for s in b.new_streams.drain(..) {
            let params = VoiceParams {
                gain,
                position: spatial.then_some(glam::Vec3::new(3.0, 0.0, 0.0)),
                range: 1.0e9,
                doppler: false,
                ..Default::default()
            };
            ids.insert(s.id, audio.play_stream(s.stream, params));
        }
        // 4 ms of sound
        out.extend(audio.render_capture(192));
        ms += 4;
    }
    out
}

fn transmission(c: &mut Core, ch: Channel, n: usize, freq: f32, amp: f32) -> Vec<Outgoing> {
    frames(c, n, freq, amp, Some(ch));
    c.mic(&[], None);
    std::mem::take(&mut c.out)
}

#[test]
fn a_held_key_sends_a_packet_every_20_ms_and_the_release_ends_it() {
    let mut a = core();
    let sent = transmission(&mut a, Channel::Nearby, 10, 440.0, 0.3);
    assert_eq!(sent.len(), 11);
    assert!(sent[..10].iter().enumerate().all(|(i, o)| !o.end
        && o.seq == i as u16
        && o.channel == Channel::Nearby
        && !o.payload.is_empty()
        && o.payload.len() <= 400));
    assert!(sent[10].end && sent[10].payload.is_empty() && sent[10].channel == Channel::Nearby);
    // no key: the microphone is not sent, whatever it hears
    frames(&mut a, 10, 440.0, 0.5, None);
    assert!(a.out.is_empty() && a.sending().is_none());
}

#[test]
fn nothing_is_sent_while_voice_chat_is_off() {
    let mut a = Core::new().unwrap();
    frames(&mut a, 10, 440.0, 0.5, Some(Channel::Radio));
    assert!(a.out.is_empty());
    assert!(!a.wants_mic(Some(Channel::Radio)));
    a.apply(&on());
    assert!(a.wants_mic(Some(Channel::Radio)) && !a.wants_mic(None));
}

#[test]
fn the_open_microphone_sends_what_is_loud_enough_on_its_channel() {
    let mut a = Core::new().unwrap();
    a.apply(&Settings {
        enabled: true,
        mode: Mode::Open,
        open_channel: Channel::Radio,
        gate_db: -40.0,
        ..Default::default()
    });
    assert!(a.wants_mic(None));
    frames(&mut a, 5, 300.0, 0.002, None);
    assert!(a.out.is_empty(), "a hum is not speech");
    frames(&mut a, 5, 300.0, 0.3, None);
    assert_eq!(a.out.len(), 5);
    assert!(a.out.iter().all(|o| o.channel == Channel::Radio));
    // the silence after: held a while, then the transmission ends
    frames(&mut a, 30, 300.0, 0.0, None);
    let ends = a.out.iter().filter(|o| o.end).count();
    assert_eq!(ends, 1);
    assert!(a.out.last().unwrap().end);
}

#[test]
fn a_nearby_voice_is_heard_as_it_was_spoken() {
    let mut a = core();
    let sent = transmission(&mut a, Channel::Nearby, 40, 440.0, 0.4);
    let packets: Vec<_> = sent
        .iter()
        .enumerate()
        .map(|(i, o)| (i as u64 * 20, as_frame(7, o)))
        .collect();
    let heard = hear(packets, 300, 1.0, false);
    // about 40 frames of 20 ms came out, at the pitch spoken
    let loud = heard.chunks(1920).filter(|c| peak(c) > 0.05).count();
    assert!((30..=44).contains(&loud), "{loud} loud blocks of 20 ms");
    let mid = &heard[heard.len() / 2 - 4800..heard.len() / 2 + 4800];
    let level = at(mid, 440.0);
    assert!(level > 0.25 && level < 0.5, "440 Hz at {level}");
    assert!(at(mid, 1700.0) < 0.03);
    // and nothing keeps ringing after
    assert!(peak(&heard[heard.len() - 4800..]) < 0.01);
}

#[test]
fn the_radio_is_band_limited_with_a_click_each_end() {
    let mut a = core();
    let sent = transmission(&mut a, Channel::Radio, 30, 100.0, 0.4);
    let packets: Vec<_> = sent
        .iter()
        .enumerate()
        .map(|(i, o)| (i as u64 * 20, as_frame(8, o)))
        .collect();
    let heard = hear(packets, 400, 1.0, false);
    // a 100 Hz hum is mostly gone through the radio's band
    let mid = &heard[heard.len() / 2 - 4800..heard.len() / 2 + 4800];
    assert!(at(mid, 100.0) < 0.1, "100 Hz at {}", at(mid, 100.0));
    // the clicks: the first 30 ms and the stretch after the voice carry something
    let first = peak(&heard[..2 * 48 * 120]);
    assert!(first > 0.01, "opening click {first}");
    let tail_start = (30 * 20 + 30) * 96;
    assert!(
        peak(&heard[tail_start..tail_start + 96 * 100]) > 0.01,
        "closing click"
    );
    assert!(
        peak(&heard[heard.len() - 9600..]) < 0.02,
        "quiet at the end"
    );
}

#[test]
fn lost_and_late_packets_are_covered() {
    let mut a = core();
    let sent = transmission(&mut a, Channel::Nearby, 50, 440.0, 0.4);
    let mut packets = Vec::new();
    for (i, o) in sent.iter().enumerate() {
        // every seventh lost, one in five delayed by 30 ms (so out of order)
        if i % 7 == 3 {
            continue;
        }
        let delay = if i % 5 == 1 { 30 } else { 0 };
        packets.push((i as u64 * 20 + delay, as_frame(9, o)));
    }
    packets.sort_by_key(|p| p.0);
    let heard = hear(packets, 300, 1.0, false);
    let mid = &heard[heard.len() / 2 - 9600..heard.len() / 2 + 9600];
    assert!(
        at(mid, 440.0) > 0.2,
        "the voice goes on through the losses: {}",
        at(mid, 440.0)
    );
    assert!(heard.iter().all(|v| v.is_finite() && v.abs() <= 1.5));
}

#[test]
fn a_spatial_voice_is_louder_in_the_nearer_ear_and_the_gain_applies() {
    let mut a = core();
    let sent = transmission(&mut a, Channel::Nearby, 30, 440.0, 0.4);
    let packets: Vec<_> = sent
        .iter()
        .enumerate()
        .map(|(i, o)| (i as u64 * 20, as_frame(7, o)))
        .collect();
    let full = hear(packets.clone(), 200, 1.0, true);
    let half = hear(packets, 200, 0.5, true);
    let mid = full.len() / 2 - 4800..full.len() / 2 + 4800;
    let (l, r): (Vec<f32>, Vec<f32>) = (
        full[mid.clone()].chunks(2).map(|c| c[0]).collect(),
        full[mid.clone()].chunks(2).map(|c| c[1]).collect(),
    );
    assert!(
        peak(&r) > peak(&l),
        "a voice to the right is louder on the right: {} {}",
        peak(&l),
        peak(&r)
    );
    assert!(peak(&half[mid.clone()]) < peak(&full[mid]) * 0.6);
}

#[test]
fn muted_players_and_all_voices_off_are_not_heard() {
    let mut a = core();
    let sent = transmission(&mut a, Channel::Radio, 10, 440.0, 0.4);
    let mut b = core();
    b.muted.insert(5);
    let now = Instant::now();
    for o in &sent {
        b.receive(now, as_frame(5, o));
    }
    assert_eq!(b.speaker_count(), 0);
    for o in &sent {
        b.receive(now, as_frame(6, o));
    }
    assert_eq!(b.speaker_count(), 1);
    b.play(now + Duration::from_millis(100));
    assert_eq!(b.talkers().len(), 1);
    // muted while talking: gone at once
    b.muted.insert(6);
    b.play(now + Duration::from_millis(140));
    assert_eq!(b.speaker_count(), 0);
    b.muted.clear();
    b.all_off = true;
    for o in &sent {
        b.receive(now, as_frame(6, o));
    }
    assert_eq!(b.speaker_count(), 0);
}

#[test]
fn a_flood_of_speakers_and_junk_is_bounded() {
    let mut b = core();
    let now = Instant::now();
    for id in 1..=500u32 {
        b.receive(
            now,
            Frame {
                from: id,
                channel: Channel::Nearby,
                seq: 1,
                end: false,
                payload: vec![1, 2, 3],
            },
        );
    }
    assert_eq!(b.speaker_count(), MAX_SPEAKERS);
    // junk that is not Opus, a packet from nobody, an end with nothing before it
    b.receive(
        now,
        Frame {
            from: 0,
            channel: Channel::Radio,
            seq: 1,
            end: false,
            payload: vec![9; 50],
        },
    );
    b.receive(
        now,
        Frame {
            from: 900,
            channel: Channel::Radio,
            seq: 1,
            end: true,
            payload: Vec::new(),
        },
    );
    for ms in (0..400).step_by(4) {
        for id in 1..=40u32 {
            b.receive(
                now + Duration::from_millis(ms),
                Frame {
                    from: id,
                    channel: Channel::Radio,
                    seq: ms as u16,
                    end: false,
                    payload: vec![0xFF; 399],
                },
            );
        }
        b.play(now + Duration::from_millis(ms));
    }
    assert!(b.speaker_count() <= MAX_SPEAKERS);
    // forgotten when silent for long
    for k in 0..120 {
        b.play(now + Duration::from_secs(60) + Duration::from_millis(20 * k));
    }
    assert_eq!(b.speaker_count(), 0);
}

/// The whole chain on its threads, with the synthetic microphone: two voices, one speaks.
#[test]
fn two_engines_talk_through_the_test_tone() {
    unsafe { std::env::set_var("OMSI_VOICE_TEST_TONE", "440") };
    let (a, b) = (Voice::new(), Voice::new());
    a.set_settings(&on());
    b.set_settings(&on());
    a.set_transmit(Some(Channel::Radio));
    let start = Instant::now();
    let mut heard = false;
    while start.elapsed() < Duration::from_millis(2500) {
        for o in a.take_outgoing() {
            b.feed(as_frame(1, &o));
        }
        if !b.status().talkers.is_empty() {
            heard = true;
            if start.elapsed() > Duration::from_millis(1200) {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let st = b.status();
    assert!(
        heard,
        "the second engine heard nothing: a {:?} b {:?}",
        a.status(),
        st
    );
    assert!(
        a.status().transmitting == Some(Channel::Radio)
            && a.status().mic_open
            && a.status().device == "test tone"
    );
    let (_, sent) = a.counters();
    let (got, _) = b.counters();
    assert!(sent >= 30 && got >= 30, "sent {sent}, got {got}");
    assert!(
        st.talkers
            .iter()
            .all(|t| t.id == 1 && t.channel == Channel::Radio && t.level_db > -40.0),
        "{:?}",
        st.talkers
    );
    // let go of the key: a final packet, then the microphone closes after its grace
    a.set_transmit(None);
    std::thread::sleep(Duration::from_millis(100));
    let tail = a.take_outgoing();
    assert!(tail.iter().any(|o| o.end));
    // switched off: the microphone is closed at once and nothing more is sent
    a.set_settings(&Settings::default());
    std::thread::sleep(Duration::from_millis(150));
    assert!(!a.status().mic_open);
    unsafe { std::env::remove_var("OMSI_VOICE_TEST_TONE") };
}
