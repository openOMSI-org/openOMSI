//! `[sound_maxcount]` (options.cfg): how many sounds Omsi.exe lets play at once - counted in
//! whole sound packs (an object's sound configuration), nearest the camera first.
//!
//! Once a frame (0x7513e0, on the calculation thread) Omsi.exe sorts the packs of the
//! vehicles (the lists at 0x858d18 and 0x859d68), of the scenery objects around the
//! camera's tile (the tile's near links) and of the people (0x858cc8) by their distance
//! from the camera over their range (0x74f4fc, see `pack_range`); the user's own
//! vehicles go in at 0 (0x751398 hands their flag at +0x26c to 0x751010). The sort
//! (0x7511b4) puts a pack after those with the same key. It then walks them nearest first
//! (0x7512bc), adding up their `[sound]`/`[loopsound]` entries (TSoundPack +0x14): a pack
//! is too far when the entries of the packs heard before it are more than
//! `[sound_maxcount]` and its key is not 0. A pack that is too far adds nothing. One that
//! becomes too far stops all its sounds ("Stopped Sound ... too far": 0x74f224 sets +0x7e
//! of each for 0x750340) and is not updated at all while it stays too far - no loop
//! starts, a trigger plays nothing, a `(T.F.)` sound neither (0x74f2e8 tests +0x34) -
//! until it comes near enough again.
//!
//! Here each [`crate::SoundSet`] is a pack: it says where it is when it is updated, and
//! the engine sorts the frame's packs when the listener is set (once a frame), for the
//! next frame's updates.

use hashbrown::{HashMap, HashSet};
use omsi_vehicle::SoundEntry;
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};

/// `[sound_maxcount]` as Omsi.exe has it before it reads options.cfg (0x82f4b4).
pub const DEFAULT_SOUND_MAXCOUNT: i32 = 100;

static SOUND_MAXCOUNT: AtomicI32 = AtomicI32::new(DEFAULT_SOUND_MAXCOUNT);

/// Set `[sound_maxcount]`: how many sound entries the nearest packs may have before the
/// next ones are cut (a negative one cuts every pack but the user's vehicles, as the
/// exe's comparison does).
pub fn set_sound_maxcount(n: i32) {
    SOUND_MAXCOUNT.store(n, Ordering::Relaxed);
}

pub fn sound_maxcount() -> i32 {
    SOUND_MAXCOUNT.load(Ordering::Relaxed)
}

/// A new name for a pack.
pub(crate) fn next_pack_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// How far a pack reaches, what its distance is divided by (the loader's end, 0x74ee68):
/// the largest `[3d]` range among its entries that can be heard from outside
/// (`[viewpoint]` 0, or one with bit 1 or 4), 10 m when none has one.
pub(crate) fn pack_range<'a>(entries: impl IntoIterator<Item = &'a SoundEntry>) -> f32 {
    let r = entries
        .into_iter()
        .filter(|d| d.pos.is_some() && (d.viewpoint == 0 || d.viewpoint & 5 != 0))
        .map(|d| d.range)
        .fold(0.0f32, f32::max);
    if r == 0.0 { 10.0 } else { r }
}

/// What a sound set is as a pack: its name, its entries and its range.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PackInfo {
    pub id: u64,
    pub entries: usize,
    pub range: f32,
}

impl PackInfo {
    pub(crate) fn of(entries: &[SoundEntry]) -> PackInfo {
        PackInfo {
            id: next_pack_id(),
            entries: entries.len(),
            range: pack_range(entries),
        }
    }
}

impl Default for PackInfo {
    fn default() -> PackInfo {
        PackInfo::of(&[])
    }
}

/// How many frames a pack counts after its last update.
const KEEP_FRAMES: u64 = 60;

/// A pack as it was updated this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Pack {
    pub id: u64,
    /// Its distance from the camera over its range; 0 for the user's vehicles.
    pub key: f32,
    /// How many `[sound]`/`[loopsound]` entries it has.
    pub entries: usize,
}

/// The packs of a frame, sorted for `maxcount`.
#[derive(Default)]
struct Sorted {
    /// Every pack sorted.
    ids: HashSet<u64>,
    too_far: HashSet<u64>,
    /// Each pack's key, nearest first, with the entries heard up to and with it.
    heard: Vec<(f32, u64)>,
    maxcount: i32,
}

impl Sorted {
    fn of(packs: &mut [Pack], maxcount: i32) -> Sorted {
        // (stable: a pack goes after those with the same key, as 0x7511b4 inserts it)
        packs.sort_by(|a, b| a.key.total_cmp(&b.key));
        let mut out = Sorted {
            maxcount,
            ..Default::default()
        };
        let mut heard: u64 = 0;
        for p in packs.iter() {
            out.ids.insert(p.id);
            if Self::cuts(maxcount, heard, p.key) {
                out.too_far.insert(p.id);
            } else {
                heard += p.entries as u64;
            }
            out.heard.push((p.key, heard));
        }
        out
    }

    /// Whether a pack at `key` is too far with `heard` entries heard before it (the exe
    /// compares the setting, sign-extended, with the unsigned count).
    fn cuts(maxcount: i32, heard: u64, key: f32) -> bool {
        i64::from(maxcount) < heard as i64 && key != 0.0
    }

    /// Whether pack `p` is too far: as sorted, or - one made since - where it would have
    /// gone in among them (so that it does not start for one frame only to stop).
    fn too_far(&self, p: &Pack) -> bool {
        if self.ids.contains(&p.id) {
            return self.too_far.contains(&p.id);
        }
        let before = self.heard.partition_point(|(k, _)| *k <= p.key);
        let heard = before.checked_sub(1).map_or(0, |i| self.heard[i].1);
        Self::cuts(self.maxcount, heard, p.key)
    }
}

/// The engine's record of the packs: those in being, and how they sorted at the end of the
/// last frame.
///
/// Omsi.exe sorts all its packs every frame. Here a pack is known by its updates, and not
/// every set is updated in every frame (the traffic's and the people's only in the frames
/// the simulation ticks, see `SimTicks::view_due`): sorted over one frame's updates alone,
/// a far car was heard after each frame without the traffic and cut after the next, its
/// loops starting over several times a second. So a pack counts for [`KEEP_FRAMES`] after
/// its last update.
#[derive(Default)]
pub(crate) struct Packs {
    /// The packs, in the order they came in, each with the frame of its last update.
    known: Vec<(Pack, u64)>,
    /// Where each of them is in `known`.
    at: HashMap<u64, usize>,
    frame: u64,
    /// Was any pack updated since the last sort?
    updated: bool,
    last: Sorted,
}

impl Packs {
    /// Pack `p` is updated: whether it may be heard (as sorted at the end of the last
    /// frame).
    pub(crate) fn heard(&mut self, p: Pack) -> bool {
        self.updated = true;
        match self.at.get(&p.id) {
            Some(&i) => self.known[i] = (p, self.frame),
            None => {
                self.at.insert(p.id, self.known.len());
                self.known.push((p, self.frame));
            }
        }
        !self.last.too_far(&p)
    }

    /// Whether pack `id` was too far at the end of the last frame.
    pub(crate) fn is_too_far(&self, id: u64) -> bool {
        self.last.too_far.contains(&id)
    }

    /// The frame is over: sort the packs for the next one. (A frame without any keeps the
    /// last one's: the listener set twice in a frame.)
    pub(crate) fn end_frame(&mut self, maxcount: i32) {
        if !std::mem::take(&mut self.updated) {
            return;
        }
        let frame = self.frame;
        self.frame += 1;
        let before = self.known.len();
        self.known.retain(|k| frame - k.1 < KEEP_FRAMES);
        if self.known.len() != before {
            self.at = self
                .known
                .iter()
                .enumerate()
                .map(|(i, k)| (k.0.id, i))
                .collect();
        }
        let mut packs: Vec<Pack> = self.known.iter().map(|k| k.0).collect();
        self.last = Sorted::of(&mut packs, maxcount);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(id: u64, key: f32, entries: usize) -> Pack {
        Pack { id, key, entries }
    }

    fn cut(mut packs: Vec<Pack>, maxcount: i32) -> Vec<u64> {
        let mut ids: Vec<u64> = Sorted::of(&mut packs, maxcount)
            .too_far
            .into_iter()
            .collect();
        ids.sort();
        ids
    }

    #[test]
    fn packs_are_cut_once_the_nearer_ones_have_more_entries_than_the_limit() {
        // nearest first: 3 (key 0.5, 4 entries), 1 (1.0, 4), 2 (2.0, 4), 4 (3.0, 4)
        let packs = vec![
            pack(1, 1.0, 4),
            pack(2, 2.0, 4),
            pack(3, 0.5, 4),
            pack(4, 3.0, 4),
        ];
        // 0, 4, 8 entries before the first three: 8 is not more than 8, 12 is
        assert_eq!(cut(packs.clone(), 8), vec![4]);
        assert_eq!(cut(packs.clone(), 7), vec![2, 4]);
        assert_eq!(cut(packs.clone(), 100), Vec::<u64>::new());
        // the nearest is always heard: nothing before it
        assert_eq!(cut(packs, 0), vec![1, 2, 4]);
    }

    #[test]
    fn a_whole_pack_is_heard_however_many_entries_it_has() {
        // 3 entries before the big one: it is heard with all its 50, the next is not
        let packs = vec![pack(1, 0.1, 3), pack(2, 0.2, 50), pack(3, 0.3, 1)];
        assert_eq!(cut(packs, 5), vec![3]);
    }

    #[test]
    fn the_user_s_vehicles_are_never_cut_but_count() {
        // the own bus (key 0) with 120 entries: with the stock 100 every other pack goes
        let packs = vec![pack(1, 0.2, 5), pack(9, 0.0, 120), pack(2, 0.4, 5)];
        assert_eq!(cut(packs.clone(), 100), vec![1, 2]);
        assert_eq!(cut(packs, 200), Vec::<u64>::new());
        // a second own vehicle past the limit still plays
        let packs = vec![pack(1, 0.0, 300), pack(2, 0.0, 10)];
        assert_eq!(cut(packs, 100), Vec::<u64>::new());
    }

    #[test]
    fn a_negative_limit_cuts_all_but_the_user_s_vehicles() {
        let packs = vec![pack(1, 0.0, 5), pack(2, 0.1, 5)];
        assert_eq!(cut(packs, -1), vec![2]);
    }

    #[test]
    fn packs_with_the_same_key_keep_the_order_they_came_in() {
        let packs = vec![pack(1, 1.0, 4), pack(2, 1.0, 4), pack(3, 1.0, 4)];
        assert_eq!(cut(packs, 3), vec![2, 3]);
        let packs = vec![pack(3, 1.0, 4), pack(2, 1.0, 4), pack(1, 1.0, 4)];
        assert_eq!(cut(packs, 3), vec![1, 2]);
    }

    #[test]
    fn the_cut_holds_for_the_next_frame_and_a_frame_without_packs_keeps_it() {
        let mut p = Packs::default();
        assert!(p.heard(pack(1, 0.5, 10)));
        assert!(p.heard(pack(2, 1.0, 10)));
        // updated twice: one pack, where it was last
        assert!(p.heard(pack(2, 2.0, 10)));
        p.end_frame(5);
        assert!(p.is_too_far(2) && !p.is_too_far(1));
        p.end_frame(5);
        assert!(p.is_too_far(2), "the listener set twice in a frame");
        assert!(p.heard(pack(1, 3.0, 10)));
        assert!(!p.heard(pack(2, 0.1, 10)), "as sorted last frame");
        p.end_frame(5);
        assert!(
            p.is_too_far(1) && !p.is_too_far(2),
            "now the other way round"
        );
    }

    /// The traffic's sounds are updated in the frames the simulation ticks, the scenery's in
    /// every frame, the listener set in every frame: a far car stays cut all along rather
    /// than start again in each frame that follows one without the traffic.
    #[test]
    fn a_pack_stays_cut_through_frames_that_do_not_update_the_others() {
        let mut p = Packs::default();
        let (near, far, scenery) = (pack(1, 0.5, 10), pack(2, 2.0, 10), pack(3, 0.1, 1));
        p.heard(scenery);
        p.heard(near);
        p.heard(far);
        p.end_frame(5);
        for tick in 0..40 {
            // a frame between two ticks
            p.heard(scenery);
            p.end_frame(5);
            // and the next with the traffic
            p.heard(scenery);
            assert!(p.heard(near), "tick {tick}");
            assert!(!p.heard(far), "tick {tick}");
            p.end_frame(5);
        }
        // a pack no longer updated is forgotten, and its entries no longer count
        for _ in 0..=KEEP_FRAMES {
            p.heard(scenery);
            p.heard(far);
            p.end_frame(5);
        }
        assert!(p.heard(far));
    }

    #[test]
    fn a_pack_made_since_the_sort_is_placed_among_the_sorted_ones() {
        let mut p = Packs::default();
        assert!(p.heard(pack(1, 1.0, 10)), "nothing sorted yet");
        p.heard(pack(2, 2.0, 10));
        p.end_frame(5);
        // 10 entries nearer than 1.5: too far; nearer than the first: heard
        assert!(!p.heard(pack(7, 1.5, 3)));
        assert!(p.heard(pack(8, 0.5, 3)));
        assert!(p.heard(pack(9, 0.0, 3)), "the user's vehicle");
        assert!(!p.is_too_far(7), "not sorted yet");
    }

    #[test]
    fn a_pack_reaches_as_far_as_its_widest_sound_heard_from_outside() {
        let e = |pos: bool, range: f32, viewpoint: i32| SoundEntry {
            pos: pos.then_some([0.0; 3]),
            range,
            viewpoint,
            ..Default::default()
        };
        assert_eq!(pack_range(&[e(true, 20.0, 0), e(true, 35.0, 1)]), 35.0);
        // the cab's own (viewpoint 2) and one without [3d] do not count
        assert_eq!(pack_range(&[e(true, 20.0, 0), e(true, 90.0, 2)]), 20.0);
        assert_eq!(pack_range(&[e(false, 90.0, 0)]), 10.0);
        assert_eq!(pack_range(&[e(true, 50.0, 4)]), 50.0);
        assert_eq!(pack_range(&[]), 10.0);
    }
}
