//! OMSI plugins, driven the way OMSI drives them:
//!
//! * the original finds every `*.opl` under `<OMSI>\plugins` (recursively) and reads
//!   `[dll]` (the library, relative to `plugins\`), `[varlist]`, `[stringvarlist]`,
//!   `[systemvarlist]` and `[triggers]` - each a count, then that many names.
//! * the original loads the library and looks up `PluginStart`, `PluginFinalize` (both
//!   required: without them the plugin is not loaded) and `AccessVariable`,
//!   `AccessTrigger`, `AccessSystemVariable`, `AccessStringVariable` (each optional, a
//!   warning when missing), then calls `PluginStart(AOwner)`. `PluginFinalize` is called
//!   when the game ends.
//! * Every frame, for each plugin in turn:
//!   every system variable of its list (`AccessSystemVariable(index, var value: Single,
//!   var write: Boolean)`, the value written back when `write` comes back true); then,
//!   with a player vehicle, its vehicle variables (`AccessVariable`, the same shape), its
//!   string variables (`AccessStringVariable(index, PWideChar, var write)`: a buffer of
//!   length + 1 wide characters holding the text and its terminating zero, read back when
//!   `write` is set) and its triggers (`AccessTrigger(index, var active: Boolean)`, starting
//!   from false each frame: a change from the last frame's state is a key going down -
//!   the trigger fires - or coming up - `<trigger>_off`). Names nobody knows are skipped.
//!   All are `stdcall`; the index is the position in the plugin's own list (a Word).
//!
//! openOMSI adds one optional procedure OMSI does not have: `AccessScriptTexture(index,
//! width, height: Integer; pixels: Pointer; var write: Boolean)`, called after the
//! triggers for each entry of the `.opl`'s `[scripttexturelist]` (a count, then that many
//! `[scripttexture]` numbers of the player's vehicle, 0-based as `[useScriptTexture]`).
//! `pixels` is `width` x `height` RGBA bytes, top row first, straight alpha; the buffer
//! belongs to that entry and keeps what the plugin drew last (zeroed when the texture's
//! size changes), so a plugin may redraw only what changed. When `write` comes back true
//! the picture replaces the texture's. A second optional procedure,
//! `AccessScriptTexturePointer(index: Word; kind: Integer; x, y: Single)`, hears the mouse on
//! those textures: the game finds where a click lands on the mesh (as for `[htmltexture]`)
//! and calls it before `AccessScriptTexture` in the next frame, once per press (kind 0),
//! move while pressed (1) and release (2), with `x`/`y` in the texture's pixels from its
//! top left (the game gathers the events in a `TexturePointer`). OMSI skips the unknown tag and never asks for the
//! procedure, so one DLL and `.opl` serve both games.
//!
//! A third, `AccessTicketRequest` (see [`TicketCall`]), hears each passenger boarding the
//! player's bus once, as they decided about their ticket, and may change it before they ask
//! for it at the cash desk (the Lua plugins' `passenger_ticket` and `people.set_ticket`). (OMSI 2 itself never gave plugins
//! the script textures: tools like OmsiHook reach them in Omsi.exe's memory, which does not
//! exist here.)
//!
//! A plugin is a Windows DLL, almost always 32-bit. A library the running process can load
//! (same system and architecture) is loaded in-process; any other runs in
//! `omsi-plugin-host`, a small program built for 32-bit Windows that loads the DLL and
//! answers over its standard input and output (started directly on Windows and through
//! Wine elsewhere). The frame is one round trip.

pub mod api;
pub mod data;
pub mod io;
pub mod lua;
pub mod ui;
pub mod oop;
pub mod wasm;
pub mod wasm_plugin;

pub use io::*;

use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

/// One `.opl` file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Opl {
    /// `[dll]`: the library, relative to the plugins folder.
    pub dll: String,
    pub vars: Vec<String>,
    pub string_vars: Vec<String>,
    pub system_vars: Vec<String>,
    pub triggers: Vec<String>,
    /// `[scripttexturelist]` (openOMSI only): numbers of the vehicle's script textures,
    /// as written (an entry that is no number is skipped, keeping the others' positions).
    pub script_textures: Vec<String>,
}

/// Read an `.opl` file's text: the tags OMSI knows, each list a count and that many
/// lines (a count that is no number reads as 0, as `StrToInt` would refuse it).
pub fn parse_opl(text: &str) -> Opl {
    let mut o = Opl::default();
    let mut lines = text.lines().map(|l| l.trim_end_matches('\r'));
    while let Some(l) = lines.next() {
        let list = match l.trim() {
            "[dll]" => {
                o.dll = lines.next().unwrap_or("").trim().to_string();
                continue;
            }
            "[varlist]" => &mut o.vars,
            "[stringvarlist]" => &mut o.string_vars,
            "[systemvarlist]" => &mut o.system_vars,
            "[triggers]" => &mut o.triggers,
            "[scripttexturelist]" => &mut o.script_textures,
            _ => continue,
        };
        let n: usize = lines.next().and_then(|c| c.trim().parse().ok()).unwrap_or(0);
        for _ in 0..n {
            match lines.next() {
                Some(name) => list.push(name.trim().to_string()),
                None => break,
            }
        }
    }
    o
}

/// Every `.opl` under `dir` (a folder's subfolders before its files, names compared without
/// case - the order FindFilesRecursive gives).
pub fn find_opls(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return out };
    let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    entries.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_uppercase()).unwrap_or_default());
    for p in entries.iter().filter(|p| p.is_dir()) {
        out.extend(find_opls(p));
    }
    for p in entries.iter().filter(|p| p.is_file()) {
        if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("opl")) {
            out.push(p.clone());
        }
    }
    out
}

/// A file name as Windows finds it: the path as given, or else the entry of each folder
/// that equals it without regard to case.
pub fn resolve_path(base: &Path, rel: &str) -> Option<PathBuf> {
    let mut cur = base.to_path_buf();
    for part in rel.split(['\\', '/']).filter(|s| !s.is_empty()) {
        let direct = cur.join(part);
        if direct.exists() {
            cur = direct;
            continue;
        }
        let found = std::fs::read_dir(&cur).ok()?.flatten().map(|e| e.path()).find(|p| {
            p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(part))
        })?;
        cur = found;
    }
    Some(cur)
}

/// What one frame hands a plugin and what it gives back.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Frame {
    /// (index in the plugin's list, value); values written back replace them.
    pub system: Vec<(u16, f32)>,
    pub vars: Vec<(u16, f32)>,
    pub strings: Vec<(u16, String)>,
    /// Indices of the triggers asked; `triggers_active` comes back in the same order.
    pub triggers: Vec<u16>,
    /// (index in `[scripttexturelist]`, width, height) of the script textures asked.
    pub textures: Vec<(u16, u32, u32)>,
    /// Mouse events on those textures since the last frame, in order: (index in
    /// `[scripttexturelist]`, kind - [`POINTER_DOWN`], [`POINTER_MOVE`], [`POINTER_UP`] -,
    /// x, y in the texture's pixels from its top left).
    pub pointer: Vec<(u16, u8, f32, f32)>,
    /// The passengers who decided about their ticket since the last frame
    /// (`AccessTicketRequest`).
    pub tickets: Vec<TicketCall>,
}

/// One call of `AccessTicketRequest` (openOMSI only):
///
/// ```pascal
/// procedure AccessTicketRequest(person: Cardinal; age: Single;
///   stop, destination: PWideChar; stopNumber, destinationNumber: Integer;
///   var kind, ticket: Integer; var price: Single; name, voice: PWideChar;
///   var write: Boolean); stdcall;
/// ```
///
/// `stop` and `destination` are the timetable's names of the stop they board at and the one
/// they ride to ("" when not known), the numbers where those are in the duty's trip (from 1;
/// 0 not in it). `kind` 0 nothing (a pass), 1 stamp, 2 buy; `ticket` the ticket pack's (from
/// 1; 0 none); `name` a buffer of [`TICKET_TEXT`] wide characters holding the ticket's name,
/// `voice` one as long, empty: a line of the passenger's voice folder said at the desk
/// instead of `Ticket_<n>_<k>`. With `write` set, what the plugin changed of them is taken
/// (the kind only before the passenger is in the bus).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TicketCall {
    pub person: u32,
    pub age: f32,
    pub stop: String,
    pub destination: String,
    pub stop_number: i32,
    pub destination_number: i32,
    pub kind: i32,
    pub ticket: i32,
    pub price: f32,
    pub name: String,
}

/// What a plugin wrote back to `AccessTicketRequest`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TicketAnswer {
    pub kind: i32,
    pub ticket: i32,
    pub price: f32,
    pub name: String,
    pub voice: String,
}

/// Wide characters of `AccessTicketRequest`'s `name` and `voice` buffers, the terminating
/// zero included.
pub const TICKET_TEXT: usize = 256;

impl TicketCall {
    pub fn of(a: &TicketAsk) -> TicketCall {
        let number = |k: Option<usize>| k.map_or(0, |k| i32::try_from(k + 1).unwrap_or(0));
        TicketCall {
            person: a.person,
            age: a.age,
            stop: a.stop_name.clone().unwrap_or_default(),
            destination: a.destination.clone().unwrap_or_default(),
            stop_number: number(a.stop_index),
            destination_number: number(a.destination_index),
            kind: a.kind.code(),
            ticket: a.ticket as i32,
            price: a.price,
            name: a.name.clone(),
        }
    }

    /// What the answer changes: only what differs from the call (a voice when one is given).
    pub fn change(&self, a: &TicketAnswer) -> TicketChange {
        TicketChange {
            kind: (a.kind != self.kind).then(|| TicketKind::from_code(a.kind)).flatten(),
            ticket: (a.ticket != self.ticket).then(|| u8::try_from(a.ticket).ok()).flatten(),
            name: (a.name != self.name).then(|| a.name.clone()),
            price: (a.price.to_bits() != self.price.to_bits() && a.price.is_finite()).then_some(a.price.max(0.0)),
            voice: (!a.voice.trim().is_empty()).then(|| a.voice.clone()),
            tag: None,
            payment: None,
        }
    }
}

/// `AccessScriptTexturePointer`'s `kind`: the button went down on the texture.
pub const POINTER_DOWN: u8 = 0;
/// The cursor moved while the button is held (it started on this texture).
pub const POINTER_MOVE: u8 = 1;
/// The button came up (where the cursor last was on the texture).
pub const POINTER_UP: u8 = 2;

/// The mouse on the script textures the `.opl` plugins listen to, as the game gathers it
/// between their frames (it hands `pending` over in `Plugins::pointer` at the frame).
#[derive(Debug, Default)]
pub struct TexturePointer {
    /// The textures listened to (`Plugins::pointer_textures`, refreshed every frame).
    pub pages: Vec<usize>,
    /// (texture number, kind, u, v from 0 to 1 across the texture, v down) in order.
    pub pending: Vec<(usize, u8, f32, f32)>,
}

impl TexturePointer {
    /// An event on texture `page`: false when no plugin listens to it. A move replaces a move
    /// not yet handed over; a release is always kept (a plugin must not see a button stuck).
    pub fn push(&mut self, page: usize, kind: u8, u: f32, v: f32) -> bool {
        if !self.pages.contains(&page) {
            return false;
        }
        if kind == POINTER_MOVE && self.pending.last().is_some_and(|l| l.0 == page && l.1 == POINTER_MOVE) {
            self.pending.pop();
        }
        if self.pending.len() < 256 || kind == POINTER_UP {
            self.pending.push((page, kind, u, v));
        }
        true
    }
}

/// The answers of a frame: per entry of the `Frame`, whether the plugin wrote it and the value.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Reply {
    pub system: Vec<Option<f32>>,
    pub vars: Vec<Option<f32>>,
    pub strings: Vec<Option<String>>,
    pub triggers_active: Vec<bool>,
    /// Per entry of `Frame::textures`: the picture the plugin wrote (width x height RGBA).
    pub textures: Vec<Option<Vec<u8>>>,
    /// Per entry of `Frame::tickets`: what the plugin wrote back.
    pub tickets: Vec<Option<TicketAnswer>>,
}

/// Which of the optional procedures the library has.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Procs {
    pub variable: bool,
    pub trigger: bool,
    pub system: bool,
    pub string: bool,
    /// `AccessScriptTexture` (openOMSI only).
    pub texture: bool,
    /// `AccessScriptTexturePointer` (openOMSI only).
    pub pointer: bool,
    /// `AccessTicketRequest` (openOMSI only).
    pub ticket: bool,
}

impl Procs {
    /// As the host reports them after `START`.
    pub fn flags(&self) -> u8 {
        self.variable as u8 | (self.trigger as u8) << 1 | (self.system as u8) << 2 | (self.string as u8) << 3 | (self.texture as u8) << 4 | (self.pointer as u8) << 5 | (self.ticket as u8) << 6
    }

    pub fn from_flags(flags: u8) -> Procs {
        Procs { variable: flags & 1 != 0, trigger: flags & 2 != 0, system: flags & 4 != 0, string: flags & 8 != 0, texture: flags & 16 != 0, pointer: flags & 32 != 0, ticket: flags & 64 != 0 }
    }
}

type StartFn = unsafe extern "system" fn(*mut std::ffi::c_void);
type FinalizeFn = unsafe extern "system" fn();
type AccessFloatFn = unsafe extern "system" fn(u16, *mut f32, *mut u8);
type AccessStringFn = unsafe extern "system" fn(u16, *mut u16, *mut u8);
type AccessTriggerFn = unsafe extern "system" fn(u16, *mut u8);
type AccessTextureFn = unsafe extern "system" fn(u16, i32, i32, *mut u8, *mut u8);
type AccessPointerFn = unsafe extern "system" fn(u16, i32, f32, f32);
type AccessTicketFn = unsafe extern "system" fn(u32, f32, *const u16, *const u16, i32, i32, *mut i32, *mut i32, *mut f32, *mut u16, *mut u16, *mut u8);

/// The largest script texture side handed to a plugin (a bad size must not allocate
/// gigabytes, here or in the host).
pub const MAX_TEXTURE_SIDE: u32 = 8192;

/// A plugin library loaded into this process.
pub struct Library {
    _lib: libloading::Library,
    start: StartFn,
    finalize: FinalizeFn,
    variable: Option<AccessFloatFn>,
    trigger: Option<AccessTriggerFn>,
    system: Option<AccessFloatFn>,
    string: Option<AccessStringFn>,
    texture: Option<AccessTextureFn>,
    pointer: Option<AccessPointerFn>,
    ticket: Option<AccessTicketFn>,
    /// One drawing buffer per `[scripttexturelist]` entry, kept from frame to frame.
    buffers: Vec<Vec<u8>>,
}

impl Library {
    /// Load the library and look its procedures up.
    pub fn load(path: &Path) -> Result<Library, String> {
        // SAFETY: loading a library runs its initialisers; that is what a plugin is for
        let lib = unsafe { libloading::Library::new(path) }.map_err(|e| {
            // (libloading 0.9 keeps the system's own reason - dlerror's text - in `source`)
            use std::error::Error as _;
            match e.source() {
                Some(why) => format!("LoadLibrary failed: {e}: {why}"),
                None => format!("LoadLibrary failed: {e}"),
            }
        })?;
        unsafe {
            let start = *lib.get::<StartFn>(b"PluginStart\0").map_err(|_| "procedure \"PluginStart\" not found".to_string())?;
            let finalize = *lib.get::<FinalizeFn>(b"PluginFinalize\0").map_err(|_| "procedure \"PluginFinalize\" not found".to_string())?;
            let variable = lib.get::<AccessFloatFn>(b"AccessVariable\0").ok().map(|s| *s);
            let trigger = lib.get::<AccessTriggerFn>(b"AccessTrigger\0").ok().map(|s| *s);
            let system = lib.get::<AccessFloatFn>(b"AccessSystemVariable\0").ok().map(|s| *s);
            let string = lib.get::<AccessStringFn>(b"AccessStringVariable\0").ok().map(|s| *s);
            let texture = lib.get::<AccessTextureFn>(b"AccessScriptTexture\0").ok().map(|s| *s);
            let pointer = lib.get::<AccessPointerFn>(b"AccessScriptTexturePointer\0").ok().map(|s| *s);
            let ticket = lib.get::<AccessTicketFn>(b"AccessTicketRequest\0").ok().map(|s| *s);
            Ok(Library { _lib: lib, start, finalize, variable, trigger, system, string, texture, pointer, ticket, buffers: Vec::new() })
        }
    }

    pub fn procs(&self) -> Procs {
        Procs { variable: self.variable.is_some(), trigger: self.trigger.is_some(), system: self.system.is_some(), string: self.string.is_some(), texture: self.texture.is_some(), pointer: self.pointer.is_some(), ticket: self.ticket.is_some() }
    }

    /// `PluginStart(AOwner)`; there is no Delphi application to own the plugin's forms here.
    pub fn start(&self) {
        unsafe { (self.start)(std::ptr::null_mut()) }
    }

    pub fn finalize(&self) {
        unsafe { (self.finalize)() }
    }

    /// One frame's calls, in OMSI's order.
    pub fn frame(&mut self, f: &Frame) -> Reply {
        let mut r = Reply::default();
        let float = |func: Option<AccessFloatFn>, list: &[(u16, f32)]| -> Vec<Option<f32>> {
            list.iter()
                .map(|&(i, v)| {
                    let Some(func) = func else { return None };
                    let (mut value, mut write) = (v, 0u8);
                    unsafe { func(i, &mut value, &mut write) };
                    (write != 0).then_some(value)
                })
                .collect()
        };
        r.system = float(self.system, &f.system);
        r.vars = float(self.variable, &f.vars);
        r.strings = f
            .strings
            .iter()
            .map(|(i, s)| {
                let func = self.string?;
                // (OMSI hands over exactly length + 1 characters; a plugin writing a
                // longer text overran the heap there. Here the buffer has room to spare,
                // zeroed, and only up to its first zero is read back)
                let mut buf: Vec<u16> = s.encode_utf16().collect();
                let size = (buf.len() + 1).max(STRING_BUFFER);
                buf.resize(size, 0);
                let mut write = 0u8;
                unsafe { func(*i, buf.as_mut_ptr(), &mut write) };
                if write == 0 {
                    return None;
                }
                let end = buf.iter().position(|&c| c == 0).unwrap_or(size);
                Some(String::from_utf16_lossy(&buf[..end]))
            })
            .collect();
        r.triggers_active = f
            .triggers
            .iter()
            .map(|&i| {
                let Some(func) = self.trigger else { return false };
                let mut active = 0u8;
                unsafe { func(i, &mut active) };
                active != 0
            })
            .collect();
        // the mouse first, so the picture drawn next can answer it in the same frame
        if let Some(func) = self.pointer {
            for &(i, kind, x, y) in &f.pointer {
                unsafe { func(i, kind as i32, x, y) };
            }
        }
        let texture = self.texture;
        r.textures = f
            .textures
            .iter()
            .map(|&(i, w, h)| {
                let func = texture?;
                if w == 0 || h == 0 || w > MAX_TEXTURE_SIDE || h > MAX_TEXTURE_SIDE {
                    return None;
                }
                let size = (w * h * 4) as usize;
                let i_us = i as usize;
                if self.buffers.len() <= i_us {
                    self.buffers.resize_with(i_us + 1, Vec::new);
                }
                let buf = &mut self.buffers[i_us];
                if buf.len() != size {
                    // (a new size: what was drawn at the old one means nothing now)
                    buf.clear();
                    buf.resize(size, 0);
                }
                let mut write = 0u8;
                unsafe { func(i, w as i32, h as i32, buf.as_mut_ptr(), &mut write) };
                (write != 0).then(|| buf.clone())
            })
            .collect();
        r.tickets = f.tickets.iter().map(|t| self.ticket.and_then(|func| call_ticket(func, t))).collect();
        r
    }
}

/// One `AccessTicketRequest`: the texts as zero-terminated wide strings, the name and voice
/// in buffers of [`TICKET_TEXT`] characters (read back up to their first zero).
fn call_ticket(func: AccessTicketFn, t: &TicketCall) -> Option<TicketAnswer> {
    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    let buffer = |s: &str| -> Vec<u16> {
        let mut b: Vec<u16> = s.encode_utf16().take(TICKET_TEXT - 1).collect();
        b.resize(TICKET_TEXT, 0);
        b
    };
    let read = |b: &[u16]| String::from_utf16_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(b.len())]);
    let (stop, dest) = (wide(&t.stop), wide(&t.destination));
    let (mut name, mut voice) = (buffer(&t.name), buffer(""));
    let (mut kind, mut ticket, mut price, mut write) = (t.kind, t.ticket, t.price, 0u8);
    unsafe { func(t.person, t.age, stop.as_ptr(), dest.as_ptr(), t.stop_number, t.destination_number, &mut kind, &mut ticket, &mut price, name.as_mut_ptr(), voice.as_mut_ptr(), &mut write) };
    // (the last character stays the terminating zero whatever the plugin wrote)
    name[TICKET_TEXT - 1] = 0;
    voice[TICKET_TEXT - 1] = 0;
    (write != 0).then(|| TicketAnswer { kind, ticket, price, name: read(&name), voice: read(&voice) })
}

/// Wide characters a plugin's string variable buffer has at least (see `frame`).
const STRING_BUFFER: usize = 4096;

// --- the host protocol: little-endian, one request and one answer at a time ---

pub mod wire {
    use super::*;

    pub const START: u8 = 1;
    pub const FRAME: u8 = 2;
    pub const FINALIZE: u8 = 3;

    pub fn put_u8(w: &mut impl Write, v: u8) -> std::io::Result<()> {
        w.write_all(&[v])
    }
    pub fn put_u16(w: &mut impl Write, v: u16) -> std::io::Result<()> {
        w.write_all(&v.to_le_bytes())
    }
    pub fn put_u32(w: &mut impl Write, v: u32) -> std::io::Result<()> {
        w.write_all(&v.to_le_bytes())
    }
    pub fn put_f32(w: &mut impl Write, v: f32) -> std::io::Result<()> {
        w.write_all(&v.to_le_bytes())
    }
    pub fn put_str(w: &mut impl Write, s: &str) -> std::io::Result<()> {
        let u: Vec<u16> = s.encode_utf16().collect();
        put_u16(w, u.len().min(u16::MAX as usize) as u16)?;
        for c in u.iter().take(u16::MAX as usize) {
            put_u16(w, *c)?;
        }
        Ok(())
    }
    pub fn get_u8(r: &mut impl Read) -> std::io::Result<u8> {
        let mut b = [0u8; 1];
        r.read_exact(&mut b)?;
        Ok(b[0])
    }
    pub fn get_u16(r: &mut impl Read) -> std::io::Result<u16> {
        let mut b = [0u8; 2];
        r.read_exact(&mut b)?;
        Ok(u16::from_le_bytes(b))
    }
    pub fn get_u32(r: &mut impl Read) -> std::io::Result<u32> {
        let mut b = [0u8; 4];
        r.read_exact(&mut b)?;
        Ok(u32::from_le_bytes(b))
    }
    pub fn get_f32(r: &mut impl Read) -> std::io::Result<f32> {
        let mut b = [0u8; 4];
        r.read_exact(&mut b)?;
        Ok(f32::from_le_bytes(b))
    }
    pub fn get_str(r: &mut impl Read) -> std::io::Result<String> {
        let n = get_u16(r)? as usize;
        let mut u = Vec::with_capacity(n);
        for _ in 0..n {
            u.push(get_u16(r)?);
        }
        Ok(String::from_utf16_lossy(&u))
    }

    /// A frame. The script textures are on the wire only when `textures` (the library has
    /// `AccessScriptTexture`, which both sides know from `START`): a host from before them
    /// keeps working for every plugin without the procedure.
    ///
    /// The mouse events likewise only when `pointer` (`AccessScriptTexturePointer`).
    pub fn put_frame(w: &mut impl Write, f: &Frame, textures: bool, pointer: bool) -> std::io::Result<()> {
        put_u16(w, f.system.len() as u16)?;
        for (i, v) in &f.system {
            put_u16(w, *i)?;
            put_f32(w, *v)?;
        }
        put_u16(w, f.vars.len() as u16)?;
        for (i, v) in &f.vars {
            put_u16(w, *i)?;
            put_f32(w, *v)?;
        }
        put_u16(w, f.strings.len() as u16)?;
        for (i, s) in &f.strings {
            put_u16(w, *i)?;
            put_str(w, s)?;
        }
        put_u16(w, f.triggers.len() as u16)?;
        for i in &f.triggers {
            put_u16(w, *i)?;
        }
        if textures {
            put_u16(w, f.textures.len() as u16)?;
            for (i, tw, th) in &f.textures {
                put_u16(w, *i)?;
                put_u32(w, *tw)?;
                put_u32(w, *th)?;
            }
        }
        if pointer {
            put_u16(w, f.pointer.len() as u16)?;
            for (i, kind, x, y) in &f.pointer {
                put_u16(w, *i)?;
                put_u8(w, *kind)?;
                put_f32(w, *x)?;
                put_f32(w, *y)?;
            }
        }
        Ok(())
    }

    pub fn get_frame(r: &mut impl Read, textures: bool, pointer: bool) -> std::io::Result<Frame> {
        let mut f = Frame::default();
        for _ in 0..get_u16(r)? {
            f.system.push((get_u16(r)?, get_f32(r)?));
        }
        for _ in 0..get_u16(r)? {
            f.vars.push((get_u16(r)?, get_f32(r)?));
        }
        for _ in 0..get_u16(r)? {
            f.strings.push((get_u16(r)?, get_str(r)?));
        }
        for _ in 0..get_u16(r)? {
            f.triggers.push(get_u16(r)?);
        }
        if textures {
            for _ in 0..get_u16(r)? {
                f.textures.push((get_u16(r)?, get_u32(r)?, get_u32(r)?));
            }
        }
        if pointer {
            for _ in 0..get_u16(r)? {
                f.pointer.push((get_u16(r)?, get_u8(r)?, get_f32(r)?, get_f32(r)?));
            }
        }
        Ok(f)
    }

    pub fn put_reply(w: &mut impl Write, r: &Reply, textures: bool) -> std::io::Result<()> {
        for v in r.system.iter().chain(&r.vars) {
            put_u8(w, v.is_some() as u8)?;
            put_f32(w, v.unwrap_or(0.0))?;
        }
        for s in &r.strings {
            put_u8(w, s.is_some() as u8)?;
            put_str(w, s.as_deref().unwrap_or(""))?;
        }
        for a in &r.triggers_active {
            put_u8(w, *a as u8)?;
        }
        if textures {
            for t in &r.textures {
                match t {
                    Some(px) => {
                        put_u8(w, 1)?;
                        put_u32(w, px.len() as u32)?;
                        w.write_all(px)?;
                    }
                    None => put_u8(w, 0)?,
                }
            }
        }
        Ok(())
    }

    /// The frame's passengers (`Frame::tickets`), after the frame, only for a library with
    /// `AccessTicketRequest` (a host from before it never sees them).
    pub fn put_tickets(w: &mut impl Write, t: &[TicketCall]) -> std::io::Result<()> {
        put_u16(w, t.len().min(u16::MAX as usize) as u16)?;
        for c in t.iter().take(u16::MAX as usize) {
            put_u32(w, c.person)?;
            put_f32(w, c.age)?;
            put_str(w, &c.stop)?;
            put_str(w, &c.destination)?;
            put_u32(w, c.stop_number as u32)?;
            put_u32(w, c.destination_number as u32)?;
            put_u32(w, c.kind as u32)?;
            put_u32(w, c.ticket as u32)?;
            put_f32(w, c.price)?;
            put_str(w, &c.name)?;
        }
        Ok(())
    }

    pub fn get_tickets(r: &mut impl Read) -> std::io::Result<Vec<TicketCall>> {
        (0..get_u16(r)?)
            .map(|_| {
                Ok(TicketCall {
                    person: get_u32(r)?,
                    age: get_f32(r)?,
                    stop: get_str(r)?,
                    destination: get_str(r)?,
                    stop_number: get_u32(r)? as i32,
                    destination_number: get_u32(r)? as i32,
                    kind: get_u32(r)? as i32,
                    ticket: get_u32(r)? as i32,
                    price: get_f32(r)?,
                    name: get_str(r)?,
                })
            })
            .collect()
    }

    /// The answers, one per passenger of the frame, after the reply.
    pub fn put_ticket_answers(w: &mut impl Write, a: &[Option<TicketAnswer>]) -> std::io::Result<()> {
        for t in a {
            match t {
                Some(t) => {
                    put_u8(w, 1)?;
                    put_u32(w, t.kind as u32)?;
                    put_u32(w, t.ticket as u32)?;
                    put_f32(w, t.price)?;
                    put_str(w, &t.name)?;
                    put_str(w, &t.voice)?;
                }
                None => put_u8(w, 0)?,
            }
        }
        Ok(())
    }

    pub fn get_ticket_answers(r: &mut impl Read, n: usize) -> std::io::Result<Vec<Option<TicketAnswer>>> {
        (0..n)
            .map(|_| {
                if get_u8(r)? == 0 {
                    return Ok(None);
                }
                Ok(Some(TicketAnswer { kind: get_u32(r)? as i32, ticket: get_u32(r)? as i32, price: get_f32(r)?, name: get_str(r)?, voice: get_str(r)? }))
            })
            .collect()
    }

    pub fn get_reply(rd: &mut impl Read, f: &Frame, textures: bool) -> std::io::Result<Reply> {
        let mut r = Reply::default();
        let mut float = |n: usize| -> std::io::Result<Vec<Option<f32>>> {
            (0..n).map(|_| Ok(if get_u8(rd)? != 0 { Some(get_f32(rd)?) } else { get_f32(rd)?; None })).collect()
        };
        r.system = float(f.system.len())?;
        r.vars = float(f.vars.len())?;
        for _ in 0..f.strings.len() {
            let w = get_u8(rd)? != 0;
            let s = get_str(rd)?;
            r.strings.push(w.then_some(s));
        }
        for _ in 0..f.triggers.len() {
            r.triggers_active.push(get_u8(rd)? != 0);
        }
        if textures {
            for &(_, tw, th) in &f.textures {
                if get_u8(rd)? == 0 {
                    r.textures.push(None);
                    continue;
                }
                let n = get_u32(rd)? as usize;
                // (exactly the size asked: anything else is a broken host, and its length
                // must not decide how much is allocated)
                if tw > MAX_TEXTURE_SIDE || th > MAX_TEXTURE_SIDE || n != (tw * th * 4) as usize {
                    return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "script texture of the wrong size"));
                }
                let mut px = vec![0u8; n];
                rd.read_exact(&mut px)?;
                r.textures.push(Some(px));
            }
        }
        Ok(r)
    }
}

/// A plugin library in `omsi-plugin-host`.
pub struct Remote {
    child: Child,
    to: BufWriter<ChildStdin>,
    from: BufReader<ChildStdout>,
    procs: Procs,
}

impl Remote {
    /// Start `host` (with `runner`, e.g. `wine`, in front when given) on the library in
    /// `dir` and have it call `PluginStart`.
    pub fn spawn(runner: Option<&Path>, host: &Path, dll: &Path, dir: &Path) -> Result<Remote, String> {
        let mut cmd = match runner {
            Some(r) => {
                let mut c = Command::new(r);
                c.arg(host);
                c
            }
            None => Command::new(host),
        };
        cmd.arg(dll).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).current_dir(dir);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let mut child = cmd.spawn().map_err(|e| format!("could not start {}: {e}", host.display()))?;
        if let Some(err) = child.stderr.take() {
            std::thread::spawn(move || {
                for line in std::io::BufRead::lines(BufReader::new(err)).map_while(Result::ok) {
                    log::warn!("plugin host: {line}");
                }
            });
        }
        let to = BufWriter::new(child.stdin.take().ok_or("no stdin")?);
        let from = BufReader::new(child.stdout.take().ok_or("no stdout")?);
        let mut r = Remote { child, to, from, procs: Procs::default() };
        let answer = (|| -> std::io::Result<(u8, u8)> {
            wire::put_u8(&mut r.to, wire::START)?;
            r.to.flush()?;
            Ok((wire::get_u8(&mut r.from)?, wire::get_u8(&mut r.from)?))
        })();
        match answer {
            Ok((1, flags)) => {
                r.procs = Procs::from_flags(flags);
                Ok(r)
            }
            Ok(_) => {
                let _ = r.child.kill();
                Err(format!("the host could not load {}", dll.display()))
            }
            Err(e) => {
                let _ = r.child.kill();
                Err(format!("the host for {} ended: {e}", dll.display()))
            }
        }
    }

    pub fn procs(&self) -> Procs {
        self.procs
    }

    pub fn frame(&mut self, f: &Frame) -> std::io::Result<Reply> {
        wire::put_u8(&mut self.to, wire::FRAME)?;
        let textures = self.procs.texture;
        wire::put_frame(&mut self.to, f, textures, self.procs.pointer)?;
        if self.procs.ticket {
            wire::put_tickets(&mut self.to, &f.tickets)?;
        }
        self.to.flush()?;
        let mut reply = wire::get_reply(&mut self.from, f, textures)?;
        if self.procs.ticket {
            reply.tickets = wire::get_ticket_answers(&mut self.from, f.tickets.len())?;
        }
        Ok(reply)
    }

    pub fn finalize(&mut self) {
        let _ = wire::put_u8(&mut self.to, wire::FINALIZE).and_then(|_| self.to.flush());
        let _ = wire::get_u8(&mut self.from);
        let _ = self.child.wait();
    }
}

/// How a plugin runs.
pub enum Backend {
    Local(Library),
    Remote(Remote),
}

/// A loaded plugin: its `.opl` and its library.
pub struct Plugin {
    pub opl_path: PathBuf,
    pub opl: Opl,
    backend: Backend,
    procs: Procs,
    /// Each trigger's state after the last frame.
    trigger_state: Vec<bool>,
    failed: bool,
}

const PATCHES_OMSI_EXE: &[&str] = &["omsipresentationtools.dll"];

fn patches_omsi_exe(dll: &Path) -> bool {
    dll.file_name().is_some_and(|n| PATCHES_OMSI_EXE.contains(&n.to_string_lossy().to_ascii_lowercase().as_str()))
}

/// Where to look for the out-of-process host and what runs it.
#[derive(Debug, Clone, Default)]
pub struct HostConfig {
    /// `omsi-plugin-host` built for 32-bit Windows (`omsi-plugin-host32.exe`).
    pub host32: Option<PathBuf>,
    /// The program that runs Windows executables here (`wine`); none on Windows.
    pub runner: Option<PathBuf>,
}

impl HostConfig {
    /// The host next to the running program and, off Windows, `wine` from the path.
    pub fn detect() -> HostConfig {
        let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
        let host32 = omsi_cfg::flags::OMSI_PLUGIN_HOST32.live_os()
            .map(PathBuf::from)
            .or_else(|| exe_dir.map(|d| d.join("omsi-plugin-host32.exe")))
            .filter(|p| p.is_file());
        let runner = if cfg!(windows) {
            None
        } else {
            omsi_cfg::flags::OMSI_WINE.live_os().map(PathBuf::from).or_else(|| {
                // the path, then where Homebrew and the Wine app bundles put it (a game
                // started from Finder gets a path without /opt/homebrew/bin)
                let from_path: Vec<PathBuf> = std::env::var_os("PATH")
                    .map(|paths| std::env::split_paths(&paths).map(|d| d.join("wine")).collect())
                    .unwrap_or_default();
                from_path
                    .into_iter()
                    .chain(
                        [
                            "/opt/homebrew/bin/wine",
                            "/usr/local/bin/wine",
                            "/Applications/Wine Stable.app/Contents/Resources/wine/bin/wine",
                            "/Applications/Wine Devel.app/Contents/Resources/wine/bin/wine",
                            "/Applications/Wine Staging.app/Contents/Resources/wine/bin/wine",
                            "/usr/bin/wine",
                        ]
                        .map(PathBuf::from),
                    )
                    .find(|p| p.is_file())
            })
        };
        HostConfig { host32, runner }
    }
}

impl Plugin {
    /// Load the plugin an `.opl` describes: in this process when its library loads here,
    /// else in the 32-bit host.
    pub fn load(opl_path: &Path, plugins_dir: &Path, hosts: &HostConfig) -> Result<Plugin, String> {
        let text = std::fs::read(opl_path).map_err(|e| e.to_string())?;
        let opl = parse_opl(&String::from_utf8_lossy(&text));
        if opl.dll.is_empty() {
            return Err("no [dll]".into());
        }
        let dll = resolve_path(plugins_dir, &opl.dll).ok_or_else(|| format!("{} not found", opl.dll))?;
        if patches_omsi_exe(&dll) {
            return Err("it patches the code of OMSI.exe, which openOMSI does not run".into());
        }
        let (backend, procs) = match Library::load(&dll) {
            Ok(lib) => {
                lib.start();
                let p = lib.procs();
                (Backend::Local(lib), p)
            }
            Err(local) => {
                let Some(host) = &hosts.host32 else {
                    return Err(format!("{local}; a 32-bit Windows plugin needs omsi-plugin-host32.exe next to the game"));
                };
                if !cfg!(windows) && hosts.runner.is_none() {
                    return Err(format!("{local}; a Windows plugin needs Wine here (or OMSI_WINE)"));
                }
                let root = plugins_dir.parent().unwrap_or(plugins_dir);
                let r = Remote::spawn(hosts.runner.as_deref(), host, &dll, root)?;
                let p = r.procs();
                (Backend::Remote(r), p)
            }
        };
        for (has, name) in [(procs.variable, "AccessVariable"), (procs.trigger, "AccessTrigger"), (procs.system, "AccessSystemVariable"), (procs.string, "AccessStringVariable")] {
            if !has {
                log::warn!("Loading plugin {}: procedure \"{name}\" not found!", opl.dll);
            }
        }
        let n = opl.triggers.len();
        Ok(Plugin { opl_path: opl_path.to_path_buf(), opl, backend, procs, trigger_state: vec![false; n], failed: false })
    }

    pub fn procs(&self) -> Procs {
        self.procs
    }

    /// Run one frame. `system` / `var` / `string` read a value by name (None: no such
    /// variable, skipped); the writes and trigger edges come back through `set_*` and `fire`.
    /// `pointer`: the mouse on the vehicle's script textures since the last frame
    /// (`TexturePointer::pending`).
    pub fn frame(&mut self, io: &mut dyn PluginIo, pointer: &[(usize, u8, f32, f32)]) {
        if self.failed {
            return;
        }
        let mut f = Frame::default();
        let mut sys_names = Vec::new();
        if self.procs.system {
            for (i, n) in self.opl.system_vars.iter().enumerate() {
                if let Some(v) = io.system(n) {
                    f.system.push((i as u16, v));
                    sys_names.push(n.clone());
                }
            }
        }
        let vehicle = io.has_vehicle();
        let (mut var_names, mut str_names, mut trig_idx, mut tex_idx) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        if vehicle {
            if self.procs.variable {
                for (i, n) in self.opl.vars.iter().enumerate() {
                    if let Some(v) = io.var(n) {
                        f.vars.push((i as u16, v));
                        var_names.push(n.clone());
                    }
                }
            }
            if self.procs.string {
                for (i, n) in self.opl.string_vars.iter().enumerate() {
                    if let Some(s) = io.string(n) {
                        f.strings.push((i as u16, s));
                        str_names.push(n.clone());
                    }
                }
            }
            if self.procs.trigger {
                for i in 0..self.opl.triggers.len() {
                    f.triggers.push(i as u16);
                    trig_idx.push(i);
                }
            }
            if self.procs.texture || self.procs.pointer {
                // (entry, texture number, width, height) of the textures the bus has
                let entries: Vec<(u16, usize, u32, u32)> = self
                    .opl
                    .script_textures
                    .iter()
                    .enumerate()
                    .filter_map(|(i, n)| {
                        let tex = n.trim().parse::<usize>().ok()?;
                        let (w, h) = io.script_texture_size(tex)?;
                        Some((i as u16, tex, w, h))
                    })
                    .collect();
                if self.procs.pointer {
                    // in the order they happened; the same texture listed twice hears both
                    for &(t, kind, u, v) in pointer {
                        for &(i, tex, w, h) in &entries {
                            if tex == t {
                                f.pointer.push((i, kind, u * w as f32, v * h as f32));
                            }
                        }
                    }
                }
                if self.procs.texture {
                    for &(i, tex, w, h) in &entries {
                        f.textures.push((i, w, h));
                        tex_idx.push(tex);
                    }
                }
            }
            if self.procs.ticket {
                f.tickets = io.ticket_asks().iter().map(TicketCall::of).collect();
            }
        }
        let reply = match &mut self.backend {
            Backend::Local(lib) => lib.frame(&f),
            Backend::Remote(r) => match r.frame(&f) {
                Ok(r) => r,
                Err(e) => {
                    log::warn!("plugin {}: the host stopped answering ({e}); it is left out from now on", self.opl.dll);
                    self.failed = true;
                    return;
                }
            },
        };
        for (n, v) in sys_names.iter().zip(&reply.system) {
            if let Some(v) = v {
                io.set_system(n, *v);
            }
        }
        for (n, v) in var_names.iter().zip(&reply.vars) {
            if let Some(v) = v {
                io.set_var(n, *v);
            }
        }
        for (n, s) in str_names.iter().zip(&reply.strings) {
            if let Some(s) = s {
                io.set_string(n, s);
            }
        }
        for (&i, &active) in trig_idx.iter().zip(&reply.triggers_active) {
            if self.trigger_state[i] != active {
                io.fire(&self.opl.triggers[i], active);
                self.trigger_state[i] = active;
            }
        }
        for ((&tex, &(_, w, h)), px) in tex_idx.iter().zip(&f.textures).zip(&reply.textures) {
            if let Some(px) = px {
                io.set_script_texture(tex, w, h, px);
            }
        }
        for (call, answer) in f.tickets.iter().zip(&reply.tickets) {
            let Some(answer) = answer else { continue };
            let by = Path::new(&self.opl.dll.replace('\\', "/")).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            if let Err(why) = io.people_set_ticket(call.person, &call.change(answer), &by) {
                log::info!("plugin {}: the ticket of passenger {} was not changed: {why}", self.opl.dll, call.person);
            }
        }
    }

    /// `PluginFinalize`.
    pub fn finalize(&mut self) {
        match &mut self.backend {
            Backend::Local(lib) => lib.finalize(),
            Backend::Remote(r) => r.finalize(),
        }
    }
}

/// Every plugin of the plugins folders.
#[derive(Default)]
pub struct Plugins {
    pub loaded: Vec<Plugin>,
    /// The Lua plugins (`plugins/*.lua`, `plugins/<name>/main.lua`, and `.oop` files of
    /// compiled Lua).
    pub lua: Vec<lua::LuaPlugin>,
    /// The WebAssembly plugins (`.oop` files of kind `wasm`).
    pub wasm: Vec<wasm_plugin::WasmHost>,
    /// What the Lua plugins show on the screen (`omsi.ui`), for the game to draw.
    pub ui: ui::SharedUi,
    /// What the plugins share: the panels, their messages, who is loaded.
    pub hub: api::runtime::SharedHub,
    /// The mouse on the script textures the `.opl` plugins listen to, for their next frame
    /// (the game puts `TexturePointer::pending` here).
    pub pointer: Vec<(usize, u8, f32, f32)>,
}

impl Plugins {
    /// Load the plugins of each `plugins` folder given (the first folder's copy of an
    /// `.opl` name wins).
    pub fn load(dirs: &[PathBuf], hosts: &HostConfig) -> Plugins {
        Self::load_with_process_grants(dirs, hosts, api::process::ProcessGrants::from_env())
    }

    pub fn load_with_process_grants(dirs: &[PathBuf], hosts: &HostConfig, process_grants: api::process::ProcessGrants) -> Plugins {
        Self::load_process_host(dirs, hosts, process_grants, None)
    }

    /// Interactive game hosts supply a broker whose requests only their trusted UI can answer.
    pub fn load_with_process_permissions(dirs: &[PathBuf], hosts: &HostConfig, process_grants: api::process::ProcessGrants, permissions: std::sync::Arc<api::process::ProcessPermissions>) -> Plugins {
        Self::load_process_host(dirs, hosts, process_grants, Some(permissions))
    }

    fn load_process_host(dirs: &[PathBuf], hosts: &HostConfig, process_grants: api::process::ProcessGrants, process_permissions: Option<std::sync::Arc<api::process::ProcessPermissions>>) -> Plugins {
        let mut loaded: Vec<Plugin> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for dir in dirs {
            for opl in find_opls(dir) {
                let key = opl.strip_prefix(dir).unwrap_or(&opl).to_string_lossy().to_ascii_lowercase();
                if !seen.insert(key) {
                    continue;
                }
                match Plugin::load(&opl, dir, hosts) {
                    Ok(p) => {
                        log::info!("plugin {} loaded ({} variables, {} strings, {} system variables, {} triggers, {} script textures{})", p.opl.dll, p.opl.vars.len(), p.opl.string_vars.len(), p.opl.system_vars.len(), p.opl.triggers.len(), if p.procs.texture { p.opl.script_textures.len() } else { 0 }, if p.procs.ticket { ", passengers' tickets" } else { "" });
                        loaded.push(p);
                    }
                    Err(e) => log::warn!("Could not load plugin {}: {e}", opl.display()),
                }
            }
        }
        let mut lua = Vec::new();
        let hub = std::rc::Rc::new(api::runtime::Hub { process_grants: std::sync::Arc::new(process_grants), process_permissions, ..Default::default() });
        let ui = hub.ui.clone();
        let mut seen = std::collections::HashSet::new();
        for dir in dirs {
            for path in lua::find_lua(dir) {
                let key = path.strip_prefix(dir).unwrap_or(&path).to_string_lossy().to_ascii_lowercase();
                if !seen.insert(key) {
                    continue;
                }
                match lua::LuaPlugin::start_spec(lua::LuaSpec::disk(&path), &path, &mut lua::NoVehicle, hub.clone()) {
                    Ok(p) => {
                        log::info!("Lua plugin {} loaded ({})", p.name, path.display());
                        lua.push(p);
                    }
                    Err(e) => log::warn!("Could not load Lua plugin {}: {e}", path.display()),
                }
            }
        }
        // compiled plugins (`.oop`): a name a `.lua` plugin has already is left out
        let mut wasm = Vec::new();
        for dir in dirs {
            for path in oop::find_oop(dir) {
                let name = path.file_stem().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
                if lua.iter().any(|p: &lua::LuaPlugin| p.name.eq_ignore_ascii_case(&name)) || wasm.iter().any(|p: &wasm_plugin::WasmHost| p.name.eq_ignore_ascii_case(&name)) {
                    log::warn!("plugin {} left out: a plugin of that name is loaded already", path.display());
                    continue;
                }
                match oop::load(&path) {
                    Ok(oop::Loaded::Lua(spec)) => match lua::LuaPlugin::start_spec(spec, &path, &mut lua::NoVehicle, hub.clone()) {
                        Ok(p) => {
                            log::info!("Lua plugin {} loaded ({})", p.name, path.display());
                            lua.push(p);
                        }
                        Err(e) => log::warn!("Could not load plugin {}: {e}", path.display()),
                    },
                    Ok(oop::Loaded::Wasm(spec)) => match wasm_plugin::WasmHost::start(spec, &path, &mut lua::NoVehicle, hub.clone()) {
                        Ok(p) => {
                            log::info!("WASM plugin {} loaded ({})", p.name, path.display());
                            wasm.push(p);
                        }
                        Err(e) => log::warn!("Could not load plugin {}: {e}", path.display()),
                    },
                    Err(e) => log::warn!("Could not load plugin {}: {e}", path.display()),
                }
            }
        }
        Plugins { loaded, lua, wasm, ui, hub, pointer: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.loaded.is_empty() && self.lua.is_empty() && self.wasm.is_empty()
    }

    pub fn frame(&mut self, io: &mut dyn PluginIo) {
        let pointer = std::mem::take(&mut self.pointer);
        for p in &mut self.loaded {
            p.frame(io, &pointer);
        }
        for p in &mut self.lua {
            p.frame(io);
        }
        for p in &mut self.wasm {
            p.frame(io);
        }
    }

    /// Send an event to every Lua plugin now, outside its frame (the game pausing: `pause`,
    /// `menu_open`, which the plugins hear although they stand still).
    pub fn emit(&mut self, io: &mut dyn PluginIo, event: &'static str, args: Vec<api::Value>) {
        for p in &mut self.lua {
            p.emit(io, event, args.clone());
        }
        for p in &mut self.wasm {
            p.emit(io, event, args.clone());
        }
    }

    /// An event for one plugin only (by its name), in its next frame: a LAN message for it.
    pub fn post(&self, plugin: &str, event: &'static str, args: Vec<api::Value>) {
        self.hub.direct.borrow_mut().push((plugin.to_string(), event, args));
    }

    /// Whether a plugin listens to `event` (its comparison or bookkeeping can be left out
    /// when none does).
    pub fn hears(&self, event: &str) -> bool {
        self.lua.iter().any(|p| p.hears(event)) || self.wasm.iter().any(|p| p.hears(event))
    }

    /// Whether a plugin hears of the passengers' tickets (`passenger_ticket`, or an `.opl`'s
    /// `AccessTicketRequest`): only then do the passengers report them and wait for it.
    pub fn hear_tickets(&self) -> bool {
        self.hears("passenger_ticket") || self.loaded.iter().any(|p| p.procs.ticket && !p.failed)
    }

    /// The player's vehicle's script textures the `.opl` plugins draw or hear the mouse on
    /// (`[scripttexturelist]` of a library with `AccessScriptTexturePointer`): a click there
    /// is theirs, as a click on a `[htmltexture]` is its page's.
    pub fn pointer_textures(&self) -> Vec<usize> {
        let mut out: Vec<usize> = self
            .loaded
            .iter()
            .filter(|p| p.procs.pointer && !p.failed)
            .flat_map(|p| p.opl.script_textures.iter().filter_map(|n| n.trim().parse().ok()))
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Whether a Lua plugin of this name is loaded.
    pub fn has(&self, plugin: &str) -> bool {
        self.lua.iter().any(|p| p.name.eq_ignore_ascii_case(plugin)) || self.wasm.iter().any(|p| p.name.eq_ignore_ascii_case(plugin))
    }

    pub fn finalize(&mut self) {
        for p in &mut self.loaded {
            p.finalize();
        }
        self.loaded.clear();
        for p in &mut self.lua {
            p.stop(&mut lua::NoVehicle);
        }
        self.lua.clear();
        for p in &mut self.wasm {
            p.stop(&mut lua::NoVehicle);
        }
        self.wasm.clear();
    }
}

impl Drop for Plugins {
    fn drop(&mut self) {
        self.finalize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opl_lists() {
        let o = parse_opl("[dll]\r\nsub\\my.dll\r\n\r\n[varlist]\r\n2\r\nVelocity\r\nthrottle\r\n[systemvarlist]\r\n1\r\nTime\r\n[stringvarlist]\r\n1\r\nIBIS_terminus_name\r\n[triggers]\r\n1\r\nbus_doorfront0\r\n[scripttexturelist]\r\n2\r\n0\r\n3\r\n");
        assert_eq!(o.dll, "sub\\my.dll");
        assert_eq!(o.vars, ["Velocity", "throttle"]);
        assert_eq!(o.system_vars, ["Time"]);
        assert_eq!(o.string_vars, ["IBIS_terminus_name"]);
        assert_eq!(o.triggers, ["bus_doorfront0"]);
        assert_eq!(o.script_textures, ["0", "3"]);
    }

    #[test]
    fn wire_round_trip() {
        let f = Frame { system: vec![(0, 1.5)], vars: vec![(1, 2.0), (3, -1.0)], strings: vec![(0, "Zoo €".into())], triggers: vec![0, 2], ..Default::default() };
        let mut buf = Vec::new();
        wire::put_frame(&mut buf, &f, false, false).unwrap();
        assert_eq!(wire::get_frame(&mut buf.as_slice(), false, false).unwrap(), f);
        let r = Reply { system: vec![None], vars: vec![Some(3.0), None], strings: vec![Some("ab".into())], triggers_active: vec![true, false], textures: vec![], tickets: vec![] };
        let mut buf = Vec::new();
        wire::put_reply(&mut buf, &r, false).unwrap();
        assert_eq!(wire::get_reply(&mut buf.as_slice(), &f, false).unwrap(), r);
    }

    #[test]
    fn wire_round_trip_textures() {
        let f = Frame { vars: vec![(0, 1.0)], textures: vec![(0, 2, 1), (1, 4, 4)], pointer: vec![(1, POINTER_DOWN, 1.5, 2.25), (1, POINTER_UP, 3.0, 0.0)], ..Default::default() };
        let mut buf = Vec::new();
        wire::put_frame(&mut buf, &f, true, true).unwrap();
        assert_eq!(wire::get_frame(&mut buf.as_slice(), true, true).unwrap(), f);
        // without the pointer procedure the events stay off the wire
        let mut buf = Vec::new();
        wire::put_frame(&mut buf, &f, true, false).unwrap();
        assert_eq!(wire::get_frame(&mut buf.as_slice(), true, false).unwrap(), Frame { pointer: vec![], ..f.clone() });
        let r = Reply { vars: vec![None], textures: vec![Some(vec![1, 2, 3, 4, 5, 6, 7, 8]), None], ..Default::default() };
        let mut buf = Vec::new();
        wire::put_reply(&mut buf, &r, true).unwrap();
        assert_eq!(wire::get_reply(&mut buf.as_slice(), &f, true).unwrap(), r);
        // a picture of another size than asked is refused, not believed
        let bad = Reply { vars: vec![None], textures: vec![Some(vec![0; 4]), None], ..Default::default() };
        let mut buf = Vec::new();
        wire::put_reply(&mut buf, &bad, true).unwrap();
        assert!(wire::get_reply(&mut buf.as_slice(), &f, true).is_err());
    }

    #[test]
    fn pointer_events_for_listened_textures_only() {
        let mut tp = TexturePointer { pages: vec![2], ..Default::default() };
        assert!(!tp.push(0, POINTER_DOWN, 0.1, 0.1), "no plugin listens to texture 0");
        assert!(tp.push(2, POINTER_DOWN, 0.1, 0.2));
        assert!(tp.push(2, POINTER_MOVE, 0.3, 0.3));
        assert!(tp.push(2, POINTER_MOVE, 0.4, 0.4));
        assert!(tp.push(2, POINTER_UP, 0.5, 0.5));
        // the second move replaced the first one
        assert_eq!(tp.pending, [(2, POINTER_DOWN, 0.1, 0.2), (2, POINTER_MOVE, 0.4, 0.4), (2, POINTER_UP, 0.5, 0.5)]);
    }

    #[test]
    fn wire_round_trip_tickets() {
        let calls = vec![
            TicketCall { person: 7, age: 34.0, stop: "Střížkov".into(), destination: "Nový Bor, aut.st.".into(), stop_number: 1, destination_number: 12, kind: 2, ticket: 1, price: 30.0, name: "Plnocenná".into() },
            TicketCall { person: 9, age: 10.0, ..Default::default() },
        ];
        let mut buf = Vec::new();
        wire::put_tickets(&mut buf, &calls).unwrap();
        assert_eq!(wire::get_tickets(&mut buf.as_slice()).unwrap(), calls);
        let answers = vec![Some(TicketAnswer { kind: 2, ticket: 1, price: 64.0, name: "IDOL 4001".into(), voice: "Zone_4001".into() }), None];
        let mut buf = Vec::new();
        wire::put_ticket_answers(&mut buf, &answers).unwrap();
        assert_eq!(wire::get_ticket_answers(&mut buf.as_slice(), 2).unwrap(), answers);
    }

    #[test]
    fn ticket_answer_changes_only_what_differs() {
        let call = TicketCall { person: 7, kind: 2, ticket: 1, price: 30.0, name: "Plnocenná".into(), ..Default::default() };
        let same = TicketAnswer { kind: 2, ticket: 1, price: 30.0, name: "Plnocenná".into(), voice: String::new() };
        assert_eq!(call.change(&same), TicketChange::default());
        let other = TicketAnswer { kind: 0, ticket: 3, price: 64.0, name: "IDOL".into(), voice: "Zone_4001".into() };
        let c = call.change(&other);
        assert_eq!(c.kind, Some(TicketKind::None));
        assert_eq!(c.ticket, Some(3));
        assert_eq!(c.price, Some(64.0));
        assert_eq!(c.name.as_deref(), Some("IDOL"));
        assert_eq!(c.voice.as_deref(), Some("Zone_4001"));
        // (a kind or ticket number out of range is left as it was)
        let bad = TicketAnswer { kind: 7, ticket: -1, ..same };
        assert_eq!(call.change(&bad), TicketChange::default());
    }

    #[test]
    fn omsi_exe_patchers_left_out() {
        assert!(patches_omsi_exe(Path::new("plugins/OMSIPresentationTools.dll")));
        assert!(!patches_omsi_exe(Path::new("plugins/Demo/demo.dll")));
    }

    #[test]
    fn procs_flags() {
        for flags in 0..128u8 {
            assert_eq!(Procs::from_flags(flags).flags(), flags);
        }
    }
}
