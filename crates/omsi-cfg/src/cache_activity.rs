//! Process-held cache leases, also acquired by games started from the command line.
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

/// A game's shared lease. The last game's orderly exit reclaims eligible cache
/// data, including when the game was started without the launcher.
pub struct Game {
    file: Option<File>,
    base: PathBuf,
    cleanup_allowed: fn() -> bool,
    limits: (u64, u64),
}

impl Game {
    /// Let the application also protect older registered games that do not yet
    /// hold a shared lease. Current games are always protected by the OS lock.
    pub fn only_cleanup_when(mut self, allowed: fn() -> bool) -> Self {
        self.cleanup_allowed = allowed;
        self
    }
}

impl Drop for Game {
    fn drop(&mut self) {
        // Close our shared handle before asking for the exclusive lease. Another
        // game (or a newly starting game) still prevents maintenance atomically.
        self.file.take();
        if cfg!(any(target_os = "android", target_os = "ios")) || !(self.cleanup_allowed)() {
            return;
        }
        let Some(_maintenance) = maintenance(&self.base) else {
            return;
        };
        use crate::cache_budget::{self, Kind};
        cache_budget::trim(&self.base.join("cache"), Kind::Local, self.limits.0);
        cache_budget::trim(&self.base.join("lan-store"), Kind::ClientLan, self.limits.1);
    }
}

fn open(base: &Path) -> std::io::Result<File> {
    std::fs::create_dir_all(base)?;
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(base.join("cache-use.lock"))
}

/// Keep this lease alive for the entire game run. The OS releases the lock after a crash.
pub fn game(base: &Path) -> std::io::Result<Game> {
    let file = open(base)?;
    file.lock_shared()?;
    Ok(Game {
        file: Some(file),
        base: base.to_path_buf(),
        cleanup_allowed: || true,
        limits: (
            crate::cache_budget::LOCAL_LIMIT,
            crate::cache_budget::LAN_LIMIT,
        ),
    })
}

/// Exclusive maintenance lease: the game waits until cleanup finishes; cleanup
/// never starts while any game holds a shared lease. Failure means do no cleanup.
pub fn maintenance(base: &Path) -> Option<File> {
    let file = open(base).ok()?;
    file.try_lock().ok()?;
    Some(file)
}

#[cfg(test)]
mod tests {
    // Exercise real lease/drop behavior without multi-gigabyte temporary files,
    // which are not necessarily sparse on every CI filesystem.
    fn game(base: &std::path::Path) -> super::Game {
        let mut lease = super::game(base).unwrap();
        lease.limits = (8, 8);
        lease
    }

    #[test]
    fn an_application_can_defer_cleanup_for_an_older_game() {
        let base = std::env::temp_dir().join(format!("omsi-cache-legacy-{}", std::process::id()));
        std::fs::create_dir_all(base.join("cache")).unwrap();
        let file = base.join("cache/old.tex");
        std::fs::File::create(&file).unwrap().set_len(9).unwrap();
        let lease = game(&base).only_cleanup_when(|| false);
        drop(lease);
        assert!(file.exists(), "an older game can still be using this cache");
        assert!(
            super::maintenance(&base).is_some(),
            "the shared lease was released"
        );
        drop(game(&base));
        assert!(
            !file.exists(),
            "the cache is reclaimed once the older game has ended"
        );
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn only_the_last_game_reclaims_over_budget_data() {
        let base =
            std::env::temp_dir().join(format!("omsi-cache-last-game-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        for (dir, name, size) in [("cache", "old.tex", 9), ("lan-store", "old.blob", 9)] {
            std::fs::create_dir_all(base.join(dir)).unwrap();
            std::fs::File::create(base.join(dir).join(name))
                .unwrap()
                .set_len(size)
                .unwrap();
        }
        let first = game(&base);
        let second = game(&base);
        drop(first);
        assert!(base.join("cache/old.tex").exists());
        assert!(base.join("lan-store/old.blob").exists());
        drop(second);
        assert!(!base.join("cache/old.tex").exists());
        assert!(!base.join("lan-store/old.blob").exists());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn shared_games_exclude_cleanup_and_release_on_drop() {
        let base = std::env::temp_dir().join(format!("omsi-cache-lease-{}", std::process::id()));
        let first = super::game(&base).unwrap();
        let second = super::game(&base).unwrap();
        assert!(super::maintenance(&base).is_none());
        drop(first);
        assert!(super::maintenance(&base).is_none());
        drop(second);
        assert!(super::maintenance(&base).is_some());
        std::fs::remove_dir_all(base).unwrap();
    }
}
