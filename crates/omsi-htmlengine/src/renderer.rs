//! [`EngineRenderer`]: glues DOM, style, layout, paint and script together behind the
//! [`HtmlRenderer`] trait.

use super::*;

/// The engine as an [`HtmlRenderer`].
pub struct EngineRenderer {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) js: Interp,
    pub(crate) dirty: bool,
    pub(crate) warned: bool,
    pub(crate) start: std::time::Instant,
    /// A press is on the page and its release has not come yet (a click needs both).
    pub(crate) pressed: bool,
    /// The pictures of the page (`<img>`, `background-image`), loaded when first drawn.
    pub(crate) imgs: Arc<ImageStore>,
    /// The laid-out page of the last `dom.generation`, shared by `render` and `hit_node`.
    pub(crate) cache: Mutex<Option<LayoutCache>>,
    /// `dom.generation` of the last frame handed out.
    pub(crate) rendered_gen: u64,
}

/// The laid-out body and the root style it was built with.
pub(crate) struct LayoutCache {
    pub(crate) generation: u64,
    pub(crate) root: Style,
    pub(crate) b: LBox,
}

impl EngineRenderer {
    pub fn new(width: u32, height: u32, html: &str) -> EngineRenderer {
        EngineRenderer::with_api(width, height, html, omsi_sim::htmltex::PageApi::Vehicle)
    }

    /// A page whose `window.omsi` is the one of `api` (see [`omsi_sim::htmltex::PageApi`]).
    pub fn with_api(
        width: u32,
        height: u32,
        html: &str,
        api: omsi_sim::htmltex::PageApi,
    ) -> EngineRenderer {
        let dom = Dom::parse(html);
        let scripts = dom.scripts.clone();
        log::debug!(
            "htmltexture: page parsed: {} nodes, {} css rules, {} script(s), {}x{}",
            dom.nodes.len(),
            dom.rules.len(),
            scripts.len(),
            width,
            height
        );
        let mut js = Interp::with_api(dom, api);
        let mut warned = false;
        for (n, s) in scripts.iter().enumerate() {
            match js.run(s) {
                Ok(()) => log::debug!("htmltexture: script {n} ran ({} steps)", js.steps),
                Err(e) => {
                    log::warn!("htmltexture: script {n} error: {e}");
                    warned = true;
                }
            }
        }
        EngineRenderer {
            width: width.max(1),
            height: height.max(1),
            js,
            dirty: true,
            warned,
            start: std::time::Instant::now(),
            pressed: false,
            imgs: Arc::new(ImageStore::new(Vec::new())),
            cache: Mutex::new(None),
            rendered_gen: 0,
        }
    }

    /// The cached layout when it is still current, else a fresh one.
    fn layout_cache(&self, lay: &Layouter) -> LayoutCache {
        if let Some(c) = self.cache.lock().unwrap().take() {
            if c.generation == self.js.dom.generation {
                return c;
            }
        }
        let root = self.root_style(lay);
        let b = lay.build(
            self.js.dom.body,
            &root,
            0.0,
            0.0,
            self.width as f32,
            self.height as f32,
        );
        LayoutCache {
            generation: self.js.dom.generation,
            root,
            b,
        }
    }

    pub(crate) fn clock(&mut self) {
        self.js.now = self.start.elapsed().as_secs_f64();
    }

    /// Run the timers that are due. True when one ran (the page probably changed).
    pub(crate) fn run_timers(&mut self) -> bool {
        let now = self.js.now;
        let due: Vec<(u32, Val)> = self
            .js
            .timers
            .iter()
            .filter(|t| t.due <= now)
            .map(|t| (t.id, t.f.clone()))
            .collect();
        if due.is_empty() {
            return false;
        }
        // an interval is set up again before it runs, so it can stop itself
        self.js.timers.retain_mut(|t| {
            if t.due > now {
                return true;
            }
            match t.every {
                Some(e) => {
                    t.due = now + e;
                    true
                }
                None => false,
            }
        });
        for (id, f) in due {
            self.js.steps = 0;
            if let Err(e) = self.js.call(f, Val::Undef, Vec::new()) {
                log::warn!("htmltexture: timer {id} failed: {e}");
            }
        }
        true
    }

    /// The root (`html`) style is what the body inherits from.
    pub(crate) fn root_style(&self, lay: &Layouter) -> Style {
        let dom = &self.js.dom;
        match dom.nodes[dom.body].parent.filter(|&p| p != 0) {
            Some(h) => lay.style_of(h, &Style::default()),
            None => Style::default(),
        }
    }

    /// The element at a point of the texture (pixels); the body when nothing is there.
    pub(crate) fn hit_node(&self, x: f32, y: f32) -> usize {
        let body = self.js.dom.body;
        with_fonts(|reg, bold| {
            let lay = Layouter {
                dom: &self.js.dom,
                reg,
                bold,
                vw: self.width as f32,
                vh: self.height as f32,
                imgs: &*self.imgs,
            };
            let c = self.layout_cache(&lay);
            let r = hit(&c.b, x, y);
            *self.cache.lock().unwrap() = Some(c);
            r
        })
        .flatten()
        .unwrap_or(body)
    }

    /// Fire `ty` on `node` and let it bubble up through its parents.
    pub(crate) fn dispatch(&mut self, node: usize, ty: &str, x: f32, y: f32) {
        let ev = obj_of(&[
            ("type", Val::Str(ty.to_string())),
            ("x", Val::Num(x as f64)),
            ("y", Val::Num(y as f64)),
            ("clientX", Val::Num(x as f64)),
            ("clientY", Val::Num(y as f64)),
            ("target", Val::Elem(node)),
        ]);
        self.js
            .global
            .lock()
            .unwrap()
            .vars
            .insert("event".into(), Val::Obj(ev.clone()));
        let mut cur = Some(node);
        while let Some(n) = cur {
            let src = self.js.dom.nodes[n]
                .on
                .iter()
                .find(|(k, _)| k == ty)
                .map(|(_, s)| s.clone());
            if let Some(src) = src {
                if let Err(e) = self.js.run(&src) {
                    log::warn!("htmltexture: on{ty} handler failed: {e}");
                }
            }
            let listeners = self
                .js
                .handlers
                .get(&(n, ty.to_string()))
                .cloned()
                .unwrap_or_default();
            for f in listeners {
                self.js.steps = 0;
                if let Err(e) = self.js.call(f, Val::Undef, vec![Val::Obj(ev.clone())]) {
                    log::warn!("htmltexture: {ty} listener failed: {e}");
                }
            }
            if ev.lock().unwrap().contains_key("cancelBubble") {
                break;
            }
            cur = self.js.dom.nodes[n].parent.filter(|&p| p != 0);
        }
    }

    /// The `window.omsi` object.
    pub(crate) fn omsi(&self) -> Option<ObjRef> {
        match self.js.window.lock().unwrap().get("omsi") {
            Some(Val::Obj(o)) => Some(o.clone()),
            _ => None,
        }
    }

    /// Keep the latest value of every variable the host sends in `omsi.vars.num` and
    /// `omsi.vars.str`, under its lower-case name (and as sent, when that differs), so a page
    /// can read any of them at any time (`omsi.vars.num.engine_n`, `omsi.getVar("Door_1")`).
    pub(crate) fn store_vars(&mut self, num: &[(String, f32)], strs: &[(String, String)]) {
        let Some(omsi) = self.omsi() else { return };
        let vars = match omsi.lock().unwrap().get("vars") {
            Some(Val::Obj(o)) => o.clone(),
            _ => return,
        };
        let sub = |kind: &str| match vars.lock().unwrap().get(kind) {
            Some(Val::Obj(o)) => Some(o.clone()),
            _ => None,
        };
        if let Some(n) = sub("num") {
            let mut g = n.lock().unwrap();
            for (k, v) in num {
                let val = Val::Num(*v as f64);
                let lower = k.to_ascii_lowercase();
                if lower != *k {
                    g.insert(k.clone(), val.clone());
                }
                g.insert(lower, val);
            }
        }
        if let Some(s) = sub("str") {
            let mut g = s.lock().unwrap();
            for (k, v) in strs {
                let val = Val::Str(v.clone());
                let lower = k.to_ascii_lowercase();
                if lower != *k {
                    g.insert(k.clone(), val.clone());
                }
                g.insert(lower, val);
            }
        }
    }

    pub(crate) fn call_update(&mut self, num: &[(String, f32)], strs: &[(String, String)]) {
        let omsi = self.js.window.lock().unwrap().get("omsi").cloned();
        let Some(Val::Obj(omsi)) = omsi else {
            log::debug!("htmltexture: the page has no window.omsi");
            return;
        };
        let update = omsi.lock().unwrap().get("update").cloned();
        let Some(update @ Val::Func(_)) = update else {
            log::debug!("htmltexture: the page has no window.omsi.update function");
            return;
        };
        log::debug!(
            "htmltexture: window.omsi.update with {} numeric and {} string variable(s)",
            num.len(),
            strs.len()
        );
        let (veh, vars) = {
            let g = omsi.lock().unwrap();
            (
                g.get("vehicle").cloned().unwrap_or(Val::Undef),
                g.get("vars").cloned().unwrap_or(Val::Undef),
            )
        };
        let nums: HashMap<String, Val> = num
            .iter()
            .map(|(k, v)| (k.clone(), Val::Num(*v as f64)))
            .collect();
        let strv: HashMap<String, Val> = strs
            .iter()
            .map(|(k, v)| (k.clone(), Val::Str(v.clone())))
            .collect();
        let mut fields = vec![
            ("num", Val::Obj(Arc::new(Mutex::new(nums)))),
            ("str", Val::Obj(Arc::new(Mutex::new(strv)))),
            ("vars", vars),
        ];
        // (a scenery object's page has no vehicle)
        if !matches!(veh, Val::Undef) {
            fields.push(("vehicle", veh));
        }
        let arg = obj_of(&fields);
        self.js.steps = 0;
        let result = self.js.call(update, Val::Obj(omsi), vec![Val::Obj(arg)]);
        log::debug!(
            "htmltexture: window.omsi.update took {} steps",
            self.js.steps
        );
        if let Err(e) = result {
            if !self.warned {
                log::warn!("htmltexture: omsi.update failed: {e}");
                self.warned = true;
            }
        }
    }

    pub(crate) fn render(&self) -> Vec<u8> {
        let started = std::time::Instant::now();
        let mut cv = Canvas {
            w: self.width,
            h: self.height,
            px: vec![0; (self.width * self.height * 4) as usize],
        };
        with_fonts(|reg, bold| {
            let lay = Layouter {
                dom: &self.js.dom,
                reg,
                bold,
                vw: self.width as f32,
                vh: self.height as f32,
                imgs: &*self.imgs,
            };
            let mut c = self.layout_cache(&lay);
            // the background of html and body covers the whole texture
            let full = [0.0, 0.0, self.width as f32, self.height as f32];
            if c.root.bg[3] > 0 {
                cv.fill(full, c.root.bg, 0.0);
            }
            paint_bg(&mut cv, &self.imgs, full, 0.0, &c.root);
            if c.b.st.bg[3] > 0 {
                cv.fill(full, c.b.st.bg, 0.0);
            }
            paint_bg(&mut cv, &self.imgs, full, 0.0, &c.b.st);
            let (bg, bg_img) = (c.b.st.bg, c.b.st.bg_img.take());
            c.b.st.bg = [0, 0, 0, 0];
            paint(&mut cv, &lay, &c.b);
            c.b.st.bg = bg;
            c.b.st.bg_img = bg_img;
            *self.cache.lock().unwrap() = Some(c);
        });
        log::debug!(
            "htmltexture: rendered {}x{} in {:?}",
            self.width,
            self.height,
            started.elapsed()
        );
        cv.px
    }

    #[cfg(test)]
    pub(crate) fn text_of(&self, id: &str) -> Option<String> {
        self.js.dom.by_id(id).map(|i| self.js.dom.text_of(i))
    }
}

impl HtmlRenderer for EngineRenderer {
    fn set_vars(&mut self, num: &[(String, f32)], strs: &[(String, String)]) {
        self.clock();
        self.store_vars(num, strs);
        self.call_update(num, strs);
    }

    fn set_vehicle(&mut self, api: &omsi_sim::vehicle_api::ApiValue) {
        if let Some(omsi) = self.omsi() {
            omsi.lock()
                .unwrap()
                .insert("vehicle".to_string(), api_to_val(api));
        }
    }

    fn set_env(&mut self, env: &omsi_sim::vehicle_api::ApiValue) {
        if let (Some(omsi), omsi_sim::vehicle_api::ApiValue::Map(m)) = (self.omsi(), env) {
            let mut o = omsi.lock().unwrap();
            for (k, v) in m {
                o.insert(k.clone(), api_to_val(v));
            }
        }
    }

    fn pointer(&mut self, x: f32, y: f32, kind: PointerKind) {
        self.clock();
        let node = self.hit_node(x, y);
        match kind {
            PointerKind::Down => {
                self.pressed = true;
                self.dispatch(node, "pointerdown", x, y);
                self.dispatch(node, "mousedown", x, y);
            }
            PointerKind::Up => {
                self.dispatch(node, "pointerup", x, y);
                self.dispatch(node, "mouseup", x, y);
                if std::mem::take(&mut self.pressed) {
                    self.dispatch(node, "click", x, y);
                }
            }
            PointerKind::Move => self.dispatch(node, "mousemove", x, y),
        }
    }

    fn poll_frame(&mut self) -> Option<Vec<u8>> {
        self.clock();
        self.run_timers();
        if !self.dirty && self.js.dom.generation == self.rendered_gen {
            return None;
        }
        self.dirty = false;
        let frame = self.render();
        self.rendered_gen = self.js.dom.generation;
        Some(frame)
    }

    fn take_events(&mut self) -> Vec<(String, f32)> {
        std::mem::take(&mut self.js.events)
    }

    fn take_triggers(&mut self) -> Vec<String> {
        std::mem::take(&mut self.js.triggers)
    }

    fn set_asset_dirs(&mut self, dirs: Vec<std::path::PathBuf>) {
        self.imgs = Arc::new(ImageStore::new(dirs));
        *self.cache.lock().unwrap() = None;
        self.dirty = true;
    }

    fn set_depot(&mut self, depot: &omsi_sim::vehicle_api::ApiValue) {
        if let Some(omsi) = self.omsi() {
            omsi.lock()
                .unwrap()
                .insert("depot".to_string(), api_to_val(depot));
        }
    }

    fn take_requests(&mut self) -> Vec<omsi_sim::htmltex::HtmlRequest> {
        std::mem::take(&mut self.js.requests)
    }

    fn set_departures(&mut self, departures: &omsi_sim::vehicle_api::ApiValue) {
        if let Some(omsi) = self.omsi() {
            omsi.lock()
                .unwrap()
                .insert("departures".to_string(), api_to_val(departures));
        }
    }

    fn take_departure_wants(&mut self) -> Vec<String> {
        std::mem::take(&mut self.js.departure_wants)
    }
}
