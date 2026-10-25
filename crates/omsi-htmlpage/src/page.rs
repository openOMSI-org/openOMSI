//! A page running: its DOM and layout (`PageState`), its script context, and the host
//! primitives the scripts reach the game through (`__host`, used by `prelude.js`).
//!
//! The scripts run in Boa, a complete JavaScript engine. Its context is not `Send`, so a page
//! lives on a thread of its own (see `renderer`); everything here is single-threaded.

use super::*;
use boa_engine::module::{ModuleLoader, Referrer};
use boa_engine::{js_string, Context, JsError, JsNativeError, JsResult, JsString, JsValue, Module, NativeFunction, Source};
use boa_engine::builtins::promise::PromiseState;
use std::cell::RefCell;
use std::rc::Rc;

/// Most stops one page keeps asking departures for.
const MAX_DEPARTURE_WANTS: usize = 8;
/// Most a page may read through `fetch` (bytes).
const MAX_FETCH: usize = 4 << 20;
/// Iterations of one loop, nested calls and stack a script may use before it is stopped.
const LOOP_LIMIT: u64 = 5_000_000;
const RECURSION_LIMIT: usize = 400;
const STACK_LIMIT: usize = 4096;

const PRELUDE: &str = include_str!("prelude.js");

/// The laid-out body and the root style it was built with.
pub(crate) struct LayoutCache {
    pub(crate) generation: u64,
    pub(crate) root: Style,
    pub(crate) b: LBox,
}

/// What a page is, apart from its script context.
pub(crate) struct PageState {
    pub(crate) dom: Dom,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) imgs: Arc<ImageStore>,
    pub(crate) dirs: Vec<std::path::PathBuf>,
    pub(crate) cache: Option<LayoutCache>,
    pub(crate) api: omsi_sim::htmltex::PageApi,
    /// Seconds since the page started; set before anything runs.
    pub(crate) now: f64,
    pub(crate) events: Vec<(String, f32)>,
    pub(crate) triggers: Vec<String>,
    pub(crate) requests: Vec<omsi_sim::htmltex::HtmlRequest>,
    pub(crate) departure_wants: Vec<String>,
}

impl PageState {
    /// The root (`html`) style is what the body inherits from.
    pub(crate) fn root_style(&self, lay: &Layouter) -> Style {
        match self.dom.nodes[self.dom.body].parent.filter(|&p| p != 0) {
            Some(h) => lay.style_of(h, &Style::default()),
            None => Style::default(),
        }
    }

    /// Run `f` with the layout of the page as it is now (made again when the page changed).
    pub(crate) fn with_layout<R>(&mut self, f: impl FnOnce(&Layouter, &mut LayoutCache) -> R) -> Option<R> {
        let (w, h) = (self.width as f32, self.height as f32);
        let mut cache = self.cache.take();
        let imgs = self.imgs.clone();
        let out = with_fonts(|reg, bold| {
            let lay = Layouter { dom: &self.dom, reg, bold, vw: w, vh: h, imgs: &imgs };
            let mut c = match cache.take() {
                Some(c) if c.generation == self.dom.generation => c,
                _ => {
                    let root = self.root_style(&lay);
                    let b = lay.build(self.dom.body, &root, 0.0, 0.0, w, h);
                    LayoutCache { generation: self.dom.generation, root, b }
                }
            };
            let r = f(&lay, &mut c);
            (r, c)
        });
        out.map(|(r, c)| {
            self.cache = Some(c);
            r
        })
    }

    /// The element at a point of the texture (pixels); the body when nothing is there.
    pub(crate) fn hit_node(&mut self, x: f32, y: f32) -> usize {
        let body = self.dom.body;
        self.with_layout(|_, c| hit(&c.b, x, y)).flatten().unwrap_or(body)
    }
}

fn find_box(b: &LBox, node: usize) -> Option<&LBox> {
    if b.node == node {
        return Some(b);
    }
    b.items.iter().find_map(|it| match it {
        Item::Block(c) => find_box(c, node),
        Item::Line(_) => None,
    })
}

fn css_color(c: [u8; 4]) -> String {
    if c[3] == 255 { format!("rgb({}, {}, {})", c[0], c[1], c[2]) } else { format!("rgba({}, {}, {}, {})", c[0], c[1], c[2], c[3] as f32 / 255.0) }
}

/// The computed value of a few properties of a laid-out box, as `getComputedStyle` gives it.
fn computed(b: Option<&LBox>, prop: &str) -> String {
    let Some(b) = b else { return if prop == "display" { "none".into() } else { String::new() } };
    let st = &b.st;
    let px = |v: f32| format!("{}px", (v * 100.0).round() / 100.0);
    match prop {
        "display" => (if st.none { "none" } else if st.flex { if st.inline_flex { "inline-flex" } else { "flex" } } else if st.inline_block { "inline-block" } else if st.inline { "inline" } else { "block" }).into(),
        "width" => px(b.rect[2] - st.padding[1] - st.padding[3] - st.bw[1] - st.bw[3]),
        "height" => px(b.rect[3] - st.padding[0] - st.padding[2] - st.bw[0] - st.bw[2]),
        "color" => css_color(st.color),
        "background-color" => css_color(st.bg),
        "opacity" => format!("{}", st.opacity),
        "visibility" => (if st.hidden { "hidden" } else { "visible" }).into(),
        "position" => ["static", "relative", "absolute", "fixed"][st.position as usize % 4].into(),
        "font-size" => px(st.font_px),
        "font-weight" => (if st.bold { "700" } else { "400" }).into(),
        "z-index" => if st.z == 0 { "auto".into() } else { st.z.to_string() },
        "text-align" => ["left", "center", "right"][st.align as usize % 3].into(),
        "margin-top" => px(st.margin[0]),
        "margin-right" => px(st.margin[1]),
        "margin-bottom" => px(st.margin[2]),
        "margin-left" => px(st.margin[3]),
        "padding-top" => px(st.padding[0]),
        "padding-right" => px(st.padding[1]),
        "padding-bottom" => px(st.padding[2]),
        "padding-left" => px(st.padding[3]),
        "border-top-width" => px(st.bw[0]),
        "border-right-width" => px(st.bw[1]),
        "border-bottom-width" => px(st.bw[2]),
        "border-left-width" => px(st.bw[3]),
        _ => String::new(),
    }
}

pub(crate) fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json_num(n: f64) -> String {
    if n.is_finite() { format!("{n}") } else { "null".into() }
}

pub(crate) fn api_to_json(a: &omsi_sim::vehicle_api::ApiValue) -> String {
    use omsi_sim::vehicle_api::ApiValue;
    match a {
        ApiValue::Null => "null".into(),
        ApiValue::Bool(b) => b.to_string(),
        ApiValue::Num(n) => json_num(*n),
        ApiValue::Str(s) => json_str(s),
        ApiValue::List(l) => format!("[{}]", l.iter().map(api_to_json).collect::<Vec<_>>().join(",")),
        ApiValue::Map(m) => format!("{{{}}}", m.iter().map(|(k, v)| format!("{}:{}", json_str(k), api_to_json(v))).collect::<Vec<_>>().join(",")),
    }
}

fn ints_json(v: impl IntoIterator<Item = usize>) -> String {
    format!("[{}]", v.into_iter().map(|i| i.to_string()).collect::<Vec<_>>().join(","))
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn btoa(s: &str) -> String {
    let bytes: Vec<u8> = s.chars().map(|c| c as u32 as u8).collect();
    let mut out = String::new();
    for ch in bytes.chunks(3) {
        let n = (ch[0] as u32) << 16 | (*ch.get(1).unwrap_or(&0) as u32) << 8 | *ch.get(2).unwrap_or(&0) as u32;
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if ch.len() > 1 { B64[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if ch.len() > 2 { B64[n as usize & 63] as char } else { '=' });
    }
    out
}

fn atob(s: &str) -> String {
    let mut bits = 0u32;
    let mut n = 0;
    let mut out = String::new();
    for c in s.bytes().filter(|c| *c != b'=') {
        let Some(v) = B64.iter().position(|&b| b == c) else { continue };
        bits = bits << 6 | v as u32;
        n += 6;
        if n >= 8 {
            n -= 8;
            out.push(((bits >> n) & 0xff) as u8 as char);
        }
    }
    out
}

/// A text file below one of the page's own folders; never one outside them.
pub(crate) fn read_page_file(dirs: &[std::path::PathBuf], rel: &str) -> Option<String> {
    let rel = rel.split(['?', '#']).next().unwrap_or("").trim_start_matches("./");
    if rel.is_empty() || rel.starts_with('/') || rel.starts_with('\\') || rel.split(['/', '\\']).any(|p| p == "..") || rel.contains(':') {
        return None;
    }
    let bytes = dirs.iter().find_map(|d| omsi_cfg::vfs::read(&omsi_cfg::resolve_path(d, rel)).ok()).filter(|b| b.len() <= MAX_FETCH)?;
    Some(String::from_utf8_lossy(&bytes).trim_start_matches('\u{feff}').to_string())
}

/// `./a/../b.js` against the folder of the module that imports it, as a path below the page's
/// folders (`None` when it climbs out of them).
fn resolve_module(from_dir: &str, spec: &str) -> Option<String> {
    let mut parts: Vec<&str> = if spec.starts_with('/') { Vec::new() } else { from_dir.split('/').filter(|p| !p.is_empty()).collect() };
    for p in spec.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p => parts.push(p),
        }
    }
    Some(parts.join("/"))
}

/// Loads the modules a page imports from its own folders.
struct PageLoader {
    st: Rc<RefCell<PageState>>,
    cache: RefCell<HashMap<String, Module>>,
}

impl ModuleLoader for PageLoader {
    async fn load_imported_module(self: Rc<Self>, referrer: Referrer, request: boa_engine::module::ModuleRequest, context: &RefCell<&mut Context>) -> JsResult<Module> {
        let spec = request.specifier().to_std_string_escaped();
        let from_dir = referrer.path().and_then(|p| p.parent()).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default();
        let path = resolve_module(&from_dir, &spec).ok_or_else(|| JsNativeError::typ().with_message(format!("cannot import {spec}: outside the page")))?;
        if let Some(m) = self.cache.borrow().get(&path) {
            return Ok(m.clone());
        }
        let text = read_page_file(&self.st.borrow().dirs, &path).ok_or_else(|| JsNativeError::typ().with_message(format!("cannot find module {spec}")))?;
        let src = Source::from_bytes(text.as_bytes()).with_path(std::path::Path::new(&path));
        let module = Module::parse(src, None, &mut context.borrow_mut())?;
        self.cache.borrow_mut().insert(path, module.clone());
        Ok(module)
    }
}

fn arg_s(args: &[JsValue], i: usize, ctx: &mut Context) -> String {
    args.get(i).and_then(|v| v.to_string(ctx).ok()).map(|s| s.to_std_string_escaped()).unwrap_or_default()
}

fn arg_n(args: &[JsValue], i: usize, ctx: &mut Context) -> f64 {
    args.get(i).and_then(|v| v.to_number(ctx).ok()).unwrap_or(f64::NAN)
}

fn arg_i(args: &[JsValue], i: usize, ctx: &mut Context) -> Option<usize> {
    let n = arg_n(args, i, ctx);
    (n.is_finite() && n >= 0.0).then_some(n as usize)
}

fn s(v: String) -> JsValue {
    JsValue::from(JsString::from(v.as_str()))
}

/// `__host(op, ...)`: what the prelude asks of the page and the game.
fn host(st: &Rc<RefCell<PageState>>, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let op = arg_s(args, 0, ctx);
    let a = &args[1.min(args.len())..];
    let node = |ctx: &mut Context, i: usize, st: &PageState| arg_i(a, i, ctx).filter(|&n| n < st.dom.nodes.len());
    let mut st = st.borrow_mut();
    let idx = |ctx: &mut Context, st: &PageState| node(ctx, 0, st);
    Ok(match op.as_str() {
        "now" => JsValue::from(st.now),
        "width" => JsValue::from(st.width as f64),
        "height" => JsValue::from(st.height as f64),
        "vehiclePage" => JsValue::from(st.api == omsi_sim::htmltex::PageApi::Vehicle),
        "body" => JsValue::from(st.dom.body as f64),
        "root" => JsValue::from(st.dom.nodes.iter().position(|n| n.tag == "html").unwrap_or(st.dom.body) as f64),
        "byId" => JsValue::from(st.dom.by_id(&arg_s(a, 0, ctx)).map_or(-1.0, |i| i as f64)),
        "query" | "queryAll" | "matches" => {
            if op == "matches" {
                let Some(i) = idx(ctx, &st) else { return Ok(JsValue::from(false)) };
                let chains = parse_selector_list(&arg_s(a, 1, ctx));
                return Ok(JsValue::from(chains.iter().any(|c| st.dom.matches(i, c))));
            }
            let sel = arg_s(a, 0, ctx);
            let root = arg_n(a, 1, ctx);
            let chains = parse_selector_list(&sel);
            let scope: Vec<usize> = if root >= 0.0 && (root as usize) < st.dom.nodes.len() { st.dom.descendants(root as usize) } else { (1..st.dom.nodes.len()).filter(|&i| st.dom.nodes[i].text.is_none() && st.dom.is_attached(i)).collect() };
            let mut hits = scope.into_iter().filter(|&i| chains.iter().any(|c| st.dom.matches(i, c)));
            if op == "query" { JsValue::from(hits.next().map_or(-1.0, |i| i as f64)) } else { s(ints_json(hits)) }
        }
        "create" => JsValue::from(st.dom.create(&arg_s(a, 0, ctx).to_ascii_lowercase()) as f64),
        "createText" => {
            let t = arg_s(a, 0, ctx);
            JsValue::from(st.dom.create_text(&t) as f64)
        }
        "append" | "insertBefore" => {
            if let (Some(p), Some(c)) = (node(ctx, 0, &st), node(ctx, 1, &st)) {
                let before = if op == "insertBefore" { node(ctx, 2, &st) } else { None };
                st.dom.insert_before(p, c, before);
            }
            JsValue::undefined()
        }
        "remove" => {
            if let Some(i) = idx(ctx, &st) {
                st.dom.detach(i);
            }
            JsValue::undefined()
        }
        "clone" => {
            let deep = args.get(2).is_some_and(|v| v.to_boolean());
            match idx(ctx, &st) {
                Some(i) => JsValue::from(st.dom.clone_node(i, deep) as f64),
                None => JsValue::from(-1.0),
            }
        }
        "kids" => s(idx(ctx, &st).map_or("[]".into(), |i| ints_json(st.dom.nodes[i].kids.iter().copied()))),
        "parent" => JsValue::from(idx(ctx, &st).and_then(|i| st.dom.nodes[i].parent).filter(|&p| p != 0).map_or(-1.0, |p| p as f64)),
        "isText" => JsValue::from(idx(ctx, &st).is_some_and(|i| st.dom.nodes[i].text.is_some())),
        "tag" => s(idx(ctx, &st).map(|i| st.dom.nodes[i].tag.clone()).unwrap_or_default()),
        "text" => s(idx(ctx, &st).map(|i| st.dom.text_of(i)).unwrap_or_default()),
        "setText" => {
            if let Some(i) = idx(ctx, &st) {
                let t = arg_s(a, 1, ctx);
                if st.dom.nodes[i].text.is_some() {
                    if st.dom.nodes[i].text.as_deref() != Some(t.as_str()) {
                        st.dom.nodes[i].text = Some(t);
                        st.dom.generation += 1;
                    }
                } else {
                    st.dom.set_text(i, t);
                }
            }
            JsValue::undefined()
        }
        "html" => s(idx(ctx, &st).map(|i| st.dom.inner_html(i)).unwrap_or_default()),
        "outer" => s(idx(ctx, &st).map(|i| st.dom.outer_html(i)).unwrap_or_default()),
        "setHtml" => {
            if let Some(i) = idx(ctx, &st) {
                let markup = arg_s(a, 1, ctx);
                if markup.contains('<') {
                    st.dom.graft(i, &markup);
                } else {
                    st.dom.set_text(i, markup);
                }
            }
            JsValue::undefined()
        }
        "attr" => match idx(ctx, &st).and_then(|i| st.dom.attr(i, &arg_s(a, 1, ctx))) {
            Some(v) => s(v),
            None => JsValue::null(),
        },
        "setAttr" => {
            if let Some(i) = idx(ctx, &st) {
                let (n, v) = (arg_s(a, 1, ctx), arg_s(a, 2, ctx));
                st.dom.set_attr(i, &n, &v);
            }
            JsValue::undefined()
        }
        "removeAttr" => {
            if let Some(i) = idx(ctx, &st) {
                let n = arg_s(a, 1, ctx);
                st.dom.remove_attr(i, &n);
            }
            JsValue::undefined()
        }
        "attrNames" => s(format!("[{}]", idx(ctx, &st).map(|i| st.dom.attr_names(i)).unwrap_or_default().iter().map(|n| json_str(n)).collect::<Vec<_>>().join(","))),
        "classes" => s(idx(ctx, &st).map(|i| st.dom.nodes[i].classes.join(" ")).unwrap_or_default()),
        "setClasses" => {
            if let Some(i) = idx(ctx, &st) {
                let cls = arg_s(a, 1, ctx);
                st.dom.set_attr(i, "class", &cls);
            }
            JsValue::undefined()
        }
        "styleGet" => {
            let prop = arg_s(a, 1, ctx);
            s(idx(ctx, &st).and_then(|i| st.dom.nodes[i].inline.iter().rev().find(|(k, _)| *k == prop).map(|(_, v)| v.clone())).unwrap_or_default())
        }
        "styleSet" => {
            if let Some(i) = idx(ctx, &st) {
                let (prop, val) = (arg_s(a, 1, ctx), arg_s(a, 2, ctx));
                let inline = &mut st.dom.nodes[i].inline;
                if val.is_empty() {
                    if inline.iter().any(|(k, _)| *k == prop) {
                        inline.retain(|(k, _)| *k != prop);
                        st.dom.generation += 1;
                    }
                } else if !inline.last().is_some_and(|(k, x)| *k == prop && *x == val) {
                    inline.retain(|(k, _)| *k != prop);
                    inline.push((prop, val));
                    st.dom.generation += 1;
                }
            }
            JsValue::undefined()
        }
        "styleText" => s(idx(ctx, &st).map(|i| style_text(&st.dom.nodes[i].inline)).unwrap_or_default()),
        "computed" => {
            let (i, prop) = (idx(ctx, &st), arg_s(a, 1, ctx));
            match i {
                Some(i) => s(st.with_layout(|_, c| computed(find_box(&c.b, i), &prop)).unwrap_or_default()),
                None => s(String::new()),
            }
        }
        "rect" => {
            let r = match idx(ctx, &st) {
                Some(i) => st.with_layout(|_, c| find_box(&c.b, i).map(|b| b.rect)).flatten().unwrap_or([0.0; 4]),
                None => [0.0; 4],
            };
            s(format!("[{},{},{},{}]", json_num(r[0] as f64), json_num(r[1] as f64), json_num(r[2] as f64), json_num(r[3] as f64)))
        }
        "hit" => {
            let (x, y) = (arg_n(a, 0, ctx) as f32, arg_n(a, 1, ctx) as f32);
            JsValue::from(st.hit_node(x, y) as f64)
        }
        "setVar" => {
            let name = arg_s(a, 0, ctx);
            let v = arg_n(a, 1, ctx) as f32;
            if v.is_finite() {
                log::debug!("htmltexture: page calls setVar({name}, {v})");
                st.events.push((name, v));
            } else {
                log::debug!("htmltexture: setVar({name}) ignored, the value is not a finite number");
            }
            JsValue::undefined()
        }
        "trigger" => {
            let name = arg_s(a, 0, ctx);
            if !name.is_empty() {
                log::debug!("htmltexture: page presses trigger {name}");
                st.triggers.push(name);
            }
            JsValue::undefined()
        }
        "request" => {
            use omsi_sim::htmltex::HtmlRequest;
            let kind = arg_s(a, 0, ctx);
            let n = arg_n(a, 1, ctx);
            let index = (n.is_finite() && n >= 0.0).then_some(n as usize);
            match kind.as_str() {
                "route" => index.map(HtmlRequest::SetRoute),
                "destination" => index.map(HtmlRequest::SetDestination),
                "nextstop" => index.map(HtmlRequest::SetNextStop),
                "clearline" => Some(HtmlRequest::ClearLine),
                "line" => {
                    let line = arg_s(a, 1, ctx);
                    (!line.trim().is_empty()).then_some(HtmlRequest::SetLine(line))
                }
                _ => None,
            }
            .map(|r| st.requests.push(r))
            .unwrap_or_else(|| log::debug!("htmltexture: request {kind} ignored, the index is not a number"));
            JsValue::undefined()
        }
        "wantDepartures" => {
            let key = arg_s(a, 0, ctx);
            if !key.is_empty() && !st.departure_wants.contains(&key) && st.departure_wants.len() < MAX_DEPARTURE_WANTS {
                st.departure_wants.push(key);
            }
            JsValue::undefined()
        }
        "log" => {
            log::info!("htmltexture console: {}", arg_s(a, 0, ctx));
            JsValue::undefined()
        }
        "read" => match read_page_file(&st.dirs, &arg_s(a, 0, ctx)) {
            Some(t) => s(t),
            None => JsValue::null(),
        },
        "atob" => s(atob(&arg_s(a, 0, ctx))),
        "btoa" => s(btoa(&arg_s(a, 0, ctx))),
        _ => JsValue::undefined(),
    })
}

/// A page's scripts and what they work on.
pub(crate) struct PageCore {
    pub(crate) st: Rc<RefCell<PageState>>,
    pub(crate) ctx: Context,
    pub(crate) dirty: bool,
    pub(crate) warned: bool,
    pub(crate) start: std::time::Instant,
    /// A press is on the page and its release has not come yet (a click needs both).
    pub(crate) pressed: bool,
    /// `dom.generation` of the last frame handed out.
    pub(crate) rendered_gen: u64,
    /// The page's scripts have run (they wait for the page's folders, which the game tells
    /// right after it makes the page).
    pub(crate) started: bool,
}

impl PageCore {
    pub(crate) fn new(width: u32, height: u32, html: &str, api: omsi_sim::htmltex::PageApi) -> PageCore {
        let dom = Dom::parse(html);
        log::debug!("htmltexture: page parsed: {} nodes, {} css rules, {} script(s), {}x{}", dom.nodes.len(), dom.rules.len(), dom.scripts.len(), width, height);
        let st = Rc::new(RefCell::new(PageState {
            dom,
            width: width.max(1),
            height: height.max(1),
            imgs: Arc::new(ImageStore::new(Vec::new())),
            dirs: Vec::new(),
            cache: None,
            api,
            now: 0.0,
            events: Vec::new(),
            triggers: Vec::new(),
            requests: Vec::new(),
            departure_wants: Vec::new(),
        }));
        let loader = Rc::new(PageLoader { st: st.clone(), cache: RefCell::new(HashMap::new()) });
        let mut ctx = Context::builder().module_loader(loader).build().unwrap_or_default();
        {
            let limits = ctx.runtime_limits_mut();
            limits.set_loop_iteration_limit(LOOP_LIMIT);
            limits.set_recursion_limit(RECURSION_LIMIT);
            limits.set_stack_size_limit(STACK_LIMIT);
        }
        let captured = st.clone();
        // SAFETY: the closure holds an `Rc` of plain data; no script value is stored in it
        let native = unsafe { NativeFunction::from_closure(move |_this, args, ctx| host(&captured, args, ctx)) };
        ctx.register_global_callable(js_string!("__host"), 1, native).ok();
        let mut core = PageCore { st, ctx, dirty: true, warned: false, start: std::time::Instant::now(), pressed: false, rendered_gen: 0, started: false };
        if let Err(e) = core.eval(PRELUDE) {
            log::warn!("htmltexture: the page runtime failed to start: {e}");
        }
        core
    }

    /// Run the page's scripts and fire its load events; once.
    pub(crate) fn start(&mut self) {
        if std::mem::replace(&mut self.started, true) {
            return;
        }
        let scripts = self.st.borrow().dom.scripts.clone();
        let meta = self.st.borrow().dom.script_meta.clone();
        for (n, src) in scripts.iter().enumerate() {
            let m = meta.get(n).cloned().unwrap_or(ScriptMeta { run: true, module: false, path: None });
            if !m.run {
                continue;
            }
            let result = if m.module { self.eval_module(src, m.path.as_deref()) } else { self.eval(src) };
            match result {
                Ok(()) => log::debug!("htmltexture: script {n} ran"),
                Err(e) => {
                    log::warn!("htmltexture: script {n} error: {e}");
                    self.warned = true;
                }
            }
        }
        if let Err(e) = self.eval("__fire('DOMContentLoaded'); __fire('load');") {
            log::warn!("htmltexture: load handlers failed: {e}");
        }
    }

    /// Run script text; promises that came due run after it.
    pub(crate) fn eval(&mut self, src: &str) -> Result<(), String> {
        let r = self.ctx.eval(Source::from_bytes(src));
        let jobs = self.ctx.run_jobs();
        r.map(|_| ()).map_err(|e| e.to_string())?;
        jobs.map_err(|e| e.to_string())
    }

    /// Run a module script (`type="module"`): its imports are read from the page's folders.
    pub(crate) fn eval_module(&mut self, src: &str, path: Option<&str>) -> Result<(), String> {
        let source = Source::from_bytes(src.as_bytes());
        let source = match path {
            Some(p) => source.with_path(std::path::Path::new(p)),
            None => source,
        };
        let module = Module::parse(source, None, &mut self.ctx).map_err(|e: JsError| e.to_string())?;
        let promise = module.load_link_evaluate(&mut self.ctx);
        let _ = self.ctx.run_jobs();
        match promise.state() {
            PromiseState::Rejected(e) => Err(e.display().to_string()),
            _ => Ok(()),
        }
    }

    /// Call a global function of the prelude with text arguments.
    pub(crate) fn call(&mut self, name: &str, args: &[JsValue]) -> Result<JsValue, String> {
        let f = self.ctx.global_object().get(JsString::from(name), &mut self.ctx).map_err(|e| e.to_string())?;
        let Some(func) = f.as_callable() else { return Err(format!("{name} is not a function")) };
        let r = func.call(&JsValue::undefined(), args, &mut self.ctx);
        let jobs = self.ctx.run_jobs();
        let v = r.map_err(|e| e.to_string())?;
        jobs.map_err(|e| e.to_string())?;
        Ok(v)
    }

    pub(crate) fn clock(&mut self) {
        self.st.borrow_mut().now = self.start.elapsed().as_secs_f64();
    }

    /// Run the timers that are due. True when one ran (the page probably changed).
    pub(crate) fn run_timers(&mut self) -> bool {
        let now = self.st.borrow().now;
        match self.call("__tick", &[JsValue::from(now)]) {
            Ok(v) => v.as_number().is_some_and(|n| n > 0.0),
            Err(e) => {
                log::warn!("htmltexture: timers failed: {e}");
                false
            }
        }
    }

    /// Fire `ty` on `node`; the script side lets it bubble up through its parents.
    pub(crate) fn dispatch(&mut self, node: usize, ty: &str, x: f32, y: f32) {
        let args = [JsValue::from(node as f64), JsValue::from(JsString::from(ty)), JsValue::from(x as f64), JsValue::from(y as f64)];
        if let Err(e) = self.call("__dispatch", &args) {
            log::warn!("htmltexture: {ty} handler failed: {e}");
        }
    }

    /// Keep the latest value of every variable the host sends in `omsi.vars.num` and
    /// `omsi.vars.str`, and call the page's `omsi.update`.
    pub(crate) fn set_vars(&mut self, num: &[(String, f32)], strs: &[(String, String)]) {
        let n = format!("{{{}}}", num.iter().map(|(k, v)| format!("{}:{}", json_str(k), json_num(*v as f64))).collect::<Vec<_>>().join(","));
        let t = format!("{{{}}}", strs.iter().map(|(k, v)| format!("{}:{}", json_str(k), json_str(v))).collect::<Vec<_>>().join(","));
        let args = [JsValue::from(JsString::from(n.as_str())), JsValue::from(JsString::from(t.as_str()))];
        if let Err(e) = self.call("__store_vars", &args) {
            log::warn!("htmltexture: omsi.vars failed: {e}");
        }
        log::debug!("htmltexture: window.omsi.update with {} numeric and {} string variable(s)", num.len(), strs.len());
        if let Err(e) = self.call("__call_update", &args) {
            if !self.warned {
                log::warn!("htmltexture: omsi.update failed: {e}");
                self.warned = true;
            }
        }
    }

    pub(crate) fn omsi_set(&mut self, name: &str, value: &omsi_sim::vehicle_api::ApiValue) {
        let json = api_to_json(value);
        let args = [JsValue::from(JsString::from(name)), JsValue::from(JsString::from(json.as_str()))];
        if let Err(e) = self.call("__omsi_set", &args) {
            log::warn!("htmltexture: omsi.{name} failed: {e}");
        }
    }

    pub(crate) fn omsi_merge(&mut self, value: &omsi_sim::vehicle_api::ApiValue) {
        let json = api_to_json(value);
        if let Err(e) = self.call("__omsi_merge", &[JsValue::from(JsString::from(json.as_str()))]) {
            log::warn!("htmltexture: omsi environment failed: {e}");
        }
    }

    pub(crate) fn render(&mut self) -> Vec<u8> {
        let started = std::time::Instant::now();
        let mut st = self.st.borrow_mut();
        let (w, h) = (st.width, st.height);
        let imgs = st.imgs.clone();
        let mut cv = Canvas::new(w, h);
        st.with_layout(|lay, c| {
            // the background of html and body covers the whole texture
            let full = [0.0, 0.0, w as f32, h as f32];
            if c.root.bg[3] > 0 {
                cv.fill(full, c.root.bg, 0.0);
            }
            paint_bg(&mut cv, &imgs, full, 0.0, &c.root);
            if c.b.st.bg[3] > 0 {
                cv.fill(full, c.b.st.bg, 0.0);
            }
            paint_bg(&mut cv, &imgs, full, 0.0, &c.b.st);
            let (bg, bg_img, grad) = (c.b.st.bg, c.b.st.bg_img.take(), c.b.st.gradient.take());
            if let Some(g) = &grad {
                cv.gradient(full, 0.0, g);
            }
            c.b.st.bg = [0, 0, 0, 0];
            paint(&mut cv, lay, &c.b);
            c.b.st.bg = bg;
            c.b.st.bg_img = bg_img;
            c.b.st.gradient = grad;
        });
        log::debug!("htmltexture: rendered {}x{} in {:?}", w, h, started.elapsed());
        cv.px
    }
}
