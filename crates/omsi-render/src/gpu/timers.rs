//! GPU pass timers (OMSI_GPU_TIMERS), the device poller thread and the encoding pool.

use crate::*;

/// GPU time per render pass from timestamp queries (OMSI_GPU_TIMERS). One frame is timed
/// at a time: its readback has to arrive before the next one is measured. The passes
/// partition the frame: each counts from the end of the one the GPU finished before it, so
/// untimed passes are in the next timed one's figure and the figures add up to
/// "(all passes)".
pub(crate) struct GpuTimers {
    pub(crate) set: wgpu::QuerySet,
    pub(crate) resolve: wgpu::Buffer,
    pub(crate) read: wgpu::Buffer,
    /// The passes of the frame being read back, in query order.
    pub(crate) pending: Vec<&'static str>,
    /// The frame's passes are timed, their stamps not yet resolved (see `collect_gpu_timers`).
    pub(crate) unresolved: bool,
    pub(crate) waiting: bool,
    pub(crate) ready: Arc<std::sync::atomic::AtomicBool>,
    /// The readback's mapping failed: the timers start over rather than wait for it forever.
    pub(crate) failed: Arc<std::sync::atomic::AtomicBool>,
    /// pass → (seconds, frames)
    pub(crate) totals: std::collections::BTreeMap<&'static str, (f64, u32)>,
}

pub(crate) const GPU_TIMER_PASSES: u32 = 16;

impl GpuTimers {
    pub(crate) fn new(device: &wgpu::Device) -> Option<GpuTimers> {
        if !omsi_cfg::flags::OMSI_GPU_TIMERS.is_set()
            || !device.features().contains(wgpu::Features::TIMESTAMP_QUERY)
        {
            return None;
        }
        let set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("pass timers"),
            ty: wgpu::QueryType::Timestamp,
            count: GPU_TIMER_PASSES * 2,
        });
        let size = (GPU_TIMER_PASSES as u64 * 16).div_ceil(wgpu::QUERY_RESOLVE_BUFFER_ALIGNMENT)
            * wgpu::QUERY_RESOLVE_BUFFER_ALIGNMENT;
        let resolve = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pass timers"),
            size,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let read = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pass timers read"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Some(GpuTimers {
            set,
            resolve,
            read,
            pending: Vec::new(),
            unresolved: false,
            waiting: false,
            ready: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            failed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            totals: Default::default(),
        })
    }
}

/// The timestamp writes of the next timed pass (none when this frame is not timed).
pub(crate) fn pass_timer<'a>(
    set: Option<&'a wgpu::QuerySet>,
    timed: &mut Vec<&'static str>,
    label: &'static str,
) -> Option<wgpu::RenderPassTimestampWrites<'a>> {
    let set = set?;
    if timed.len() as u32 >= GPU_TIMER_PASSES {
        return None;
    }
    let i = timed.len() as u32 * 2;
    timed.push(label);
    Some(wgpu::RenderPassTimestampWrites {
        query_set: set,
        beginning_of_pass_write_index: Some(i),
        end_of_pass_write_index: Some(i + 1),
    })
}

pub(crate) struct DevicePoller {
    pub(crate) stop: Arc<std::sync::atomic::AtomicBool>,
    pub(crate) thread: Option<std::thread::JoinHandle<()>>,
}

impl DevicePoller {
    pub(crate) fn start(device: &wgpu::Device) -> Option<Self> {
        // Not on OpenGL: there every poll takes the one GL context, and whenever the thread
        // drawing held it for more than a second (a big shader linked while the world
        // loads, a slow chip's frame) this thread gave up with wgpu-hal's panic "Could not
        // lock adapter context" (#898, after #843). It is not needed there: every submit
        // of the frame runs the same upkeep (wgpu-core's `maintain` after `queue.submit`).
        if cfg!(target_arch = "wasm32") || gl_backend() || omsi_cfg::flags::OMSI_NO_POLL_THREAD.is_set() {
            return None;
        }
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (device, flag) = (device.clone(), stop.clone());
        let thread = std::thread::Builder::new()
            .name("omsi-gpu-poll".into())
            .spawn(move || {
                // (every 4 ms: a frame's own submits run the same upkeep, and this thread
                // only takes the freeing of finished work off the frame's thread; polling
                // every millisecond, a few dozen times a frame, it fought the frame's thread
                // for the queue's lock - a third of its time was spent spinning on it)
                let pause = std::time::Duration::from_millis(4);
                while !flag.load(std::sync::atomic::Ordering::Relaxed) {
                    let _ = device.poll(wgpu::PollType::Poll);
                    std::thread::sleep(pause);
                }
            })
            .ok()?;
        Some(Self { stop, thread: Some(thread) })
    }
}

impl Drop for DevicePoller {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

pub(crate) fn in_scope<'s, R>(pool: Option<&rayon::ThreadPool>, op: impl FnOnce(&rayon::Scope<'s>) -> R) -> R {
    match pool {
        Some(p) => p.in_place_scope(op),
        None => rayon::in_place_scope(op),
    }
}

/// How many parts `run_parts` takes `n` instances in: many small ones, so a dense part holds up none.
pub(crate) fn instance_parts(pool: Option<&rayon::ThreadPool>, n: usize) -> usize {
    match pool {
        Some(p) => (n / 1024).clamp(1, (p.current_num_threads() + 1) * 8),
        // (a thread per part without a pool)
        None => (n / 8192).clamp(1, 4),
    }
}

pub(crate) fn run_parts<T: Send>(pool: Option<&rayon::ThreadPool>, parts: usize, f: impl Fn(usize) -> T + Sync) -> Vec<T> {
    let Some(pool) = pool else {
        return std::thread::scope(|s| {
            let f = &f;
            let helpers: Vec<_> = (1..parts.max(1)).map(|k| s.spawn(move || f(k))).collect();
            let mut out = vec![f(0)];
            out.extend(helpers.into_iter().map(|h| h.join().expect("render part")));
            out
        });
    };
    // The parts are taken in turn by whoever is free, the calling thread too: a helper the
    // system has not got round to running yet holds up no part of its own - the caller,
    // done with one part, takes the next instead of waiting for it.
    let n = parts.max(1);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let out: Vec<std::sync::Mutex<Option<T>>> = (0..n).map(|_| std::sync::Mutex::new(None)).collect();
    let work = || loop {
        let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if k >= n {
            break;
        }
        let v = f(k);
        *out[k].lock().unwrap_or_else(|e| e.into_inner()) = Some(v);
    };
    pool.in_place_scope(|s| {
        let work = &work;
        for _ in 1..n {
            s.spawn(move |_| work());
        }
        work();
    });
    out.into_iter().map(|o| o.into_inner().unwrap_or_else(|e| e.into_inner()).expect("render part")).collect()
}

impl GpuTimers {
    /// Take in the pass times of the last timed frame once its readback has arrived.
    pub(crate) fn collect(&mut self, period: f64) {
        let t = self;
        if t.waiting && t.failed.swap(false, std::sync::atomic::Ordering::Relaxed) {
            // (nothing mapped to read: the next timed frame asks again)
            t.waiting = false;
            return;
        }
        if !t.waiting || !t.ready.swap(false, std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let n = t.pending.len() * 2;
        {
            let view = t.read.slice(0..n as u64 * 8).get_mapped_range();
            let stamps: &[u64] = bytemuck::cast_slice(&view[..n * 8]);
            // The passes in the order the GPU finished them, each counted from where the one
            // before it ended (the untimed passes in between go to the next timed one). A
            // tile-based GPU takes a pass's first stamp when its vertex stage starts, well
            // before the pass in front of it has finished its fragments: measured from their
            // own first stamps the post passes of the enhanced path overlapped and added up
            // to 25 ms of a 10 ms frame.
            if omsi_cfg::flags::OMSI_GPU_TIMERS_RAW.is_set() {
                log::info!(
                    "gpu stamps: {:?}",
                    t.pending
                        .iter()
                        .enumerate()
                        .map(|(k, label)| (*label, stamps[k * 2], stamps[k * 2 + 1]))
                        .collect::<Vec<_>>()
                );
            }
            let (passes, all) = partition(&t.pending, stamps);
            for (label, ticks) in passes {
                let e = t.totals.entry(label).or_default();
                e.0 += ticks as f64 * period * 1e-9;
                e.1 += 1;
            }
            if let Some(ticks) = all {
                let e = t.totals.entry("(all passes)").or_default();
                e.0 += ticks as f64 * period * 1e-9;
                e.1 += 1;
            }
        }
        t.read.unmap();
        t.waiting = false;
    }
}

/// The timed passes of a picture (`labels`, their `stamps` begin and end in query order) as
/// the GPU finished them, each counted from where the one before it ended - the untimed
/// passes in between go to the next timed one - and the whole picture, from the first begin
/// to the last end (in timestamp ticks).
fn partition(labels: &[&'static str], stamps: &[u64]) -> (Vec<(&'static str, u64)>, Option<u64>) {
    let mut order: Vec<(u64, u64, &'static str)> = labels
        .iter()
        .enumerate()
        .filter_map(|(k, label)| Some((*stamps.get(k * 2)?, *stamps.get(k * 2 + 1)?, *label)))
        .filter(|(a, b, _)| *b >= *a && *b > 0)
        .collect();
    order.sort_by_key(|(_, b, _)| *b);
    let mut prev: Option<u64> = None;
    let mut out = Vec::with_capacity(order.len());
    for (a, b, label) in &order {
        out.push((*label, b.saturating_sub(prev.unwrap_or(*a))));
        prev = Some(*b);
    }
    let all = order.iter().map(|o| o.0).min().zip(order.last()).map(|(first, last)| last.1.saturating_sub(first));
    (out, all)
}

/// A mirror's average GPU time per picture as a time per frame: a frame draws
/// `mirrors / windows` mirror pictures on average (0, 1 or 2 of them; one is timed at a time).
fn per_frame(per_picture_ms: f64, mirrors: u64, windows: u64) -> Option<f64> {
    (windows > 0).then(|| per_picture_ms * mirrors as f64 / windows as f64)
}

impl Renderer {
    /// Take in the pass times of the last timed frame once its readback has arrived.
    pub(crate) fn collect_gpu_timers(&mut self) {
        let period = self.queue.get_timestamp_period() as f64;
        for t in self.gpu_timers.iter_mut().flatten() {
            t.collect(period);
            // The last frame's stamps are read in a command buffer of their own, submitted
            // after that frame's: resolved in the frame's own buffer, the end stamp of its
            // last pass was often not written yet (Metal) and read as a stale number.
            if t.unresolved {
                t.unresolved = false;
                let n = t.pending.len() as u32 * 2;
                let mut enc = self
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("pass timers"),
                    });
                enc.resolve_query_set(&t.set, 0..n, &t.resolve, 0);
                enc.copy_buffer_to_buffer(&t.resolve, 0, &t.read, 0, n as u64 * 8);
                self.queue.submit([enc.finish()]);
                let (ready, failed) = (t.ready.clone(), t.failed.clone());
                t.read.map_async(wgpu::MapMode::Read, .., move |r| {
                    let flag = if r.is_ok() { &ready } else { &failed };
                    flag.store(true, std::sync::atomic::Ordering::Relaxed)
                });
                t.waiting = true;
            }
        }
    }

    /// Average GPU time per pass so far: (pass, milliseconds, frames measured).
    /// The mirrors' frames (drawn without the overlays) are timed on their own.
    pub fn gpu_pass_times(&self) -> Vec<(String, f64, u32)> {
        let mut out = Vec::new();
        for (k, t) in self.gpu_timers.iter().enumerate() {
            let Some(t) = t else { continue };
            for (label, v) in &t.totals {
                let name = if k == 0 {
                    format!("mirrors: {label}")
                } else {
                    label.to_string()
                };
                out.push((name, v.0 / v.1.max(1) as f64 * 1000.0, v.1));
            }
        }
        // the mirrors per frame, not per picture: a frame drawing two mirrors pays twice
        let [mirrors, windows] = self.pictures.get();
        let mirror_all = out.iter().find(|(n, _, _)| n == "mirrors: (all passes)").map(|(_, ms, k)| (*ms, *k));
        if let Some((ms, k)) = mirror_all {
            if let Some(f) = per_frame(ms, mirrors, windows) {
                out.push(("mirrors: per frame".to_string(), f, k));
            }
        }
        out
    }
}

#[cfg(test)]
mod timer_tests {
    use super::{partition, per_frame};

    /// The first timed pass counts from its own begin; each other from the end of the one
    /// finished before it (the untimed work between them goes to it); the whole picture from
    /// the first begin. A split main pass timed in both parts: nothing of it is left out.
    #[test]
    fn the_passes_partition_the_picture() {
        // main part 100..400, (untimed gap 400..450), main 450..900, upscale 900..1000
        let (p, all) = partition(&["main part", "main", "upscale"], &[100, 400, 450, 900, 905, 1000]);
        assert_eq!(p, vec![("main part", 300), ("main", 500), ("upscale", 100)]);
        assert_eq!(all, Some(900));
        // with the first part untimed (as before), the main pass alone missed its 300
        let (p, all) = partition(&["main", "upscale"], &[450, 900, 905, 1000]);
        assert_eq!(p, vec![("main", 450), ("upscale", 100)]);
        assert_eq!(all, Some(550));
        // stamps never written (0) or backwards are left out; short stamp lists are not read past
        let (p, _) = partition(&["a", "b", "c"], &[10, 20, 0, 0, 30]);
        assert_eq!(p, vec![("a", 10)]);
    }

    #[test]
    fn the_mirrors_per_frame() {
        assert_eq!(per_frame(5.0, 200, 100), Some(10.0));
        assert_eq!(per_frame(5.0, 50, 100), Some(2.5));
        assert_eq!(per_frame(5.0, 0, 100), Some(0.0));
        assert_eq!(per_frame(5.0, 3, 0), None);
    }
}

#[cfg(test)]
mod run_parts_tests {
    /// Every part runs once and comes back in its place, however the threads take them -
    /// with the caller taking them too while a helper is slow to start.
    #[test]
    fn every_part_once_in_order() {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(2).build().unwrap();
        let runs = std::sync::atomic::AtomicUsize::new(0);
        let out = super::run_parts(Some(&pool), 9, |k| {
            runs.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if k == 1 {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            k * 10
        });
        assert_eq!(out, (0..9).map(|k| k * 10).collect::<Vec<_>>());
        assert_eq!(runs.into_inner(), 9);
        assert_eq!(super::run_parts(None, 3, |k| k), vec![0, 1, 2]);
        assert_eq!(super::run_parts(Some(&pool), 0, |k| k), vec![0]);
    }

    /// A city's instances in many parts with a pool, a few threads' worth without one.
    #[test]
    fn instance_parts_many_with_a_pool_few_without() {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(8).build().unwrap();
        assert_eq!(super::instance_parts(Some(&pool), 140_000), 72);
        assert_eq!(super::instance_parts(Some(&pool), 500), 1);
        assert_eq!(super::instance_parts(None, 140_000), 4);
    }
}
