//! A bounded system-RAM second level for static textures prepared for the GPU.
//!
//! This cache is deliberately separate from the VRAM budget. An already-compressed
//! image can be reused after its GPU mip levels were reduced, without decoding the
//! original file or recompressing it for each restore. GPU uploads remain bounded by
//! the normal video-memory policy.

use omsi_texture::gpu::{GpuOptions, PixelFormat, TextureData, gpu_options};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::SystemTime;

const DEFAULT_MB: u64 = if cfg!(any(target_os = "android", target_os = "ios")) {
    0
} else {
    128
};
const MAX_MB: u64 = 512;
const MAX_ENTRY_BYTES: u64 = 16 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SourceStamp {
    size: u64,
    modified: SystemTime,
    options: GpuOptions,
    generation: u64,
}

/// A changed source or GPU compression mode must never reuse a stale upload.
pub(super) fn source_stamp(path: &Path) -> Option<SourceStamp> {
    if path.to_string_lossy().ends_with("#bump") || omsi_cfg::vfs::is_sealed(path) {
        return None;
    }
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    Some(SourceStamp {
        size: metadata.len(),
        modified: metadata.modified().ok()?,
        options: gpu_options(),
        generation: omsi_cfg::content_generation(),
    })
}

struct Item {
    data: Arc<TextureData>,
    stamp: SourceStamp,
    bytes: u64,
    touched: u64,
}

#[derive(Default)]
struct State {
    items: HashMap<PathBuf, Item>,
    used: u64,
    tick: u64,
    hits: u64,
    misses: u64,
    evictions: u64,
}

pub(super) struct TextureRamCache {
    cap: u64,
    state: Mutex<State>,
}

/// Shared by texture loaders across maps, bounded and source-validated on each hit.
pub(super) fn global() -> &'static TextureRamCache {
    static CACHE: OnceLock<TextureRamCache> = OnceLock::new();
    CACHE.get_or_init(TextureRamCache::default)
}

impl Default for TextureRamCache {
    fn default() -> Self {
        let mb = omsi_cfg::flags::OMSI_TEXTURE_RAM_CACHE_MB
            .parse::<u64>()
            .unwrap_or(DEFAULT_MB)
            .min(MAX_MB);
        Self::with_capacity(mb << 20)
    }
}

impl TextureRamCache {
    pub(super) fn with_capacity(cap: u64) -> Self {
        Self {
            cap,
            state: Mutex::new(State::default()),
        }
    }

    pub(super) fn get(&self, path: &Path) -> Option<Arc<TextureData>> {
        if self.cap == 0 {
            return None;
        }
        let stamp = source_stamp(path);
        let mut state = self.state.lock();
        if state
            .items
            .get(path)
            .is_some_and(|item| Some(item.stamp) != stamp)
        {
            if let Some(old) = state.items.remove(path) {
                state.used = state.used.saturating_sub(old.bytes);
            }
        }
        state.tick = state.tick.saturating_add(1);
        let tick = state.tick;
        if let Some(item) = state.items.get_mut(path) {
            item.touched = tick;
            let data = item.data.clone();
            state.hits += 1;
            return Some(data);
        }
        state.misses += 1;
        None
    }

    /// Admission happens off the render thread. Never cache a live script texture,
    /// an uncompressed RGBA image, a modified source or a single huge texture.
    pub(super) fn insert(&self, path: &Path, data: Arc<TextureData>, before: SourceStamp) {
        if self.cap == 0 || data.format == PixelFormat::Rgba8 {
            return;
        }
        let bytes = data.cpu_bytes() as u64;
        if bytes == 0 || bytes > self.cap / 4 || bytes > MAX_ENTRY_BYTES {
            return;
        }
        if source_stamp(path) != Some(before) {
            return;
        }
        let mut state = self.state.lock();
        if let Some(old) = state.items.remove(path) {
            state.used = state.used.saturating_sub(old.bytes);
        }
        while state.used.saturating_add(bytes) > self.cap {
            let oldest = state
                .items
                .iter()
                .min_by_key(|(_, item)| item.touched)
                .map(|(p, _)| p.clone());
            let Some(oldest) = oldest else {
                return;
            };
            if let Some(old) = state.items.remove(&oldest) {
                state.used = state.used.saturating_sub(old.bytes);
                state.evictions += 1;
            }
        }
        state.tick = state.tick.saturating_add(1);
        let tick = state.tick;
        state.items.insert(
            path.to_path_buf(),
            Item {
                data,
                stamp: before,
                bytes,
                touched: tick,
            },
        );
        state.used += bytes;
    }

    /// Perform a compressed source load, using RAM on subsequent requests.
    /// The existing load path is the fallback and stays the only source of truth.
    pub(super) fn prepare_with(
        &self,
        path: &Path,
        load: impl FnOnce() -> Option<TextureData>,
    ) -> Option<TextureData> {
        if self.cap == 0 {
            return load();
        }
        let stamp = source_stamp(path);
        if let Some(data) = self.get(path) {
            return Some(data.as_ref().clone());
        }
        let data = load()?;
        if data.format.is_compressed() {
            if let Some(stamp) = stamp {
                self.insert(path, Arc::new(data.clone()), stamp);
            }
        }
        Some(data)
    }

    #[cfg(test)]
    pub(super) fn usage(&self) -> (u64, usize, u64, u64) {
        let state = self.state.lock();
        (state.used, state.items.len(), state.hits, state.misses)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Source(PathBuf);

    impl Source {
        fn new() -> Self {
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let p = std::env::temp_dir().join(format!(
                "openomsi-ram-tex-{}-{}",
                std::process::id(),
                id
            ));
            std::fs::write(&p, b"original texture").unwrap();
            Self(p)
        }

        fn stamp(&self) -> SourceStamp {
            source_stamp(&self.0).unwrap()
        }
    }

    impl Drop for Source {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn compressed(bytes: usize) -> Arc<TextureData> {
        Arc::new(TextureData {
            width: 4,
            height: 4,
            format: PixelFormat::Bc3,
            levels: vec![vec![1u8; bytes]],
            has_alpha: true,
            gpu_mips: false,
        })
    }

    #[test]
    fn lru_evicts_least_recent_source_under_a_hard_cap() {
        let a = Source::new();
        let b = Source::new();
        let c = Source::new();
        let d = Source::new();
        let e = Source::new();
        let cache = TextureRamCache::with_capacity(160);
        for source in [&a, &b, &c, &d] {
            cache.insert(&source.0, compressed(40), source.stamp());
        }
        assert_eq!(cache.usage().0, 160);
        assert!(cache.get(&a.0).is_some());
        cache.insert(&e.0, compressed(40), e.stamp());
        assert!(cache.get(&a.0).is_some());
        assert!(
            cache.get(&b.0).is_none(),
            "least recently accessed entry should go"
        );
        assert!(cache.get(&c.0).is_some());
        assert!(cache.get(&d.0).is_some());
        assert!(cache.get(&e.0).is_some());
        assert_eq!(cache.usage().0, 160);
    }

    #[test]
    fn disabled_oversized_and_uncompressed_entries_are_never_retained() {
        let source = Source::new();
        let disabled = TextureRamCache::with_capacity(0);
        disabled.insert(&source.0, compressed(16), source.stamp());
        assert!(disabled.get(&source.0).is_none());
        let cache = TextureRamCache::with_capacity(128);
        cache.insert(&source.0, compressed(40), source.stamp());
        assert_eq!(
            cache.usage().0,
            0,
            "one entry must fit inside a quarter of the budget"
        );
        cache.insert(&source.0, compressed(16), source.stamp());
        assert_eq!(cache.usage().0, 16);
        cache.insert(
            &source.0,
            Arc::new(TextureData {
                width: 4,
                height: 4,
                format: PixelFormat::Rgba8,
                levels: vec![vec![0; 64]],
                has_alpha: false,
                gpu_mips: false,
            }),
            source.stamp(),
        );
        assert_eq!(
            cache.usage().0,
            16,
            "RGBA must not replace a compressed entry"
        );
    }

    #[test]
    fn changed_or_missing_files_cannot_reuse_old_texture_bytes() {
        let source = Source::new();
        let old = source.stamp();
        let cache = TextureRamCache::with_capacity(128);
        cache.insert(&source.0, compressed(16), old);
        std::fs::write(&source.0, b"changed content and length").unwrap();
        assert!(cache.get(&source.0).is_none());
        cache.insert(&source.0, compressed(16), old);
        assert_eq!(cache.usage().0, 0);
        cache.insert(&source.0, compressed(16), source.stamp());
        std::fs::remove_file(&source.0).unwrap();
        assert!(cache.get(&source.0).is_none());
        assert_eq!(cache.usage().0, 0);
    }

    #[test]
    fn preparing_an_already_cached_source_skips_expensive_work() {
        let source = Source::new();
        let cache = TextureRamCache::with_capacity(128);
        let original = compressed(16);
        cache.insert(&source.0, original, source.stamp());
        let source_data = cache
            .prepare_with(&source.0, || {
                panic!("a RAM cache hit must not decode the source again");
            })
            .unwrap();
        assert_eq!(source_data.cpu_bytes(), 16);
        assert_eq!(cache.usage().2, 1);
    }

    #[test]
    fn cache_returns_the_same_compressed_allocation_for_a_restore() {
        let source = Source::new();
        let cache = TextureRamCache::with_capacity(128);
        let source_data = compressed(16);
        cache.insert(&source.0, source_data.clone(), source.stamp());
        let restored = cache.get(&source.0).unwrap();
        assert!(Arc::ptr_eq(&restored, &source_data));
        assert_eq!(cache.usage().2, 1);
    }
}
