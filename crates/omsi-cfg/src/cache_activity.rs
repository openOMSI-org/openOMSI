//! Process-held cache leases, also acquired by games started from the command line.
use std::fs::{File, OpenOptions};
use std::path::Path;

fn open(base: &Path) -> std::io::Result<File> {
    std::fs::create_dir_all(base)?;
    OpenOptions::new().create(true).truncate(false).read(true).write(true).open(base.join("cache-use.lock"))
}

/// Keep this file alive for the entire game run. The OS releases it after a crash.
pub fn game(base: &Path) -> std::io::Result<File> {
    let file = open(base)?;
    file.lock_shared()?;
    Ok(file)
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
