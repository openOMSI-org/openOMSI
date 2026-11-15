//! The launcher showroom on a worker, isolated from the active game's renderer.

use crate::launcher::showroom::{Look, PreviewView, Showroom};
use omsi_render::{RenderOptions, Renderer};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::time::{Duration, Instant};

#[derive(Clone, PartialEq)]
struct Request {
    serial: u64,
    look: Look,
    view: PreviewView,
    orbit: (f32, f32),
    zoom: f32,
}

pub(crate) struct Reply {
    serial: u64,
    pixels: Option<(u32, u32, Vec<u8>)>,
    busy: bool,
    error: Option<String>,
    views: Vec<&'static str>,
}

struct Worker {
    tx: SyncSender<Request>,
    rx: Receiver<Reply>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            // Joining off the UI thread keeps closing responsive even during asset I/O.
            std::thread::spawn(move || match thread.join() {
                Ok(()) => log::info!("bus preview: worker joined; thread exited"),
                Err(_) => log::warn!("bus preview: worker panicked during shutdown"),
            });
        }
    }
}

#[derive(Default)]
pub(crate) struct State {
    pub active: bool,
    pub title: String,
    pub meta: String,
    pub busy: bool,
    pub error: Option<String>,
    pub views: Vec<&'static str>,
    pub view: PreviewView,
    pub texture: Option<usize>,
    pub ready: bool,
    pub drag: Option<glam::Vec2>,
    pub orbit: (f32, f32),
    pub zoom: f32,
    look: Option<Look>,
    selection: Option<(crate::game_lists::ListKind, usize, Option<(String, String)>)>,
    serial: u64,
    sent: Option<Request>,
    worker: Option<Worker>,
    shutdown: Option<std::thread::JoinHandle<()>>,
    pixels: Option<(u32, u32, Vec<u8>)>,
}

impl State {
    pub fn invalidate_selection(&mut self) {
        self.selection = None;
    }

    pub fn selection_changed(
        &mut self,
        kind: &crate::game_lists::ListKind,
        index: usize,
        row: Option<&(String, String)>,
    ) -> bool {
        if self
            .selection
            .as_ref()
            .is_some_and(|(previous, selected, item)| {
                previous == kind && *selected == index && item.as_ref() == row
            })
        {
            return false;
        }
        self.selection = Some((kind.clone(), index, row.cloned()));
        true
    }

    pub fn invalidate_texture(&mut self) {
        self.texture = None;
        self.ready = false;
        self.serial += 1;
        self.sent = None;
    }

    pub fn select(&mut self, look: Option<Look>, title: String, meta: String) {
        self.active = true;
        self.title = title;
        self.meta = meta;
        if self.look == look {
            return;
        }
        self.serial += 1;
        self.ready = false;
        self.pixels = None;
        self.error = None;
        self.busy = look.is_some();
        if self.look.as_ref().map(|l| &l.bus) != look.as_ref().map(|l| &l.bus) {
            self.view = PreviewView::Outside;
            self.orbit = (0.0, 0.0);
            self.zoom = 1.0;
            self.drag = None;
            self.views.clear();
        }
        self.look = look;
        if self.look.is_none() {
            self.sent = None;
        }
    }

    fn request(&mut self) -> Option<Request> {
        let mut request = Request {
            serial: self.serial,
            look: self.look.clone()?,
            view: self.view,
            orbit: self.orbit,
            zoom: self.zoom,
        };
        if let Some(sent) = &self.sent {
            // Completed camera frames remain useful while dragging; only a view switch
            // invalidates them. Bus and paint changes advance the serial in select().
            if request.view != sent.view && request.serial == sent.serial {
                self.serial += 1;
                request.serial = self.serial;
            }
        }
        Some(request)
    }

    fn accept(&mut self, reply: Reply) {
        if reply.serial != self.serial {
            return;
        }
        self.busy = reply.busy;
        self.error = reply.error;
        self.views = reply.views;
        if self.error.is_some() {
            self.ready = false;
            self.pixels = None;
        }
        if let Some(pixels) = reply.pixels {
            self.pixels = Some(pixels);
            self.ready = true;
        }
    }

    fn shutdown_pending(&mut self) -> bool {
        let Some(thread) = self.shutdown.as_ref() else {
            return false;
        };
        if !thread.is_finished() {
            return true;
        }
        match self.shutdown.take().unwrap().join() {
            Ok(()) => log::info!("bus preview: worker joined; thread exited"),
            Err(_) => log::warn!("bus preview: worker panicked during shutdown"),
        }
        false
    }

    pub fn update(&mut self, options: RenderOptions, adapter: Option<wgpu::Adapter>) {
        // Keep the text chooser usable while the previous renderer finishes teardown.
        if self.shutdown_pending() {
            return;
        }
        let Some(request) = self.request() else {
            return;
        };
        if self.worker.is_none() {
            let Some(adapter) = adapter.filter(|a| a.get_info().backend != wgpu::Backend::Gl) else {
                self.busy = false;
                return;
            };
            let (tx, requests) = mpsc::sync_channel(1);
            let (answers, rx) = mpsc::sync_channel(1);
            match std::thread::Builder::new()
                .name("bus chooser preview".into())
                .spawn(move || {
                    omsi_render::with_auxiliary_renderer(|| run(requests, answers, options, adapter));
                    // run's showroom (including loaders) and renderer are dropped here.
                    log::info!("bus preview: worker finished; showroom, loaders and renderer released");
                })
            {
                Ok(thread) => self.worker = Some(Worker { tx, rx, thread: Some(thread) }),
                Err(error) => {
                    self.busy = false;
                    self.error = Some(error.to_string());
                    return;
                }
            }
        }
        if self.sent.as_ref() != Some(&request) {
            match self.worker.as_ref().unwrap().tx.try_send(request.clone()) {
                Ok(()) => self.sent = Some(request),
                Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Disconnected(_)) => {
                    self.busy = false;
                    self.error = Some("No preview of this bus".into());
                    self.ready = false;
                    return;
                }
            }
        }
        loop {
            let reply = self.worker.as_ref().unwrap().rx.try_recv();
            match reply {
                Ok(reply) => self.accept(reply),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.busy = false;
                    self.error = Some("No preview of this bus".into());
                    self.ready = false;
                    break;
                }
            }
        }
    }
}

impl super::Shell {
    pub(crate) fn clear_bus_preview(&mut self) {
        if let (Some(texture), Some((gpu, _))) = (self.bus_preview.texture, self.gpu.as_mut()) {
            gpu.free(texture);
        }
        // Preserve the only closing worker across resets; reopening must await its exit.
        let shutdown = self.bus_preview.worker.as_mut().and_then(|worker| worker.thread.take())
            .or_else(|| self.bus_preview.shutdown.take());
        self.bus_preview = State { shutdown, ..State::default() };
    }

    pub(crate) fn upload_bus_preview(&mut self, renderer: &Renderer) {
        self.ensure_gpu(renderer);
        let Some((w, h, pixels)) = self.bus_preview.pixels.take() else {
            return;
        };
        let gpu = &mut self.gpu.as_mut().unwrap().0;
        match self.bus_preview.texture {
            Some(id) => gpu.update_image(&renderer.device, &renderer.queue, id, w, h, &pixels),
            None => {
                self.bus_preview.texture =
                    Some(gpu.add_image(&renderer.device, &renderer.queue, w, h, &pixels))
            }
        }
        self.bus_preview.ready = true;
    }
}

fn run(requests: Receiver<Request>, answers: SyncSender<Reply>, options: RenderOptions, adapter: wgpu::Adapter) {
    let Ok(mut request) = requests.recv() else {
        return;
    };
    let mut renderer = match pollster::block_on(Renderer::new_on(
        adapter,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        options,
    )) {
        Ok(renderer) => renderer,
        Err(error) => {
            let _ = answers.send(Reply {
                serial: request.serial,
                pixels: None,
                busy: false,
                error: Some(error.to_string()),
                views: Vec::new(),
            });
            return;
        }
    };
    let mut showroom = Showroom::new();
    let mut previous: Option<Request> = None;
    let mut pending = None;
    let mut status = None;
    let mut last = Instant::now();
    loop {
        let changed = previous.as_ref() != Some(&request);
        if changed {
            showroom.want(request.look.clone());
            let old = previous.as_ref().filter(|p| p.look.bus == request.look.bus);
            let orbit = old.map(|p| p.orbit).unwrap_or_default();
            let zoom = old.map(|p| p.zoom).unwrap_or(1.0);
            showroom.set_view(request.view);
            showroom.orbit(request.orbit.0 - orbit.0, request.orbit.1 - orbit.1);
            showroom.zoom_by(request.zoom / zoom);
            previous = Some(request.clone());
        }
        showroom.update(&renderer, last.elapsed().as_secs_f32().min(0.1));
        last = Instant::now();
        let drawn = showroom.drawn;
        if !showroom.busy && showroom.error.is_none() {
            showroom.preview(&mut renderer, 960, 540);
        }
        let next_status = (showroom.busy, showroom.error.clone(), showroom.views());
        let new_picture =
            !showroom.busy && showroom.error.is_none() && (showroom.drawn != drawn || changed);
        if new_picture || status.as_ref() != Some(&next_status) {
            pending = Some(Reply {
                serial: request.serial,
                pixels: new_picture.then(|| showroom.pixels(&renderer)).flatten(),
                busy: showroom.busy,
                error: showroom.error.clone(),
                views: showroom.views(),
            });
            status = Some(next_status);
        }
        if let Some(reply) = pending.take() {
            match answers.try_send(reply) {
                Ok(()) => {}
                Err(TrySendError::Full(reply)) => pending = Some(reply),
                Err(TrySendError::Disconnected(_)) => break,
            }
        }
        let waiting = showroom.busy || showroom.moving() || pending.is_some();
        let next = if waiting {
            requests.recv_timeout(Duration::from_millis(16))
        } else {
            requests
                .recv()
                .map_err(|_| mpsc::RecvTimeoutError::Disconnected)
        };
        match next {
            Ok(next) => {
                request = next;
                while let Ok(next) = requests.try_recv() {
                    request = next;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    showroom.close();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_game_bus_preview_close_disconnects_and_releases_worker_payload() {
        let (tx, requests) = mpsc::sync_channel(1);
        let (answers, rx) = mpsc::sync_channel(1);
        let (done, finished) = mpsc::channel();
        let owner = std::sync::Arc::new(());
        let weak = std::sync::Arc::downgrade(&owner);
        let thread = std::thread::spawn(move || {
            let _answers = answers;
            let _owner = owner;
            assert!(requests.recv().is_err());
            drop(_owner);
            done.send(()).unwrap();
        });
        drop(Worker { tx, rx, thread: Some(thread) });
        finished.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn in_game_bus_preview_defers_reopen_until_delayed_teardown_finishes() {
        let mut shell = super::super::Shell::new();
        let (tx, requests) = mpsc::sync_channel(1);
        let (answers, rx) = mpsc::sync_channel(1);
        let (closing, started) = mpsc::channel();
        let (release, delayed) = mpsc::channel();
        let (done, finished) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            let _answers = answers;
            assert!(requests.recv().is_err());
            closing.send(()).unwrap();
            delayed.recv().unwrap();
            done.send(()).unwrap();
        });
        shell.bus_preview.worker = Some(Worker { tx, rx, thread: Some(thread) });
        shell.clear_bus_preview();
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        let mut deferred = Vec::new();
        for _ in 0..10 {
            shell.bus_preview.select(Some(Look { bus: "bus".into(), ..Default::default() }),
                "Selected bus".into(), "Text chooser stays responsive".into());
            shell.bus_preview.update(RenderOptions::default(), None);
            deferred.push(shell.bus_preview.busy);
            assert_eq!(shell.bus_preview.title, "Selected bus");
            assert!(shell.bus_preview.worker.is_none());
            shell.clear_bus_preview();
        }
        release.send(()).unwrap();
        finished.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(deferred.into_iter().all(|waiting| waiting),
            "every reopen must wait for the previous worker, even across repeated clears");
        let deadline = Instant::now() + Duration::from_secs(5);
        while shell.bus_preview.shutdown_pending() {
            assert!(Instant::now() < deadline, "completed teardown must free the worker slot");
            std::thread::yield_now();
        }
        assert!(shell.bus_preview.shutdown.is_none());
        shell.bus_preview.select(Some(Look { bus: "next".into(), ..Default::default() }),
            "Next bus".into(), String::new());
        shell.bus_preview.update(RenderOptions::default(), None);
        assert!(!shell.bus_preview.busy, "normal adapter handling must resume after teardown");
    }

    #[test]
    fn in_game_bus_preview_resolves_only_changed_selection() {
        use crate::game_lists::ListKind;
        let mut state = State::default();
        let row = ("My depot: Bus".into(), "depot test".into());
        assert!(state.selection_changed(&ListKind::PlaceMaker, 1, Some(&row)));
        for _ in 0..120 {
            assert!(!state.selection_changed(&ListKind::PlaceMaker, 1, Some(&row)));
        }
        assert!(state.selection_changed(&ListKind::PlaceMaker, 2, Some(&row)));
        let changed = ("Renamed bus".into(), row.1.clone());
        assert!(state.selection_changed(&ListKind::PlaceMaker, 2, Some(&changed)));
        assert!(state.selection_changed(&ListKind::PlaceType("maker".into()), 2, Some(&changed)));
        state.invalidate_selection();
        assert!(state.selection_changed(&ListKind::PlaceType("maker".into()), 2, Some(&changed)));
        assert!(state.selection_changed(&ListKind::PlaceMaker, 0, None));
        assert!(!state.selection_changed(&ListKind::PlaceMaker, 0, None));
    }

    #[test]
    fn in_game_bus_preview_keeps_worker_on_non_vehicle_rows() {
        let mut state = State::default();
        let look = Look {
            bus: "bus".into(),
            ..Default::default()
        };
        state.select(Some(look.clone()), String::new(), String::new());
        let previous = state.request().unwrap();
        state.sent = Some(previous.clone());
        let (tx, requests) = mpsc::sync_channel(1);
        let (answers, rx) = mpsc::sync_channel(1);
        state.worker = Some(Worker { tx, rx, thread: None });
        for _ in 0..10 {
            state.select(None, "Search".into(), String::new());
            assert!(
                state.worker.is_some(),
                "navigation must keep the renderer alive"
            );
            assert!(state.request().is_none());
            assert!(!state.ready);
            assert!(!state.busy);
            state.select(Some(look.clone()), String::new(), String::new());
            assert!(state.worker.is_some());
            assert!(state.request().unwrap().serial > previous.serial);
            state.accept(Reply {
                serial: previous.serial,
                pixels: Some((1, 1, vec![255; 4])),
                busy: false,
                error: None,
                views: Vec::new(),
            });
            assert!(state.pixels.is_none());
        }
        let mut shell = super::super::Shell::new();
        shell.bus_preview = state;
        shell.clear_bus_preview();
        assert!(shell.bus_preview.worker.is_none());
        assert!(requests.recv().is_err());
        assert!(answers
            .send(Reply {
                serial: 0,
                pixels: None,
                busy: false,
                error: None,
                views: Vec::new()
            })
            .is_err());
    }

    #[test]
    fn in_game_bus_preview_delivers_frames_while_dragging() {
        let mut state = State::default();
        state.select(
            Some(Look {
                bus: "bus".into(),
                ..Default::default()
            }),
            String::new(),
            String::new(),
        );
        let first = state.request().unwrap();
        state.sent = Some(first.clone());
        for step in 1..=12 {
            state.orbit.0 += 5.0;
            state.zoom = 1.0 + step as f32 * 0.01;
            let next = state.request().unwrap();
            state.accept(Reply {
                serial: first.serial,
                pixels: Some((1, 1, vec![step; 4])),
                busy: false,
                error: None,
                views: Vec::new(),
            });
            assert!(
                state.pixels.take().is_some(),
                "camera updates must not starve completed frames"
            );
            state.sent = Some(next);
        }
    }

    #[test]
    fn in_game_bus_preview_rejects_previous_paint_and_camera_view() {
        let mut state = State::default();
        let look = Look {
            bus: "bus".into(),
            ..Default::default()
        };
        state.select(Some(look.clone()), String::new(), String::new());
        let original = state.request().unwrap();
        state.sent = Some(original.clone());
        state.select(
            Some(Look {
                paint_index: Some(1),
                ..look
            }),
            String::new(),
            String::new(),
        );
        state.accept(Reply {
            serial: original.serial,
            pixels: Some((1, 1, vec![255; 4])),
            busy: false,
            error: None,
            views: Vec::new(),
        });
        assert!(state.pixels.is_none());
        let painted = state.request().unwrap();
        state.sent = Some(painted.clone());
        state.view = PreviewView::Driver;
        state.request();
        state.accept(Reply {
            serial: painted.serial,
            pixels: Some((1, 1, vec![255; 4])),
            busy: false,
            error: None,
            views: Vec::new(),
        });
        assert!(state.pixels.is_none());
        let previous = state.request().unwrap();
        state.select(
            Some(Look {
                bus: "bus".into(),
                ..Default::default()
            }),
            String::new(),
            String::new(),
        );
        state.accept(Reply {
            serial: previous.serial,
            pixels: Some((1, 1, vec![255; 4])),
            busy: false,
            error: None,
            views: Vec::new(),
        });
        assert!(
            state.pixels.is_none(),
            "returning to default paint must reject the previous scheme"
        );
    }

    #[test]
    #[ignore = "Requires local OMSI assets and a graphics device"]
    fn in_game_bus_preview_rotation_and_repaint_real_vehicle() {
        let _ = env_logger::builder()
            .is_test(true)
            .filter_level(log::LevelFilter::Warn)
            .try_init();
        let root: std::path::PathBuf = std::env::var_os("OMSI_ROOT").expect("OMSI_ROOT").into();
        let registered_root = root.clone();
        let owns_root = !omsi_cfg::content_roots().contains(&root);
        if owns_root {
            omsi_cfg::add_content_root(root.clone());
        }
        let bus = std::env::var("OPENOMSI_PREVIEW_BUS").expect("bus");
        let map = std::env::var("OPENOMSI_PREVIEW_MAP").expect("map");
        let out =
            std::path::PathBuf::from(std::env::var_os("OPENOMSI_PAUSE_PREVIEW").expect("output"));
        let ty = omsi_sim::VehicleType::load(&root, &omsi_cfg::resolve_path(&root, &bus)).unwrap();
        let paint = ty
            .paint_schemes
            .first()
            .expect("vehicle with a paint scheme")
            .name
            .clone();
        let (substitutions, _) = ty.scheme_substitutions(0, omsi_texture::find_texture);
        assert!(
            !substitutions.is_empty(),
            "a visible paint substitution is required"
        );
        for path in substitutions.values() {
            assert!(
                omsi_texture::find_texture(path, &[]).is_some(),
                "paint texture must resolve: {path}"
            );
        }
        let look = Look {
            root,
            bus,
            map,
            ..Default::default()
        };
        let options = crate::launcher::showroom_options(&crate::settings::Settings::load());
        let instance = crate::graphics_instance();
        let renderer = pollster::block_on(Renderer::new_with(&instance, None, None, options)).unwrap();
        let mut state = State::default();
        state.select(Some(look.clone()), "Bus".into(), String::new());
        let wait = |state: &mut State| {
            let start = Instant::now();
            loop {
                state.update(options, renderer.adapter());
                assert!(state.error.is_none(), "{:?}", state.error);
                if let Some(pixels) = state.pixels.take() {
                    break pixels;
                }
                assert!(
                    start.elapsed() < Duration::from_secs(180),
                    "preview timeout"
                );
                std::thread::sleep(Duration::from_millis(16));
            }
        };
        let original = wait(&mut state);
        let start = Instant::now();
        let mut last = start;
        let mut max_gap = Duration::ZERO;
        let mut frames = 0;
        let mut picture = original.clone();
        while start.elapsed() < Duration::from_secs(3) {
            state.orbit.0 = start.elapsed().as_secs_f32() * 100.0;
            state.update(options, renderer.adapter());
            assert!(state.error.is_none(), "{:?}", state.error);
            if let Some(pixels) = state.pixels.take() {
                max_gap = max_gap.max(last.elapsed());
                last = Instant::now();
                picture = pixels;
                frames += 1;
            }
            std::thread::sleep(Duration::from_millis(16));
        }
        max_gap = max_gap.max(last.elapsed());
        let rotation_seconds = start.elapsed().as_secs_f64();
        assert!(frames > 1, "continuous dragging must deliver frames");
        assert_ne!(
            original.2, picture.2,
            "camera movement must change the picture"
        );
        image::save_buffer(
            out.join("bus-preview-rotation.png"),
            &picture.2,
            picture.0,
            picture.1,
            image::ColorType::Rgba8,
        )
        .unwrap();
        state.select(
            Some(Look {
                bus: crate::game_lists::bus_preview::repaint_bus_path(&look.root, &ty.def.path),
                paint_index: Some(0),
                ..look
            }),
            "Bus".into(),
            paint.clone(),
        );
        assert!(!state.ready && state.pixels.is_none());
        let painted = wait(&mut state);
        image::save_buffer(
            out.join("bus-preview-repaint.png"),
            &painted.2,
            painted.0,
            painted.1,
            image::ColorType::Rgba8,
        )
        .unwrap();
        let metrics = serde_json::json!({
            "rotation_seconds": rotation_seconds, "delivered_frames": frames,
            "max_delivery_gap_ms": max_gap.as_secs_f64() * 1000.0, "paint": paint,
            "scope": "offscreen worker rendering and GPU readback; excludes game and UI upload"
        });
        std::fs::write(
            out.join("rotation-metrics.json"),
            serde_json::to_string_pretty(&metrics).unwrap(),
        )
        .unwrap();
        println!("{metrics}");
        if owns_root {
            omsi_cfg::remove_content_root(&registered_root);
        }
    }

    #[test]
    fn in_game_bus_preview_rejects_stale_pictures_and_errors() {
        let mut state = State::default();
        state.select(
            Some(Look {
                bus: "first.bus".into(),
                ..Default::default()
            }),
            String::new(),
            String::new(),
        );
        let old = state.serial;
        state.select(
            Some(Look {
                bus: "second.bus".into(),
                ..Default::default()
            }),
            String::new(),
            String::new(),
        );
        state.accept(Reply {
            serial: old,
            pixels: Some((1, 1, vec![255; 4])),
            busy: false,
            error: Some("old failure".into()),
            views: vec!["driver"],
        });
        assert!(!state.ready);
        assert!(state.error.is_none());
        assert!(state.busy);
        assert!(state.views.is_empty());
    }

    #[test]
    fn in_game_bus_preview_clears_the_picture_on_failure_or_non_vehicle_row() {
        let mut state = State::default();
        state.select(
            Some(Look {
                bus: "bus".into(),
                ..Default::default()
            }),
            String::new(),
            String::new(),
        );
        state.ready = true;
        state.accept(Reply {
            serial: state.serial,
            pixels: None,
            busy: false,
            error: Some("missing model".into()),
            views: Vec::new(),
        });
        assert!(!state.ready);
        state.select(None, String::new(), String::new());
        assert!(state.look.is_none());
        assert!(state.error.is_none());
        assert!(!state.busy);
    }
    #[test]
    fn in_game_bus_preview_worker_disconnect_stops_the_loading_indicator() {
        let mut state = State::default();
        state.select(
            Some(Look {
                bus: "bus".into(),
                ..Default::default()
            }),
            String::new(),
            String::new(),
        );
        let (tx, _requests) = mpsc::sync_channel(1);
        let (answers, rx) = mpsc::sync_channel(1);
        drop(answers);
        state.worker = Some(Worker { tx, rx, thread: None });
        state.sent = state.request();
        state.update(RenderOptions::default(), None);
        assert!(!state.busy);
        assert!(state.error.is_some());
    }
}
