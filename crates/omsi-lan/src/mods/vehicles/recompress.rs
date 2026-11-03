//! Kept files packed at their smallest in the background, for the players who fetch them.

use super::*;

pub(in crate::mods) fn packed(sha: &str, i: u64) -> Option<(u64, char, Vec<u8>)> {
    buses().packed(sha, i)
}

/// Piece `i` of the kept file `sha` as it was just packed on request (at the speed of the
/// moment): kept, so that the next player who asks for it does not pack it again - with a
/// whole session fetching the same bus, every piece was packed once per player. Packing it at
/// its smallest (`recompress`) replaces it.
pub(in crate::mods) fn keep_packed(sha: &str, i: u64, codec: char, wire: &[u8]) {
    buses().keep_packed(sha, i, codec, wire)
}

pub(in crate::mods) fn recompress_later(sha: &str) {
    buses().recompress_later(sha)
}

/// The files to pack at their smallest, one after the other, off the game's way.
#[derive(Default)]
pub(super) struct Recompressing {
    pub(super) queue: VecDeque<String>,
    pub(super) queued: std::collections::BTreeSet<String>,
    pub(super) running: bool,
}

impl Buses {
    /// Piece `i` of the kept file `sha` packed at its smallest (`recompress`), if it is:
    /// (its raw size, codec, bytes on the wire).
    pub(super) fn packed(&self, sha: &str, i: u64) -> Option<(u64, char, Vec<u8>)> {
        let sha = sha.to_ascii_lowercase();
        let g = self.lock();
        let v = g.as_ref()?;
        let size = *v.sizes.get(&sha)?;
        if i >= pieces::count(size) {
            return None;
        }
        let (codec, wire) = v.store.packed(&sha, i)?;
        Some((pieces::span(size, i).1, codec, wire))
    }

    pub(super) fn keep_packed(&self, sha: &str, i: u64, codec: char, wire: &[u8]) {
        let sha = sha.to_ascii_lowercase();
        let g = self.lock();
        let Some(v) = g.as_ref().filter(|v| v.sizes.get(&sha).is_some_and(|size| i < pieces::count(*size))) else { return };
        // (one that does not get smaller is kept as a mark: it goes as it is)
        let kept = if codec == '-' { v.store.put_packed(&sha, i, '-', &[]) } else { v.store.put_packed(&sha, i, codec, wire) };
        if let Err(e) = kept {
            log::warn!("vehicles: a packed piece not kept: {e}");
        }
    }

    fn recompressing(&self) -> std::sync::MutexGuard<'_, Recompressing> {
        self.recompressing.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A kept file was asked for piece by piece before it was packed at its smallest: it is,
    /// in the background (one file at a time, on one thread) - the players who fetch it after
    /// that get it smaller (`packed`), the bytes it unpacks to the same.
    pub(super) fn recompress_later(&'static self, sha: &str) {
        let mut r = self.recompressing();
        let sha = sha.to_ascii_lowercase();
        if r.queued.insert(sha.clone()) {
            r.queue.push_back(sha);
        }
        // (the tests pack what is queued themselves: `recompress_queued`)
        if r.running || cfg!(test) {
            return;
        }
        r.running = true;
        let spawned = std::thread::Builder::new().name("recompress".into()).spawn(move || {
            self.recompress_queued();
            self.recompressing().running = false;
        });
        if spawned.is_err() {
            r.running = false;
        }
    }

    /// Pack every file queued (`recompress_later`), until none is left.
    pub(super) fn recompress_queued(&self) {
        let (mut files, mut raw, mut wire) = (0, 0u64, 0u64);
        let t0 = std::time::Instant::now();
        loop {
            let next = {
                let mut r = self.recompressing();
                let next = r.queue.pop_front();
                if let Some(s) = &next {
                    r.queued.remove(s);
                }
                next
            };
            let Some(sha) = next else { break };
            if let Some((r, w)) = self.recompress(&sha) {
                files += 1;
                raw += r;
                wire += w;
            }
        }
        if files > 0 {
            log::info!("vehicles: {files} files packed at their smallest - {:.1} MB go as {:.1} MB ({:.0} s)", raw as f64 / 1e6, wire as f64 / 1e6, t0.elapsed().as_secs_f64());
        }
    }

    /// Every piece of the kept file `sha` packed at its smallest (`pieces::ZSTD_BEST`) and
    /// kept so: (its bytes, what they take on the wire). None when it is not kept (any more).
    pub(super) fn recompress(&self, sha: &str) -> Option<(u64, u64)> {
        let data = self.blob(sha)?;
        let size = data.len() as u64;
        let mut wire = 0u64;
        for i in 0..pieces::count(size) {
            let (start, len) = pieces::span(size, i);
            let piece = &data[start as usize..(start + len) as usize];
            // (the slowest is not the smallest every time: the smaller of the two is kept)
            let (codec, packed) = [pieces::ZSTD_BEST, pieces::ZSTD_FAST].map(|level| pieces::pack_zstd(piece, level)).into_iter().min_by_key(|(_, p)| p.len()).unwrap_or(('-', Vec::new()));
            wire += packed.len() as u64;
            // (kept with the store held: a file let go meanwhile leaves no packed pieces behind)
            let g = self.lock();
            let v = g.as_ref()?;
            if !v.sizes.contains_key(sha) {
                v.store.remove_packed(sha, i);
                return None;
            }
            // (one that does not get smaller is kept as a mark: it goes as it is)
            let kept = if codec == '-' { v.store.put_packed(sha, i, '-', &[]) } else { v.store.put_packed(sha, i, codec, &packed) };
            if let Err(e) = kept {
                log::warn!("vehicles: a packed piece not kept: {e}");
                return None;
            }
        }
        Some((size, wire))
    }
}
