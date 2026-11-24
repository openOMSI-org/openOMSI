//! Files of a bus this machine has itself, in the same version: taken from here, not sent.

use super::*;

pub(in crate::mods) fn adopt_local(m: &VehicleManifest) -> usize {
    buses().adopt_local(m)
}

/// Hashes of this machine's own files, by path: (size, modification time, SHA-256).
pub(super) type LocalShas = HashMap<PathBuf, (u64, u64, String)>;

impl Buses {
    /// The files of `m` not kept yet that this machine has itself in the same version (in its
    /// own content or the installation, at the same path) are kept from there: nothing to
    /// send. How many.
    ///
    /// A list comes from any player, and names what it likes: one offer at a time compares (a
    /// list of thousands of the server's own paths, sent over a hundred connections at once,
    /// had the server read and hash its whole installation over and over), each file by its
    /// size first and by its hash remembered (`local_sha`) - only a file that is the same is
    /// read.
    pub(super) fn adopt_local(&self, m: &VehicleManifest) -> usize {
        let _one = self.adopting.lock().unwrap_or_else(|e| e.into_inner());
        let mut n = 0;
        for k in self.missing(m) {
            let e = &m.entries[k];
            if let Some(d) = self.own_copy(e) {
                if self.put_sent(e, &d).is_ok() {
                    n += 1;
                }
            }
        }
        n
    }

    /// This machine's own file at `e`'s path (in its content or the installation, never a
    /// session's), if it is that file: read only once its size and hash say so.
    pub(super) fn own_copy(&self, e: &Entry) -> Option<Vec<u8>> {
        let (_, p) = omsi_cfg::find_in_roots(&e.path).filter(|(r, _)| !omsi_cfg::is_sandbox(r))?;
        if !self
            .local_sha(&p, e.size)
            .is_some_and(|h| h.eq_ignore_ascii_case(&e.sha256))
        {
            return None;
        }
        omsi_cfg::vfs::read(&p).ok().filter(|d| {
            d.len() as u64 == e.size && crate::mods::sha256_of(d).eq_ignore_ascii_case(&e.sha256)
        })
    }

    /// The SHA-256 of the local file `p` if it has `size` bytes: nothing is read for another
    /// size, and a plain file's hash is remembered by its size and time.
    pub(super) fn local_sha(&self, p: &Path, size: u64) -> Option<String> {
        let Ok(md) = std::fs::metadata(p) else {
            // (a file in a mounted archive: its size, then its bytes)
            if omsi_cfg::vfs::file_size(p) != Some(size) {
                return None;
            }
            return omsi_cfg::vfs::read(p)
                .ok()
                .filter(|d| d.len() as u64 == size)
                .map(|d| crate::mods::sha256_of(&d));
        };
        if !md.is_file() || md.len() != size {
            return None;
        }
        let mtime = md
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let shas = || self.local_shas.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, _, h)) = shas().get(p).filter(|(s, t, _)| *s == size && *t == mtime) {
            return Some(h.clone());
        }
        let h = crate::mods::sha256_of(&std::fs::read(p).ok()?);
        shas().insert(p.to_path_buf(), (size, mtime, h.clone()));
        Some(h)
    }
}
