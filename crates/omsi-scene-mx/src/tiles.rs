//! Tile streaming: [`Streamer`] loads the tiles around the camera and the player's bus on a
//! worker thread, uploads what they produced a little each frame and unloads the far ones.
//! The map-wide index and the placement of the attached objects are
//! `omsi_world::map_index`.

use glam::DVec3;
use omsi_map::tile_size;
use std::path::PathBuf;


/// A finished batch from the worker: the prepared tiles, their statistics and how long the
/// preparation took.
/// A finished batch: the tiles asked for, what was made of them (a tile that could not be
/// made is missing), statistics and seconds.
type Batch = (Vec<(i32, i32)>, Vec<crate::Prepared>, crate::LoadStats, f64);

/// The threads tiles are prepared on while the game runs: a pool of their own, a third of
/// the cores, so that the frame's parallel work (culling, the AI scripts) never waits for a
/// tile to finish on the shared pool - and at a lower priority, so that they do not take
/// the cores those workers need either (see `threads`).
fn loader_pool() -> &'static rayon::ThreadPool {
    static POOL: std::sync::OnceLock<rayon::ThreadPool> = std::sync::OnceLock::new();
    POOL.get_or_init(|| {
        let n = (std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4) / 3).max(2);
        rayon::ThreadPoolBuilder::new().num_threads(n).thread_name(|i| format!("tile loader {i}")).start_handler(|_| crate::threads::lower_thread_priority()).build().expect("tile loader pool")
    })
}

/// Candidates whose tile rectangles may touch `radius` around a center. The exact
/// circular distance check follows in `missing`.
fn tile_candidates(
    lookup: &hashbrown::HashMap<(i32, i32), Vec<usize>>,
    tile_count: usize,
    centers: &[DVec3],
    radius: f64,
) -> Vec<usize> {
    let ts = tile_size();
    let mut candidates: Vec<usize> = Vec::new();
    for c in centers {
        let min_x = ((c.x - radius) / ts).floor() as i32 - 1;
        let max_x = ((c.x + radius) / ts).floor() as i32;
        let min_y = ((c.y - radius) / ts).floor() as i32 - 1;
        let max_y = ((c.y + radius) / ts).floor() as i32;
        let cells = (max_x as i64 - min_x as i64 + 1) * (max_y as i64 - min_y as i64 + 1);
        if cells > tile_count as i64 * 2 {
            candidates.extend(0..tile_count);
            break;
        }
        for x in min_x..=max_x {
            for y in min_y..=max_y {
                if let Some(indices) = lookup.get(&(x, y)) {
                    candidates.extend_from_slice(indices);
                }
            }
        }
    }
    candidates.sort_unstable();
    candidates.dedup();
    candidates
}

fn initial_stall_due(
    stalled_for: std::time::Duration,
    since_last_stall: Option<std::time::Duration>,
) -> bool {
    stalled_for >= std::time::Duration::from_secs(15)
        && match since_last_stall {
            Some(age) => age >= std::time::Duration::from_secs(30),
            None => true,
        }
}

/// Loads the tiles around a few points as they move (the camera, and the player's bus,
/// which must not lose the ground under it when the free camera flies off), like OMSI's
/// tile streaming: the tiles within `load_radius` of any of them are read, tessellated and
/// placed on a worker thread (a few at a time, nearest first), put on the GPU a little each
/// frame, and the tiles beyond `unload_radius` of all of them are taken away again, so the
/// memory a map needs is bounded by the view distance and not by the size of the map.
pub struct Streamer {
    world: std::sync::Arc<crate::World>,
    tiles: Vec<(i32, i32, PathBuf)>,
    /// Indices into `tiles`, including duplicate coordinates, in map-file order.
    tile_lookup: hashbrown::HashMap<(i32, i32), Vec<usize>>,
    pub load_radius: f64,
    pub unload_radius: f64,
    tx: std::sync::mpsc::Sender<Batch>,
    rx: std::sync::mpsc::Receiver<Batch>,
    inflight: bool,
    queue: std::collections::VecDeque<crate::PendingUpload>,
    /// Tiles in flight or waiting for upload.
    requested: hashbrown::HashSet<(i32, i32)>,
    /// Tiles that could not be made (a damaged file): not asked for again.
    failed: hashbrown::HashSet<(i32, i32)>,
    /// The first area around the start: (tiles, of which uploaded).
    initial: Option<(hashbrown::HashSet<(i32, i32)>, usize)>,
    pub stats: crate::LoadStats,
    pub loaded_total: usize,
    pub unloaded_total: usize,
    /// Seconds the worker spent preparing, and the longest upload work of one frame.
    pub prepare_secs: f64,
    pub worst_upload_ms: f64,
    /// The longest frame share of streaming after the first area (upload, unload and the
    /// lists), in ms, and how often it took more than 16 ms.
    pub worst_frame_ms: f64,
    pub slow_frames: usize,
    started: std::time::Instant,
    last_summary: std::time::Instant,
    /// First-area diagnostics are deliberately tiny: one progress line per tile and a
    /// rate-limited stall line. They exist only while the loading screen is up.
    initial_last_done: usize,
    initial_last_progress: std::time::Instant,
    initial_last_stall_log: Option<std::time::Instant>,
    /// The worker batch currently being prepared, for a useful stall message.
    inflight_batch: Option<(Vec<(i32, i32)>, std::time::Instant)>,
    /// One initial tile may take several frames to upload/place; time it across those frames.
    initial_upload: Option<((i32, i32), std::time::Instant)>,
    /// Tiles loaded and unloaded when the heap's free pages were last given back, and when.
    relieved_at: (usize, usize, std::time::Instant),
    /// Hands memory the allocator holds free back to the system after tiles were let
    /// go (the game's own, per platform: see `new`).
    relieve: fn(),
    /// Since when tiles came in whose stops, obstacles and lights are not in the world's
    /// lists yet (see `update`).
    lists_due: Option<std::time::Instant>,
    /// Loaded tiles to be read again (see `reload`).
    stale: hashbrown::HashSet<(i32, i32)>,
}

/// How long the lists of the world (`World::refresh_tile_lists`) may leave out tiles that
/// came in: they are made again for all the loaded tiles at once, 0.3 to 6 ms on
/// Berlin-Spandau, and while driving a tile came in every few frames.
const LISTS_AFTER_LOADS: std::time::Duration = std::time::Duration::from_millis(400);

impl Streamer {
    /// Start streaming around `centers`: the tiles within `initial_radius` of them form the
    /// first area, which is what the loading screen waits for.
    pub fn new(world: std::sync::Arc<crate::World>, centers: &[DVec3], load_radius: f64, initial_radius: f64, relieve: fn()) -> Streamer {
        let tiles = world.select_tiles(None, None);
        let mut tile_lookup: hashbrown::HashMap<(i32, i32), Vec<usize>> = hashbrown::HashMap::new();
        for (i, t) in tiles.iter().enumerate() {
            tile_lookup.entry((t.0, t.1)).or_default().push(i);
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let mut s = Streamer {
            world,
            tiles,
            tile_lookup,
            load_radius,
            unload_radius: load_radius + tile_size() * 1.5,
            tx,
            rx,
            inflight: false,
            queue: Default::default(),
            requested: Default::default(),
            failed: Default::default(),
            initial: None,
            stats: Default::default(),
            loaded_total: 0,
            unloaded_total: 0,
            prepare_secs: 0.0,
            worst_upload_ms: 0.0,
            worst_frame_ms: 0.0,
            slow_frames: 0,
            started: std::time::Instant::now(),
            last_summary: std::time::Instant::now(),
            initial_last_done: 0,
            initial_last_progress: std::time::Instant::now(),
            initial_last_stall_log: None,
            inflight_batch: None,
            initial_upload: None,
            relieved_at: (0, 0, std::time::Instant::now()),
            relieve,
            lists_due: None,
            stale: Default::default(),
        };
        let first: hashbrown::HashSet<(i32, i32)> = s.tiles.iter().filter(|t| Self::nearest(centers, t.0, t.1) <= initial_radius.min(load_radius)).map(|t| (t.0, t.1)).collect();
        log::info!("tile streaming: {} tiles in the map, load radius {:.0} m around {} points ({} tiles now), first area {} tiles", s.tiles.len(), load_radius, centers.len(), s.tiles.iter().filter(|t| Self::nearest(centers, t.0, t.1) <= load_radius).count(), first.len());
        s.initial = Some((first, 0));
        s
    }

    /// Distance from `p` to the nearest point of tile (tx, ty).
    pub fn distance(p: DVec3, tx: i32, ty: i32) -> f64 {
        let ts = tile_size();
        let (x0, y0) = (tx as f64 * ts, ty as f64 * ts);
        let dx = (x0 - p.x).max(0.0).max(p.x - (x0 + ts));
        let dy = (y0 - p.y).max(0.0).max(p.y - (y0 + ts));
        (dx * dx + dy * dy).sqrt()
    }

    /// Distance from the nearest of `centers` to tile (tx, ty).
    pub fn nearest(centers: &[DVec3], tx: i32, ty: i32) -> f64 {
        centers.iter().map(|c| Self::distance(*c, tx, ty)).fold(f64::INFINITY, f64::min)
    }

    /// Read tiles `keys` again (a chrono scenario changed them, the season's textures
    /// changed): those loaded go and come back with the next batches. `None`: every loaded
    /// tile. They go in the frames after, the farthest first and as many a frame as the
    /// unloading's share of the budget allows (see `update`): a snow weather that changed
    /// the season's textures took the whole map away in one frame.
    pub fn reload(&mut self, keys: Option<&[(i32, i32)]>) {
        let keys: Vec<(i32, i32)> = match keys {
            Some(k) => k.to_vec(),
            None => {
                self.world.forget_all_staged();
                self.world.loaded_tiles()
            }
        };
        for k in &keys {
            self.failed.remove(k);
            if !self.requested.contains(k) {
                self.stale.insert(*k);
            }
        }
        log::info!("tile streaming: {} tiles to be read again", keys.len());
    }

    /// (uploaded, total) of the first area while it is still loading.
    pub fn initial_progress(&self) -> Option<(usize, usize)> {
        self.initial.as_ref().filter(|(set, done)| *done < set.len()).map(|(set, done)| (*done, set.len()))
    }

    fn missing(&self, centers: &[DVec3]) -> Vec<(f64, (i32, i32, PathBuf))> {
        let loaded: hashbrown::HashSet<(i32, i32)> = self.world.loaded_tiles().into_iter().collect();
        let mut out: Vec<(f64, (i32, i32, PathBuf))> = tile_candidates(&self.tile_lookup, self.tiles.len(), centers, self.load_radius)
            .into_iter()
            .map(|i| &self.tiles[i])
            .filter(|t| !loaded.contains(&(t.0, t.1)) && !self.requested.contains(&(t.0, t.1)) && !self.failed.contains(&(t.0, t.1)))
            .map(|t| (Self::nearest(centers, t.0, t.1), t.clone()))
            .filter(|(d, _)| *d <= self.load_radius)
            .collect();
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        out
    }

    /// One frame of streaming around `centers`: take in what the worker finished, upload and
    /// place for about `budget` (always a little), unload what is far from all of them (for
    /// about half a budget more, at least one tile), and start the next batch. Returns true
    /// when the loaded tiles changed (the caller then refreshes whatever copies world data).
    pub fn update(&mut self, renderer: &omsi_render::Renderer, scene: &mut omsi_render::Scene, centers: &[DVec3], budget: std::time::Duration, audio: Option<&omsi_audio::AudioEngine>) -> bool {
        let mut changed = false;
        let first_area = self.initial.is_some();
        while let Ok((asked, prepared, stats, secs)) = self.rx.try_recv() {
            self.inflight = false;
            if let Some((keys, started)) = self.inflight_batch.take() {
                let elapsed = started.elapsed().as_secs_f64();
                if elapsed >= 2.0 {
                    log::warn!("tile loading: first-area worker batch {:?} returned after {:.2} s", keys, elapsed);
                } else {
                    log::info!("tile loading: first-area worker batch {:?} returned in {:.2} s", keys, elapsed);
                }
            }
            // a tile the worker could not make is let go (else it stayed "requested" for
            // ever: never retried, never unloaded, and the loading screen waited for it)
            let made: hashbrown::HashSet<(i32, i32)> = prepared.iter().map(|p| (p.tx, p.ty)).collect();
            for k in asked.into_iter().filter(|k| !made.contains(k)) {
                log::warn!("tile streaming: tile {},{} could not be loaded; left out", k.0, k.1);
                self.requested.remove(&k);
                self.failed.insert(k);
                if let Some((_, done)) = self.initial.as_mut().filter(|(set, _)| set.contains(&k)) {
                    *done += 1;
                    changed = true;
                }
            }
            self.prepare_secs += secs;
            self.stats.add_prepared(&stats);
            self.queue.extend(prepared.into_iter().map(|p| self.world.begin_upload(p)));
        }
        let t0 = std::time::Instant::now();
        let mut uploaded = 0usize;
        let mut freed_types = false;
        let deadline = t0 + budget;
        while let Some(mut p) = self.queue.pop_front() {
            let key = p.key();
            let initial = self.initial.as_ref().map(|(set, _)| set.contains(&key)).unwrap_or(false);
            if initial && self.initial_upload.as_ref().map(|(k, _)| *k) != Some(key) {
                self.initial_upload = Some((key, std::time::Instant::now()));
            }
            if !initial && Self::nearest(centers, key.0, key.1) > self.unload_radius {
                // gone out of range while it was being prepared: what it already holds on
                // the GPU goes back with it
                self.requested.remove(&key);
                self.world.abandon_upload(renderer, scene, p, audio);
                changed = true;
                continue;
            }
            let t = std::time::Instant::now();
            // its new textures and object types first, then the tile itself, a little a frame
            let ready = self.world.upload_step(renderer, scene, &mut p, Some(deadline)) && self.world.place_step(renderer, scene, &mut p, Some(deadline));
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            if self.initial.as_ref().map(|(_, done)| *done > 0).unwrap_or(true) {
                self.worst_upload_ms = self.worst_upload_ms.max(ms);
            }
            if !ready {
                self.queue.push_front(p);
                break;
            }
            self.requested.remove(&key);
            if initial {
                let secs = self
                    .initial_upload
                    .take()
                    .filter(|(k, _)| *k == key)
                    .map(|(_, started)| started.elapsed().as_secs_f64())
                    .unwrap_or(ms / 1000.0);
                if secs >= 2.0 {
                    log::warn!("tile loading: slow upload/place tile {},{} took {:.2} s", key.0, key.1, secs);
                } else {
                    log::info!("tile loading: uploaded/placed tile {},{} in {:.2} s", key.0, key.1, secs);
                }
            }
            let mut stats = crate::LoadStats::default();
            self.world.commit_upload(p, &mut stats);
            self.stats.objects += stats.objects;
            self.stats.trees += stats.trees;
            self.stats.splines += stats.splines;
            self.stats.tiles += 1;
            self.loaded_total += 1;
            if let Some((_, done)) = self.initial.as_mut().filter(|(set, _)| set.contains(&key)) {
                *done += 1;
            }
            changed = true;
            uploaded += 1;
            if t0.elapsed() >= budget {
                break;
            }
        }
        let t_upload = t0.elapsed();
        let mut unloaded = 0usize;
        if self.initial.as_ref().map(|(set, done)| *done >= set.len()).unwrap_or(false) {
            let (set, _) = self.initial.take().unwrap();
            self.inflight_batch = None;
            self.initial_upload = None;
            log::info!("tile streaming: first area of {} tiles loaded in {:.2} s", set.len(), self.started.elapsed().as_secs_f64());
        }
        // Far tiles go (never the ones on their way in), the farthest first and only as many
        // as fit in the budget (at least one a frame): a camera jump across a big map had
        // 42 tiles taken away in one frame of 400 ms. Unloading gets its own share of the
        // budget, so that a busy upload queue cannot hold the memory of the old area.
        if self.initial.is_none() {
            let now = std::time::Instant::now();
            let unload_deadline = deadline.max(now + budget / 2);
            let loaded = self.world.loaded_tiles();
            if !self.stale.is_empty() {
                let now_loaded: hashbrown::HashSet<(i32, i32)> = loaded.iter().copied().collect();
                self.stale.retain(|k| now_loaded.contains(k));
            }
            let mut far: Vec<(f64, (i32, i32))> = loaded.into_iter().filter(|k| !self.requested.contains(k)).map(|k| (Self::nearest(centers, k.0, k.1), k)).filter(|(d, k)| *d > self.unload_radius || self.stale.contains(k)).collect();
            far.sort_by(|a, b| b.0.total_cmp(&a.0));
            for (_, key) in far {
                if unloaded > 0 && std::time::Instant::now() >= unload_deadline {
                    break;
                }
                self.stale.remove(&key);
                freed_types |= self.world.unload_tile(renderer, scene, key, audio);
                self.unloaded_total += 1;
                unloaded += 1;
                changed = true;
            }
        }
        if freed_types {
            self.world.trim_object_types();
        }
        if unloaded > 0 {
            // the tiles only they depended on go too
            self.world.trim_staged(&self.requested);
        }
        let t_unload = t0.elapsed();
        // A tile gone must leave the lists at once (they name its instances and materials,
        // which are handed to other objects); tiles that came in - at the edge of the load
        // radius - are put in together, 0.4 s after the first of them, as are the first
        // area's at once.
        if changed {
            self.lists_due.get_or_insert_with(std::time::Instant::now);
        }
        let lists_now = self.lists_due.is_some_and(|t| unloaded > 0 || first_area || t.elapsed() >= LISTS_AFTER_LOADS);
        if lists_now {
            self.lists_due = None;
            self.world.refresh_tile_lists();
        }
        // (the caller hands the new lists on - the bus's obstacles - when they change)
        let changed = changed || lists_now;
        // Loading and unloading tiles leaves the allocator with pages it keeps for later: they
        // count as the game's memory until they are handed back (half a gigabyte on
        // Ahlheim). At most every few seconds, on a thread of its own.
        if (self.loaded_total, self.unloaded_total) != (self.relieved_at.0, self.relieved_at.1) && self.relieved_at.2.elapsed().as_secs_f32() >= 4.0 && self.queue.is_empty() && !self.inflight {
            self.relieved_at = (self.loaded_total, self.unloaded_total, std::time::Instant::now());
            self.world.compact_slots(renderer, scene);
            (self.relieve)();
        }
        if self.last_summary.elapsed().as_secs_f32() >= 10.0 && omsi_cfg::flags::OMSI_PROFILE.is_set() {
            self.last_summary = std::time::Instant::now();
            let at: Vec<String> = centers.iter().map(|c| format!("({:.0}, {:.0})", c.x, c.y)).collect();
            log::info!("tile streaming at {}: {} loaded / {} unloaded so far; {}", at.join(" "), self.loaded_total, self.unloaded_total, self.world.gpu_summary(scene));
            if omsi_cfg::flags::OMSI_PROFILE.is_set() {
                log::info!("  {}", self.world.cpu_summary());
            }
        }
        let total = t0.elapsed();
        if self.initial.is_none() && total.as_secs_f64() * 1000.0 > 16.0 {
            self.slow_frames += 1;
            if omsi_cfg::flags::OMSI_PROFILE.is_set() {
                log::info!("tile streaming: {:.0} ms this frame ({uploaded} uploaded in {:.0} ms, {unloaded} unloaded in {:.0} ms, lists {:.0} ms)", total.as_secs_f64() * 1000.0, t_upload.as_secs_f64() * 1000.0, (t_unload - t_upload).as_secs_f64() * 1000.0, (total - t_unload).as_secs_f64() * 1000.0);
            }
        }
        if self.initial.is_none() {
            self.worst_frame_ms = self.worst_frame_ms.max(total.as_secs_f64() * 1000.0);
        }

        // While the loading screen is up, make a stuck worker visible without writing once
        // per frame. Progress is at most one line per initial tile; a genuine stall is one
        // warning after 15 s and then at most one every 30 s.
        if let Some((done, total_initial)) = self.initial.as_ref().map(|(set, done)| (*done, set.len())) {
            let now = std::time::Instant::now();
            if done != self.initial_last_done {
                self.initial_last_done = done;
                self.initial_last_progress = now;
                self.initial_last_stall_log = None;
                log::info!(
                    "tile loading: first area progress {}/{} after {:.1} s",
                    done,
                    total_initial,
                    self.started.elapsed().as_secs_f64()
                );
            } else {
                let stalled_for = now.duration_since(self.initial_last_progress);
                let since_last = self.initial_last_stall_log.map(|t| now.duration_since(t));
                if initial_stall_due(stalled_for, since_last) {
                    let worker = self
                        .inflight_batch
                        .as_ref()
                        .map(|(keys, t)| format!("worker {:?} running {:.1} s", keys, t.elapsed().as_secs_f64()))
                        .unwrap_or_else(|| "worker idle".to_string());
                    let upload = self
                        .initial_upload
                        .as_ref()
                        .map(|(key, t)| format!("upload/place {},{} running {:.1} s", key.0, key.1, t.elapsed().as_secs_f64()))
                        .unwrap_or_else(|| format!("{} prepared tile(s) waiting for upload", self.queue.len()));
                    log::warn!(
                        "tile loading: first area stalled at {}/{} for {:.1} s; {}; {}",
                        done,
                        total_initial,
                        stalled_for.as_secs_f64(),
                        worker,
                        upload
                    );
                    self.initial_last_stall_log = Some(now);
                }
            }
        }

        if !self.inflight {
            let missing = self.missing(centers);
            if !missing.is_empty() {
                // the first area in bigger bites (the loading screen shows the progress),
                // then a few tiles at a time
                let n = if self.initial.is_some() { 6 } else { 3 };
                let batch: Vec<(i32, i32, PathBuf)> = missing.into_iter().take(n).map(|(_, t)| t).collect();
                let keys: Vec<(i32, i32)> = batch.iter().map(|t| (t.0, t.1)).collect();
                self.requested.extend(keys.iter().copied());
                self.inflight = true;
                let world = self.world.clone();
                let tx = self.tx.clone();
                // the first area gets every core; later tiles only the loader's own threads
                let first = self.initial.is_some();
                if first {
                    log::info!("tile loading: first-area worker batch {:?} started", keys);
                    self.inflight_batch = Some((keys.clone(), std::time::Instant::now()));
                }
                let spawned = std::thread::Builder::new().name("tile loader".into()).spawn(move || {
                    let t = std::time::Instant::now();
                    // (a panic on a damaged file must not end the streaming: the batch comes
                    // back empty and its tiles are let go)
                    let made = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| if first { world.prepare_tiles_initial(&batch) } else { loader_pool().install(|| world.prepare_tiles(&batch)) }));
                    let (prepared, stats) = made.unwrap_or_else(|_| {
                        log::error!("tile streaming: loading tiles {:?} failed", batch.iter().map(|t| (t.0, t.1)).collect::<Vec<_>>());
                        Default::default()
                    });
                    let _ = tx.send((batch.iter().map(|t| (t.0, t.1)).collect(), prepared, stats, t.elapsed().as_secs_f64()));
                });
                if let Err(e) = spawned {
                    log::warn!("tile loader thread: {e}");
                    self.inflight = false;
                    self.inflight_batch = None;
                    for k in keys {
                        self.requested.remove(&k);
                    }
                }
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_area_stall_log_is_delayed_and_rate_limited() {
        use std::time::Duration;

        assert!(!initial_stall_due(Duration::from_secs(14), None));
        assert!(initial_stall_due(Duration::from_secs(15), None));
        assert!(!initial_stall_due(
            Duration::from_secs(60),
            Some(Duration::from_secs(29))
        ));
        assert!(initial_stall_due(
            Duration::from_secs(60),
            Some(Duration::from_secs(30))
        ));
    }

    #[test]
    fn indexed_tile_search_matches_full_distance_scan() {
        let coords: Vec<(i32, i32)> = (-5..=5).flat_map(|x| (-5..=5).map(move |y| (x, y))).collect();
        let mut lookup: hashbrown::HashMap<(i32, i32), Vec<usize>> = hashbrown::HashMap::new();
        for (i, &key) in coords.iter().enumerate() {
            lookup.entry(key).or_default().push(i);
        }
        let ts = tile_size();
        for centers in [
            vec![DVec3::new(0.0, 0.0, 0.0)],
            vec![DVec3::new(-0.3 * ts, 1.8 * ts, 0.0), DVec3::new(3.2 * ts, -2.1 * ts, 0.0)],
        ] {
            for radius in [0.0, 0.7 * ts, 2.5 * ts, 20.0 * ts] {
                let expected: Vec<usize> = coords.iter().enumerate()
                    .filter_map(|(i, &(x, y))| (Streamer::nearest(&centers, x, y) <= radius).then_some(i))
                    .collect();
                let actual: Vec<usize> = tile_candidates(&lookup, coords.len(), &centers, radius)
                    .into_iter()
                    .filter(|&i| Streamer::nearest(&centers, coords[i].0, coords[i].1) <= radius)
                    .collect();
                assert_eq!(actual, expected, "centers {centers:?}, radius {radius}");
            }
        }
    }
}
