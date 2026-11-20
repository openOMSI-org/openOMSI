//! Depot files that keep two recordings per stop: the "next stop is …" one that is heard
//! when the bus leaves a stop, and the "… arrived" one. OMSI 2's IBIS scripts step their stop
//! counter on by themselves only on leaving a stop, so the second recording is never played
//! unless the key at the stop is pressed - the game does that (`omsi_app::auto_announce`).
//! This module says which entries of a depot file's `[infosystem_busstop_list]` are one
//! stop's two recordings, and where its first "next stop is …" one stands.

use omsi_vehicle::Hof;
use omsi_vehicle::hof::{ident_names, stop_words};

/// A depot file that keeps two recordings per stop: where the list's first "next stop is …"
/// entry stands, and how many of the list's entries are pairs at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TwoRecordings {
    /// The list's first "next stop is …" entry (0-based).
    pub first: usize,
    /// Neighbouring entries that are one stop's two recordings (of `list.len() - 1`).
    pub pairs: usize,
}

/// An ident without what only tells two recordings of one stop apart: the line number in
/// front of it (`102-FuNiuLuLongHaiLu`, as GreenCity writes them) and the digits at its end
/// that number them (`Anzhenli1`, `Anzhenli2`), in lower case.
fn stem(ident: &str) -> String {
    let lower = ident.trim().to_lowercase();
    let body = match lower.split_once('-') {
        Some((line, rest)) if !line.is_empty() && line.bytes().all(|b| b.is_ascii_digit()) => rest,
        _ => lower.as_str(),
    };
    body.trim_end_matches(|c: char| c.is_ascii_digit())
        .to_string()
}

/// Whether `b` is the second recording of the stop whose "next stop is …" recording is `a`.
/// A depot file that keeps one recording per stop has none of these, and a stop's name that
/// is all digits (`1041`, a line's start recording) pairs with nothing.
pub fn is_pair(a: &str, b: &str) -> bool {
    if a.trim() == b.trim() {
        return false;
    }
    let (a, b) = (stem(a), stem(b));
    a == b && a.chars().count() >= 3
}

/// Where a list's first "next stop is …" entry stands: the first pair that does not itself
/// follow the second half of another. The start of such a list holds the terminus's own
/// recording, the line's start recording (`welcome…`, `1041`) or a placeholder a map author
/// left, and one or two entries before the first pair.
pub fn first_preview(list: &[String]) -> Option<usize> {
    (1..list.len().saturating_sub(1))
        .find(|&j| is_pair(&list[j], &list[j + 1]) && !is_pair(&list[j - 1], &list[j]))
}

/// The two-recordings layout of `route`, when it has one: at least three pairs, and its first
/// one near the start. A depot file that keeps one recording per stop has no pair at all
/// (Spandau, Grundorf and the other German maps), and one whose start holds more than two
/// entries before its first pair is not built the way the Chinese maps are.
pub fn layout(hof: &Hof, route: usize) -> Option<TwoRecordings> {
    let list = hof.info_busstop_lists.get(route)?;
    let pairs = list.windows(2).filter(|w| is_pair(&w[0], &w[1])).count();
    let first = first_preview(list)?;
    (pairs >= 3 && first <= 4).then_some(TwoRecordings { first, pairs })
}

/// The entry that holds the arrival recording of the `k`th stop of a trip (0-based; the first
/// stop is the one the bus pulls out of, and has none).
///
/// It is found by where the trip stands, not by the stop's name: a depot file keeps the stops
/// in the order the trip runs them, two entries each, so the `k`th stop's arrival half is the
/// `2k - 1`th entry after the list's first "next stop is …" one. That is what the driver's key
/// does - one press a stop, the counter stepping on by itself in between - and it holds
/// whatever the map calls its stops, which the depot file often spells otherwise: Guangfo's
/// 996 timetable has `shuxiuluzhong` for the depot file's `shuixiuluzhong` and
/// `fcchegaunsuo` for `fccheguansuo`, its 812 has `Dongjiao Huainingfang` for `dongjiaohnf`.
/// None of those stop the press, and no map has to be edited for them.
///
/// The name decides where the two disagree: a stop list that does not keep every stop of the
/// trip (or keeps one the trip has not) stands behind the count from there on, and the name
/// then finds the stop again. Where a trip runs back along its own way, both halves are in the
/// list, and the count says which of them the bus is at.
pub fn arrival_entry(hof: &Hof, route: usize, name: &str, k: usize) -> Option<usize> {
    let list = hof.info_busstop_lists.get(route)?;
    let first = first_preview(list)?;
    let by_count = (k > 0)
        .then(|| first + 2 * k - 1)
        .filter(|e| *e > 0 && *e < list.len());
    // (the count is where the bus's own step puts the stop: the name looks for the stop
    // nearest to it, so that a trip back along its own way finds the second half)
    let near = by_count.unwrap_or(first);
    let by_name = entry_of_name(hof, list, name, near);
    let by_count = by_count.filter(|e| is_pair(&list[*e - 1], &list[*e]));
    by_name.or(by_count)
}

/// The entry that goes by the stop's `name`, the one nearest `near` where the list holds the
/// stop twice: its arrival half, or the "next stop is …" half where that is the one that pairs
/// with the entry after it.
fn entry_of_name(hof: &Hof, list: &[String], name: &str, near: usize) -> Option<usize> {
    let flat_name = flat(name);
    let words_name = stop_words(name);
    let mut best: Option<(u8, usize, usize)> = None;
    for (i, ident) in list.iter().enumerate() {
        let names = ident_names(hof, ident);
        let Some(score) = names
            .iter()
            .filter_map(|(raw, words)| name_score(raw, words, &flat_name, &words_name))
            .max()
        else {
            continue;
        };
        let entry = if i > 0 && is_pair(&list[i - 1], &list[i]) {
            i
        } else if i + 1 < list.len() && is_pair(&list[i], &list[i + 1]) {
            i + 1
        } else {
            continue;
        };
        // (a better fit first, then the nearer entry: `MAX - distance` sorts that way down)
        let key = (score, usize::MAX - i.abs_diff(near), entry);
        if best.is_none_or(|b| key > b) {
            best = Some(key);
        }
    }
    best.map(|(_, _, entry)| entry)
}

/// How well a name of the depot file fits the stop: 3 spelt the same, 2 the same words, 1 one
/// the start of the other, 0 one letter apart (the maps type `shuxiuluzhong` for
/// `shuixiuluzhong`). None where it is no name of that stop at all.
fn name_score(raw: &str, words: &[String], flat_name: &str, words_name: &[String]) -> Option<u8> {
    let flat_raw = flat(raw);
    if flat_raw.is_empty() || flat_name.is_empty() {
        return None;
    }
    if flat_raw == flat_name {
        return Some(3);
    }
    if !words_name.is_empty() && words == words_name {
        return Some(2);
    }
    let (a, b) = (flat_raw.chars().count(), flat_name.chars().count());
    if a >= 4 && b >= 4 && (flat_raw.starts_with(flat_name) || flat_name.starts_with(&flat_raw)) {
        return Some(1);
    }
    (a >= 6 && b >= 6 && one_letter_apart(&flat_raw, flat_name)).then_some(0)
}

/// A name as its letters only, lowercased: `Shuixiulu Zhong.` is `shuixiuluzhong`, which is
/// how the depot files spell it.
fn flat(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Whether two names of the same length, or one letter apart, differ by one letter at most -
/// a letter left out, doubled or put in another place.
fn one_letter_apart(a: &str, b: &str) -> bool {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a.len().abs_diff(b.len()) > 1 {
        return false;
    }
    let (mut i, mut j, mut wrong) = (0, 0, 0);
    while i < a.len() && j < b.len() {
        if a[i] == b[j] {
            i += 1;
            j += 1;
            continue;
        }
        wrong += 1;
        if wrong > 1 {
            return false;
        }
        match a.len().cmp(&b.len()) {
            std::cmp::Ordering::Greater => i += 1,
            std::cmp::Ordering::Less => j += 1,
            std::cmp::Ordering::Equal => {
                i += 1;
                j += 1;
            }
        }
    }
    wrong + (a.len() - i) + (b.len() - j) <= 1
}

/// The keys a vehicle offers the game to step its IBIS on with (#3281): `IBIS_vor` plays the
/// next recording and steps the counter on, `IBIS_vor_stumm` steps it over silently.
///
/// Most vehicle folders never mount a script that defines either - out of 382 folders in a
/// common install, 18 mount one - so on most buses there is nothing the game can press, and
/// the automatic announcements stay the bus's own. That is worth saying to the player once,
/// which is what [`quiet`] spells out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keys {
    /// Both keys: a press plays, and an entry it would pass over is stepped over silently.
    Both,
    /// Only `IBIS_vor`: a press plays whatever entry it lands on, so one is made only where it
    /// lands on the recording wanted.
    Play,
    /// Only `IBIS_vor_stumm`: entries can be passed over, but nothing is played by it.
    Step,
    /// Neither: an ibox or an Atron unit, whose announcements are its own.
    Neither,
}

/// The keys a vehicle has, from whether each of the two exists on it.
pub fn keys(vor: bool, stumm: bool) -> Keys {
    match (vor, stumm) {
        (true, true) => Keys::Both,
        (true, false) => Keys::Play,
        (false, true) => Keys::Step,
        (false, false) => Keys::Neither,
    }
}

/// Whether the game can announce a depot file of `pairs` pairs of recordings on this bus: it
/// needs a way into the counter and a second recording per stop to miss.
pub fn usable(keys: Keys, pairs: usize) -> bool {
    pairs > 0 && keys != Keys::Neither
}

/// Why the game leaves the announcements to the bus, in one line for the player (#3281) -
/// None where it does not.
pub fn quiet(keys: Keys, pairs: usize) -> Option<&'static str> {
    if usable(keys, pairs) {
        return None;
    }
    Some(if pairs == 0 {
        "Automatic announcements: this route's HOF keeps one recording per stop, so there is nothing to press for."
    } else if keys == Keys::Neither {
        "Automatic announcements: this bus has no IBIS \"next stop\" key, so its announcements stay its own."
    } else {
        "Automatic announcements are off."
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(idents: &[&str]) -> Vec<String> {
        idents.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_two_recordings_of_a_stop_pair_up() {
        assert!(is_pair("Anzhenli1", "Anzhenli2"));
        assert!(is_pair("fangcundadaoxi", "fangcundadaoxi1"));
        assert!(is_pair("102-FuNiuLuLongHaiLu", "102-FuNiuLuLongHaiLu1"));
        // (two stops of a line, and a line's start recording: nothing to pair)
        assert!(!is_pair("Anzhenli1", "AnwaiGanshuiqiao1"));
        assert!(!is_pair("1041", "1042"));
        assert!(!is_pair("505 HY", "505 HY"));
    }

    #[test]
    fn the_first_next_stop_follows_the_start_recording() {
        // (the terminus' own recording, then the line's start recording, then the pairs)
        let beijing = list(&[
            "Wuluju",
            "1041",
            "Anzhenli1",
            "Anzhenli2",
            "Anwai1",
            "Anwai2",
        ]);
        assert_eq!(first_preview(&beijing), Some(2));
        // (a map that also repeats the terminus' arrival before the first pair)
        let zhengzhou = list(&[
            "Zhan", "Zhan1", "Zhan2", "Xilu", "Xilu1", "Donglu", "Donglu1",
        ]);
        assert_eq!(first_preview(&zhengzhou), Some(3));
        // (one recording per stop)
        assert_eq!(
            first_preview(&list(&["Alpha", "Beta", "Gamma", "Delta"])),
            None
        );
    }

    #[test]
    fn a_depot_file_of_one_recording_per_stop_has_no_layout() {
        let mut hof = Hof::default();
        hof.info_busstop_lists = vec![list(&[
            "Alpha", "Beta", "Gamma", "Delta", "Epsilon", "Zeta",
        ])];
        assert_eq!(layout(&hof, 0), None);
        hof.info_busstop_lists = vec![list(&[
            "Start",
            "welcome19",
            "FRoad",
            "FRoad1",
            "QRoad",
            "QRoad1",
            "DRoad",
            "DRoad1",
        ])];
        assert_eq!(layout(&hof, 0), Some(TwoRecordings { first: 2, pairs: 3 }));
        // (a route the depot file has no list for)
        assert_eq!(layout(&hof, 1), None);
    }

    #[test]
    fn the_arrival_half_is_counted_from_the_first_next_stop() {
        // (Guangfo's 996: the line's start recording, then a pair a stop)
        let mut hof = Hof::default();
        hof.info_busstop_lists = vec![list(&[
            "terminus",
            "welcome99600",
            "shuixiuluzhong",
            "shuixiuluzhong1",
            "zengnanlu",
            "zengnanlu1",
            "fccheguansuo",
            "fccheguansuo1",
        ])];
        // (that map's timetable spells the first one `shuxiuluzhong`, and the third
        // `fcchegaunsuo`: the bus still stands at a stop of the trip, and the press is the
        // same one, so a name of its own is not needed)
        assert_eq!(arrival_entry(&hof, 0, "shuxiuluzhong", 1), Some(3));
        assert_eq!(arrival_entry(&hof, 0, "zengnanlu", 2), Some(5));
        assert_eq!(arrival_entry(&hof, 0, "fcchegaunsuo", 3), Some(7));
        // (the first stop is the one the bus pulls out of: no arrival half for it)
        assert_eq!(arrival_entry(&hof, 0, "terminus", 0), None);
    }

    #[test]
    fn the_name_finds_the_stop_where_the_list_keeps_one_the_trip_has_not() {
        // (a stop list with an entry the timetable does not have: counting stands a stop
        // behind from there on, and the name is what puts it right)
        let mut hof = Hof::default();
        hof.info_busstop_lists = vec![list(&[
            "startroad",
            "welcome19",
            "alpharoad",
            "alpharoad1",
            "skippedroad",
            "skippedroad1",
            "charlieroad",
            "charlieroad1",
            "deltaroad",
            "deltaroad1",
        ])];
        // (the third stop of the trip is `deltaroad`, which the count would put on
        // `charlieroad1` (entry 7); the stop after `deltaroad` stands at entry 9)
        assert_eq!(arrival_entry(&hof, 0, "deltaroad", 2), Some(9));
    }

    /// #3281: most buses mount no script with the IBIS keys, and the game says so once instead
    /// of staying quiet for a whole duty.
    #[test]
    fn a_bus_with_no_ibis_keys_says_so_instead_of_staying_quiet() {
        assert_eq!(keys(true, true), Keys::Both);
        assert_eq!(keys(true, false), Keys::Play);
        assert_eq!(keys(false, true), Keys::Step);
        assert_eq!(keys(false, false), Keys::Neither);
        assert!(usable(Keys::Both, 22) && usable(Keys::Play, 1) && usable(Keys::Step, 1));
        assert!(!usable(Keys::Neither, 22), "an ibox announces on its own");
        assert!(
            !usable(Keys::Both, 0),
            "one recording per stop: nothing to miss"
        );
        assert!(quiet(Keys::Both, 22).is_none(), "works: say nothing");
        assert!(quiet(Keys::Neither, 22).unwrap().contains("no IBIS"));
        assert!(
            quiet(Keys::Both, 0)
                .unwrap()
                .contains("one recording per stop")
        );
    }
}
