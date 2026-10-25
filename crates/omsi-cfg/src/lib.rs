//! OMSI's text formats.
//!
//! Almost every OMSI content file (`.bus`, `.sco`, `.sli`, `.cfg`, `.hof`, `.map`, …) is a list
//! of lines in which a *keyword* line such as `[mesh]` is followed by a fixed number of parameter
//! lines.  Everything else is free text and ignored.  The rules, read off the loaders of
//! `Omsi.exe` 2.2.032 (every one compares the line it read with its keyword literals by plain
//! string equality) and checked against the stock and installed content, are:
//!
//! * a keyword is recognised only when the whole line is `[name]`: an indented keyword
//!   (the stock files' help texts, the F90 lorry's disabled second rear axle, the EN92's
//!   disabled fourth door-slam sound) is free text, and so is one followed by spaces;
//! * the name is spelled as the original spells it (`[matl_noZwrite]`, not `[matl_nozWrite]`);
//!   names the original does not know are compared without regard to case;
//! * two loaders differ: `.hof` cuts trailing tabs, spaces and quotes off every line
//!   (spreadsheet exports), `ailists.cfg` lower-cases its lines ([`KeywordRule`]);
//! * the main content loaders skip `-<DISABLED>-` … `-<ENABLED>-` between blocks
//!   ([`CfgReader::disabled_blocks`]);
//! * parameters are read verbatim, one per line, empty lines included.
//!
//! Text is read in the code page it was written in: a byte-order mark says so, otherwise
//! [`codepage::detect`] tells UTF-8, Windows-1251 (Russian mods), 1250 (Polish, Czech) and
//! the stock content's Windows-1252 apart.

use std::path::{Path, PathBuf};

pub mod codepage;
pub mod collate;
pub mod flags;
pub mod lang;
mod keywords;
pub mod number;
pub mod par;
pub mod radio;
pub mod template;
pub mod transport;
pub mod panic_guard;
pub mod profile_keys;
pub mod vfs;
pub mod install_search;
pub mod copied_install;
pub mod user_data;
mod walk;
use walk::{pop_within, resolve_existing};
pub use install_search::find_original_install;
pub use number::{parse_f32, parse_f64, parse_i32, parse_i64};
pub use vfs::{add_content_zip, mount_zip};

/// Decode raw file bytes with OMSI's encoding rules.
pub fn decode_text(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        let (s, _) = encoding_rs::UTF_16LE.decode_without_bom_handling(&bytes[2..]);
        s.into_owned()
    } else if bytes.len() >= 3 && bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF {
        String::from_utf8_lossy(&bytes[3..]).into_owned()
    } else {
        codepage::decode(bytes)
    }
}

/// Encode a string the way OMSI writes it (Windows-1252, CR LF).
pub fn encode_text(s: &str) -> Vec<u8> {
    let (b, _, _) = encoding_rs::WINDOWS_1252.encode(s);
    b.into_owned()
}

/// Split into lines. Handles CR LF, LF and bare CR.
pub fn split_lines(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push(std::mem::take(&mut cur));
            }
            '\n' => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[derive(Debug, thiserror::Error)]
pub enum CfgError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// A loaded text file, split into lines.
#[derive(Debug, Clone)]
pub struct CfgFile {
    pub path: PathBuf,
    pub lines: Vec<String>,
}

impl CfgFile {
    pub fn read(path: impl AsRef<Path>) -> Result<Self, CfgError> {
        let path = path.as_ref();
        let bytes = vfs::read(path).map_err(|e| CfgError::Io { path: path.to_path_buf(), source: e })?;
        Ok(Self::from_bytes(path, &bytes))
    }

    pub fn from_bytes(path: impl AsRef<Path>, bytes: &[u8]) -> Self {
        Self { path: path.as_ref().to_path_buf(), lines: split_lines(&decode_text(bytes)) }
    }

    pub fn from_str(path: impl AsRef<Path>, text: &str) -> Self {
        Self { path: path.as_ref().to_path_buf(), lines: split_lines(text) }
    }

    pub fn reader(&self) -> CfgReader<'_> {
        CfgReader { file: self, pos: 0, block_line: 0, rule: KeywordRule::Exact, disabled_blocks: false }
    }

    /// Directory the file lives in (for resolving relative paths in it).
    pub fn dir(&self) -> &Path {
        self.path.parent().unwrap_or(Path::new(""))
    }
}

/// How a loader of the original tells a keyword line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeywordRule {
    /// The line is exactly `[name]` with the original's spelling (almost every loader).
    #[default]
    Exact,
    /// Trailing tabs, spaces, line breaks and double quotes are cut off first (`.hof`, whose
    /// stock files are spreadsheet exports: `[infosystem_trip]` followed by twelve tabs).
    TrimEnd,
    /// The line is lower-cased first (`ailists.cfg`).
    AnyCase,
}

/// What `.hof` cuts off the end of every line.
fn hof_trailing(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | ' ' | '"')
}

/// The original's spelling of a keyword it knows (by the name in any case).
fn omsi_spelling(name: &str) -> Option<&'static str> {
    let lower;
    let key = if name.bytes().any(|b| b.is_ascii_uppercase()) {
        lower = name.to_ascii_lowercase();
        lower.as_str()
    } else {
        name
    };
    if let Ok(i) = keywords::MIXED_CASE.binary_search_by(|k| cmp_lower(k, key)) {
        return Some(keywords::MIXED_CASE[i]);
    }
    keywords::LOWER_CASE.binary_search(&key).ok().map(|i| keywords::LOWER_CASE[i])
}

/// `a.to_ascii_lowercase().cmp(b)` without allocating.
fn cmp_lower(a: &str, b: &str) -> std::cmp::Ordering {
    a.bytes().map(|c| c.to_ascii_lowercase()).cmp(b.bytes())
}

/// Test whether a line is a keyword line and return the keyword (without brackets, as
/// written): the whole line is `[name]`, and `name` is spelled like the original spells it
/// when the original knows it.
pub fn keyword_of(line: &str) -> Option<&str> {
    keyword_with(line, KeywordRule::Exact)
}

/// [`keyword_of`] for a loader with its own rule.
pub fn keyword_with(line: &str, rule: KeywordRule) -> Option<&str> {
    let t = if rule == KeywordRule::TrimEnd { line.trim_end_matches(hof_trailing) } else { line };
    if t.len() < 2 || t.as_bytes()[0] != b'[' || !t.ends_with(']') {
        return None;
    }
    let inner = &t[1..t.len() - 1];
    // Keywords never contain a second bracket; free text like "[mesh] macht ..." is never a
    // keyword because it does not end with ']'.
    if inner.is_empty() || inner.contains('[') || inner.contains(']') {
        return None;
    }
    if rule != KeywordRule::AnyCase {
        if let Some(spelling) = omsi_spelling(inner) {
            if spelling != inner {
                return None;
            }
        }
    }
    Some(inner)
}

/// What [`CfgReader::next_entry`] stopped at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry<'t> {
    /// A keyword line: the keyword in lower case.
    Keyword(String),
    /// A line that is exactly one of the bare tokens asked for.
    Token(&'t str),
}

/// Sequential reader over a [`CfgFile`] with the original semantics.
#[derive(Clone)]
pub struct CfgReader<'a> {
    file: &'a CfgFile,
    pos: usize,
    block_line: usize,
    rule: KeywordRule,
    disabled_blocks: bool,
}

impl<'a> CfgReader<'a> {
    /// Read keyword lines by `rule` instead of [`KeywordRule::Exact`].
    pub fn with_rule(mut self, rule: KeywordRule) -> Self {
        self.rule = rule;
        self
    }

    /// Skip `-<DISABLED>-` … `-<ENABLED>-` between blocks, as the original's loaders of
    /// `.bus`/`.sco`/`model.cfg`/`paths.cfg` and the rest do: the marker lines must stand
    /// alone, exactly so - a marker read as a parameter is a parameter. Mods use it.
    pub fn disabled_blocks(mut self) -> Self {
        self.disabled_blocks = true;
        self
    }

    /// Whether `line` is a keyword line by this reader's rule.
    pub fn keyword(&self, line: &'a str) -> Option<&'a str> {
        keyword_with(line, self.rule)
    }

    pub fn path(&self) -> &Path {
        &self.file.path
    }

    /// Current line index (0-based) of the *next* line to be read.
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// 1-based line number of the keyword that opened the current block (for messages).
    pub fn block_line(&self) -> usize {
        self.block_line
    }

    pub fn seek(&mut self, pos: usize) {
        self.pos = pos.min(self.file.lines.len());
    }

    pub fn at_end(&self) -> bool {
        self.pos >= self.file.lines.len()
    }

    /// The next line as a parameter, or an empty string where a keyword line starts: a fixed
    /// list of parameters ends at the next block. An `.ovh` that leaves the registration
    /// affixes out (`[registration_automatic]` straight before `[model]`, the Urumqi AI cars)
    /// must not have the `[model]` read as one of them.
    pub fn param_line(&mut self) -> &'a str {
        if self.file.lines.get(self.pos).is_some_and(|l| keyword_with(l, self.rule).is_some()) {
            ""
        } else {
            self.line()
        }
    }

    /// Advance to the next keyword line and return the keyword in lower case.
    pub fn next_keyword(&mut self) -> Option<String> {
        match self.next_entry(&[]) {
            Some(Entry::Keyword(k)) => Some(k),
            _ => None,
        }
    }

    /// Advance to the next keyword line or the next line that is exactly one of `tokens`.
    /// The original's loaders see sub-commands such as `attach_trans` or `anim_rot` the way
    /// they see keywords: as a line of their own anywhere between blocks, spelled exactly so.
    pub fn next_entry<'t>(&mut self, tokens: &[&'t str]) -> Option<Entry<'t>> {
        while self.pos < self.file.lines.len() {
            let line = self.file.lines[self.pos].as_str();
            self.pos += 1;
            if let Some(k) = keyword_with(line, self.rule) {
                self.block_line = self.pos;
                return Some(Entry::Keyword(k.to_ascii_lowercase()));
            }
            if let Some(t) = tokens.iter().find(|t| **t == line) {
                self.block_line = self.pos;
                return Some(Entry::Token(t));
            }
            if self.disabled_blocks && line == "-<DISABLED>-" {
                while self.pos < self.file.lines.len() {
                    self.pos += 1;
                    if self.file.lines[self.pos - 1] == "-<ENABLED>-" {
                        break;
                    }
                }
            }
        }
        None
    }

    /// Peek at the next keyword without consuming anything.
    pub fn peek_keyword(&self) -> Option<String> {
        let mut c = self.clone();
        c.next_keyword()
    }

    /// Read the next raw parameter line. Missing lines at EOF read as empty strings, which is
    /// what the original does with a truncated file.
    pub fn line(&mut self) -> &'a str {
        if self.pos < self.file.lines.len() {
            let l = &self.file.lines[self.pos];
            self.pos += 1;
            l.as_str()
        } else {
            self.pos = self.file.lines.len() + 1;
            ""
        }
    }

    /// Like [`line`](Self::line) but with trailing whitespace removed.
    pub fn str(&mut self) -> &'a str {
        self.line().trim_end()
    }

    /// Trimmed on both sides (numbers, identifiers).
    pub fn word(&mut self) -> &'a str {
        self.line().trim()
    }

    pub fn f64(&mut self) -> f64 {
        parse_f64(self.line())
    }

    pub fn f32(&mut self) -> f32 {
        parse_f32(self.line())
    }

    pub fn i32(&mut self) -> i32 {
        parse_i32(self.line())
    }

    /// `n` integers, then any further whole-number lines that follow at once (a blank line,
    /// a keyword or anything else ends the list): OMSI's fixed-length lists read as always,
    /// and a file may give more (openOMSI: `[illumination_interior]` with more than four
    /// lamps).
    pub fn i32_list(&mut self, n: usize) -> Vec<i32> {
        let mut v: Vec<i32> = (0..n).map(|_| self.i32()).collect();
        while self.pos < self.file.lines.len() {
            let l = self.file.lines[self.pos].trim();
            let whole = !l.is_empty() && l.strip_prefix('-').unwrap_or(l).chars().all(|c| c.is_ascii_digit());
            if !whole || keyword_with(self.file.lines[self.pos].as_str(), self.rule).is_some() {
                break;
            }
            v.push(parse_i32(l));
            self.pos += 1;
        }
        v
    }

    pub fn i64(&mut self) -> i64 {
        parse_i64(self.line())
    }

    pub fn u32(&mut self) -> u32 {
        parse_i64(self.line()).max(0) as u32
    }

    pub fn usize(&mut self) -> usize {
        parse_i64(self.line()).max(0) as usize
    }

    pub fn count(&mut self) -> usize {
        let n = self.usize();
        n.min(self.file.lines.len().saturating_sub(self.pos))
    }

    /// `1`/`true` → true; anything else false.
    pub fn bool(&mut self) -> bool {
        let w = self.word();
        w == "1" || w.eq_ignore_ascii_case("true")
    }

    /// Read `n` lines as `f32`.
    pub fn f32s<const N: usize>(&mut self) -> [f32; N] {
        let mut a = [0.0; N];
        for v in a.iter_mut() {
            *v = self.f32();
        }
        a
    }

    pub fn f64s<const N: usize>(&mut self) -> [f64; N] {
        let mut a = [0.0; N];
        for v in a.iter_mut() {
            *v = self.f64();
        }
        a
    }

    /// Read all lines up to (not including) a line that is `terminator` (by the reader's
    /// rule, like a keyword). Used for `[description] … [end]`.
    pub fn until(&mut self, terminator: &str) -> Vec<&'a str> {
        let mut out = Vec::new();
        while self.pos < self.file.lines.len() {
            let l = self.file.lines[self.pos].as_str();
            self.pos += 1;
            let hit = match self.rule {
                KeywordRule::Exact => l == terminator,
                KeywordRule::TrimEnd => l.trim_end_matches(hof_trailing) == terminator,
                KeywordRule::AnyCase => l.eq_ignore_ascii_case(terminator),
            };
            if hit {
                break;
            }
            out.push(l);
        }
        out
    }

    /// All remaining lines that are not keywords, until the next keyword (not consumed).
    pub fn rest_of_block(&mut self) -> Vec<&'a str> {
        let mut out = Vec::new();
        while self.pos < self.file.lines.len() {
            let l = self.file.lines[self.pos].as_str();
            if keyword_with(l, self.rule).is_some() {
                break;
            }
            self.pos += 1;
            out.push(l);
        }
        out
    }

    /// Raw access to the lines.
    pub fn lines(&self) -> &'a [String] {
        &self.file.lines
    }
}

/// Convert an OMSI content path (backslashes, case-insensitive on Windows) to a real path
/// relative to `base`, resolving every component's case on case-sensitive systems. The
/// components are as Windows reads them: `\` and `/` separate, empty and `.` vanish, a
/// folder loses one trailing dot and the last name all trailing dots and spaces (an
/// Ahlheim mesh's `anz-oben.jpg.` opens as `anz-oben.jpg`); `..` is left for the caller.
pub fn windows_components(rel: &str) -> Vec<&str> {
    let parts: Vec<&str> = rel.trim().split(['/', '\\']).collect();
    let n = parts.len();
    let mut out = Vec::with_capacity(n);
    for (i, c) in parts.into_iter().enumerate() {
        if c.is_empty() || c == "." || c.contains(':') {
            continue;
        }
        if c == ".." {
            out.push(c);
            continue;
        }
        if c.trim_matches(['.', ' ']).is_empty() {
            continue;
        }
        // a path ending in a separator has an empty last part, so its last folder counts
        // as a folder here
        let c = if i + 1 == n { c.trim_end_matches(['.', ' ']) } else { c.strip_suffix('.').unwrap_or(c) };
        if !c.is_empty() {
            out.push(c);
        }
    }
    out
}

/// How names compare, as on Windows: without regard to case (letters beyond ASCII too -
/// `Bahnübergang` is `BAHNÜBERGANG`), and in one Unicode form (NFC). Also the form a bus's
/// file keys take in its fingerprint (the game's `lan_mods::identity`), so a name stored
/// decomposed on a Mac is the same file there as here.
pub fn name_key(name: &str) -> String {
    if name.is_ascii() {
        name.to_ascii_lowercase()
    } else {
        // (composed: a Russian stop's announcement `Лесной.wav` unpacked on a Mac is often
        // stored with `й` as `и` + a combining breve, and was never found)
        use unicode_normalization::UnicodeNormalization;
        name.nfc().collect::<String>().to_lowercase()
    }
}

/// The entries of one directory by [`name_key`]. A key holds more than one name only on a
/// case-sensitive file system that has two spellings of it side by side (`maps` and `Maps`,
/// after an add-on was unpacked next to the original folders) - a name means any of them.
type Listing = std::sync::Arc<std::collections::HashMap<String, Vec<std::ffi::OsString>>>;
// (read far more often than added to, by the loaders side by side: a lock for readers)
static LISTINGS: std::sync::OnceLock<std::sync::RwLock<std::collections::HashMap<PathBuf, Listing>>> = std::sync::OnceLock::new();
/// Bumped whenever content may have come or gone (a root added or taken away, a mod
/// installed while the game runs): the caches of what is where start afresh.
static CONTENT_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The content changed: directory listings are read again, and lookups that missed before
/// (see `content_generation`) are tried again.
pub fn content_changed() {
    CONTENT_GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    if let Some(l) = LISTINGS.get() {
        l.write().unwrap_or_else(|e| e.into_inner()).clear();
    }
}

/// Content came at `at` alone (a sealed mount of one bus, see `vfs::mount_sealed`): only the
/// listings of `at`, of what is inside it and of the folders above it are read again. Unlike
/// `content_changed`, the caches of lookups elsewhere (textures, depot files) are kept and
/// the generation stays: every bus fetched from another player emptied them all.
pub fn content_added_at(at: &Path) {
    if let Some(l) = LISTINGS.get() {
        l.write().unwrap_or_else(|e| e.into_inner()).retain(|dir, _| !(at.starts_with(dir) || dir.starts_with(at)));
    }
}

/// How often the content changed so far (a cache of lookups keeps its misses only as
/// long as this stays the same).
pub fn content_generation() -> u64 {
    CONTENT_GENERATION.load(std::sync::atomic::Ordering::SeqCst)
}

/// Pictures that came with a model of another tool's format (a `.glb`'s packed textures,
/// the `.png` beside an `.fbx`), by the bare file name its material gives: the texture
/// lookup takes them when the usual folders have no picture of that name. The material
/// keeps the bare name, so the `[matl]` lines of a model's `.cfg` find it as in OMSI.
static MODEL_TEXTURES: std::sync::RwLock<Option<std::collections::HashMap<String, PathBuf>>> = std::sync::RwLock::new(None);

pub fn register_model_texture(name: &str, path: PathBuf) {
    MODEL_TEXTURES.write().unwrap().get_or_insert_with(std::collections::HashMap::new).entry(name.to_lowercase()).or_insert(path);
}

/// The picture a model brought for the bare name `name` (see [`register_model_texture`]).
pub fn model_texture(name: &str) -> Option<PathBuf> {
    MODEL_TEXTURES.read().unwrap().as_ref()?.get(&name.trim().to_lowercase()).cloned()
}

/// The entries of a directory by [`name_key`], listed once.
fn dir_listing(dir: &Path) -> Listing {
    let cache = LISTINGS.get_or_init(|| std::sync::RwLock::new(std::collections::HashMap::new()));
    if let Some(l) = cache.read().unwrap().get(dir) {
        return l.clone();
    }
    let mut map: std::collections::HashMap<String, Vec<std::ffi::OsString>> = std::collections::HashMap::new();
    let entries = vfs::list_dir(dir).unwrap_or_default();
    for (name, _) in &entries {
        // every spelling, not the last one read: one that held the rest of a path used to be
        // dropped for another differing only in case, and its content went missing
        let names = map.entry(name_key(&name.to_string_lossy())).or_default();
        if !names.contains(name) {
            names.push(name.clone());
        }
    }
    // the file system's own order is no order of ours: sorted, the spelling a lookup
    // settles on is the same from one run to the next
    for names in map.values_mut() {
        if names.len() > 1 {
            names.sort();
        }
    }
    // the other spellings of names beyond ASCII (see `codepage::name_variants`), behind
    // every real name
    for (name, _) in &entries {
        for v in codepage::name_variants(&name.to_string_lossy()) {
            map.entry(name_key(&v)).or_insert_with(|| vec![name.clone()]);
        }
    }
    let l = std::sync::Arc::new(map);
    cache.write().unwrap().insert(dir.to_path_buf(), l.clone());
    l
}

/// Every entry of `dir` that `name` can mean, the spelling asked for first: the same name
/// without regard to case, or one of its other spellings (a name beyond ASCII read in
/// another code page on the way). More than one only where the file system holds two names
/// differing in case alone - which of them is meant is decided by what lies inside, so the
/// walk over a path ([`resolve_existing`]) tries each.
fn candidates_in_dir(dir: &Path, name: &str) -> Vec<std::ffi::OsString> {
    let listing = dir_listing(dir);
    let mut out = listing.get(&name_key(name)).cloned().unwrap_or_default();
    if out.is_empty() {
        out = codepage::name_variants(name)
            .iter()
            .find_map(|v| listing.get(&name_key(v)).cloned())
            .unwrap_or_default();
    }
    // the folder spells it exactly as asked: that one is meant
    if let Some(i) = out.iter().position(|n| n.as_os_str() == std::ffi::OsStr::new(name)) {
        out.swap(0, i);
    }
    out
}

/// The entry of `dir` that `name` means (the first of [`candidates_in_dir`]).
fn find_in_dir(dir: &Path, name: &str) -> Option<std::ffi::OsString> {
    candidates_in_dir(dir, name).into_iter().next()
}

/// Content roots, highest priority first: the game's own content folder next to its
/// binary (where mods are installed) and the OMSI 2 installation. A path asked for
/// relative to a folder of one root is looked for at the same place under the others,
/// so a mod's `Vehicles\\Foo\\foo.bus` or a replacement texture is found as if it had
/// been copied into the original installation.
static CONTENT_ROOTS: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());

/// Register a content root. Roots are searched in the order they were added.
pub fn add_content_root(root: PathBuf) {
    let mut r = CONTENT_ROOTS.lock().unwrap();
    if !r.iter().any(|x| x == &root) {
        r.push(root);
        drop(r);
        content_changed();
    }
}

/// Register a content root to be searched just before `before` (at the end when `before`
/// is not a root): an archive installed while the launcher runs goes in front of the
/// OMSI installation, where the game will have it on its next start.
pub fn add_content_root_before(root: PathBuf, before: &Path) {
    let mut r = CONTENT_ROOTS.lock().unwrap();
    if r.iter().any(|x| x == &root) {
        return;
    }
    match r.iter().position(|x| x == before) {
        Some(i) => r.insert(i, root),
        None => r.push(root),
    }
    drop(r);
    content_changed();
}

/// Register a content root searched before every other one (the content a LAN host sent
/// for the session: its files are the host's versions, and must win).
pub fn add_content_root_first(root: PathBuf) {
    let mut r = CONTENT_ROOTS.lock().unwrap();
    r.retain(|x| x != &root);
    r.insert(0, root);
    drop(r);
    content_changed();
}

/// Take a content root away again (a LAN session's content, when the session ends).
pub fn remove_content_root(root: &Path) {
    CONTENT_ROOTS.lock().unwrap().retain(|x| x != root);
    content_changed();
}

static SANDBOX_ROOTS: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());

/// Mark a content root as a sandbox: content received from another machine. It is read as
/// data (maps, vehicles, objects) like any other root, but nothing that runs code comes from
/// it - no plugins are loaded from a sandbox.
pub fn mark_sandbox(root: PathBuf) {
    let mut r = SANDBOX_ROOTS.lock().unwrap();
    if !r.contains(&root) {
        r.push(root);
    }
}

/// Does `path` lie in a sandbox root (see [`mark_sandbox`])?
pub fn is_sandbox(path: &Path) -> bool {
    SANDBOX_ROOTS.lock().unwrap().iter().any(|r| path.starts_with(r))
}

/// The first content root (in the order they are searched) that has `rel`, and the file
/// or folder found there (case-insensitive, as Windows reads the folder).
pub fn find_in_roots(rel: &str) -> Option<(PathBuf, PathBuf)> {
    let comps = windows_components(rel);
    content_roots().into_iter().find_map(|r| resolve_existing(&r, &comps).map(|p| (r, p)))
}

pub fn content_roots() -> Vec<PathBuf> {
    CONTENT_ROOTS.lock().unwrap().clone()
}

/// Every content root's copy of the folder `dir` (which lies under one of the roots) that
/// exists, highest priority first; just `dir` when it is under no root. A mod that adds
/// files to a stock folder (a repaint's `.cti` next to the original ones) puts them into
/// the same folder under the content folder (or an archive in `<content>/Archives`), and a
/// listing must see both.
pub fn mirrored_dirs(dir: &Path) -> Vec<PathBuf> {
    let roots = content_roots();
    let Some(suffix) = owner_root(dir, &roots).and_then(|r| dir.strip_prefix(r).ok()) else { return vec![dir.to_path_buf()] };
    // a real folder: its names as they are
    let rel: Vec<String> = suffix.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    let rel: Vec<&str> = rel.iter().map(|s| s.as_str()).collect();
    let mut out: Vec<PathBuf> = Vec::new();
    for r in &roots {
        let found = if rel.is_empty() { Some(r.clone()) } else { resolve_existing(r, &rel) };
        if let Some(p) = found.filter(|p| vfs::is_dir(p)) {
            if !out.contains(&p) {
                out.push(p);
            }
        }
    }
    if out.is_empty() {
        out.push(dir.to_path_buf());
    }
    out
}

/// The content root `path` belongs to: the deepest one that contains it. An archive
/// mounted from `<content>/Archives` lies inside the content folder, and a path inside the
/// archive is the archive's, not the content folder's.
fn owner_root<'a>(path: &Path, roots: &'a [PathBuf]) -> Option<&'a PathBuf> {
    roots.iter().filter(|r| path.starts_with(r)).max_by_key(|r| r.components().count())
}

/// The same relative location of `base` under `root` (None if `base` belongs to `root`
/// itself, or to no root at all).
fn mirrored_base(base: &Path, root: &Path, roots: &[PathBuf]) -> Option<PathBuf> {
    let owner = owner_root(base, roots)?;
    if owner == root {
        return None;
    }
    base.strip_prefix(owner).ok().map(|suffix| {
        // The suffix is spelled as in `base`'s root, which need not be how `root` spells it:
        // a mod that ships `Vehicles/Foo/texture/...` next to the original pack's
        // `Vehicles/Foo/Texture`. A case-sensitive file system (Linux) would not find the
        // original's folder under the mod's spelling, so walk the suffix case-insensitively.
        let parts: Vec<String> = suffix
            .components()
            .filter_map(|c| match c {
                std::path::Component::Normal(n) => Some(n.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect();
        let refs: Vec<&str> = parts.iter().map(String::as_str).collect();
        resolve_existing(root, &refs).unwrap_or_else(|| root.join(suffix))
    })
}

/// The vehicle pack (`Vehicles/<folder>`, relative to its root) that `rel` from `base` lands
/// in, when it does: the files of one vehicle refer to each other relative to its folder.
fn vehicle_package(base: &Path, owner: &Path, rel: &[&str]) -> Option<String> {
    let suffix = base.strip_prefix(owner).ok()?;
    let mut parts: Vec<String> = suffix
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(n) => Some(n.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    for comp in rel {
        if *comp == ".." {
            parts.pop();
        } else {
            parts.push(comp.to_string());
        }
    }
    if parts.len() >= 3 && parts[0].eq_ignore_ascii_case("Vehicles") {
        Some(format!("Vehicles/{}", parts[1]))
    } else {
        None
    }
}

/// The vehicle definitions (`.bus`, `.ovh`, `.zug`) lying directly in a pack folder.
fn package_definitions(root: &Path, package: &str) -> std::collections::HashSet<String> {
    let comps: Vec<&str> = package.split('/').collect();
    let Some(dir) = resolve_existing(root, &comps) else {
        return Default::default();
    };
    vfs::list_dir(&dir)
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, is_dir)| !is_dir)
        .map(|(n, _)| name_key(&n.to_string_lossy()))
        .filter(|n| n.ends_with(".bus") || n.ends_with(".ovh") || n.ends_with(".zug"))
        .collect()
}

/// Whether the copy of vehicle pack `package` under the higher-priority root `over` patches
/// the one under `owner` (the root the vehicle was loaded from): its files replace the
/// owner's, the way files copied over an installation would. A pack with vehicles the
/// owner's copy does not know is another *version* of the pack - a map archive bundles its
/// own Citaro Facelift next to the installed one, with scripts that belong together - and
/// taking single files from it broke the bus they did not come from (the Ahlheim Citaro's
/// VDV and IBIS scripts ran on the wrong varlists, every display stayed dark). Such a copy
/// is only asked for files the owner's pack does not have.
fn overlays_package(over: &Path, owner: &Path, package: &str) -> bool {
    type Key = (PathBuf, PathBuf, String);
    static CACHE: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<Key, bool>>> =
        std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let key = (over.to_path_buf(), owner.to_path_buf(), package.to_ascii_lowercase());
    if let Some(v) = cache.lock().unwrap().get(&key) {
        return *v;
    }
    // (a root without the pack at all is only where the lookup starts: `--bus` is resolved
    // from the installation's root)
    let comps: Vec<&str> = package.split('/').collect();
    let owner_has = resolve_existing(owner, &comps).map(|d| vfs::is_dir(&d)).unwrap_or(false);
    let theirs = package_definitions(over, package);
    let ok = !owner_has || theirs.is_empty() || theirs.is_subset(&package_definitions(owner, package));
    cache.lock().unwrap().insert(key, ok);
    ok
}

/// The vehicle pack a missing file belongs to, when that whole pack is installed in no
/// content root: a mod that borrows parts from another (the Ahlheim Citaro's Faremaster
/// ticket machine and dashboard come from `Vehicles/Urbino_II`) is missing them because the
/// other pack is not installed, not because of a broken path, and the player can fix that.
pub fn missing_vehicle_pack(path: &Path) -> Option<String> {
    let roots = content_roots();
    let owner = owner_root(path, &roots)?.clone();
    let suffix = path.strip_prefix(&owner).ok()?;
    let mut comps = suffix.components().filter_map(|c| match c {
        std::path::Component::Normal(n) => Some(n.to_string_lossy().into_owned()),
        _ => None,
    });
    let first = comps.next()?;
    let pack = comps.next()?;
    if !first.eq_ignore_ascii_case("Vehicles") || comps.next().is_none() {
        return None;
    }
    let installed = roots.iter().any(|r| {
        resolve_existing(r, &["Vehicles", pack.as_str()]).map(|d| vfs::is_dir(&d)).unwrap_or(false)
    });
    (!installed).then_some(pack)
}

/// Every content root's version of a folder given relative to a root (existing ones only),
/// highest priority first - for listing maps, vehicles, weathers across the installation
/// and the installed mods.
pub fn content_dirs(rel: &str) -> Vec<PathBuf> {
    let rel = windows_components(rel);
    content_roots().iter().filter_map(|r| resolve_existing(r, &rel)).filter(|p| vfs::is_dir(p)).collect()
}

/// Directory entries of `rel` (relative to a content root) merged over all roots; an
/// entry of a higher-priority root hides the same name lower down.
pub fn read_dir_merged(rel: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for d in content_dirs(rel) {
        for (name, _) in vfs::list_dir(&d).unwrap_or_default() {
            if seen.insert(name_key(&name.to_string_lossy())) {
                out.push(d.join(name));
            }
        }
    }
    out
}

/// The file a content file names (`rel`, relative to the folder `base`), found the way
/// Windows finds it: see [`windows_components`] for the spelling rules, names compare without
/// regard to case, and a mod's copy under a content root of higher priority comes first.
pub fn resolve_path(base: &Path, rel: &str) -> PathBuf {
    let rel = windows_components(rel);
    let rel = rel.as_slice();
    // a mod's copy first: the same place under a content root of higher priority than the
    // one `base` belongs to
    let roots = content_roots();
    let floor = owner_root(base, &roots).cloned();
    let owner = floor.clone().filter(|_| roots.len() > 1);
    if let Some(owner) = &owner {
        let package = vehicle_package(base, owner, rel);
        for r in roots.iter().take_while(|r| *r != owner) {
            // another version of the same vehicle pack does not patch this one (see
            // `overlays_package`); it is only asked below, for what this one lacks
            if let Some(pkg) = &package {
                if !overlays_package(r, owner, pkg) {
                    continue;
                }
            }
            if let Some(mb) = mirrored_base(base, r, &roots) {
                if let Some(p) = resolve_existing(&mb, rel) {
                    return p;
                }
            }
        }
    }
    let mut cur = base.to_path_buf();
    for comp in rel {
        if *comp == ".." {
            pop_within(&mut cur, floor.as_deref());
            continue;
        }
        // the folder's listing, kept from the first time, has the name spelled just so:
        // the path as asked for, which the file system would only confirm (a look on the
        // disk for every component of every path was most of a lookup's time)
        if dir_listing(&cur).get(&name_key(comp)).is_some_and(|names| names.iter().any(|n| n.as_os_str() == std::ffi::OsStr::new(comp))) {
            cur.push(comp);
            continue;
        }
        let direct = cur.join(comp);
        if vfs::exists(&direct) {
            cur = direct;
            continue;
        }
        // case-insensitive lookup through a per-directory listing kept from the first
        // time (scanning the folder afresh for every miss made texture lookups crawl)
        let found = find_in_dir(&cur, comp).map(|n| cur.join(n));
        cur = found.unwrap_or(direct);
    }
    // The walk above takes each component on its own, which is enough until a folder has two
    // names differing in case alone: it then follows the spelling asked for, and the rest of
    // the path may lie under the other one. Looking the whole path up at once weighs the
    // spellings against what is inside them (see `resolve_existing`).
    if !vfs::exists(&cur) {
        if let Some(p) = resolve_existing(base, rel) {
            return p;
        }
    }
    if let (Some(owner), false) = (&owner, vfs::exists(&cur)) {
        // not in `base`'s root either: another root may still have it - a lower-priority
        // one, or another version of the pack that was passed over above
        for r in roots.iter().filter(|r| *r != owner) {
            if let Some(mb) = mirrored_base(base, r, &roots) {
                if let Some(p) = resolve_existing(&mb, rel) {
                    return p;
                }
            }
        }
    }
    cur
}

/// What an installation of the original OMSI 2 must have for openOMSI to run on it: the
/// game and the stock content every player gets (both stock maps, the buses, people, fonts,
/// weather, inputs). A folder without them (openOMSI's own content folder, a mod pack, a
/// half-copied installation) is not a base to play on - everybody plays on the same original
/// content.
pub const ORIGINAL_ESSENTIALS: &[&str] = &[
    "Omsi.exe",
    "maps/Grundorf/global.cfg",
    "maps/Berlin-Spandau/global.cfg",
    "Vehicles/MAN_SD200",
    "Vehicles/MAN_SD202",
    "Vehicles/MAN_NL_NG",
    "Sceneryobjects",
    "Splines",
    "Texture",
    "Fonts",
    "Humans",
    "Weather",
    "Inputs",
    "envir.cfg",
];

/// The key assignment of the OMSI installation at `root`: its `Inputs/keyboard.cfg`, or
/// where it has none (it is the player's own, and not every copy comes with one) the
/// standard `keyboard_reset.cfg`, which OMSI loads then too.
pub fn original_keyboard_cfg(root: &Path) -> PathBuf {
    resolve_existing(root, &["Inputs", "keyboard.cfg"])
        .or_else(|| resolve_existing(root, &["Inputs", "keyboard_reset.cfg"]))
        .unwrap_or_else(|| root.join("Inputs").join("keyboard.cfg"))
}

/// The essentials (see [`ORIGINAL_ESSENTIALS`]) that `root` lacks; empty for a complete
/// original installation. openOMSI's content folder never counts as one.
pub fn missing_original_essentials(root: &Path) -> Vec<String> {
    // (a folder with Omsi.exe in it is the game's, even marked: openOMSI unpacked into the OMSI
    // folder made it its content folder once - see `content_folder_of` - and every start after
    // that said the game was not there)
    let marked = root.join(CONTENT_MARKER).exists() || root.join(LEGACY_CONTENT_MARKER).exists();
    if marked && resolve_existing(root, &["Omsi.exe"]).is_none() {
        return vec![format!("{} (this is the openOMSI content folder, not the original game)", root.display())];
    }
    ORIGINAL_ESSENTIALS
        .iter()
        .filter(|rel| {
            let comps: Vec<&str> = rel.split('/').collect();
            // case-insensitive, as Windows reads the folder
            resolve_existing(root, &comps).is_none()
        })
        .map(|s| s.to_string())
        .collect()
}

/// The top-level content folders openOMSI recognises. Most use OMSI 2's original spelling;
/// `HOFs` is an openOMSI extension for depot files shared by every vehicle. A content
/// folder is laid out with these names, so the mod installer and mounted archives can merge
/// them into the same virtual installation.
pub const CONTENT_FOLDERS: &[&str] = &[
    "Vehicles", "HOFs", "maps", "Sceneryobjects", "Splines", "Texture", "Fonts", "Plugins", "TicketPacks", "Drivers", "Weather", "Announcements", "Humans", "Money", "Scripts", "Trains", "Situations", "Inputs", "Sounds",
];

/// Marker file of an openOMSI content folder (so it is never mistaken for the OMSI 2
/// installation itself).
pub const CONTENT_MARKER: &str = ".openomsi-content";
/// The marker of a content folder made before the project was called openOMSI.
pub const LEGACY_CONTENT_MARKER: &str = ".omsi-rewrite-content";

/// Move the data of a version from before the rename (`~/.omsi-rewrite`,
/// `~/.omsi-rewrite-root`) to its new place (`~/.openomsi`, `~/.openomsi-root`), once.
/// (First thing at the start of every program: an unusable `HOME` is dropped here, see
/// [`drop_unusable_home`].)
pub fn migrate_legacy_data_dir() {
    drop_unusable_home();
    let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(std::path::PathBuf::from) else {
        return;
    };
    for (old, new) in [(".omsi-rewrite", ".openomsi"), (".omsi-rewrite-root", ".openomsi-root")] {
        let (old, new) = (home.join(old), home.join(new));
        if old.exists() && !new.exists() {
            let _ = std::fs::rename(&old, &new);
        }
    }
}

/// On Windows a `HOME` variable some other program set for itself (a Unix-style path, a
/// network drive that is not connected) is no folder to keep openOMSI's data in: the
/// launcher's settings went nowhere, and the OMSI folder chosen under Setup was forgotten
/// as soon as it was saved - the lists stayed empty. Such a `HOME` is dropped for this
/// program (and the game it starts), which then uses `USERPROFILE` as without one.
pub fn drop_unusable_home() {
    if !cfg!(windows) {
        return;
    }
    let Some(h) = std::env::var_os("HOME") else { return };
    let p = std::path::PathBuf::from(&h);
    let usable = p.is_absolute() && p.is_dir() && std::fs::create_dir_all(p.join(".openomsi")).is_ok();
    if !usable && std::env::var_os("USERPROFILE").is_some() {
        unsafe { std::env::remove_var("HOME") };
    }
}

/// openOMSI's content folder for a program in `dir`: that folder - unless it is the original
/// OMSI 2 folder itself (openOMSI unpacked into it), which openOMSI never writes to: then the
/// `openOMSI` folder inside it.
pub fn content_folder_of(dir: &Path) -> PathBuf {
    if resolve_existing(dir, &["Omsi.exe"]).is_some() && resolve_existing(dir, &["maps"]).is_some() {
        dir.join("openOMSI")
    } else {
        dir.to_path_buf()
    }
}

/// A folder of programs - macOS's `/Applications` or `~/Applications`, where an
/// openOMSI.app is usually copied to - is no place for the content folder: the game laid
/// OMSI's folders (Vehicles, maps, Mods ...) out among the user's applications (#1043). The
/// content folder then lives in the user's data folder instead - unless content was already
/// installed there by an older version, which then stays where the player put it.
pub fn is_programs_folder(dir: &Path) -> bool {
    dir.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("Applications")) && !holds_content(dir)
}

/// Whether `dir` holds installed OMSI content: a vehicle or scenery folder with something in
/// it, a map (a folder with its global.cfg - the game itself leaves only a `laststn.osn`
/// there), or a mod beside the `Mods` folder's own README.
fn holds_content(dir: &Path) -> bool {
    let map = resolve_existing(dir, &["maps"]).and_then(|d| std::fs::read_dir(d).ok()).is_some_and(|mut it| {
        it.any(|e| e.is_ok_and(|e| resolve_existing(&e.path(), &["global.cfg"]).is_some()))
    });
    let filled = |name: &str, skip: &[&str]| {
        resolve_existing(dir, &[name]).and_then(|d| std::fs::read_dir(d).ok()).is_some_and(|mut it| {
            it.any(|e| e.is_ok_and(|e| {
                let n = e.file_name().to_string_lossy().to_string();
                !n.starts_with('.') && !skip.iter().any(|s| n.eq_ignore_ascii_case(s))
            }))
        })
    };
    map || ["Vehicles", "Sceneryobjects", "Splines"].iter().any(|n| filled(n, &[])) || filled("Mods", &["README.txt"])
}

/// Check if a directory is writable by attempting to create and remove a probe file.
pub fn is_writable(dir: &Path) -> bool {
    let probe = dir.join(".openomsi-write-test");
    let ok = std::fs::write(&probe, b"x").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// The game's own content folder, where it writes what it makes (drivers, saves, journeys):
/// `OMSI_CONTENT`, else the content folder beside the program (beside an .app bundle) when it
/// may be written, else `~/.openomsi/content`.
pub fn content_dir() -> Option<PathBuf> {
    if let Some(d) = crate::flags::OMSI_CONTENT.os() {
        return Some(PathBuf::from(d));
    }
    let cand = content_folder_of_program(&std::env::current_exe().ok()?)?;
    if !is_programs_folder(&cand) && (cand.exists() || std::fs::create_dir_all(&cand).is_ok()) && is_writable(&cand) {
        Some(cand)
    } else {
        let fallback = user_data::base_dir()?.join("content");
        let _ = ensure_content_layout(&fallback);
        Some(fallback)
    }
}

fn content_folder_of_program(exe: &Path) -> Option<PathBuf> {
    let dir = exe.parent()?;
    // inside an .app bundle the binary sits in Contents/MacOS: use the folder beside the bundle
    let dir = if dir.ends_with("Contents/MacOS") { dir.parent()?.parent()?.parent()? } else { dir };
    Some(content_folder_of(dir))
}

const OWN_INPUTS: [&str; 2] = ["keyboard.cfg", "gamectrler.cfg"];

pub fn own_input(name: &str) -> Option<PathBuf> {
    if crate::flags::OMSI_CONTENT.os().is_some() {
        return content_dir().map(|c| c.join("Inputs").join(name));
    }
    Some(user_data::base_dir()?.join("Inputs").join(name))
}

pub fn input_in_use(name: &str) -> Option<PathBuf> {
    newest([own_input(name), content_dir().map(|c| c.join("Inputs").join(name))])
}

pub fn adopt_own_inputs(previous_program: Option<&Path>) {
    if crate::flags::OMSI_CONTENT.os().is_some() {
        return;
    }
    let home = user_data::base_dir().map(|d| d.join("content"));
    let here = std::env::current_exe().ok().and_then(|e| content_folder_of_program(&e));
    for name in OWN_INPUTS {
        let Some(own) = own_input(name).filter(|p| !p.exists()) else { continue };
        let mut dirs: Vec<PathBuf> = [content_dir(), previous_program.and_then(content_folder_of_program), home.clone()].into_iter().flatten().collect();
        dirs.extend(here.as_deref().map(sibling_installs).unwrap_or_default());
        if let Some(found) = newest(dirs.iter().map(|d| Some(d.join("Inputs").join(name)))) {
            adopt_input(&own, &found);
        }
    }
}

fn sibling_installs(here: &Path) -> Vec<PathBuf> {
    let Some(parent) = here.parent() else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(parent) else { return Vec::new() };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|d| ["openomsi", "openomsi.exe", "openOMSI.app"].iter().any(|p| d.join(p).exists()))
        .map(|d| content_folder_of(&d))
        .collect()
}

fn newest(paths: impl IntoIterator<Item = Option<PathBuf>>) -> Option<PathBuf> {
    let mut best: Option<(PathBuf, std::time::SystemTime)> = None;
    for p in paths.into_iter().flatten() {
        let Some(t) = std::fs::metadata(&p).ok().filter(|m| m.is_file()).and_then(|m| m.modified().ok()) else { continue };
        if best.as_ref().is_none_or(|(_, b)| t > *b) {
            best = Some((p, t));
        }
    }
    best.map(|(p, _)| p)
}

fn adopt_input(own: &Path, from: &Path) {
    let (Some(dir), Some(name)) = (own.parent(), own.file_name()) else { return };
    if own.exists() || own == from {
        return;
    }
    let tmp = dir.join(format!("{}.{}.tmp", name.to_string_lossy(), std::process::id()));
    let _ = std::fs::create_dir_all(dir).and_then(|_| std::fs::copy(from, &tmp)).and_then(|_| {
        std::fs::hard_link(&tmp, own).or_else(|e| if e.kind() == std::io::ErrorKind::AlreadyExists { Ok(()) } else { std::fs::rename(&tmp, own) })
    });
    let _ = std::fs::remove_file(&tmp);
}

/// Create the content folder layout at `dir` (idempotent).
pub fn ensure_content_layout(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    // (versions up to 0.2.9 made it `Sound`; OMSI 2's folder is `Sounds`)
    let (old, new) = (dir.join("Sound"), dir.join("Sounds"));
    if old.is_dir() && !new.exists() {
        let _ = std::fs::rename(&old, &new);
    } else if old.is_dir() {
        let _ = std::fs::remove_dir(&old); // only when empty
    }
    for f in CONTENT_FOLDERS {
        std::fs::create_dir_all(dir.join(f))?;
    }
    std::fs::create_dir_all(dir.join("Mods"))?;
    let marker = dir.join(CONTENT_MARKER);
    if !marker.exists() {
        std::fs::write(&marker, "This folder holds openOMSI's own content and installed mods, laid out like the original game.\nDrop a mod folder or zip into Mods/ and the launcher sorts it into place.\n")?;
    }
    let readme = dir.join("Mods").join("README.txt");
    if !readme.exists() {
        std::fs::write(&readme, "Put a mod here (a folder or a .zip) and start the launcher: it works out what the mod is\n(a bus, a map, scenery objects, splines, textures, fonts ...) and sorts it into the folders\nnext to this one. The original OMSI 2 folder is never written to.\n")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;

/// The `OMSI_*` switches of the environment, read once: the frame asked for dozens of them
/// every time round (`getenv` takes a lock and walks the environment - 2 % of a frame's CPU
/// time in `sync_materials` alone). The environment is not changed while the game runs.
pub mod env {
    use std::collections::HashMap;
    use std::ffi::OsString;
    use std::sync::{OnceLock, RwLock};

    fn cache() -> &'static RwLock<HashMap<String, Option<OsString>>> {
        static C: OnceLock<RwLock<HashMap<String, Option<OsString>>>> = OnceLock::new();
        C.get_or_init(Default::default)
    }

    #[derive(Default)]
    struct Fnv(u64);

    impl std::hash::Hasher for Fnv {
        fn finish(&self) -> u64 {
            self.0
        }
        fn write(&mut self, bytes: &[u8]) {
            let mut h = if self.0 == 0 { 0xcbf2_9ce4_8422_2325 } else { self.0 };
            for b in bytes {
                h = (h ^ *b as u64).wrapping_mul(0x0100_0000_01b3);
            }
            self.0 = h;
        }
    }

    type Local = std::cell::RefCell<HashMap<String, Option<OsString>, std::hash::BuildHasherDefault<Fnv>>>;

    thread_local! {
        static LOCAL: Local = Default::default();
    }

    pub fn var_os(name: &str) -> Option<OsString> {
        if let Some(v) = LOCAL.with(|l| l.borrow().get(name).cloned()) {
            return v;
        }
        let v = shared(name);
        LOCAL.with(|l| l.borrow_mut().insert(name.to_string(), v.clone()));
        v
    }

    fn shared(name: &str) -> Option<OsString> {
        if let Some(v) = cache().read().ok().and_then(|c| c.get(name).cloned()) {
            return v;
        }
        let v = std::env::var_os(name);
        if let Ok(mut c) = cache().write() {
            c.insert(name.to_string(), v.clone());
        }
        v
    }

    pub fn var(name: &str) -> Result<String, std::env::VarError> {
        match var_os(name) {
            Some(s) => s.into_string().map_err(std::env::VarError::NotUnicode),
            None => Err(std::env::VarError::NotPresent),
        }
    }
}
