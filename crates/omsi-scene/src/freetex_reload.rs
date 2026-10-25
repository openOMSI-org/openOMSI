//! Loose `[matl_freetex]` pictures can be rewritten by external display generators.
//! File I/O and decoding stay on one low-priority worker, never in view sync.

use hashbrown::HashMap;
use omsi_render::TextureId;
use omsi_texture::TextureData;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, OnceLock, Weak};
use std::time::{Duration, Instant, SystemTime};

const POLL: Duration = Duration::from_millis(250);
const SETTLE: Duration = Duration::from_millis(150);
type Shared = Mutex<HashMap<PathBuf, (TextureId, usize)>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FileStamp {
    modified: SystemTime,
    len: u64,
}

impl FileStamp {
    /// Worker threads only (including the existing texture compression workers).
    pub(crate) fn read(path: &Path) -> Option<Self> {
        let m = std::fs::metadata(path).ok()?;
        m.is_file().then_some(Self {
            modified: m.modified().ok()?,
            len: m.len(),
        })
    }
}

pub(crate) type TextureUpgrade = (PathBuf, Arc<TextureData>, Option<FileStamp>);

pub(crate) struct Reload {
    pub path: PathBuf,
    pub id: TextureId,
    pub stamp: FileStamp,
    pub data: Arc<TextureData>,
}

struct Watch {
    path: PathBuf,
    id: TextureId,
    shared: Weak<Shared>,
    max_dimension: u32,
}

struct Watched {
    watch: Watch,
    checked: Instant,
    candidate: Option<FileStamp>,
    stable_since: Instant,
    published: Option<FileStamp>,
    initialized: bool,
}

impl Watched {
    fn new(watch: Watch, now: Instant) -> Self {
        Self {
            watch,
            checked: now - POLL,
            candidate: None,
            stable_since: now,
            published: None,
            initialized: false,
        }
    }

    fn held(&self) -> bool {
        self.watch.shared.upgrade().is_some_and(|s| {
            s.lock()
                .get(&self.watch.path)
                .is_some_and(|e| e.0 == self.watch.id)
        })
    }

    fn due(&self, now: Instant) -> bool {
        now.duration_since(self.checked) >= POLL
    }

    fn poll(&mut self, now: Instant) -> Option<Reload> {
        if !self.due(now) {
            return None;
        }
        self.checked = now;
        let stamp = FileStamp::read(&self.watch.path);
        if !self.initialized {
            self.initialized = true;
            self.published = stamp;
            self.candidate = stamp;
            self.stable_since = now;
            return None;
        }
        if stamp != self.candidate {
            self.candidate = stamp;
            self.stable_since = now;
            return None;
        }
        let stamp = stamp?;
        if self.published == Some(stamp) || now.duration_since(self.stable_since) < SETTLE {
            return None;
        }
        // Read afresh: TextureCache may still contain the previous file's pixels.
        // The on-disk compression cache is content-addressed, so it is safe here.
        let Ok(data) = crate::threads::background_pool().install(|| {
            omsi_texture::gpu::load_gpu(&self.watch.path).map(|(data, _)| {
                omsi_render::fit_texture(&data, self.watch.max_dimension).unwrap_or(data)
            })
        }) else {
            return None;
        };
        if FileStamp::read(&self.watch.path) != Some(stamp) {
            self.candidate = None;
            return None;
        }
        self.published = Some(stamp);
        Some(Reload {
            path: self.watch.path.clone(),
            id: self.watch.id,
            stamp,
            data: Arc::new(data),
        })
    }
}

struct Worker {
    watch: mpsc::Sender<Watch>,
    ready: Mutex<mpsc::Receiver<Reload>>,
    versions: Arc<Mutex<HashMap<PathBuf, FileStamp>>>,
}

/// Shared by the world's freetex slots. Starts no thread until a slot loads a file.
/// Dropping the world closes the worker's channel; it is never joined on the frame.
#[derive(Default)]
pub struct FreeTextureReloads {
    worker: OnceLock<Option<Worker>>,
    // Sources of uploaded freetex images, not arbitrary files requested by a peer.
    snapshots: Mutex<HashMap<TextureId, Arc<TextureData>>>,
    paths: Mutex<HashMap<TextureId, PathBuf>>,
}

impl FreeTextureReloads {
    pub fn source_path(&self, id: TextureId) -> Option<PathBuf> {
        self.paths.lock().get(&id).cloned()
    }
    pub fn snapshot(&self, id: TextureId) -> Option<Arc<TextureData>> {
        self.snapshots.lock().get(&id).cloned()
    }

    pub fn remember(&self, id: TextureId, data: Arc<TextureData>) {
        let mut images = self.snapshots.lock();
        let used: usize = images.iter().filter(|(key, _)| **key != id).map(|(_, d)| d.cpu_bytes()).sum();
        if used.saturating_add(data.cpu_bytes()) <= 64 * 1024 * 1024 {
            images.insert(id, data);
        } else {
            images.remove(&id);
        }
    }

    pub(crate) fn forget(&self, id: TextureId) {
        self.snapshots.lock().remove(&id);
        self.paths.lock().remove(&id);
    }

    pub fn watch(&self, path: PathBuf, id: TextureId, shared: &Arc<Shared>, max_dimension: u32) {
        self.paths.lock().insert(id, path.clone());
        let Some(worker) = self.worker.get_or_init(|| {
            let (watch, commands) = mpsc::channel();
            // At most two decoded pictures wait for upload, plus the worker's current one.
            let (done, ready) = mpsc::sync_channel(2);
            let versions = Arc::new(Mutex::new(HashMap::new()));
            let seen = versions.clone();
            match std::thread::Builder::new()
                .name("freetex reload".into())
                .spawn(move || {
                    crate::threads::lower_thread_priority();
                    run(commands, done, seen);
                }) {
                Ok(_) => Some(Worker {
                    watch,
                    ready: Mutex::new(ready),
                    versions,
                }),
                Err(e) => {
                    log::warn!("Cannot watch free textures: {e}");
                    None
                }
            }
        }) else {
            return;
        };
        let _ = worker.watch.send(Watch {
            path,
            id,
            shared: Arc::downgrade(shared),
            max_dimension,
        });
    }

    pub(crate) fn next(&self) -> Option<Reload> {
        self.worker.get()?.as_ref()?.ready.lock().try_recv().ok()
    }

    /// A slow compression job must not put old pixels back after a file reload.
    /// This checks the worker's snapshot, with no filesystem calls on the frame.
    pub(crate) fn accepts(&self, path: &Path, stamp: Option<FileStamp>) -> bool {
        let Some(Some(worker)) = self.worker.get() else {
            return true;
        };
        worker
            .versions
            .lock()
            .get(path)
            .is_none_or(|seen| Some(*seen) == stamp)
    }

    pub(crate) fn tracks(&self, path: &Path) -> bool {
        let Some(Some(worker)) = self.worker.get() else {
            return false;
        };
        worker.versions.lock().contains_key(path)
    }
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;
    #[test]
    fn snapshots_follow_uploaded_pixels_and_keep_other_surfaces_separate() {
        let snapshots = FreeTextureReloads::default();
        let data = |color| Arc::new(TextureData::from_image(omsi_texture::Image::solid(color)));
        let first = data([1, 2, 3, 255]);
        let other = data([4, 5, 6, 255]);
        snapshots.remember(1, first.clone());
        snapshots.remember(2, other.clone());
        assert!(Arc::ptr_eq(&snapshots.snapshot(1).unwrap(), &first));
        let next = data([7, 8, 9, 255]);
        snapshots.remember(1, next.clone());
        assert!(Arc::ptr_eq(&snapshots.snapshot(1).unwrap(), &next));
        assert!(Arc::ptr_eq(&snapshots.snapshot(2).unwrap(), &other));
        snapshots.forget(1);
        assert!(snapshots.snapshot(1).is_none());
        assert!(snapshots.snapshot(2).is_some());
    }
}

fn run(
    commands: mpsc::Receiver<Watch>,
    done: mpsc::SyncSender<Reload>,
    versions: Arc<Mutex<HashMap<PathBuf, FileStamp>>>,
) {
    let mut watched: HashMap<PathBuf, Watched> = HashMap::new();
    let mut order = VecDeque::new();
    let mut pending = None;
    loop {
        // No busy loop, even with no pictures; commands wake an idle watcher promptly.
        let received = if watched.is_empty() {
            commands
                .recv()
                .map_err(|_| mpsc::RecvTimeoutError::Disconnected)
        } else {
            let now = Instant::now();
            let delay = if pending.is_some() {
                POLL
            } else {
                watched
                    .values()
                    .map(|file| (file.checked + POLL).saturating_duration_since(now))
                    .min()
                    .unwrap_or(POLL)
            };
            commands.recv_timeout(delay)
        };
        let command = match received {
            Ok(w) => Some(w),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        for watch in command.into_iter().chain(commands.try_iter()).take(64) {
            if let Some(old) = watched.get(&watch.path) {
                if old.watch.id == watch.id && Weak::ptr_eq(&old.watch.shared, &watch.shared) {
                    continue;
                }
            } else {
                order.push_back(watch.path.clone());
            }
            versions.lock().remove(&watch.path);
            watched.insert(watch.path.clone(), Watched::new(watch, Instant::now()));
        }
        if let Some(reload) = pending.take() {
            match done.try_send(reload) {
                Ok(()) => {}
                Err(mpsc::TrySendError::Full(reload)) => {
                    pending = Some(reload);
                    continue;
                }
                Err(mpsc::TrySendError::Disconnected(_)) => break,
            }
        }
        // Bound work per pass and rotate, so a large cache cannot monopolise a core.
        for _ in 0..order.len().min(32) {
            let path = order.pop_front().unwrap();
            let file = watched.get_mut(&path).unwrap();
            let now = Instant::now();
            if !file.due(now) {
                order.push_back(path);
                continue;
            }
            if !file.held() {
                watched.remove(&path);
                versions.lock().remove(&path);
                continue;
            }
            order.push_back(path.clone());
            if let Some(reload) = file.poll(now) {
                versions.lock().insert(path, reload.stamp);
                match done.try_send(reload) {
                    Ok(()) => {}
                    Err(mpsc::TrySendError::Full(reload)) => {
                        pending = Some(reload);
                        break;
                    }
                    Err(mpsc::TrySendError::Disconnected(_)) => return,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Picture {
        directory: PathBuf,
        path: PathBuf,
        shared: Arc<Shared>,
    }

    impl Picture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let directory = std::env::temp_dir().join(format!(
                "freetex-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&directory).unwrap();
            let path = directory.join("Connections_0.bmp");
            let shared = Arc::new(Mutex::new(HashMap::from([(path.clone(), (7, 1))])));
            Self {
                directory,
                path,
                shared,
            }
        }

        fn write(&self, colour: [u8; 3], revision: u64) {
            image::RgbImage::from_pixel(4, 4, image::Rgb(colour))
                .save(&self.path)
                .unwrap();
            // Do not depend on a filesystem's clock resolution or sleeps.
            std::fs::OpenOptions::new()
                .write(true)
                .open(&self.path)
                .unwrap()
                .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1000 + revision))
                .unwrap();
        }

        fn watched(&self, now: Instant) -> Watched {
            Watched::new(
                Watch {
                    path: self.path.clone(),
                    id: 7,
                    shared: Arc::downgrade(&self.shared),
                    max_dimension: 4096,
                },
                now,
            )
        }
    }

    impl Drop for Picture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }

    fn pixel(reload: &Reload) -> [u8; 4] {
        assert_eq!(reload.data.format, omsi_texture::PixelFormat::Rgba8);
        reload.data.levels[0][..4].try_into().unwrap()
    }

    #[test]
    fn initial_stamp_does_not_decode_or_publish_unchanged_files() {
        let picture = Picture::new();
        std::fs::write(&picture.path, b"not an image").unwrap();
        let now = Instant::now();
        let mut watched = picture.watched(now);
        for tick in 0..10 {
            assert!(watched.poll(now + POLL * tick).is_none());
        }
        assert_eq!(watched.published, FileStamp::read(&picture.path));
    }

    #[test]
    fn a_missing_initial_file_is_loaded_after_creation() {
        let picture = Picture::new();
        let now = Instant::now();
        let mut watched = picture.watched(now);
        assert!(watched.poll(now).is_none());
        picture.write([0, 255, 0], 1);
        assert!(watched.poll(now + POLL).is_none());
        assert_eq!(
            pixel(&watched.poll(now + POLL * 2).unwrap()),
            [0, 255, 0, 255]
        );
    }

    #[test]
    fn rewritten_inactive_page_is_fresh_when_returning_to_its_cached_id() {
        let picture = Picture::new();
        picture.write([255, 0, 0], 1);
        let now = Instant::now();
        let mut watched = picture.watched(now);
        assert!(watched.poll(now).is_none());
        assert!(watched.poll(now + POLL).is_none());
        // A second page is displayed; the old page is still held by the material cache.
        picture
            .shared
            .lock()
            .insert(picture.directory.join("Connections_1.bmp"), (8, 1));
        picture.write([0, 255, 0], 2); // same path and same file length
        assert!(watched.poll(now + POLL * 2).is_none());
        let updated = watched.poll(now + POLL * 3).unwrap();
        assert_eq!(updated.id, 7);
        assert_eq!(pixel(&updated), [0, 255, 0, 255]);
        assert!(
            watched.poll(now + POLL * 4).is_none(),
            "unchanged files are not decoded again"
        );
    }

    #[test]
    fn unfinished_write_keeps_the_last_good_picture_and_is_retried() {
        let picture = Picture::new();
        picture.write([255, 0, 0], 1);
        let now = Instant::now();
        let mut watched = picture.watched(now);
        watched.poll(now);
        assert!(watched.poll(now + POLL).is_none());
        let old = watched.published;
        std::fs::write(&picture.path, b"BM").unwrap();
        assert!(watched.poll(now + POLL * 2).is_none());
        assert!(watched.poll(now + POLL * 3).is_none());
        assert_eq!(watched.published, old);
        picture.write([0, 0, 255], 3);
        assert!(watched.poll(now + POLL * 4).is_none());
        assert_eq!(
            pixel(&watched.poll(now + POLL * 5).unwrap()),
            [0, 0, 255, 255]
        );
    }

    #[test]
    fn repeated_writes_coalesce_and_delete_then_recreate_recovers() {
        let picture = Picture::new();
        picture.write([255, 0, 0], 1);
        let now = Instant::now();
        let mut watched = picture.watched(now);
        watched.poll(now);
        assert!(watched.poll(now + POLL).is_none());
        picture.write([0, 255, 0], 2);
        assert!(watched.poll(now + POLL * 2).is_none());
        picture.write([0, 0, 255], 3);
        assert!(watched.poll(now + POLL * 3).is_none());
        assert_eq!(
            pixel(&watched.poll(now + POLL * 4).unwrap()),
            [0, 0, 255, 255]
        );
        std::fs::remove_file(&picture.path).unwrap();
        assert!(watched.poll(now + POLL * 5).is_none());
        assert!(watched.poll(now + POLL * 6).is_none());
        picture.write([255, 255, 0], 4);
        assert!(watched.poll(now + POLL * 7).is_none());
        assert_eq!(
            pixel(&watched.poll(now + POLL * 8).unwrap()),
            [255, 255, 0, 255]
        );
        picture.shared.lock().clear();
        assert!(!watched.held(), "unloaded files must stop being watched");
    }

    #[test]
    fn compression_from_an_older_file_cannot_overwrite_a_reload() {
        let picture = Picture::new();
        picture.write([255, 0, 0], 1);
        let old = FileStamp::read(&picture.path).unwrap();
        picture.write([0, 255, 0], 2);
        let new = FileStamp::read(&picture.path).unwrap();
        let reloads = FreeTextureReloads::default();
        let (watch, _) = mpsc::channel();
        let (_, ready) = mpsc::channel();
        let versions = Arc::new(Mutex::new(HashMap::from([(picture.path.clone(), new)])));
        assert!(reloads
            .worker
            .set(Some(Worker {
                watch,
                ready: Mutex::new(ready),
                versions
            }))
            .is_ok());
        assert!(!reloads.accepts(&picture.path, Some(old)));
        assert!(!reloads.accepts(&picture.path, None));
        assert!(reloads.accepts(&picture.path, Some(new)));
    }

    #[test]
    fn background_worker_watches_shared_files_once_and_releases_unloaded_paths() {
        let picture = Picture::new();
        picture.write([255, 0, 0], 1);
        let reloads = FreeTextureReloads::default();
        for _ in 0..3 {
            reloads.watch(picture.path.clone(), 7, &picture.shared, 4096);
        }
        let next = || {
            let until = Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(reload) = reloads.next() {
                    return reload;
                }
                assert!(Instant::now() < until, "background reload did not arrive");
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        std::thread::sleep(POLL * 3);
        assert!(
            reloads.next().is_none(),
            "initial pixels must not be loaded twice"
        );
        let old = FileStamp::read(&picture.path).unwrap();
        picture.write([0, 255, 0], 2);
        let updated = next();
        assert_eq!(pixel(&updated), [0, 255, 0, 255]);
        assert!(!reloads.accepts(&picture.path, Some(old)));
        assert!(reloads.accepts(&picture.path, Some(updated.stamp)));
        picture.shared.lock().clear();
        let until = Instant::now() + Duration::from_secs(5);
        while !reloads
            .worker
            .get()
            .unwrap()
            .as_ref()
            .unwrap()
            .versions
            .lock()
            .is_empty()
        {
            assert!(Instant::now() < until, "an unloaded path was retained");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
