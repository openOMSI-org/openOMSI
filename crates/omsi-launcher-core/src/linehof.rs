//! The depot-file side of the player's lines (`lines`): what the destination displays and the
//! IBIS need. A bus finds its trip's destination in its depot file (`.hof`): an
//! `[addterminus]` gives the matrix texts, an `[infosystem_trip]` with the code
//! line × 100 + nn the route the IBIS types, its `[infosystem_busstop_list]` the stops (each
//! the ident of an `[addbusstop]`). A line to a destination the depot file lacks, or a line
//! the file does not know at all, needs entries of its own there - without them the game
//! finds no terminus (the matrix stays blank) and no route (the IBIS cannot be typed).
//!
//! The entries go into a block at the end of the file between two comment lines naming the
//! map (`markers`), written anew on every save from the registry, so that nothing of the
//! file's own is changed and a deleted line leaves nothing behind. Every vehicle folder that
//! has the depot file gets it: a copy in openOMSI's content folder (the game reads a folder's
//! copy there before the installation's, `omsi_vehicle::hof::depot_files`), or the content
//! folder's file itself - never the OMSI 2 folder's. The file keeps its code page and its
//! line ends.

use crate::lines::{Direction, LineDesign, Registry};
use omsi_cfg::codepage::CodePage;
use omsi_vehicle::hof::{BusStop, Terminus};
use omsi_vehicle::Hof;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

// --- the block -------------------------------------------------------------------------------

/// The comment lines around what the line editor of map `map` adds to a depot file (one
/// block per map: two maps may use one depot file).
pub fn markers(map: &str) -> (String, String) {
    let map = map.trim();
    (format!("--- openOMSI line editor, map {map}: begin (written again on every save) ---"), format!("--- openOMSI line editor, map {map}: end ---"))
}

/// `text` with the block of map `map` taken out, and `block` (`\r\n` line ends) put at its end
/// when it is not empty, its lines ending as the file's do. Everything else stays as it was.
pub fn put_block(text: &str, map: &str, block: &str) -> String {
    let (begin, end) = markers(map);
    let nl = if text.contains("\r\n") || !text.contains('\n') { "\r\n" } else { "\n" };
    let mut out = String::with_capacity(text.len() + block.len() + 200);
    let mut inside = false;
    for line in text.split_inclusive('\n') {
        let t = line.trim();
        if !inside && t == begin {
            inside = true;
            // (the empty line put before it)
            let twice = format!("{nl}{nl}");
            if out.ends_with(&twice) {
                out.truncate(out.len() - nl.len());
            }
            continue;
        }
        if inside {
            if t == end {
                inside = false;
            }
            continue;
        }
        out.push_str(line);
    }
    if !block.trim().is_empty() {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push_str(nl);
        }
        let body = if nl == "\n" { block.replace("\r\n", "\n") } else { block.to_string() };
        out.push_str(&format!("{nl}{begin}{nl}{nl}{body}{end}{nl}"));
    }
    out
}

/// `text` without the block of map `map`.
pub fn strip_block(text: &str, map: &str) -> String {
    put_block(text, map, "")
}

/// How a depot file is written: its code page (a UTF-16 one with its byte order mark).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Coding {
    Utf16,
    Page(CodePage),
}

/// A depot file's text, and how it was written (`omsi_cfg::decode_text`'s rules).
pub fn decode(bytes: &[u8]) -> (String, Coding) {
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        let units: Vec<u16> = bytes[2..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        return (String::from_utf16_lossy(&units), Coding::Utf16);
    }
    // (a UTF-8 byte order mark stays in the text, and so is written back)
    let page = omsi_cfg::codepage::detect(bytes);
    (page.encoding().decode_without_bom_handling(bytes).0.into_owned(), Coding::Page(page))
}

/// `text` written as `coding`; a character the code page lacks becomes `?`.
pub fn encode(text: &str, coding: Coding) -> Vec<u8> {
    match coding {
        Coding::Utf16 => {
            let mut b = vec![0xFF, 0xFE];
            for u in text.encode_utf16() {
                b.extend_from_slice(&u.to_le_bytes());
            }
            b
        }
        Coding::Page(page) => {
            let (b, _, lossy) = page.encoding().encode(text);
            if !lossy {
                return b.into_owned();
            }
            let mut out = Vec::with_capacity(text.len());
            let mut buf = [0u8; 4];
            for c in text.chars() {
                let (b, _, bad) = page.encoding().encode(c.encode_utf8(&mut buf));
                if bad {
                    out.push(b'?');
                } else {
                    out.extend_from_slice(&b);
                }
            }
            out
        }
    }
}

// --- what the strings are for ----------------------------------------------------------------

/// What a string of a depot file's termini is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    IbisDisplay,
    FrontTop,
    FrontBottom,
    Side,
    RollerBlind,
    ClearName,
    ExtraSign,
    Bitmap,
    /// Any text in it makes the trip a special one (the matrix script hides the line number).
    Special,
    /// The small sign at the back.
    Rear,
    /// Read by some buses' signs only, which show string `n` when it is empty.
    Fallback(u8),
    Other,
}

/// The layout of OMSI's own depot files (Grundorf, Spandau), taken for a file that has no
/// notes of its own but looks like one.
const USUAL: [Role; 8] = [Role::IbisDisplay, Role::FrontTop, Role::FrontBottom, Role::Side, Role::RollerBlind, Role::ClearName, Role::ExtraSign, Role::Bitmap];

/// OMSI's layout for a file of `n` strings: those after the eighth for nothing it names.
fn usual_roles(n: usize) -> Vec<Role> {
    (0..n).map(|k| USUAL.get(k).copied().unwrap_or(Role::Other)).collect()
}

/// The string a note says is shown when this one is empty ("Wenn diese Zeile leer ist, wird
/// String 1 ausgelesen"), if it says so.
fn fallback_of(l: &str) -> Option<u8> {
    // (a note on centring with spaces is no fallback)
    let l = l.replace("leerzeichen", "");
    if !["leer", "empty", "ausgelesen", "falls back"].iter().any(|w| l.contains(w)) {
        return None;
    }
    l.match_indices("string").find_map(|(at, w)| {
        let digits: String = l[at + w.len()..].trim_start().chars().take_while(char::is_ascii_digit).collect();
        digits.parse::<u8>().ok()
    })
}

impl Role {
    /// The role a depot file's note on a string describes (the stock files' German ones, and
    /// English).
    pub fn from_note(note: &str) -> Role {
        let l = note.to_lowercase();
        let has = |w: &[&str]| w.iter().any(|x| l.contains(x));
        if has(&["sonderziel"]) || (l.contains("special") && !l.contains("special char")) {
            Role::Special
        } else if let Some(n) = fallback_of(&l) {
            Role::Fallback(n)
        } else if has(&["code", "kürzel", "kuerzel"]) {
            // (before the IBIS: "IBIS-Code für Gegenroute" is no display)
            Role::Other
        } else if has(&["ibis2", "ibis 2", "klarname", "clear name"]) {
            Role::ClearName
        } else if has(&["krüger", "kruger", "krueger", "bitmap", ".bmp", "bildanzeige", "picture"]) {
            Role::Bitmap
        } else if has(&["steckschild", "zusätzliches schild", "zusaetzliches schild", "additional sign", "extra sign"]) {
            Role::ExtraSign
        } else if has(&["heckanzeige", "heckschild", "rear"]) || l.trim_start().starts_with("heck") {
            Role::Rear
        } else if has(&["ibis"]) {
            Role::IbisDisplay
        } else if has(&["rollband", "roller", "fallblatt", "textur", "texture", ".tga"]) {
            Role::RollerBlind
        } else if has(&["seite", "side"]) {
            Role::Side
        } else if has(&["2. zeile", "zeile 2", "line 2", "2nd line", "bottom"]) {
            Role::FrontBottom
        } else if has(&["1. zeile", "zeile 1", "line 1", "front", "zielschild", "matrix", "annax", "ziel", "destination"]) {
            Role::FrontTop
        } else {
            Role::Other
        }
    }
}

/// The notes a depot file has on its terminus strings (`string0: IBIS-Display ...`, in the
/// comments before its first terminus), "" where it has none.
pub fn notes(text: &str, count: usize) -> Vec<String> {
    let mut out = vec![String::new(); count];
    for line in text.lines() {
        let t = line.trim();
        let low = t.to_ascii_lowercase();
        if low.starts_with("[addterminus") {
            break;
        }
        let Some(rest) = low.strip_prefix("string") else { continue };
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        let Ok(k) = digits.parse::<usize>() else { continue };
        let after = rest[digits.len()..].trim_start();
        if !after.starts_with(':') {
            continue;
        }
        // (as written: `low` is `t` with only ASCII letters lowered, the same length)
        let note = t[t.len() - after.len() + 1..].trim();
        if k < count && out[k].is_empty() && !note.is_empty() {
            out[k] = note.to_string();
        }
    }
    out
}

/// The most characters a note allows ("max. 16 Zeichen").
pub fn note_max(note: &str) -> Option<usize> {
    let l = note.to_lowercase();
    let at = l.find("max")?;
    let digits: String = l[at + 3..].chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit()).collect();
    digits.parse::<usize>().ok().filter(|n| (4..=60).contains(n))
}

/// The roles of a depot file's strings: from its notes, else OMSI's usual layout when the file
/// looks like it, else its first text the front.
pub fn roles(notes: &[String], cols: &[Column]) -> Vec<Role> {
    if notes.iter().any(|x| !x.is_empty()) {
        return notes.iter().map(|x| if x.is_empty() { Role::Other } else { Role::from_note(x) }).collect();
    }
    let text = |k: usize| matches!(cols.get(k), Some(Column::Text { .. }));
    // OMSI's layout is what nearly every depot file has, notes or not: a texture fifth
    // (Grundorf, Spandau), or that string left empty (the HOF Suite's files: Bad Hügelsdorf
    // 2020, Hohenkirchen) - or a small file without one, its second and third the front's
    // rows (the buses read the strings by their place)
    let usual = match cols.get(4) {
        Some(Column::File(_)) => true,
        Some(Column::Empty) | None => text(1) || text(2),
        Some(Column::Text { .. }) => false,
    };
    if usual {
        return usual_roles(notes.len());
    }
    // a layout of its own: its first text is the front, as `front` shows it
    let first = cols.iter().position(|c| matches!(c, Column::Text { .. }));
    (0..notes.len()).map(|k| if Some(k) == first { Role::FrontTop } else { Role::Other }).collect()
}

/// What a column of a depot file's strings holds, as its rows show it.
#[derive(Clone, Debug, PartialEq)]
pub enum Column {
    /// (Almost) always empty.
    Empty,
    /// A texture or picture: the file most rows share (a blank sign), else none.
    File(String),
    /// Text: in capitals or not, at most `max` characters, centred with spaces in front.
    Text { upper: bool, max: usize, centred: bool },
}

fn is_file(s: &str) -> bool {
    let l = s.trim().to_lowercase();
    [".tga", ".bmp", ".png", ".dds", ".jpg", ".jpeg"].iter().any(|e| l.ends_with(e))
}

/// The columns of `rows` (each record's strings), `count` of them; `notes` may give a column's
/// length. Without rows every column is text in capitals.
pub fn columns(rows: &[Vec<String>], count: usize, notes: &[String]) -> Vec<Column> {
    (0..count)
        .map(|k| {
            let noted = notes.get(k).and_then(|n| note_max(n));
            if rows.is_empty() {
                return Column::Text { upper: true, max: noted.unwrap_or(16), centred: false };
            }
            let vals: Vec<&str> = rows.iter().filter_map(|r| r.get(k)).map(|s| s.trim_end()).filter(|s| !s.trim().is_empty()).collect();
            if vals.is_empty() || vals.len() * 4 < rows.len() {
                return Column::Empty;
            }
            if vals.iter().filter(|s| is_file(s)).count() * 2 > vals.len() {
                let mut count: HashMap<String, usize> = HashMap::new();
                for v in vals.iter().filter(|s| is_file(s)) {
                    *count.entry(v.trim().to_string()).or_default() += 1;
                }
                let common = count.into_iter().filter(|(_, n)| *n >= 2).max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0))).map(|(f, _)| f).unwrap_or_default();
                return Column::File(common);
            }
            let alpha: Vec<&&str> = vals.iter().filter(|s| s.chars().any(char::is_alphabetic)).collect();
            let upper = alpha.iter().filter(|s| s.to_uppercase() == ***s).count() * 2 > alpha.len();
            // (a file of few termini shows less than a display takes: 16, as the IBIS and the Annax)
            let max = noted.unwrap_or_else(|| vals.iter().map(|s| s.chars().count()).max().unwrap_or(16).max(16));
            let centred = vals.iter().filter(|s| s.starts_with(' ')).count() * 2 > vals.len();
            Column::Text { upper, max, centred }
        })
        .collect()
}

// --- texts made from a name ------------------------------------------------------------------

/// `name` with the endings signs shorten shortened (Straße → Str., Bahnhof → Bhf., Platz →
/// Pl.), its words as they are otherwise.
fn abbreviated(name: &str) -> String {
    const ENDS: [(&str, &str); 6] = [("hauptbahnhof", "hbf."), ("bahnhof", "bhf."), ("straße", "str."), ("strasse", "str."), ("platz", "pl."), ("allee", "al.")];
    name.split(' ')
        .map(|w| {
            // (a comma after the word stays after its short form)
            let (w, comma) = match w.strip_suffix(',') {
                Some(x) => (x, ","),
                None => (w, ""),
            };
            let low = w.to_lowercase();
            let caps = w.chars().any(char::is_alphabetic) && w.to_uppercase() == w;
            for (end, short) in ENDS {
                if low.ends_with(end) && low.chars().count() == w.chars().count() {
                    let keep: String = w.chars().take(w.chars().count() - end.chars().count()).collect();
                    let short = if caps {
                        short.to_uppercase()
                    } else if keep.is_empty() {
                        // (a word of its own keeps its capital: "Bahnhof" → "Bhf.")
                        let mut c = short.chars();
                        c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
                    } else {
                        short.to_string()
                    };
                    return keep + &short + comma;
                }
            }
            format!("{w}{comma}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `name` in at most `max` characters. A name too long keeps what a sign shows of it: a place
/// and its stop ("Hohenkirchen, Flugplatz") without the comma, with the endings shortened
/// (Straße → Str., Bahnhof → Bhf.) where that is not enough, else the stop alone as the stock
/// files write it ("Therme"); only then is it cut.
pub fn shorten(name: &str, max: usize) -> String {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    let fits = |s: &str| s.chars().count() <= max;
    if fits(&name) {
        return name;
    }
    let short = abbreviated(&name);
    let Some(at) = name.rfind(", ") else {
        return if fits(&short) { short } else { short.chars().take(max).collect::<String>().trim_end().to_string() };
    };
    let joined = name.replace(", ", " ");
    if fits(&joined) {
        return joined;
    }
    let joined = short.replace(", ", " ");
    if fits(&joined) {
        return joined;
    }
    let stop = name[at + 2..].trim();
    if stop.chars().count() >= 3 {
        if fits(stop) {
            return stop.to_string();
        }
        let s = abbreviated(stop);
        if fits(&s) {
            return s;
        }
    }
    joined.chars().take(max).collect::<String>().trim_end().to_string()
}

/// `name` the way a column writes it: in capitals or not, shortened to its length.
pub fn styled(name: &str, upper: bool, max: usize) -> String {
    shorten(&if upper { name.to_uppercase() } else { name.to_string() }, max)
}

/// `s` centred in `max` characters with spaces in front.
fn centre(s: &str, max: usize) -> String {
    let pad = max.saturating_sub(s.chars().count()) / 2;
    format!("{}{s}", " ".repeat(pad))
}

/// A name on a two-line sign: on the first line when it fits, else split between its words.
fn two_lines(name: &str, upper: bool, top: usize, bottom: usize) -> (String, String) {
    let s = if upper { name.to_uppercase() } else { name.to_string() };
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() <= top {
        return (s, String::new());
    }
    let words: Vec<&str> = s.split(' ').collect();
    let mut k = 0;
    let mut len = 0;
    for (i, w) in words.iter().enumerate() {
        let add = w.chars().count() + usize::from(i > 0);
        if len + add > top {
            break;
        }
        len += add;
        k = i + 1;
    }
    let k = k.max(1);
    if k < words.len() {
        // (a place's comma goes where its name is split: "Hohenkirchen" over "Therme")
        let first = shorten(&words[..k].join(" "), top);
        return (first.trim_end_matches(',').trim_end().to_string(), shorten(&words[k..].join(" "), bottom));
    }
    (shorten(&s, top), String::new())
}

/// One line of a depot file: no line breaks, no keyword.
fn clean(s: &str) -> String {
    let s = s.replace(['\r', '\n'], " ");
    if s.trim_start().starts_with('[') {
        s.replacen('[', "(", 1)
    } else {
        s
    }
}

// --- the depot file as the line editor sees it -----------------------------------------------

/// A depot file without the line editor's blocks, and what its strings are for.
#[derive(Clone, Debug, Default)]
pub struct Depot {
    pub hof: Hof,
    /// The file's own notes on its terminus strings ("" where none).
    pub notes: Vec<String>,
    pub roles: Vec<Role>,
    pub cols: Vec<Column>,
    pub stop_cols: Vec<Column>,
    /// The codes the line editor's blocks of other maps give in it (two maps may use one depot
    /// file): kept free, never used - that map takes its entries out again.
    pub others: Taken,
    /// The file writes a name of one row on the front's second row (Spandau, Grundorf), not
    /// on the first (the HOF Suite's files): most of its termini do.
    pub one_row_bottom: bool,
}

impl Depot {
    /// The depot file `text` (read from `path`) as the line editor of map `map` sees it: its
    /// own entries, without the blocks of the line editor - the map's, written anew, and other
    /// maps' (only their codes, `others`).
    pub fn from_text(path: &Path, text: &str, map: &str) -> Depot {
        let (text, blocks) = crate::owndepot::split_blocks(text);
        let hof = Hof::parse(&omsi_cfg::CfgFile::from_str(path, &text));
        let notes = notes(&text, hof.string_count_terminus);
        let mut d = Self::from_hof(hof, notes);
        for (_, body) in blocks.iter().filter(|(m, _)| m.as_str() != map.trim()) {
            let block = format!("stringcount_terminus\r\n{}\r\nstringcount_busstop\r\n{}\r\n\r\n{body}", d.hof.string_count_terminus, d.hof.string_count_busstop);
            d.others.add(&Hof::parse(&omsi_cfg::CfgFile::from_str(path, &block)));
        }
        d
    }

    /// A depot file read already (`hof`), with its notes on its terminus strings.
    pub fn from_hof(hof: Hof, mut notes: Vec<String>) -> Depot {
        let n = hof.string_count_terminus;
        notes.resize(n, String::new());
        let rows: Vec<Vec<String>> = hof.termini.iter().map(|t| t.strings.clone()).collect();
        let mut cols = columns(&rows, n, &notes);
        // (a file with neither notes nor destinations - the player's own, all of them taken
        // out - shows nothing of its layout: OMSI's, as the buses read the strings by their
        // place; its columns, all text, would make its first the front alone)
        let roles = if rows.is_empty() && notes.iter().all(|x| x.is_empty()) { usual_roles(n) } else { roles(&notes, &cols) };
        let at = |role: Role| roles.iter().position(|r| *r == role);
        let mut one_row_bottom = false;
        if let (Some(top), Some(bottom)) = (at(Role::FrontTop), at(Role::FrontBottom)) {
            let filled = |r: &Vec<String>, k: usize| r.get(k).is_some_and(|s| !s.trim().is_empty());
            let top_only = rows.iter().filter(|r| filled(r, top) && !filled(r, bottom)).count();
            let bottom_only = rows.iter().filter(|r| !filled(r, top) && filled(r, bottom)).count();
            one_row_bottom = bottom_only > top_only;
            // (a young file - the player's own, of a destination or two - has no name of two
            // rows yet: a long one still splits over its second row)
            if matches!(cols.get(top), Some(Column::Text { .. })) && cols.get(bottom) == Some(&Column::Empty) {
                cols[bottom] = cols[top].clone();
            }
        }
        let stop_rows: Vec<Vec<String>> = hof.bus_stops.iter().map(|b| b.strings.clone()).collect();
        let stop_cols = columns(&stop_rows, hof.string_count_busstop, &[]);
        Depot { hof, notes, roles, cols, stop_cols, others: Taken::default(), one_row_bottom }
    }

    /// The depot file at `path` (through the content file system).
    pub fn load(path: &Path, map: &str) -> Option<Depot> {
        let b = omsi_cfg::vfs::read(path).ok()?;
        Some(Self::from_text(path, &decode(&b).0, map))
    }

    /// The file's own terminus a destination names (its ident, as the game looks for it).
    pub fn terminus(&self, name: &str) -> Option<&Terminus> {
        let n = name.trim();
        if n.is_empty() {
            return None;
        }
        self.hof.termini.iter().find(|t| t.texture_id.trim() == n).or_else(|| self.hof.termini.iter().find(|t| t.texture_id.trim().eq_ignore_ascii_case(n)))
    }

    /// The file has an IBIS route of the code `code` of its own (`[infosystem_trip]`).
    pub fn has_route(&self, code: u32) -> bool {
        code > 0 && self.hof.info_trips.iter().any(|t| t.code.trim().parse::<u32>().ok() == Some(code))
    }

    /// The file's own IBIS route through the stops `stops` (by name, in order: a route's stop
    /// is the ident of an `[addbusstop]`, matched by its part before `#`) - one of line `line`
    /// first (its code `line` × 100 and two digits). A direction of a line of the map's keeps
    /// it while its stops are the map's (`lines::Direction::map_route`).
    pub fn route_of(&self, line: &str, stops: &[&str]) -> Option<u32> {
        let same = |list: &Vec<String>| list.len() == stops.len() && list.iter().zip(stops).all(|(i, s)| i.split('#').next().unwrap_or("").trim().eq_ignore_ascii_case(s.trim()));
        let found: Vec<u32> = self.hof.info_trips.iter().enumerate().filter(|(k, _)| self.hof.info_busstop_lists.get(*k).is_some_and(same)).filter_map(|(_, t)| t.code.trim().parse().ok()).collect();
        let want = line_number(line);
        found.iter().copied().find(|c| want > 0 && c / 100 == want).or(found.first().copied())
    }

    /// The file's own stop called `ident`.
    pub fn own_stop(&self, ident: &str) -> Option<&BusStop> {
        let n = ident.trim();
        self.hof.bus_stops.iter().find(|b| b.ident.trim().eq_ignore_ascii_case(n))
    }

    /// The labels of the terminus strings: what each is for, or the file's note on it.
    pub fn labels(&self) -> Vec<(Role, String)> {
        self.roles.iter().enumerate().map(|(k, r)| (*r, if *r == Role::Other { self.notes.get(k).cloned().unwrap_or_default() } else { String::new() })).collect()
    }

    fn first(&self, role: Role) -> Option<usize> {
        self.roles.iter().position(|r| *r == role).filter(|k| matches!(self.cols.get(*k), Some(Column::Text { .. })))
    }

    /// The texts a new destination `name` gets: the front's rows split as they fit (a name of
    /// one row on the row the file writes it on), the IBIS's and the first side sign's texts
    /// in each column's way (capitals, length, centred), a texture column the blank sign most
    /// of the file's termini share - and every other text empty.
    pub fn sign_defaults(&self, name: &str) -> Vec<String> {
        let name = clean(name.trim());
        let (top, bottom, side) = (self.first(Role::FrontTop), self.first(Role::FrontBottom), self.first(Role::Side));
        let width = |k: Option<usize>| match k.and_then(|k| self.cols.get(k)) {
            Some(Column::Text { upper, max, .. }) => (*upper, *max),
            _ => (true, 16),
        };
        let (mut l1, mut l2) = match (top, bottom) {
            (Some(_), Some(_)) => two_lines(&name, width(top).0, width(top).1, width(bottom).1),
            (Some(_), None) => (styled(&name, width(top).0, width(top).1), String::new()),
            (None, Some(_)) => (String::new(), styled(&name, width(bottom).0, width(bottom).1)),
            (None, None) => (String::new(), String::new()),
        };
        // (the buses move a lone bottom row up: the file's habit decides where it is written)
        if l2.is_empty() && bottom.is_some() && self.one_row_bottom {
            (l1, l2) = (String::new(), styled(&name, width(bottom).0, width(bottom).1));
        }
        self.cols
            .iter()
            .enumerate()
            .map(|(k, c)| match c {
                Column::Empty => String::new(),
                Column::File(f) => f.clone(),
                Column::Text { upper, max, centred } => {
                    let s = match self.roles.get(k).copied().unwrap_or(Role::Other) {
                        Role::FrontTop if Some(k) == top => l1.clone(),
                        Role::FrontBottom if Some(k) == bottom => l2.clone(),
                        // (each a display of one row of its own)
                        Role::IbisDisplay | Role::ClearName => styled(&name, *upper, *max),
                        Role::Side if Some(k) == side => styled(&name, *upper, *max),
                        // another sign's rows (they fall back to these when empty), pictures,
                        // marks and what the file does not explain stay empty: a text copied
                        // into each of them showed on two rows at once
                        _ => return String::new(),
                    };
                    if *centred && !s.is_empty() {
                        centre(&s, *max)
                    } else {
                        s
                    }
                }
            })
            .collect()
    }

    /// The texts of direction `d`'s destination, and whether it is a new one: the file's own
    /// terminus's, else the direction's own (the default where one is empty).
    pub fn sign_of(&self, d: &Direction) -> (Vec<String>, bool) {
        let dest = d.destination();
        if let Some(t) = self.terminus(&dest) {
            return (t.strings.clone(), false);
        }
        let mut s = self.sign_defaults(&dest);
        for (k, own) in d.sign.iter().enumerate().take(s.len()) {
            if !own.is_empty() {
                s[k] = clean(own);
            }
        }
        (s, true)
    }

    /// What a front matrix shows of `strings`: its two lines (the IBIS display's text, or the
    /// first text, when the file names no front).
    pub fn front(&self, strings: &[String]) -> (String, String) {
        let get = |k: Option<usize>| k.and_then(|k| strings.get(k)).map(|s| s.trim().to_string()).unwrap_or_default();
        let (top, bottom) = (self.first(Role::FrontTop), self.first(Role::FrontBottom));
        if top.is_some() || bottom.is_some() {
            let (l1, l2) = (get(top), get(bottom));
            // (the buses move a lone bottom row up)
            return if l1.is_empty() { (l2, String::new()) } else { (l1, l2) };
        }
        let any = self.first(Role::IbisDisplay).or_else(|| self.cols.iter().position(|c| matches!(c, Column::Text { .. })));
        (get(any), String::new())
    }

    /// What the side sign (the first) and the IBIS show of `strings` - the IBIS's display, else
    /// its name as written; "" where the file has none.
    pub fn shown(&self, strings: &[String]) -> (String, String) {
        let get = |role: Role| self.first(role).and_then(|k| strings.get(k)).map(|s| s.trim().to_string()).unwrap_or_default();
        let ibis = get(Role::IbisDisplay);
        (get(Role::Side), if ibis.is_empty() { get(Role::ClearName) } else { ibis })
    }

    /// The file's own terminus `name` means when it is none of them by its ident: the same
    /// words in another order, case, commas and dots aside ("Hohenkirchen, Therme" for
    /// "Therme Hohenkirchen").
    pub fn similar(&self, name: &str) -> Option<&Terminus> {
        let words = |s: &str| {
            let mut w: Vec<String> = s.to_lowercase().replace([',', '.'], " ").split_whitespace().map(str::to_string).collect();
            w.sort();
            w
        };
        let key = words(name);
        if key.is_empty() || self.terminus(name).is_some() {
            return None;
        }
        self.hof.termini.iter().find(|t| !t.all_exit && words(&t.texture_id) == key)
    }

    /// What the IBIS shows for the stop `name` by default: the file's own name for it (its
    /// `[addbusstop]` of that ident), else the name the way the file writes its stops.
    pub fn stop_display(&self, name: &str) -> String {
        if let Some(s) = self.own_stop(name).and_then(|b| b.strings.first()).filter(|s| !s.trim().is_empty()) {
            return s.trim_end().to_string();
        }
        match self.stop_cols.first() {
            Some(Column::Text { upper, max, .. }) => styled(name, *upper, *max),
            _ => styled(name, true, 16),
        }
    }

    /// The strings of a new `[addbusstop]` for the stop `name` shown as `display`: that first,
    /// the name in the first and the last text column after it (a name and a clear name, as
    /// the stock files have them), the others empty.
    pub fn stop_strings(&self, name: &str, display: &str) -> Vec<String> {
        let n = self.hof.string_count_busstop;
        let text: Vec<usize> = (1..n).filter(|k| matches!(self.stop_cols.get(*k), Some(Column::Text { .. }))).collect();
        (0..n)
            .map(|k| {
                if k == 0 {
                    clean(display)
                } else if text.first() == Some(&k) || text.last() == Some(&k) {
                    match self.stop_cols.get(k) {
                        Some(Column::Text { upper, max, .. }) => styled(name, *upper, (*max).max(20)),
                        _ => name.to_string(),
                    }
                } else {
                    String::new()
                }
            })
            .collect()
    }
}

// --- the codes -------------------------------------------------------------------------------

/// The codes a depot file has (all its copies together).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Taken {
    pub termini: HashSet<i32>,
    pub routes: HashSet<u32>,
}

impl Taken {
    pub fn add(&mut self, h: &Hof) {
        self.termini.extend(h.termini.iter().map(|t| t.code));
        self.routes.extend(h.info_trips.iter().filter_map(|t| t.code.trim().parse::<u32>().ok()));
    }

    /// The codes of `d`: its own, and those other maps' blocks give in it (`Depot::others`).
    pub fn add_depot(&mut self, d: &Depot) {
        self.add(&d.hof);
        self.termini.extend(&d.others.termini);
        self.routes.extend(&d.others.routes);
    }
}

/// A line's number as the IBIS takes it: the digits after a letter in front ("X10" → 10), else
/// its leading digits; 0 without.
pub fn line_number(line: &str) -> u32 {
    let l = line.trim();
    let digits: String = match l.chars().next() {
        Some(c) if c.is_ascii_alphabetic() && l.len() > 1 && l[1..].chars().all(|c| c.is_ascii_digit()) => l[1..].to_string(),
        _ => l.chars().take_while(|c| c.is_ascii_digit()).collect(),
    };
    digits.parse::<u32>().ok().filter(|n| *n < 10_000).unwrap_or(0)
}

/// A route code of line `line` for direction `dir`: `wanted` when it is one of the line's
/// (line × 100 + 1 … 99) and free, else the first free from dir + 1 on, then from 1. None when
/// the line has none free.
pub fn route_code(line: &str, dir: usize, wanted: u32, taken: &HashSet<u32>) -> Option<u32> {
    let base = line_number(line) * 100;
    if wanted / 100 * 100 == base && (1..=99).contains(&(wanted % 100)) && !taken.contains(&wanted) {
        return Some(wanted);
    }
    let first = (dir as u32 + 1).min(99);
    (first..=99).chain(1..first).map(|n| base + n).find(|c| !taken.contains(c))
}

/// A free terminus code: `wanted` when free, else the first free of 901 - 999, 100 - 900, then
/// from 1002 (the IBIS types destination codes under 1000).
pub fn terminus_code(wanted: i32, taken: &HashSet<i32>) -> i32 {
    if wanted > 0 && !taken.contains(&wanted) {
        return wanted;
    }
    (901..=999).chain(100..=900).chain(1002..).find(|c| !taken.contains(c)).unwrap_or(1002)
}

/// The lines whose files are written (see `lines::problems`) and whose depot group uses the
/// depot file `depot`, in the registry's order - each as the map's timetable has it
/// (`LineDesign::in_timetable`: a change waiting for its day as the line runs until then).
fn lines_of<'a>(reg: &'a Registry, groups: &HashMap<String, String>, depot: &str) -> Vec<&'a LineDesign> {
    reg.lines
        .iter()
        .filter(|l| {
            let t = l.in_timetable();
            crate::lines::written(t) && depot_of(t, groups).is_some_and(|d| d.eq_ignore_ascii_case(depot))
        })
        .collect()
}

/// The depot file (its name) of a line's depot group.
pub fn depot_of(l: &LineDesign, groups: &HashMap<String, String>) -> Option<String> {
    groups.get(&l.ai_group.trim().to_lowercase()).filter(|h| !h.trim().is_empty()).cloned()
}

/// Give the directions of the lines of depot file `depot` their codes: a new destination its
/// terminus code (one for every direction to it), every direction its route code - the codes
/// kept where they are still free, so that they stay what the player knows them by.
pub fn assign(reg: &mut Registry, groups: &HashMap<String, String>, depot_name: &str, depot: &Depot, taken: &Taken) {
    let ids: Vec<u64> = lines_of(reg, groups, depot_name).iter().map(|l| l.id).collect();
    assign_lines(reg, &ids, std::slice::from_ref(depot), taken, &HashMap::new());
}

/// `assign` for the lines `ids` of the registry (as the map's timetable has them), whose depot
/// file is written into the copies `depots` (or one of the player's own, `owndepot`): a
/// destination is new when one of them lacks it. `shared`: the codes other lines give new
/// destinations of these (by the name lowercased), taken for them as well.
pub fn assign_lines(reg: &mut Registry, ids: &[u64], depots: &[Depot], taken: &Taken, shared: &HashMap<String, i32>) {
    let mut t = taken.clone();
    let mut new_termini: HashMap<String, i32> = shared.clone();
    for &id in ids {
        let Some(l) = reg.line_mut(id) else { continue };
        let l = l.in_timetable_mut();
        let number = l.number.clone();
        for (k, d) in l.directions.iter_mut().enumerate() {
            if d.stops.len() < 2 {
                continue;
            }
            let dest = d.destination();
            // (a copy of another bus pack may lack a destination the first one has)
            if depots.is_empty() || depots.iter().any(|x| x.terminus(&dest).is_none()) {
                let key = dest.to_lowercase();
                d.terminus_code = match new_termini.get(&key) {
                    Some(c) => *c,
                    None => {
                        let c = terminus_code(d.terminus_code, &t.termini);
                        t.termini.insert(c);
                        new_termini.insert(key, c);
                        c
                    }
                };
            }
            // (a direction of a line of the map's that calls at the map's stops: the depot
            // file's own route of it, as the driver knows it)
            if keeps_map_route(d, depots) {
                d.ibis_route = d.map_route;
                continue;
            }
            d.ibis_route = match route_code(&d.number_or(&number), k, d.ibis_route, &t.routes) {
                Some(c) => {
                    t.routes.insert(c);
                    c
                }
                None => 0,
            };
        }
    }
}

/// A direction of a line of the map's keeps the depot files' own IBIS route of it
/// (`Direction::map_route`): it calls at the map's stops, and every copy of the file has it.
fn keeps_map_route(d: &Direction, depots: &[Depot]) -> bool {
    d.map_route > 0 && d.has_map_stops() && !depots.is_empty() && depots.iter().all(|x| x.has_route(d.map_route))
}

/// The block of the lines `lines` (their codes given, `assign`) for one copy of their depot
/// file: a new destination's `[addterminus]` (once), the stops the file has no name for as
/// `[addbusstop]`, and for every direction its `[infosystem_trip]` and stop list - none for a
/// direction of a line of the map's that keeps the file's own route (`keeps_map_route`).
pub fn block(lines: &[&LineDesign], depot: &Depot) -> String {
    let mut o = String::new();
    // (every route to a new destination by the code its one `[addterminus]` has: two lines to
    // it may have been given two codes, one of them in a depot file of the player's own)
    let mut termini: HashMap<String, i32> = HashMap::new();
    let mut stops: HashMap<String, String> = HashMap::new();
    for l in lines {
        for d in &l.directions {
            if d.stops.len() < 2 || d.ibis_route == 0 {
                continue;
            }
            let number = clean(&d.number_or(&l.number));
            let dest = clean(&d.destination());
            let code = match depot.terminus(&dest) {
                Some(t) => t.code,
                None => match termini.get(&dest.to_lowercase()) {
                    Some(c) => *c,
                    None => {
                        termini.insert(dest.to_lowercase(), d.terminus_code);
                        let (strings, _) = depot.sign_of(d);
                        o.push_str(&format!("[addterminus]\r\n{}\r\n{dest}\r\n", d.terminus_code));
                        for s in strings {
                            o.push_str(&s);
                            o.push_str("\r\n");
                        }
                        o.push_str("\r\n");
                        d.terminus_code
                    }
                },
            };
            if d.ibis_route == d.map_route && keeps_map_route(d, std::slice::from_ref(depot)) {
                continue;
            }
            let mut idents = Vec::new();
            for s in &d.stops {
                let name = clean(s.name.trim());
                let display = if s.ibis.trim().is_empty() { depot.stop_display(&name) } else { clean(s.ibis.trim_end()) };
                idents.push(stop_ident(depot, &name, &display, &mut stops, &mut o));
            }
            let first = d.stops.first().map(|s| s.name.trim().to_uppercase()).unwrap_or_default();
            o.push_str(&format!("[infosystem_trip]\r\n{}\r\n{}\r\n{code}\r\n{number}\r\n\r\n", d.ibis_route, clean(&format!("{first}-{}", dest.to_uppercase()))));
            o.push_str(&format!("[infosystem_busstop_list]\r\n{}\r\n", idents.len()));
            for i in idents {
                o.push_str(&i);
                o.push_str("\r\n");
            }
            o.push_str("\r\n");
        }
    }
    o
}

/// The ident a route's list names the stop `name` by: the file's own stop of that name when it
/// shows `display`, else one of the block's (the name, else the name with `#oo` - the part
/// before `#` is what the stop is matched by), written once.
fn stop_ident(depot: &Depot, name: &str, display: &str, ours: &mut HashMap<String, String>, o: &mut String) -> String {
    if let Some(b) = depot.own_stop(name) {
        if b.strings.first().map(|s| s.trim_end()).unwrap_or(display) == display {
            return b.ident.trim_end().to_string();
        }
    }
    for k in 0.. {
        let ident = match k {
            0 => name.to_string(),
            1 => format!("{name}#oo"),
            _ => format!("{name}#oo{k}"),
        };
        match ours.get(&ident.to_lowercase()) {
            Some(shown) if shown == display => return ident,
            Some(_) => continue,
            None if depot.own_stop(&ident).is_some() => continue,
            None => {
                ours.insert(ident.to_lowercase(), display.to_string());
                o.push_str(&format!("[addbusstop]\r\n{ident}\r\n"));
                for s in depot.stop_strings(name, display) {
                    o.push_str(&s);
                    o.push_str("\r\n");
                }
                o.push_str("\r\n");
                return ident;
            }
        }
    }
    unreachable!()
}

// --- the files -------------------------------------------------------------------------------

/// A depot file of a vehicle folder: the one the game reads (`source`, the highest content
/// root's), the copies below it, and its place relative to a root (`Vehicles/...`, '/').
#[derive(Clone, Debug, PartialEq)]
pub struct Copy {
    pub rel: String,
    pub source: PathBuf,
    pub lower: Vec<PathBuf>,
}

/// Every vehicle folder's depot file called `name` (by its file name or its `[name]`), over the
/// content roots `bases` (highest first): in the folders of `Vehicles` and the folders in
/// those (a pack's buses read the pack's).
pub fn copies(name: &str, bases: &[PathBuf]) -> Vec<Copy> {
    let mut rels: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for b in bases {
        let v = b.join("Vehicles");
        for (n, is_dir) in omsi_cfg::vfs::list_dir(&v).unwrap_or_default() {
            if !is_dir {
                continue;
            }
            let n = n.to_string_lossy().into_owned();
            let rel = format!("Vehicles/{n}");
            if seen.insert(rel.to_lowercase()) {
                rels.push(rel);
            }
            for (m, sub) in omsi_cfg::vfs::list_dir(&v.join(&n)).unwrap_or_default() {
                let rel = format!("Vehicles/{n}/{}", m.to_string_lossy());
                if sub && seen.insert(rel.to_lowercase()) {
                    rels.push(rel);
                }
            }
        }
    }
    rels.sort_by_key(|r| r.to_lowercase());
    let mut out = Vec::new();
    for rel in rels {
        let mut files: Vec<(String, Vec<PathBuf>)> = Vec::new();
        for b in bases {
            for p in omsi_cfg::vfs::read_dir_paths(&b.join(&rel)) {
                if !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("hof")) {
                    continue;
                }
                let key = p.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
                match files.iter_mut().find(|f| f.0 == key) {
                    Some(f) => f.1.push(p),
                    None => files.push((key, vec![p])),
                }
            }
        }
        for (_, paths) in files {
            // (never the player's own depot files beside buses: `owndepot` writes those)
            if crate::depot::answers_to(&paths[0], name) && !omsi_vehicle::hof::is_players(&paths[0]) {
                let file = paths[0].file_name().unwrap_or_default().to_string_lossy().into_owned();
                out.push(Copy { rel: format!("{rel}/{file}"), source: paths[0].clone(), lower: paths[1..].to_vec() });
            }
        }
    }
    out
}

fn same_path(a: &Path, b: &Path) -> bool {
    let low = |x: &Path| x.components().map(|c| c.as_os_str().to_string_lossy().to_lowercase()).collect::<Vec<_>>();
    low(a) == low(b)
}

/// `block` (empty: none) written into `copy` as the content folder `content` has it: the file
/// itself when it lies there, else a copy of the file the game reads. A copy that is then
/// the file below it again is deleted. Never anything in the OMSI 2 folder `original`'s
/// vehicles. Returns whether a file changed.
pub fn write_copy(content: &Path, original: Option<&Path>, copy: &Copy, map: &str, block: &str) -> Result<bool, String> {
    let target = content.join(copy.rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    if original.is_some_and(|o| crate::depot::lies_in(&target, &o.join("Vehicles"))) || !target.starts_with(content) {
        return Err(format!("{} would be outside the content folder", target.display()));
    }
    let bytes = omsi_cfg::vfs::read(&copy.source).map_err(|e| format!("{}: {e}", copy.source.display()))?;
    let (text, coding) = decode(&bytes);
    let new = put_block(&text, map, block);
    let in_place = same_path(&copy.source, &target);
    if block.trim().is_empty() {
        if !in_place {
            return Ok(false);
        }
        // a copy that only held the block goes again
        if let Some(lower) = copy.lower.first() {
            if let Ok(lb) = omsi_cfg::vfs::read(lower) {
                if decode(&lb).0.trim_end() == new.trim_end() {
                    std::fs::remove_file(&target).map_err(|e| format!("{}: {e}", target.display()))?;
                    return Ok(true);
                }
            }
        }
    }
    if in_place && new == text {
        return Ok(false);
    }
    if let Some(d) = target.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    // (written beside it and then put in its place: a write cut off leaves the file whole)
    let part = PathBuf::from(format!("{}.openomsi-part", target.display()));
    std::fs::write(&part, encode(&new, coding)).map_err(|e| format!("{}: {e}", part.display()))?;
    std::fs::rename(&part, &target).map_err(|e| {
        let _ = std::fs::remove_file(&part);
        format!("{}: {e}", target.display())
    })?;
    Ok(true)
}

/// The depot file each depot group of the map names (`ailists.cfg`), by the group's name
/// lowercased.
pub fn group_depots(map_dir: &Path) -> HashMap<String, String> {
    omsi_map::ailists::ailists_with_chrono(map_dir, &[])
        .groups
        .iter()
        .filter(|g| g.is_depot && !g.name.trim().is_empty())
        .filter_map(|g| g.hof.as_ref().filter(|h| !h.trim().is_empty()).map(|h| (g.name.trim().to_lowercase(), h.trim().to_string())))
        .collect()
}

/// The depot files to write: each with its copies.
pub type Plan = Vec<(String, Vec<Copy>)>;

/// Before the registry is saved: the codes of its lines given (`assign`, against every copy of
/// their depot files), and the depot files to write - those of the lines and those written last
/// time (`Registry::depots`, whose blocks may have to go).
pub fn prepare(reg: &mut Registry, groups: &HashMap<String, String>, bases: &[PathBuf]) -> Plan {
    let mut names: Vec<String> = Vec::new();
    for l in reg.lines.iter().map(LineDesign::in_timetable).filter(|l| crate::lines::written(l)) {
        if let Some(d) = depot_of(l, groups) {
            if !names.iter().any(|n| n.eq_ignore_ascii_case(&d)) {
                names.push(d);
            }
        }
    }
    let mut plan: Plan = Vec::new();
    for name in &names {
        let found = copies(name, bases);
        let depots: Vec<Depot> = found.iter().filter_map(|c| Depot::load(&c.source, &reg.map)).collect();
        if !depots.is_empty() {
            let mut taken = Taken::default();
            for d in &depots {
                taken.add_depot(d);
            }
            let ids: Vec<u64> = lines_of(reg, groups, name).iter().map(|l| l.id).collect();
            assign_lines(reg, &ids, &depots, &taken, &HashMap::new());
        }
        plan.push((name.clone(), found));
    }
    let old = std::mem::replace(&mut reg.depots, names.clone());
    for name in old {
        if !names.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
            let found = copies(&name, bases);
            plan.push((name, found));
        }
    }
    plan
}

/// Write the blocks of `plan` (see `prepare`; the registry as saved) into the content folder
/// `content`. Returns how many files changed, and the first file that could not be written.
pub fn write(content: &Path, original: Option<&Path>, reg: &Registry, groups: &HashMap<String, String>, plan: &Plan) -> (usize, Option<String>) {
    let mut changed = 0;
    let mut error = None;
    for (name, found) in plan {
        let lines: Vec<&LineDesign> = lines_of(reg, groups, name).into_iter().map(LineDesign::in_timetable).collect();
        for c in found {
            let block = if lines.is_empty() {
                String::new()
            } else {
                match Depot::load(&c.source, &reg.map) {
                    Some(d) => block(&lines, &d),
                    None => continue,
                }
            };
            match write_copy(content, original, c, &reg.map, &block) {
                Ok(true) => changed += 1,
                Ok(false) => {}
                Err(e) => {
                    error.get_or_insert(e);
                }
            }
        }
    }
    (changed, error)
}

/// The first copy of the depot file `name` as the line editor of `map` sees it (for its page).
pub fn first_depot(name: &str, bases: &[PathBuf], map: &str) -> Option<Depot> {
    copies(name, bases).iter().find_map(|c| Depot::load(&c.source, map))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::lines::{Leg, StopRef};

    /// The head of Grundorf's depot file and three of its termini, a stop and a route, as the
    /// stock file has them (CR LF, Windows-1252).
    pub(crate) const GRUNDORF: &str = "[name]\r\nGrundorf\r\n\r\nstringcount_terminus\r\n8\r\n\r\n\
        \tstring0:\tIBIS-Display & Rollband-Textur\r\n\tstring1:\tAnnax (Front, 1. Zeile)\r\n\tstring2:\tAnnax (Front, 2. Zeile)\r\n\
        \tstring3:\tAnnax (SD Seite)\r\n\tstring4:\tRollbandtextur\r\n\tstring5:\tIBIS2-Display (Klarname in Groß-/Kleinschreibung), max 20 Zeichen\r\n\
        \tstring6:\tZusätzliches Schild\r\n\tstring7:\tKrüger-Bitmap\r\n\r\nstringcount_busstop\r\n4\r\n\r\n\
        \t[addterminus_{allexit}]\r\n\t{ident}\r\n\r\n\
        [addterminus_allexit]\r\n0\r\nEmpty\r\nLEERFELD\r\n\r\n\r\n\r\nBlanko.tga\r\nLeerfeld\r\n\r\n\r\n................\r\n\r\n\
        [addterminus]\r\n105\r\nKrankenhaus\r\n  KRANKENHAUS\r\n  EINSTEINDORF\r\n  KRANKENHAUS\r\n  KRANKENHAUS\r\nGru_Krankenhaus.tga\r\nE.-Dorf Krankenhaus\r\n\r\n\r\n................\r\n\r\n\
        [addterminus]\r\n107\r\nBauernhof\r\nBAUERNHOF\r\n   NORDSPITZE\r\n   BAUERNHOF\r\nNORDSP. BAUERNH.\r\nGru_Bauernhof.tga\r\nNordsp. Bauernhof\r\n\r\n\r\n................\r\n\r\n\
        [addterminus]\r\n113\r\nFlugplatz\r\nFLUGPLATZ\r\n   FLUGPLATZ\r\n    GRUNDORF\r\n   FLUGPLATZ\r\nGru_Flugplatz.tga\r\nFlugplatz Grundorf\r\n\r\n\r\n................\r\n\r\n\
        [addterminus]\r\n1001\r\nTutorial\r\n\r\n\r\n\r\n\r\nBlanko.tga\r\n\r\n2_Tutorial.bmp\r\n\r\n\
        [addbusstop]\r\nBauernhof\r\nNORDS. BAUERNHOF\r\nNordspitze\r\nBauernhof\r\nNordsp. Bauernhof\r\n................\r\n\r\n\
        [addbusstop]\r\nKrankenhaus\r\nE.DORF KRK.HAUS\r\nEinsteindorf\r\nKrankenhaus\r\nE.-dorf Krkhaus\r\n\r\n\
        [infosystem_trip]\r\n7601\r\nBAUERNHOF-KRANKENHAUS\r\n105\r\nTML\r\n\r\n\
        [infosystem_busstop_list]\r\n2\r\nBauernhof\r\nKrankenhaus\r\n";

    fn stop(id: i64, name: &str) -> StopRef {
        StopRef { id, name: name.into(), ..Default::default() }
    }

    fn ok_leg() -> Leg {
        Leg { length: 300.0, ok: true, steps: vec![Default::default()], ..Default::default() }
    }

    /// Line 42 of the registry: Bauernhof - Marktplatz Süd (a new destination) and back to
    /// Bauernhof (the file's own), by the depot group Busses.
    pub(crate) fn registry() -> Registry {
        let mut reg = Registry { map: "Grundorf".into(), ..Default::default() };
        let l = reg.add_line("Busses");
        l.number = "42".into();
        let out = Direction { terminus: "Marktplatz Süd".into(), stops: vec![stop(1, "Bauernhof"), stop(2, "Kirche"), stop(3, "Marktplatz Süd")], legs: vec![ok_leg(), ok_leg()], ..Default::default() };
        let back = Direction { stops: vec![stop(4, "Marktplatz Süd"), stop(5, "Kirche"), stop(6, "Bauernhof")], legs: vec![ok_leg(), ok_leg()], ..Default::default() };
        l.directions = vec![out, back];
        for d in &mut l.directions {
            d.refresh_times();
        }
        reg
    }

    pub(crate) fn groups() -> HashMap<String, String> {
        [("busses".to_string(), "Grundorf".to_string())].into_iter().collect()
    }

    /// A direction of a line of the map's that calls at the map's stops keeps the depot file's
    /// own IBIS route (the driver types what he knows); one with other stops gets one of its own.
    #[test]
    fn a_line_of_the_maps_keeps_its_own_ibis_route() {
        let d = Depot::from_text(Path::new("Grundorf.hof"), GRUNDORF, "Grundorf");
        assert_eq!(d.route_of("76", &["Bauernhof", "Krankenhaus"]), Some(7601));
        assert_eq!(d.route_of("76", &["Krankenhaus", "Bauernhof"]), None);
        assert!(d.has_route(7601) && !d.has_route(7602));
        let mut reg = Registry { map: "Grundorf".into(), ..Default::default() };
        let out = Direction { terminus: "Krankenhaus".into(), stops: vec![stop(11, "Bauernhof"), stop(12, "Krankenhaus")], legs: vec![Leg { map_way: Some([11, 12]), ..ok_leg() }], map_stops: vec![11, 12], map_route: 7601, ..Default::default() };
        let id = reg.add_map_line(LineDesign { number: "76".into(), ai_group: "Busses".into(), directions: vec![out], map: Some(Default::default()), ..Default::default() }).id;
        let mut taken = Taken::default();
        taken.add_depot(&d);
        assign_lines(&mut reg, &[id], std::slice::from_ref(&d), &taken, &HashMap::new());
        assert_eq!(reg.line(id).unwrap().directions[0].ibis_route, 7601);
        let lines: Vec<&LineDesign> = reg.lines.iter().collect();
        let b = block(&lines, &d);
        assert!(!b.contains("[infosystem_trip]") && !b.contains("[addterminus]"), "the file's own route and destination: {b}");
        // a stop added: a route of its own
        let l = reg.line_mut(id).unwrap();
        l.directions[0].stops.insert(1, stop(13, "Kirche"));
        l.directions[0].legs.push(ok_leg());
        assign_lines(&mut reg, &[id], std::slice::from_ref(&d), &taken, &HashMap::new());
        assert_eq!(reg.line(id).unwrap().directions[0].ibis_route, 7602);
        let lines: Vec<&LineDesign> = reg.lines.iter().collect();
        assert!(block(&lines, &d).contains("[infosystem_trip]\r\n7602\r\n"));
    }

    #[test]
    fn the_strings_are_known_by_the_files_notes() {
        let d = Depot::from_text(Path::new("Grundorf.hof"), GRUNDORF, "Grundorf");
        assert_eq!(d.roles, vec![Role::IbisDisplay, Role::FrontTop, Role::FrontBottom, Role::Side, Role::RollerBlind, Role::ClearName, Role::ExtraSign, Role::Bitmap]);
        assert_eq!(d.cols[4], Column::File("Blanko.tga".into()), "a roller blind's blank texture");
        assert_eq!(d.cols[6], Column::Empty);
        assert_eq!(d.cols[5], Column::Text { upper: false, max: 20, centred: false }, "the note's length");
        assert!(matches!(d.cols[1], Column::Text { upper: true, centred: true, .. }));
        // other files' notes
        assert_eq!(Role::from_note("Anzeige Seiten-ANNAX-Matrix, wenn diese einzeilig ist"), Role::Side);
        assert_eq!(Role::from_note("Texturname für Steckschild (wenn Ziel in Matrix nicht darstellbar ist)"), Role::ExtraSign);
        assert_eq!(Role::from_note("Zielschildtext"), Role::FrontTop);
        assert_eq!(Role::from_note("ist Sonderziel (löscht das Matrix-Skript die Liniennr.)"), Role::Special);
        assert_eq!(Role::from_note("Zielname fuer andere Busse mit Standard-IBIS (SD etc)"), Role::IbisDisplay);
        // a file without notes that looks like the stock ones
        let cols = vec![Column::Empty, Column::Empty, Column::Empty, Column::Empty, Column::File(String::new()), Column::Empty];
        assert_eq!(roles(&vec![String::new(); 6], &cols)[1], Role::FrontTop);
    }

    #[test]
    fn a_new_destination_is_written_as_the_file_writes_its_own() {
        let d = Depot::from_text(Path::new("Grundorf.hof"), GRUNDORF, "Grundorf");
        let s = d.sign_defaults("Marktplatz Süd");
        assert_eq!(s.len(), 8);
        assert_eq!(s[0], "MARKTPLATZ SÜD");
        assert_eq!(s[1].trim(), "MARKTPLATZ SÜD", "the front's first line, centred");
        assert!(s[1].starts_with(' '));
        assert_eq!(s[2], "", "one line is enough");
        assert_eq!(s[4], "Blanko.tga");
        assert_eq!(s[5], "Marktplatz Süd");
        assert_eq!((s[6].as_str(), s[7].as_str()), ("", ""));
        // too long for a line: split between the words, the ends shortened
        let s = d.sign_defaults("Albert-Einstein-Straße Hauptbahnhof");
        assert_eq!((s[1].trim(), s[2].trim()), ("ALBERT-EINSTEIN-", "HAUPTBAHNHOF"));
        assert_eq!(shorten("Albert-Einstein-Straße", 18), "Albert-Einstein-st");
        assert_eq!(shorten("Hauptstraße am Bahnhof", 16), "Hauptstr. am Bhf");
        assert_eq!(styled("Lindenplatz Nord", true, 14), "LINDENPL. NORD");
        // the front of a sign: its two lines
        let (a, b) = d.front(&d.terminus("Krankenhaus").unwrap().strings);
        assert_eq!((a.as_str(), b.as_str()), ("EINSTEINDORF", "KRANKENHAUS"));
        // a stop's IBIS name: the file's own, else made like its own
        assert_eq!(d.stop_display("Bauernhof"), "NORDS. BAUERNHOF");
        assert_eq!(d.stop_display("Kirche am Markt"), "KIRCHE AM MARKT");
    }

    /// A depot file as the HOF Suite writes it: no notes on its strings, its termini in a list
    /// (padded to 16), the roller blind's string (the fifth) empty in every row.
    pub(crate) const HOF_SUITE: &str = "This file was written for a test.\r\n\r\n[name]\r\nTalstadt\r\n\r\n[servicetrip]\r\nBetriebsfahrt\r\n\r\n\
        stringcount_terminus\r\n26\r\n\r\nstringcount_busstop\r\n9\r\n\r\n[addterminus_list]\r\n\
        {ALLEX}\t0\tBetriebsfahrt\tBETRIEBSFAHRT   \tBetriebsfahrt   \t\tBetriebsfahrt   \t\tBetriebsfahrt\r\n\
        \t5\tAltdorf\tALTDORF         \tAltdorf         \t\tAltdorf         \t\tAltdorf\r\n\
        \t7\tNeustadt, Marktplatz\tNEUSTADT MARKT  \tNeustadt        \tMarktplatz      \tNeustadt Markt  \t\tNeustadt, Marktplatz\r\n\
        \t9\tSchule Bergheim\tSCHULE          \tSchule          \tBergheim        \tSchule          \t\tSchule Bergheim\r\n\
        \t12\tLindenau\tLINDENAU        \tLindenau        \t\tLindenau        \t\tLindenau\r\n\
        \t14\tTalstadt, Hauptbahnhof\tHAUPTBAHNHOF    \tTalstadt        \tHauptbahnhof    \tHauptbahnhof    \t\tHauptbahnhof\r\n\
        [end]\r\n";

    #[test]
    fn a_file_without_notes_has_omsis_layout() {
        let d = Depot::from_text(Path::new("Talstadt.hof"), HOF_SUITE, "Talstadt");
        assert_eq!(d.roles.len(), 26);
        assert_eq!(d.roles[..8], USUAL);
        assert!(d.roles[8..].iter().all(|r| *r == Role::Other));
        assert_eq!(d.cols[4], Column::Empty, "no roller blind");
        assert!(!d.one_row_bottom, "a name of one row is on the first");
        // the file's own: its two rows
        let (a, b) = d.front(&d.terminus("Neustadt, Marktplatz").unwrap().strings);
        assert_eq!((a.as_str(), b.as_str()), ("Neustadt", "Marktplatz"));
        assert_eq!(d.shown(&d.terminus("Neustadt, Marktplatz").unwrap().strings), ("Neustadt Markt".to_string(), "NEUSTADT MARKT".to_string()));
        // a small file: its second and third text the front's rows, as the buses read them
        let text = |upper: bool| Column::Text { upper, max: 16, centred: false };
        assert_eq!(roles(&vec![String::new(); 3], &[text(true), text(true), text(false)]), USUAL[..3]);
        // a file of its own layout: its first text is the front
        assert_eq!(roles(&vec![String::new(); 4], &[Column::Empty, Column::File(String::new()), Column::Empty, text(false)]), vec![Role::Other, Role::Other, Role::Other, Role::FrontTop]);
        assert_eq!(roles(&vec![String::new(); 6], &[text(true), text(true), text(true), text(true), text(false), text(true)])[..2], [Role::FrontTop, Role::Other], "a text fifth: not OMSI's");
    }

    #[test]
    fn a_new_destination_shows_once_on_a_two_row_sign() {
        let d = Depot::from_text(Path::new("Talstadt.hof"), HOF_SUITE, "Talstadt");
        let s = d.sign_defaults("Hohenkirchen, Therme");
        assert_eq!(s.len(), 26);
        assert_eq!((s[1].as_str(), s[2].as_str()), ("Hohenkirchen", "Therme"), "split, without the comma");
        assert_eq!(s[0], "THERME", "the stop alone, as the file writes its own");
        assert_eq!(s[3], "Therme");
        assert_eq!(s[5], "Hohenkirchen, Therme", "the name as written fits its 20");
        assert!(s[4].is_empty() && s[6..].iter().all(String::is_empty), "{s:?}");
        assert_eq!(d.front(&s), ("Hohenkirchen".to_string(), "Therme".to_string()));
        // a name of one row: the second row stays empty
        let s = d.sign_defaults("Altheim");
        assert_eq!((s[1].as_str(), s[2].as_str()), ("Altheim", ""));
        for name in ["Hohenkirchen, Therme", "Altheim", "Talstadt Hauptbahnhof", "Neustadt, Marktplatz Süd", "Am Waldrand Ost", "Bergheim, Schule"] {
            let s = d.sign_defaults(name);
            assert!(s[2].trim().is_empty() || s[1].trim() != s[2].trim(), "{name}: {s:?}");
            let (a, b) = d.front(&s);
            assert_eq!((a.as_str(), b.as_str()), (s[1].trim(), s[2].trim()), "{name}");
        }
        // a name the file knows in other words
        assert_eq!(d.similar("Bergheim, Schule").map(|t| t.code), Some(9));
        assert_eq!(d.similar("schule bergheim."), Some(&d.hof.termini[3]));
        assert!(d.similar("Schule Bergheim").is_none(), "its own ident: no other");
        assert!(d.similar("Betriebsfahrt").is_none() && d.similar("Bergheim").is_none());
    }

    /// A file without notes whose destinations are all taken out still has its front's rows
    /// where the buses read them, for its next destination and the one after.
    #[test]
    fn a_file_without_notes_or_destinations_has_omsis_layout() {
        let mut hof = Depot::from_text(Path::new("Talstadt.hof"), HOF_SUITE, "Talstadt").hof;
        hof.termini.clear();
        let d = Depot::from_hof(hof.clone(), Vec::new());
        assert_eq!(d.roles[..8], USUAL);
        let s = d.sign_defaults("Krankenhaus Nordstadt");
        assert_eq!((s[1].as_str(), s[2].as_str()), ("KRANKENHAUS", "NORDSTADT"), "{s:?}");
        assert_eq!(d.front(&s), ("KRANKENHAUS".to_string(), "NORDSTADT".to_string()));
        // the next one, with that one in the file
        hof.termini.push(Terminus { code: 1, texture_id: "Krankenhaus Nordstadt".into(), terminus_stop: None, all_exit: false, strings: s });
        let d = Depot::from_hof(hof, Vec::new());
        assert_eq!(d.roles[..8], USUAL);
        assert_eq!(d.front(&d.sign_defaults("Altdorf")), ("ALTDORF".to_string(), String::new()));
    }

    #[test]
    fn one_row_names_go_on_the_row_the_file_uses() {
        // (as Spandau's: a name of one row on the second, centred)
        const WESTHAFEN: &str = "[name]\r\nWesthafen\r\n\r\nstringcount_terminus\r\n6\r\n\r\n\
            \tstring0:\tIBIS-Anzeige, max. 16 Zeichen\r\n\tstring1:\tMatrix vorne, 1. Zeile\r\n\tstring2:\tMatrix vorne, 2. Zeile\r\n\
            \tstring3:\tMatrix Seite\r\n\tstring4:\tRollbandtextur\r\n\tstring5:\tIBIS2 (Klarname), max 20 Zeichen\r\n\r\nstringcount_busstop\r\n4\r\n\r\n\
            [addterminus]\r\n3\r\nSchulbus\r\nSCHULBUS\r\n                \r\n    SCHULBUS\r\n    SCHULBUS\r\nSchulbus.tga\r\nSchulbus\r\n\r\n\
            [addterminus]\r\n11\r\nHafen\r\nHAFEN\r\n\r\n      HAFEN\r\n      HAFEN\r\nWH_Hafen.tga\r\nHafen\r\n\r\n\
            [addterminus]\r\n12\r\nRathaus\r\nRATHAUS\r\n   WESTHAFEN\r\n    RATHAUS\r\n    RATHAUS\r\nWH_Rathaus.tga\r\nRathaus Westhafen\r\n\r\n\
            [addterminus]\r\n14\r\nKlinik\r\nKLINIK\r\n\r\n     KLINIK\r\n     KLINIK\r\nWH_Klinik.tga\r\nKlinik\r\n\r\n";
        let d = Depot::from_text(Path::new("Westhafen.hof"), WESTHAFEN, "Westhafen");
        assert_eq!(d.roles, USUAL[..6]);
        assert!(d.one_row_bottom);
        let s = d.sign_defaults("Flugplatz");
        assert_eq!((s[1].as_str(), s[2].trim()), ("", "FLUGPLATZ"));
        assert_eq!(d.front(&s), ("FLUGPLATZ".to_string(), String::new()), "the buses move it up");
        assert_eq!(d.front(&d.terminus("Hafen").unwrap().strings), ("HAFEN".to_string(), String::new()));
        // two rows as ever
        let s = d.sign_defaults("Westhafen Hauptbahnhof");
        assert_eq!((s[1].trim(), s[2].trim()), ("WESTHAFEN", "HAUPTBAHNHOF"));
    }

    #[test]
    fn texts_for_other_signs_stay_empty() {
        // (eighteen texts, as Vienna's: other makes' matrices, falling back to the stock ones)
        const NOTES: [&str; 18] = [
            "IBIS-Anzeige (Großbuchstaben, max. 16 Zeichen)",
            "Matrix Front, 1. Zeile",
            "Matrix Front, 2. Zeile",
            "Seitenmatrix, einzeilig",
            "Rollbandtextur (tga)",
            "IBIS 2 (Klarname, max. 20 Zeichen)",
            "Steckschild (Textur)",
            "Bild für die Krüger-Matrix (bmp)",
            "Frontmatrix in Groß- und Kleinbuchstaben",
            "Heckanzeige (max. 4 Zeichen)",
            "Liniencode für die Gegenrichtung",
            "Vollmatrix oben (wenn leer, wird String 1 gezeigt)",
            "Vollmatrix unten (wenn leer, wird String 2 gezeigt)",
            "Lawo-Matrix vorne oben (bleibt dieser String leer, wird String 1 ausgelesen)",
            "Lawo-Matrix vorne unten (leer: String 13)",
            "Lawo-Matrix Seite oben (leer: String 16)",
            "Lawo-Matrix Seite unten (leer: String 15)",
            "Anzeige für ein anderes IBIS-Gerät (max. 16 Zeichen, Sonderzeichen möglich)",
        ];
        let notes: String = NOTES.iter().enumerate().map(|(k, n)| format!("\tstring{k}:\t{n}\r\n")).collect();
        let rows = [
            ["OSTPARK", "Ostpark", "", "Ostpark", "Ostpark.tga", "Ostpark", "", "", "Ostpark", "OP", "2101", "Ostpark", "", "Ostpark", "", "Ostpark", "", "Ostpark"],
            ["AM WEIHER", "Am Weiher", "", "Am Weiher", "Weiher.tga", "Am Weiher", "", "", "Am Weiher", "AW", "2102", "Am", "Weiher", "Am", "Weiher", "Am", "Weiher", "Am Weiher"],
        ];
        let termini: String = rows.iter().enumerate().map(|(i, r)| format!("[addterminus]\r\n{}\r\nZiel {i}\r\n{}\r\n\r\n", 21 + i, r.join("\r\n"))).collect();
        let text = format!("[name]\r\nAuring\r\n\r\nstringcount_terminus\r\n18\r\n\r\n{notes}\r\nstringcount_busstop\r\n4\r\n\r\n{termini}");
        let d = Depot::from_text(Path::new("Auring.hof"), &text, "Auring");
        assert_eq!(d.roles[9], Role::Rear);
        assert_eq!(d.roles[10], Role::Other);
        assert_eq!(d.roles[11..17], [Role::Fallback(1), Role::Fallback(2), Role::Fallback(1), Role::Fallback(13), Role::Fallback(16), Role::Fallback(15)]);
        assert_eq!(d.roles[17], Role::IbisDisplay);
        assert!((8..=17).all(|k| matches!(d.cols[k], Column::Text { .. })), "{:?}", d.cols);
        let s = d.sign_defaults("Neues Ziel");
        assert_eq!((s[0].as_str(), s[1].as_str(), s[2].as_str(), s[3].as_str(), s[5].as_str()), ("NEUES ZIEL", "Neues Ziel", "", "Neues Ziel", "Neues Ziel"));
        assert!(s[8..=16].iter().all(String::is_empty), "{s:?}");
        assert_eq!(s[17], "Neues Ziel", "another IBIS: a display of its own");
    }

    #[test]
    fn notes_say_which_texts_fall_back() {
        assert_eq!(Role::from_note("Wenn diese Zeile leer ist, wird String 1 ausgelesen"), Role::Fallback(1));
        assert_eq!(Role::from_note("Top row of the full matrix (if empty, string 2 is shown)"), Role::Fallback(2));
        assert_eq!(Role::from_note("Side, falls back to string12"), Role::Fallback(12));
        assert_eq!(Role::from_note("Leerzeichen vorne zentrieren den Text"), Role::Other);
        assert_eq!(Role::from_note("Zielschild (mit Leerzeichen zentriert, wie String 1)"), Role::FrontTop, "spaces are no fallback");
        assert_eq!(Role::from_note("Zielanzeige (leer lassen, wenn nicht gebraucht)"), Role::FrontTop, "empty, but no string it falls back to");
        assert_eq!(Role::from_note("Heckschild"), Role::Rear);
        assert_eq!(Role::from_note("Rear sign (4 characters)"), Role::Rear);
        assert_eq!(Role::from_note("IBIS-Code für die Gegenrichtung"), Role::Other);
        assert_eq!(Role::from_note("Special trip (no line number)"), Role::Special);
        assert_eq!(Role::from_note("IBIS display, special characters allowed"), Role::IbisDisplay);
    }

    #[test]
    fn shorten_keeps_the_stop() {
        assert_eq!(shorten("Hohenkirchen, Therme", 16), "Therme");
        assert_eq!(shorten("Hohenkirchen, Flugplatz", 20), "Hohenkirchen Flugpl.");
        assert_eq!(shorten("Erfurt, Hauptbahnhof", 19), "Erfurt Hauptbahnhof", "the comma goes before a word is shortened");
        assert_eq!(styled("Herrenhof, Hauptbahnhof", true, 16), "HERRENHOF HBF.");
        assert_eq!(styled("Herrenhof, Einkaufszentrum", true, 16), "EINKAUFSZENTRUM");
        assert_eq!(shorten("Kreuzungspunkt, B2", 12), "Kreuzungspun", "a stop of under 3 characters is not taken alone");
        let cut = shorten("Langenberg, Am Sportplatz Nordost", 12);
        assert!(cut.chars().count() <= 12 && cut.starts_with("Langenberg"), "{cut}");
    }

    #[test]
    fn codes_are_free_and_kept() {
        let taken: HashSet<u32> = [4201, 7601].into_iter().collect();
        assert_eq!(route_code("42", 0, 0, &taken), Some(4202));
        assert_eq!(route_code("42", 1, 0, &taken), Some(4202));
        assert_eq!(route_code("42", 1, 4210, &taken), Some(4210), "a free one of the line's stays");
        assert_eq!(route_code("43", 0, 4210, &taken), Some(4301), "the line's number changed");
        assert_eq!(route_code("X10", 0, 0, &taken), Some(1001));
        assert_eq!(line_number("5E"), 5);
        let all: HashSet<u32> = (4201..=4299).collect();
        assert_eq!(route_code("42", 0, 0, &all), None);
        let termini: HashSet<i32> = [901, 902, 105].into_iter().collect();
        assert_eq!(terminus_code(0, &termini), 903);
        assert_eq!(terminus_code(950, &termini), 950);
        assert_eq!(terminus_code(105, &termini), 903);
    }

    #[test]
    fn the_block_is_replaced_and_the_file_kept() {
        let once = put_block(GRUNDORF, "Grundorf", "[addterminus]\r\n1\r\nA\r\n");
        let twice = put_block(&once, "Grundorf", "[addterminus]\r\n2\r\nB\r\n");
        assert_eq!(twice.matches("openOMSI line editor, map Grundorf: begin").count(), 1);
        assert!(twice.contains("\r\nB\r\n") && !twice.contains("\r\nA\r\n"));
        // taken out: the file as it was, however often it was written
        assert_eq!(strip_block(&twice, "Grundorf"), GRUNDORF);
        // another map's block stays
        let other = put_block(&twice, "Ahlheim", "[addterminus]\r\n3\r\nC\r\n");
        assert!(strip_block(&other, "Grundorf").contains("\r\nC\r\n"));
        // a file of LF line ends keeps them
        let lf = GRUNDORF.replace("\r\n", "\n");
        let put = put_block(&lf, "Grundorf", "[addterminus]\r\n1\r\nA\r\n");
        assert!(!put.contains('\r'));
        assert_eq!(strip_block(&put, "Grundorf"), lf);
    }

    #[test]
    fn the_code_page_is_kept() {
        let text = "[name]\r\nLöptener\r\n";
        let bytes = encode(text, Coding::Page(CodePage::Windows1252));
        assert_eq!(bytes[9], 0xF6);
        let (back, coding) = decode(&bytes);
        assert_eq!(back, text);
        assert!(matches!(coding, Coding::Page(_)));
        // what the code page lacks is no HTML entity
        assert_eq!(encode("Ж", Coding::Page(CodePage::Windows1252)), b"?");
        let u16 = encode("[name]\r\nA\r\n", Coding::Utf16);
        assert_eq!(decode(&u16), ("[name]\r\nA\r\n".to_string(), Coding::Utf16));
    }

    #[test]
    fn a_line_gets_its_terminus_route_and_stops() {
        let mut reg = registry();
        let d = Depot::from_text(Path::new("Grundorf.hof"), GRUNDORF, "Grundorf");
        let mut taken = Taken::default();
        taken.add(&d.hof);
        assign(&mut reg, &groups(), "Grundorf", &d, &taken);
        let l = &reg.lines[0];
        assert_eq!((l.directions[0].terminus_code, l.directions[0].ibis_route, l.directions[1].ibis_route), (901, 4201, 4202));
        // the player's name for a stop on the IBIS
        reg.lines[0].directions[0].stops[1].ibis = "KIRCHE MITTE".into();
        let lines: Vec<&LineDesign> = reg.lines.iter().collect();
        let text = put_block(GRUNDORF, "Grundorf", &block(&lines, &d));
        let h = Hof::parse(&omsi_cfg::CfgFile::from_str("Grundorf.hof", &text));
        // the file's own entries as they were, ours after them
        assert_eq!(h.termini.len(), 6);
        let t = h.termini.iter().find(|t| t.texture_id == "Marktplatz Süd").unwrap();
        assert_eq!(t.code, 901);
        assert_eq!(t.strings.len(), 8);
        assert_eq!(t.strings[0], "MARKTPLATZ SÜD");
        let trips: Vec<(&str, &str, &str)> = h.info_trips.iter().map(|t| (t.code.as_str(), t.route.as_str(), t.line.as_str())).collect();
        assert_eq!(trips, vec![("7601", "105", "TML"), ("4201", "901", "42"), ("4202", "107", "42")]);
        assert_eq!(h.info_busstop_lists[1], vec!["Bauernhof", "Kirche", "Marktplatz Süd"]);
        assert_eq!(h.info_busstop_lists[2], vec!["Marktplatz Süd", "Kirche#oo", "Bauernhof"], "the way back's Kirche shows its own name, not the one given on the way out");
        let kirche = h.bus_stops.iter().find(|b| b.ident == "Kirche").unwrap();
        assert_eq!(kirche.strings, vec!["KIRCHE MITTE", "Kirche", "", "Kirche"]);
        let back = h.bus_stops.iter().find(|b| b.ident == "Kirche#oo").unwrap();
        assert_eq!(back.strings[0], "KIRCHE");
        // the stock stop keeps its own name; it is not written again
        assert_eq!(h.bus_stops.iter().filter(|b| b.ident.eq_ignore_ascii_case("Bauernhof")).count(), 1);
        // the codes stay what they were on the next save
        let before = reg.clone();
        assign(&mut reg, &groups(), "Grundorf", &d, &taken);
        assert_eq!(reg, before);
    }

    /// A destination the first copy of the depot file has and another lacks (another bus pack's
    /// version): a code of its own for that one, free in both.
    #[test]
    fn a_destination_a_copy_lacks_gets_a_code_there() {
        let with = format!("{GRUNDORF}\r\n[addterminus]\r\n120\r\nMarktplatz Süd\r\n{}\r\n", "MARKT\r\n".repeat(8));
        let depots = vec![Depot::from_text(Path::new("A/Grundorf.hof"), &with, "Grundorf"), Depot::from_text(Path::new("B/Grundorf.hof"), GRUNDORF, "Grundorf")];
        let mut taken = Taken::default();
        for d in &depots {
            taken.add_depot(d);
        }
        let mut reg = registry();
        let id = reg.lines[0].id;
        assign_lines(&mut reg, &[id], &depots, &taken, &HashMap::new());
        assert_eq!(reg.lines[0].directions[0].terminus_code, 901);
        let lines: Vec<&LineDesign> = reg.lines.iter().collect();
        let route = |text: &str| {
            let h = Hof::parse(&omsi_cfg::CfgFile::from_str("x.hof", &put_block(text, "Grundorf", &block(&lines, &Depot::from_text(Path::new("x.hof"), text, "Grundorf")))));
            let code = h.info_trips.iter().find(|t| t.code == "4201").unwrap().route.clone();
            (h.termini.iter().filter(|t| t.texture_id == "Marktplatz Süd").map(|t| t.code).collect::<Vec<_>>(), code)
        };
        // the copy that has it: its own; the other: the code given
        assert_eq!(route(with.as_str()), (vec![120], "120".to_string()));
        assert_eq!(route(GRUNDORF), (vec![901], "901".to_string()));
    }

    /// Two lines to one new destination by two codes (one of them given in a depot file of the
    /// player's own): one `[addterminus]`, and both routes go to its code.
    #[test]
    fn two_lines_to_one_new_destination_share_its_entry() {
        let d = Depot::from_text(Path::new("Grundorf.hof"), GRUNDORF, "Grundorf");
        let mut reg = registry();
        let mut other = reg.lines[0].clone();
        other.id = 2;
        other.number = "43".into();
        reg.lines.push(other);
        for (l, (code, route)) in reg.lines.iter_mut().zip([(902, 4200), (901, 4300)]) {
            l.directions[0].terminus_code = code;
            for (k, x) in l.directions.iter_mut().enumerate() {
                x.ibis_route = route + 1 + k as u32;
            }
        }
        let lines: Vec<&LineDesign> = reg.lines.iter().collect();
        let h = Hof::parse(&omsi_cfg::CfgFile::from_str("Grundorf.hof", &put_block(GRUNDORF, "Grundorf", &block(&lines, &d))));
        assert_eq!(h.termini.iter().filter(|t| t.texture_id == "Marktplatz Süd").map(|t| t.code).collect::<Vec<_>>(), vec![902]);
        for code in ["4201", "4301"] {
            assert_eq!(h.info_trips.iter().find(|t| t.code == code).unwrap().route, "902", "{code}");
        }
    }

    /// A change waiting for its day: the depot files keep the line as the timetable runs it.
    #[test]
    fn a_change_waiting_for_its_day_is_not_in_the_depot_file_yet() {
        let d = Depot::from_text(Path::new("Grundorf.hof"), GRUNDORF, "Grundorf");
        let mut taken = Taken::default();
        taken.add_depot(&d);
        let mut reg = registry();
        let running = reg.lines[0].clone();
        let mut pending = running.clone();
        pending.directions[0].terminus = "Rathaus".into();
        reg.lines[0] = LineDesign { live: Some(Box::new(running)), pending_from: "2024-03-06".into(), ..pending };
        assign(&mut reg, &groups(), "Grundorf", &d, &taken);
        assert_eq!(reg.lines[0].in_timetable().directions[0].terminus_code, 901);
        let lines: Vec<&LineDesign> = lines_of(&reg, &groups(), "Grundorf").into_iter().map(LineDesign::in_timetable).collect();
        let text = block(&lines, &d);
        assert!(text.contains("\r\nMarktplatz Süd\r\n") && !text.contains("Rathaus"), "{text}");
    }

    /// Another map's block in a shared depot file: not the file's own (that map takes it out
    /// again), but its codes are not given twice.
    #[test]
    fn another_maps_block_is_not_the_files_own() {
        let theirs = format!("[addterminus]\r\n901\r\nMarktplatz Süd\r\n{}\r\n[addbusstop]\r\nKirche\r\nKIRCHE\r\n\r\n\r\n\r\n\r\n[infosystem_trip]\r\n4201\r\nX\r\n901\r\n42\r\n\r\n", "MARKT\r\n".repeat(8));
        let text = put_block(GRUNDORF, "Ahlheim", &theirs);
        let d = Depot::from_text(Path::new("Grundorf.hof"), &text, "Grundorf");
        assert!(d.terminus("Marktplatz Süd").is_none() && d.own_stop("Kirche").is_none());
        assert!(d.others.termini.contains(&901) && d.others.routes.contains(&4201));
        let mut taken = Taken::default();
        taken.add_depot(&d);
        let mut reg = registry();
        assign(&mut reg, &groups(), "Grundorf", &d, &taken);
        let l = &reg.lines[0];
        assert_eq!((l.directions[0].terminus_code, l.directions[0].ibis_route), (902, 4202));
        // its own entries for the map's lines, beside the other map's
        let lines: Vec<&LineDesign> = reg.lines.iter().collect();
        let ours = block(&lines, &d);
        assert!(ours.contains("[addterminus]\r\n902\r\nMarktplatz Süd\r\n") && ours.contains("[addbusstop]\r\nKirche\r\n"), "{ours}");
    }

    /// Two content roots: the content folder, an installation. Every bus folder's depot file
    /// gets the block in the content folder, the installation is not touched, and taking the
    /// line out leaves the content folder as it was.
    #[test]
    fn every_bus_folder_gets_it_in_the_content_folder() {
        let base = std::env::temp_dir().join(format!("omsi_linehof_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (content, omsi) = (base.join("content"), base.join("omsi"));
        let put = |p: PathBuf, text: &str| {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, encode(text, Coding::Page(CodePage::Windows1252))).unwrap();
        };
        put(omsi.join("Vehicles/MAN_NL/Grundorf.hof"), GRUNDORF);
        put(omsi.join("Vehicles/Pack/Solo/Grundorf.hof"), GRUNDORF);
        put(omsi.join("Vehicles/Pack/Solo/Spandau.hof"), "[name]\r\nSpandau\r\n");
        // a depot file of the content folder (added there for a mod bus)
        let own = GRUNDORF.replace("Krankenhaus", "Klinikum");
        put(content.join("Vehicles/O530/Grundorf.hof"), &own);
        let bases = vec![content.clone(), omsi.clone()];
        let found = copies("Grundorf", &bases);
        assert_eq!(found.iter().map(|c| c.rel.as_str()).collect::<Vec<_>>(), vec!["Vehicles/MAN_NL/Grundorf.hof", "Vehicles/O530/Grundorf.hof", "Vehicles/Pack/Solo/Grundorf.hof"]);
        let before: Vec<Vec<u8>> = ["Vehicles/MAN_NL/Grundorf.hof", "Vehicles/Pack/Solo/Grundorf.hof"].iter().map(|r| std::fs::read(omsi.join(r)).unwrap()).collect();
        let mut reg = registry();
        let plan = prepare(&mut reg, &groups(), &bases);
        assert_eq!(reg.depots, vec!["Grundorf".to_string()]);
        let (n, err) = write(&content, Some(&omsi), &reg, &groups(), &plan);
        assert_eq!((n, err), (3, None));
        for rel in ["Vehicles/MAN_NL/Grundorf.hof", "Vehicles/O530/Grundorf.hof", "Vehicles/Pack/Solo/Grundorf.hof"] {
            let h = Hof::load(&content.join(rel)).unwrap();
            assert!(h.termini.iter().any(|t| t.texture_id == "Marktplatz Süd" && t.code == 901), "{rel}");
            assert!(h.info_trips.iter().any(|t| t.code == "4201"), "{rel}");
        }
        assert!(Hof::load(&content.join("Vehicles/O530/Grundorf.hof")).unwrap().termini.iter().any(|t| t.texture_id == "Klinikum"), "the content folder's own file, written in place");
        let after: Vec<Vec<u8>> = ["Vehicles/MAN_NL/Grundorf.hof", "Vehicles/Pack/Solo/Grundorf.hof"].iter().map(|r| std::fs::read(omsi.join(r)).unwrap()).collect();
        assert_eq!(before, after, "the installation is not touched");
        // the copy is in the file's code page ("Süd" in Windows-1252)
        let b = std::fs::read(content.join("Vehicles/MAN_NL/Grundorf.hof")).unwrap();
        assert!(b.windows(3).any(|w| w == [b'S', 0xFC, b'd']));
        // saved again: nothing changes
        let plan = prepare(&mut reg, &groups(), &bases);
        assert_eq!(write(&content, Some(&omsi), &reg, &groups(), &plan), (0, None));
        // the line deleted: the copies made for it go, the content folder's own file is as it was
        reg.lines.clear();
        let plan = prepare(&mut reg, &groups(), &bases);
        assert!(reg.depots.is_empty());
        assert_eq!(write(&content, Some(&omsi), &reg, &groups(), &plan), (3, None));
        assert!(!content.join("Vehicles/MAN_NL/Grundorf.hof").exists() && !content.join("Vehicles/Pack/Solo/Grundorf.hof").exists());
        assert_eq!(decode(&std::fs::read(content.join("Vehicles/O530/Grundorf.hof")).unwrap()).0, own);
        let _ = std::fs::remove_dir_all(&base);
    }
}
