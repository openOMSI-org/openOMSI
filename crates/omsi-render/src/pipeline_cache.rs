//! The driver's compiled pipelines kept between runs (`~/.openomsi/cache/pipelines-*.bin`).
//!
//! Vulkan and OpenGL (the fork's `PIPELINE_CACHE` for program binaries: ANGLE, phones) compile
//! every pipeline from the shader text at each start; with the cache the second start takes the
//! driver's binaries instead - seconds on a phone's GL driver. Metal and Direct3D 12 keep their
//! own caches and do not offer the feature. Data from another driver or GPU is refused by wgpu
//! (`fallback`), so a driver update only costs one slow start. OMSI_NO_PIPELINE_CACHE=1 leaves it
//! out.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

struct Slot {
    device: wgpu::Device,
    cache: wgpu::PipelineCache,
    path: PathBuf,
    /// When the cache was last written (or opened): see `get`.
    written: Instant,
    /// Digest of what the file holds, as read at `open` or as last written by this run. `save`
    /// writes when the driver's data differs from it - not when only its size does, which says
    /// nothing: a recompiled pipeline can change a blob and leave its length alone.
    on_disk: Option<u64>,
}

/// How often the cache is written while pipelines are being made (see `get`).
const CHECKPOINT: Duration = Duration::from_secs(2);

/// The cache of the device last opened (one game renderer at a time; a device opened again after
/// a fallback, or a second renderer, replaces it - the other one's pipelines are made without).
/// (Devices compare with their instance since the fork's oo/device-eq: two instances' devices
/// had the same ids, and a test's device took another's cache, "PipelineCache does not exist".)
static SLOT: Mutex<Option<Slot>> = Mutex::new(None);

/// Whether to ask the device for the feature.
pub(crate) fn wanted(adapter: &wgpu::Adapter) -> wgpu::Features {
    if omsi_cfg::flags::OMSI_NO_PIPELINE_CACHE.is_set() {
        return wgpu::Features::empty();
    }
    adapter.features() & wgpu::Features::PIPELINE_CACHE
}

fn path(info: &wgpu::AdapterInfo) -> Option<PathBuf> {
    Some(omsi_cfg::user_data::base_dir()?.join("cache").join(file_name(info.backend, info.vendor, info.device)))
}

// (one file per GPU: a laptop's two GPUs would throw each other's data away)
fn file_name(backend: wgpu::Backend, vendor: u32, device: u32) -> String {
    format!("pipelines-{}-{vendor:04x}-{device:04x}.bin", format!("{backend:?}").to_lowercase())
}

/// Open the cache of a device made with [`wanted`]'s feature, from last run's file.
pub(crate) fn open(device: &wgpu::Device, info: &wgpu::AdapterInfo) {
    let mut slot = SLOT.lock().unwrap_or_else(|e| e.into_inner());
    *slot = None;
    if !device.features().contains(wgpu::Features::PIPELINE_CACHE) {
        return;
    }
    let Some(path) = path(info) else { return };
    // (a cache is a few MB; anything far bigger is not ours)
    let data = std::fs::metadata(&path)
        .ok()
        .filter(|m| m.len() < 256 << 20)
        .and_then(|_| std::fs::read(&path).ok());
    // SAFETY: the data was written by `save` from `get_data` of a cache of this backend; wgpu
    // checks its header against the adapter and driver and starts empty on a mismatch
    // (`fallback`), and a file cut short fails the same check.
    let cache = unsafe {
        device.create_pipeline_cache(&wgpu::PipelineCacheDescriptor {
            label: Some("omsi pipelines"),
            data: data.as_deref(),
            fallback: true,
        })
    };
    log::info!("pipeline cache: {} ({})", path.display(), match &data { Some(d) => format!("{} KB from the last run", d.len() / 1024), None => "new".into() });
    remove_stale_temporaries(&path);
    let on_disk = data.as_deref().map(digest);
    *slot = Some(Slot { device: device.clone(), cache, path, written: Instant::now(), on_disk });
}

/// The cache to build `device`'s pipelines with, if it has one. While they are being made it
/// is also written to its file every couple of seconds: a phone's first start compiles for
/// half a minute and more, and when Android ended the app meanwhile (a start that seemed to
/// hang, #1708) nothing of it was kept - the cache was written only once every pipeline was
/// there, and each start began from nothing again.
pub(crate) fn get(device: &wgpu::Device) -> Option<wgpu::PipelineCache> {
    let mut slot = SLOT.lock().unwrap_or_else(|e| e.into_inner());
    let s = slot.as_mut().filter(|s| &s.device == device)?;
    let cache = s.cache.clone();
    if s.written.elapsed() >= CHECKPOINT {
        s.written = Instant::now();
        drop(slot);
        save();
    }
    Some(cache)
}

/// Write the cache to its file (after the pipelines were made, and at the end of the game, for
/// the ones made later: puddles, Enhanced+). Nothing happens without a cache.
pub fn save() {
    let mut slot = SLOT.lock().unwrap_or_else(|e| e.into_inner());
    let Some(s) = slot.as_mut() else { return };
    s.written = Instant::now();
    let Some(data) = s.cache.get_data() else { return };
    if data.is_empty() {
        return;
    }
    match write_if_changed(&s.path, &data, &mut s.on_disk) {
        Ok(true) => log::info!("pipeline cache saved: {} KB", data.len() / 1024),
        Ok(false) => {}
        Err(e) => log::warn!("pipeline cache not saved to {}: {e}", s.path.display()),
    }
}

fn digest(data: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    data.hash(&mut h);
    h.finish()
}

/// Put `data` in `path` unless the file already holds it (`on_disk`, the digest of what it held
/// when last seen by this run, and its length - a file deleted or cut meanwhile is written
/// again). Returns whether it was written.
fn write_if_changed(path: &Path, data: &[u8], on_disk: &mut Option<u64>) -> std::io::Result<bool> {
    let sum = digest(data);
    if *on_disk == Some(sum) && std::fs::metadata(path).is_ok_and(|m| m.len() == data.len() as u64) {
        return Ok(false);
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // written beside and renamed over: a game ended mid-write leaves the old file whole. The
    // temporary is this process's own: two games started together (or a second launcher
    // window) would otherwise write into the same file and rename a mixture of both.
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    let done = std::fs::write(&tmp, data).and_then(|_| std::fs::rename(&tmp, path));
    if done.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    done?;
    *on_disk = Some(sum);
    Ok(true)
}

/// A game killed while it wrote (Android does that) leaves its temporary file, which no later
/// run - with another process id - would ever overwrite. Those a while old are removed; a
/// younger one may be another game's, writing right now.
fn remove_stale_temporaries(path: &Path) {
    const OLD: Duration = Duration::from_secs(3600);
    let (Some(dir), Some(stem)) = (path.parent(), path.file_stem().and_then(|s| s.to_str())) else { return };
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        // (the dot after the stem: a device id has no fixed width, "…-4305" begins "…-43050a01")
        if !(name.strip_prefix(stem).is_some_and(|rest| rest.starts_with('.')) && name.ends_with(".tmp")) {
            continue;
        }
        let old = entry.metadata().and_then(|m| m.modified()).is_ok_and(|t| SystemTime::now().duration_since(t).is_ok_and(|age| age >= OLD));
        if old {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Forget `device`'s cache (the device is lost or failed to make the renderer).
pub(crate) fn close(device: &wgpu::Device) {
    let mut slot = SLOT.lock().unwrap_or_else(|e| e.into_inner());
    if slot.as_ref().is_some_and(|s| &s.device == device) {
        *slot = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of the test's own under the system's temporary one, removed at its end.
    struct Dir(PathBuf);
    impl Dir {
        fn new(name: &str) -> Dir {
            let d = std::env::temp_dir().join(format!("omsi-pipeline-cache-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).unwrap();
            Dir(d)
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn temporaries(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.ends_with(".tmp")).collect();
        v.sort();
        v
    }

    #[test]
    fn a_blob_of_the_same_size_but_other_content_is_written() {
        let d = Dir::new("same-size");
        let file = d.0.join("pipelines-gl-0001-0002.bin");
        let (a, b) = (vec![1u8; 4096], vec![2u8; 4096]);
        let mut on_disk = None;
        assert!(write_if_changed(&file, &a, &mut on_disk).unwrap());
        assert_eq!(std::fs::read(&file).unwrap(), a);
        assert!(write_if_changed(&file, &b, &mut on_disk).unwrap(), "same length, other bytes: must be written");
        assert_eq!(std::fs::read(&file).unwrap(), b);
        assert!(temporaries(&d.0).is_empty());
    }

    #[test]
    fn the_same_content_is_not_written_again() {
        let d = Dir::new("unchanged");
        let file = d.0.join("pipelines-gl-0001-0002.bin");
        let data = vec![7u8; 1000];
        let mut on_disk = None;
        assert!(write_if_changed(&file, &data, &mut on_disk).unwrap());
        assert!(!write_if_changed(&file, &data, &mut on_disk).unwrap());
        // what `open` read from the last run counts as already on disk
        let mut from_open = Some(digest(&data));
        assert!(!write_if_changed(&file, &data, &mut from_open).unwrap());
    }

    #[test]
    fn a_file_deleted_or_cut_is_written_again() {
        let d = Dir::new("deleted");
        let file = d.0.join("pipelines-gl-0001-0002.bin");
        let data = vec![9u8; 1000];
        let mut on_disk = None;
        write_if_changed(&file, &data, &mut on_disk).unwrap();
        std::fs::remove_file(&file).unwrap();
        assert!(write_if_changed(&file, &data, &mut on_disk).unwrap());
        std::fs::write(&file, &data[..10]).unwrap();
        assert!(write_if_changed(&file, &data, &mut on_disk).unwrap());
        assert_eq!(std::fs::read(&file).unwrap(), data);
    }

    #[test]
    fn a_failed_write_keeps_the_old_file_and_leaves_no_temporary() {
        let d = Dir::new("failed");
        let file = d.0.join("pipelines-gl-0001-0002.bin");
        let old = vec![1u8; 100];
        let mut on_disk = None;
        write_if_changed(&file, &old, &mut on_disk).unwrap();
        let before = on_disk;
        let tmp = file.with_extension(format!("{}.tmp", std::process::id()));
        std::fs::create_dir_all(tmp.join("inside")).unwrap();
        assert!(write_if_changed(&file, &[5u8; 100], &mut on_disk).is_err());
        std::fs::remove_dir_all(&tmp).unwrap();
        assert!(temporaries(&d.0).is_empty());
        assert_eq!(std::fs::read(&file).unwrap(), old);
        assert_eq!(before, on_disk);
    }

    #[test]
    fn the_temporary_belongs_to_the_process() {
        let d = Dir::new("pid");
        let file = d.0.join("pipelines-gl-0001-0002.bin");
        std::fs::create_dir_all(file.with_extension("tmp").join("reserved")).unwrap();
        // a temporary of another process is left alone by a write
        let other = d.0.join("pipelines-gl-0001-0002.1.tmp");
        std::fs::write(&other, b"someone else's").unwrap();
        write_if_changed(&file, &[3u8; 10], &mut None).unwrap();
        assert_eq!(std::fs::read(&other).unwrap(), b"someone else's");
    }

    #[test]
    fn only_old_temporaries_of_the_cache_are_removed() {
        let d = Dir::new("stale");
        let file = d.0.join("pipelines-gl-0001-0002.bin");
        let old_time = SystemTime::now() - Duration::from_secs(2 * 3600);
        let make = |name: &str, old: bool| {
            let f = std::fs::File::create(d.0.join(name)).unwrap();
            if old {
                f.set_modified(old_time).unwrap();
            }
        };
        make("pipelines-gl-0001-0002.4242.tmp", true); // dead process
        make("pipelines-gl-0001-0002.tmp", true); // the name this file used to be written with
        make("pipelines-gl-0001-0002.77.tmp", false); // maybe a game writing right now
        make("pipelines-vulkan-0001-0002.5.tmp", true); // another backend's cache
        make("pipelines-gl-0001-00020a01.5.tmp", true); // another GPU whose id begins like this one's
        make("pipelines-gl-0001-0002.bin", true); // the cache itself
        remove_stale_temporaries(&file);
        assert_eq!(temporaries(&d.0), ["pipelines-gl-0001-0002.77.tmp", "pipelines-gl-0001-00020a01.5.tmp", "pipelines-vulkan-0001-0002.5.tmp"]);
        assert!(file.exists());
    }

    #[test]
    fn file_names_backend_and_gpu() {
        assert_eq!(super::file_name(wgpu::Backend::Gl, 0x5143, 0x43050a01), "pipelines-gl-5143-43050a01.bin");
        assert_eq!(super::file_name(wgpu::Backend::Vulkan, 0x10de, 0x2684), "pipelines-vulkan-10de-2684.bin");
    }
}
