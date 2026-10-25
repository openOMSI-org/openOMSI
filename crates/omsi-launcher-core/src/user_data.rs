//! Changing the desktop user-data location without merging or discarding existing files.
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ChangeMode { Move, Fresh, UseExisting, Overwrite }

pub fn selected_path(path: &Path) -> PathBuf {
    if path.file_name().is_some_and(|n| n == ".openomsi") { path.to_path_buf() } else { path.join(".openomsi") }
}

/// Copy first (also works across disks); remove the source only after pointer files commit.
fn copy_tree_contents(source: &Path, target: &Path) -> Result<()> {
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let to = target.join(entry.file_name());
        if kind.is_symlink() {
            bail!(crate::i18n::trf("Cannot move a data folder containing symbolic links: {path}", &[("path", &entry.path().display())]));
        } else if kind.is_dir() {
            std::fs::create_dir(&to)?;
            copy_tree_contents(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}

fn validate_destination(source: &Path, target: &Path) -> Result<()> {
    omsi_cfg::user_data::validate_path(target)?;
    if source == target { return Ok(()); }
    // Canonicalize the existing parent as well, so symlinked parents cannot nest a copy.
    let mut ancestor = target.parent().context(crate::i18n::tr("The data folder needs a parent"))?;
    while !ancestor.exists() {
        ancestor = ancestor.parent().context(crate::i18n::tr("The data folder needs an existing ancestor"))?;
    }
    let canonical_source = source.canonicalize()?;
    if target.exists() {
        let canonical_target = target.canonicalize()?;
        if canonical_target.starts_with(&canonical_source) || canonical_source.starts_with(&canonical_target) {
            bail!(crate::i18n::tr("The new .openomsi folder cannot contain or be inside the current folder"));
        }
    }
    if ancestor.canonicalize()?.starts_with(&canonical_source) || canonical_source.starts_with(target) {
        bail!(crate::i18n::tr("The new .openomsi folder cannot contain or be inside the current folder"));
    }
    Ok(())
}

fn prepare(source: &Path, target: &Path, move_current: bool) -> Result<()> {
    validate_destination(source, target)?;
    if target.exists() {
        bail!(crate::i18n::tr("The destination already exists. Choose another location to avoid overwriting its data"));
    }
    std::fs::create_dir_all(target.parent().unwrap())?;
    // Only clean up a destination that this operation successfully claimed.
    std::fs::create_dir(target)?;
    if move_current {
        if let Err(e) = copy_tree_contents(source, target) {
            let _ = std::fs::remove_dir_all(target);
            return Err(e).context(crate::i18n::tr("Could not copy the current .openomsi folder; the original is unchanged"));
        }
    }
    Ok(())
}

/// Replace only after the full copy succeeds, retaining the destination for rollback.
fn prepare_overwrite(source: &Path, target: &Path) -> Result<PathBuf> {
    validate_destination(source, target)?;
    if !target.is_dir() || std::fs::symlink_metadata(target)?.file_type().is_symlink() {
        bail!(crate::i18n::tr("The existing .openomsi destination must be a real directory"));
    }
    let unique = format!("{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos());
    let stage = target.with_file_name(format!(".openomsi-stage-{unique}"));
    let backup = target.with_file_name(format!(".openomsi-backup-{unique}"));
    std::fs::create_dir(&stage)?;
    if let Err(e) = copy_tree_contents(source, &stage) {
        let _ = std::fs::remove_dir_all(&stage);
        return Err(e).context(crate::i18n::tr("Could not copy the current folder; the existing destination is unchanged"));
    }
    if let Err(e) = std::fs::rename(target, &backup) {
        let _ = std::fs::remove_dir_all(&stage);
        return Err(e).context(crate::i18n::tr("Could not back up the existing destination"));
    }
    if let Err(e) = std::fs::rename(&stage, target) {
        let restored = std::fs::rename(&backup, target);
        let _ = std::fs::remove_dir_all(&stage);
        if restored.is_err() { bail!(crate::i18n::trf("Could not replace the destination ({error}); its original data is preserved at {path}", &[("error", &e), ("path", &backup.display())])); }
        return Err(e.into());
    }
    Ok(backup)
}

fn prepare_destination(source: &Path, target: &Path, mode: ChangeMode) -> Result<Option<PathBuf>> {
    match mode {
        ChangeMode::UseExisting => {
            validate_destination(source, target)?;
            if !target.is_dir() { bail!(crate::i18n::tr("The existing .openomsi folder was not found")); }
            Ok(None)
        }
        ChangeMode::Overwrite => prepare_overwrite(source, target).map(Some),
        _ => { prepare(source, target, mode == ChangeMode::Move)?; Ok(None) }
    }
}

/// Whether `folder` is the default one (`~/.openomsi`), where other installations still look.
fn is_default_folder(folder: &Path) -> bool {
    let Ok(default) = omsi_cfg::user_data::default_dir() else { return false };
    default == folder || matches!((default.canonicalize(), folder.canonicalize()), (Ok(a), Ok(b)) if a == b)
}

/// Move to `path`. The copy runs first and takes as long as it takes; `hold` is called just
/// before the pointers are written, so a caller can keep the other work of the launcher out
/// of the switch (and the files written right after it) and nothing else.
pub fn change(path: &Path, mode: ChangeMode, game: &Path, hold: impl FnOnce()) -> Result<Option<String>> {
    if cfg!(any(target_os = "android", target_os = "ios")) {
        bail!(crate::i18n::tr("Changing the .openomsi folder is only available on desktop"));
    }
    if crate::list_instances().iter().any(|i| i.running) {
        bail!(crate::i18n::tr("Close running games before changing the .openomsi folder"));
    }
    if crate::install::jobs().iter().any(|job| job.finished.is_none()) {
        bail!(crate::i18n::tr("Wait for mod installations to finish before changing the .openomsi folder"));
    }
    let source = crate::data_dir();
    let target = selected_path(path);
    if source == target { return Ok(None); }
    let launcher = std::env::current_exe()?;
    let canonical_source = source.canonicalize()?;
    if target.exists() && target.canonicalize()? == canonical_source { return Ok(None); }
    if matches!(mode, ChangeMode::Move | ChangeMode::Overwrite) && (game.canonicalize()?.starts_with(&canonical_source) || launcher.canonicalize()?.starts_with(&canonical_source)) {
        bail!(crate::i18n::tr("Move the game program and launcher outside .openomsi before changing its location"));
    }
    validate_destination(&source, &target)?;
    if mode == ChangeMode::Overwrite {
        let canonical_target = target.canonicalize()?;
        if game.canonicalize()?.starts_with(&canonical_target) || launcher.canonicalize()?.starts_with(&canonical_target) {
            bail!(crate::i18n::tr("The destination contains the game program or launcher and cannot be overwritten"));
        }
    }
    let game_config = omsi_cfg::user_data::config_file_for(game);
    let launcher_config = omsi_cfg::user_data::config_file_for(&launcher);
    let mut pointers = vec![game_config.clone()];
    if launcher_config != game_config { pointers.push(launcher_config); }
    let previous: Vec<_> = pointers.iter().map(|file| {
        match std::fs::read(file) {
            Ok(bytes) => Ok((file.clone(), Some(bytes))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((file.clone(), None)),
            Err(e) => Err(e),
        }
    }).collect::<std::io::Result<_>>()?;
    let backup = prepare_destination(&source, &target, mode)?;
    hold();
    let commit = (|| -> Result<()> {
        omsi_cfg::user_data::write_config_for(game, &target)?;
        if omsi_cfg::user_data::config_file_for(&launcher) != game_config {
            omsi_cfg::user_data::write_config_for(&launcher, &target)?;
        }
        omsi_cfg::user_data::set_active_path(&target)?;
        Ok(())
    })();
    if let Err(e) = commit {
        let restored = previous.into_iter().try_for_each(|(file, bytes)| match bytes {
            Some(bytes) => if std::fs::read(&file).ok().as_ref() == Some(&bytes) { Ok(()) } else { std::fs::write(file, bytes) },
            None => match std::fs::remove_file(file) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            },
        });
        // Keep the prepared copy if rollback failed: a pointer might still name it.
        if restored.is_ok() && mode != ChangeMode::UseExisting {
            if std::fs::remove_dir_all(&target).is_ok() {
                if let Some(backup) = &backup {
                    std::fs::rename(backup, &target).with_context(|| crate::i18n::trf("The original destination is preserved at {path}", &[("path", &backup.display())]))?;
                }
            }
        }
        return Err(e).context(crate::i18n::tr("Could not save .openomsi-config; the original folder is unchanged"));
    }
    let mut warnings = Vec::new();
    if matches!(mode, ChangeMode::Move | ChangeMode::Overwrite) && is_default_folder(&source) {
        // (every installation starts on `~/.openomsi`: the others, the dedicated server, a test
        // build unpacked elsewhere, still read their pointer and would silently begin again
        // with an empty folder, which looks exactly like lost data)
        warnings.push(crate::i18n::trf("The new folder is active. The old folder at {path} was kept, because other copies of openOMSI may still use it.", &[("path", &source.display())]));
    } else if matches!(mode, ChangeMode::Move | ChangeMode::Overwrite) {
        if let Err(e) = std::fs::remove_dir_all(&source) {
            warnings.push(crate::i18n::trf("The new folder is active, but the old copy at {path} could not be removed: {error}", &[("path", &source.display()), ("error", &e)]));
        }
    }
    if let Some(backup) = backup {
        if let Err(e) = std::fs::remove_dir_all(&backup) {
            warnings.push(crate::i18n::trf("The destination was replaced, but its backup at {path} could not be removed: {error}", &[("path", &backup.display()), ("error", &e)]));
        }
    }
    Ok((!warnings.is_empty()).then(|| warnings.join("\n")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn existing_folders() -> (PathBuf, PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("openomsi-existing-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let source = root.join("current/.openomsi");
        let target = root.join("existing/.openomsi");
        std::fs::create_dir_all(source.join("profiles")).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(source.join("profiles/current.json"), b"current").unwrap();
        std::fs::write(target.join("settings.cfg"), b"existing").unwrap();
        (root, source, target)
    }

    #[test]
    fn using_existing_keeps_both_folders_unchanged() {
        let (root, source, target) = existing_folders();
        assert!(prepare_destination(&source, &target, ChangeMode::UseExisting).unwrap().is_none());
        assert_eq!(std::fs::read(target.join("settings.cfg")).unwrap(), b"existing");
        assert_eq!(std::fs::read(source.join("profiles/current.json")).unwrap(), b"current");
        assert!(!target.join("profiles").exists());
        assert!(prepare_destination(&source, &root.join("missing/.openomsi"), ChangeMode::UseExisting).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn overwrite_replaces_instead_of_merging_and_retains_backup() {
        let (root, source, target) = existing_folders();
        let backup = prepare_destination(&source, &target, ChangeMode::Overwrite).unwrap().unwrap();
        assert_eq!(std::fs::read(target.join("profiles/current.json")).unwrap(), b"current");
        assert!(!target.join("settings.cfg").exists());
        assert_eq!(std::fs::read(backup.join("settings.cfg")).unwrap(), b"existing");
        assert!(source.join("profiles/current.json").exists());
        // A failed pointer commit can restore the original destination intact.
        std::fs::remove_dir_all(&target).unwrap();
        std::fs::rename(backup, &target).unwrap();
        assert_eq!(std::fs::read(target.join("settings.cfg")).unwrap(), b"existing");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_overwrite_copy_preserves_existing_destination() {
        let (root, source, target) = existing_folders();
        let missing_source = root.join("missing/.openomsi");
        std::fs::create_dir_all(&missing_source).unwrap();
        std::fs::remove_dir(&missing_source).unwrap();
        assert!(prepare_overwrite(&missing_source, &target).is_err());
        assert_eq!(std::fs::read(target.join("settings.cfg")).unwrap(), b"existing");
        assert!(prepare_overwrite(&source, &source.join("nested/.openomsi")).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    /// A copy that fails halfway (here a symbolic link among the files) leaves the original
    /// folder as it was, takes the half-made destination away again, and changes no pointer.
    #[cfg(unix)]
    #[test]
    fn a_copy_that_fails_halfway_leaves_the_original_intact() {
        let (root, source, _) = existing_folders();
        std::fs::write(source.join("a-first.txt"), b"first").unwrap();
        std::os::unix::fs::symlink("a-first.txt", source.join("zz-link")).unwrap();
        let target = root.join("new/.openomsi");
        assert!(prepare(&source, &target, true).is_err());
        assert!(!target.exists(), "the half-made copy is removed");
        assert_eq!(std::fs::read(source.join("a-first.txt")).unwrap(), b"first");
        assert_eq!(std::fs::read(source.join("profiles/current.json")).unwrap(), b"current");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_target_that_already_holds_an_openomsi_is_refused_when_moving() {
        let (root, source, target) = existing_folders();
        assert!(prepare(&source, &target, true).is_err());
        assert!(prepare(&source, &target, false).is_err());
        assert_eq!(std::fs::read(target.join("settings.cfg")).unwrap(), b"existing");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_target_inside_the_old_folder_is_refused() {
        let (root, source, _) = existing_folders();
        let inside = source.join("sub/.openomsi");
        assert!(prepare(&source, &inside, true).is_err());
        assert!(!inside.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn selection_keeps_required_folder_name() {
        assert_eq!(selected_path(Path::new("parent")), Path::new("parent").join(".openomsi"));
        assert_eq!(selected_path(Path::new("parent/.openomsi")), Path::new("parent/.openomsi"));
    }

    #[test]
    fn prepare_moves_or_creates_without_overwriting() {
        let root = std::env::temp_dir().join(format!("openomsi-migration-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let source = root.join("old/.openomsi");
        let target = root.join("new/.openomsi");
        std::fs::create_dir_all(source.join("profiles")).unwrap();
        std::fs::write(source.join("profiles/driver.json"), b"driver").unwrap();
        prepare(&source, &target, true).unwrap();
        assert_eq!(std::fs::read(target.join("profiles/driver.json")).unwrap(), b"driver");
        assert!(source.join("profiles/driver.json").exists());
        assert!(prepare(&source, &target, true).is_err());
        assert!(prepare(&source, &source.join("nested/.openomsi"), true).is_err());
        let fresh = root.join("fresh/.openomsi");
        prepare(&source, &fresh, false).unwrap();
        assert_eq!(std::fs::read_dir(fresh).unwrap().count(), 0);
        std::fs::remove_dir_all(root).unwrap();
    }
}
