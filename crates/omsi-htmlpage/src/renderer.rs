//! [`EngineRenderer`]: a page on a thread of its own behind the [`HtmlRenderer`] trait.
//!
//! The scripts run in Boa, whose context is not `Send`, so the page (`PageCore`) lives on a
//! worker thread and every call of the trait is a round trip to it: the caller sends what to do
//! and waits for the answer. The caller sees what it saw when the page ran on its own thread.

use super::*;
use crate::page::PageCore;
use std::sync::mpsc;

type Job = Box<dyn FnOnce(&mut PageCore) + Send>;

/// The engine as an [`HtmlRenderer`].
pub struct EngineRenderer {
    tx: Option<mpsc::Sender<Job>>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl EngineRenderer {
    pub fn new(width: u32, height: u32, html: &str) -> EngineRenderer {
        EngineRenderer::with_api(width, height, html, omsi_sim::htmltex::PageApi::Vehicle)
    }

    /// A page whose `window.omsi` is the one of `api` (see [`omsi_sim::htmltex::PageApi`]).
    pub fn with_api(width: u32, height: u32, html: &str, api: omsi_sim::htmltex::PageApi) -> EngineRenderer {
        let (tx, rx) = mpsc::channel::<Job>();
        let (ready_tx, ready_rx) = mpsc::channel::<()>();
        let html = html.to_string();
        let worker = std::thread::Builder::new()
            .name("html page".into())
            // (a script's recursion and the layout of a deep page need room)
            .stack_size(32 << 20)
            .spawn(move || {
                let mut core = PageCore::new(width, height, &html, api);
                let _ = ready_tx.send(());
                while let Ok(job) = rx.recv() {
                    // a script that brings Boa down takes the page with it, not the game
                    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| job(&mut core)));
                    if r.is_err() {
                        log::warn!("htmltexture: the page stopped after an internal error");
                        break;
                    }
                }
            })
            .ok();
        let up = worker.is_some() && ready_rx.recv().is_ok();
        EngineRenderer { tx: up.then_some(tx), worker }
    }

    /// Run `f` on the page's thread and wait for its answer; `None` when the page has stopped.
    /// The page's scripts have run by then.
    fn call<R: Send + 'static>(&self, f: impl FnOnce(&mut PageCore) -> R + Send + 'static) -> Option<R> {
        self.call_raw(move |c| {
            c.start();
            f(c)
        })
    }

    /// As `call`, before the scripts have run (telling the page its folders).
    fn call_raw<R: Send + 'static>(&self, f: impl FnOnce(&mut PageCore) -> R + Send + 'static) -> Option<R> {
        let tx = self.tx.as_ref()?;
        let (rtx, rrx) = mpsc::channel();
        tx.send(Box::new(move |core| {
            let _ = rtx.send(f(core));
        }))
        .ok()?;
        rrx.recv().ok()
    }

    /// The page's time is set directly (the tests move the clock).
    #[cfg(test)]
    pub(crate) fn set_now(&self, t: f64) {
        self.call(move |c| c.st.borrow_mut().now = t);
    }

    #[cfg(test)]
    pub(crate) fn advance_now(&self, dt: f64) {
        self.call(move |c| c.st.borrow_mut().now += dt);
    }

    #[cfg(test)]
    pub(crate) fn render(&self) -> Vec<u8> {
        self.call(|c| c.render()).unwrap_or_default()
    }

    #[cfg(test)]
    pub(crate) fn run_timers(&self) -> bool {
        self.call(|c| c.run_timers()).unwrap_or(false)
    }

    #[cfg(test)]
    pub(crate) fn run_js(&self, src: &str) -> Result<(), String> {
        let src = src.to_string();
        self.call(move |c| c.eval(&src)).unwrap_or_else(|| Err("page stopped".into()))
    }

    #[cfg(test)]
    pub(crate) fn hit_node(&self, x: f32, y: f32) -> usize {
        self.call(move |c| c.st.borrow_mut().hit_node(x, y)).unwrap_or(0)
    }

    #[cfg(test)]
    pub(crate) fn by_id(&self, id: &str) -> Option<usize> {
        let id = id.to_string();
        self.call(move |c| c.st.borrow().dom.by_id(&id)).flatten()
    }

    #[cfg(test)]
    pub(crate) fn parent_of(&self, n: usize) -> Option<usize> {
        self.call(move |c| c.st.borrow().dom.nodes[n].parent).flatten()
    }

    #[cfg(test)]
    pub(crate) fn text_of(&self, id: &str) -> Option<String> {
        let id = id.to_string();
        self.call(move |c| {
            let st = c.st.borrow();
            st.dom.by_id(&id).map(|i| st.dom.text_of(i))
        })
        .flatten()
    }
}

impl Drop for EngineRenderer {
    fn drop(&mut self) {
        self.tx = None;
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}

impl HtmlRenderer for EngineRenderer {
    fn set_vars(&mut self, num: &[(String, f32)], strs: &[(String, String)]) {
        let (num, strs) = (num.to_vec(), strs.to_vec());
        self.call(move |c| {
            c.clock();
            c.set_vars(&num, &strs);
        });
    }

    fn set_vehicle(&mut self, api: &omsi_sim::vehicle_api::ApiValue) {
        let api = api.clone();
        self.call(move |c| c.omsi_set("vehicle", &api));
    }

    fn set_env(&mut self, env: &omsi_sim::vehicle_api::ApiValue) {
        let env = env.clone();
        self.call(move |c| {
            if matches!(env, omsi_sim::vehicle_api::ApiValue::Map(_)) {
                c.omsi_merge(&env);
            }
        });
    }

    fn pointer(&mut self, x: f32, y: f32, kind: PointerKind) {
        self.call(move |c| {
            c.clock();
            let node = c.st.borrow_mut().hit_node(x, y);
            match kind {
                PointerKind::Down => {
                    c.pressed = true;
                    c.dispatch(node, "pointerdown", x, y);
                    c.dispatch(node, "mousedown", x, y);
                }
                PointerKind::Up => {
                    c.dispatch(node, "pointerup", x, y);
                    c.dispatch(node, "mouseup", x, y);
                    if std::mem::take(&mut c.pressed) {
                        c.dispatch(node, "click", x, y);
                    }
                }
                PointerKind::Move => c.dispatch(node, "mousemove", x, y),
            }
        });
    }

    fn poll_frame(&mut self) -> Option<Vec<u8>> {
        self.call(|c| {
            c.clock();
            c.run_timers();
            let generation = c.st.borrow().dom.generation;
            if !c.dirty && generation == c.rendered_gen {
                return None;
            }
            c.dirty = false;
            let frame = c.render();
            c.rendered_gen = c.st.borrow().dom.generation;
            Some(frame)
        })
        .flatten()
    }

    fn take_events(&mut self) -> Vec<(String, f32)> {
        self.call(|c| std::mem::take(&mut c.st.borrow_mut().events)).unwrap_or_default()
    }

    fn take_triggers(&mut self) -> Vec<String> {
        self.call(|c| std::mem::take(&mut c.st.borrow_mut().triggers)).unwrap_or_default()
    }

    fn set_asset_dirs(&mut self, dirs: Vec<std::path::PathBuf>) {
        self.call_raw(move |c| {
            let mut st = c.st.borrow_mut();
            st.imgs = Arc::new(ImageStore::new(dirs.clone()));
            st.dirs = dirs;
            st.cache = None;
            c.dirty = true;
        });
    }

    fn set_depot(&mut self, depot: &omsi_sim::vehicle_api::ApiValue) {
        let depot = depot.clone();
        self.call(move |c| c.omsi_set("depot", &depot));
    }

    fn take_requests(&mut self) -> Vec<omsi_sim::htmltex::HtmlRequest> {
        self.call(|c| std::mem::take(&mut c.st.borrow_mut().requests)).unwrap_or_default()
    }

    fn set_departures(&mut self, departures: &omsi_sim::vehicle_api::ApiValue) {
        let departures = departures.clone();
        self.call(move |c| c.omsi_set("departures", &departures));
    }

    fn take_departure_wants(&mut self) -> Vec<String> {
        self.call(|c| std::mem::take(&mut c.st.borrow_mut().departure_wants)).unwrap_or_default()
    }
}
