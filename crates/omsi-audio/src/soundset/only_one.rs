//! `[onlyone]`: a sound played once, from the nearest object that plays it.
//!
//! Omsi.exe (sound-pack sort 0x7513e0 with 0x74fab0, the update 0x750340 stopping an entry
//! flagged at +0x41) visits the objects nearest first - by their distance over their largest
//! `[3d]` range (0x74f4fc) - and lets an `[onlyone]` sound play only while none visited
//! before it plays the same file (0x74fab0 adds the file to the list of those playing only
//! when the entry plays, +0x7c): of all the objects with that sound, only the nearest that
//! plays it is heard. The map's fireworks (`Sceneryobjects\Generic\sound\fireworks.cfg`) are
//! one display, not one more for every object placed.
//!
//! Here the objects are not updated nearest first, so the nearest one that plays a file
//! holds it; a nearer one that plays it takes it over; one that falls silent, or whose
//! object is gone or no longer updated, gives it up.

use omsi_vehicle::SoundEntry;
use std::collections::HashMap;
use std::sync::Mutex;

/// Who holds an `[onlyone]` file.
struct Claim {
    /// The entry (see [`OnlyOne::owner`]).
    owner: u64,
    /// How far its object is (see [`pack_range`]).
    key: f32,
    /// How many times its entry has said it plays on: a claim that stays the same while
    /// another entry is updated [`STALE_PASSES`] times more was left by its entry.
    renewals: u64,
}

/// The `[onlyone]` files playing anywhere, by their path in lower case.
static CLAIMS: Mutex<Option<HashMap<String, Claim>>> = Mutex::new(None);

/// A claim another entry finds unrenewed on this many of its updates in a row is given up:
/// its object is gone or no longer updated. Counted in updates rather than in time, so that
/// a long frame does not make the holder's claim look old to an object updated before it;
/// two, so that the holder has missed a whole frame however the objects' order changes.
const STALE_PASSES: u8 = 2;

/// An `[onlyone]` entry of a sound set.
pub(super) struct OnlyOne {
    /// Who it is among all the sets' entries.
    owner: u64,
    /// The file it shares with the other objects' entries (its whole path in lower case:
    /// Omsi.exe tells `[onlyone]` sounds apart by it).
    file: String,
    /// The claim of another entry it was refused by: (owner, renewals, the updates in a
    /// row it found it so).
    seen: Option<(u64, u64, u8)>,
}

impl OnlyOne {
    pub(super) fn new(file: String) -> OnlyOne {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        OnlyOne {
            owner: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            file,
            seen: None,
        }
    }

    /// Whether the entry, its object at `key` (see [`pack_range`]), may play its file this
    /// update; `plays`: it plays on, or is started, this update if let.
    pub(super) fn permits(&mut self, key: f32, plays: bool) -> bool {
        let mut claims = CLAIMS.lock().unwrap_or_else(|e| e.into_inner());
        let claims = claims.get_or_insert_with(Default::default);
        let holder = claims.get_mut(&self.file);
        let lapsed = match holder {
            Some(c) if c.owner == self.owner => {
                self.seen = None;
                if plays {
                    c.key = key;
                    c.renewals += 1;
                } else {
                    claims.remove(&self.file);
                }
                return true;
            }
            Some(c) => {
                let unchanged = match self.seen {
                    Some((o, r, n)) if o == c.owner && r == c.renewals => n.saturating_add(1),
                    _ => 0,
                };
                self.seen = Some((c.owner, c.renewals, unchanged));
                let lapsed = unchanged >= STALE_PASSES;
                if !lapsed && key.partial_cmp(&c.key) != Some(std::cmp::Ordering::Less) {
                    // a nearer object plays it
                    return false;
                }
                lapsed
            }
            None => false,
        };
        self.seen = None;
        if plays {
            claims.insert(
                self.file.clone(),
                Claim {
                    owner: self.owner,
                    key,
                    renewals: 0,
                },
            );
        } else if lapsed {
            claims.remove(&self.file);
        }
        true
    }

    /// Give the file up if this entry holds it: it falls silent.
    pub(super) fn release(&mut self) {
        self.seen = None;
        let mut claims = CLAIMS.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(claims) = claims.as_mut()
            && claims
                .get(&self.file)
                .is_some_and(|c| c.owner == self.owner)
        {
            claims.remove(&self.file);
        }
    }
}

impl Drop for OnlyOne {
    fn drop(&mut self) {
        self.release();
    }
}

/// How far a sound pack reaches, what Omsi.exe divides its distance by to sort the packs
/// (the loader's end, 0x74ee68): the largest `[3d]` range among its entries that can be
/// heard from outside (`[viewpoint]` 0, or one with bit 1 or 4), 10 m when none has one.
pub(super) fn pack_range<'a>(entries: impl Iterator<Item = &'a SoundEntry>) -> f32 {
    let r = entries
        .filter(|d| d.pos.is_some() && (d.viewpoint == 0 || d.viewpoint & 5 != 0))
        .map(|d| d.range)
        .fold(0.0f32, f32::max);
    if r == 0.0 { 10.0 } else { r }
}

#[cfg(test)]
mod tests {
    use super::super::tests::globals;
    use super::super::{RuntimeSound, SoundSet};
    use super::*;
    use crate::mixer::{AudioEngine, Clip};
    use glam::{Mat4, Vec3};
    use std::sync::Arc;

    /// Of the objects sharing the sound, the nearest one that plays it holds it, a nearer
    /// one takes it over, one falling silent gives it up.
    #[test]
    fn an_onlyone_sound_is_held_by_the_nearest_object() {
        let f = || OnlyOne::new("test-onlyone-claims.wav".into());
        let (mut a, mut b, mut c) = (f(), f(), f());
        assert!(a.permits(4.0, true), "the first one heard");
        assert!(b.permits(1.0, true), "a nearer one takes over");
        assert!(!a.permits(4.0, true), "the far one is stopped");
        assert!(b.permits(1.0, false), "the near one falls silent");
        assert!(a.permits(4.0, true), "and the far one is heard again");
        assert!(b.permits(1.0, false), "a nearer silent one may be silent");
        assert!(a.permits(4.0, true), "and does not take the file");
        assert!(b.permits(1.0, true));
        // the far one updated twice in a frame (a long one, or one the near one is updated
        // later in): the near one's claim stands
        assert!(!a.permits(4.0, true));
        assert!(!a.permits(4.0, true));
        assert!(b.permits(1.0, true), "renewed");
        assert!(!a.permits(4.0, true));
        assert!(!a.permits(4.0, true));
        // the near one no longer updated: its claim lapses
        assert!(!c.permits(9.0, true));
        assert!(!c.permits(9.0, true));
        assert!(c.permits(9.0, true), "a claim not renewed lapses");
        c.release();
        assert!(a.permits(4.0, true), "given up when its entry falls silent");
        drop(a);
        let mut d = f();
        assert!(d.permits(99.0, true), "given up when its entry goes");
    }

    /// A triggered `[onlyone]` sound fired again at a volume that is not heard plays on: it
    /// keeps its file (Omsi.exe's +0x7c stays set while the buffer plays).
    #[test]
    fn a_playing_onlyone_sound_keeps_its_file() {
        let _g = globals();
        let engine = AudioEngine::capture(48_000);
        let clip = Arc::new(Clip {
            sample_rate: 48_000,
            channels: 1,
            samples: vec![1000; 48_000],
        });
        let entry = SoundEntry {
            file: "test-onlyone-bounce.wav".into(),
            volume: 1.0,
            only_one: true,
            triggers: vec!["ev_bang".into()],
            vol_curves: vec![omsi_vehicle::VolCurve {
                variable: "loud".into(),
                points: vec![(0.0, 0.0), (1.0, 1.0)],
            }],
            pos: Some([0.0, 0.0, 0.0]),
            range: 50.0,
            ..Default::default()
        };
        let (mut near, mut far) = (set(&entry, &clip), set(&entry, &clip));
        let near_at = Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0));
        let far_at = Mat4::from_translation(Vec3::new(40.0, 0.0, 0.0));
        let loud = |n: &str| (n == "loud").then_some(1.0);
        let quiet = |n: &str| (n == "loud").then_some(0.0);
        let bang = ["ev_bang".to_string()];
        let plays = |s: &SoundSet| s.sounds[0].voice.is_some_and(|id| engine.is_playing(id));
        near.update(&engine, &loud, &near_at, &bang);
        assert!(plays(&near));
        near.update(&engine, &quiet, &near_at, &bang);
        assert!(plays(&near), "fired again, unheard: the bang plays on");
        far.update(&engine, &loud, &far_at, &bang);
        assert!(!plays(&far), "while it plays, the far one is not heard");
    }

    fn set(entry: &SoundEntry, clip: &Arc<Clip>) -> SoundSet {
        SoundSet {
            sounds: vec![RuntimeSound::new(entry.clone(), Some(clip.clone()))],
            master: 1.0,
            dir: Default::default(),
            inside: false,
            amount: 0.0,
            ai: true,
            scenery: true,
            listener_vehicle: false,
            muffled: false,
            parts: Vec::new(),
            vars: None,
        }
    }

    /// Two fireworks objects (`[onlyone]`, `[3d]` range 50): only the nearer is heard.
    #[test]
    fn two_objects_with_the_same_onlyone_sound_play_it_once() {
        let _g = globals();
        let engine = AudioEngine::capture(48_000);
        let clip = Arc::new(Clip {
            sample_rate: 48_000,
            channels: 1,
            samples: vec![1000; 4800],
        });
        let entry = SoundEntry {
            file: "test-onlyone-fireworks.wav".into(),
            volume: 1.0,
            only_one: true,
            pos: Some([0.0, 0.0, 0.0]),
            range: 50.0,
            ..Default::default()
        };
        let far_at = Mat4::from_translation(Vec3::new(40.0, 0.0, 0.0));
        let near_at = Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0));
        let (mut far, mut near) = (set(&entry, &clip), set(&entry, &clip));
        let none = |_: &str| None;
        let plays = |s: &SoundSet| s.sounds[0].voice.is_some_and(|id| engine.is_playing(id));
        far.update(&engine, &none, &far_at, &[]);
        assert!(plays(&far), "alone, the far one is heard");
        near.update(&engine, &none, &near_at, &[]);
        far.update(&engine, &none, &far_at, &[]);
        assert!(plays(&near));
        assert!(!plays(&far), "the nearer one has taken the sound over");
        near.stop_all(&engine);
        far.update(&engine, &none, &far_at, &[]);
        assert!(
            plays(&far),
            "the nearer one stopped, the far one plays at once"
        );
        near.update(&engine, &none, &near_at, &[]);
        far.update(&engine, &none, &far_at, &[]);
        assert!(plays(&near) && !plays(&far));
        drop(near);
        far.update(&engine, &none, &far_at, &[]);
        assert!(plays(&far), "the nearer one gone, the far one plays again");
    }

    /// Two objects' configs naming the same file in other words - another folder, another
    /// case - share it: [`SoundSet::new`] keys it by the whole path in lower case.
    #[test]
    fn the_same_file_named_otherwise_is_one_onlyone_sound() {
        let _g = globals();
        let root = std::env::temp_dir().join(format!("openomsi-onlyone-{}", std::process::id()));
        let sound = root.join("fireworks").join("sound");
        let model = root.join("fireworks").join("model");
        std::fs::create_dir_all(&sound).unwrap();
        std::fs::create_dir_all(&model).unwrap();
        let data = vec![0x10u8; 48_000];
        let mut wav = b"RIFF".to_vec();
        wav.extend_from_slice(&((36 + data.len()) as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&1u16.to_le_bytes()); // mono
        wav.extend_from_slice(&24_000u32.to_le_bytes());
        wav.extend_from_slice(&48_000u32.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
        wav.extend_from_slice(&data);
        std::fs::write(sound.join("fireworks.wav"), wav).unwrap();
        let cfg = |file: &str| omsi_vehicle::SoundCfg {
            sounds: vec![SoundEntry {
                file: file.into(),
                volume: 1.0,
                only_one: true,
                pos: Some([0.0, 0.0, 0.0]),
                range: 50.0,
                ..Default::default()
            }],
            ..Default::default()
        };
        let engine = AudioEngine::capture(48_000);
        let mut near = SoundSet::new_scenery(&engine, &cfg("Fireworks.WAV"), &sound);
        let mut far = SoundSet::new_scenery(&engine, &cfg(r"..\SOUND\fireworks.wav"), &model);
        let _ = std::fs::remove_dir_all(&root);
        let file = |s: &SoundSet| s.sounds[0].only_one.as_ref().map(|o| o.file.clone());
        assert!(near.sounds[0].clip.is_some() && far.sounds[0].clip.is_some());
        assert_eq!(file(&near), file(&far));
        let near_at = Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0));
        let far_at = Mat4::from_translation(Vec3::new(40.0, 0.0, 0.0));
        let none = |_: &str| None;
        let plays = |s: &SoundSet| s.sounds[0].voice.is_some_and(|id| engine.is_playing(id));
        far.update(&engine, &none, &far_at, &[]);
        near.update(&engine, &none, &near_at, &[]);
        far.update(&engine, &none, &far_at, &[]);
        assert!(plays(&near));
        assert!(!plays(&far), "one sound: the nearer one plays it");
    }

    #[test]
    fn a_pack_reaches_as_far_as_its_largest_outside_3d_range() {
        let e = |vp: i32, range: f32, pos: bool| SoundEntry {
            viewpoint: vp,
            range,
            pos: pos.then_some([0.0; 3]),
            ..Default::default()
        };
        let pack = [
            e(0, 20.0, true),
            e(2, 80.0, true),
            e(1, 30.0, true),
            e(0, 99.0, false),
        ];
        assert_eq!(pack_range(pack.iter()), 30.0);
        assert_eq!(
            pack_range([e(2, 80.0, true)].iter()),
            10.0,
            "none from outside: 10 m"
        );
    }
}
