//! The developer tools: a panel over the game for those who build vehicles (Ctrl+Shift+D).
//!
//! * **Variables**: the bus's script variables and string variables - looked up by name,
//!   watched (the list kept per bus in `~/.openomsi/devtools/`), set, or held at a value
//!   every frame against what the scripts write.
//! * **Overlays** on the picture: the paths and their points, the seats and standing
//!   places, the boxes, the axles and wheels, the interior lights with their variables,
//!   the ticket and stamper points, the mirrors, the centre of gravity and the views.
//! * **Mirrors**: each `[add_camera_reflexion]` moved, turned and widened while the bus
//!   drives, and copied as the `[add_camera_reflexion_2]` block it has become.
//! * **Files**: the bus's files read again when they change (or by hand) - paths.cfg and
//!   `[passengercabin]`, sound.cfg, the `.bus` (physics, cameras, mirrors, box), model.cfg
//!   (its animations and lights, as long as its meshes stay) and the constfiles (while
//!   the scripts and their variables stay) - and the game's log opened.
//!
//! It lives in the game rather than in a window of its own: it shows the bus where it is
//! and draws with the game's interface (`shell`). Closed it is gone - nothing of it is
//! kept, looked at or drawn, and the variables it held are let go.

mod overlay;
mod panel;
mod reload;

use std::path::PathBuf;
use std::time::Instant;

use glam::Vec2;
use winit::keyboard::KeyCode;

use omsi_ui::kit::Key;
use crate::App;

pub(crate) use reload::Kind;

/// A variable on the watch list: its name, whether it is a string variable, and the value
/// it is held at (None: it goes as the scripts make it).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Watch {
    pub name: String,
    pub string: bool,
    pub hold: Option<String>,
}

/// What the panel asks of the game.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Request {
    Reload(Kind),
    OpenLog,
    CopyMirror(usize),
    Close,
}

pub(crate) struct DevTools {
    /// The open page: variables, overlays, mirrors, files.
    pub tab: usize,
    /// The variables' search.
    pub filter: String,
    pub watch: Vec<Watch>,
    /// The watched variable being edited, and the value typed for it.
    pub selected: Option<usize>,
    pub edit: String,
    pub overlays: overlay::Overlays,
    pub mirror: usize,
    /// The bus the watch list and the files are of.
    pub bus: PathBuf,
    /// The files of each part of the bus the overlays show (front first).
    pub parts: Vec<overlay::PartFiles>,
    /// The files looked at for changes: (what they are, file, when it was last changed).
    pub files: Vec<(Kind, PathBuf, Option<std::time::SystemTime>)>,
    /// Read a file again as soon as it changes.
    pub auto_reload: bool,
    last_check: Instant,
    /// What was done last (shown on the files' page), newest last.
    pub messages: Vec<String>,
    /// A script or a variable list changed since the tools opened: compiled again, the
    /// constfiles would bring the changed scripts along, so they wait for the bus to be
    /// placed anew too.
    pub scripts_changed: bool,
}

/// Where the watch lists are kept: `~/.openomsi/devtools/<bus file>.watch`.
fn watch_file(bus: &std::path::Path) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    let name = bus.file_name()?.to_string_lossy().to_string();
    Some(PathBuf::from(home).join(".openomsi").join("devtools").join(format!("{name}.watch")))
}

/// A watch list as its file has it: a name per line, a string variable's after `$`.
fn parse_watch(text: &str) -> Vec<Watch> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| match l.strip_prefix('$') {
            Some(n) => Watch { name: n.trim().to_string(), string: true, hold: None },
            None => Watch { name: l.to_string(), string: false, hold: None },
        })
        .collect()
}

fn watch_text(list: &[Watch]) -> String {
    let mut out = String::from("# openOMSI developer tools: the variables watched for this bus ($: a string variable)\n");
    for w in list {
        out.push_str(&format!("{}{}\n", if w.string { "$" } else { "" }, w.name));
    }
    out
}

impl DevTools {
    pub fn new(p: Option<&crate::player::Player>) -> DevTools {
        let bus = p.map(|p| p.vehicle.ty.def.path.clone()).unwrap_or_default();
        let watch = watch_file(&bus).and_then(|f| std::fs::read_to_string(f).ok()).map(|t| parse_watch(&t)).unwrap_or_default();
        let mut d = DevTools {
            tab: 0,
            filter: String::new(),
            watch,
            selected: None,
            edit: String::new(),
            overlays: Default::default(),
            mirror: 0,
            bus,
            parts: Vec::new(),
            files: Vec::new(),
            auto_reload: true,
            last_check: Instant::now(),
            messages: Vec::new(),
            scripts_changed: false,
        };
        if let Some(p) = p {
            d.parts = reload::part_defs(&p.vehicle).iter().map(|def| overlay::PartFiles::load(def)).collect();
            d.files = reload::watched_files(&p.vehicle);
        }
        d
    }

    /// Keep the watch list for the next time (on every change of it).
    fn save_watch(&self) {
        let Some(f) = watch_file(&self.bus) else { return };
        let _ = std::fs::create_dir_all(f.parent().unwrap_or(&f));
        if let Err(e) = std::fs::write(&f, watch_text(&self.watch)) {
            log::warn!("developer tools: {}: {e}", f.display());
        }
    }

    /// Put `name` on the watch list (once) and choose it.
    pub fn add_watch(&mut self, name: &str, string: bool) {
        let k = match self.watch.iter().position(|w| w.name.eq_ignore_ascii_case(name) && w.string == string) {
            Some(k) => k,
            None => {
                self.watch.push(Watch { name: name.to_string(), string, hold: None });
                self.save_watch();
                self.watch.len() - 1
            }
        };
        self.selected = Some(k);
    }

    pub fn remove_watch(&mut self, k: usize) {
        if k < self.watch.len() {
            self.watch.remove(k);
            self.selected = None;
            self.save_watch();
        }
    }

    fn say(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        log::info!("developer tools: {msg}");
        self.messages.push(msg);
        if self.messages.len() > 8 {
            self.messages.remove(0);
        }
    }

    /// The variables held: written into the bus before its scripts run and after (what
    /// the scripts make of them in between is not seen).
    pub fn apply_holds(&self, v: &mut omsi_sim::VehicleInstance) {
        for w in &self.watch {
            if let Some(h) = &w.hold {
                set_value(v, &w.name, w.string, h);
            }
        }
    }
}

/// Set variable `name` of `v` to the text `value` (a number for a numeric one); false when
/// the bus has no such variable or the text is no number.
pub(crate) fn set_value(v: &mut omsi_sim::VehicleInstance, name: &str, string: bool, value: &str) -> bool {
    if string {
        match v.ty.program.str_var(name) {
            Some(i) => {
                if let Some(s) = v.state.str_vars.get_mut(i as usize) {
                    if s != value {
                        *s = value.to_string();
                    }
                }
                true
            }
            None => false,
        }
    } else {
        match value.trim().replace(',', ".").parse::<f32>().ok().filter(|x| x.is_finite()) {
            Some(x) => v.set_var(name, x),
            None => false,
        }
    }
}

/// What a key types into a text field when the system gives no text with it (an
/// `OMSI_INPUT` script's keys): the letters, digits and the signs of numbers and names.
fn key_char(code: KeyCode, shift: bool) -> Option<char> {
    let s = format!("{code:?}");
    let c = if let Some(l) = s.strip_prefix("Key") {
        l.chars().next().map(|c| if shift { c } else { c.to_ascii_lowercase() })
    } else if let Some(d) = s.strip_prefix("Digit").or_else(|| s.strip_prefix("Numpad")) {
        d.chars().next().filter(|c| c.is_ascii_digit())
    } else {
        match code {
            KeyCode::Minus | KeyCode::NumpadSubtract => Some(if shift { '_' } else { '-' }),
            KeyCode::Period | KeyCode::NumpadDecimal => Some('.'),
            KeyCode::Space => Some(' '),
            _ => None,
        }
    };
    c.filter(|c| !c.is_control())
}

impl App {
    /// Open or close the developer tools (Ctrl+Shift+D).
    pub(crate) fn toggle_devtools(&mut self) {
        // (a text field of the panel lets the keyboard go either way)
        self.shell.ui.focus = None;
        if self.devtools.take().is_some() {
            self.shell.clear();
            self.service_msg = Some(("Developer tools closed".into(), 2.0));
        } else {
            self.devtools = Some(Box::new(DevTools::new(self.player.as_ref())));
            self.service_msg = Some(("Developer tools (Ctrl+Shift+D closes them)".into(), 3.0));
        }
    }

    /// `on_key` for the developer tools: Ctrl+Shift+D, and the keys of a text field of
    /// theirs being typed into. True when the key was theirs.
    pub(crate) fn devtools_key(&mut self, code: KeyCode, pressed: bool, repeat: bool) -> bool {
        let ctrl = self.input.keys.contains(&KeyCode::ControlLeft) || self.input.keys.contains(&KeyCode::ControlRight);
        let shift = self.input.keys.contains(&KeyCode::ShiftLeft) || self.input.keys.contains(&KeyCode::ShiftRight);
        if pressed && !repeat && code == KeyCode::KeyD && ctrl && shift {
            self.toggle_devtools();
            return true;
        }
        pressed && self.devtools_typing(code, None)
    }

    /// A key pressed while a text field of the developer tools has the keyboard: it types
    /// there (`text`: what the keyboard layout made of it) and goes nowhere else.
    pub(crate) fn devtools_typing(&mut self, code: KeyCode, text: Option<&str>) -> bool {
        if self.devtools.is_none() || self.shell.ui.focus.is_none() || self.menus.game_menu.is_some() {
            return false;
        }
        let shift = self.input.keys.contains(&KeyCode::ShiftLeft) || self.input.keys.contains(&KeyCode::ShiftRight);
        let input = &mut self.shell.ui.input;
        let key = match code {
            KeyCode::ArrowLeft => Some(Key::Left),
            KeyCode::ArrowRight => Some(Key::Right),
            KeyCode::Home => Some(Key::Home),
            KeyCode::End => Some(Key::End),
            KeyCode::Backspace => Some(Key::Backspace),
            KeyCode::Delete => Some(Key::Delete),
            KeyCode::Enter | KeyCode::NumpadEnter => Some(Key::Enter),
            KeyCode::Tab => Some(Key::Tab),
            KeyCode::Escape => {
                self.shell.ui.focus = None;
                return true;
            }
            _ => None,
        };
        match key {
            Some(k) => input.keys.push(k),
            None => match text.filter(|t| t.chars().any(|c| !c.is_control())) {
                Some(t) => input.text.push_str(t),
                None => {
                    if let Some(c) = key_char(code, shift) {
                        input.text.push(c);
                    }
                }
            },
        }
        true
    }

    /// The left button while the developer tools are open: the panel's when it is pressed
    /// over it (or let go after that). True when it was the panel's.
    pub(crate) fn devtools_click(&mut self, pressed: bool) -> bool {
        if self.devtools.is_none() || self.menus.game_menu.is_some() {
            return false;
        }
        if pressed && self.shell.over_ui || !pressed && self.shell.ui.input.down {
            self.shell.button(pressed);
            return true;
        }
        if pressed {
            // (a click into the picture: the text field lets the keyboard go)
            self.shell.ui.focus = None;
        }
        false
    }

    /// The wheel over the panel scrolls it.
    pub(crate) fn devtools_wheel(&mut self, amount: f32) -> bool {
        if self.devtools.is_some() && self.menus.game_menu.is_none() && self.shell.over_ui {
            self.shell.wheel(amount);
            return true;
        }
        false
    }

    /// The developer tools' frame: their files looked at, the overlays and the panel drawn
    /// over the picture (in place of the pause menu's screens, which are closed).
    pub(crate) fn frame_devtools(&mut self, dt: f32) {
        let sim = self.player_to_drawn_pose();
        self.devtools_now(dt);
        self.player_back_to_sim_pose(sim);
    }

    fn devtools_now(&mut self, dt: f32) {
        let Some(s) = self.gfx.surface.as_ref() else { return };
        let (w, h) = (s.config.width as f32, s.config.height as f32);
        let dpi = self.window.as_ref().map(|w| w.scale_factor() as f32).unwrap_or(1.0);
        let scale = dpi * crate::ui::size_factor(h, dpi, self.settings.ui_scale, self.settings.ui_scale_window);
        self.devtools_watch_files();
        // (a bus changed under the tools: their lists are of the new one)
        let bus = self.player.as_ref().map(|p| p.vehicle.ty.def.path.clone()).unwrap_or_default();
        if self.devtools.as_ref().is_some_and(|d| d.bus != bus) {
            let keep = self.devtools.as_ref().map(|d| (d.tab, d.overlays));
            let mut d = DevTools::new(self.player.as_ref());
            if let Some((tab, overlays)) = keep {
                (d.tab, d.overlays) = (tab, overlays);
            }
            self.devtools = Some(Box::new(d));
        }
        self.shell.opaque = false;
        self.shell.begin(w, h, scale, dt);
        let size = Vec2::new(w, h) / self.shell.scale;
        let requests = {
            let Some(d) = self.devtools.as_deref_mut() else { return };
            if let (true, Some(cam), Some(p)) = (d.overlays.any(), self.camera.as_ref(), self.player.as_ref()) {
                let screen = overlay::Screen::new(cam, size);
                let v = &p.vehicle;
                let mut parts = vec![overlay::Part { def: &v.ty.def, model: &v.ty.model, files: d.parts.first(), position: v.position, rot: v.body_rotation() }];
                for (k, t) in v.trailers.iter().enumerate() {
                    parts.push(overlay::Part { def: &t.ty.def, model: &t.ty.model, files: d.parts.get(k + 1), position: t.position, rot: t.body_rotation() });
                }
                overlay::draw(&mut self.shell.ui, &screen, &d.overlays, &parts, &|n| v.var(n));
            }
            let out = panel::draw(&mut self.shell.ui, d, self.player.as_mut(), size);
            // (the text fields are on the variables' page: on another one nothing types)
            if d.tab != 0 {
                self.shell.ui.focus = None;
            }
            out
        };
        self.shell.finish();
        for r in requests {
            self.devtools_request(r);
        }
    }

    fn devtools_request(&mut self, r: Request) {
        match r {
            Request::Reload(kind) => self.devtools_reload(kind),
            Request::OpenLog => {
                let log = omsi_launcher_lib::data_dir().join("game.log");
                open_file(&log);
                if let Some(d) = self.devtools.as_mut() {
                    d.say(format!("opened {}", log.display()));
                }
            }
            Request::CopyMirror(i) => {
                let block = self.player.as_ref().and_then(|p| panel::mirror_block(p, i));
                if let (Some(block), Some(d)) = (block, self.devtools.as_mut()) {
                    let ok = copy_text(&block);
                    log::info!("developer tools: mirror {i}:\n{block}");
                    d.say(if ok { format!("mirror {i}: its [add_camera_reflexion_2] block copied") } else { format!("mirror {i}: the clipboard is not there - the block is in the log") });
                }
            }
            Request::Close => self.toggle_devtools(),
        }
    }

    /// The input scripts' `devtools ...`: `open`, `close`, `tab <n>`, `overlay <name>|all|none`,
    /// `watch <name>` (`$name` a string variable), `set <name>=<value>`, `hold <name>=<value>`,
    /// `release <name>`, `filter <text>`, `mirror <n> <dx>,<dy>,<dz>,<dyaw>,<dpitch>,<dfov>`,
    /// `copy <n>`, `reload paths|sound|bus|model|const`.
    pub(crate) fn devtools_command(&mut self, arg: &str) {
        let (verb, rest) = arg.split_once(' ').map(|(a, b)| (a, b.trim())).unwrap_or((arg, ""));
        if verb == "open" || verb == "close" {
            if (verb == "open") != self.devtools.is_some() {
                self.toggle_devtools();
            }
            return;
        }
        if self.devtools.is_none() {
            self.toggle_devtools();
        }
        let name_of = |s: &str| match s.strip_prefix('$') {
            Some(n) => (n.to_string(), true),
            None => (s.to_string(), false),
        };
        match verb {
            "tab" => {
                if let Some(d) = self.devtools.as_mut() {
                    d.tab = rest.parse::<usize>().unwrap_or(0).min(panel::TABS.len() - 1);
                }
            }
            "overlay" => {
                if let Some(d) = self.devtools.as_mut() {
                    match rest {
                        "all" => d.overlays.on = [true; overlay::NAMES.len()],
                        "none" => d.overlays.on = [false; overlay::NAMES.len()],
                        k => match overlay::KEYS.iter().position(|x| *x == k) {
                            Some(i) => d.overlays.on[i] = !d.overlays.on[i],
                            None => log::warn!("devtools: no overlay {k} (one of {:?})", overlay::KEYS),
                        },
                    }
                }
            }
            "watch" => {
                let (n, string) = name_of(rest);
                if let Some(d) = self.devtools.as_mut() {
                    d.add_watch(&n, string);
                }
            }
            "set" | "hold" => {
                let Some((n, v)) = rest.split_once('=') else { return };
                let (n, string) = name_of(n.trim());
                let ok = self.player.as_mut().is_some_and(|p| set_value(&mut p.vehicle, &n, string, v.trim()));
                if let Some(d) = self.devtools.as_mut() {
                    d.add_watch(&n, string);
                    if verb == "hold" {
                        if let Some(k) = d.selected {
                            d.watch[k].hold = Some(v.trim().to_string());
                        }
                    }
                }
                log::info!("devtools: {verb} {n} = {} -> {ok}", v.trim());
            }
            "release" => {
                let (n, string) = name_of(rest);
                if let Some(w) = self.devtools.as_mut().and_then(|d| d.watch.iter_mut().find(|w| w.name.eq_ignore_ascii_case(&n) && w.string == string)) {
                    w.hold = None;
                }
            }
            "filter" => {
                if let Some(d) = self.devtools.as_mut() {
                    d.filter = rest.to_string();
                }
            }
            "mirror" => {
                let mut it = rest.split_whitespace();
                let i = it.next().and_then(|x| x.parse::<usize>().ok()).unwrap_or(0);
                let v: Vec<f32> = it.next().unwrap_or("").split(',').filter_map(|x| x.trim().parse().ok()).collect();
                if let Some(d) = self.devtools.as_mut() {
                    d.mirror = i;
                    d.tab = 2;
                }
                if let (Some(p), 6) = (self.player.as_mut(), v.len()) {
                    panel::adjust_mirror(p, i, [v[0], v[1], v[2]], [v[3], v[4]], v[5]);
                }
            }
            "copy" => self.devtools_request(Request::CopyMirror(rest.parse().unwrap_or(0))),
            "reload" => match Kind::from_name(rest) {
                Some(k) => self.devtools_reload(k),
                None => log::warn!("devtools: reload what? ({rest})"),
            },
            _ => log::warn!("devtools: unknown command {arg}"),
        }
    }
}

/// The variables held by the open developer tools, put into the bus (see
/// [`DevTools::apply_holds`]); nothing when they are closed.
pub(crate) fn apply_holds(d: Option<&DevTools>, v: &mut omsi_sim::VehicleInstance) {
    if let Some(d) = d {
        d.apply_holds(v);
    }
}

/// Text onto the system's clipboard; false where there is none.
fn copy_text(text: &str) -> bool {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        thread_local! {
            // (kept alive: on X11 the text is gone when the clipboard is dropped)
            static CLIPBOARD: std::cell::RefCell<Option<arboard::Clipboard>> = const { std::cell::RefCell::new(None) };
        }
        CLIPBOARD.with(|c| {
            let mut c = c.borrow_mut();
            if c.is_none() {
                *c = arboard::Clipboard::new().ok();
            }
            c.as_mut().is_some_and(|cb| cb.set_text(text.to_string()).is_ok())
        })
    }
    #[cfg(target_os = "ios")]
    {
        crate::apple::set_clipboard(text)
    }
    #[cfg(target_os = "android")]
    {
        let _ = text;
        false
    }
}

/// Open a file with the program the system gives its kind (a text editor for the log).
fn open_file(path: &std::path::Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd").args(["/C", "start", ""]).arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos"), not(target_os = "android"), not(target_os = "ios")))]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    #[cfg(any(target_os = "android", target_os = "ios"))]
    let _ = path;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_watch_list_is_kept_as_names_a_line() {
        let list = vec![
            Watch { name: "engine_n".into(), string: false, hold: Some("900".into()) },
            Watch { name: "IBIS_Linie".into(), string: true, hold: None },
        ];
        let back = parse_watch(&watch_text(&list));
        assert_eq!(back.iter().map(|w| (w.name.as_str(), w.string)).collect::<Vec<_>>(), [("engine_n", false), ("IBIS_Linie", true)]);
        assert!(back.iter().all(|w| w.hold.is_none()), "what was held is not kept");
    }

    #[test]
    fn keys_without_text_type_names_and_numbers() {
        assert_eq!(key_char(KeyCode::KeyA, false), Some('a'));
        assert_eq!(key_char(KeyCode::KeyA, true), Some('A'));
        assert_eq!(key_char(KeyCode::Digit7, false), Some('7'));
        assert_eq!(key_char(KeyCode::Numpad3, false), Some('3'));
        assert_eq!(key_char(KeyCode::Minus, true), Some('_'));
        assert_eq!(key_char(KeyCode::Period, false), Some('.'));
        assert_eq!(key_char(KeyCode::F5, false), None);
    }
}
