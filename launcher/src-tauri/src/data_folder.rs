//! Native authorization is separate from saving settings or moving user data.

use omsi_launcher_lib::i18n::tr;
use std::path::{Component, Path, PathBuf};

#[cfg(target_os = "macos")]
mod macos;

fn normalize(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err(tr("Choose an absolute data folder path"));
    }
    let mut target = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                target.pop();
            }
            _ => target.push(component.as_os_str()),
        }
    }
    if target.file_name().is_none_or(|name| name != ".openomsi") {
        target.push(".openomsi");
    }
    let text = target
        .to_str()
        .ok_or_else(|| tr("The data folder path must be valid UTF-8"))?;
    if text.contains(['\0', '\n', '\r']) {
        return Err(tr("The data folder path contains an invalid character"));
    }
    Ok(target)
}

pub fn pick(path: Option<String>) -> Result<Option<String>, String> {
    let requested = path
        .as_deref()
        .map(|path| normalize(Path::new(path)))
        .transpose()?;
    #[cfg(target_os = "macos")]
    let target = macos::pick(requested)?;
    #[cfg(not(target_os = "macos"))]
    let target = match requested {
        Some(target) => Some(target),
        None => omsi_launcher_lib::pick_folder(&tr("Choose .openomsi or its parent folder"))
            .map(|path| normalize(&path))
            .transpose()?,
    };
    target
        .map(|path| {
            path.into_os_string()
                .into_string()
                .map_err(|_| tr("The data folder path must be valid UTF-8"))
        })
        .transpose()
}

pub fn restore_access() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    macos::restore()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_parent_and_target_identically() {
        let parent = std::env::temp_dir().join("openomsi-data-picker");
        let target = parent.join(".openomsi");
        assert_eq!(normalize(&parent).unwrap(), target);
        assert_eq!(normalize(&target).unwrap(), target);
        assert_eq!(
            normalize(&parent.join("child/../.openomsi/.")).unwrap(),
            target
        );
    }

    #[test]
    fn rejects_relative_and_invalid_paths() {
        assert!(normalize(Path::new("")).is_err());
        assert!(normalize(Path::new("relative/.openomsi")).is_err());
        assert!(normalize(&std::env::temp_dir().join("invalid\npath")).is_err());
    }
}
