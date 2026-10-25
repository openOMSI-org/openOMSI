//! Local content lookup only: peer names never become absolute paths or output files.
use super::{MAX_PIXELS_BYTES, MAX_SLOTS, pixels};
use omsi_net::scripttex::Image;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
    time::{Instant, SystemTime},
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Candidate {
    pub name: String,
    pub dirs: Vec<PathBuf>,
    pub root: PathBuf,
}

/// File SHA-256 is required, plus a digest of the displayed pixels. The latter prevents
/// a file being replaced before the reload worker uploads it from certifying old pixels.
pub(super) fn identity(path: &Path, image: &Image) -> Option<[u8; 32]> {
    let before = stamp(path)?;
    if before.0 > 64 * 1024 * 1024 {
        return None;
    }
    let mut file = std::fs::File::open(path).ok()?.take(64 * 1024 * 1024 + 1);
    let mut file_hash = Sha256::new();
    let mut buffer = [0u8; 32768];
    let mut bytes = 0;
    loop {
        let n = file.read(&mut buffer).ok()?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        if bytes > before.0 {
            return None;
        }
        file_hash.update(&buffer[..n]);
    }
    if bytes != before.0 || stamp(path)? != before {
        return None;
    }
    let mut hash = Sha256::new();
    hash.update(b"openOMSI freetex SHA256 v1\0");
    hash.update(file_hash.finalize());
    hash.update(image.width.to_le_bytes());
    hash.update(image.height.to_le_bytes());
    hash.update(image.rgba.as_slice());
    Some(hash.finalize().into())
}

fn stamp(path: &Path) -> Option<(u64, SystemTime)> {
    let meta = std::fs::metadata(path).ok()?;
    meta.is_file()
        .then_some((meta.len(), meta.modified().ok()?))
}

struct Entry {
    path: Option<PathBuf>,
    stamp: Option<(u64, SystemTime)>,
    image: Option<([u8; 32], Image)>,
    checked: Instant,
}

#[derive(Default)]
pub(super) struct Cache {
    entries: BTreeMap<Candidate, Entry>,
    cursor: usize,
}

impl Cache {
    pub fn update(&mut self, candidates: &[Candidate]) {
        self.entries.retain(|key, _| candidates.contains(key));
        // At most one local lookup/decode per iteration, never on the frame. Bound the
        // total candidate storage too; a large session cannot grow it without limit.
        let Some(candidate) = candidates.get(self.cursor % candidates.len().max(1)) else {
            return;
        };
        self.cursor = self.cursor.wrapping_add(1);
        if !self.entries.contains_key(candidate) && self.entries.len() >= MAX_SLOTS {
            return;
        }
        let now = Instant::now();
        if self
            .entries
            .get(candidate)
            .is_some_and(|e| now.duration_since(e.checked).as_millis() < 500)
        {
            return;
        }
        let path = resolve(candidate);
        let current = path.as_deref().and_then(stamp);
        if let Some(entry) = self.entries.get_mut(candidate) {
            if entry.path == path && entry.stamp == current {
                entry.checked = now;
                return;
            }
        }
        let used: usize = self
            .entries
            .iter()
            .filter(|(key, _)| *key != candidate)
            .filter_map(|(_, entry)| entry.image.as_ref())
            .map(|(_, image)| image.rgba.len())
            .sum();
        let image = path.as_deref().and_then(|path| {
            if current?.0 > 64 * 1024 * 1024 {
                return None;
            }
            let (data, _) = omsi_texture::gpu::load_gpu(path).ok()?;
            let bytes = u64::from(data.width) * u64::from(data.height) * 4;
            if bytes > MAX_PIXELS_BYTES as u64 || used as u64 + bytes > 64 * 1024 * 1024 {
                return None;
            }
            let image = pixels(&data)?;
            let key = identity(path, &image)?;
            (stamp(path) == current).then_some((key, image))
        });
        self.entries.insert(
            candidate.clone(),
            Entry {
                path,
                stamp: current,
                image,
                checked: now,
            },
        );
    }

    pub fn images(&self) -> BTreeMap<[u8; 32], Image> {
        self.entries
            .values()
            .filter_map(|entry| entry.image.clone())
            .collect()
    }

    pub fn contains(&self, image: &Image) -> bool {
        self.entries
            .values()
            .filter_map(|entry| entry.image.as_ref())
            .any(|(_, cached)| std::sync::Arc::ptr_eq(&cached.rgba, &image.rgba))
    }
}

fn resolve(candidate: &Candidate) -> Option<PathBuf> {
    if candidate.name.is_empty() || !omsi_net::remote_text::freetex_ok(&candidate.name) {
        return None;
    }
    let dirs: Vec<_> = candidate.dirs.iter().map(PathBuf::as_path).collect();
    let path = omsi_texture::find_texture(&candidate.name, &dirs).or_else(|| {
        let normalized = candidate.name.replace('\\', "/");
        let parts: Vec<_> = normalized.split('/').collect();
        let at = parts
            .iter()
            .position(|p| p.eq_ignore_ascii_case("Texture"))?;
        omsi_texture::find_texture(&parts.get(at + 1..)?.join("/"), &dirs)
    })?;
    let path = std::fs::canonicalize(path).ok()?;
    let roots = std::iter::once(candidate.root.clone()).chain(omsi_cfg::content_roots());
    roots
        .filter_map(|root| std::fs::canonicalize(root).ok())
        .any(|root| path.starts_with(root))
        .then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unused_local_file_can_satisfy_a_reference_and_changes_invalidate_it() {
        let id = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("omsi-freetex-cache-{}-{id}", std::process::id()));
        std::fs::create_dir_all(root.join("Texture")).unwrap();
        let path = root.join("Texture/display.png");
        image::RgbaImage::from_pixel(8, 4, image::Rgba([12u8, 34, 56, 255]))
            .save(&path)
            .unwrap();
        let candidate = Candidate {
            name: "display.png".into(),
            dirs: vec![root.join("Texture")],
            root: root.clone(),
        };
        let mut cache = Cache::default();
        cache.update(&[candidate.clone()]);
        let images = cache.images();
        assert_eq!(images.len(), 1);
        let (key, cached) = images.first_key_value().unwrap();
        assert_eq!(identity(&path, cached), Some(*key));
        assert_eq!(cached.width, 8);
        image::RgbaImage::from_pixel(9, 4, image::Rgba([90u8, 80, 70, 255]))
            .save(&path)
            .unwrap();
        cache.entries.get_mut(&candidate).unwrap().checked -= std::time::Duration::from_secs(1);
        cache.update(&[candidate.clone()]);
        assert!(!cache.images().contains_key(key));
        assert_eq!(cache.images().values().next().unwrap().width, 9);
        // Neither a remote absolute name nor relative traversal out of the content root
        // is a way to read a local file through the reference cache.
        assert!(
            resolve(&Candidate {
                name: path.display().to_string(),
                ..candidate.clone()
            })
            .is_none()
        );
        assert!(
            resolve(&Candidate {
                root: root.join("Texture/subdir"),
                ..candidate
            })
            .is_none()
        );
        cache.update(&[]);
        assert!(cache.images().is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }
}
