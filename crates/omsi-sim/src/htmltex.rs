use crate::vehicle::VehicleInstance;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// What the pointer (a finger, the mouse) does on the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerKind {
    Down,
    Up,
    Move,
}

/// Something a page asks of the vehicle beyond a variable or a trigger: the IBIS duty.
/// The game applies it (`VehicleInstance::take_html_requests`), since only it knows the
/// driver's IBIS keys.
#[derive(Clone, Debug, PartialEq)]
pub enum HtmlRequest {
    /// `omsi.setRoute(index)`: line, route and destination of `omsi.depot.routes[index]`.
    SetRoute(usize),
    /// `omsi.setLine(text)`: the first route of that line in the depot file.
    SetLine(String),
    /// `omsi.setDestination(index)`: the destination sign, `omsi.depot.destinations[index]`.
    SetDestination(usize),
    /// `omsi.clearLine()`: the IBIS shows no line (before a route is started).
    ClearLine,
    /// `omsi.setNextStop(index)`: the duty goes on with stop `index` of its trip (stops
    /// before it are skipped; an earlier stop makes the stops from there on due again).
    SetNextStop(usize),
}

/// What `window.omsi` offers a page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageApi {
    /// A vehicle's page: the vehicle (`omsi.vehicle`), the depot and the IBIS requests
    /// (`omsi.setRoute` ...), besides the basic API.
    Vehicle,
    /// A scenery object's page: only the basic API - `omsi.setVar`, `.trigger`, `.getVar`,
    /// `.vars`, `.time`, `.date` and `.locale`.
    Scenery,
}

/// Most requests kept for the game between two of its frames (a page that asks in a loop).
const MAX_REQUESTS: usize = 32;

pub trait HtmlRenderer: Send {
    fn set_vars(&mut self, num: &[(String, f32)], strs: &[(String, String)]);
    fn poll_frame(&mut self) -> Option<Vec<u8>>;
    fn take_events(&mut self) -> Vec<(String, f32)>;
    /// Triggers the page pressed (`omsi.trigger(name)`) since the last call.
    fn take_triggers(&mut self) -> Vec<String> {
        Vec::new()
    }
    /// The pointer is at (`x`, `y`) in texture pixels. A backend that cannot be operated
    /// leaves this as it is.
    fn pointer(&mut self, _x: f32, _y: f32, _kind: PointerKind) {}
    /// The normalised state of the vehicle (`window.omsi.vehicle`, see
    /// [`crate::vehicle_api`]). Called before [`Self::set_vars`] whenever it changed. A
    /// backend that has no such object leaves this as it is.
    fn set_vehicle(&mut self, _api: &crate::vehicle_api::ApiValue) {}
    /// `omsi.time`, `omsi.date` and `omsi.locale` (see [`crate::vehicle_api::environment`]): a
    /// map whose entries become properties of `window.omsi`. Called before [`Self::set_vars`]
    /// whenever it changed.
    fn set_env(&mut self, _env: &crate::vehicle_api::ApiValue) {}
    /// The depot file as a page sees it (`window.omsi.depot`, see [`crate::vehicle_api::depot`]).
    /// Called once, before the first update.
    fn set_depot(&mut self, _depot: &crate::vehicle_api::ApiValue) {}
    /// The folders a page's pictures (`<img src>`, `url(...)`) are looked up in, in order.
    /// Called once, right after the page is created. A backend without pictures leaves it.
    fn set_asset_dirs(&mut self, _dirs: Vec<PathBuf>) {}
    /// Route, line and destination requests the page made since the last call.
    fn take_requests(&mut self) -> Vec<HtmlRequest> {
        Vec::new()
    }
    /// The departures of the stops the page asked for (`window.omsi.departures`, see
    /// [`crate::vehicle_api::departures`]). Called before [`Self::set_vars`] whenever it changed.
    fn set_departures(&mut self, _departures: &crate::vehicle_api::ApiValue) {}
    /// The stops the page asked departures for (`omsi.getDepartures(stop)`) since the last call,
    /// as keys: trimmed, lower case.
    fn take_departure_wants(&mut self) -> Vec<String> {
        Vec::new()
    }
}

pub type BackendFactory = fn(width: u32, height: u32, html: &str, api: PageApi) -> Box<dyn HtmlRenderer>;

static BACKEND: OnceLock<BackendFactory> = OnceLock::new();

pub fn set_backend(factory: BackendFactory) -> bool {
    BACKEND.set(factory).is_ok()
}

fn make_renderer(width: u32, height: u32, html: &str, api: PageApi) -> Box<dyn HtmlRenderer> {
    match BACKEND.get() {
        Some(f) => f(width, height, html, api),
        None => {
            static WARNED: std::sync::Once = std::sync::Once::new();
            WARNED.call_once(|| log::warn!("htmltexture: no page backend registered, the pages stay empty"));
            Box::new(NoPage)
        }
    }
}

/// The page of a game that registered no backend ([`set_backend`]: the game registers
/// omsi-htmlengine's at start): it draws nothing and does nothing.
struct NoPage;

impl HtmlRenderer for NoPage {
    fn set_vars(&mut self, _num: &[(String, f32)], _strs: &[(String, String)]) {}
    fn poll_frame(&mut self) -> Option<Vec<u8>> {
        None
    }
    fn take_events(&mut self) -> Vec<(String, f32)> {
        Vec::new()
    }
}

/// Find `rel` (a `model.cfg` path, backslashes, any letter case) under the first of `dirs`
/// that has it: the model folder first, then the vehicle folder, then the model folder's
/// parent.
fn find_file(dirs: &[&Path], rel: &str) -> Option<PathBuf> {
    let mut all: Vec<PathBuf> = dirs.iter().map(|d| d.to_path_buf()).collect();
    if let Some(parent) = dirs.first().and_then(|d| d.parent()) {
        all.push(parent.to_path_buf());
    }
    all.iter().map(|d| omsi_cfg::resolve_path(d, rel)).find(|p| omsi_cfg::vfs::is_file(p))
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(p) = lower[from..].find(name) {
        let at = from + p;
        from = at + name.len();
        let before_ok = at == 0 || lower.as_bytes()[at - 1].is_ascii_whitespace();
        let rest = tag[from..].trim_start();
        if !before_ok || !rest.starts_with('=') {
            continue;
        }
        let rest = rest[1..].trim_start();
        return match rest.chars().next()? {
            q @ ('"' | '\'') => rest[1..].split(q).next().map(str::to_string),
            _ => rest.split(|c: char| c.is_whitespace() || c == '>').next().map(str::to_string),
        };
    }
    None
}

/// The folder part of a style sheet's `href` (`"css/"` for `"css/style.css"`), empty when
/// the sheet lies next to the page.
fn css_dir(href: &str) -> &str {
    let h = href.split(['?', '#']).next().unwrap_or(href);
    match h.rfind(['/', '\\']) {
        Some(p) => &h[..=p],
        None => "",
    }
}

/// A style sheet is pasted into the page, so its `url(...)` paths, which are relative to the
/// sheet, get the sheet's folder (`prefix`) put in front: they then read relative to the page.
pub(crate) fn rebase_css_urls(css: &str, prefix: &str) -> String {
    if prefix.is_empty() {
        return css.to_string();
    }
    let lower = css.to_ascii_lowercase();
    let mut out = String::with_capacity(css.len() + 64);
    let mut i = 0;
    while let Some(p) = lower[i..].find("url(") {
        let inner = i + p + 4;
        let Some(e) = css[inner..].find(')').map(|e| inner + e) else { break };
        out.push_str(&css[i..inner]);
        let raw = css[inner..e].trim();
        let (quote, path) = match raw.chars().next() {
            Some(c @ ('"' | '\'')) => (c.to_string(), raw.trim_matches(c)),
            _ => (String::new(), raw),
        };
        let absolute = path.is_empty() || path.starts_with(['/', '\\', '#']) || path.starts_with("data:") || path.contains("://");
        if absolute {
            out.push_str(&css[inner..e]);
        } else {
            out.push_str(&quote);
            out.push_str(prefix);
            out.push_str(path);
            out.push_str(&quote);
        }
        i = e;
    }
    out.push_str(&css[i..]);
    out
}

/// The folders the pictures of a page are looked up in: the page's own folder, the
/// model folder, the vehicle folder and the model folder's parent (the order of `load_page`).
pub fn asset_dirs(dirs: &[&Path], rel: &str) -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(d) = find_file(dirs, rel).and_then(|p| p.parent().map(Path::to_path_buf)) {
        v.push(d);
    }
    v.extend(dirs.iter().map(|d| d.to_path_buf()));
    if let Some(parent) = dirs.first().and_then(|d| d.parent()) {
        v.push(parent.to_path_buf());
    }
    v
}

/// Read the page of an `[htmltexture]` and put its external style sheets and scripts
/// (`<link rel="stylesheet" href>`, `<script src>`) into it, so the engine sees one file.
/// A missing page gives an empty one (a blank texture, logged).
pub fn load_page(dirs: &[&Path], rel: &str) -> String {
    let Some(path) = find_file(dirs, rel) else {
        log::warn!("htmltexture: page {rel} not found");
        return String::new();
    };
    log::debug!("htmltexture: page {rel} -> {}", path.display());
    let Ok(bytes) = omsi_cfg::vfs::read(&path) else {
        log::debug!("htmltexture: {} cannot be read", path.display());
        return String::new();
    };
    let html = String::from_utf8_lossy(&bytes).trim_start_matches('\u{feff}').to_string();
    let base = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut bases: Vec<&Path> = vec![base.as_path()];
    bases.extend(dirs.iter().copied());
    let read = |href: &str| -> Option<String> {
        let href = href.split(['?', '#']).next().unwrap_or(href);
        let text = find_file(&bases, href)
            .and_then(|p| omsi_cfg::vfs::read(&p).ok())
            .map(|b| String::from_utf8_lossy(&b).trim_start_matches('\u{feff}').to_string());
        match &text {
            Some(t) => log::debug!("htmltexture: inlined {href} ({} bytes)", t.len()),
            None => log::debug!("htmltexture: {href} not found, left as it is"),
        }
        text
    };
    let mut out = String::with_capacity(html.len());
    let mut rest = html.as_str();
    loop {
        let lower = rest.to_ascii_lowercase();
        let next = [lower.find("<link"), lower.find("<script")].into_iter().flatten().min();
        let Some(at) = next else { break };
        out.push_str(&rest[..at]);
        let Some(end) = rest[at..].find('>').map(|e| at + e + 1) else {
            rest = &rest[at..];
            break;
        };
        let tag = &rest[at..end];
        let is_link = lower[at..].starts_with("<link");
        if is_link {
            let sheet = attr(tag, "rel").map_or(false, |r| r.to_ascii_lowercase().contains("stylesheet"));
            let loaded = attr(tag, "href").filter(|_| sheet).and_then(|h| read(&h).map(|css| rebase_css_urls(&css, css_dir(&h))));
            match loaded {
                Some(css) => out.push_str(&format!("<style>{css}</style>")),
                None => out.push_str(tag),
            }
            rest = &rest[end..];
        } else if let Some((src, js)) = attr(tag, "src").and_then(|s| read(&s).map(|t| (s, t))) {
            // (a module keeps its kind and the file it came from: the imports of a module are
            // relative to it. A plain script is inlined exactly as it always was)
            let module = attr(tag, "type").is_some_and(|t| t.eq_ignore_ascii_case("module"));
            if module {
                out.push_str(&format!("<script type=\"module\" data-omsi-src=\"{}\">{js}</script>", src.split(['?', '#']).next().unwrap_or(&src).replace('"', "")));
            } else {
                out.push_str(&format!("<script>{js}</script>"));
            }
            let after = &rest[end..];
            let close = after.to_ascii_lowercase().find("</script>").map(|c| c + "</script>".len()).unwrap_or(0);
            rest = &after[close..];
        } else {
            // an inline script (or one that cannot be read) is copied whole, so text inside
            // it that looks like a tag (`"<link ..."`) is not taken for one
            let after = &rest[end..];
            let close = after.to_ascii_lowercase().find("</script>").map(|c| c + "</script>".len()).unwrap_or(after.len());
            out.push_str(tag);
            out.push_str(&after[..close]);
            rest = &after[close..];
        }
    }
    out.push_str(rest);
    log::debug!("htmltexture: page {rel}: {} bytes after inlining", out.len());
    out
}

pub struct HtmlTexture {
    pub script_index: usize,
    pub width: u32,
    pub height: u32,
    renderer: Box<dyn HtmlRenderer>,
    /// The variables the page has seen, by index (None: not given yet).
    last_num: Vec<Option<f32>>,
    last_str: Vec<Option<String>>,
    /// The vehicle snapshot the page has seen.
    last_api: Option<crate::vehicle_api::ApiValue>,
    /// The time, date and locale the page has seen.
    last_env: Option<crate::vehicle_api::ApiValue>,
    /// The departures the page has seen.
    last_departures: Option<crate::vehicle_api::ApiValue>,
    started: bool,
}

impl HtmlTexture {
    /// A vehicle's page.
    pub fn new(script_index: usize, width: i32, height: i32, html: &str) -> HtmlTexture {
        HtmlTexture::with_api(script_index, width, height, html, PageApi::Vehicle)
    }

    pub fn with_api(script_index: usize, width: i32, height: i32, html: &str, api: PageApi) -> HtmlTexture {
        let (w, h) = (width.max(1) as u32, height.max(1) as u32);
        log::debug!("htmltexture #{script_index}: {w}x{h}, page of {} bytes", html.len());
        HtmlTexture {
            script_index,
            width: w,
            height: h,
            renderer: make_renderer(w, h, html, api),
            last_num: Vec::new(),
            last_str: Vec::new(),
            last_api: None,
            last_env: None,
            last_departures: None,
            started: false,
        }
    }
}

impl HtmlTexture {
    /// Tell the page where its pictures are (see [`asset_dirs`]).
    pub fn with_asset_dirs(mut self, dirs: Vec<PathBuf>) -> HtmlTexture {
        self.renderer.set_asset_dirs(dirs);
        self
    }

    /// The pointer at (`u`, `v`), both 0..1 across the texture (`v` down from the top).
    pub fn pointer(&mut self, u: f32, v: f32, kind: PointerKind) {
        self.renderer.pointer(u * self.width as f32, v * self.height as f32, kind);
    }
}

/// What the pages of one owner (a vehicle, a scenery object) did in one update.
#[derive(Default)]
pub struct PageOutput {
    pub events: Vec<(String, f32)>,
    pub triggers: Vec<String>,
    pub requests: Vec<HtmlRequest>,
    /// The stops the pages asked departures for (keys: trimmed, lower case).
    pub departure_wants: Vec<String>,
    /// New pictures: (script texture index, width, height, RGBA).
    pub frames: Vec<(usize, u32, u32, Vec<u8>)>,
}

/// The owner's variables for its pages: names and values side by side, by index (the
/// program's numeric and string variables).
pub(crate) struct PageVars<'a> {
    pub num_names: &'a [String],
    pub num: &'a [f32],
    pub str_names: &'a [String],
    pub strs: &'a [String],
}

/// Give every page what changed since it last looked (variables, the vehicle's state when
/// `api` is given, time), and collect what the pages did (variables set, triggers pressed, requests) and
/// their new pictures. Shared by vehicles and scenery objects. The variables are compared by
/// index with what the page saw last; only the changed ones are copied (every name and value
/// was, every frame).
pub(crate) fn drive_pages(
    pages: &mut [HtmlTexture],
    vars: &PageVars<'_>,
    api: Option<&crate::vehicle_api::ApiValue>,
    env: &crate::vehicle_api::ApiValue,
    depot: Option<&crate::vehicle_api::ApiValue>,
    departures: Option<&crate::vehicle_api::ApiValue>,
) -> PageOutput {
    let mut out = PageOutput::default();
    for t in pages.iter_mut() {
        let api_changed = api.is_some_and(|a| t.last_api.as_ref() != Some(a));
        let env_changed = t.last_env.as_ref() != Some(env);
        let departures_changed = departures.is_some_and(|d| t.last_departures.as_ref() != Some(d));
        // (`!=`, as before: a NaN is given to the page again every time)
        let dn: Vec<(String, f32)> = vars
            .num_names
            .iter()
            .zip(vars.num)
            .enumerate()
            .filter(|(i, (_, v))| t.last_num.get(*i).copied().flatten().is_none_or(|last| last != **v))
            .map(|(_, (n, v))| (n.clone(), *v))
            .collect();
        let ds: Vec<(String, String)> = vars
            .str_names
            .iter()
            .zip(vars.strs)
            .enumerate()
            .filter(|(i, (_, v))| t.last_str.get(*i).and_then(|l| l.as_ref()) != Some(*v))
            .map(|(_, (n, v))| (n.clone(), v.clone()))
            .collect();
        if !t.started || !dn.is_empty() || !ds.is_empty() || api_changed || env_changed || departures_changed {
            log::debug!(
                "htmltexture #{}: {} numeric and {} string variable(s) to the page{}",
                t.script_index,
                dn.len(),
                ds.len(),
                if t.started { "" } else { " (first update)" }
            );
            if !t.started && api.is_some() {
                match depot {
                    Some(d) => {
                        log::info!("htmltexture #{}: omsi.depot set on the page", t.script_index);
                        t.renderer.set_depot(d);
                    }
                    None => log::debug!("htmltexture #{}: first update without a depot: omsi.depot is empty", t.script_index),
                }
            }
            if let Some(a) = api.filter(|_| api_changed) {
                t.renderer.set_vehicle(a);
                t.last_api = Some(a.clone());
            }
            if env_changed {
                t.renderer.set_env(env);
                t.last_env = Some(env.clone());
            }
            if let Some(d) = departures.filter(|_| departures_changed) {
                t.renderer.set_departures(d);
                t.last_departures = Some(d.clone());
            }
            t.renderer.set_vars(&dn, &ds);
            t.last_num.resize(vars.num.len(), None);
            for (last, v) in t.last_num.iter_mut().zip(vars.num) {
                *last = Some(*v);
            }
            t.last_str.resize(vars.strs.len(), None);
            for (last, v) in t.last_str.iter_mut().zip(vars.strs) {
                if last.as_ref() != Some(v) {
                    *last = Some(v.clone());
                }
            }
            t.started = true;
        }
        let page_events = t.renderer.take_events();
        if !page_events.is_empty() {
            log::debug!("htmltexture #{}: the page sets {:?}", t.script_index, page_events);
        }
        out.events.extend(page_events);
        out.triggers.extend(t.renderer.take_triggers());
        out.requests.extend(t.renderer.take_requests());
        for key in t.renderer.take_departure_wants() {
            if !out.departure_wants.contains(&key) {
                out.departure_wants.push(key);
            }
        }
        if let Some(rgba) = t.renderer.poll_frame() {
            log::debug!("htmltexture #{}: new frame of {} bytes", t.script_index, rgba.len());
            out.frames.push((t.script_index, t.width, t.height, rgba));
        }
    }
    out
}

/// The pages of a scenery object (`[htmltexture]` in its model), see
/// [`crate::scenery::SceneryInstance`].
pub fn scenery_pages(defs: &[omsi_model::HtmlTextureDef], model_dir: &Path, object_dir: &Path) -> Vec<HtmlTexture> {
    defs.iter()
        .map(|d| {
            let dirs = [model_dir, object_dir];
            let html = load_page(&dirs, &d.path);
            HtmlTexture::with_api(d.script_index, d.width, d.height, &html, PageApi::Scenery).with_asset_dirs(asset_dirs(&dirs, &d.path))
        })
        .collect()
}

/// A press, release or move on the page `script_index`; what it does to the variables
/// comes back as (variables set, triggers pressed). Shared by scenery objects.
pub(crate) fn pointer_on(
    pages: &mut [HtmlTexture],
    script_index: usize,
    u: f32,
    v: f32,
    kind: PointerKind,
) -> Option<(Vec<(String, f32)>, Vec<String>)> {
    let t = pages.iter_mut().find(|t| t.script_index == script_index)?;
    t.pointer(u, v, kind);
    Some((t.renderer.take_events(), t.renderer.take_triggers()))
}

impl VehicleInstance {
    /// Pass a press, release or move on an `[htmltexture]` (given by its script texture
    /// index) to its page. `u`/`v` are 0..1 across the texture, `v` down from the top.
    /// False when the vehicle has no such page. What the page does with it
    /// (`omsi.setVar`, `omsi.trigger`) is applied to the vehicle at once.
    pub fn html_pointer(&mut self, script_index: usize, u: f32, v: f32, kind: PointerKind) -> bool {
        let Some(t) = self.html_textures.iter_mut().find(|t| t.script_index == script_index) else {
            return false;
        };
        t.pointer(u, v, kind);
        let events = t.renderer.take_events();
        let triggers = t.renderer.take_triggers();
        let requests = t.renderer.take_requests();
        self.queue_html_requests(requests);
        for (name, value) in events {
            if !self.set_var(&name, value) {
                log::debug!("htmltexture: the page sets {name}, which the vehicle does not have");
            }
        }
        for name in triggers {
            if !self.trigger(&name) {
                log::debug!("htmltexture: the page presses {name}, which the vehicle does not have");
            }
        }
        true
    }

    /// What the pages asked of the IBIS (route, line, destination) since the last call.
    pub fn take_html_requests(&mut self) -> Vec<HtmlRequest> {
        std::mem::take(&mut self.host.html_requests)
    }

    fn queue_html_requests(&mut self, requests: Vec<HtmlRequest>) {
        for r in requests {
            if self.host.html_requests.len() < MAX_REQUESTS {
                log::debug!("htmltexture: page asks {r:?}");
                self.host.html_requests.push(r);
            }
        }
    }

    pub fn update_html_textures(&mut self) {
        if self.host.remote_script_textures || self.html_textures.is_empty() {
            return;
        }
        // the depot file, for the pages that start now
        let depot = if self.html_textures.iter().any(|t| !t.started) {
            match self.host.hof.as_ref() {
                Some(h) => {
                    log::info!("htmltexture: page starts, depot '{}' ({}) goes to omsi.depot", h.name.trim(), h.path.display());
                    Some(crate::vehicle_api::depot(h))
                }
                None => {
                    log::warn!("htmltexture: page starts, but the vehicle has no depot file (host.hof is None): omsi.depot stays empty");
                    None
                }
            }
        } else {
            None
        };
        let mut requests: Vec<HtmlRequest> = Vec::new();
        let p = self.ty.program.clone();
        let vars = PageVars {
            num_names: &p.var_names,
            num: &self.state.vars[..p.var_names.len()],
            str_names: &p.str_var_names,
            strs: &self.state.str_vars[..p.str_var_names.len()],
        };
        // one snapshot of the vehicle for all pages
        let api = self.html_api_snapshot();
        let env = self.html_env_snapshot();
        let departures = (!self.host.html_departures.is_empty()).then(|| crate::vehicle_api::departures(&self.host.html_departures));
        let out = drive_pages(&mut self.html_textures, &vars, Some(&api), &env, depot.as_ref(), departures.as_ref());
        for key in out.departure_wants {
            self.host.want_departures(key);
        }
        let (events, triggers, frames) = (out.events, out.triggers, out.frames);
        requests.extend(out.requests);
        self.queue_html_requests(requests);
        for (name, v) in events {
            if !self.set_var(&name, v) {
                log::debug!("htmltexture: the page sets {name}, which the vehicle does not have");
            }
        }
        for name in triggers {
            if !self.trigger(&name) {
                log::debug!("htmltexture: the page presses {name}, which the vehicle does not have");
            }
        }
        for (index, w, h, rgba) in frames {
            match self.host.script_textures.get_mut(index) {
                Some(st) if st.width == w && st.height == h && st.rgba.len() == rgba.len() => {
                    st.rgba = rgba;
                    st.dirty = true;
                }
                Some(st) => log::debug!(
                    "htmltexture #{index}: frame {w}x{h} does not fit the script texture {}x{}",
                    st.width,
                    st.height
                ),
                None => log::debug!("htmltexture #{index}: no script texture with this index"),
            }
        }
    }
}
#[cfg(test)]
mod page_vars_tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    type Sent = Arc<Mutex<Vec<(Vec<(String, f32)>, Vec<(String, String)>)>>>;

    struct Recorder(Sent);

    impl HtmlRenderer for Recorder {
        fn set_vars(&mut self, num: &[(String, f32)], strs: &[(String, String)]) {
            self.0.lock().unwrap().push((num.to_vec(), strs.to_vec()));
        }
        fn poll_frame(&mut self) -> Option<Vec<u8>> {
            None
        }
        fn take_events(&mut self) -> Vec<(String, f32)> {
            Vec::new()
        }
    }

    /// A page is given all variables first and then only the ones that changed (a NaN
    /// every time, as it never equals what was given), in the order of the variables.
    #[test]
    fn a_page_gets_what_changed_by_index() {
        let sent: Sent = Arc::default();
        let mut page = HtmlTexture::new(0, 4, 4, "");
        page.renderer = Box::new(Recorder(sent.clone()));
        let mut pages = [page];
        let names: Vec<String> = ["a", "B", "c"].map(String::from).to_vec();
        let str_names: Vec<String> = ["s", "t"].map(String::from).to_vec();
        let env = crate::vehicle_api::ApiValue::Null;
        let mut drive = |num: [f32; 3], strs: [&str; 2]| {
            let strs = strs.map(String::from);
            let vars = PageVars { num_names: &names, num: &num, str_names: &str_names, strs: &strs };
            drive_pages(&mut pages, &vars, None, &env, None, None);
        };
        let s = |v: &[(&str, &str)]| -> Vec<(String, String)> { v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect() };
        drive([1.0, 2.0, 3.0], ["x", ""]);
        drive([1.0, 2.0, 3.0], ["x", ""]);
        drive([1.0, 5.0, f32::NAN], ["x", "y"]);
        drive([1.0, 5.0, f32::NAN], ["x", "y"]);
        drive([0.0, 5.0, 3.0], ["z", "y"]);
        let got = sent.lock().unwrap().clone();
        let num = |v: &[(&str, f32)]| -> Vec<(String, f32)> { v.iter().map(|(a, b)| (a.to_string(), *b)).collect() };
        assert_eq!(got.len(), 4, "{got:?}");
        assert_eq!(got[0], (num(&[("a", 1.0), ("B", 2.0), ("c", 3.0)]), s(&[("s", "x"), ("t", "")])));
        assert_eq!(got[1].0[0], ("B".to_string(), 5.0));
        assert!(got[1].0.len() == 2 && got[1].0[1].0 == "c" && got[1].0[1].1.is_nan());
        assert_eq!(got[1].1, s(&[("t", "y")]));
        assert!(got[2].0.len() == 1 && got[2].0[0].1.is_nan() && got[2].1.is_empty());
        assert_eq!(got[3], (num(&[("a", 0.0), ("c", 3.0)]), s(&[("s", "z")])));
    }
}

#[cfg(test)]
mod css_url_tests {
    use super::rebase_css_urls;

    #[test]
    fn style_sheet_urls_are_made_relative_to_the_page() {
        let css = "a{background:url(../img/x.png)} b{background:url('y.png')} c{background:url(data:x)} d{background:url(/abs.png)}";
        assert_eq!(
            rebase_css_urls(css, "css/"),
            "a{background:url(css/../img/x.png)} b{background:url('css/y.png')} c{background:url(data:x)} d{background:url(/abs.png)}"
        );
        assert_eq!(rebase_css_urls(css, ""), css);
    }
}
