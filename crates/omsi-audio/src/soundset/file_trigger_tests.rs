//! `(T.F.trigger)` with an empty or a blank file name (0x74f2e8).

use super::*;

fn announcer(engine: &AudioEngine, clip: Option<Arc<Clip>>) -> SoundSet {
    let mut set = SoundSet::new(engine, &SoundCfg::default(), Path::new("/nonexistent"));
    set.sounds.push(RuntimeSound::new(
        SoundEntry {
            file: "gong.wav".into(),
            volume: 1.0,
            triggers: vec!["ann".into()],
            ..Default::default()
        },
        clip,
    ));
    set
}

fn gong() -> Arc<Clip> {
    Arc::new(Clip {
        sample_rate: 48_000,
        channels: 1,
        samples: vec![1000; 4800],
    })
}

fn playing(set: &SoundSet, engine: &AudioEngine) -> bool {
    set.sounds[0].voice.is_some_and(|id| engine.is_playing(id))
}

#[test]
fn an_empty_file_plays_the_entry_s_own_file_again() {
    let engine = AudioEngine::silent();
    let clip = gong();
    let mut set = announcer(&engine, Some(clip.clone()));
    set.play_file_trigger(&engine, "ANN", "", &|_| None, &Mat4::IDENTITY);
    assert!(playing(&set, &engine));
    let first = set.sounds[0].voice;
    assert!(
        set.sounds[0]
            .clip
            .as_ref()
            .is_some_and(|c| Arc::ptr_eq(c, &clip)),
        "the file it had"
    );
    // fired again: started anew
    set.play_file_trigger(&engine, "ann", "", &|_| None, &Mat4::IDENTITY);
    assert!(playing(&set, &engine));
    assert_ne!(set.sounds[0].voice, first);
    assert!(first.is_some_and(|id| !engine.is_playing(id)));
}

#[test]
fn a_blank_file_plays_nothing() {
    let engine = AudioEngine::silent();
    let mut set = announcer(&engine, Some(gong()));
    set.play_file_trigger(&engine, "ann", " ", &|_| None, &Mat4::IDENTITY);
    assert!(set.sounds[0].voice.is_none());
}

/// `" " (T.F.x)` frees the entry's buffer (0x74fb3c -> 0x75030c): what it plays stops, and
/// it stays silent - `""` re-fires it with no file - until a `(T.F.)` names one that loads.
#[test]
fn a_blank_file_stops_the_entry_and_keeps_it_silent() {
    let engine = AudioEngine::silent();
    let mut set = announcer(&engine, Some(gong()));
    set.play_file_trigger(&engine, "ann", "", &|_| None, &Mat4::IDENTITY);
    let voice = set.sounds[0].voice;
    assert!(playing(&set, &engine));
    set.play_file_trigger(&engine, "ann", " ", &|_| None, &Mat4::IDENTITY);
    assert!(voice.is_some_and(|id| !engine.is_playing(id)), "stopped");
    assert!(set.sounds[0].voice.is_none() && set.sounds[0].clip.is_none());
    set.play_file_trigger(&engine, "ann", "", &|_| None, &Mat4::IDENTITY);
    assert!(set.sounds[0].voice.is_none(), "no file to play again");
}

/// A named file that is not there does the same (0x74fe28 marks the entry not loaded).
#[test]
fn a_missing_file_stops_the_entry_and_keeps_it_silent() {
    let engine = AudioEngine::silent();
    let mut set = announcer(&engine, Some(gong()));
    set.play_file_trigger(&engine, "ann", "", &|_| None, &Mat4::IDENTITY);
    let voice = set.sounds[0].voice;
    set.play_file_trigger(&engine, "ann", "missing.wav", &|_| None, &Mat4::IDENTITY);
    assert!(voice.is_some_and(|id| !engine.is_playing(id)), "stopped");
    assert!(set.sounds[0].clip.is_none());
}

#[test]
fn an_empty_file_without_a_file_to_play_again_is_silent() {
    let engine = AudioEngine::silent();
    let mut set = announcer(&engine, None);
    set.play_file_trigger(&engine, "ann", "", &|_| None, &Mat4::IDENTITY);
    assert!(set.sounds[0].voice.is_none());
}

#[test]
fn another_trigger_s_empty_file_leaves_the_entry_alone() {
    let engine = AudioEngine::silent();
    let mut set = announcer(&engine, Some(gong()));
    set.play_file_trigger(&engine, "door", "", &|_| None, &Mat4::IDENTITY);
    assert!(set.sounds[0].voice.is_none());
}
